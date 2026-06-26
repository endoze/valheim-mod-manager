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
/// latest version, so a reinstall *is* the update. Already-downloaded archives
/// and already-extracted packages are skipped underneath, making a no-change run
/// cheap.
///
/// Nothing is explicitly named here: `update mods` reinstalls the whole
/// recorded set on the user's behalf rather than at their request, so every
/// disabled mod is restored, which `install_batch` does after the whole batch,
/// even when the batch failed partway.
pub async fn run_mods(
  client: &ThunderstoreClient,
  eco: &Ecosystem,
  target: &Target,
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
  // so every disabled mod is restored.
  let batch = profile::plan_install_batch(&target.dir, &desired, &[])?;
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

  for entry in &super::read_modlist(target)? {
    let state = if entry.enabled { "" } else { " (disabled)" };

    println!("{} {}{}", entry.name, entry.version_number, state);
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
      .block_on(run_mods(&fixture.client, &eco, &target));

    assert!(result.is_ok());
    assert!(modlist::read(&target.dir).unwrap().is_empty());
  }

  #[test]
  fn update_mods_reinstalls_every_recorded_mod() {
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
      ))
      .unwrap();

    let plugin = target.dir.join("BepInEx/plugins/Owner-ModA/ModA.dll");

    std::fs::remove_file(&plugin).unwrap();

    runtime
      .block_on(run_mods(&fixture.client, &eco, &target))
      .unwrap();

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
      ))
      .unwrap();

    let plugin = target.dir.join("BepInEx/plugins/Owner-ModA/ModA.dll");

    std::fs::remove_file(&plugin).unwrap();

    runtime
      .block_on(run_mods(&fixture.client, &eco, &target))
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
}
