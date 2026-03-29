mod api;
mod cli;
mod commands;
mod config;
mod error;
mod intern;
mod logs;
mod manifest;
mod package;
mod zip;

use crate::{cli::AppCli, error::AppResult};
use clap::Parser;
use cli::{Command, UpdatesCommand};
use config::{APP_CACHE_DIR, get_config};

#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> AppResult<()> {
  let app = AppCli::parse();
  let config = get_config(app.config.as_deref())
    .unwrap_or_else(|err| panic!("An error has occurred getting the config: '{err}'"));

  logs::setup_logging(&config.log_level);
  tracing::info!("Starting valheim mod manager");

  let cache_dir: &str = &APP_CACHE_DIR;

  match &app.command {
    Command::Update(subcmd) => match subcmd.command {
      UpdatesCommand::Manifest => commands::update::run_manifest(cache_dir, None).await?,
      UpdatesCommand::Mods => {
        commands::update::run_mods(
          cache_dir,
          config.mod_list.clone(),
          config.install_dir.as_deref(),
          None,
        )
        .await?
      }
    },
    Command::List(list_args) => {
      commands::list::run(cache_dir, config.mod_list.clone(), &list_args.format, None).await?
    }
    Command::Search(search_args) => {
      commands::search::run(cache_dir, &search_args.term, None).await?
    }
  }

  Ok(())
}
