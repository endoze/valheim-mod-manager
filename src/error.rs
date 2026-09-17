use std::io;
use thiserror::Error;

/// A type alias for `Result<T, AppError>`.
pub type AppResult<T> = Result<T, AppError>;

/// Represents all possible errors that can occur in the application.
#[derive(Error, Debug)]
pub enum AppError {
  /// Error from the IO system.
  #[error("IO error: {0}")]
  Io(#[from] io::Error),

  /// Error from JSON serialization (used when rendering list output).
  #[error("JSON error: {0}")]
  Json(#[from] serde_json::Error),

  /// Error originating in the Thunderstore engine.
  #[error(transparent)]
  Thunderstore(#[from] thunderstore_engine::Error),

  /// Generic application error.
  #[error("{0}")]
  Other(String),

  /// Error when serializing a config fails.
  #[error("Config serialization error: {0}")]
  ConfigSerialization(String),
}

/// Renders a user-facing message in the shape every `vmm` message uses: a
/// one-line headline saying what happened, a paragraph saying why, then the
/// commands that resolve it, indented four spaces so they can be copied
/// straight out of the terminal.
///
/// An empty `detail` or `next` is omitted rather than emitted as trailing blank
/// lines, so a bare headline renders as exactly that one line. The `vmm: `
/// prefix is deliberately absent: `main` applies it once to whatever it prints,
/// so a message embedding its own prefix would double it.
pub fn advice_message(headline: &str, detail: &str, next: &[&str]) -> String {
  let mut rendered = String::from(headline);

  if !detail.is_empty() {
    rendered.push_str("\n\n");
    rendered.push_str(detail);
  }

  if !next.is_empty() {
    rendered.push('\n');

    for command in next {
      rendered.push_str("\n    ");
      rendered.push_str(command);
    }
  }

  rendered
}

impl AppError {
  /// An error carrying an [`advice_message`].
  ///
  /// Every user-facing refusal is built through this rather than
  /// [`AppError::Other`] directly, so the whole CLI reads the same way.
  pub fn advice(headline: impl Into<String>, detail: impl Into<String>, next: &[&str]) -> Self {
    AppError::Other(advice_message(&headline.into(), &detail.into(), next))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn advice_renders_headline_detail_and_indented_commands() {
    let message = advice_message(
      "there's nothing to migrate.",
      "Your config has no `mod_list` key.",
      &["vmm install <Owner-ModName>"],
    );

    assert_eq!(
      message,
      "there's nothing to migrate.\n\nYour config has no `mod_list` key.\n\n    vmm install <Owner-ModName>"
    );
  }

  #[test]
  fn advice_omits_empty_sections_without_leaving_blank_lines() {
    assert_eq!(
      advice_message("just a headline.", "", &[]),
      "just a headline."
    );
    assert_eq!(
      advice_message("headline.", "", &["vmm list"]),
      "headline.\n\n    vmm list"
    );
    assert_eq!(
      advice_message("headline.", "detail.", &[]),
      "headline.\n\ndetail."
    );
  }

  #[test]
  fn advice_indents_every_command_under_one_blank_line() {
    assert_eq!(
      advice_message("headline.", "detail.", &["vmm list", "vmm migrate"]),
      "headline.\n\ndetail.\n\n    vmm list\n    vmm migrate"
    );
  }

  #[test]
  fn app_error_advice_displays_the_rendered_message() {
    let error = AppError::advice("headline.", "detail.", &["vmm list"]);

    assert_eq!(error.to_string(), "headline.\n\ndetail.\n\n    vmm list");
  }
}
