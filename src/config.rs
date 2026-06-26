use config::{Config, ConfigError, File};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{fs::OpenOptions, path::Path};

use crate::error::{AppError, AppResult};

/// How the game is launched, mapped onto the engine's `LaunchContext`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LaunchConfig {
  /// The store the game was installed from: `steam`, `steam-direct`, or `other`.
  pub store: String,
  /// Whether the game runs natively or under Proton: `native` or `proton`.
  pub runtime: String,
  /// Extra arguments appended after the loader's own.
  pub extra_args: Vec<String>,
}

impl Default for LaunchConfig {
  fn default() -> Self {
    Self {
      store: "steam".into(),
      runtime: "native".into(),
      extra_args: Vec::new(),
    }
  }
}

/// Application configuration loaded from vmm_config.toml.
///
/// This structure defines all user-configurable settings for the Valheim Mod Manager,
/// including which mods to manage, logging preferences, and file system paths.
#[derive(Serialize, Deserialize)]
pub struct AppConfig {
  /// Deprecated. Superseded by `mods.yml`, which is now the authoritative record
  /// of what is installed. Retained so existing config files continue to parse
  /// and so `vmm migrate` can adopt the list; delete the key after migrating.
  ///
  /// Skipped when empty so a config written from scratch does not offer a key
  /// the README tells the reader to delete. An existing list still round-trips,
  /// which is what keeps `vmm migrate` able to see it.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub mod_list: Vec<String>,
  /// Logging level (e.g., "error", "warn", "info", "debug", "trace").
  pub log_level: String,
  /// The game root: the directory containing `BepInEx/` and the loader proxy
  /// (`winhttp.dll`). Mods are installed here using the loader's install rules.
  #[serde(default)]
  pub game_dir: Option<String>,
  /// Deprecated. Previously a per-mod copy target; its meaning changed when the
  /// schema-driven installer was adopted, so it is no longer used. Set
  /// [`AppConfig::game_dir`] instead. Retained only so existing config files
  /// continue to parse.
  #[serde(default)]
  pub install_dir: Option<String>,
  /// The base directory holding the package cache, exports, profiles, and the
  /// client's download cache. Defaults to the XDG config home for vmm.
  #[serde(default)]
  pub data_dir: Option<String>,
  /// Launch settings.
  #[serde(default)]
  pub launch: LaunchConfig,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      mod_list: vec![],
      log_level: "error".into(),
      game_dir: None,
      install_dir: None,
      data_dir: None,
      launch: LaunchConfig::default(),
    }
  }
}

/// The outcome of resolving where mods should be installed.
#[derive(Debug, PartialEq, Eq)]
pub enum GameDirStatus<'a> {
  /// `game_dir` is configured; mods install here.
  Set(&'a str),
  /// `game_dir` is absent but the deprecated `install_dir` is present, an
  /// upgrade from an older version whose config needs migrating.
  NeedsMigration,
  /// Neither `game_dir` nor `install_dir` is configured.
  Unset,
}

impl AppConfig {
  /// Resolves the game root to install into, distinguishing a missing config
  /// from an old-version config that needs migrating.
  pub fn game_dir_status(&self) -> GameDirStatus<'_> {
    if let Some(dir) = self.game_dir.as_deref() {
      GameDirStatus::Set(dir)
    } else if self.install_dir.is_some() {
      GameDirStatus::NeedsMigration
    } else {
      GameDirStatus::Unset
    }
  }

  /// The base directory: `data_dir` when set (see [`expand_path`]), else the XDG
  /// config home. This is the engine base *and* the client cache directory, so
  /// downloads, the package cache, exports, and profiles share one root.
  pub fn base_dir(&self) -> std::path::PathBuf {
    match self.data_dir.as_deref() {
      Some(dir) => expand_path(dir),
      None => std::path::PathBuf::from(&*APP_CACHE_DIR),
    }
  }
}

