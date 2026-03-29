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
  /// List all mods from config including resolved dependencies.
  List(ListArgs),
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
        .contains("List all mods from config including resolved dependencies")
    );

    let list_args = list_command.get_arguments().collect::<Vec<_>>();
    let format_arg = list_args
      .iter()
      .find(|a| a.get_id().as_str() == "format")
      .unwrap();
    assert!(format_arg.get_default_values().iter().any(|v| v == "text"));
  }
}
