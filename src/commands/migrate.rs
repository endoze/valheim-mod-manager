use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::target::{GAME, Target};
use thunderstore_engine::client::ThunderstoreClient;
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile::portability;

/// Whether this config still needs migrating: the deprecated `mod_list` names
/// at least one mod that `mods.yml` does not yet record.
///
/// A thin wrapper over [`portability::needs_adoption`], which is keyed on
/// missing *entries* rather than a missing `mods.yml` (see its own doc for
/// why). That function cannot actually fail today, an unreadable record reads
/// as empty rather than erroring, but its signature still returns a `Result`.
/// An unexpected `Err` here defaults to `true` rather than `false`: that is
/// the side that keeps [`hint_if_unmigrated`] warning and keeps [`run`]
/// willing to try, instead of silently reporting nothing to do when there
/// might be something.
pub fn needs_migration(target: &Target, config: &AppConfig) -> bool {
  if config.mod_list.is_empty() {
    return false;
  }

  portability::needs_adoption(&target.dir, &config.mod_list).unwrap_or(true)
}

/// The `mod_list` entries this target's `mods.yml` does not record.
///
/// The same set [`needs_migration`] reduces to a bool, kept here so the hint can
/// name them. An unreadable record reads as empty, which is what
/// [`portability::needs_adoption`] does too, so the two cannot disagree about
/// whether there is anything to say.
fn unadopted_names(target: &Target, config: &AppConfig) -> Vec<String> {
  let installed = thunderstore_engine::profile::modlist::read(&target.dir).unwrap_or_default();

  config
    .mod_list
    .iter()
    .filter(|name| thunderstore_engine::profile::modlist::find(&installed, name).is_none())
    .cloned()
    .collect()
}

/// The warning [`hint_if_unmigrated`] prints, pulled out so its wording is
/// testable without capturing stderr.
///
/// Two different situations produce the same state on disk, and the hint has to
/// serve both because nothing can tell them apart. `mod_list` naming something
/// `mods.yml` does not record means either that it was never adopted, or that it
/// *was* adopted and the mod has since been deliberately uninstalled: deleting
/// the key after migrating is advised but never enforced, so a leftover
/// `mod_list` outlives the migration. Only the first reading wants
/// `vmm migrate`; for the second, migrating would reinstall the mod that was
/// just removed, so the entry has to come out of `mod_list` instead.
fn unmigrated_hint(target: &Target, config: &AppConfig) -> String {
  let unadopted = unadopted_names(target, config);

  crate::error::advice_message(
    "your config's `mod_list` names mods that aren't recorded as installed.",
    &format!(
      "{} does not record:\n\n  {}\n\n`mod_list` is deprecated and only \
       mods.yml is consulted, so commands that read this target may not see \
       them. If they were never adopted, migrating adopts them and you can \
       then delete `mod_list`. If you uninstalled one of them on purpose, \
       delete just that entry from `mod_list` instead, or migrating will \
       install it again.",
      target.mods_yml().display(),
      unadopted.join("\n  ")
    ),
    &["vmm migrate"],
  )
}

/// Warns when this config's `mod_list` is not fully reflected in `mods.yml`.
/// Written to stderr unconditionally rather than through the log filter, so it
/// is seen on the default `log_level = "error"`.
pub fn hint_if_unmigrated(target: &Target, config: &AppConfig) {
  if !needs_migration(target, config) {
    return;
  }

  eprintln!("vmm: {}", unmigrated_hint(target, config));
}

