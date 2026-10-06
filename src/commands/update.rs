use crate::error::AppResult;
use crate::target::{GAME, Target};
use thunderstore_engine::client::ThunderstoreClient;
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile;

/// Refreshes the cached package index.
pub async fn run_manifest(client: &ThunderstoreClient) -> AppResult<()> {
  tracing::info!("Checking for manifest updates");

  let _ = client.get_manifest().await?;

  Ok(())
}

/// Reinstalls every mod recorded in the target's `mods.yml`.
///
/// The engine's resolution is version-agnostic and always installs a package's
/// latest version, so a reinstall *is* the update. A mod already recorded at
/// that version with its files in place is skipped and counted as up to date,
/// making a no-change run cheap. A mod whose files went missing is reinstalled
/// at its latest version, which repairs it. Only whether files exist is
/// checked, not what they hold, so `force` reinstalls every recorded mod
/// regardless, removing its existing files first, to undo edits or corruption.
///
/// Nothing is explicitly named here: `update mods` reinstalls the whole
/// recorded set on the user's behalf rather than at their request, so every
/// disabled mod is restored, which `install_batch` does after the whole batch,
/// even when the batch failed partway.
pub async fn run_mods(
  client: &ThunderstoreClient,
  eco: &Ecosystem,
  target: &Target,
  force: bool,
) -> AppResult<()> {
  let installed = super::read_modlist(target)?;

  if installed.is_empty() {
    println!(
      "vmm: {}",
      crate::error::advice_message(
        &format!(
          "there is nothing installed in {}.",
          crate::target::describe(target)
        ),
        "Nothing was changed.",
        &["vmm install <Owner-ModName>"],
      )
    );

    return Ok(());
  }

  let desired: Vec<String> = installed.iter().map(|entry| entry.name.clone()).collect();

  tracing::info!("Updating {} installed mods", desired.len());

  // Nothing is explicitly named: `update mods` reinstalls on the user's behalf,
  // so every disabled mod is restored. Without `force`, a mod already at its
  // latest version is skipped rather than reinstalled. `InstallBatch` is
  // non-exhaustive, so the planned batch is adjusted in place.
  let mut batch = profile::plan_install_batch(&target.dir, &desired, &[])?;

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

  // The install has already happened, so an unreadable record must not abort
  // the report before `report_batch_failures` gets to say what failed.
  let recorded = thunderstore_engine::profile::modlist::read(&target.dir).unwrap_or_default();

  for entry in &recorded {
    let state = if entry.enabled { "" } else { " (disabled)" };

    println!("{} {}{}", entry.name, entry.version_number, state);
  }

  println!(
    "\n{}.",
    update_summary(outcome.succeeded.len(), outcome.unchanged.len())
  );

  super::report_batch_failures(&outcome)
}

