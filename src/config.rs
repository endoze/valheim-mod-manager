use config::{Config, ConfigError, File};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{fs::OpenOptions, path::Path};

use crate::error::{AppError, AppResult};

/// Application configuration loaded from vmm_config.toml.
///
/// This structure defines all user-configurable settings for the Valheim Mod Manager,
/// including which mods to manage, logging preferences, and file system paths.
#[derive(Serialize, Deserialize)]
pub struct AppConfig {
  /// List of mods to install and manage, specified as "Owner-ModName" strings.
  pub mod_list: Vec<String>,
  /// Logging level (e.g., "error", "warn", "info", "debug", "trace").
  pub log_level: String,
  /// Optional directory path where mods should be installed.
  pub install_dir: Option<String>,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      mod_list: vec![],
      log_level: "error".into(),
      install_dir: None,
    }
  }
}

/// The XDG config directory for vmm (~/.config/vmm).
///
/// Used as the cache and data directory for downloaded manifests and mod files.
#[cfg(not(tarpaulin_include))]
pub static APP_CACHE_DIR: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
  xdg::BaseDirectories::with_prefix("vmm")
    .expect("Failed to initialize XDG base directories")
    .get_config_home()
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
  let default_config_data = AppConfig::default();

  let mut builder = Config::builder()
    .set_default("mod_list", default_config_data.mod_list.clone())?
    .set_default("log_level", default_config_data.log_level.clone())?
    .set_default("install_dir", default_config_data.install_dir.clone())?;

  if let Some(path) = config_override {
    builder = builder.add_source(File::with_name(path.to_str().unwrap_or_default()));
  } else {
    let local_path = Path::new("vmm_config.toml");
    let xdg_dirs =
      xdg::BaseDirectories::with_prefix("vmm").expect("Failed to initialize XDG base directories");
    let global_path = xdg_dirs.get_config_file("vmm_config.toml");

    if !local_path.exists() && !global_path.exists() {
      if let Ok(path) = xdg_dirs.place_config_file("vmm_config.toml") {
        let _ = create_missing_config_file(&path, &default_config_data);
      }
    }

    if global_path.exists() {
      builder = builder.add_source(File::with_name(global_path.to_str().unwrap_or_default()));
    }

    if local_path.exists() {
      builder = builder.add_source(File::with_name("vmm_config.toml"));
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
    assert!(content.contains("mod_list"));
    assert!(content.contains("log_level"));
    assert!(content.contains("error"));

    let parsed: AppConfig = toml::from_str(&content).unwrap();
    assert_eq!(parsed.log_level, default_config.log_level);
    assert_eq!(parsed.mod_list.len(), default_config.mod_list.len());
  }

  #[test]
  fn test_custom_config_values() {
    let custom_config = AppConfig {
      mod_list: vec!["Owner1-ModA".to_string(), "Owner2-ModB".to_string()],
      log_level: "debug".to_string(),
      install_dir: Some("/path/to/install/directory".to_string()),
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
      install_dir: None,
    };

    create_missing_config_file(&config_path, &custom_config).unwrap();

    let loaded = get_config(Some(&config_path)).unwrap();
    assert_eq!(loaded.log_level, "debug");
    assert_eq!(loaded.mod_list, vec!["Owner1-ModA"]);
  }
}
