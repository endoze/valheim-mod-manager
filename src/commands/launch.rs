use crate::config::LaunchConfig;
use crate::error::{AppError, AppResult};
use crate::target::{GAME, Target};
use std::path::Path;
use thunderstore_engine::Error;
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile::launch::{
  self, HostOs, LaunchContext, LaunchMode, LaunchNotice, Runtime, StorePlatform, launch_plan_in,
  prepare_launch, write_wrapper_script,
};
use thunderstore_engine::steam::{Severity, Visibility, visibility};

/// Config extras first, command-line extras second, so an invocation can append
/// to a persistent set without rewriting it.
pub fn combined_args(config: &LaunchConfig, extra_args: &[String]) -> Vec<String> {
  let mut args = config.extra_args.clone();

  args.extend(extra_args.iter().cloned());

  args
}

/// Refuses a launch whose Steam-resolved paths the running Steam client cannot
/// see, and warns when that cannot be proven either way.
///
/// Every path checked here is resolved by *Steam*, never by vmm: Steam `exec`s the
/// wrapper when it expands `%command%`, and the wrapper's own `[ -d ]` test on
/// `--ts-target`, plus Doorstop's open of the preloader beneath it, run inside the
/// game process. vmm only ever passes them along as command-line text, so nothing
/// on this side fails when they are wrong and `steam -applaunch` still exits 0.
/// Without this check the entire failure is one line in Steam's own console log and
/// a game that never appears.
///
/// An unproven suspicion is a warning, not a refusal: a path is only rejected when
/// a live Steam client's mount namespace has actually been inspected and found to
/// lack it. See the engine's `thunderstore_engine::steam` module.
///
/// The full explanation goes to stderr and the returned error is a single line
/// pointing at it, rather than carrying the whole thing. `main` prints errors
/// with `Display`, so a multi-line message would render fine; the split is to
/// avoid printing the same long explanation twice, once as the warning and again
/// as the error.
fn check_steam_can_see(script: &Path, target_dir: &Path) -> AppResult<()> {
  for (label, path) in [
    ("wrapper script", script),
    ("profile directory", target_dir),
  ] {
    apply_visibility(label, path, visibility(path))?;
  }

  Ok(())
}

/// Turns one [`Visibility`] verdict into a refusal, a warning, or silence.
///
/// Split from [`check_steam_can_see`] so the policy can be asserted for every
/// verdict without depending on whether a Steam client happens to be running on
/// the machine the tests run on.
fn apply_visibility(label: &str, path: &Path, verdict: Visibility) -> AppResult<()> {
  match verdict.severity() {
    Severity::Ok => Ok(()),
    Severity::Warn => {
      eprintln!("\nvmm: warning: {}\n", suspect_message(label, path));

      Ok(())
    }
    Severity::Refuse => {
      let Visibility::Unreachable { steam_pid } = verdict else {
        unreachable!("only an Unreachable verdict refuses");
      };

      eprintln!("\nvmm: {}\n", unreachable_message(label, path, steam_pid));

      Err(AppError::advice(
        format!("Steam cannot reach the {label}."),
        format!(
          "{} is not visible to the running Steam client, so a launch through \
           Steam would silently do nothing. The explanation above says why. \
           The game was not started.",
          path.display()
        ),
        &[],
      ))
    }
  }
}

/// The operator-facing explanation for a path Steam cannot resolve, given the
/// `label` of what the path is for.
///
/// Written to name the evidence, which pid's namespace was inspected, so a
/// wrong verdict can be recognised as wrong instead of merely disbelieved.
fn unreachable_message(label: &str, path: &Path, steam_pid: u32) -> String {
  format!(
    "Steam cannot see the {label} it would be told to use:\n\n    {path}\n\n\
     The running Steam client (pid {steam_pid}) resolves paths in its own mount\n\
     namespace, and that path does not exist inside it. Launching would appear to\n\
     do nothing: Steam would fail to run it and record the failure only in its own\n\
     console log (~/.steam/steam/logs/console-linux.txt).\n\n\
     A sandboxed Steam does this to /tmp in particular, which it replaces with a\n\
     private empty filesystem. Point `data_dir` in your config at a location Steam\n\
     can see, such as somewhere under your home directory, then paste the new\n\
     `vmm launch --print-steam-options` string into this game's Steam launch\n\
     options once.",
    path = path.display(),
  )
}

