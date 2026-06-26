use crate::error::{AppError, AppResult};
use crate::target::{GAME, Target};
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile;

/// Enables or disables an installed mod.
///
/// The engine applies the on-disk mechanics for the mod's tracking method,
/// `.old` renames for subdir-routed files, `_state`-recorded deletion and
/// restore for state-tracked ones, then flips `enabled` in `mods.yml`. A mod
/// loader pack is a no-op, matching r2modman, so this reports that truthfully
/// instead of claiming a change that did not happen. Errors when the mod is
/// not installed.
pub fn run(eco: &Ecosystem, target: &Target, full_name: &str, enabled: bool) -> AppResult<()> {
  // Read the record first, not for its own sake: `set_enabled_in` reads the
  // same file immediately after, but reading it here first means an unreadable
  // mods.yml speaks in vmm's advice voice instead of the engine's raw error
  // surfacing from inside the engine.
  super::read_modlist(target)?;

  profile::set_enabled_in(&target.dir, eco, GAME, full_name, enabled)?;

  if eco.modloader_package(full_name).is_some() {
    println!("vmm: {}", loader_not_toggled_message(full_name));

    return Ok(());
  }

  let verb = if enabled { "enabled" } else { "disabled" };

  println!("{verb} {full_name}");

  Ok(())
}

/// The advice `run` prints instead of a false "enabled"/"disabled" success when
/// `full_name` names the mod loader, matching the voice of
/// [`nothing_toggleable_message`], which already talks about the loader being
/// always active.
fn loader_not_toggled_message(full_name: &str) -> String {
  crate::error::advice_message(
    &format!("{full_name} is the mod loader, so it was not changed."),
    "It is always active with no disabled state, matching r2modman, so it \
     cannot be enabled or disabled.",
    &["vmm list"],
  )
}

/// Enables or disables every mod recorded in the target's `mods.yml`.
///
/// The engine's [`profile::plan_set_enabled_batch`] excludes the mod loader
/// from the batch entirely rather than merely skipping it: a `ToggleBatch`
/// holds a recognised loader apart in `loaders` rather than in `changing` or
/// `already`, so counting it as "already enabled" the way a mere skip would is
/// never on the table.
///
/// One mod's failure does not abort the batch. Returning early would leave a
/// half-applied toggle with nothing reported, so every failure is collected, the
/// successes are still printed, and an error naming the failures is returned at
/// the end so the exit code is non-zero.
pub fn run_all(eco: &Ecosystem, target: &Target, enabled: bool) -> AppResult<()> {
  let action = if enabled { "enable" } else { "disable" };
  let verb = if enabled { "enabled" } else { "disabled" };

  // Read the record first, not for its own sake: the planner below reads the
  // same file immediately after, but reading it here first means an unreadable
  // mods.yml speaks in vmm's advice voice instead of the engine's raw error
  // surfacing from inside the planner.
  super::read_modlist(target)?;

  let batch = profile::plan_set_enabled_batch(&target.dir, eco, None, enabled)?;

  if batch.changing.is_empty() && batch.already.is_empty() {
    println!(
      "vmm: {}",
      nothing_toggleable_message(target, action, batch.loaders.is_empty())
    );

    return Ok(());
  }

  let outcome = profile::set_enabled_batch(&target.dir, eco, GAME, &batch)?;

  for name in &outcome.succeeded {
    println!("{verb} {name}");
  }

  let mut failed = Vec::new();

  for (name, error) in &outcome.failed {
    eprintln!("vmm: couldn't {action} {name}: {error}");

    failed.push(name.clone());
  }

  report_toggle_all(verb, outcome.succeeded.len(), batch.already.len(), &failed)
}

/// The no-op advice `run_all` prints when nothing here can be toggled, worded
/// according to whether the target is bare or holds only a mod loader.
///
/// The two cases read differently: a bare target has nothing installed at
/// all, while a loader-only target does have something installed, just
/// nothing with a disabled state. Saying "the mod loader is always active"
/// when there is no loader, or anything else, installed would be a false
/// claim about the target.
fn nothing_toggleable_message(target: &Target, action: &str, mods_is_empty: bool) -> String {
  let detail = if mods_is_empty {
    "Nothing is installed here, so nothing was changed."
  } else {
    "Only the mod loader is installed here, and it is always active with no \
     disabled state, so nothing was changed."
  };

  crate::error::advice_message(
    &format!(
      "there is nothing to {action} in {}.",
      crate::target::describe(target)
    ),
    detail,
    &["vmm list"],
  )
}

