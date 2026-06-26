use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Root command-line interface structure for the application.
///
/// This struct uses clap's derive macros to parse command-line arguments
/// and route to the appropriate subcommand handler.
#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct AppCli {
  /// Path to a config file, overriding the default XDG location.
  #[arg(long, global = true)]
  pub config: Option<PathBuf>,
  /// Operate on this named profile instead of the game directory.
  #[arg(long, global = true, conflicts_with = "no_profile")]
  pub profile: Option<String>,
  /// Ignore any persisted profile selection and operate on the game directory.
  #[arg(long, global = true)]
  pub no_profile: bool,
  /// The subcommand to execute.
  #[command(subcommand)]
  pub command: Command,
}

/// Top-level commands available to the user.
#[derive(Subcommand)]
pub enum Command {
  /// Update the mod manifest or installed mods.
  Update(CommandArgs),
  /// Search for mods by name.
  Search(SearchArgs),
  /// List installed mods and their versions.
  List(ListArgs),
  /// Install one or more mods and their dependencies.
  Install(ModsArgs),
  /// Uninstall one or more mods.
  Uninstall(UninstallArgs),
  /// Enable an installed mod, or every installed mod with `--all`.
  Enable(ToggleArgs),
  /// Disable an installed mod without uninstalling it, or every installed mod
  /// with `--all`.
  Disable(ToggleArgs),
  /// Adopt the deprecated `mod_list` config key as this target's mods.yml.
  Migrate,
  /// Manage named r2modman-compatible profiles.
  Profile(ProfileArgs),
  /// Launch the game with this target's mods applied.
  Launch(LaunchArgs),
  /// Export this target's mods and config for sharing.
  Export(ExportArgs),
  /// Import mods from an export file, a profile directory, or a profile code.
  Import(ImportArgs),
}

/// Arguments for commands taking one or more mod identifiers.
#[derive(Args)]
pub struct ModsArgs {
  /// One or more `Owner-ModName` identifiers.
  #[arg(required = true)]
  pub mods: Vec<String>,
}

/// Arguments for the uninstall command.
#[derive(Args)]
pub struct UninstallArgs {
  /// One or more `Owner-ModName` identifiers.
  #[arg(required_unless_present = "all")]
  pub mods: Vec<String>,
  /// Uninstall every mod recorded in this target's mods.yml, including the mod
  /// loader, leaving a vanilla install.
  #[arg(long, conflicts_with = "mods")]
  pub all: bool,
  /// Skip the confirmation prompt that `--all` shows.
  // "Requires --all" is expressed here as `conflicts_with = "mods"`, not as
  // `requires = "all"`. clap's own requires-validation excuses a missing
  // "required" argument whenever that argument itself conflicts with
  // something already present: since `all` is `conflicts_with = "mods"`,
  // `#[arg(requires = "all")]` here would let `vmm uninstall --yes Owner-Mod`
  // parse cleanly ("well, demanding `--all` would conflict with the `mods`
  // you already gave, so the missing `--all` is fine"), silently ignoring
  // `--yes`. `mods` is required unless `all` is present, so the only way
  // `yes` and `mods` can coexist is if `yes` is meaningless (`all` absent);
  // conflicting with `mods` directly rejects exactly that combination instead
  // of excusing it. A plain comment, not a doc comment, so this rationale
  // does not leak into `--help` output.
  #[arg(long, conflicts_with = "mods")]
  pub yes: bool,
  /// Uninstall even when mod folders that mods.yml does not record are present
  /// under the install routes. Those folders are deleted along with the named
  /// mods, since an uninstall reconciles the whole namespaced tree against
  /// mods.yml.
  #[arg(long)]
  pub force: bool,
}

/// Arguments for the enable and disable commands.
#[derive(Args)]
pub struct ToggleArgs {
  /// An `Owner-ModName` identifier.
  #[arg(required_unless_present = "all")]
  pub name: Option<String>,
  /// Apply to every mod recorded in this target's mods.yml. The mod loader is
  /// left alone, since it has no disabled state.
  #[arg(long, conflicts_with = "name")]
  pub all: bool,
}

/// Arguments for the list command.
#[derive(Args)]
pub struct ListArgs {
  /// Output format.
  #[arg(long, value_enum, default_value_t = ListFormat::Text)]
  pub format: ListFormat,
}

/// Output format for the list command.
#[derive(Clone, ValueEnum)]
pub enum ListFormat {
  /// Plain text, one mod per line.
  Text,
  /// JSON array.
  Json,
}

