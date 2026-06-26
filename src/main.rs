mod cli;
mod commands;
mod config;
mod error;
mod logs;
mod progress;
mod target;
#[cfg(test)]
mod test_support;

use crate::{
  cli::AppCli,
  error::{AppError, AppResult},
  progress::IndicatifProgress,
  target::Target,
};
use clap::Parser;
use cli::{Command, UpdatesCommand};
use config::{GameDirStatus, get_config};
use std::cell::LazyCell;
use std::sync::Arc;
use thunderstore_engine::client::ThunderstoreClient;
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile::layout;

/// Thunderstore's canonical host.
///
/// This must be the bare host, not a per-community subdomain: the client
/// composes the index URL as `{BASE_URL}/c/{COMMUNITY}/api/v1/package/`, but the
/// profile share endpoints (`/api/experimental/legacyprofile/...`) carry no
/// community segment at all. A subdomain like `valheim.thunderstore.io` supplies
/// its own community on redirect, which would silently point the profile
/// endpoints at the wrong community, and the 302 would drop the POST body
/// besides.
const BASE_URL: &str = "https://thunderstore.io";
/// The Valheim community slug on Thunderstore.
const COMMUNITY: &str = "valheim";

/// Resolves the game directory for installs, or prints actionable guidance and
/// exits when it cannot proceed.
///
/// The message is written to stderr unconditionally (not via the log filter) so
/// it is always seen, including by users on the default `log_level = "error"`.
fn require_game_dir(config: &config::AppConfig) -> &str {
  match config.game_dir_status() {
    GameDirStatus::Set(dir) => dir,
    GameDirStatus::NeedsMigration => {
      report_and_exit(needs_migration_error(config.install_dir.as_deref()))
    }
    GameDirStatus::Unset => report_and_exit(unset_game_dir_error()),
  }
}

/// The refusal [`require_game_dir`] exits with when a config predating
/// `game_dir` is still carrying the deprecated `install_dir`.
///
/// Takes the old `install_dir` rather than the whole config because it is the
/// only thing the wording turns on: a value [`config::suggest_game_dir`] can
/// read a game root out of earns the exact line to paste, and anything else
/// falls back to the generic example.
///
/// Pulled out as a pure function so both wordings are testable, the same way
/// [`missing_game_dir_error`] is. [`require_game_dir`] ends in
/// [`report_and_exit`], which never returns, so this is the only way the
/// message every upgrading user meets can be inspected at all.
fn needs_migration_error(install_dir: Option<&str>) -> AppError {
  let suggested = install_dir.and_then(config::suggest_game_dir);

  let detail = match &suggested {
    Some(dir) => format!(
      "`install_dir` is no longer used. Its meaning changed: vmm now installs into\n\
       your game root using the loader's install rules (an r2modman-compatible\n\
       layout) and manages BepInEx itself.\n\n\
       Based on your old `install_dir`, your game root is probably:\n\n\
       \x20   game_dir = \"{dir}\"\n\n\
       Then remove the now-unused `install_dir`. Because the on-disk layout changed,\n\
       start from a clean BepInEx setup (delete the existing `BepInEx/plugins`\n\
       contents, or the whole `BepInEx` folder) so stale files from the old layout\n\
       don't linger, then re-run `vmm update mods`.\n\n\
       Nothing was changed."
    ),
    None => format!(
      "`install_dir` is no longer used. Its meaning changed: vmm now installs into\n\
       your game root using the loader's install rules (an r2modman-compatible\n\
       layout) and manages BepInEx itself.\n\n\
       Set `game_dir` to your Valheim game folder (the directory containing the\n\
       game executable, where BepInEx lives), for example:\n\n\
       \x20   game_dir = \"{example}\"\n\n\
       Then remove the now-unused `install_dir`. Because the on-disk layout changed,\n\
       start from a clean BepInEx setup (delete the existing `BepInEx/plugins`\n\
       contents, or the whole `BepInEx` folder) so stale files from the old layout\n\
       don't linger, then re-run `vmm update mods`.\n\n\
       Nothing was changed.",
      example = config::example_game_dir(),
    ),
  };

  AppError::advice(
    "your configuration needs migrating before mods can be installed.",
    detail,
    &[],
  )
}

/// The refusal [`require_game_dir`] exits with when neither `game_dir` nor the
/// deprecated `install_dir` is set, which is a fresh config rather than an
/// upgraded one. Pulled out for the same reason as [`needs_migration_error`].
fn unset_game_dir_error() -> AppError {
  AppError::advice(
    "`game_dir` is not set, so there is nowhere to install mods.",
    format!(
      "Set `game_dir` to your Valheim game folder, for example:\n\n\
       \x20   game_dir = \"{example}\"\n\n\
       Nothing was changed.",
      example = config::example_game_dir(),
    ),
    &[],
  )
}

