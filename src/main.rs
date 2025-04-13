mod api;
mod cli;
mod config;
mod error;
mod logs;
mod manifest;
mod package;
mod zip;

use crate::{cli::AppCli, error::AppResult};
use clap::Parser;
use cli::{Command, UpdatesCommand};
use config::APP_CONFIG;
use package::DependencyGraph;

/// The main entry point for the Valheim Mod Manager application.
///
/// This function initializes the logging system, fetches the manifest of available
/// mods, and downloads a predefined set of mods and their dependencies.
///
/// # Returns
///
/// `Ok(())` if the program runs successfully, or an error if something fails.
#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> AppResult<()> {
  logs::setup_logging();
  let app = AppCli::parse();
  tracing::info!("Starting valheim mod manager");

  match &app.command {
    Command::Update(subcmd) => match subcmd.command {
      UpdatesCommand::Manifest => {
        tracing::info!("Checking for manifest updates");
        let _ = api::get_manifest(&APP_CONFIG.cache_dir, None).await?;
      }
      UpdatesCommand::Mods => {
        let manifest = api::get_manifest(&APP_CONFIG.cache_dir, None).await?;
        let packages = APP_CONFIG.mod_list.clone();

        tracing::info!("Building dependency graph for mods");

        let dg = DependencyGraph::new(packages);
        let urls = dg.resolve(&manifest);

        tracing::info!("Done building dependency graph, proceeding to download mods if necessary");

        api::download_files(urls, &APP_CONFIG.cache_dir).await?;

        zip::unzip_downloaded_mods(&APP_CONFIG.cache_dir)?;
      }
    },
    Command::Search(search_args) => {
      let manifest = api::get_manifest(&APP_CONFIG.cache_dir, None).await?;
      let search_term = search_args.term.to_lowercase();

      let search_results: Vec<_> = manifest
        .iter()
        .filter(|(_, package)| {
          if let Some(name) = &package.name {
            if name.to_lowercase().contains(&search_term) {
              return true;
            }
          }

          if let Some(full_name) = &package.full_name {
            if full_name.to_lowercase().contains(&search_term) {
              return true;
            }
          }

          false
        })
        .collect();

      if search_results.is_empty() {
        println!("No mods found matching '{}'", search_args.term);
      } else {
        println!(
          "Found {} mods matching '{}':\n",
          search_results.len(),
          search_args.term
        );

        for (full_name, package) in search_results {
          let version = package.latest_version().map_or("Unknown".to_string(), |v| {
            v.version_number
              .clone()
              .unwrap_or_else(|| "Unknown".to_string())
          });

          let description = package
            .latest_version()
            .and_then(|v| v.description.clone())
            .unwrap_or_default();
          let name = package.name.as_ref().unwrap_or(full_name);
          let unknown = "Unknown".to_string();
          let owner = package.owner.as_ref().unwrap_or(&unknown);

          println!("{}-{} ({})", owner, name, version);
          if !description.is_empty() {
            println!("  {}", description);
          }
          println!();
        }
      }
    }
  }

  Ok(())
}