/// Arguments for the update command.
#[derive(Args)]
pub struct CommandArgs {
  /// The specific update operation to perform.
  #[command(subcommand)]
  pub command: UpdatesCommand,
}

/// Arguments for the search command.
#[derive(Args)]
pub struct SearchArgs {
  /// Search term to find mods by name.
  pub term: String,
}

/// Subcommands for the update operation.
#[derive(Subcommand)]
pub enum UpdatesCommand {
  /// Update the mod manifest from the server.
  Manifest,
  /// Update installed mods to their latest versions.
  Mods,
}

/// Arguments for the profile command.
#[derive(Args)]
pub struct ProfileArgs {
  /// The profile operation to perform.
  #[command(subcommand)]
  pub command: ProfileCommand,
}

/// Operations on named r2modman-compatible profiles.
#[derive(Subcommand)]
pub enum ProfileCommand {
  /// List profiles, marking the selected one.
  List,
  /// Create a new, empty profile.
  Create {
    /// The profile name.
    name: String,
  },
  /// Delete a profile and everything in it.
  Delete {
    /// The profile name.
    name: String,
  },
  /// Rename a profile.
  Rename {
    /// The existing profile name.
    from: String,
    /// The new profile name.
    to: String,
  },
  /// Copy a profile under a new name.
  Duplicate {
    /// The profile to copy.
    from: String,
    /// The name of the copy.
    to: String,
  },
  /// Select a profile for subsequent commands.
  Use {
    /// The profile name.
    name: String,
  },
  /// Clear the selection, returning to the game directory.
  Clear,
}

/// Arguments for the launch command.
#[derive(Args)]
pub struct LaunchArgs {
  /// Launch without mods.
  #[arg(long)]
  pub vanilla: bool,
  /// Print the string to paste into Steam's launch options, without launching.
  #[arg(long)]
  pub print_steam_options: bool,
  /// Extra arguments passed to the game, after `--`.
  #[arg(last = true)]
  pub args: Vec<String>,
}

/// Arguments for the export command.
#[derive(Args)]
pub struct ExportArgs {
  /// Upload the export and print a shareable Thunderstore profile code.
  #[arg(long)]
  pub code: bool,
}

/// Arguments for the import command.
#[derive(Args)]
pub struct ImportArgs {
  /// An `.r2z` file, an r2modman profile directory, or a profile code. Note: a
  /// directory import copies over the destination without clearing it first, so
  /// mods already there under a different set can be left behind untracked.
  pub source: String,
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::CommandFactory;

  #[test]
  fn test_command_search() {
    let app = AppCli::command();
    let search_command = app.find_subcommand("search").unwrap();

    assert_eq!(search_command.get_name(), "search");
    assert!(search_command.get_about().is_some());
    assert!(
      search_command
        .get_about()
        .unwrap()
        .to_string()
        .contains("Search for mods by name")
    );

    let search_args = search_command.get_arguments().collect::<Vec<_>>();
    assert_eq!(search_args.len(), 1);
    assert_eq!(search_args[0].get_id().as_str(), "term");
    assert!(search_args[0].get_help().is_some());
    assert!(
      search_args[0]
        .get_help()
        .unwrap()
        .to_string()
        .contains("Search term to find mods by name")
    );
  }

  #[test]
  fn test_command_update() {
    let app = AppCli::command();
    let update_command = app.find_subcommand("update").unwrap();

    assert_eq!(update_command.get_name(), "update");
    assert!(update_command.get_about().is_some());
    assert!(
      update_command
        .get_about()
        .unwrap()
        .to_string()
        .contains("Update the mod manifest or installed mods")
    );

    let update_subcommands = update_command.get_subcommands().collect::<Vec<_>>();
    assert_eq!(update_subcommands.len(), 2);

    let manifest_cmd = update_subcommands
      .iter()
      .find(|cmd| cmd.get_name() == "manifest")
      .unwrap();
    let mods_cmd = update_subcommands
      .iter()
      .find(|cmd| cmd.get_name() == "mods")
      .unwrap();

    assert!(manifest_cmd.get_about().is_some());
    assert!(
      manifest_cmd
        .get_about()
        .unwrap()
        .to_string()
        .contains("Update the mod manifest from the server")
    );
    assert!(mods_cmd.get_about().is_some());
    assert!(
      mods_cmd
        .get_about()
        .unwrap()
        .to_string()
        .contains("Update installed mods to their latest versions")
    );
  }

