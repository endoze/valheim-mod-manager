pub mod install;
pub mod launch;
pub mod list;
pub mod migrate;
pub mod portability;
pub mod profile;
pub mod search;
pub mod toggle;
pub mod uninstall;
pub mod update;

use crate::error::{AppError, AppResult};
use crate::target::Target;
use thunderstore_engine::profile::BatchOutcome;
use thunderstore_engine::profile::modlist::{self, ProfileMod};

/// Reads the target's record, naming the target on failure.
///
/// The engine's message already names the file, guaranteed as of the record
/// error-path change, so this adds only which target it was about.
///
/// Deliberately suggests no follow-up command. Every command that touches a
/// target reads the record through here first, `list` included, so any `vmm`
/// invocation offered as the next step would fail with this same message; the
/// only way forward is to repair the file itself.
pub fn read_modlist(target: &Target) -> AppResult<Vec<ProfileMod>> {
  modlist::read(&target.dir).map_err(|error| {
    AppError::advice(
      format!(
        "the install record for {} could not be read.",
        crate::target::describe(target)
      ),
      format!(
        "{error}\n\nNothing was changed. Every vmm command reads this file \
         before doing anything, so each will refuse the same way until it \
         parses. Repair {} by hand if you can read what it once held; \
         otherwise delete it and reinstall the mods you had, which writes a \
         fresh one.",
        target.mods_yml().display()
      ),
      &[],
    )
  })
}

/// Renders `count` as `"1 mod"` or `"N mods"`, so a prompt or summary reads
/// correctly at the boundary that matters most: a single-mod count.
pub fn describe_count(count: usize) -> String {
  match count {
    1 => "1 mod".to_string(),
    n => format!("{n} mods"),
  }
}

/// Prints each installed identifier with the version recorded in `mods.yml`, so
/// what is reported is what was actually written.
pub fn report_installed(target: &Target, installed: &[String]) -> AppResult<()> {
  let mods = read_modlist(target)?;

  for name in installed {
    match modlist::find(&mods, name) {
      Some(entry) => println!("installed {} {}", entry.name, entry.version_number),
      None => println!("installed {name}"),
    }
  }

  Ok(())
}

/// Turns a batch's per-item failures into a single error, after its successes
/// have already been printed.
///
/// A batch does not abort on the first failure, so the successes are real and
/// worth reporting; this is what still makes the exit code non-zero.
pub fn report_batch_failures(outcome: &BatchOutcome) -> AppResult<()> {
  if outcome.is_ok() {
    return Ok(());
  }

  let named: Vec<String> = outcome
    .failed
    .iter()
    .map(|(name, error)| format!("{name}: {error}"))
    .collect();

  // Counted from the outcome rather than asserted. A batch where every item
  // failed is the ordinary shape of a single-mod command that went wrong, and
  // claiming "everything else succeeded" there names successes that do not
  // exist. For an install this count is the dependency closure that landed
  // rather than the identifiers the user typed, which is what actually
  // succeeded and so what is worth reporting.
  let tail = match outcome.succeeded.is_empty() {
    true => String::new(),
    false => format!("\n\n{} succeeded.", describe_count(outcome.succeeded.len())),
  };

  Err(AppError::advice(
    format!(
      "{} could not be done.",
      describe_count(outcome.failed.len())
    ),
    format!(
      "These were left as they were:\n\n  {}{tail}",
      named.join("\n  ")
    ),
    &["vmm list"],
  ))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;

  #[test]
  fn an_installed_name_the_record_does_not_carry_is_still_reported() {
    let fixture = Fixture::new();
    let target = fixture.target();

    // The version comes out of `mods.yml` so what is reported is what was
    // actually written. An identifier the record does not name therefore has no
    // version to quote, and the fallback exists so it is still reported rather
    // than silently dropped from a batch's output: `install_batch` records a
    // success before this reads the file back, so a name that went missing in
    // between is precisely the case worth seeing.
    report_installed(&target, &["Owner-ModA".to_string()]).unwrap();
  }

  /// A batch that failed `failed` items and completed `succeeded` ones.
  fn outcome(succeeded: &[&str], failed: &[&str]) -> BatchOutcome {
    let mut outcome = BatchOutcome::default();

    outcome.succeeded = succeeded.iter().map(|name| name.to_string()).collect();
    outcome.failed = failed
      .iter()
      .map(|name| {
        (
          name.to_string(),
          thunderstore_engine::Error::Profile("no such package".to_string()),
        )
      })
      .collect();

    outcome
  }

  #[test]
  fn a_batch_where_nothing_succeeded_does_not_claim_otherwise() {
    let message = report_batch_failures(&outcome(&[], &["Owner-Typo"]))
      .unwrap_err()
      .to_string();

    // The shape of every single-mod command that went wrong. There is no "else"
    // to have succeeded, so saying so would be a plain falsehood in the most
    // common failure a user meets.
    assert!(
      !message.contains("succeeded"),
      "a batch with no successes must not report any; got: {message}"
    );
    assert!(message.contains("Owner-Typo"), "got: {message}");
    // `describe_count` is how every other summary in the CLI words this
    // boundary, so a single failure reads as "1 mod", never "1 of the mods".
    assert!(
      message.contains("1 mod could not be done"),
      "got: {message}"
    );
  }

  #[test]
  fn a_partial_batch_counts_both_sides() {
    let message = report_batch_failures(&outcome(&["Owner-ModA", "Owner-ModB"], &["Owner-Typo"]))
      .unwrap_err()
      .to_string();

    assert!(
      message.contains("1 mod could not be done"),
      "got: {message}"
    );
    assert!(message.contains("2 mods succeeded"), "got: {message}");
  }

  #[test]
  fn a_batch_with_no_failures_is_not_an_error_at_all() {
    assert!(report_batch_failures(&outcome(&["Owner-ModA"], &[])).is_ok());
  }
}
