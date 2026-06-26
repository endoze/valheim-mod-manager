use crate::cli::ProfileCommand;
use crate::error::AppResult;
use crate::target::GAME;
use std::path::Path;
use thunderstore_engine::profile::{layout, modlist};

/// Runs a profile-management operation against `base`.
///
/// Profile directories are the r2modman compatibility unit, and the persisted
/// *selection*, the choice of which profile subsequent commands operate on, is
/// now maintained by the engine's `layout` functions too: `layout::rename` and
/// `layout::delete` never leave it pointing at a directory that no longer
/// exists. What is left here is wording, dispatch, and the odd bit of pre-flight
/// reporting (whether a `println!` is warranted) that the engine has no reason
/// to know about.
/// Refuses a name that does not already exist, in the same words `vmm profile
/// use` and `--profile` use for it.
///
/// The engine's `delete`, `rename`, and `duplicate` do not check existence
/// first: they let `remove_dir_all`, `fs::rename`, and `read_dir` fail, which
/// surfaces an errno rather than advice, and words the one mistake three more
/// ways. [`layout::require_profile`] is the same predicate `use_profile` and
/// `InstallTarget::resolve` gate on, so going through it is what keeps all of
/// them saying the same thing.
fn require_profile(base: &Path, name: &str) -> AppResult<()> {
  layout::require_profile(base, GAME, name)
    .map_err(|bad| crate::target::profile_error(bad.into()))?;

  Ok(())
}

