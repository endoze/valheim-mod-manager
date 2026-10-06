use crate::error::AppResult;
use crate::target::{GAME, Target};
use thunderstore_engine::client::ThunderstoreClient;
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile::{self, modlist};

/// Installs each mod and its full dependency closure into the target.
///
/// The engine resolves, downloads, extracts into the shared package cache under
/// `target.base`, installs into `target.dir`, and upserts `mods.yml` plus any
/// `_state` tracker. Nothing here duplicates that pipeline.
///
/// A mod whose recorded version is the one that would be installed, with its
/// files still in place, is skipped and reported as already installed. `force`
/// reinstalls the named mods regardless, removing their existing files first; it
/// never reaches their dependencies, which are still skipped when current.
///
/// `mods` is passed as both the desired set and the explicit set, so a mod
/// named on the command line is left enabled: asking for it by name is a
/// request to have it active. One that was disabled is reported as enabled.
/// Only an incidentally reinstalled dependency keeps its disabled state, which
/// `install_batch` restores after the whole batch, even when the batch failed
/// partway.
pub async fn run(
  client: &ThunderstoreClient,
  eco: &Ecosystem,
  target: &Target,
  mods: &[String],
  force: bool,
) -> AppResult<()> {
  // Read the record first: the planner below reads the same file immediately
  // after, but reading it here first means an unreadable mods.yml speaks in
  // vmm's advice voice instead of the engine's raw error surfacing from inside
  // the planner. It is also the before side of the enabled-state report.
  let before = super::read_modlist(target)?;

  // `InstallBatch` is non-exhaustive, so the planned batch is adjusted in place
  // rather than rebuilt with struct-update syntax.
  let mut batch = profile::plan_install_batch(&target.dir, mods, mods)?;

  batch.force = force;

  let index = client.get_manifest().await?;

  let outcome = profile::install_batch(
    &target.dir,
    &target.base,
    eco,
    &index,
    client,
    GAME,
    &batch,
    thunderstore_engine::profile::modlist::now_millis(),
  )
  .await?;

  // Read once for every line below. The install has already happened, so an
  // unreadable record must not abort the report before `report_batch_failures`
  // gets to say what failed; each name is then printed without its version.
  let recorded = modlist::read(&target.dir).unwrap_or_default();

  super::report_recorded(&recorded, "installed", &outcome.succeeded);
  super::report_unchanged(&recorded, &outcome.unchanged);

  // A named mod that was disabled is enabled by the install, even when it was
  // skipped as current, which would otherwise read as nothing having changed.
  super::report_state_changes(&before, &recorded, &batch.enable_requested);

  for full_name in &batch.protect_disabled {
    if outcome.succeeded.contains(full_name) {
      println!("kept {full_name} disabled");
    }
  }

  super::report_batch_failures(&outcome)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use thunderstore_engine::ecosystem::Ecosystem;
  use thunderstore_engine::profile::modlist;
  use tokio::runtime::Runtime;

  #[test]
  fn install_places_files_and_records_mods_yml() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );

    let mods = modlist::read(&target.dir).unwrap();

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name, "Owner-ModA");
    assert!(mods[0].enabled);
  }

  #[test]
  fn install_extracts_into_the_base_cache_not_the_target() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    // The package cache is shared across targets and must never land inside the
    // game directory.
    let cached = thunderstore_engine::profile::cache::package_cache_dir(
      &target.base,
      crate::target::GAME,
      "Owner-ModA",
      "1.0.0",
    );

    assert!(cached.join("plugins/ModA.dll").exists());
  }

  #[test]
  fn install_pulls_in_the_dependency_closure() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModC".to_string()],
        false,
      ))
      .unwrap();

    // Owner-ModC depends on Owner-ModA-1.0.0, so both install.
    let mods = modlist::read(&target.dir).unwrap();

    assert_eq!(mods.len(), 2);
    assert!(modlist::find(&mods, "Owner-ModA").is_some());
    assert!(modlist::find(&mods, "Owner-ModC").is_some());
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModC/ModC.dll")
        .exists()
    );
  }

  #[test]
  fn installing_a_disabled_mod_by_name_re_enables_it() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    thunderstore_engine::profile::set_enabled_in(
      &target.dir,
      &eco,
      crate::target::GAME,
      "Owner-ModA",
      false,
    )
    .unwrap();

    // A reinstall clears the mod's folder first, so this surviving shows the
    // mod was skipped rather than reinstalled.
    let stray = target.dir.join("BepInEx/plugins/Owner-ModA/stray.json");

    std::fs::write(&stray, b"left behind").unwrap();

    // Naming a mod explicitly is a request to have it active. It is already at
    // the recorded version, so it is skipped rather than reinstalled, and is
    // enabled anyway: `run` reports that so the change is not silent.
    runtime
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    let mods = modlist::read(&target.dir).unwrap();

    assert!(stray.exists(), "a current mod should be skipped");
    assert!(modlist::find(&mods, "Owner-ModA").unwrap().enabled);
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn a_disabled_mod_reinstalled_as_a_dependency_stays_disabled() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    thunderstore_engine::profile::set_enabled_in(
      &target.dir,
      &eco,
      crate::target::GAME,
      "Owner-ModA",
      false,
    )
    .unwrap();

    // The other half of `installing_a_disabled_mod_by_name_re_enables_it`.
    // Owner-ModC depends on Owner-ModA, so this reinstalls ModA without anyone
    // naming it, and nothing about installing ModC is a request to turn ModA
    // back on. The batch protects that state, and `run` reports each mod it kept
    // disabled so a silently inert dependency is not left to be discovered in
    // game.
    runtime
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModC".to_string()],
        false,
      ))
      .unwrap();

    let mods = modlist::read(&target.dir).unwrap();

    assert!(!modlist::find(&mods, "Owner-ModA").unwrap().enabled);
    assert!(modlist::find(&mods, "Owner-ModC").unwrap().enabled);
  }

  #[test]
  fn install_into_a_profile_never_writes_to_the_game_dir() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("experiment");
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
    // The point of profile mode: the game directory is untouched.
    assert!(
      !fixture
        .game_dir
        .path()
        .join("BepInEx/plugins/Owner-ModA")
        .exists()
    );
    assert!(!fixture.game_dir.path().join("mods.yml").exists());
  }

  #[test]
  fn the_fixture_can_install_a_recognised_mod_loader() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["denikson-BepInExPack_Valheim".to_string()],
        false,
      ))
      .unwrap();

    // The ecosystem must recognise it as a loader, or every loader-specific
    // code path stays untested.
    assert!(
      eco
        .modloader_package("denikson-BepInExPack_Valheim")
        .is_some()
    );
    // A loader pack unpacks to the target root with its `rootFolder` stripped,
    // and is recorded as `State` so it has an install record.
    assert!(target.dir.join("winhttp.dll").exists());
    assert!(target.dir.join("BepInEx/core/BepInEx.dll").exists());
    assert!(
      thunderstore_engine::install::state_file_path(&target.dir, "denikson-BepInExPack_Valheim")
        .exists()
    );
  }

  #[test]
  fn an_unreadable_mods_yml_reports_its_path_before_installing() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    std::fs::write(target.mods_yml(), "not: [valid").unwrap();

    let message = Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap_err()
      .to_string();

    // vmm's own advice voice, the same shape `list`/`update`/`uninstall` use,
    // not the engine's raw parse error leaking through the planner.
    assert!(message.contains("the install record for"), "got: {message}");
    assert!(message.contains(&target.mods_yml().display().to_string()));
  }

  #[test]
  fn install_reports_an_unknown_mod() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    let result = Runtime::new().unwrap().block_on(run(
      &fixture.client,
      &eco,
      &target,
      &["Owner-Nonexistent".to_string()],
      false,
    ));

    assert!(result.is_err());
  }

  #[test]
  fn reinstalling_a_recorded_version_without_force_leaves_it_unchanged() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();
    let mods = ["Owner-ModA".to_string()];

    runtime
      .block_on(run(&fixture.client, &eco, &target, &mods, false))
      .unwrap();

    let folder = target.dir.join("BepInEx/plugins/Owner-ModA");
    let plugin = folder.join("ModA.dll");
    let stray = folder.join("stray.json");

    std::fs::write(&plugin, b"edited").unwrap();
    std::fs::write(&stray, b"left behind").unwrap();

    runtime
      .block_on(run(&fixture.client, &eco, &target, &mods, false))
      .unwrap();

    // Already at the recorded version, so nothing is reinstalled and nothing on
    // disk is touched, including files the install never placed.
    assert_eq!(std::fs::read(&plugin).unwrap(), b"edited");
    assert!(stray.exists());
  }

  #[test]
  fn force_reinstalls_a_recorded_version_from_a_clean_folder() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();
    let mods = ["Owner-ModA".to_string()];

    runtime
      .block_on(run(&fixture.client, &eco, &target, &mods, false))
      .unwrap();

    let folder = target.dir.join("BepInEx/plugins/Owner-ModA");
    let plugin = folder.join("ModA.dll");
    // The shape issue #23 left behind: a file that belongs in a subfolder,
    // installed flat into the mod's namespaced folder.
    let stray = folder.join("german.json");

    std::fs::write(&plugin, b"edited").unwrap();
    std::fs::write(&stray, b"flattened").unwrap();

    runtime
      .block_on(run(&fixture.client, &eco, &target, &mods, true))
      .unwrap();

    assert!(!stray.exists(), "--force should clear the old folder first");
    assert_eq!(std::fs::read(&plugin).unwrap(), b"dll-bytes");
    assert!(
      modlist::find(&modlist::read(&target.dir).unwrap(), "Owner-ModA")
        .unwrap()
        .enabled
    );
  }

  #[test]
  fn force_does_not_reinstall_the_named_mods_dependencies() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();
    let mods = ["Owner-ModC".to_string()];

    runtime
      .block_on(run(&fixture.client, &eco, &target, &mods, false))
      .unwrap();

    let plugins = target.dir.join("BepInEx/plugins");
    let named_stray = plugins.join("Owner-ModC/stray.json");
    let dependency_stray = plugins.join("Owner-ModA/stray.json");

    std::fs::write(&named_stray, b"left behind").unwrap();
    std::fs::write(&dependency_stray, b"left behind").unwrap();

    runtime
      .block_on(run(&fixture.client, &eco, &target, &mods, true))
      .unwrap();

    // Owner-ModC is named, so it is reinstalled from a clean folder. Owner-ModA
    // only comes along as its dependency, and is current, so it is left alone.
    assert!(!named_stray.exists());
    assert!(dependency_stray.exists());
  }
}
