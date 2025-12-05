/// API client for fetching and caching the Thunderstore package manifest.
pub mod api;
/// Command-line interface definitions and argument parsing.
pub mod cli;
/// Application configuration management.
pub mod config;
/// Error types and result aliases for the application.
pub mod error;
/// String interning utilities for memory-efficient string storage.
pub mod intern;
/// Logging configuration and setup.
pub mod logs;
/// Manifest file parsing for individual mod packages.
pub mod manifest;
/// Package data structures and dependency graph resolution.
pub mod package;
/// Zip archive extraction and mod installation utilities.
pub mod zip;
