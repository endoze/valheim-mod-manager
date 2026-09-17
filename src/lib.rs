/// Command-line interface definitions and argument parsing.
pub mod cli;
/// Application configuration management.
pub mod config;
/// Error types and result aliases for the application.
pub mod error;
/// Logging configuration and setup.
pub mod logs;
/// Progress reporting backed by `indicatif` for the CLI.
pub mod progress;
/// Resolves which directory an invocation installs into and tracks against.
pub mod target;
