use std::io;
use std::num::ParseIntError;
use thiserror::Error;

/// A type alias for `Result<T, AppError>`.
pub type AppResult<T> = Result<T, AppError>;

/// Represents all possible errors that can occur in the application.
#[derive(Error, Debug)]
pub enum AppError {
  /// Error from the IO system.
  #[error("IO error: {0}")]
  Io(#[from] io::Error),

  /// Error from HTTP operations.
  #[error("HTTP error: {0}")]
  Http(#[from] reqwest::Error),

  /// Error from JSON parsing.
  #[error("JSON error: {0}")]
  Json(#[from] serde_json::Error),

  /// Error from ZIP operations.
  #[error("ZIP error: {0}")]
  Zip(#[from] zip::result::ZipError),

  /// Error from date/time parsing.
  #[error("DateTime parse error: {0}")]
  DateTime(#[from] chrono::ParseError),

  /// Error from header parsing.
  #[error("Header error: {0}")]
  Header(#[from] reqwest::header::ToStrError),

  /// Error from parsing integers.
  #[error("Parse integer error: {0}")]
  ParseInt(#[from] ParseIntError),

  /// Missing header in response.
  #[error("Missing header: {0}")]
  MissingHeader(String),

  /// Error from manifest parsing.
  #[error("Manifest error: {0}")]
  Manifest(String),

  /// Generic application error.
  #[error("{0}")]
  Other(String),

  /// Error when a task fails.
  #[error("Task failed: {0}")]
  TaskFailed(#[from] tokio::task::JoinError),

  /// Error when serializing a config fails.
  #[error("Config serialization error: {0}")]
  ConfigSerialization(String),
}
