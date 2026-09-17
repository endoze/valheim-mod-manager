use crate::error::{AppError, AppResult};
use crate::target::{GAME, Target};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile::{self, NotRemovable, UninstallBatch, modlist};

/// Uninstalls each mod from the target, removing its files, its `_state`
/// payload, and its `mods.yml` entry.
///
/// The batch is planned through the engine's [`profile::plan_uninstall_batch`]
/// before anything is removed, so one bad identifier cannot leave a partial
/// uninstall behind. That refusal collapses to one sentence, so [`plan_batch`]
/// asks [`profile::check_removable_batch`] for the typed reason instead and
/// words each kind the way vmm always has.
///
/// The engine's `uninstall_mod_in` does not remove one mod's folders in
/// isolation: it derives a keep-set from the *remaining* `mods.yml` and hands
/// it to `remove_delisted`, which deletes every `<Owner-Name>`-shaped folder
/// under every namespaced route that the keep-set does not name. In game-dir
/// mode that route tree is the user's live game directory, so a mod folder
/// placed there by hand, or by r2modman, or by an older vmm whose `mod_list`
/// was never migrated, is swept along with the mod actually being
/// uninstalled. Rather than silently deleting somebody else's install, the
/// planned batch's `untracked` list names exactly those folders and
/// [`untracked_error`] refuses the whole command with them, unless `force`
/// says to go ahead.
pub fn run(eco: &Ecosystem, target: &Target, mods: &[String], force: bool) -> AppResult<()> {
  let installed = super::read_modlist(target)?;

  for full_name in mods {
    if modlist::find(&installed, full_name).is_none() {
      return Err(AppError::advice(
        format!(
          "{full_name} is not installed in {}.",
          crate::target::describe(target)
        ),
        "Nothing was removed. Identifiers are `Owner-ModName` and are \
         case-sensitive, so check the spelling against what is installed.",
        &["vmm list"],
      ));
    }
  }

  let batch = plan_batch(eco, target, Some(mods))?;

  if !force && !batch.untracked.is_empty() {
    return Err(untracked_error(target, &batch));
  }

  let outcome = profile::uninstall_batch(&target.dir, eco, GAME, &batch)?;

  for name in &outcome.succeeded {
    println!("removed {name}");
  }

  super::report_batch_failures(&outcome)
}

/// Plans an uninstall batch, wording the engine's typed refusal as vmm's two
/// distinct advice messages when a subject cannot be removed exactly.
///
/// `plan_uninstall_batch` returns a single `Err` for both an absent install
/// record and an unreadable one, so [`profile::check_removable_batch`] is
/// called here for the [`NotRemovable`] behind it: two refusals that want
/// different advice, and the engine has already decided which is which. When
/// nothing is blocked, the engine's own refusal is returned as is, which is
/// only reachable through `run_all`'s `names: None`, since [`run`] above
/// already checked every name's presence before calling this.
fn plan_batch(
  eco: &Ecosystem,
  target: &Target,
  names: Option<&[String]>,
) -> AppResult<UninstallBatch> {
  let plan_error = match profile::plan_uninstall_batch(&target.dir, eco, GAME, names) {
    Ok(batch) => return Ok(batch),
    Err(error) => error,
  };

  let subjects = match names {
    Some(names) => names.to_vec(),
    None => super::read_modlist(target)?
      .into_iter()
      .map(|entry| entry.name)
      .collect(),
  };

  let blocked = profile::check_removable_batch(&target.dir, eco, &subjects);

  if let Some((name, refusal)) = blocked.into_iter().next() {
    let install = format!("vmm install {name}");

    return Err(match refusal {
      NotRemovable::RecordUnreadable { path, source, .. } => AppError::advice(
        format!("{name}'s install record can't be read, so it cannot be removed exactly."),
        format!(
          "vmm found a record at {} but couldn't parse it: {source}\n\nThe record \
           is unreadable, not absent, so vmm will not guess what it once tracked. \
           Nothing was removed.\n\nReinstall {name} to rebuild its record, then \
           try again, or remove its files by hand.",
          path.display()
        ),
        &[install.as_str()],
      ),
      NotRemovable::RecordAbsent { path, .. } => AppError::advice(
        format!("{name} has no install record, so it cannot be removed exactly."),
        format!(
          "vmm expected one at {}. This target was adopted from disk rather than \
           installed, so vmm does not know which files the loader placed and will \
           not guess. Nothing was removed.\n\nReinstall the loader to give it a \
           record, then try again, or remove its files by hand.",
          path.display()
        ),
        &[install.as_str()],
      ),
    });
  }

  Err(plan_error.into())
}