/// The refusal `run` returns when `game_dir` resolves to something that is not a
/// directory.
///
/// Worth a check of its own because nothing downstream performs one: both
/// `apply_install` and the record writer `create_dir_all` their way to the
/// route they were handed, so a typo'd `game_dir`, or one whose `~`/`$HOME`
/// never expanded, quietly grows a fresh `BepInEx/plugins` tree at that path
/// (relative to the working directory, in the unexpanded case) and every
/// command reports success while the real game stays untouched.
///
/// Pulled out as a pure function so its wording is testable: `run` itself parses
/// argv and cannot be called from a test.
fn missing_game_dir_error(dir: &std::path::Path) -> AppError {
  AppError::advice(
    "the configured `game_dir` is not a directory.",
    format!(
      "vmm looked for it at:\n\n\x20   {}\n\nNothing was changed. Check the path \
       in your config against where the game is actually installed. `~` and \
       environment variables like `$HOME` are both expanded, so either form \
       works, but a name that is not set is left as written and would show up \
       above exactly as you typed it.",
      dir.display()
    ),
    &[],
  )
}

/// Routes a toggle to the single-mod or whole-target path.
///
/// `clap` guarantees exactly one of `name` and `all` is set
/// (`required_unless_present` plus `conflicts_with`), so the `None` case is
/// unreachable rather than a silent no-op.
fn toggle_dispatch(
  ecosystem: &Ecosystem,
  target: &Target,
  args: &cli::ToggleArgs,
  enabled: bool,
) -> AppResult<()> {
  match &args.name {
    Some(name) => commands::toggle::run(ecosystem, target, name, enabled),
    None => commands::toggle::run_all(ecosystem, target, enabled),
  }
}

/// Prints `error` in the house style and exits non-zero.
///
/// The `vmm: ` prefix lives here and nowhere else, so no message body carries
/// its own. `Display` is used deliberately: returning `AppResult` from `main`
/// instead would let Rust's `Termination` impl format with `Debug`, which is
/// what produced output like `Error: Other("...")`.
#[cfg(not(tarpaulin_include))]
fn report_and_exit(error: AppError) -> ! {
  eprintln!("vmm: {error}");

  std::process::exit(1)
}

#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() {
  if let Err(error) = run().await {
    report_and_exit(error);
  }
}