/// The same explanation for a path only *suspected* unreachable, because no Steam
/// client was running to prove it either way.
fn suspect_message(label: &str, path: &Path) -> String {
  format!(
    "the {label} is under a path a sandboxed Steam replaces with a private\n\
     filesystem of its own:\n\n    {path}\n\n\
     Steam is not running, so this could not be confirmed against its actual mount\n\
     namespace. If the game does not start, this is why: move `data_dir` somewhere\n\
     under your home directory and re-paste the launch options.",
    path = path.display(),
  )
}

/// The refusal for a modded launch into a target with no mod loader.
///
/// A target with no loader is the ordinary state after `vmm uninstall --all`, so
/// this is guidance rather than a fault.
fn no_loader_error(target: &Target) -> AppError {
  AppError::advice(
    format!(
      "there is no mod loader installed in {}.",
      crate::target::describe(target)
    ),
    "vmm loads mods through BepInEx, so a modded launch needs it present. \
     Installing any mod pulls the loader in as a dependency. To play without \
     mods, start the game from Steam as usual, or launch vanilla through vmm.",
    &["vmm install <Owner-ModName>", "vmm launch --vanilla"],
  )
}

/// vmm's wording for the launch refusals it has advice for. Any other engine
/// error is returned as is.
fn launch_error(target: &Target, error: Error) -> AppError {
  match error {
    Error::NoModLoader { .. } => no_loader_error(target),
    Error::MissingGameDir { path } => crate::missing_game_dir_error(&path),
    other => other.into(),
  }
}

/// The refusal [`run`] returns when the host is not one the engine recognises.
///
/// macOS is deliberately absent from the list. `HostOs::detect` does recognise
/// it, so this is not the refusal a macOS user sees: they get the engine's own
/// "macOS launch is not supported: the injection chain requires r2modman's
/// closed-source proxy" from `prepare_launch` instead. Naming macOS here would
/// only ever promise, to some *other* host, support that does not exist either.
///
/// Pulled out as a pure function so the wording is testable: `HostOs::detect`
/// is decided at compile time by `cfg!`, so on any host these tests run on it
/// returns `Some` and this branch is unreachable from `run`.
fn unsupported_os_error() -> AppError {
  AppError::advice(
    "launching is not supported on this operating system.",
    "Launching is supported on Windows and Linux. Installing and managing mods \
     still works here; only `vmm launch` does not.",
    &[],
  )
}