  #[test]
  fn profile_flags_are_global_and_mutually_exclusive() {
    let app = AppCli::command();

    let profile = app
      .get_arguments()
      .find(|a| a.get_id().as_str() == "profile")
      .expect("--profile should be a global argument");

    assert!(profile.is_global_set());

    let no_profile = app
      .get_arguments()
      .find(|a| a.get_id().as_str() == "no_profile")
      .expect("--no-profile should be a global argument");

    assert!(no_profile.is_global_set());

    // Both at once is rejected rather than one silently winning.
    let conflict = AppCli::try_parse_from([
      "vmm",
      "--profile",
      "experiment",
      "--no-profile",
      "search",
      "jotunn",
    ]);

    assert!(conflict.is_err());
  }

  #[test]
  fn uninstall_takes_mods_and_an_opt_in_force_flag() {
    let app = AppCli::command();
    let uninstall = app.find_subcommand("uninstall").unwrap();

    let force = uninstall
      .get_arguments()
      .find(|arg| arg.get_id().as_str() == "force")
      .expect("uninstall should offer --force");

    assert!(force.get_help().is_some());
    // Opt-in: absent means the untracked-folder pre-flight check applies.
    let parsed = AppCli::try_parse_from(["vmm", "uninstall", "Owner-Mod"]).unwrap();

    match parsed.command {
      Command::Uninstall(args) => {
        assert_eq!(args.mods, vec!["Owner-Mod".to_string()]);
        assert!(!args.force);
      }
      _ => panic!("expected the uninstall subcommand"),
    }

    let forced = AppCli::try_parse_from(["vmm", "uninstall", "--force", "Owner-Mod"]).unwrap();

    match forced.command {
      Command::Uninstall(args) => assert!(args.force),
      _ => panic!("expected the uninstall subcommand"),
    }
  }

  #[test]
  fn uninstall_takes_either_mods_or_all_and_an_opt_in_yes() {
    let all = AppCli::try_parse_from(["vmm", "uninstall", "--all", "--yes"]).unwrap();

    match all.command {
      Command::Uninstall(args) => {
        assert!(args.all);
        assert!(args.yes);
        assert!(args.mods.is_empty());
      }
      _ => panic!("expected the uninstall subcommand"),
    }

    // --yes is opt-in: absent means the confirmation prompt applies.
    let prompted = AppCli::try_parse_from(["vmm", "uninstall", "--all"]).unwrap();

    match prompted.command {
      Command::Uninstall(args) => assert!(!args.yes),
      _ => panic!("expected the uninstall subcommand"),
    }

    // Both at once is rejected rather than one silently winning.
    assert!(AppCli::try_parse_from(["vmm", "uninstall", "--all", "Owner-Mod"]).is_err());
    // Neither is rejected: an argument-less uninstall has no meaning.
    assert!(AppCli::try_parse_from(["vmm", "uninstall"]).is_err());
    // `--yes` means nothing without `--all`, so it is rejected rather than
    // silently ignored.
    assert!(AppCli::try_parse_from(["vmm", "uninstall", "--yes", "Owner-Mod"]).is_err());
  }

  #[test]
  fn test_command_list() {
    let app = AppCli::command();
    let list_command = app.find_subcommand("list").unwrap();

    assert_eq!(list_command.get_name(), "list");
    assert!(list_command.get_about().is_some());
    assert!(
      list_command
        .get_about()
        .unwrap()
        .to_string()
        .contains("List installed mods")
    );

    let list_args = list_command.get_arguments().collect::<Vec<_>>();
    let format_arg = list_args
      .iter()
      .find(|a| a.get_id().as_str() == "format")
      .unwrap();
    assert!(format_arg.get_default_values().iter().any(|v| v == "text"));
  }

  #[test]
  fn toggle_takes_either_a_mod_name_or_all_but_not_both() {
    // A bare name still works.
    let named = AppCli::try_parse_from(["vmm", "disable", "Owner-Mod"]).unwrap();

    match named.command {
      Command::Disable(args) => {
        assert_eq!(args.name.as_deref(), Some("Owner-Mod"));
        assert!(!args.all);
      }
      _ => panic!("expected the disable subcommand"),
    }

    let all = AppCli::try_parse_from(["vmm", "enable", "--all"]).unwrap();

    match all.command {
      Command::Enable(args) => {
        assert!(args.all);
        assert!(args.name.is_none());
      }
      _ => panic!("expected the enable subcommand"),
    }

    // Both at once is rejected rather than one silently winning.
    assert!(AppCli::try_parse_from(["vmm", "enable", "--all", "Owner-Mod"]).is_err());
    // Neither is rejected too: an argument-less toggle has no meaning.
    assert!(AppCli::try_parse_from(["vmm", "enable"]).is_err());
  }
}
