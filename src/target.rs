use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use thunderstore_engine::profile::layout::BadProfile;

/// The Valheim community slug, which is also the engine's ecosystem game key.
pub const GAME: &str = "valheim";

/// Where this invocation installs, tracks, and launches from. The engine owns the
/// layout; vmm adds only the wording below.
pub type Target = thunderstore_engine::profile::target::InstallTarget;

/// A phrase naming `target` for use inside a sentence, so a message can say which
/// install root it is about without the caller branching on the mode.
///
/// A free function rather than a method because `Target` is a foreign type. This
/// is the only part of the target concept that stays in vmm: it is wording, and a
/// desktop client would write its own.
pub fn describe(target: &Target) -> String {
  match &target.profile {
    Some(name) => format!("profile {name:?}"),
    None => "the game directory".to_string(),
  }
}

/// Resolves the install target, wording a refused profile name.
///
/// Every command that takes an install target goes through here, so naming a
/// profile that does not exist reads the same whether it came from `--profile`,
/// a persisted selection, or `vmm profile use`.
pub fn resolve(base: PathBuf, game_dir: PathBuf, profile: Option<&str>) -> AppResult<Target> {
  Target::resolve(base, game_dir, GAME, profile).map_err(profile_error)
}

/// vmm's wording for an engine failure that refused a profile name.
///
/// Only `Missing` has advice worth adding: a name that could escape the profiles
/// directory is not one a user should be offered the chance to create, so it
/// keeps the engine's own sentence, as does any other kind of failure.
///
/// Takes the whole [`thunderstore_engine::Error`] rather than a
/// [`BadProfile`] so it can be dropped onto any engine operation that gates on a
/// profile name, however else that operation can fail. vmm adds wording without
/// taking over the operation.
pub fn profile_error(error: thunderstore_engine::Error) -> AppError {
  match error {
    thunderstore_engine::Error::BadProfile(BadProfile::Missing { name, .. }) => {
      missing_profile_error(&name)
    }
    other => other.into(),
  }
}

/// Whether `name` can stand as a bare argument in a command a user copies.
///
/// [`thunderstore_engine::profile::layout::validate_name`] refuses only the
/// names that could escape the profiles directory: empty, `.`, `..`, and
/// anything containing a separator or a NUL. Spaces, `;`, `$`, quotes, and
/// backticks are all legal profile names, so a name interpolated into a command
/// string is not automatically a single argument, and is not automatically
/// harmless either.
///
/// Deliberately an allowlist rather than an escaper. The characters permitted
/// here are bare-word safe in POSIX shells, `cmd.exe`, and PowerShell alike,
/// whereas quoting *correctly* differs between all three, and vmm has no way to
/// know which one the message is being read in. A leading `-` is excluded on top
/// of that, since it reads as a flag rather than a value.
fn is_bare_shell_word(name: &str) -> bool {
  !name.is_empty()
    && !name.starts_with('-')
    && name
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The advice returned when a name matches no existing profile.
///
/// [`crate::error::advice_message`] presents its `next` entries as commands to
/// copy, so the concrete `vmm profile create <name>` is only offered when the
/// name survives [`is_bare_shell_word`]. Anything else degrades to the
/// placeholder form: a name carrying a space would otherwise be pasted as two
/// arguments, and one carrying `;` would run whatever followed it. The headline
/// still shows the name as typed, where `{:?}` quotes and escapes it and nothing
/// invites a paste.
pub fn missing_profile_error(name: &str) -> AppError {
  let create = match is_bare_shell_word(name) {
    true => format!("vmm profile create {name}"),
    false => "vmm profile create <name>".to_string(),
  };

  AppError::advice(
    format!("there is no profile named {name:?}."),
    "Nothing was changed. Create it first, or list the profiles you have.",
    &[create.as_str(), "vmm profile list"],
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::tempdir;

  #[test]
  fn describe_names_the_profile_or_the_game_directory() {
    let base = tempdir().unwrap();
    let game_dir = tempdir().unwrap();

    let game = Target::resolve(
      base.path().to_path_buf(),
      game_dir.path().to_path_buf(),
      GAME,
      None,
    )
    .unwrap();

    assert_eq!(describe(&game), "the game directory");

    thunderstore_engine::profile::layout::create(base.path(), GAME, "experiment").unwrap();

    let profile = Target::resolve(
      base.path().to_path_buf(),
      game_dir.path().to_path_buf(),
      GAME,
      Some("experiment"),
    )
    .unwrap();

    assert_eq!(describe(&profile), "profile \"experiment\"");
  }

  #[test]
  fn resolving_an_unsafe_profile_name_still_refuses() {
    let base = tempdir().unwrap();
    let game_dir = tempdir().unwrap();

    // An unsafe name is not a profile a user could be offered the chance to
    // create, so it keeps the engine's refusal rather than the advice above.
    let error = resolve(
      base.path().to_path_buf(),
      game_dir.path().to_path_buf(),
      Some("../escape"),
    )
    .unwrap_err();

    assert_eq!(
      error.to_string(),
      "Profile error: invalid profile name \"../escape\""
    );
  }

  #[test]
  fn resolve_returns_a_target_for_a_profile_that_exists() {
    let base = tempdir().unwrap();
    let game_dir = tempdir().unwrap();

    thunderstore_engine::profile::layout::create(base.path(), GAME, "experiment").unwrap();

    let target = resolve(
      base.path().to_path_buf(),
      game_dir.path().to_path_buf(),
      Some("experiment"),
    )
    .unwrap();

    assert_eq!(target.profile.as_deref(), Some("experiment"));
    assert_eq!(
      target.dir,
      thunderstore_engine::profile::layout::profile_dir(base.path(), GAME, "experiment")
    );
  }

  #[test]
  fn an_ordinary_profile_name_is_offered_as_a_command_to_copy() {
    let message = missing_profile_error("experiment").to_string();

    assert!(
      message.contains("vmm profile create experiment"),
      "a name that needs no quoting must stay concrete; got: {message}"
    );
    assert!(message.contains("vmm profile list"), "got: {message}");
  }

  #[test]
  fn a_name_needing_quoting_is_never_offered_as_a_command_to_copy() {
    // `validate_name` accepts every one of these: it refuses only what could
    // escape the profiles directory. Pasted as written, the first becomes two
    // arguments, the second runs `rm`, and the third substitutes a command.
    for name in [
      "work profile",
      "main; rm -rf ~",
      "$(whoami)",
      "a`id`b",
      "-all",
    ] {
      let message = missing_profile_error(name).to_string();

      assert!(
        message.contains("vmm profile create <name>"),
        "{name:?} must degrade to the placeholder; got: {message}"
      );
      assert!(
        !message.contains(&format!("vmm profile create {name}")),
        "{name:?} must not appear in a copyable command; got: {message}"
      );
      // The headline still reports what was actually asked for, where `{:?}`
      // quotes it and nothing suggests running it.
      assert!(
        message.contains("there is no profile named"),
        "got: {message}"
      );
      assert!(message.contains("vmm profile list"), "got: {message}");
    }
  }
}