/// Launches the game with the target's mods applied.
///
/// The engine prepares the launch and vmm spawns it. A Linux native Steam launch
/// injects through a wrapper script Steam must invoke, and Steam stores launch
/// options in `localconfig.vdf`, which no manager writes, so the user is told to
/// paste the options once. The script resolves which target to chainload from
/// the argv at run time, so that one paste stays correct across every later
/// profile switch.
pub fn run(
  eco: &Ecosystem,
  target: &Target,
  config: &LaunchConfig,
  vanilla: bool,
  print_steam_options: bool,
  extra_args: &[String],
) -> AppResult<()> {
  // `--print-steam-options` plans vanilla even without `--vanilla`. It is a
  // query for the string to paste, not a launch, and that string is derived from
  // the wrapper's path alone: `needs_wrapper` inside `launch_plan_in` turns on
  // the loader kind, the host, the runtime, and the store, never on the mode, so
  // planning vanilla prints the same bytes a modded plan would. What it avoids is
  // the loader check, which refuses a modded plan when the target has no loader.
  // Pasting the options is setup a user does before installing anything, so
  // requiring a loader first would deadlock the two steps against each other.
  let mode = if vanilla || print_steam_options {
    LaunchMode::Vanilla
  } else {
    LaunchMode::Modded
  };

  let runtime = config.runtime.parse::<Runtime>()?;

  let host = HostOs::detect().ok_or_else(unsupported_os_error)?;

  let ctx = LaunchContext::new(host, config.store.parse::<StorePlatform>()?, runtime, mode)
    .with_extra_args(combined_args(config, extra_args));

  if print_steam_options {
    return print_launch_options(eco, target, &ctx);
  }

  let prepared = prepare_launch(target, eco, &ctx).map_err(|error| launch_error(target, error))?;

  for notice in &prepared.notices {
    match notice {
      LaunchNotice::SteamWrapper {
        script,
        launch_options,
      } => {
        // After the wrapper write and chmod, but staging never runs on the
        // wrapper path, so a refusal here leaves the game folder untouched.
        check_steam_can_see(script, &target.dir)?;

        eprintln!("{}", wrapper_reminder(vanilla, launch_options));
      }
      LaunchNotice::ProtonDllOverride { launch_options, .. } => {
        eprintln!("{}", proton_reminder(vanilla, launch_options));
      }
      LaunchNotice::MadeExecutable { path } => {
        tracing::info!("marked {} executable", path.display());
      }
      _ => {}
    }
  }

  for path in &prepared.staged {
    tracing::info!("staged {}", path.display());
  }

  let mut command = prepared.command;
  let program = command.get_program().to_os_string();

  let status = command.status().map_err(|error| {
    AppError::advice(
      format!("couldn't start {}.", program.to_string_lossy()),
      format!("{error}\n\nCheck that the file exists and is executable."),
      &[],
    )
  })?;

  if !status.success() {
    return Err(AppError::advice(
      format!("the game exited with {status}."),
      "vmm started it successfully, so this is the game's own failure rather \
       than a problem with vmm or your mods list. Check the game's log, and try \
       a vanilla launch to tell a game problem from a mod problem.",
      &["vmm launch --vanilla"],
    ));
  }

  Ok(())
}

/// Prints the options to paste into the game's Steam launch options: the line
/// naming the Steam wrapper script, which is written first, or the Proton DLL
/// override.
///
/// Planned directly rather than through [`prepare_launch`]: this is a query, and
/// preparing a launch would need the game folder to exist, mark loader scripts
/// executable, and stage a profile's loader files into the game folder.
///
/// The wrapper names no target: it reads one from the argv each launch passes,
/// so its bytes are the same for every target and a rewrite only heals a deleted
/// or truncated file.
fn print_launch_options(eco: &Ecosystem, target: &Target, ctx: &LaunchContext) -> AppResult<()> {
  let plan = launch_plan_in(&target.dir, eco, GAME, ctx)?;
  let script = launch::wrapper_path(&target.base, GAME);

  let Some(options) = plan.steam_launch_options(&script) else {
    return Err(AppError::advice(
      "there are no Steam launch options to paste on this platform.",
      "vmm injects into the game directly here rather than through a wrapper \
       script, so Steam needs nothing configured. Just launch normally.",
      &["vmm launch"],
    ));
  };

  if plan.steam_wrapper.is_some() {
    write_wrapper_script(&script)?;
  }

  println!("{options}");

  Ok(())
}

/// The reminder a Steam-wrapper launch prints, so the user pastes
/// `launch_options` into Steam once.
fn wrapper_reminder(vanilla: bool, launch_options: &str) -> String {
  match vanilla {
    true => format!(
      "vmm: this launch routes through a wrapper script that Steam must invoke.\n\
       This launch is unmodded, as requested. If the options below are not already\n\
       pasted into this game's Steam launch options, paste them once. The script\n\
       resolves its target at run time, so that paste stays correct across every\n\
       later profile switch:\n\n    {launch_options}\n"
    ),
    false => format!(
      "vmm: this launch routes through a wrapper script that Steam must invoke.\n\
       Mods load only once the options below are pasted into this game's Steam\n\
       launch options. That paste is one-time: the script resolves its target at\n\
       run time, so it stays correct across every later profile switch:\n\n    {launch_options}\n"
    ),
  }
}

