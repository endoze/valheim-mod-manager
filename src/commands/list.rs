use crate::cli::ListFormat;
use crate::error::AppResult;
use crate::target::Target;
use thunderstore_engine::profile::modlist::ProfileMod;

/// Renders the installed mods, sorted by identifier.
///
/// Pure so the output can be asserted without capturing stdout.
pub fn render(mods: &[ProfileMod], format: &ListFormat) -> AppResult<String> {
  let mut sorted: Vec<&ProfileMod> = mods.iter().collect();

  sorted.sort_by(|a, b| a.name.cmp(&b.name));

  let rendered = match format {
    ListFormat::Text => {
      let mut out = String::new();

      for entry in sorted {
        let state = if entry.enabled { "" } else { " (disabled)" };

        out.push_str(&format!(
          "{} {}{}\n",
          entry.name, entry.version_number, state
        ));
      }

      out
    }
    ListFormat::Json => {
      let entries: Vec<serde_json::Value> = sorted
        .iter()
        .map(|entry| {
          serde_json::json!({
            "full_name": entry.name,
            "version": entry.version_number.to_string(),
            "enabled": entry.enabled,
          })
        })
        .collect();

      serde_json::to_string_pretty(&entries)?
    }
  };

  Ok(rendered)
}

/// Prints the mods recorded in the target's `mods.yml`.
pub fn run(target: &Target, format: &ListFormat) -> AppResult<()> {
  let mods = super::read_modlist(target)?;

  print!("{}", render(&mods, format)?);

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use thunderstore_engine::ecosystem::Ecosystem;
  use thunderstore_engine::profile::modlist;
  use tokio::runtime::Runtime;

  /// Installs two mods and disables one, returning the recorded list.
  fn installed_mods(fixture: &Fixture, target: &crate::target::Target) -> Vec<modlist::ProfileMod> {
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        target,
        &["Owner-ModB".to_string(), "Owner-ModA".to_string()],
      ))
      .unwrap();

    thunderstore_engine::profile::set_enabled_in(
      &target.dir,
      &eco,
      crate::target::GAME,
      "Owner-ModB",
      false,
    )
    .unwrap();

    modlist::read(&target.dir).unwrap()
  }

  #[test]
  fn text_output_is_sorted_and_marks_disabled_mods() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let mods = installed_mods(&fixture, &target);

    let rendered = render(&mods, &ListFormat::Text).unwrap();

    assert_eq!(rendered, "Owner-ModA 1.0.0\nOwner-ModB 1.0.0 (disabled)\n");
  }

  #[test]
  fn json_output_carries_version_and_enabled() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let mods = installed_mods(&fixture, &target);

    let rendered = render(&mods, &ListFormat::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&rendered).unwrap();

    assert_eq!(parsed[0]["full_name"], "Owner-ModA");
    assert_eq!(parsed[0]["version"], "1.0.0");
    assert_eq!(parsed[0]["enabled"], true);
    assert_eq!(parsed[1]["full_name"], "Owner-ModB");
    assert_eq!(parsed[1]["enabled"], false);
  }

  #[test]
  fn nothing_installed_renders_empty() {
    assert_eq!(render(&[], &ListFormat::Text).unwrap(), "");

    let json = render(&[], &ListFormat::Json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.as_array().unwrap().len(), 0);
  }

  #[test]
  fn run_reads_the_target_and_succeeds() {
    let fixture = Fixture::new();
    let target = fixture.target();

    installed_mods(&fixture, &target);

    assert!(run(&target, &ListFormat::Text).is_ok());
  }

  #[test]
  fn a_profile_lists_its_own_mods_not_the_game_dirs() {
    let fixture = Fixture::new();
    let profile = fixture.profile_target("experiment");
    let eco = Ecosystem::bundled();

    // An unparseable `mods.yml` in the game directory: `run` reading the wrong
    // target would surface that parse error instead of the profile's list, which
    // is what makes the `is_ok()` below a real check rather than a smoke test.
    std::fs::write(fixture.game_dir.path().join("mods.yml"), "not: [valid").unwrap();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &profile,
        &["Owner-ModB".to_string()],
      ))
      .unwrap();

    let rendered = render(&modlist::read(&profile.dir).unwrap(), &ListFormat::Text).unwrap();

    assert_eq!(rendered, "Owner-ModB 1.0.0\n");
    assert!(modlist::read(fixture.game_dir.path()).is_err());
    assert!(run(&profile, &ListFormat::Text).is_ok());
    // The profile's install stayed out of the game directory.
    assert!(
      !fixture
        .game_dir
        .path()
        .join("BepInEx/plugins/Owner-ModB")
        .exists()
    );
  }
}