/// Adopts the deprecated `mod_list` as the target's `mods.yml`.
///
/// Installing rewrites the files the old folder-based installer placed, so this
/// is safe to run over an existing install, including retrying after a prior
/// partial failure: an already-recorded entry is simply reinstalled, which is
/// cheap since its archive is already downloaded and already extracted in the
/// shared cache. The adoption itself, and the rule that its delisted sweep runs
/// only once every `mod_list` entry is adopted, are [`portability::adopt_names_in`]'s;
/// this only reports what it did and turns a leftover `remaining` list into a
/// resumable error.
pub async fn run(
  client: &ThunderstoreClient,
  eco: &Ecosystem,
  target: &Target,
  config: &AppConfig,
) -> AppResult<()> {
  if !needs_migration(target, config) {
    if config.mod_list.is_empty() {
      // Nothing to migrate is not a failure: the config is already on the
      // current format, so say so and succeed rather than making a script
      // treat a no-op as an error.
      println!(
        "vmm: {}",
        crate::error::advice_message(
          "there's nothing to migrate.",
          "Your config has no `mod_list` key, so you're already on the current \
           format. Nothing was changed.",
          &["vmm install <Owner-ModName>"],
        )
      );

      return Ok(());
    }

    return Err(AppError::advice(
      "your `mod_list` has already been migrated.",
      format!(
        "Every mod it names is already recorded in {}, so there is nothing left \
         to adopt and nothing was changed.\n\nYou can delete `mod_list` from \
         your config.",
        target.mods_yml().display()
      ),
      &["vmm list"],
    ));
  }

  // Read the record first, not for its own sake: the planner inside
  // `adopt_names_in` reads the same file immediately after, but reading it
  // here first means an unreadable mods.yml speaks in vmm's advice voice
  // instead of the engine's raw error surfacing from inside the planner.
  super::read_modlist(target)?;

  let index = client.get_manifest().await?;

  println!("adopting {} mods from mod_list", config.mod_list.len());

  let outcome = portability::adopt_names_in(
    &target.dir,
    &target.base,
    eco,
    &index,
    client,
    GAME,
    &config.mod_list,
    thunderstore_engine::profile::modlist::now_millis(),
  )
  .await?;

  super::report_installed(target, &outcome.adopted)?;

  for dir in &outcome.swept {
    println!("swept stale folder {}", dir.display());
  }

  if !outcome.remaining.is_empty() {
    return Err(unadopted_error(&outcome.remaining, &outcome.failed));
  }

  println!(
    "\nwrote {}\n\nYou can now delete `mod_list` from your config.",
    target.mods_yml().display()
  );

  Ok(())
}