/// Expands `~` and environment variables in a path read from the config.
///
/// [`shellexpand::full`] rather than `tilde` because vmm's own guidance offers a
/// `$HOME`-prefixed example while the README shows the `~` form: whichever a user
/// copies has to resolve, since an unexpanded `$HOME` yields a *relative* path
/// that every install route is then silently created beneath.
///
/// A name that is not set leaves the string exactly as written rather than
/// collapsing it to something shorter, so the caller's own reporting names the
/// path the user actually typed.
pub fn expand_path(path: &str) -> std::path::PathBuf {
  match shellexpand::full(path) {
    Ok(expanded) => std::path::PathBuf::from(expanded.as_ref()),
    Err(_) => std::path::PathBuf::from(path),
  }
}

/// The example `game_dir` value offered on Unix.
pub const EXAMPLE_GAME_DIR_UNIX: &str = "$HOME/.steam/steam/steamapps/common/Valheim";

/// The example `game_dir` value offered on Windows.
///
/// Drive-rooted rather than variable-prefixed: [`expand_path`] resolves `$VAR`
/// and `~`, and Windows sets neither `HOME` nor anything else `shellexpand`
/// reads, so an example in the Unix shape would be left exactly as written and
/// taken as a *relative* path. Forward slashes because the value is pasted into
/// a TOML string, where a backslash would have to be escaped, and Windows
/// accepts them regardless. This is the default Steam location; a user who
/// installed elsewhere substitutes their own, exactly as on Unix.
pub const EXAMPLE_GAME_DIR_WINDOWS: &str = "C:/Program Files (x86)/Steam/steamapps/common/Valheim";

/// The example `game_dir` value to show on this platform.
///
/// Resolved from the compile-time target: a build cannot run on a platform it
/// was not compiled for, so there is nothing to detect at run time.
pub fn example_game_dir() -> &'static str {
  if cfg!(windows) {
    EXAMPLE_GAME_DIR_WINDOWS
  } else {
    EXAMPLE_GAME_DIR_UNIX
  }
}

/// Derives a likely `game_dir` value from a legacy `install_dir`.
///
/// The common old configuration pointed `install_dir` at `<game>/BepInEx/plugins`
/// (or `<game>/BepInEx/...`). The new `game_dir` is the game root, so this strips
/// the path at the `BepInEx` segment. Returns `None` when no `BepInEx` segment is
/// present (so the caller falls back to generic guidance).
///
/// Matched on whole path segments rather than on a substring, and from the right
/// rather than the left, so a directory that merely *starts* with the same
/// letters (`/games/bepinex-mods/Valheim/BepInEx/plugins`) neither passes for the
/// real segment nor shadows the one further along. Backslashes are folded to
/// forward slashes first so a legacy Windows `install_dir` is handled too, and
/// the suggestion keeps them: it is pasted into a TOML string, where a backslash
/// would have to be escaped, and Windows accepts forward slashes regardless.
pub fn suggest_game_dir(install_dir: &str) -> Option<String> {
  let normalized = install_dir.replace('\\', "/");
  let mut segments: Vec<&str> = normalized.split('/').collect();
  let idx = segments
    .iter()
    .rposition(|segment| segment.eq_ignore_ascii_case("bepinex"))?;

  segments.truncate(idx);

  let joined = segments.join("/");
  let candidate = joined.trim_end_matches('/');

  if candidate.is_empty() {
    None
  } else {
    Some(candidate.to_string())
  }
}

/// The per-user config directory for vmm.
///
/// [`dirs::config_dir`] rather than the `xdg` crate because `xdg` is gated
/// `#![cfg(any(unix, target_os = "redox"))]` and so compiles to an *empty crate*
/// off Unix, which makes every use of it a build error on Windows rather than a
/// runtime fallback. `dirs` resolves `$XDG_CONFIG_HOME`, falling back to
/// `~/.config`, on Linux and `%APPDATA%` on Windows, so the Linux path this
/// produces is byte-for-byte the one `xdg` produced and no existing config
/// moves.
///
/// `None` only when no home directory can be resolved at all.
pub fn config_home() -> Option<std::path::PathBuf> {
  dirs::config_dir().map(|dir| dir.join("vmm"))
}