/// The refusal [`run`] returns when the reconcile sweep would take mod folders
/// `mods.yml` does not track, unless `force` accepts that.
fn untracked_error(target: &Target, batch: &UninstallBatch) -> AppError {
  AppError::advice(
    format!(
      "{} holds mod folders that vmm does not track.",
      crate::target::describe(target)
    ),
    format!(
      "An uninstall reconciles every install route against {}, so these \
       would be deleted along with the mods you named:\n\n  {}\n\nNothing \
       was removed. Bring them under management with `vmm install \
       <Owner-ModName>` if you installed them by hand, or `vmm migrate` if \
       they're left over from a deprecated `mod_list`. Otherwise, re-run \
       with `--force` to delete them too.",
      target.mods_yml().display(),
      render_untracked(&batch.untracked)
    ),
    &["vmm install <Owner-ModName>", "vmm migrate"],
  )
}

/// Renders a batch's untracked folders as the indented list every uninstall
/// message shows them in.
fn render_untracked(untracked: &[PathBuf]) -> String {
  untracked
    .iter()
    .map(|path| path.display().to_string())
    .collect::<Vec<_>>()
    .join("\n  ")
}

/// How many mods `batch` accounts for, loaders included.
///
/// A local helper rather than a method on `UninstallBatch`: it is a foreign
/// type, so vmm cannot add an inherent method to it.
fn batch_len(batch: &UninstallBatch) -> usize {
  batch.mods.len() + batch.loaders.len()
}

/// Uninstalls every mod in the target, confirming first.
///
/// `assume_yes` skips `confirm`. Because the prompt is what lists the untracked
/// folders the sweep would take, skipping it re-imposes the `--force` gate the
/// single-mod path uses: the prompt can be skipped, the awareness cannot.
///
/// `confirm` is taken as a parameter rather than called directly so tests can
/// exercise the interactive branch (decline, proceed without `--force`, and the
/// batch handed to the prompt) without a TTY. [`confirm_uninstall_all`] is the
/// real implementation `main` passes.
pub fn run_all(
  eco: &Ecosystem,
  target: &Target,
  force: bool,
  assume_yes: bool,
  confirm: impl FnOnce(&Target, &UninstallBatch) -> AppResult<bool>,
) -> AppResult<()> {
  let batch = plan_batch(eco, target, None)?;

  if batch_len(&batch) == 0 {
    println!(
      "vmm: {}",
      crate::error::advice_message(
        &format!(
          "there is nothing to uninstall in {}.",
          crate::target::describe(target)
        ),
        "No mods are installed here, so nothing was changed.",
        &["vmm list"],
      )
    );

    return Ok(());
  }

  if assume_yes {
    if !batch.untracked.is_empty() && !force {
      return Err(AppError::advice(
        format!(
          "{} holds mod folders that vmm does not track.",
          crate::target::describe(target)
        ),
        format!(
          "Removing every mod reconciles each install route against {}, so \
           these would be deleted too:\n\n  {}\n\nNothing was removed. \
           `--yes` skips the prompt that would have listed them, so confirm \
           them with `--force` instead.",
          target.mods_yml().display(),
          render_untracked(&batch.untracked)
        ),
        &["vmm uninstall --all --yes --force"],
      ));
    }
  } else if !confirm(target, &batch)? {
    println!("Nothing was changed.");

    return Ok(());
  }

  let outcome = profile::uninstall_batch(&target.dir, eco, GAME, &batch)?;

  for name in &outcome.succeeded {
    println!("{}", removed_line(name, &batch));
  }

  if let Some(summary) = removed_summary(&outcome.succeeded, &batch, target) {
    println!("{summary}");
  }

  super::report_batch_failures(&outcome)
}

/// The line `run_all` prints for one removed identifier: the mod loader gets a
/// parenthetical naming it, an ordinary mod does not.
///
/// Pulled out as a pure function so the two wordings are directly testable
/// without capturing stdout, the same way [`untracked_error`] and
/// [`no_terminal_error`] are.
fn removed_line(name: &str, batch: &UninstallBatch) -> String {
  match batch.loaders.iter().any(|loader| loader == name) {
    true => format!("removed {name} (the mod loader)"),
    false => format!("removed {name}"),
  }
}

