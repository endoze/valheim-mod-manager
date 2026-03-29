use config::{Config, ConfigError, File};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::LazyLock;
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
  /// Directory path for caching downloaded manifests and mod files.
  pub cache_dir: String,
  /// Optional directory path where mods should be installed.
  pub install_dir: Option<String>,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      mod_list: vec![],
      log_level: "error".into(),
      cache_dir: "~/.config/vmm".into(),
      install_dir: None,
    }
  }
}

/// Global application configuration instance.
///
/// This static is lazily initialized on first access by reading from vmm_config.toml
/// or creating a new config file with default values if none exists.
///
/// # Panics
///
/// Panics if the configuration file cannot be read or parsed.
#[cfg(not(tarpaulin_include))]
pub static APP_CONFIG: LazyLock<AppConfig> = LazyLock::new(|| {
  get_config().unwrap_or_else(|err| panic!("An error has occurred getting the config: '{err}'"))
});

/// Loads application configuration from vmm_config.toml.
///
/// If a config.toml doesn't already exist, a new one is created with default values.
///
/// # Returns
///
/// The loaded configuration, or an error if reading/parsing fails.
#[cfg(not(tarpaulin_include))]
fn get_config() -> Result<AppConfig, ConfigError> {
  let default_config_data = AppConfig::default();
  let config_path_name = "vmm_config.toml";
  let config_path = Path::new(config_path_name);

  if !config_path.exists() {
    let _ = create_missing_config_file(config_path, &default_config_data);
  }

  Config::builder()
    .set_default("mod_list", default_config_data.mod_list)?
    .set_default("log_level", default_config_data.log_level)?
    .set_default("cache_dir", default_config_data.cache_dir)?
    .set_default("install_dir", default_config_data.install_dir)?
    .add_source(File::with_name(config_path_name))
    .build()?
    .try_deserialize()
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
    let dir = tempdir().unwrap();

    let custom_config = AppConfig {
      mod_list: vec!["Owner1-ModA".to_string(), "Owner2-ModB".to_string()],
      log_level: "debug".to_string(),
      cache_dir: dir
        .path()
        .to_str()
        .expect("Should get string represenation of temporary directory path")
        .to_string(),
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
}