/// Resolves `name` inside `config_home`, creating that directory if it does not
/// exist, and returns the full path.
///
/// The directory creation is not incidental: it replaces the side effect of
/// `xdg::place_config_file`, which vmm relied on for first run.
/// [`create_missing_config_file`] opens with `create_new`, which fails with
/// `NotFound` rather than creating a missing parent, so without this a machine
/// with no config directory yet could never write its first config.
pub fn place_config_file(config_home: &Path, name: &str) -> std::io::Result<std::path::PathBuf> {
  std::fs::create_dir_all(config_home)?;

  Ok(config_home.join(name))
}

/// The config directory for vmm, as a string.
///
/// Used as the cache and data directory for downloaded manifests and mod files.
#[cfg(not(tarpaulin_include))]
pub static APP_CACHE_DIR: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
  config_home()
    .expect("Failed to resolve the user config directory")
    .to_string_lossy()
    .into_owned()
});

/// Loads application configuration.
///
/// If `config_override` is provided, loads only from that path.
/// Otherwise, looks for a local `vmm_config.toml` first, then falls back to
/// the XDG config location (`~/.config/vmm/vmm_config.toml`). If neither
/// exists, a default config is created at the XDG location.
pub fn get_config(config_override: Option<&Path>) -> Result<AppConfig, ConfigError> {
  let home = config_home();

  get_config_from(
    config_override,
    Path::new("vmm_config.toml"),
    home.as_deref(),
  )
}

/// [`get_config`] with its two discovery locations supplied.
///
/// Split out purely to make the precedence testable. The real locations are the
/// process working directory and the user's config home, neither of which a test
/// can move without reaching for a process-global: `set_current_dir` and the XDG
/// environment are both shared by every test in the binary, so exercising this
/// through [`get_config`] would mean serialising the whole suite around it and
/// would still write into the developer's own config directory. Taking the two
/// paths as arguments keeps the ordering rules here, where they can be checked
/// against temporary directories, rather than in the caller.
///
/// `home` is `None` when no home directory could be resolved at all, which is
/// the case that has to degrade to "local file or defaults" instead of failing.
fn get_config_from(
  config_override: Option<&Path>,
  local_path: &Path,
  home: Option<&Path>,
) -> Result<AppConfig, ConfigError> {
  let default_config_data = AppConfig::default();

  let mut builder = Config::builder()
    .set_default("mod_list", default_config_data.mod_list.clone())?
    .set_default("log_level", default_config_data.log_level.clone())?
    .set_default("game_dir", default_config_data.game_dir.clone())?
    .set_default("install_dir", default_config_data.install_dir.clone())?
    .set_default("data_dir", default_config_data.data_dir.clone())?
    .set_default("launch.store", default_config_data.launch.store.clone())?
    .set_default("launch.runtime", default_config_data.launch.runtime.clone())?
    .set_default(
      "launch.extra_args",
      default_config_data.launch.extra_args.clone(),
    )?;

  if let Some(path) = config_override {
    builder = builder.add_source(File::with_name(path.to_str().unwrap_or_default()));
  } else {
    let global_path = home.map(|dir| dir.join("vmm_config.toml"));
    let global_exists = global_path.as_deref().is_some_and(Path::exists);

    if !local_path.exists()
      && !global_exists
      && let Some(dir) = home
      && let Ok(path) = place_config_file(dir, "vmm_config.toml")
    {
      let _ = create_missing_config_file(&path, &default_config_data);
    }

    if let Some(path) = global_path.filter(|path| path.exists()) {
      builder = builder.add_source(File::with_name(path.to_str().unwrap_or_default()));
    }

    if local_path.exists() {
      builder = builder.add_source(File::with_name(local_path.to_str().unwrap_or_default()));
    }
  }

  builder.build()?.try_deserialize()
}

