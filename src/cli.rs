use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct AppCli {
  #[command(subcommand)]
  pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
  /// Update the mod manifest or installed mods
  Update(CommandArgs),
  /// Search for mods by name
  Search(SearchArgs),
}

#[derive(Args)]
pub struct CommandArgs {
  #[command(subcommand)]
  pub command: UpdatesCommand,
}

#[derive(Args)]
pub struct SearchArgs {
  /// Search term to find mods by name
  pub term: String,
}

#[derive(Subcommand)]
pub enum UpdatesCommand {
  /// Update the mod manifest from the server
  Manifest,
  /// Update installed mods to their latest versions
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
}
