use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;
use thunderstore_engine::progress::{ProgressHandle, ProgressReporter};

/// A [`ProgressReporter`] that renders progress with `indicatif`.
///
/// This lives in the binary so the engine stays UI-agnostic. A call without a
/// known total gets an indeterminate spinner, as the manifest fetch does; one
/// with a total gets a `#>-` byte bar.
pub struct IndicatifProgress {
  multi: MultiProgress,
}

impl IndicatifProgress {
  /// Creates a new reporter with its own multi-progress display.
  pub fn new() -> Self {
    Self {
      multi: MultiProgress::new(),
    }
  }
}

impl Default for IndicatifProgress {
  fn default() -> Self {
    Self::new()
  }
}

impl ProgressReporter for IndicatifProgress {
  fn begin(&self, label: &str, total: Option<u64>) -> Box<dyn ProgressHandle> {
    let bar = match total {
      None => {
        // Every template must name `{msg}`: `begin` and `finish` both carry
        // their text as the bar's message, so a template without it renders a
        // labelless bar and swallows the completion line entirely.
        let style = ProgressStyle::with_template("{spinner} {msg}")
          .unwrap()
          .tick_strings(&["-", "\\", "|", "/", ""]);

        let pb = self.multi.add(ProgressBar::new_spinner());
        pb.set_style(style);
        pb.enable_steady_tick(Duration::from_millis(130));

        pb
      }
      Some(total) => {
        let style = ProgressStyle::with_template("{msg} [{bar:40}] {bytes}/{total_bytes}")
          .unwrap()
          .progress_chars("#>-");

        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(style);

        pb
      }
    };

    bar.set_message(label.to_string());

    Box::new(IndicatifHandle { bar })
  }
}

/// A single `indicatif` progress bar handle.
struct IndicatifHandle {
  bar: ProgressBar,
}

impl ProgressHandle for IndicatifHandle {
  fn inc(&self, delta: u64) {
    self.bar.inc(delta);
  }

  fn set_message(&self, msg: &str) {
    self.bar.set_message(msg.to_string());
  }

  fn finish(&self, msg: Option<&str>) {
    match msg {
      Some(m) => self.bar.finish_with_message(m.to_string()),
      None => self.bar.finish(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn both_styles_are_valid_templates() {
    let reporter = IndicatifProgress::new();

    // `begin` unwraps `with_template`, so a malformed template panics on first
    // use rather than at compile time. Exercising both branches here is what
    // stands between a typo and a panic mid-download.
    let spinner = reporter.begin("fetching", None);
    let bar = reporter.begin("downloading", Some(1024));

    spinner.finish(Some("fetched"));
    bar.inc(1024);
    bar.finish(None);
  }

  #[test]
  fn the_default_reporter_reports() {
    // `Default` is what a caller reaches for to build a reporter without naming
    // `new`; it has to hand back a working display rather than merely compile.
    let reporter = IndicatifProgress::default();
    let bar = reporter.begin("downloading", Some(1024));

    // `set_message` is the engine's way of retitling a bar mid-transfer, so it
    // runs far more often than `begin` does and is worth exercising at least
    // once: an `indicatif` handle that has already finished panics on a later
    // call, and this is the ordering the engine actually uses.
    bar.set_message("downloading ModA");
    bar.inc(512);
    bar.finish(Some("downloaded"));
  }
}