/// Builds the error `run` returns when a migration leaves some `mod_list`
/// entries unadopted, so the caller knows exactly what is still stranded on
/// disk from the old installer and that a reconciling command (e.g.
/// `vmm uninstall`) would sweep it as delisted before a follow-up
/// `vmm migrate` finishes the job.
///
/// Takes no `target`: `remaining` and `failed` already come from the
/// adoption outcome rather than needing to be recomputed from the target's
/// record.
fn unadopted_error(
  remaining: &[String],
  failed: &[(String, thunderstore_engine::error::Error)],
) -> AppError {
  let mut detail = format!(
    "These mods were not adopted: {}\n\nTheir files are still on disk from \
     the old installer, but mods.yml does not record them, so a command \
     that reconciles installs would sweep them as delisted. Finish the \
     migration before running one.",
    remaining.join(", ")
  );

  if !failed.is_empty() {
    let reasons = failed
      .iter()
      .map(|(name, error)| format!("{name}: {error}"))
      .collect::<Vec<_>>()
      .join("; ");

    detail.push_str(&format!("\n\nKnown reasons: {reasons}"));
  }

  AppError::advice(
    "migration stopped part way through.",
    detail,
    &["vmm migrate"],
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::AppConfig;
  use crate::test_support::Fixture;
  use thunderstore_engine::profile::modlist;
  use tokio::runtime::Runtime;

  /// A config whose deprecated `mod_list` names `mods`.
  fn config_with(mods: &[&str]) -> AppConfig {
    AppConfig {
      mod_list: mods.iter().map(|name| name.to_string()).collect(),
      ..Default::default()
    }
  }

  #[test]
  fn migrate_adopts_mod_list_and_sweeps_stale_folders() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // A folder the old installer left behind for a mod no longer in mod_list.
    let stale = target.dir.join("BepInEx/plugins/Owner-Dropped");

    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("Dropped.dll"), b"dll").unwrap();

    Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &config_with(&["Owner-ModA"]),
      ))
      .unwrap();

    let mods = modlist::read(&target.dir).unwrap();

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name, "Owner-ModA");
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
    assert!(!stale.exists(), "a dropped mod's folder should be swept");
  }

  #[test]
  fn migrate_adopts_into_the_profile_not_the_game_dir() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("experiment");
    let eco = Ecosystem::bundled();

    // A stale folder inside the profile, and one in the game directory that a
    // profile-mode migration must not reach.
    let stale = target.dir.join("BepInEx/plugins/Owner-Dropped");
    let in_game_dir = fixture
      .game_dir
      .path()
      .join("BepInEx/plugins/Owner-Untouched");

    std::fs::create_dir_all(&stale).unwrap();
    std::fs::create_dir_all(&in_game_dir).unwrap();
    std::fs::write(in_game_dir.join("Untouched.dll"), b"dll").unwrap();

    Runtime::new()
      .unwrap()
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &config_with(&["Owner-ModA"]),
      ))
      .unwrap();

    assert!(target.mods_yml().exists());
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
    assert!(!stale.exists(), "the profile's stale folder is swept");
    // Nothing was installed into, or swept from, the game directory.
    assert!(
      !fixture
        .game_dir
        .path()
        .join("BepInEx/plugins/Owner-ModA")
        .exists()
    );
    assert!(!fixture.game_dir.path().join("mods.yml").exists());
    assert!(in_game_dir.join("Untouched.dll").exists());
  }

  #[test]
  fn migrate_refuses_when_every_mod_list_entry_is_already_recorded() {
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

    // Every mod_list entry names a mod already recorded, so there is nothing
    // left to adopt, so refuse rather than reinstalling for no reason.
    let result = runtime.block_on(run(
      &fixture.client,
      &eco,
      &target,
      &config_with(&["Owner-ModA"]),
    ));

    assert!(result.is_err());
    // The existing record is untouched.
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
  }

  #[test]
  fn migrate_with_no_mod_list_is_a_friendly_no_op() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // Nothing to migrate is not a failure: the config is simply already on the
    // current format, so this succeeds and explains itself.
    let result =
      Runtime::new()
        .unwrap()
        .block_on(run(&fixture.client, &eco, &target, &config_with(&[])));

    assert!(result.is_ok());
    // Succeeding must not have written a record for mods that do not exist.
    assert!(!target.mods_yml().exists());
  }

  #[test]
  fn an_unreadable_mods_yml_reports_its_path_before_migrating() {
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
        &config_with(&["Owner-ModA"]),
      ))
      .unwrap_err()
      .to_string();

    // vmm's own advice voice, the same shape `list`/`update`/`uninstall` use,
    // not the engine's raw parse error leaking through the planner.
    assert!(message.contains("the install record for"), "got: {message}");
    assert!(message.contains(&target.mods_yml().display().to_string()));
  }

  #[test]
  fn migrate_stops_and_leaves_a_resumable_state_on_an_unresolvable_entry() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // Owner-Delisted is absent from the fixture's index, standing in for a mod
    // delisted from Thunderstore since the config was written, the realistic
    // way a stale `mod_list` fails partway through. The engine's own rules for
    // this case, keying adoption on missing entries and never sweeping until
    // every name adopts, are `adopt_names_in`'s and are covered by its own
    // test; this only pins what vmm adds on top: the error `run` surfaces.
    let result = Runtime::new().unwrap().block_on(run(
      &fixture.client,
      &eco,
      &target,
      &config_with(&["Owner-ModA", "Owner-Delisted", "Owner-ModB"]),
    ));

    let error = result.unwrap_err();

    // `run` reports its own resumable-migration advice rather than the raw
    // engine error the old sequential loop used to propagate.
    assert!(matches!(error, AppError::Other(_)));

    let message = error.to_string();

    assert!(message.contains("migration stopped part way through."));
    assert!(message.contains("Owner-Delisted"));
    assert!(message.contains("vmm migrate"));
    // The reason Owner-Delisted failed (an unresolved dependency, standing in
    // for a delisted mod) is now surfaced too, not just its name.
    assert!(message.contains("Known reasons:"));
    assert!(message.contains("unresolved dependencies"));
  }

  #[test]
  fn migrate_is_resumable_after_a_partial_failure() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(run(
        &fixture.client,
        &eco,
        &target,
        &config_with(&["Owner-ModA", "Owner-Delisted"]),
      ))
      .unwrap_err();

    // A prior partial migration still reads as needing migration, so a retry
    // is not silently refused.
    assert!(needs_migration(
      &target,
      &config_with(&["Owner-ModA", "Owner-Delisted"])
    ));

    // Fix the config (drop the now-delisted entry) and retry.
    let result = runtime.block_on(run(
      &fixture.client,
      &eco,
      &target,
      &config_with(&["Owner-ModA", "Owner-ModB"]),
    ));

    assert!(result.is_ok());

    let mods = modlist::read(&target.dir).unwrap();

    assert_eq!(mods.len(), 2);
    assert!(modlist::find(&mods, "Owner-ModA").is_some());
    assert!(modlist::find(&mods, "Owner-ModB").is_some());
  }

  #[test]
  fn the_hint_names_what_is_unadopted_and_warns_that_migrating_undoes_a_removal() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();
    let config = config_with(&["Owner-ModA", "Owner-ModB"]);

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string(), "Owner-ModB".to_string()],
      ))
      .unwrap();

    // A fully migrated config, then a deliberate uninstall. `needs_adoption` is
    // keyed on missing entries, and deleting `mod_list` is only ever advised,
    // never enforced, so this reads exactly like a half-finished migration and
    // the hint fires on every command from here on. It must therefore say the
    // other thing that can be true, because `vmm migrate` would reinstall the
    // mod that was just deliberately removed.
    crate::commands::uninstall::run(&eco, &target, &["Owner-ModB".to_string()], false).unwrap();

    assert!(needs_migration(&target, &config));

    let hint = unmigrated_hint(&target, &config);

    assert!(
      hint.contains("Owner-ModB"),
      "the hint must name what is unadopted; got: {hint}"
    );
    assert!(
      !hint.contains("Owner-ModA"),
      "an already-recorded mod is not unadopted; got: {hint}"
    );
    assert!(
      hint.contains("mod_list"),
      "the hint must name the deprecated key to edit; got: {hint}"
    );
    assert!(
      hint.contains("uninstalled"),
      "the hint must cover the deliberate-removal reading; got: {hint}"
    );
  }

  #[test]
  fn hint_fires_only_for_an_unmigrated_config() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // mod_list present, no mods.yml => needs migrating.
    assert!(needs_migration(&target, &config_with(&["Owner-ModA"])));
    // No mod_list => nothing to migrate.
    assert!(!needs_migration(&target, &config_with(&[])));

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    // Every mod_list entry is recorded => already migrated.
    assert!(!needs_migration(&target, &config_with(&["Owner-ModA"])));
    // mods.yml exists but is missing an entry mod_list still names => a
    // partial migration, so migrating is still needed.
    assert!(needs_migration(
      &target,
      &config_with(&["Owner-ModA", "Owner-ModB"])
    ));
  }

  #[test]
  fn the_hint_stays_quiet_when_there_is_nothing_left_to_adopt() {
    let fixture = Fixture::new();
    let target = fixture.target();

    // Almost every command calls this before doing its own work, so the silent
    // branch carries as much weight as the warning one: a config with nothing
    // outstanding must not nag on every single invocation. Both are driven here
    // because the gate is the only thing between the two.
    hint_if_unmigrated(&target, &config_with(&[]));
    hint_if_unmigrated(&target, &config_with(&["Owner-ModA"]));
  }
}