/// Builds the summary line [`run_mods`] prints under the recorded list, so a run
/// that changed nothing says so instead of looking identical to one that
/// updated everything. Agrees in number the same way `toggle_summary` does.
fn update_summary(installed: usize, unchanged: usize) -> String {
  let mut summary = format!("installed {}", super::describe_count(installed));

  if unchanged > 0 {
    let auxiliary = if unchanged == 1 { "was" } else { "were" };

    summary.push_str(&format!(
      ", {} {auxiliary} already up to date",
      super::describe_count(unchanged)
    ));
  }

  summary
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use thunderstore_engine::ecosystem::Ecosystem;
  use thunderstore_engine::profile::modlist;
  use tokio::runtime::Runtime;

  #[test]
  fn run_manifest_succeeds() {
    let fixture = Fixture::new();

    let result = Runtime::new()
      .unwrap()
      .block_on(run_manifest(&fixture.client));

    assert!(result.is_ok());
  }

  #[test]
  fn update_mods_is_a_no_op_with_nothing_installed() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    let result = Runtime::new()
      .unwrap()
      .block_on(run_mods(&fixture.client, &eco, &target, false));

    assert!(result.is_ok());
    assert!(modlist::read(&target.dir).unwrap().is_empty());
  }

  #[test]
  fn update_mods_reinstalls_a_mod_whose_files_went_missing() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    let plugin = target.dir.join("BepInEx/plugins/Owner-ModA/ModA.dll");

    std::fs::remove_file(&plugin).unwrap();

    runtime
      .block_on(run_mods(&fixture.client, &eco, &target, false))
      .unwrap();

    // Same version, but its files went missing, so it is reinstalled rather than
    // skipped as up to date.
    assert!(plugin.exists(), "the update should restore the mod's files");
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
  }

  #[test]
  fn update_mods_reinstalls_into_the_profile_not_the_game_dir() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("experiment");
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
        false,
      ))
      .unwrap();

    let plugin = target.dir.join("BepInEx/plugins/Owner-ModA/ModA.dll");

    std::fs::remove_file(&plugin).unwrap();

    runtime
      .block_on(run_mods(&fixture.client, &eco, &target, false))
      .unwrap();

    assert!(plugin.exists(), "the update should restore the mod's files");
    // The restore landed in the profile; the game directory saw nothing.
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
  fn update_mods_leaves_a_current_mod_as_it_is() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModC".to_string()],
        false,
      ))
      .unwrap();

    let plugins = target.dir.join("BepInEx/plugins");
    let plugin = plugins.join("Owner-ModA/ModA.dll");
    let stray = plugins.join("Owner-ModC/stray.json");

    std::fs::write(&plugin, b"edited").unwrap();
    std::fs::write(&stray, b"left behind").unwrap();

    runtime
      .block_on(run_mods(&fixture.client, &eco, &target, false))
      .unwrap();

    // Both mods are already at their latest version, so an update without
    // `force` reinstalls neither: the files are left as they were.
    assert_eq!(std::fs::read(&plugin).unwrap(), b"edited");
    assert!(stray.exists());
  }

  #[test]
  fn force_reinstalls_every_recorded_mod_from_a_clean_folder() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModC".to_string()],
        false,
      ))
      .unwrap();

    let plugins = target.dir.join("BepInEx/plugins");
    let plugin = plugins.join("Owner-ModA/ModA.dll");
    let stray = plugins.join("Owner-ModC/stray.json");

    std::fs::write(&plugin, b"edited").unwrap();
    std::fs::write(&stray, b"left behind").unwrap();

    runtime
      .block_on(run_mods(&fixture.client, &eco, &target, true))
      .unwrap();

    // Every recorded mod is named in the batch, so `force` reaches each one,
    // the dependency included, unlike `install --force`.
    assert_eq!(std::fs::read(&plugin).unwrap(), b"dll-bytes");
    assert!(!stray.exists(), "--force should clear the old folder first");
  }

  #[test]
  fn force_keeps_a_disabled_mod_disabled() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
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

    let stray = target.dir.join("BepInEx/plugins/Owner-ModA/stray.json");

    std::fs::write(&stray, b"left behind").unwrap();

    runtime
      .block_on(run_mods(&fixture.client, &eco, &target, true))
      .unwrap();

    // Reinstalled, which writes `enabled: true`, but nothing named the mod, so
    // the batch restores the state the user chose.
    assert!(!stray.exists(), "--force should still reinstall it");
    assert!(
      !modlist::find(&modlist::read(&target.dir).unwrap(), "Owner-ModA")
        .unwrap()
        .enabled
    );
  }

  #[test]
  fn the_update_summary_names_both_counts() {
    assert_eq!(
      update_summary(1, 2),
      "installed 1 mod, 2 mods were already up to date"
    );
    assert_eq!(
      update_summary(2, 1),
      "installed 2 mods, 1 mod was already up to date"
    );
  }

  #[test]
  fn the_update_summary_omits_an_empty_up_to_date_count() {
    assert_eq!(update_summary(3, 0), "installed 3 mods");
  }

  #[test]
  fn the_update_summary_still_counts_installs_when_there_were_none() {
    // A run that changed nothing still leads with the installed count, the same
    // way `toggle_summary` words a zero count.
    assert_eq!(
      update_summary(0, 1),
      "installed 0 mods, 1 mod was already up to date"
    );
  }
}