/// The reminder a Steam launch under Proton prints, so the user pastes
/// `launch_options` into Steam once. Without them Wine loads its builtin DLL
/// and the game starts unmodded.
fn proton_reminder(vanilla: bool, launch_options: &str) -> String {
  match vanilla {
    true => format!(
      "vmm: this launch goes through Steam under Proton, which starts the game with\n\
       its own environment. This launch is unmodded, as requested. If the options\n\
       below are not already pasted into this game's Steam launch options, paste\n\
       them once so modded launches work:\n\n    {launch_options}\n"
    ),
    false => format!(
      "vmm: this launch goes through Steam under Proton, which starts the game with\n\
       its own environment. Mods load only once the options below are pasted into\n\
       this game's Steam launch options. That paste is one-time:\n\n    {launch_options}\n"
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use std::path::PathBuf;
  use tempfile::tempdir;
  use thunderstore_engine::ecosystem::Ecosystem;
  use thunderstore_engine::profile::launch::WRAPPER_TARGET_FLAG;

  #[test]
  fn the_unsupported_os_refusal_does_not_advertise_macos() {
    let message = unsupported_os_error().to_string();

    // `HostOs::detect` returns `Some(MacOs)` on macOS, so a macOS user never
    // reaches this refusal at all: they get the engine's `MACOS_UNSUPPORTED`
    // from `launch_plan_in`. Listing macOS here therefore only ever promises
    // support to a *non*-macOS host that does not have it either.
    assert!(
      !message.to_ascii_lowercase().contains("macos"),
      "the refusal must not offer an OS whose launch is refused; got: {message}"
    );
    assert!(message.contains("Windows"), "got: {message}");
    assert!(message.contains("Linux"), "got: {message}");
  }

  /// A target directory with the BepInEx preloader the plan resolves against.
  fn bepinex_target(dir: &Path) {
    let core = dir.join("BepInEx/core");

    std::fs::create_dir_all(&core).unwrap();
    std::fs::write(core.join("BepInEx.Preloader.dll"), b"dll").unwrap();
  }

  /// A game directory containing the Linux executable.
  fn game_dir_with_exe(dir: &Path) {
    std::fs::write(dir.join("valheim.x86_64"), b"exe").unwrap();
  }

  #[test]
  fn to_command_targets_steam_with_the_app_id() {
    let target = tempdir().unwrap();
    let game = tempdir().unwrap();

    bepinex_target(target.path());
    game_dir_with_exe(game.path());

    let eco = Ecosystem::bundled();
    let ctx = LaunchContext::new(
      HostOs::Windows,
      StorePlatform::Steam,
      Runtime::Native,
      LaunchMode::Modded,
    );

    let plan = launch_plan_in(target.path(), &eco, GAME, &ctx).unwrap();
    let command = plan.to_command(game.path()).unwrap();

    assert_eq!(
      command.get_program(),
      thunderstore_engine::steam::steam_exe().as_os_str()
    );

    let args: Vec<String> = command
      .get_args()
      .map(|arg| arg.to_string_lossy().to_string())
      .collect();

    assert_eq!(args[0], "-applaunch");
    assert_eq!(args[1], "892970");
  }

  #[test]
  fn extra_args_come_from_config_then_the_command_line() {
    let config = crate::config::LaunchConfig {
      store: "other".to_string(),
      runtime: "proton".to_string(),
      extra_args: vec!["-from-config".to_string()],
    };

    let args = combined_args(&config, &["-from-cli".to_string()]);

    assert_eq!(
      args,
      vec!["-from-config".to_string(), "-from-cli".to_string()]
    );
  }

  /// A proven-absent path is refused rather than handed to Steam. This is the
  /// regression that matters: without it, `steam -applaunch` is spawned, exits 0,
  /// and the game silently never starts, with the only evidence in Steam's own
  /// console log.
  #[test]
  fn apply_visibility_refuses_a_proven_unreachable_path() {
    let path = Path::new("/tmp/t/d/valheim/steam-wrapper.sh");

    let error = apply_visibility(
      "wrapper script",
      path,
      Visibility::Unreachable { steam_pid: 32372 },
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("Steam cannot reach"), "got: {error}");
    assert!(error.contains("wrapper script"), "got: {error}");
    assert!(error.contains(&path.display().to_string()), "got: {error}");
  }

  /// A suspicion that could not be confirmed against a live Steam client must not
  /// block a launch: Steam may well not be sandboxed, and refusing on a guess would
  /// break working setups.
  #[test]
  fn apply_visibility_warns_but_permits_an_unconfirmed_suspicion() {
    assert!(
      apply_visibility(
        "wrapper script",
        Path::new("/tmp/x/steam-wrapper.sh"),
        Visibility::Unknown { suspect: true },
      )
      .is_ok()
    );
  }

  #[test]
  fn apply_visibility_permits_a_reachable_or_unremarkable_path() {
    let path = Path::new("/home/u/.local/share/vmm/valheim/steam-wrapper.sh");

    assert!(apply_visibility("wrapper script", path, Visibility::Reachable).is_ok());
    assert!(
      apply_visibility(
        "wrapper script",
        path,
        Visibility::Unknown { suspect: false }
      )
      .is_ok()
    );
  }

  #[test]
  fn unreachable_message_names_the_path_and_the_evidence() {
    let message = unreachable_message(
      "wrapper script",
      Path::new("/tmp/t/d/valheim/steam-wrapper.sh"),
      32372,
    );

    assert!(message.contains("wrapper script"), "got: {message}");
    assert!(message.contains("/tmp/t/d/valheim/steam-wrapper.sh"));
    assert!(message.contains("32372"), "must name the pid inspected");
    assert!(message.contains("data_dir"), "must say how to fix it");
  }

  #[test]
  fn suspect_message_says_it_is_unconfirmed() {
    let message = suspect_message("wrapper script", Path::new("/tmp/x/steam-wrapper.sh"));

    assert!(message.contains("/tmp/x/steam-wrapper.sh"));
    assert!(message.contains("not running"), "got: {message}");
  }

  /// The real probe, against a location no Steam namespace contains. Asserts only
  /// that a verdict is reached without panicking and that a home path is never
  /// refused, since whether Steam is running is not under the test's control.
  #[test]
  fn check_steam_can_see_permits_a_home_directory_target() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    let script = PathBuf::from(&home);

    assert!(check_steam_can_see(&script, &script).is_ok());
  }

  /// Writes `run_bepinex.sh` into `dir`'s root, one of the loader scripts that
  /// forces script injection (and, under Steam, a `steam_wrapper`) for a Linux
  /// native launch.
  fn loader_script(dir: &Path) {
    std::fs::write(dir.join("run_bepinex.sh"), b"#!/bin/sh\n").unwrap();
  }

  #[test]
  fn launch_plan_in_appends_the_wrapper_target_flag_for_a_linux_native_steam_plan() {
    let target = tempdir().unwrap();

    bepinex_target(target.path());
    loader_script(target.path());

    let eco = Ecosystem::bundled();
    let ctx = LaunchContext::new(
      HostOs::Linux,
      StorePlatform::Steam,
      Runtime::Native,
      LaunchMode::Modded,
    );

    let plan = launch_plan_in(target.path(), &eco, GAME, &ctx).unwrap();

    assert!(plan.steam_wrapper.is_some());

    // `launch_plan_in` is documented to append `wrapper_target_args(target_dir)`
    // to `LaunchPlan::args` whenever it populates `steam_wrapper`; this pins that
    // vmm relies on rather than assuming it, since `run` only forwards
    // `plan.args` through `LaunchPlan::to_command` and adds nothing of its own.
    let flag_index = plan
      .args
      .iter()
      .position(|arg| arg == WRAPPER_TARGET_FLAG)
      .expect("WRAPPER_TARGET_FLAG missing from plan.args");

    assert_eq!(
      plan.args[flag_index + 1],
      target.path().to_string_lossy().to_string()
    );
  }

  #[test]
  #[cfg(target_os = "linux")]
  fn run_print_steam_options_writes_the_shared_wrapper_and_returns_ok() {
    let base = tempdir().unwrap();
    let game = tempdir().unwrap();

    bepinex_target(game.path());
    loader_script(game.path());

    // Game-dir mode, so `target.dir == target.game_dir == game.path()`.
    let target = Target::resolve(
      base.path().to_path_buf(),
      game.path().to_path_buf(),
      GAME,
      None,
    )
    .unwrap();

    let eco = Ecosystem::bundled();
    let config = crate::config::LaunchConfig {
      store: "steam".to_string(),
      runtime: "native".to_string(),
      extra_args: Vec::new(),
    };

    let result = run(&eco, &target, &config, false, true, &[]);

    assert!(result.is_ok());

    let script = launch::wrapper_path(&target.base, GAME);

    assert_eq!(script, base.path().join(GAME).join("steam-wrapper.sh"));

    let contents = std::fs::read_to_string(&script).unwrap();

    // Proves the runtime-resolving script shipped, not one with a target baked
    // in at write time.
    assert!(contents.contains("--ts-target"), "got: {contents}");
  }

  #[test]
  #[cfg(target_os = "linux")]
  fn run_print_steam_options_answers_before_any_loader_is_installed() {
    let base = tempdir().unwrap();
    let game = tempdir().unwrap();

    // No `bepinex_target`, no `loader_script`: a game directory exactly as it is
    // before the first `vmm install`. Pasting the launch options is setup a user
    // does *then*, so the query has to answer here or the two steps deadlock.
    let target = Target::resolve(
      base.path().to_path_buf(),
      game.path().to_path_buf(),
      GAME,
      None,
    )
    .unwrap();

    let eco = Ecosystem::bundled();
    let config = crate::config::LaunchConfig {
      store: "steam".to_string(),
      runtime: "native".to_string(),
      extra_args: Vec::new(),
    };

    let result = run(&eco, &target, &config, false, true, &[]);

    assert!(
      result.is_ok(),
      "the query must not require a loader; got: {:?}",
      result.err().map(|error| error.to_string())
    );

    // The options string is derived from the wrapper path alone, so it is the
    // same bytes a fully set-up target would print. That is what makes the paste
    // one-time.
    let script = launch::wrapper_path(&target.base, GAME);

    assert!(script.is_file(), "the wrapper must still be written");
  }

  #[test]
  #[cfg(target_os = "linux")]
  fn run_print_steam_options_errors_when_the_plan_carries_no_wrapper() {
    let base = tempdir().unwrap();
    let game = tempdir().unwrap();

    // Game-dir mode: `target.dir == target.game_dir == game.path()`, which has
    // no BepInEx preloader, irrelevant here since a vanilla-mode plan never
    // looks for one.
    let target = Target::resolve(
      base.path().to_path_buf(),
      game.path().to_path_buf(),
      GAME,
      None,
    )
    .unwrap();

    let eco = Ecosystem::bundled();
    // Proton outside Steam: no wrapper, and no Steam launch options to carry
    // the DLL override either.
    let config = crate::config::LaunchConfig {
      store: "other".to_string(),
      runtime: "proton".to_string(),
      extra_args: Vec::new(),
    };

    let error = run(&eco, &target, &config, true, true, &[])
      .unwrap_err()
      .to_string();

    assert!(error.contains("no Steam launch options"), "got: {error}");
  }

  // `--print-steam-options` plans vanilla so it works before a loader is
  // installed, so the paste line has to come back for a vanilla plan.
  #[test]
  fn a_vanilla_proton_steam_plan_has_the_dll_override_to_paste() {
    let target = tempdir().unwrap();
    let eco = Ecosystem::bundled();
    let ctx = LaunchContext::new(
      HostOs::Linux,
      StorePlatform::Steam,
      Runtime::Proton,
      LaunchMode::Vanilla,
    );

    let plan = launch_plan_in(target.path(), &eco, GAME, &ctx).unwrap();

    assert_eq!(
      plan
        .steam_launch_options(&target.path().join("steam-wrapper.sh"))
        .as_deref(),
      Some("WINEDLLOVERRIDES=\"winhttp=n,b\" %command%")
    );
  }

  #[test]
  fn a_plan_launched_outside_steam_has_no_dll_override_to_paste() {
    let target = tempdir().unwrap();
    let eco = Ecosystem::bundled();
    let ctx = LaunchContext::new(
      HostOs::Linux,
      StorePlatform::Other,
      Runtime::Proton,
      LaunchMode::Vanilla,
    );

    let plan = launch_plan_in(target.path(), &eco, GAME, &ctx).unwrap();

    assert_eq!(
      plan.steam_launch_options(&target.path().join("steam-wrapper.sh")),
      None
    );
  }

  #[test]
  fn a_missing_loader_is_worded_as_vmm_advice() {
    let target = Fixture::new().target();
    let error = Error::NoModLoader {
      loader: thunderstore_engine::ecosystem::LoaderKind::BepInEx,
    };

    assert_eq!(
      launch_error(&target, error).to_string(),
      no_loader_error(&target).to_string()
    );
  }

  #[test]
  fn the_proton_reminder_carries_the_options_to_paste() {
    let options = "WINEDLLOVERRIDES=\"winhttp=n,b\" %command%";

    for vanilla in [false, true] {
      let reminder = proton_reminder(vanilla, options);

      assert!(reminder.contains(options), "got: {reminder}");
      assert!(reminder.contains("Proton"), "got: {reminder}");
    }
  }

  #[test]
  fn a_vanilla_proton_reminder_does_not_promise_mods() {
    let options = "WINEDLLOVERRIDES=\"winhttp=n,b\" %command%";
    let vanilla = proton_reminder(true, options);
    let modded = proton_reminder(false, options);

    assert!(vanilla.contains("unmodded"), "got: {vanilla}");
    assert!(!vanilla.contains("Mods load"), "got: {vanilla}");
    assert!(modded.contains("Mods load"), "got: {modded}");
  }

  // macOS launches are refused before the loader is checked, so this end-to-end
  // path never reaches the loader advice there.
  #[cfg(not(target_os = "macos"))]
  #[test]
  fn launching_a_target_with_no_mod_loader_explains_what_to_do() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    // No loader installed: the shape a target has after `vmm uninstall --all`.
    let result = run(
      &eco,
      &target,
      &crate::config::LaunchConfig::default(),
      false,
      false,
      &[],
    );

    let message = result.unwrap_err().to_string();

    assert!(
      message.contains("mod loader"),
      "the error must say the loader is missing; got: {message}"
    );
    assert!(
      message.contains("vmm install"),
      "the error must say how to get one; got: {message}"
    );
  }

  #[test]
  fn launching_into_a_missing_game_dir_names_the_configured_path() {
    let base = tempdir().unwrap();
    let missing = base.path().join("does-not-exist");
    let target = Target::resolve(base.path().to_path_buf(), missing.clone(), GAME, None).unwrap();

    let message = run(
      &Ecosystem::bundled(),
      &target,
      &crate::config::LaunchConfig::default(),
      true,
      false,
      &[],
    )
    .unwrap_err()
    .to_string();

    assert!(message.contains("`game_dir`"), "got: {message}");
    assert!(
      message.contains(&missing.display().to_string()),
      "got: {message}"
    );
  }

  #[test]
  fn launch_error_passes_other_engine_errors_through() {
    let target = Fixture::new().target();
    let error = Error::Profile("boom".to_string());

    assert_eq!(
      launch_error(&target, error.clone()).to_string(),
      error.to_string()
    );
  }
}