/// The summary line `run_all` prints once the batch has run: mentions the target
/// going vanilla when a loader was among what was removed.
///
/// Counted from `succeeded` rather than from `batch`, and the loader checked for
/// membership in it rather than merely being planned, because
/// [`profile::uninstall_batch`] records per-item failures instead of aborting.
/// The batch is what was attempted; only `succeeded` is what came off, and
/// reporting the plan would claim removals that failed and call a target with
/// its loader still in place vanilla.
///
/// A batch where nothing at all came off has no summary to print: `None` rather
/// than a "removed 0 mods" line, since [`super::report_batch_failures`] is about
/// to name every failure anyway.
///
/// Pulled out for the same reason as [`removed_line`].
fn removed_summary(
  succeeded: &[String],
  batch: &UninstallBatch,
  target: &Target,
) -> Option<String> {
  if succeeded.is_empty() {
    return None;
  }

  let loader_removed = succeeded.iter().any(|name| batch.loaders.contains(name));

  Some(match loader_removed {
    false => format!(
      "\nremoved {} from {}.",
      super::describe_count(succeeded.len()),
      crate::target::describe(target)
    ),
    true => format!(
      "\nremoved {}; {} is now vanilla.",
      super::describe_count(succeeded.len()),
      crate::target::describe(target)
    ),
  })
}

/// The refusal [`confirm_uninstall_all`] returns when there is no terminal to
/// confirm with.
///
/// Pulled out as a pure function (no I/O) so its message is directly testable.
/// `confirm_uninstall_all` itself cannot safely be exercised end to end in a
/// test either way it could go: a real TTY would block on `read_line` waiting
/// for input that never comes, and there is no portable way to fake "this is
/// not a terminal" for `io::stdin()` from within the test process. This is the
/// one piece of that function a test can call directly, with zero TTY risk.
fn no_terminal_error(target: &Target, batch: &UninstallBatch) -> AppError {
  AppError::advice(
    "removing every mod needs confirming, and there is no terminal to ask.",
    format!(
      "This would remove {} from {} ({}). Nothing was changed.\n\nRe-run with \
       `--yes` to confirm without a prompt.",
      super::describe_count(batch_len(batch)),
      crate::target::describe(target),
      target.dir.display()
    ),
    &["vmm uninstall --all --yes"],
  )
}