/// The real entry point. Separate from `main` so every failure path returns an
/// error to one printer rather than formatting its own.
#[cfg(not(tarpaulin_include))]
async fn run() -> AppResult<()> {
  let app = AppCli::parse();
  let config = get_config(app.config.as_deref())
    .unwrap_or_else(|err| panic!("An error has occurred getting the config: '{err}'"));

  logs::setup_logging(&config.log_level);
  tracing::info!("Starting valheim mod manager");

  let base = config.base_dir();

  let client = ThunderstoreClient::builder()
    .base_url(BASE_URL)
    .community(COMMUNITY)
    .cache_dir(&base)
    .progress(Arc::new(IndicatifProgress::new()))
    .build()?;

  // Commands that never touch an install target dispatch before one is
  // resolved, so they work with no `game_dir` configured.
  match &app.command {
    Command::Search(args) => return commands::search::run(&client, &args.term).await,
    Command::Update(sub) if matches!(sub.command, UpdatesCommand::Manifest) => {
      return commands::update::run_manifest(&client).await;
    }
    Command::Profile(args) => return commands::profile::run(&base, &args.command),
    _ => {}
  }

  let game_dir = config::expand_path(require_game_dir(&config));

  if !game_dir.is_dir() {
    return Err(missing_game_dir_error(&game_dir));
  }

  let persisted = layout::read_selection(&base, target::GAME);
  let profile =
    layout::select_profile(app.profile.as_deref(), app.no_profile, persisted.as_deref());
  let target = target::resolve(base, game_dir, profile.as_deref())?;

  // Skipped for `migrate` itself: it would print "run `vmm migrate`" as the
  // immediately preceding line to a command that's about to do exactly that.
  if !matches!(app.command, Command::Migrate) {
    commands::migrate::hint_if_unmigrated(&target, &config);
  }

  // The bundled snapshot is zstd-decompressed and parsed on construction, so
  // build it only if a command actually consults the install rules.
  let ecosystem = LazyCell::new(Ecosystem::bundled);

  match &app.command {
    // Matched on the subcommand rather than assumed: `Manifest` returned above,
    // but a third `UpdatesCommand` variant must not silently reinstall every
    // recorded mod, so the compiler is made to demand an arm for it.
    Command::Update(sub) => match sub.command {
      UpdatesCommand::Mods => commands::update::run_mods(&client, &ecosystem, &target).await?,
      UpdatesCommand::Manifest => unreachable!("dispatched before target resolution"),
    },
    Command::List(list_args) => commands::list::run(&target, &list_args.format)?,
    Command::Install(args) => {
      commands::install::run(&client, &ecosystem, &target, &args.mods).await?
    }
    Command::Uninstall(args) => match args.all {
      true => commands::uninstall::run_all(
        &ecosystem,
        &target,
        args.force,
        args.yes,
        commands::uninstall::confirm_uninstall_all,
      )?,
      false => commands::uninstall::run(&ecosystem, &target, &args.mods, args.force)?,
    },
    Command::Enable(args) => toggle_dispatch(&ecosystem, &target, args, true)?,
    Command::Disable(args) => toggle_dispatch(&ecosystem, &target, args, false)?,
    Command::Migrate => commands::migrate::run(&client, &ecosystem, &target, &config).await?,
    Command::Launch(args) => commands::launch::run(
      &ecosystem,
      &target,
      &config.launch,
      args.vanilla,
      args.print_steam_options,
      &args.args,
    )?,
    Command::Export(args) => match args.code {
      true => commands::portability::export_code(&client, &target).await?,
      false => commands::portability::export_file(&target)?,
    },
    Command::Import(args) => {
      commands::portability::import(&client, &ecosystem, &target, &args.source).await?
    }
    Command::Search(_) | Command::Profile(_) => {
      unreachable!("dispatched before target resolution")
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::Path;

  #[test]
  fn a_game_dir_that_is_not_a_directory_names_the_path_that_was_looked_for() {
    let message = missing_game_dir_error(Path::new("/games/Valhiem")).to_string();

    // The path as vmm resolved it, so a typo or an unexpanded variable is
    // visible in the message rather than inferred from a later surprise.
    assert!(
      message.contains("/games/Valhiem"),
      "the error must name the path it looked for; got: {message}"
    );
    assert!(
      message.contains("game_dir"),
      "the error must name the setting to fix; got: {message}"
    );
    assert!(message.contains("Nothing was changed"), "got: {message}");
  }

  #[test]
  fn the_migration_refusal_offers_the_game_root_it_can_infer() {
    let message = needs_migration_error(Some("/games/Valheim/BepInEx/plugins")).to_string();

    // An old `install_dir` pointed at the plugins folder, and the game root is
    // two levels above it. Handing back the exact line to paste is the whole
    // reason this branch exists, so the inferred path has to appear as a
    // `game_dir` assignment rather than merely being mentioned.
    assert!(
      message.contains("game_dir = \"/games/Valheim\""),
      "the refusal must offer the inferred game root; got: {message}"
    );
    assert!(
      message.contains("install_dir"),
      "the refusal must name the setting being retired; got: {message}"
    );
    assert!(message.contains("Nothing was changed"), "got: {message}");
  }

  #[test]
  fn the_migration_refusal_falls_back_to_the_example_when_it_cannot_infer() {
    // Nothing about this path says where a game root would be, so
    // `suggest_game_dir` declines and the generic example has to carry the
    // message instead of a wrong guess.
    let message = needs_migration_error(Some("/somewhere/else")).to_string();

    assert!(
      message.contains(config::example_game_dir()),
      "the fallback must show the platform example; got: {message}"
    );
    assert!(
      !message.contains("is probably"),
      "an inference that was not made must not be claimed; got: {message}"
    );

    // A config that reports NeedsMigration always has an `install_dir`, but the
    // wording must not depend on being able to read it.
    assert!(needs_migration_error(None).to_string().contains("game_dir"));
  }

  #[test]
  fn the_unset_refusal_names_the_setting_and_shows_an_example() {
    let message = unset_game_dir_error().to_string();

    assert!(message.contains("game_dir"), "got: {message}");
    assert!(
      message.contains(config::example_game_dir()),
      "the refusal must show a usable example; got: {message}"
    );
    // A fresh config has no `install_dir` to retire, so mentioning it here would
    // send a first-time user looking for a key they do not have.
    assert!(
      !message.contains("install_dir"),
      "a fresh config must not be told about a deprecated key; got: {message}"
    );
    assert!(message.contains("Nothing was changed"), "got: {message}");
  }

  #[test]
  fn toggle_dispatch_routes_by_whether_a_mod_was_named() {
    let fixture = test_support::Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    let named = cli::ToggleArgs {
      name: Some("Owner-ModA".to_string()),
      all: false,
    };
    let every = cli::ToggleArgs {
      name: None,
      all: true,
    };

    // `clap` guarantees exactly one of the two is set, so the routing is the
    // only thing standing between `--all` and a silent no-op. Owner-ModA is not
    // installed here, so the single-mod route refuses while the whole-target one
    // has nothing to do and succeeds: two different outcomes, which is what
    // proves they went to different places.
    assert!(toggle_dispatch(&eco, &target, &named, false).is_err());
    assert!(toggle_dispatch(&eco, &target, &every, false).is_ok());
  }
}