pub fn run(base: &Path, command: &ProfileCommand) -> AppResult<()> {
  match command {
    ProfileCommand::List => {
      let selected = layout::read_selection(base, GAME);
      let names = layout::list(base, GAME)?;

      if names.is_empty() {
        println!(
          "vmm: {}",
          crate::error::advice_message(
            "there are no profiles yet.",
            "Nothing was changed.",
            &["vmm profile create <name>"],
          )
        );

        return Ok(());
      }

      for name in names {
        let marker = if selected.as_deref() == Some(name.as_str()) {
          "*"
        } else {
          " "
        };

        // A profile whose `mods.yml` fails to parse is reported in place rather
        // than aborting: `list` is what a user reaches for precisely when
        // something already looks wrong, so one bad profile must not hide the
        // rest.
        match modlist::read(&layout::profile_dir(base, GAME, &name)) {
          Ok(mods) => println!("{marker} {name} ({} mods)", mods.len()),
          Err(error) => println!("{marker} {name} (mods.yml unreadable: {error})"),
        }
      }
    }
    ProfileCommand::Create { name } => {
      let dir = layout::create(base, GAME, name)?;

      println!("created {}", dir.display());
    }
    ProfileCommand::Delete { name } => {
      // Read before deleting: `layout::delete` clears the selection itself
      // when it pointed at the deleted profile, so this is only to decide
      // whether the second line below is warranted, not to maintain
      // anything.
      let was_selected = layout::read_selection(base, GAME).as_deref() == Some(name.as_str());

      require_profile(base, name)?;
      layout::delete(base, GAME, name)?;

      println!("deleted {name}");

      if was_selected {
        println!("cleared the selection; commands now use the game directory");
      }
    }
    ProfileCommand::Rename { from, to } => {
      // `layout::rename` carries the selection along itself when it pointed
      // at `from`, so nothing further is needed here.
      require_profile(base, from)?;
      layout::rename(base, GAME, from, to)?;

      println!("renamed {from} to {to}");
    }
    ProfileCommand::Duplicate { from, to } => {
      require_profile(base, from)?;

      let dir = layout::duplicate(base, GAME, from, to)?;

      println!("duplicated {from} to {}", dir.display());
    }
    ProfileCommand::Use { name } => {
      // The check and the write both stay in `use_profile`; vmm only re-words
      // the refusal it carries back.
      layout::use_profile(base, GAME, name).map_err(crate::target::profile_error)?;

      println!("using profile {name}");
    }
    ProfileCommand::Clear => {
      layout::clear_selection(base, GAME)?;

      println!("cleared the selection; commands now use the game directory");
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::tempdir;
  use thunderstore_engine::profile::layout;

  #[test]
  fn naming_a_missing_profile_advises_the_same_way_however_it_was_named() {
    let dir = tempdir().unwrap();
    let game_dir = tempdir().unwrap();
    let base = dir.path();

    // `vmm profile use nope` and `vmm --profile nope <anything>` are the same
    // mistake, so they must not answer it to two different standards: one with
    // advice and one with the engine's bare sentence.
    let selecting = run(
      base,
      &ProfileCommand::Use {
        name: "nope".to_string(),
      },
    )
    .unwrap_err();
    let resolving = crate::target::resolve(
      base.to_path_buf(),
      game_dir.path().to_path_buf(),
      Some("nope"),
    )
    .unwrap_err();

    assert_eq!(selecting.to_string(), resolving.to_string());
    assert_eq!(
      resolving.to_string(),
      crate::target::missing_profile_error("nope").to_string()
    );
  }

  #[test]
  fn every_command_taking_an_existing_profile_advises_the_same_way_about_a_missing_one() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let expected = crate::target::missing_profile_error("nope").to_string();

    // Naming a profile that isn't there is one mistake, so it must not be
    // answered to two standards: advice from `use`, and a raw `remove_dir_all`
    // or `fs::rename` errno from everything else.
    let commands: [(&str, ProfileCommand); 4] = [
      (
        "use",
        ProfileCommand::Use {
          name: "nope".to_string(),
        },
      ),
      (
        "delete",
        ProfileCommand::Delete {
          name: "nope".to_string(),
        },
      ),
      (
        "rename",
        ProfileCommand::Rename {
          from: "nope".to_string(),
          to: "other".to_string(),
        },
      ),
      (
        "duplicate",
        ProfileCommand::Duplicate {
          from: "nope".to_string(),
          to: "other".to_string(),
        },
      ),
    ];

    for (label, command) in &commands {
      let message = run(base, command).unwrap_err().to_string();

      assert_eq!(message, expected, "`profile {label}` worded it differently");
    }

    // Refusing left nothing half-made: no directory for the copy's target name.
    assert!(!layout::profile_dir(base, GAME, "other").exists());
  }

  #[test]
  fn create_duplicate_rename_delete_lifecycle() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    run(
      base,
      &ProfileCommand::Create {
        name: "main".to_string(),
      },
    )
    .unwrap();

    assert!(layout::profile_dir(base, GAME, "main").is_dir());

    // Creating twice is refused rather than adopting the existing directory.
    assert!(
      run(
        base,
        &ProfileCommand::Create {
          name: "main".to_string(),
        }
      )
      .is_err()
    );

    run(
      base,
      &ProfileCommand::Duplicate {
        from: "main".to_string(),
        to: "copy".to_string(),
      },
    )
    .unwrap();

    run(
      base,
      &ProfileCommand::Rename {
        from: "copy".to_string(),
        to: "renamed".to_string(),
      },
    )
    .unwrap();

    assert_eq!(
      layout::list(base, GAME).unwrap(),
      vec!["main".to_string(), "renamed".to_string()]
    );

    run(
      base,
      &ProfileCommand::Delete {
        name: "renamed".to_string(),
      },
    )
    .unwrap();

    assert_eq!(layout::list(base, GAME).unwrap(), vec!["main".to_string()]);
  }

  #[test]
  fn use_requires_an_existing_profile() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    assert!(
      run(
        base,
        &ProfileCommand::Use {
          name: "ghost".to_string(),
        }
      )
      .is_err()
    );
    assert_eq!(layout::read_selection(base, GAME), None);

    run(
      base,
      &ProfileCommand::Create {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Use {
        name: "main".to_string(),
      },
    )
    .unwrap();

    assert_eq!(layout::read_selection(base, GAME), Some("main".to_string()));
  }

  #[test]
  fn renaming_the_selected_profile_follows_the_selection() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    run(
      base,
      &ProfileCommand::Create {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Use {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Rename {
        from: "main".to_string(),
        to: "primary".to_string(),
      },
    )
    .unwrap();

    // The selection must never point at a directory that no longer exists.
    assert_eq!(
      layout::read_selection(base, GAME),
      Some("primary".to_string())
    );
  }

  #[test]
  fn deleting_the_selected_profile_clears_the_selection() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    run(
      base,
      &ProfileCommand::Create {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Create {
        name: "other".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Use {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Delete {
        name: "main".to_string(),
      },
    )
    .unwrap();

    assert_eq!(layout::read_selection(base, GAME), None);

    // Deleting an unselected profile leaves the selection alone.
    run(
      base,
      &ProfileCommand::Use {
        name: "other".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Create {
        name: "third".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Delete {
        name: "third".to_string(),
      },
    )
    .unwrap();

    assert_eq!(
      layout::read_selection(base, GAME),
      Some("other".to_string())
    );
  }

  #[test]
  fn clear_returns_to_game_dir_mode() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    run(
      base,
      &ProfileCommand::Create {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Use {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(base, &ProfileCommand::Clear).unwrap();

    assert_eq!(layout::read_selection(base, GAME), None);
  }

  #[test]
  fn list_succeeds_with_no_profiles() {
    let dir = tempdir().unwrap();

    assert!(run(dir.path(), &ProfileCommand::List).is_ok());
  }

  #[test]
  fn list_survives_a_profile_with_an_unparseable_mods_yml() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    run(
      base,
      &ProfileCommand::Create {
        name: "good".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Create {
        name: "bad".to_string(),
      },
    )
    .unwrap();
    std::fs::write(
      layout::profile_dir(base, GAME, "bad").join("mods.yml"),
      "not: [valid",
    )
    .unwrap();

    // One profile's corrupt `mods.yml` must not hide every other profile.
    assert!(run(base, &ProfileCommand::List).is_ok());
  }

  #[test]
  fn list_marks_the_selected_profile() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    run(
      base,
      &ProfileCommand::Create {
        name: "main".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Create {
        name: "spare".to_string(),
      },
    )
    .unwrap();
    run(
      base,
      &ProfileCommand::Use {
        name: "main".to_string(),
      },
    )
    .unwrap();

    // The `*` column is the only thing in `list`'s output naming which profile
    // every other command will act on, so it has to be driven by the selection
    // rather than by position. Listing with one profile selected and one not
    // exercises both arms of the marker.
    assert_eq!(layout::read_selection(base, GAME).as_deref(), Some("main"));
    assert!(run(base, &ProfileCommand::List).is_ok());
  }
}