/// Summarises a `run_all` pass, erroring when any mod failed.
fn report_toggle_all(
  verb: &str,
  changed: usize,
  unchanged: usize,
  failed: &[String],
) -> AppResult<()> {
  println!("{}.", toggle_summary(verb, changed, unchanged));

  if failed.is_empty() {
    return Ok(());
  }

  Err(AppError::advice(
    format!("{} of the mods could not be {verb}.", failed.len()),
    format!(
      "These were left as they were: {}\n\nEverything else was {verb} \
       successfully.",
      failed.join(", ")
    ),
    &["vmm list"],
  ))
}

/// Builds the summary line [`report_toggle_all`] prints, agreeing in number on
/// both counts it names: `"1 mod"` vs `"N mods"` for the count itself, and
/// `"was"` vs `"were"` for the already-toggled clause.
fn toggle_summary(verb: &str, changed: usize, unchanged: usize) -> String {
  let mut summary = format!("\n{verb} {}", super::describe_count(changed));

  if unchanged > 0 {
    let auxiliary = if unchanged == 1 { "was" } else { "were" };

    summary.push_str(&format!(
      ", {} {auxiliary} already {verb}",
      super::describe_count(unchanged)
    ));
  }

  summary
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use thunderstore_engine::profile::modlist;
  use tokio::runtime::Runtime;

  #[test]
  fn disable_then_enable_moves_files_and_flips_mods_yml() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    let plugin = target.dir.join("BepInEx/plugins/Owner-ModA/ModA.dll");
    let disabled = target.dir.join("BepInEx/plugins/Owner-ModA/ModA.dll.old");

    run(&eco, &target, "Owner-ModA", false).unwrap();

    assert!(disabled.exists());
    assert!(!plugin.exists());
    assert!(!modlist::read(&target.dir).unwrap()[0].enabled);

    run(&eco, &target, "Owner-ModA", true).unwrap();

    assert!(plugin.exists());
    assert!(!disabled.exists());
    assert!(modlist::read(&target.dir).unwrap()[0].enabled);
  }

  #[test]
  fn toggling_in_a_profile_renames_inside_the_profile_only() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("experiment");
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    run(&eco, &target, "Owner-ModA", false).unwrap();

    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll.old")
        .exists()
    );
    assert!(!modlist::read(&target.dir).unwrap()[0].enabled);
    // The game directory is not this target: nothing was written there.
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
  fn toggling_a_mod_that_is_not_installed_fails() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    assert!(run(&eco, &target, "Owner-Absent", false).is_err());
  }

  #[test]
  fn disable_all_then_enable_all_round_trips_every_mod() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string(), "Owner-ModB".to_string()],
      ))
      .unwrap();

    run_all(&eco, &target, false).unwrap();

    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll.old")
        .exists()
    );
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModB/ModB.dll.old")
        .exists()
    );
    assert!(
      modlist::read(&target.dir)
        .unwrap()
        .iter()
        .all(|entry| !entry.enabled)
    );

    run_all(&eco, &target, true).unwrap();

    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
    assert!(
      modlist::read(&target.dir)
        .unwrap()
        .iter()
        .all(|entry| entry.enabled)
    );
  }

  #[test]
  fn toggling_all_with_nothing_installed_is_a_friendly_no_op() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // No mods.yml at all: an empty target must succeed rather than error.
    assert!(run_all(&eco, &target, false).is_ok());
  }

  #[test]
  fn an_unreadable_mods_yml_reports_its_path_before_toggling_one() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    std::fs::write(target.mods_yml(), "not: [valid").unwrap();

    let message = run(&eco, &target, "Owner-ModA", false)
      .unwrap_err()
      .to_string();

    // The same advice voice `run_all` and the other target-scoped commands
    // use, not the engine's raw parse error leaking through `set_enabled_in`.
    assert!(message.contains("the install record for"), "got: {message}");
    assert!(message.contains(&target.mods_yml().display().to_string()));
  }

  #[test]
  fn an_unreadable_mods_yml_reports_its_path_before_toggling_all() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    std::fs::write(target.mods_yml(), "not: [valid").unwrap();

    let message = run_all(&eco, &target, false).unwrap_err().to_string();

    // vmm's own advice voice, the same shape `list`/`update`/`uninstall` use,
    // not the engine's raw parse error leaking through the planner.
    assert!(message.contains("the install record for"), "got: {message}");
    assert!(message.contains(&target.mods_yml().display().to_string()));
  }

  #[test]
  fn nothing_toggleable_message_distinguishes_bare_and_loader_only_targets() {
    let fixture = Fixture::new();
    let target = fixture.target();

    let bare = nothing_toggleable_message(&target, "enable", true);

    assert!(bare.contains("Nothing is installed here"), "got: {bare}");
    assert!(
      !bare.contains("mod loader"),
      "a bare target has no loader to claim is always active; got: {bare}"
    );

    let loader_only = nothing_toggleable_message(&target, "enable", false);

    assert!(loader_only.contains("mod loader"), "got: {loader_only}");
    assert!(loader_only.contains("always active"), "got: {loader_only}");
  }

  #[test]
  fn toggle_summary_agrees_in_number_for_a_single_mod() {
    assert_eq!(toggle_summary("enabled", 1, 0), "\nenabled 1 mod");
    assert_eq!(
      toggle_summary("disabled", 1, 1),
      "\ndisabled 1 mod, 1 mod was already disabled"
    );
    assert_eq!(
      toggle_summary("disabled", 3, 2),
      "\ndisabled 3 mods, 2 mods were already disabled"
    );
  }

  #[test]
  fn toggling_all_leaves_the_mod_loader_alone() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &[
          "Owner-ModA".to_string(),
          "denikson-BepInExPack_Valheim".to_string(),
        ],
      ))
      .unwrap();

    run_all(&eco, &target, false).unwrap();

    let mods = modlist::read(&target.dir).unwrap();
    let loader = modlist::find(&mods, "denikson-BepInExPack_Valheim").unwrap();
    let regular = modlist::find(&mods, "Owner-ModA").unwrap();

    // The engine no-ops a loader toggle, so its record must stay enabled and
    // its files must stay in place.
    assert!(loader.enabled);
    assert!(target.dir.join("winhttp.dll").exists());
    assert!(!regular.enabled);
  }

  #[test]
  fn disabling_the_mod_loader_by_name_leaves_it_enabled() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["denikson-BepInExPack_Valheim".to_string()],
      ))
      .unwrap();

    // Naming the loader directly must not error and must not report a change
    // that did not happen: the engine no-ops a loader toggle, so its record
    // must stay enabled and its files must stay in place.
    run(&eco, &target, "denikson-BepInExPack_Valheim", false).unwrap();

    let mods = modlist::read(&target.dir).unwrap();
    let loader = modlist::find(&mods, "denikson-BepInExPack_Valheim").unwrap();

    assert!(loader.enabled);
    assert!(target.dir.join("winhttp.dll").exists());
  }

  #[test]
  fn loader_not_toggled_message_names_the_loader_and_says_it_is_always_active() {
    let message = loader_not_toggled_message("denikson-BepInExPack_Valheim");

    assert!(
      message.contains("denikson-BepInExPack_Valheim"),
      "got: {message}"
    );
    assert!(message.contains("always active"), "got: {message}");
    assert!(message.contains("was not changed"), "got: {message}");
  }

  #[test]
  fn toggling_all_with_no_toggleable_mods_is_a_no_op() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // Only the loader installed, which cannot be toggled, so there is nothing
    // to do and that is not a failure.
    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["denikson-BepInExPack_Valheim".to_string()],
      ))
      .unwrap();

    assert!(run_all(&eco, &target, false).is_ok());
  }

  #[test]
  fn toggling_all_with_a_mix_of_already_toggled_and_not_disables_only_what_changed() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string(), "Owner-ModB".to_string()],
      ))
      .unwrap();

    // Owner-ModA is already disabled before the batch runs, so the "were
    // already {verb}" branch runs alongside a mod that actually changes.
    run(&eco, &target, "Owner-ModA", false).unwrap();

    run_all(&eco, &target, false).unwrap();

    let mods = modlist::read(&target.dir).unwrap();

    assert!(!modlist::find(&mods, "Owner-ModA").unwrap().enabled);
    assert!(!modlist::find(&mods, "Owner-ModB").unwrap().enabled);
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModB/ModB.dll.old")
        .exists()
    );
  }

  #[cfg(unix)]
  #[test]
  fn a_mid_batch_failure_still_toggles_the_rest_and_names_the_failure() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string(), "Owner-ModB".to_string()],
      ))
      .unwrap();

    let blocked_dir = target.dir.join("BepInEx/plugins/Owner-ModA");
    let original_mode = fs::metadata(&blocked_dir).unwrap().permissions().mode();

    // Renaming a file to `.old` needs write permission on its containing
    // directory, not the file itself, so stripping it here makes Owner-ModA's
    // toggle fail with a real EACCES without touching the file.
    fs::set_permissions(&blocked_dir, fs::Permissions::from_mode(0o555)).unwrap();

    let result = run_all(&eco, &target, false);

    // Restored before asserting, so the fixture's TempDir can clean itself up
    // even if an assertion below fails.
    fs::set_permissions(&blocked_dir, fs::Permissions::from_mode(original_mode)).unwrap();

    let error = result.unwrap_err();

    assert!(error.to_string().contains("Owner-ModA"));

    let mods = modlist::read(&target.dir).unwrap();

    // The batch kept going: Owner-ModB still toggled despite Owner-ModA's
    // failure.
    assert!(!modlist::find(&mods, "Owner-ModB").unwrap().enabled);
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModB/ModB.dll.old")
        .exists()
    );
  }
}