/// Creates a new configuration file with default values.
///
/// # Parameters
///
/// * `config_path` - Path where the config file should be created
/// * `default_config_data` - Default configuration values to serialize
///
/// # Returns
///
/// `Ok(())` on success, or an error if file creation or serialization fails.
fn create_missing_config_file(
  config_path: &Path,
  default_config_data: &AppConfig,
) -> AppResult<()> {
  let serialized_config_data = toml::to_string(&default_config_data)
    .map_err(|e| AppError::ConfigSerialization(format!("{}", e)))?;

  let mut config_file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(config_path)?;

  write!(config_file, "{}", serialized_config_data)?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use std::path::PathBuf;
  use tempfile::tempdir;

  #[test]
  fn test_default_config() {
    let default_config = AppConfig::default();

    assert!(default_config.mod_list.is_empty());
    assert_eq!(default_config.log_level, "error");
  }

  #[test]
  fn test_create_missing_config_file() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("test_config.toml");
    let default_config = AppConfig::default();

    let result = create_missing_config_file(&config_path, &default_config);
    assert!(result.is_ok());

    assert!(config_path.exists());

    let content = fs::read_to_string(&config_path).unwrap();

    // `mod_list` is deprecated and exists only to be migrated away from, so a
    // config written from scratch must not advertise it. `test_custom_config_values`
    // covers the other side: a list that is actually set is still written.
    assert!(
      !content.contains("mod_list"),
      "a new config must not offer a deprecated key; got:\n{content}"
    );
    assert!(content.contains("log_level"));
    assert!(content.contains("error"));

    let parsed: AppConfig = toml::from_str(&content).unwrap();
    assert_eq!(parsed.log_level, default_config.log_level);
    // Absent parses back to empty rather than failing, so an omitted key and an
    // empty list are the same config.
    assert!(parsed.mod_list.is_empty());
  }

  #[test]
  fn test_custom_config_values() {
    let custom_config = AppConfig {
      mod_list: vec!["Owner1-ModA".to_string(), "Owner2-ModB".to_string()],
      log_level: "debug".to_string(),
      game_dir: Some("/path/to/game".to_string()),
      ..Default::default()
    };

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("custom_config.toml");

    let result = create_missing_config_file(&config_path, &custom_config);
    assert!(result.is_ok());

    let content = fs::read_to_string(&config_path).unwrap();

    assert!(content.contains("Owner1-ModA"));
    assert!(content.contains("Owner2-ModB"));
    assert!(content.contains("debug"));

    let parsed: AppConfig = toml::from_str(&content).unwrap();
    assert_eq!(parsed.log_level, "debug");
    assert_eq!(parsed.mod_list.len(), 2);
    assert_eq!(parsed.mod_list[0], "Owner1-ModA");
    assert_eq!(parsed.mod_list[1], "Owner2-ModB");
  }

  #[test]
  fn test_get_config_with_override() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("override_config.toml");

    let custom_config = AppConfig {
      mod_list: vec!["Owner1-ModA".to_string()],
      log_level: "debug".to_string(),
      ..Default::default()
    };

    create_missing_config_file(&config_path, &custom_config).unwrap();

    let loaded = get_config(Some(&config_path)).unwrap();
    assert_eq!(loaded.log_level, "debug");
    assert_eq!(loaded.mod_list, vec!["Owner1-ModA"]);
  }

  #[test]
  fn a_local_config_layers_over_the_global_one() {
    let dir = tempdir().unwrap();
    let home = dir.path().join("home");
    let local = dir.path().join("project/vmm_config.toml");

    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(local.parent().unwrap()).unwrap();
    fs::write(
      home.join("vmm_config.toml"),
      "log_level = \"warn\"\ngame_dir = \"/from/global\"\n",
    )
    .unwrap();
    fs::write(&local, "log_level = \"debug\"\n").unwrap();

    let loaded = get_config_from(None, &local, Some(&home)).unwrap();

    // The local file is added last, so it wins key by key rather than replacing
    // the global wholesale. That distinction is the whole point of having two
    // locations: a project-local file can override the log level without also
    // having to restate where the game is installed.
    assert_eq!(loaded.log_level, "debug");
    assert_eq!(loaded.game_dir.as_deref(), Some("/from/global"));
  }

  #[test]
  fn the_global_config_is_read_when_there_is_no_local_one() {
    let dir = tempdir().unwrap();
    let home = dir.path().join("home");
    let local = dir.path().join("project/vmm_config.toml");

    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("vmm_config.toml"), "log_level = \"info\"\n").unwrap();

    let loaded = get_config_from(None, &local, Some(&home)).unwrap();

    assert_eq!(loaded.log_level, "info");
    // A global config already answering for this machine means there is nothing
    // to seed, so nothing may be written into the working directory either.
    assert!(!local.exists(), "discovery must not create a local config");
  }

  #[test]
  fn a_first_run_seeds_a_default_config_in_the_config_home() {
    let dir = tempdir().unwrap();
    let home = dir.path().join("home");
    let local = dir.path().join("project/vmm_config.toml");

    // Neither location holds a config and the config home does not exist yet,
    // which is exactly what a first run looks like.
    let loaded = get_config_from(None, &local, Some(&home)).unwrap();

    assert!(
      home.join("vmm_config.toml").is_file(),
      "first run must leave a config the user can edit"
    );
    assert_eq!(loaded.log_level, AppConfig::default().log_level);
    assert!(!local.exists());
  }

  #[test]
  fn discovery_survives_a_machine_with_no_resolvable_home() {
    let dir = tempdir().unwrap();
    let local = dir.path().join("vmm_config.toml");

    fs::write(&local, "log_level = \"trace\"\n").unwrap();

    // `config_home` yields `None` when no home directory resolves at all. There
    // is then nowhere to seed a default and no global to read, so the local file
    // has to carry the load on its own rather than the whole thing failing.
    let loaded = get_config_from(None, &local, None).unwrap();

    assert_eq!(loaded.log_level, "trace");
  }

  #[test]
  fn test_game_dir_status() {
    let with_game_dir = AppConfig {
      game_dir: Some("/games/valheim".to_string()),
      ..Default::default()
    };
    assert_eq!(
      with_game_dir.game_dir_status(),
      GameDirStatus::Set("/games/valheim")
    );

    // Only the deprecated install_dir set => an upgrade that needs migrating.
    let legacy = AppConfig {
      install_dir: Some("/old/plugins".to_string()),
      ..Default::default()
    };
    assert_eq!(legacy.game_dir_status(), GameDirStatus::NeedsMigration);

    // game_dir takes precedence over a leftover install_dir.
    let both = AppConfig {
      game_dir: Some("/games/valheim".to_string()),
      install_dir: Some("/old/plugins".to_string()),
      ..Default::default()
    };
    assert_eq!(both.game_dir_status(), GameDirStatus::Set("/games/valheim"));

    // Fresh config with neither set.
    assert_eq!(AppConfig::default().game_dir_status(), GameDirStatus::Unset);
  }

  #[test]
  fn launch_config_defaults_to_native_steam() {
    let launch = LaunchConfig::default();

    assert_eq!(launch.store, "steam");
    assert_eq!(launch.runtime, "native");
    assert!(launch.extra_args.is_empty());
  }

  #[test]
  fn base_dir_prefers_data_dir_and_expands_tilde() {
    let explicit = AppConfig {
      data_dir: Some("/opt/vmm".to_string()),
      ..Default::default()
    };

    assert_eq!(explicit.base_dir(), std::path::PathBuf::from("/opt/vmm"));

    let expanded = AppConfig {
      data_dir: Some("~/vmm-data".to_string()),
      ..Default::default()
    };

    assert!(!expanded.base_dir().to_string_lossy().starts_with('~'));
  }

  #[test]
  fn base_dir_falls_back_to_the_config_home_when_data_dir_is_unset() {
    let fallback = AppConfig::default().base_dir();

    // This branch is the only reader of `APP_CACHE_DIR`, and that static
    // `expect`s on a machine with no resolvable home, so forcing it here is what
    // separates "the fallback is the config home" from "the fallback panics".
    assert_eq!(fallback, std::path::PathBuf::from(&*APP_CACHE_DIR));
    assert_eq!(fallback, config_home().unwrap());
  }

  #[test]
  fn launch_settings_load_from_a_config_file() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("launch_config.toml");

    fs::write(
      &config_path,
      "log_level = \"info\"\n\
       [launch]\n\
       store = \"other\"\n\
       runtime = \"proton\"\n\
       extra_args = [\"-nolog\"]\n",
    )
    .unwrap();

    let loaded = get_config(Some(&config_path)).unwrap();

    assert_eq!(loaded.launch.store, "other");
    assert_eq!(loaded.launch.runtime, "proton");
    assert_eq!(loaded.launch.extra_args, vec!["-nolog".to_string()]);
  }

  #[test]
  fn launch_settings_fall_back_to_defaults_when_absent() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("bare_config.toml");

    fs::write(&config_path, "log_level = \"info\"\n").unwrap();

    let loaded = get_config(Some(&config_path)).unwrap();

    assert_eq!(loaded.launch.store, "steam");
    assert_eq!(loaded.launch.runtime, "native");
    assert!(loaded.mod_list.is_empty());
  }

  // Split from the environment-variable case below, which is Unix-only: `HOME`
  // is not normally set on Windows, whereas tilde expansion resolves there too
  // (`shellexpand`'s `tilde` feature goes through `dirs`, which reads the
  // Windows profile directory). Reading `HOME` unconditionally would panic the
  // whole test on Windows and take this cross-platform coverage down with it.
  #[test]
  fn expand_path_expands_a_tilde() {
    let home = dirs::home_dir().expect("a home directory");

    assert_eq!(
      expand_path("~/games/Valheim"),
      home.join("games").join("Valheim")
    );
  }

  #[test]
  fn expand_path_passes_through_what_it_cannot_expand() {
    // An absolute path is passed through untouched.
    assert_eq!(
      expand_path("/games/Valheim"),
      PathBuf::from("/games/Valheim")
    );

    // A variable that is not set is left exactly as written, so the caller's
    // existence check reports the path the user actually typed rather than a
    // silently shortened one.
    assert_eq!(
      expand_path("$VMM_UNSET_FOR_TESTS/games"),
      PathBuf::from("$VMM_UNSET_FOR_TESTS/games")
    );
  }

  // Unix-only, and not merely by subject. `expand_path` reads `$HOME` through
  // `shellexpand`, which errors when the variable is unset and leaves the string
  // exactly as written. Windows does not normally set `HOME`, so there the
  // example stays `$HOME/...` and is a *relative* path, which is the trap the
  // sibling test below exists to keep out of the Windows example. Asserting
  // absoluteness off Unix would therefore fail on the platform this example is
  // not for.
  #[cfg(unix)]
  #[test]
  fn the_unix_example_game_dir_expands_to_an_absolute_path() {
    let expanded = expand_path(EXAMPLE_GAME_DIR_UNIX);

    assert!(
      expanded.is_absolute(),
      "an example a user pastes into their config must resolve absolutely; got: {}",
      expanded.display()
    );
  }

  #[test]
  fn the_windows_example_game_dir_is_drive_rooted_and_needs_no_expansion() {
    // Windows does not normally set `HOME`, and `shellexpand` understands only
    // `$VAR` syntax, so a `$HOME`-prefixed example is left there exactly as
    // written and becomes a *relative* path that every install route is then
    // silently created beneath. That is the failure `missing_game_dir_error`
    // exists to report, and offering an example that causes it is a trap.
    assert!(
      !EXAMPLE_GAME_DIR_WINDOWS.contains('$'),
      "a Windows example must not depend on variable expansion; got: {EXAMPLE_GAME_DIR_WINDOWS}"
    );

    // `Path::is_absolute` cannot judge this from a Unix test host, which does
    // not recognise drive letters, so the drive prefix is asserted directly.
    assert!(
      EXAMPLE_GAME_DIR_WINDOWS
        .strip_prefix(char::is_alphabetic)
        .is_some_and(|rest| rest.starts_with(":/")),
      "a Windows example must be drive-rooted; got: {EXAMPLE_GAME_DIR_WINDOWS}"
    );
  }

  #[cfg(unix)]
  #[test]
  fn expand_path_expands_environment_variables() {
    let home = std::env::var("HOME").unwrap();

    // vmm's own guidance offers a `$HOME`-prefixed example, so a config written
    // from that suggestion has to resolve to the same place as the `~` form the
    // README shows. Expanding only one of the two silently yields a *relative*
    // path, which every install route is then created beneath.
    assert_eq!(
      expand_path("$HOME/games/Valheim"),
      PathBuf::from(&home).join("games/Valheim")
    );
    assert_eq!(
      expand_path("${HOME}/games/Valheim"),
      PathBuf::from(&home).join("games/Valheim")
    );
  }

  // Pins the Linux location across the move off the `xdg` crate. `xdg` produced
  // `$XDG_CONFIG_HOME/vmm`, falling back to `~/.config/vmm`; a replacement that
  // resolved anywhere else would silently strand every existing user's config.
  #[cfg(unix)]
  #[test]
  fn config_home_keeps_the_location_the_xdg_crate_produced() {
    let expected = std::env::var_os("XDG_CONFIG_HOME")
      .map(PathBuf::from)
      .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap()).join(".config"))
      .join("vmm");

    assert_eq!(config_home().unwrap(), expected);
  }

  // `xdg::place_config_file` created the config directory as a side effect;
  // `dirs` resolves a path and creates nothing. `create_missing_config_file`
  // opens with `create_new`, which errors with `NotFound` rather than creating
  // the parent, so dropping that side effect would break first run on any
  // machine that has no config directory yet.
  #[test]
  fn place_config_file_creates_the_config_directory_when_it_is_missing() {
    let dir = tempdir().unwrap();
    let home = dir.path().join("vmm");

    assert!(!home.exists());

    let path = place_config_file(&home, "vmm_config.toml").unwrap();

    assert_eq!(path, home.join("vmm_config.toml"));
    assert!(home.is_dir());
  }

  // The whole first-run path end to end, which is what the side effect above
  // exists to serve.
  #[test]
  fn a_fresh_config_is_written_into_a_config_home_that_did_not_exist() {
    let dir = tempdir().unwrap();
    let home = dir.path().join("vmm");

    let path = place_config_file(&home, "vmm_config.toml").unwrap();

    create_missing_config_file(&path, &AppConfig::default()).unwrap();

    assert!(path.is_file());
  }

  #[test]
  fn test_suggest_game_dir() {
    assert_eq!(
      suggest_game_dir("/games/Valheim/BepInEx/plugins").as_deref(),
      Some("/games/Valheim")
    );
    assert_eq!(
      suggest_game_dir("~/.steam/steam/steamapps/common/Valheim/BepInEx").as_deref(),
      Some("~/.steam/steam/steamapps/common/Valheim")
    );
    // Case-insensitive match on the BepInEx segment.
    assert_eq!(
      suggest_game_dir("/games/Valheim/bepinex/plugins").as_deref(),
      Some("/games/Valheim")
    );
    // No BepInEx segment => no suggestion.
    assert_eq!(suggest_game_dir("/some/other/path"), None);
  }

  #[test]
  fn suggest_game_dir_handles_windows_paths_and_repeated_segments() {
    // A legacy Windows `install_dir`. The suggestion is normalized to forward
    // slashes: it is pasted into a TOML string, where a backslash would have to
    // be escaped, and Windows accepts forward slashes anyway.
    assert_eq!(
      suggest_game_dir("C:\\Games\\Valheim\\BepInEx\\plugins").as_deref(),
      Some("C:/Games/Valheim")
    );

    // The *last* BepInEx segment is the game's, not an earlier lookalike
    // directory that merely starts with the same letters.
    assert_eq!(
      suggest_game_dir("/games/bepinex-mods/Valheim/BepInEx/plugins").as_deref(),
      Some("/games/bepinex-mods/Valheim")
    );

    // `bepinex-mods` is not a BepInEx segment, so it is not a game root either.
    assert_eq!(suggest_game_dir("/games/bepinex-mods/plugins"), None);

    // Nothing precedes the segment, so there is no game root to suggest.
    assert_eq!(suggest_game_dir("/BepInEx/plugins"), None);
  }
}