/// Describes what will be removed and asks for confirmation.
///
/// A non-interactive stdin cannot answer, so rather than defaulting either way
/// this refuses and names the flag that makes the intent explicit.
pub fn confirm_uninstall_all(target: &Target, batch: &UninstallBatch) -> AppResult<bool> {
  if !io::stdin().is_terminal() {
    return Err(no_terminal_error(target, batch));
  }

  println!(
    "This will remove {} from {}:\n  {}",
    super::describe_count(batch_len(batch)),
    crate::target::describe(target),
    target.dir.display()
  );

  match batch.loaders.len() {
    0 => {}
    1 => println!("\nThe mod loader goes too, so you'll be left with a vanilla install."),
    _ => println!("\nThe mod loaders go too, so you'll be left with a vanilla install."),
  }

  println!("Your config files under BepInEx/config are left alone.");

  if !batch.untracked.is_empty() {
    println!(
      "\nThese mod folders aren't tracked by vmm and will be removed as \
       well:\n  {}",
      render_untracked(&batch.untracked)
    );
  }

  print!(
    "\nRemove {}? [y/N] ",
    super::describe_count(batch_len(batch))
  );
  io::stdout().flush()?;

  let mut answer = String::new();

  io::stdin().read_line(&mut answer)?;

  let answer = answer.trim().to_ascii_lowercase();

  Ok(answer == "y" || answer == "yes")
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use thunderstore_engine::ecosystem::Ecosystem;
  use thunderstore_engine::profile::modlist;
  use tokio::runtime::Runtime;

  #[test]
  fn an_unreadable_mods_yml_reports_its_path() {
    let fixture = Fixture::new();
    let target = fixture.target();

    std::fs::write(target.mods_yml(), "not: [valid").unwrap();

    let message = crate::commands::read_modlist(&target)
      .unwrap_err()
      .to_string();

    assert!(
      message.contains(&target.mods_yml().display().to_string()),
      "the error must name the file that could not be read; got: {message}"
    );
    assert!(
      message.contains("the game directory"),
      "the error must name the target; got: {message}"
    );
    // `list::run` reads this same file through this same function before doing
    // anything else, so offering `vmm list` as the way forward promises a
    // command that is guaranteed to fail with this exact message again.
    assert!(
      !message.contains("vmm list"),
      "the advice must not be a command that reproduces the error; got: {message}"
    );
    assert!(message.contains("Nothing was changed"), "got: {message}");
  }

  #[test]
  fn uninstall_removes_files_and_the_mods_yml_entry() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string(), "Owner-ModB".to_string()],
      ))
      .unwrap();

    run(&eco, &target, &["Owner-ModB".to_string()], false).unwrap();

    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
    assert!(!target.dir.join("BepInEx/plugins/Owner-ModB").exists());

    let mods = modlist::read(&target.dir).unwrap();

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name, "Owner-ModA");
  }

  #[test]
  fn uninstall_refuses_when_an_untracked_mod_folder_would_be_swept() {
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

    // A mod installed by hand (or by r2modman): `mods.yml` never recorded it, so
    // the sweep inside `uninstall_mod_in` would take it along.
    let by_hand = target.dir.join("BepInEx/plugins/Azumatt-AzuAntiCheat");
    // A loader-shipped folder that is not `<Owner-Name>`-shaped, which the sweep
    // ignores, so it must not be reported either.
    let loader_folder = target.dir.join("BepInEx/plugins/MMHOOK");

    std::fs::create_dir_all(&by_hand).unwrap();
    std::fs::write(by_hand.join("AzuAntiCheat.dll"), b"dll").unwrap();
    std::fs::create_dir_all(&loader_folder).unwrap();

    let result = run(&eco, &target, &["Owner-ModB".to_string()], false);
    let message = result.unwrap_err().to_string();

    assert!(
      message.contains("Azumatt-AzuAntiCheat"),
      "the error must name the folder at risk; got: {message}"
    );
    assert!(
      !message.contains("MMHOOK"),
      "a folder the sweep would never remove must not be reported; got: {message}"
    );
    assert!(message.contains("--force"), "got: {message}");
    // Nothing was mutated: the check runs before any removal.
    assert!(by_hand.join("AzuAntiCheat.dll").exists());
    assert!(target.dir.join("BepInEx/plugins/Owner-ModB").exists());
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 2);
  }

  #[test]
  fn force_uninstalls_and_accepts_the_sweep() {
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

    let by_hand = target.dir.join("BepInEx/plugins/Azumatt-AzuAntiCheat");

    std::fs::create_dir_all(&by_hand).unwrap();
    std::fs::write(by_hand.join("AzuAntiCheat.dll"), b"dll").unwrap();

    run(&eco, &target, &["Owner-ModB".to_string()], true).unwrap();

    assert!(!target.dir.join("BepInEx/plugins/Owner-ModB").exists());
    // `--force` accepts the documented consequence: mods.yml is authoritative
    // over the whole namespaced tree, so an unrecorded folder goes with it.
    assert!(!by_hand.exists());
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn uninstall_from_a_profile_never_touches_the_game_dir() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("experiment");
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

    // A game directory holding a hand-installed mod: a profile-mode uninstall
    // reconciles the profile's routes, so it must neither refuse because of this
    // folder nor delete it.
    let in_game_dir = fixture
      .game_dir
      .path()
      .join("BepInEx/plugins/Azumatt-AzuAntiCheat");

    std::fs::create_dir_all(&in_game_dir).unwrap();
    std::fs::write(in_game_dir.join("AzuAntiCheat.dll"), b"dll").unwrap();

    run(&eco, &target, &["Owner-ModB".to_string()], false).unwrap();

    assert!(!target.dir.join("BepInEx/plugins/Owner-ModB").exists());
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
    assert!(in_game_dir.join("AzuAntiCheat.dll").exists());
    assert!(
      !fixture
        .game_dir
        .path()
        .join("BepInEx/plugins/Owner-ModA")
        .exists()
    );
  }

  #[test]
  fn uninstall_rejects_the_whole_batch_when_one_mod_is_absent() {
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

    let result = run(
      &eco,
      &target,
      &["Owner-ModA".to_string(), "Owner-Absent".to_string()],
      false,
    );

    assert!(result.is_err());
    // Nothing was removed: the batch is checked before any mutation.
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn uninstalling_a_mod_that_is_not_installed_points_at_list() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    let message = run(&eco, &target, &["Owner-Absent".to_string()], false)
      .unwrap_err()
      .to_string();

    assert!(
      message.contains("Owner-Absent"),
      "the error must name the mod; got: {message}"
    );
    assert!(
      message.contains("vmm list"),
      "the error must say how to see what is installed; got: {message}"
    );
  }

  #[test]
  fn uninstalling_a_mixed_batch_refuses_before_touching_any_mod_when_the_loader_has_no_record() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("adopted");
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

    // Simulate a profile adopted from disk: mods.yml records the loader, but
    // the tracker that says which files it placed does not exist.
    std::fs::remove_file(thunderstore_engine::install::state_file_path(
      &target.dir,
      "denikson-BepInExPack_Valheim",
    ))
    .unwrap();

    // Owner-ModA is named first: without the pre-flight this would remove it,
    // then fail on the loader, leaving a half-done batch.
    let message = run(
      &eco,
      &target,
      &[
        "Owner-ModA".to_string(),
        "denikson-BepInExPack_Valheim".to_string(),
      ],
      false,
    )
    .unwrap_err()
    .to_string();

    assert!(
      message.contains("denikson-BepInExPack_Valheim"),
      "the error must name the loader; got: {message}"
    );
    assert!(message.contains("Nothing was removed"), "got: {message}");
    // Nothing was touched: Owner-ModA survives even though it was named first
    // and would otherwise have been removed before the loader was reached.
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 2);
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn skipping_the_prompt_over_untracked_folders_still_needs_force() {
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

    let by_hand = target.dir.join("BepInEx/plugins/Azumatt-AzuAntiCheat");

    std::fs::create_dir_all(&by_hand).unwrap();
    std::fs::write(by_hand.join("AzuAntiCheat.dll"), b"dll").unwrap();

    // --yes skips the prompt that would have listed these, so the awareness has
    // to come from --force instead. `assume_yes = true` means `confirm` is never
    // reached, so a confirmer that panics if called proves that.
    let message = run_all(&eco, &target, false, true, unreachable_confirm)
      .unwrap_err()
      .to_string();

    assert!(message.contains("Azumatt-AzuAntiCheat"), "got: {message}");
    assert!(message.contains("--force"), "got: {message}");
    assert!(by_hand.join("AzuAntiCheat.dll").exists());
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);

    // With --force it proceeds and accepts the documented consequence.
    run_all(&eco, &target, true, true, unreachable_confirm).unwrap();

    assert!(modlist::read(&target.dir).unwrap().is_empty());
    assert!(!by_hand.exists());
  }

  #[test]
  fn uninstalling_everything_with_nothing_installed_is_a_friendly_no_op() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // `assume_yes = false` so this actually proves the property that matters:
    // an empty target returns before the interactive prompt is ever reached.
    // `unreachable_confirm` panics if it is called, so this would fail loudly
    // if the empty-plan short circuit were ever removed.
    assert!(run_all(&eco, &target, false, false, unreachable_confirm).is_ok());
  }

  #[test]
  fn uninstalling_everything_from_a_profile_never_touches_the_game_dir() {
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

    let in_game_dir = fixture.game_dir.path().join("BepInEx/plugins/Owner-Kept");

    std::fs::create_dir_all(&in_game_dir).unwrap();
    std::fs::write(in_game_dir.join("Kept.dll"), b"dll").unwrap();

    // assume_yes so the interactive branch is not reached; nothing here is
    // untracked, so --force is not needed either.
    run_all(&eco, &target, false, true, unreachable_confirm).unwrap();

    assert!(modlist::read(&target.dir).unwrap().is_empty());
    assert!(in_game_dir.join("Kept.dll").exists());
  }

  #[test]
  fn run_all_removes_everything_including_the_loader() {
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

    // assume_yes so the interactive branch is not reached; nothing here is
    // untracked, so --force is not needed either. This drives `run_all`
    // through both of its loader-dependent print branches (`removed_line`'s
    // "(the mod loader)" suffix and `removed_summary`'s "is now vanilla"
    // wording, asserted for content below), proving the whole pipeline
    // reaches them and leaves the target vanilla. The removal order itself is
    // the engine's `uninstall_batch`'s guarantee, proved at the engine layer.
    run_all(&eco, &target, false, true, unreachable_confirm).unwrap();

    assert!(modlist::read(&target.dir).unwrap().is_empty());
    assert!(!target.dir.join("winhttp.dll").exists());
    assert!(!target.dir.join("BepInEx/core/BepInEx.dll").exists());
  }

  #[test]
  fn removed_line_and_summary_distinguish_the_mod_loader() {
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

    let batch = plan_batch(&eco, &target, None).unwrap();

    assert_eq!(removed_line("Owner-ModA", &batch), "removed Owner-ModA");
    assert_eq!(
      removed_line("denikson-BepInExPack_Valheim", &batch),
      "removed denikson-BepInExPack_Valheim (the mod loader)"
    );

    let vanilla_summary = removed_summary(
      &[
        "Owner-ModA".to_string(),
        "denikson-BepInExPack_Valheim".to_string(),
      ],
      &batch,
      &target,
    )
    .unwrap();

    assert!(
      vanilla_summary.contains("is now vanilla"),
      "got: {vanilla_summary}"
    );

    let loaderless_fixture = Fixture::new();
    let loaderless_target = loaderless_fixture.target();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &loaderless_fixture.client,
        &eco,
        &loaderless_target,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    let loaderless_batch = plan_batch(&eco, &loaderless_target, None).unwrap();
    let plain_summary = removed_summary(
      &["Owner-ModA".to_string()],
      &loaderless_batch,
      &loaderless_target,
    )
    .unwrap();

    assert!(!plain_summary.contains("vanilla"), "got: {plain_summary}");
    assert!(plain_summary.contains("removed"), "got: {plain_summary}");
  }

  #[test]
  fn the_summary_reports_what_came_off_not_what_was_planned() {
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

    let batch = plan_batch(&eco, &target, None).unwrap();

    // `uninstall_batch` records per-item failures instead of aborting, so the
    // batch is what was *attempted*. A run where the loader failed to come off
    // must neither count it nor call the target vanilla, because it isn't.
    let partial = removed_summary(&["Owner-ModA".to_string()], &batch, &target).unwrap();

    assert!(
      partial.contains("removed 1 mod"),
      "the summary must count what succeeded; got: {partial}"
    );
    assert!(
      !partial.contains("vanilla"),
      "the loader is still installed, so the target is not vanilla; got: {partial}"
    );
    // Nothing came off at all: `report_batch_failures` is about to say why, so
    // there is no summary to print rather than a "removed 0 mods" line.
    assert_eq!(removed_summary(&[], &batch, &target), None);
  }

  #[test]
  fn planning_refuses_a_loader_with_no_install_record() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("adopted");
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

    // Simulate a profile adopted from disk: the record exists but the tracker
    // that says which files the loader placed does not.
    std::fs::remove_file(thunderstore_engine::install::state_file_path(
      &target.dir,
      "denikson-BepInExPack_Valheim",
    ))
    .unwrap();

    let message = plan_batch(&eco, &target, None).unwrap_err().to_string();

    assert!(
      message.contains("denikson-BepInExPack_Valheim"),
      "the error must name the loader; got: {message}"
    );
    // Refusing before mutating leaves the record intact.
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
  }

  #[test]
  fn planning_refuses_a_loader_whose_state_file_is_unparseable() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("adopted-with-junk");
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

    // The `_state` file is present (so a bare `.exists()` check would pass) but
    // truncated or hand-edited, so parsing it fails. `read_state_file` reports
    // that as `Err`, not `None`, and the engine's own `uninstall_mod_in` refuses
    // on exactly that predicate. Planning must use the same one, or this case
    // would pass the pre-flight, remove every other mod, and only then fail on
    // the loader itself.
    std::fs::write(
      thunderstore_engine::install::state_file_path(&target.dir, "denikson-BepInExPack_Valheim"),
      "not: [valid",
    )
    .unwrap();

    let message = plan_batch(&eco, &target, None).unwrap_err().to_string();

    assert!(
      message.contains("denikson-BepInExPack_Valheim"),
      "the error must name the loader; got: {message}"
    );
    // Distinguished from the missing-record case: the record exists, it just
    // cannot be read, so the advice should say so rather than reuse the
    // "no install record" wording.
    assert!(
      message.contains("can't be read"),
      "the message must say the record is unreadable, not absent; got: {message}"
    );
    assert!(message.contains("Nothing was removed"), "got: {message}");
    // Refusing before mutating leaves the record, and everything else, intact.
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
  }

  #[test]
  fn planning_refuses_a_regular_mod_whose_state_file_is_unparseable() {
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

    // Ordinary mods installed into a namespaced `<Owner-Name>/` folder get no
    // `_state` file at all: `remove_delisted` reconciles their folder by
    // presence, not via a tracker, so `write_state_file` is never called for
    // them. Nothing stops one from existing anyway though (a stray leftover, or
    // written by hand), so the `_state` directory and file are fabricated here
    // to exercise exactly that: a record that is present but unparseable for a
    // mod the loader pre-flight never used to check at all.
    let state_path = thunderstore_engine::install::state_file_path(&target.dir, "Owner-ModA");

    std::fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    std::fs::write(&state_path, "not: [valid").unwrap();

    let message = plan_batch(&eco, &target, None).unwrap_err().to_string();

    assert!(
      message.contains("Owner-ModA"),
      "the error must name the mod; got: {message}"
    );
    assert!(message.contains("can't be read"), "got: {message}");
    // Refusing before mutating leaves both mods fully present: recorded in
    // mods.yml and with their files still on disk.
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 2);
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModB/ModB.dll")
        .exists()
    );
  }

  /// A confirmer that panics if called, for `run_all` calls where `assume_yes`
  /// is `true` and the interactive path must not be reached at all.
  fn unreachable_confirm(_target: &Target, _batch: &UninstallBatch) -> AppResult<bool> {
    panic!("confirm must not be called when --yes skips the prompt")
  }

  #[test]
  fn declining_the_interactive_prompt_changes_nothing() {
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

    let result = run_all(&eco, &target, false, false, |_, _| Ok(false));

    assert!(result.is_ok());
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
    assert!(
      target
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn accepting_the_interactive_prompt_does_not_require_force() {
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

    let by_hand = target.dir.join("BepInEx/plugins/Azumatt-AzuAntiCheat");

    std::fs::create_dir_all(&by_hand).unwrap();
    std::fs::write(by_hand.join("AzuAntiCheat.dll"), b"dll").unwrap();

    // Unlike the `--yes` path, accepting the interactive prompt is enough on its
    // own: the prompt already listed the untracked folder, so `--force` is not
    // demanded here too.
    run_all(&eco, &target, false, false, |_, _| Ok(true)).unwrap();

    assert!(modlist::read(&target.dir).unwrap().is_empty());
    assert!(!by_hand.exists());
  }

  #[test]
  fn the_interactive_prompt_is_handed_the_plan_it_confirms() {
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

    let by_hand = target.dir.join("BepInEx/plugins/Azumatt-AzuAntiCheat");

    std::fs::create_dir_all(&by_hand).unwrap();
    std::fs::write(by_hand.join("AzuAntiCheat.dll"), b"dll").unwrap();

    let seen_untracked = std::cell::RefCell::new(Vec::new());

    run_all(&eco, &target, false, false, |_, batch: &UninstallBatch| {
      *seen_untracked.borrow_mut() = batch.untracked.clone();

      Ok(false)
    })
    .unwrap();

    assert!(
      seen_untracked
        .borrow()
        .iter()
        .any(|entry| entry.display().to_string().contains("Azumatt-AzuAntiCheat")),
      "the plan handed to the confirmer must name the untracked folder; got: {:?}",
      seen_untracked.borrow()
    );
    // Declined, so nothing was mutated.
    assert_eq!(modlist::read(&target.dir).unwrap().len(), 1);
    assert!(by_hand.join("AzuAntiCheat.dll").exists());
  }

  #[test]
  fn the_non_tty_refusal_names_yes() {
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

    let batch = plan_batch(&eco, &target, None).unwrap();

    let message = no_terminal_error(&target, &batch).to_string();

    assert!(message.contains("--yes"), "got: {message}");
  }
}
