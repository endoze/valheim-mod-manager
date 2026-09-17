use crate::config::LaunchConfig;
use crate::error::{AppError, AppResult};
use crate::target::{GAME, Target};
use std::path::{Path, PathBuf};
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile::launch::{
  self, HostOs, LaunchContext, LaunchMode, Runtime, StorePlatform, bepinex_preloader,
  ensure_loader_scripts_executable, launch_plan_in, stage_for_launch_in, steam_launch_options,
  write_wrapper_script,
};
use thunderstore_engine::steam::{Severity, Visibility, visibility};

/// Config extras first, command-line extras second, so an invocation can append
/// to a persistent set without rewriting it.
pub fn combined_args(config: &LaunchConfig, extra_args: &[String]) -> Vec<String> {
  let mut args = config.extra_args.clone();

  args.extend(extra_args.iter().cloned());

  args
}

/// Stages the target's root loader files into the game directory, unless the
/// target *is* the game directory.
///
/// The engine's own `stage_for_launch_in` already detects a same-file
/// destination by filesystem identity (`same_file`/`stage_one_file`) and
/// leaves it untouched rather than copying onto itself, so this check is not
/// the only thing standing between a game-dir-mode launch and a truncated
/// loader proxy. It is a second line of defense, in case some future
/// loader's staging destination is ever computed as something other than
/// `game_dir.join(name)`, and it also skips a pointless directory walk in
/// the common game-dir-mode case, where the engine would filter out every
/// entry as already in place anyway. It additionally means a game-dir-mode
/// target whose directory does not exist yet returns an empty list instead
/// of surfacing the engine's `read_dir` error.
pub fn stage_if_needed(
  target: &Target,
  eco: &Ecosystem,
  ctx: &LaunchContext,
) -> AppResult<Vec<PathBuf>> {
  if !target.needs_staging() {
    return Ok(Vec::new());
  }

  let staged = stage_for_launch_in(&target.dir, eco, GAME, &target.game_dir, ctx)?;

  for path in &staged {
    tracing::info!("staged {}", path.display());
  }

  Ok(staged)
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
           Nothing was changed.",
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

/// Refuses a modded launch into a target with no mod loader.
///
/// `launch_plan_in` already fails in this case, but it fails for several other
/// reasons too (a script-injected mode with no script, an unresolvable plan), so
/// its error cannot be reworded into loader-specific guidance. Probing
/// separately keeps the message accurate. `bepinex_preloader` errors exactly when
/// `<dir>/BepInEx/core` holds no preloader assembly, and it reads the filesystem
/// rather than `mods.yml`, so a loader installed by hand or by r2modman is not
/// reported as missing.
///
/// A target with no loader is the ordinary state after `vmm uninstall --all`, so
/// this is guidance rather than a fault.
fn require_loader(target: &Target, runtime: Runtime) -> AppResult<()> {
  if bepinex_preloader(&target.dir, runtime).is_ok() {
    return Ok(());
  }

  Err(AppError::advice(
    format!(
      "there is no mod loader installed in {}.",
      crate::target::describe(target)
    ),
    "vmm loads mods through BepInEx, so a modded launch needs it present. \
     Installing any mod pulls the loader in as a dependency. To play without \
     mods, start the game from Steam as usual, or launch vanilla through vmm.",
    &["vmm install <Owner-ModName>", "vmm launch --vanilla"],
  ))
}

/// The refusal [`run`] returns when the host is not one the engine recognises.
///
/// macOS is deliberately absent from the list. `HostOs::detect` does recognise
/// it, so this is not the refusal a macOS user sees: they get the engine's own
/// "macOS launch is not supported: the injection chain requires r2modman's
/// closed-source proxy" from `launch_plan_in` instead. Naming macOS here would
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

/// The first `<root>/Steam/steam.exe` that exists, over `program_files` in order.
fn steam_exe_in(program_files: &[PathBuf]) -> Option<PathBuf> {
  program_files
    .iter()
    .map(|root| root.join("Steam").join("steam.exe"))
    .find(|candidate| candidate.is_file())
}

/// The Steam executable to start, when this platform needs one named explicitly.
///
/// The engine's [`launch::LaunchPlan::to_command`] builds `Command::new("steam")`
/// for [`launch::LaunchProgram::Steam`], which resolves on Linux, where Steam is
/// on `PATH`. On Windows it does not: the installer adds nothing to `PATH`, so
/// that spawn fails outright and the *default* configuration (`store = "steam"`)
/// could never launch anything.
///
/// Only the two standard install roots are searched. A Steam installed anywhere
/// else falls through to `None`, which leaves the engine's `PATH` lookup in
/// place: no worse than the current behaviour, and it still works for anyone who
/// has put Steam on `PATH` themselves. Reading Steam's own `SteamExe` value out
/// of the registry would cover a relocated install, but that needs a Windows-only
/// registry dependency, and this is the smaller change.
///
/// Gated on `cfg!(windows)` rather than `#[cfg]` so the whole path stays compiled
/// and tested everywhere; the guard also keeps a `ProgramFiles` variable that
/// happens to be set on a non-Windows host from diverting a launch.
fn steam_exe() -> Option<PathBuf> {
  if !cfg!(windows) {
    return None;
  }

  let roots: Vec<PathBuf> = ["ProgramFiles(x86)", "ProgramFiles"]
    .iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .collect();

  steam_exe_in(&roots)
}

/// `steam_exe -applaunch <id> <args>`, the invocation the engine expresses as
/// [`launch::LaunchProgram::Steam`], against an explicitly resolved executable.
///
/// [`launch::LaunchPlan::env`] is deliberately not carried over: the engine
/// documents that it cannot reach the game through `-applaunch`, because Steam
/// is already running with an environment of its own.
fn steam_command(steam_exe: &Path, steam_app_id: u32, args: &[String]) -> std::process::Command {
  let mut command = std::process::Command::new(steam_exe);

  command.arg("-applaunch").arg(steam_app_id.to_string());
  command.args(args);

  command
}

/// The process to spawn for `plan`, overriding the engine's own translation only
/// where that translation cannot work on this platform.
///
/// Every other plan shape, and a Steam plan whose executable could not be found,
/// goes through [`launch::LaunchPlan::to_command`] unchanged.
fn plan_command(plan: &launch::LaunchPlan, game_dir: &Path) -> AppResult<std::process::Command> {
  if let launch::LaunchProgram::Steam { steam_app_id } = plan.program
    && let Some(exe) = steam_exe()
  {
    return Ok(steam_command(&exe, steam_app_id, &plan.args));
  }

  Ok(plan.to_command(game_dir)?)
}

/// Launches the game with the target's mods applied.
///
/// The engine computes the invocation; vmm spawns it. A Linux native Steam launch
/// injects through a wrapper script Steam must invoke, and Steam stores launch
/// options in `localconfig.vdf`, which no manager writes, so the script is
/// written once and the user is told to paste the options once. The script
/// resolves which target to chainload from the argv at run time rather than
/// from anything baked in at write time, so that one paste stays correct across
/// every later profile switch.
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
  // `resolve_program`, which refuses a modded plan outright when the target ships
  // none of the loader scripts. Pasting the options is setup a user does before
  // installing anything, so requiring a loader first would deadlock the two
  // steps against each other.
  let mode = if vanilla || print_steam_options {
    LaunchMode::Vanilla
  } else {
    LaunchMode::Modded
  };

  let runtime = config.runtime.parse::<Runtime>()?;

  let host = HostOs::detect().ok_or_else(unsupported_os_error)?;

  let ctx = LaunchContext::new(host, config.store.parse::<StorePlatform>()?, runtime, mode)
    .with_extra_args(combined_args(config, extra_args));

  // Gated on the mode rather than on `vanilla` alone, for the same reason: the
  // query is answerable without a loader, and every path that actually starts the
  // game still goes through this.
  if mode == LaunchMode::Modded {
    require_loader(target, runtime)?;
  }

  // Computed before staging so a plan that fails to resolve (a missing
  // preloader, a script-injected mode with no script present, and so on) is
  // reported before anything is written into the game directory.
  // `launch_plan_in` reads only `target.dir` and staging writes only into
  // `target.game_dir`, so neither call observes the other and this order is
  // purely about validating before mutating.
  let plan = launch_plan_in(&target.dir, eco, GAME, &ctx)?;

  match &plan.steam_wrapper {
    // `SteamWrapper`'s fields are informational only: the script names no target
    // and the launch-options string is derived from its path alone.
    Some(_) => {
      let script = launch::wrapper_path(&target.base, GAME);

      // The script names no target at all: it reads one from
      // `WRAPPER_TARGET_FLAG` in the argv `launch_plan_in` already built, and a
      // launch that reaches it without that flag, such as someone starting the
      // game straight from Steam outside vmm, is deliberately vanilla. So this
      // is the same bytes for every target, written once per game and rewritten
      // only to heal a deleted or truncated file. That is what makes a single
      // shared path safe, and what keeps one paste into Steam's launch options
      // correct across every later profile switch.
      write_wrapper_script(&script)?;

      let options = steam_launch_options(&script);

      if print_steam_options {
        println!("{options}");

        return Ok(());
      }

      // Deliberately after the `--print-steam-options` return: that flag is a
      // query for the string a user would paste, and answering it must not depend
      // on whether a Steam client happens to be running right now, nor on a target
      // directory that may not exist yet. A silent failure only occurs on an actual
      // launch, so this guards the launch and nothing else. Placed before the
      // paste reminder below so a refusal is not preceded by advice.
      check_steam_can_see(&script, &target.dir)?;

      if vanilla {
        eprintln!(
          "vmm: this launch routes through a wrapper script that Steam must invoke.\n\
           This launch is unmodded, as requested. If the options below are not already\n\
           pasted into this game's Steam launch options, paste them once. The script\n\
           resolves its target at run time, so that paste stays correct across every\n\
           later profile switch:\n\n    {options}\n"
        );
      } else {
        eprintln!(
          "vmm: this launch routes through a wrapper script that Steam must invoke.\n\
           Mods load only once the options below are pasted into this game's Steam\n\
           launch options. That paste is one-time: the script resolves its target at\n\
           run time, so it stays correct across every later profile switch:\n\n    {options}\n"
        );
      }
    }
    None if print_steam_options => {
      return Err(AppError::advice(
        "there are no Steam launch options to paste on this platform.",
        "vmm injects into the game directly here rather than through a wrapper \
         script, so Steam needs nothing configured. Just launch normally.",
        &["vmm launch"],
      ));
    }
    None => {}
  }

  // A loader package ships its `*_bepinex.sh` scripts without an execute bit, and
  // both the Steam wrapper and a `ProfileScript` plan `exec` one of them directly.
  // Under Steam the resulting `Permission denied` is written only to Steam's own
  // console log, so an un-chmodded profile presents as the game silently never
  // starting. Done here, after the plan validates and before anything is spawned,
  // so it covers every target regardless of how it reached disk.
  for path in ensure_loader_scripts_executable(&target.dir)? {
    tracing::info!("marked {} executable", path.display());
  }

  stage_if_needed(target, eco, &ctx)?;

  let mut command = plan_command(&plan, &target.game_dir)?;
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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use tempfile::tempdir;
  use thunderstore_engine::ecosystem::Ecosystem;
  use thunderstore_engine::profile::launch::WRAPPER_TARGET_FLAG;

  #[test]
  fn steam_exe_in_picks_the_first_root_that_actually_has_steam() {
    let dir = tempdir().unwrap();
    let empty = dir.path().join("Program Files (x86)");
    let real = dir.path().join("Program Files");

    std::fs::create_dir_all(empty.join("Steam")).unwrap();
    std::fs::create_dir_all(real.join("Steam")).unwrap();
    std::fs::write(real.join("Steam").join("steam.exe"), b"steam").unwrap();

    assert_eq!(
      steam_exe_in(&[empty, real.clone()]),
      Some(real.join("Steam").join("steam.exe"))
    );
  }

  #[test]
  fn steam_exe_in_is_none_when_no_root_holds_steam() {
    let dir = tempdir().unwrap();

    assert_eq!(steam_exe_in(&[dir.path().to_path_buf()]), None);
  }

  #[test]
  fn the_steam_command_passes_applaunch_the_app_id_and_then_the_plan_args() {
    use std::ffi::OsStr;

    let args = vec!["--doorstop-enabled".to_string(), "true".to_string()];
    let command = steam_command(Path::new("/steam/steam.exe"), 892970, &args);

    assert_eq!(command.get_program(), OsStr::new("/steam/steam.exe"));

    let passed: Vec<String> = command
      .get_args()
      .map(|arg| arg.to_string_lossy().into_owned())
      .collect();

    // The app id has to precede the loader's own arguments: Steam consumes the
    // word after `-applaunch` as the id and forwards the rest to the game.
    assert_eq!(
      passed,
      vec!["-applaunch", "892970", "--doorstop-enabled", "true"]
    );
  }

  // The override exists only because Windows Steam is not on `PATH`. Off Windows
  // `steam` is, and the engine's own `to_command` must stay the thing that runs,
  // so a stray `ProgramFiles` in the environment cannot divert a Linux launch.
  #[cfg(not(windows))]
  #[test]
  fn no_steam_executable_is_resolved_off_windows() {
    assert_eq!(steam_exe(), None);
  }

  // The same guarantee one level up, at the function `run` actually spawns
  // through. `steam_exe` returning `None` is only half of it: what matters is
  // that `plan_command` then falls through rather than building something of its
  // own, and a Steam plan is the one shape it is allowed to divert.
  #[cfg(not(windows))]
  #[test]
  fn plan_command_defers_to_the_engine_when_no_steam_executable_is_resolved() {
    use std::ffi::OsString;

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
    let engine = plan.to_command(game.path()).unwrap();
    let chosen = plan_command(&plan, game.path()).unwrap();

    assert_eq!(chosen.get_program(), engine.get_program());

    let chosen_args: Vec<OsString> = chosen.get_args().map(|arg| arg.to_owned()).collect();
    let engine_args: Vec<OsString> = engine.get_args().map(|arg| arg.to_owned()).collect();

    assert_eq!(chosen_args, engine_args);
  }

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

    assert_eq!(command.get_program(), std::ffi::OsStr::new("steam"));

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

  #[test]
  fn game_dir_mode_never_stages_over_itself() {
    let base = tempdir().unwrap();
    let game = tempdir().unwrap();

    bepinex_target(game.path());
    game_dir_with_exe(game.path());

    // A root-level loader file that staging would copy onto itself.
    let proxy = game.path().join("winhttp.dll");

    std::fs::write(&proxy, b"proxy-bytes").unwrap();

    let target = Target::resolve(
      base.path().to_path_buf(),
      game.path().to_path_buf(),
      GAME,
      None,
    )
    .unwrap();

    let eco = Ecosystem::bundled();
    let ctx = LaunchContext::new(
      HostOs::Linux,
      StorePlatform::Other,
      Runtime::Proton,
      LaunchMode::Modded,
    );

    stage_if_needed(&target, &eco, &ctx).unwrap();

    // The engine's own same-file check already leaves this untouched even
    // without vmm's guard; kept as a regression check against that engine
    // behavior changing, not as proof of the guard below.
    assert_eq!(std::fs::read(&proxy).unwrap(), b"proxy-bytes");

    // A game-dir-mode target whose directory does not exist still returns an
    // empty list rather than an error: `needs_staging` is false (dir ==
    // game_dir, so canonicalization of either falls back to the same
    // not-yet-existing path), and the guard returns before the engine ever
    // gets a chance to `read_dir` a path that isn't there. Without the guard
    // this assertion fails, because `stage_for_launch_in`'s `read_dir` on a
    // missing directory returns `Err`.
    let missing = Target::resolve(
      base.path().to_path_buf(),
      base.path().join("does-not-exist"),
      GAME,
      None,
    )
    .unwrap();

    assert_eq!(
      stage_if_needed(&missing, &eco, &ctx).unwrap(),
      Vec::<PathBuf>::new()
    );
  }

  #[test]
  fn profile_mode_stages_root_files_into_the_game_dir() {
    let base = tempdir().unwrap();
    let game = tempdir().unwrap();

    thunderstore_engine::profile::layout::create(base.path(), GAME, "experiment").unwrap();

    let target = Target::resolve(
      base.path().to_path_buf(),
      game.path().to_path_buf(),
      GAME,
      Some("experiment"),
    )
    .unwrap();

    bepinex_target(&target.dir);
    std::fs::write(target.dir.join("winhttp.dll"), b"proxy-bytes").unwrap();

    let eco = Ecosystem::bundled();
    // Proton, so injection is via the proxy DLL rather than a loader script.
    let ctx = LaunchContext::new(
      HostOs::Linux,
      StorePlatform::Other,
      Runtime::Proton,
      LaunchMode::Modded,
    );

    stage_if_needed(&target, &eco, &ctx).unwrap();

    assert_eq!(
      std::fs::read(game.path().join("winhttp.dll")).unwrap(),
      b"proxy-bytes"
    );
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
    // Proton, not native, so `steam_wrapper` is `None` even under Steam: script
    // injection never applies off native Linux.
    let config = crate::config::LaunchConfig {
      store: "steam".to_string(),
      runtime: "proton".to_string(),
      extra_args: Vec::new(),
    };

    let error = run(&eco, &target, &config, true, true, &[])
      .unwrap_err()
      .to_string();

    assert!(error.contains("no Steam launch options"), "got: {error}");
  }

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
  fn require_loader_accepts_a_target_that_has_the_preloader() {
    let base = tempdir().unwrap();
    let game = tempdir().unwrap();

    bepinex_target(game.path());

    let target = Target::resolve(
      base.path().to_path_buf(),
      game.path().to_path_buf(),
      GAME,
      None,
    )
    .unwrap();

    // Checked directly rather than through `run`. This gate is what every real
    // modded launch passes on its way to spawning the game, and a test cannot
    // follow it that far, so the accepting side would otherwise go unexercised
    // now that `--print-steam-options` no longer routes through it.
    assert!(require_loader(&target, Runtime::Native).is_ok());
  }
}
