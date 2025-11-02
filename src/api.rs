use crate::error::{AppError, AppResult};
use crate::package::{Package, PackageManifest};

use chrono::prelude::*;
use futures::stream::{FuturesUnordered, StreamExt};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use reqwest::header;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const API_URL: &str = "https://stacklands.thunderstore.io/c/valheim/api/v1/package/";
const LAST_MODIFIED_FILENAME: &str = "last_modified";
const API_MANIFEST_FILENAME: &str = "api_manifest_v2.bin.zst";
const API_MANIFEST_FILENAME_V1: &str = "api_manifest.bin.zst";

/// Returns the path to the last_modified file in the cache directory.
fn last_modified_path(cache_dir: &str) -> PathBuf {
  let expanded_path = shellexpand::tilde(cache_dir);
  let mut path = PathBuf::from(expanded_path.as_ref());
  path.push(LAST_MODIFIED_FILENAME);
  path
}

/// Returns the path to the api_manifest file in the cache directory.
fn api_manifest_path(cache_dir: &str) -> PathBuf {
  let expanded_path = shellexpand::tilde(cache_dir);
  let mut path = PathBuf::from(expanded_path.as_ref());
  path.push(API_MANIFEST_FILENAME);
  path
}

/// Returns the path to the old v1 api_manifest file in the cache directory.
fn api_manifest_path_v1(cache_dir: &str) -> PathBuf {
  let expanded_path = shellexpand::tilde(cache_dir);
  let mut path = PathBuf::from(expanded_path.as_ref());
  path.push(API_MANIFEST_FILENAME_V1);
  path
}

/// Retrieves the manifest of available packages.
///
/// This function first checks if there's a cached manifest file that is up-to-date.
/// If a cached version exists and is current, it loads from disk.
/// Otherwise, it downloads the manifest from the network and caches it.
///
/// # Parameters
///
/// * `cache_dir` - The directory to store cache files in
/// * `api_url` - The API URL to use for network requests (defaults to API_URL)
///
/// # Returns
///
/// A `PackageManifest` containing all available packages in SoA format.
///
/// # Errors
///
/// Returns an error if:
/// - Failed to check or retrieve last modified dates
/// - Network request fails
/// - Parsing fails
pub async fn get_manifest(cache_dir: &str, api_url: Option<&str>) -> AppResult<PackageManifest> {
  let api_url = api_url.unwrap_or(API_URL);
  let last_modified = local_last_modified(cache_dir).await?;
  tracing::info!("Manifest last modified: {}", last_modified);

  if api_manifest_file_exists(cache_dir) && network_last_modified(api_url).await? <= last_modified {
    tracing::info!("Loading manifest from cache");
    get_manifest_from_disk(cache_dir).await
  } else {
    tracing::info!("Downloading new manifest");
    get_manifest_from_network_and_cache(cache_dir, api_url).await
  }
}

/// Reads the cached API manifest from disk.
///
/// Attempts to read the new v2 format first, falls back to v1 format if needed.
/// When v1 format is detected, it automatically migrates to v2.
///
/// # Returns
///
/// The deserialized manifest as a PackageManifest.
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be opened or read
/// - The data cannot be decompressed or deserialized
async fn get_manifest_from_disk(cache_dir: &str) -> AppResult<PackageManifest> {
  let path_v2 = api_manifest_path(cache_dir);
  let path_v1 = api_manifest_path_v1(cache_dir);

  if path_v2.exists() {
    tracing::debug!("Loading v2 manifest format");

    let mut file = fs::File::open(&path_v2).await?;
    let mut compressed_data = Vec::new();
    file.read_to_end(&mut compressed_data).await?;

    let decompressed_data = zstd::decode_all(compressed_data.as_slice())
      .map_err(|e| AppError::Manifest(format!("Failed to decompress v2 manifest: {}", e)))?;

    let manifest: PackageManifest = bincode::deserialize(&decompressed_data)
      .map_err(|e| AppError::Manifest(format!("Failed to deserialize v2 manifest: {}", e)))?;

    manifest
      .validate()
      .map_err(|e| AppError::Manifest(format!("V2 manifest validation failed: {}", e)))?;

    Ok(manifest)
  } else {
    if path_v1.exists() {
      tracing::info!("Found v1 manifest, migrating to v2 format");

      let mut file = fs::File::open(&path_v1).await?;
      let mut compressed_data = Vec::new();
      file.read_to_end(&mut compressed_data).await?;

      let decompressed_data = zstd::decode_all(compressed_data.as_slice())
        .map_err(|e| AppError::Manifest(format!("Failed to decompress v1 manifest: {}", e)))?;

      let packages: Vec<Package> = bincode::deserialize(&decompressed_data)
        .map_err(|e| AppError::Manifest(format!("Failed to deserialize v1 manifest: {}", e)))?;

      let manifest: PackageManifest = packages.into();

      manifest.validate().map_err(|e| {
        AppError::Manifest(format!(
          "V2 manifest validation failed during migration: {}",
          e
        ))
      })?;

      let binary_data = bincode::serialize(&manifest)
        .map_err(|e| AppError::Manifest(format!("Failed to serialize v2 manifest: {}", e)))?;

      write_cache_to_disk(path_v2.clone(), &binary_data, true).await?;

      match tokio::fs::metadata(&path_v2).await {
        Ok(metadata) if metadata.len() > 0 => {
          tracing::info!("V2 manifest written successfully, removing v1");

          if let Err(e) = fs::remove_file(&path_v1).await {
            tracing::warn!(
              "Failed to remove old v1 manifest (keeping as backup): {}",
              e
            );
          }
        }
        Ok(_) => {
          tracing::error!("V2 manifest written but is empty, keeping v1 as backup");
        }
        Err(e) => {
          tracing::error!(
            "Failed to verify v2 manifest write: {}, keeping v1 as backup",
            e
          );
        }
      }

      return Ok(manifest);
    }

    Err(AppError::Manifest("No cached manifest found".to_string()))
  }
}

/// Downloads the manifest from the API server and filters out unnecessary fields.
///
/// This function shows a progress bar while downloading the manifest.
/// It parses the downloaded data to filter out fields marked with #[serde(skip_deserializing)],
/// which helps reduce the size of stored data on disk.
///
/// # Parameters
///
/// * `api_url` - The API URL to download the manifest from
///
/// # Returns
///
/// A tuple containing:
/// - The filtered manifest data as a vector of Packages (with unnecessary fields removed)
/// - The last modified date from the response headers
///
/// # Errors
///
/// Returns an error if:
/// - Network request fails
/// - Response headers cannot be parsed
/// - Last modified date cannot be parsed
/// - JSON parsing or serialization fails
async fn get_manifest_from_network(
  api_url: &str,
) -> AppResult<(Vec<Package>, DateTime<FixedOffset>)> {
  let client = Client::builder().build()?;

  let multi_progress = MultiProgress::new();
  let style_template = "";
  let progress_style = ProgressStyle::with_template(style_template)
    .unwrap()
    .tick_strings(&["-", "\\", "|", "/", ""]);

  let progress_bar = multi_progress.add(ProgressBar::new(100000));
  progress_bar.set_style(progress_style);
  progress_bar.set_message("Downloading Api Manifest");
  progress_bar.enable_steady_tick(Duration::from_millis(130));

  let response = client.get(api_url).send().await?;
  let last_modified_str = response
    .headers()
    .get(header::LAST_MODIFIED)
    .ok_or(AppError::MissingHeader("Last-Modified".to_string()))?
    .to_str()?;
  let last_modified = DateTime::parse_from_rfc2822(last_modified_str)?;

  let raw_response_data = response.bytes().await?;

  let packages: Vec<Package> = serde_json::from_slice(&raw_response_data)?;

  progress_bar.finish_with_message(" Downloaded Api Manifest");

  Ok((packages, last_modified))
}

/// Downloads the manifest from the network and caches it locally.
///
/// This function retrieves the manifest from the API server and saves both
/// the manifest content and the last modified date to disk.
/// The manifest is converted to the SoA format before caching.
///
/// # Parameters
///
/// * `cache_dir` - The directory to store cache files in
/// * `api_url` - The API URL to download the manifest from
///
/// # Returns
///
/// The manifest data as PackageManifest.
///
/// # Errors
///
/// Returns an error if:
/// - Network request fails
/// - Writing cache files fails
async fn get_manifest_from_network_and_cache(
  cache_dir: &str,
  api_url: &str,
) -> AppResult<PackageManifest> {
  let results = get_manifest_from_network(api_url).await?;
  let packages = results.0;
  let last_modified_from_network = results.1;

  write_cache_to_disk(
    last_modified_path(cache_dir),
    last_modified_from_network.to_rfc2822().as_bytes(),
    false,
  )
  .await?;

  let manifest: PackageManifest = packages.into();
  let binary_data = bincode::serialize(&manifest)
    .map_err(|e| AppError::Manifest(format!("Failed to serialize manifest: {}", e)))?;

  write_cache_to_disk(api_manifest_path(cache_dir), &binary_data, true).await?;

  Ok(manifest)
}

/// Retrieves the last modified date from the local cache file.
///
/// If the file doesn't exist, returns a default date (epoch time).
///
/// # Returns
///
/// The last modified date as a `DateTime<FixedOffset>`.
///
/// # Errors
///
/// Returns an error if:
/// - The file exists but cannot be read
/// - The date string cannot be parsed
async fn local_last_modified(cache_dir: &str) -> AppResult<DateTime<FixedOffset>> {
  let path = last_modified_path(cache_dir);

  if let Ok(mut file) = fs::File::open(&path).await {
    tracing::info!("Last modified file exists and was opened.");
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;

    let last_modified = DateTime::parse_from_rfc2822(&contents)?;

    Ok(last_modified)
  } else {
    tracing::info!("Last modified file does not exist and was not opened.");
    let dt = DateTime::from_timestamp(0, 0).unwrap().naive_utc();
    let offset = FixedOffset::east_opt(0).unwrap();

    let last_modified = DateTime::<FixedOffset>::from_naive_utc_and_offset(dt, offset);
    Ok(last_modified)
  }
}

/// Retrieves the last modified date from the API server.
///
/// Makes a HEAD request to the API endpoint to check when the manifest was last updated.
///
/// # Parameters
///
/// * `api_url` - The API URL to check
///
/// # Returns
///
/// The last modified date from the server as a `DateTime<FixedOffset>`.
///
/// # Errors
///
/// Returns an error if:
/// - Network request fails
/// - Last-Modified header is missing
/// - Date string cannot be parsed
async fn network_last_modified(api_url: &str) -> AppResult<DateTime<FixedOffset>> {
  let client = Client::builder().build()?;

  let response = client.head(api_url).send().await?;

  let last_modified =
    response
      .headers()
      .get(header::LAST_MODIFIED)
      .ok_or(AppError::MissingHeader(
        "Last-Modified for API manifest head request".to_string(),
      ))?;
  let last_modified_date = DateTime::parse_from_rfc2822(last_modified.to_str()?)?;

  Ok(last_modified_date)
}

/// Checks if the cached manifest file exists.
///
/// # Returns
///
/// `true` if the api_manifest file exists, `false` otherwise.
fn api_manifest_file_exists(cache_dir: &str) -> bool {
  api_manifest_path(cache_dir).exists()
}

/// Writes data to a cache file on disk.
///
/// Creates the parent directory if it doesn't exist, then the file if it doesn't exist,
/// then writes the provided contents to it.
///
/// # Parameters
///
/// * `path` - The path to the cache file
/// * `contents` - The data to write to the file
/// * `use_compression` - If true, the data will be compressed with zstd level 9
///   (balanced compression ratio for SoA structure's repetitive patterns)
///
/// # Errors
///
/// Returns an error if:
/// - Directory creation fails
/// - File creation fails
/// - Opening the file for writing fails
/// - Writing to the file fails
/// - Compression fails
async fn write_cache_to_disk<T: AsRef<Path>>(
  path: T,
  contents: &[u8],
  use_compression: bool,
) -> AppResult<()> {
  if let Some(parent) = path.as_ref().parent() {
    fs::create_dir_all(parent).await?;
  }

  if !path.as_ref().exists() {
    fs::File::create(&path).await?;
  }

  let mut file = fs::OpenOptions::new()
    .write(true)
    .truncate(true)
    .open(&path)
    .await?;

  if use_compression {
    let compressed_data = zstd::encode_all(contents, 9)
      .map_err(|e| AppError::Manifest(format!("Failed to compress data: {}", e)))?;

    file.write_all(&compressed_data).await?;
    file.flush().await?;
  } else {
    file.write_all(contents).await?;
    file.flush().await?;
  }

  Ok(())
}

/// Downloads multiple files concurrently with progress indicators.
/// Skips files that already exist with the correct version.
///
/// # Parameters
///
/// * `urls` - A HashMap where keys are filenames and values are the URLs to download from
/// * `cache_dir` - The app's cache directory
///
/// # Notes
///
/// - Files are downloaded in parallel with a limit of 2 concurrent downloads
/// - Progress bars show download status for each file
/// - Files are saved in the 'cache_dir/downloads' directory
/// - Files that already exist with the correct version are skipped
pub async fn download_files(urls: HashMap<String, String>, cache_dir: &str) -> AppResult<()> {
  if urls.is_empty() {
    tracing::debug!("No files to download");

    return Ok(());
  }

  tracing::debug!("Processing {} mods", urls.len());

  let client = Client::builder().timeout(Duration::from_secs(60)).build()?;

  let multi_progress = MultiProgress::new();
  let style_template = "";
  let progress_style = ProgressStyle::with_template(style_template)
    .unwrap()
    .progress_chars("#>-");

  let futures: FuturesUnordered<_> = urls
    .into_iter()
    .map(|(archive_filename, url)| {
      let client = client.clone();
      let multi_progress = multi_progress.clone();
      let progress_style = progress_style.clone();

      let cache_dir = cache_dir.to_string();
      tokio::spawn(async move {
        download_file(
          client,
          &url,
          &archive_filename,
          multi_progress,
          progress_style,
          &cache_dir,
        )
        .await
      })
    })
    .collect();

  let responses = futures::stream::iter(futures)
    .buffer_unordered(2)
    .collect::<Vec<_>>()
    .await;

  for response in responses {
    match response {
      Ok(Ok(_)) => {}
      Ok(Err(err)) => {
        tracing::error!("Download error: {:?}", err);
      }
      Err(err) => {
        return Err(AppError::TaskFailed(err));
      }
    }
  }

  Ok(())
}

/// Downloads a single file from a URL with a progress indicator.
/// Checks if the file already exists before downloading to avoid duplicate downloads.
///
/// # Parameters
///
/// * `client` - The HTTP client to use for the download
/// * `url` - The URL to download from
/// * `filename` - The name to save the file as
/// * `multi_progress` - The multi-progress display for coordinating multiple progress bars
/// * `progress_style` - The style to use for the progress bar
/// * `cache_dir` - The app's cache directory
///
/// # Returns
///
/// `Ok(())` on successful download or if file already exists.
///
/// # Errors
///
/// Returns an error if:
/// - Network request fails
/// - Content-Length header is missing
/// - Creating directories or files fails
/// - Writing to the file fails
async fn download_file(
  client: reqwest::Client,
  url: &String,
  filename: &String,
  multi_progress: MultiProgress,
  progress_style: ProgressStyle,
  cache_dir: &str,
) -> AppResult<()> {
  let expanded_path = shellexpand::tilde(cache_dir);
  let mut downloads_directory = PathBuf::from(expanded_path.as_ref());
  downloads_directory.push("downloads");
  let mut file_path = downloads_directory.clone();
  file_path.push(filename);

  tokio::fs::DirBuilder::new()
    .recursive(true)
    .create(&downloads_directory)
    .await?;

  if file_path.exists() {
    tracing::debug!("{} already exists, skipping download", filename);

    return Ok(());
  }

  let response = client.get(url).send().await?;
  let content_length = response
    .headers()
    .get(header::CONTENT_LENGTH)
    .ok_or(AppError::MissingHeader(format!(
      "Content-Length header for {}",
      url
    )))?
    .to_str()?
    .parse::<u64>()?;
  let mut response_data = response.bytes_stream();

  let progress_bar = multi_progress.add(ProgressBar::new(content_length));
  progress_bar.set_style(progress_style);
  progress_bar.set_message(filename.clone());

  let mut file = tokio::fs::File::create(file_path).await?;

  while let Some(result) = response_data.next().await {
    let chunk = result?;
    file.write_all(&chunk).await?;
    progress_bar.inc(chunk.len() as u64);
  }

  file.flush().await?;
  file.sync_all().await?;

  progress_bar.finish();

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use mockito::Server;
  use std::fs::File;
  use std::io::Write;
  use tempfile::tempdir;
  use time::OffsetDateTime;
  use tokio::runtime::Runtime;

  #[test]
  fn test_downloads_directory_construction() {
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    let expanded_path = shellexpand::tilde(cache_dir);
    let mut downloads_directory = PathBuf::from(expanded_path.as_ref());
    downloads_directory.push("downloads");

    let expected_directory = PathBuf::from(cache_dir).join("downloads");
    assert_eq!(downloads_directory, expected_directory);
  }

  #[test]
  fn test_api_manifest_file_exists() {
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    assert!(!api_manifest_file_exists(temp_dir_str));

    let mut path = PathBuf::from(temp_dir_str);
    path.push(API_MANIFEST_FILENAME);
    let _ = File::create(path).unwrap();

    assert!(api_manifest_file_exists(temp_dir_str));
  }

  #[test]
  fn test_path_with_tilde() {
    let home_path = "~/some_test_dir";
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let expected_path = Path::new(&home_dir).join("some_test_dir");

    let last_modified = last_modified_path(home_path);
    let api_manifest = api_manifest_path(home_path);

    assert_eq!(last_modified.parent().unwrap(), expected_path);
    assert_eq!(api_manifest.parent().unwrap(), expected_path);
  }

  #[test]
  fn test_write_cache_to_disk() {
    let temp_dir = tempdir().unwrap();
    let cache_path = temp_dir.path().join("test_cache.txt");
    let test_data = b"Test cache data";

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(write_cache_to_disk(&cache_path, test_data, false));

    assert!(result.is_ok());
    assert!(cache_path.exists());

    let content = std::fs::read(&cache_path).unwrap();
    assert_eq!(content, test_data);

    let new_data = b"Updated cache data";
    let result = rt.block_on(write_cache_to_disk(&cache_path, new_data, false));

    assert!(result.is_ok());
    let content = std::fs::read(&cache_path).unwrap();

    assert_eq!(content, new_data);
  }

  #[test]
  fn test_write_cache_to_disk_creates_directories() {
    let temp_dir = tempdir().unwrap();

    let nonexistent_subdir = temp_dir.path().join("subdir1/subdir2");
    let cache_path = nonexistent_subdir.join("test_cache.txt");

    if nonexistent_subdir.exists() {
      std::fs::remove_dir_all(&nonexistent_subdir).unwrap();
    }

    assert!(!nonexistent_subdir.exists());
    assert!(!cache_path.exists());

    let test_data = b"Test cache data";
    let rt = Runtime::new().unwrap();

    let result = rt.block_on(write_cache_to_disk(&cache_path, test_data, false));
    assert!(result.is_ok());

    assert!(
      nonexistent_subdir.exists(),
      "Directory structure should be created"
    );
    assert!(cache_path.exists(), "File should be created");

    let content = std::fs::read(&cache_path).unwrap();
    assert_eq!(content, test_data, "File should contain the expected data");
  }

  #[test]
  fn test_write_cache_to_disk_with_compression() {
    let temp_dir = tempdir().unwrap();
    let api_manifest_path = temp_dir.path().join("test_compressed.bin");

    let package = Package {
      name: Some("TestMod".to_string()),
      full_name: Some("TestOwner-TestMod".to_string()),
      owner: Some("TestOwner".to_string()),
      package_url: Some("https://example.com/TestMod".to_string()),
      date_created: OffsetDateTime::now_utc(),
      date_updated: OffsetDateTime::now_utc(),
      uuid4: Some("test-uuid".to_string()),
      rating_score: Some(5),
      is_pinned: Some(false),
      is_deprecated: Some(false),
      has_nsfw_content: Some(false),
      categories: vec!["test".to_string()],
      versions: vec![],
    };

    let packages = vec![package];
    let manifest: PackageManifest = packages.into();
    let binary_data = bincode::serialize(&manifest).unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(write_cache_to_disk(&api_manifest_path, &binary_data, true));

    assert!(result.is_ok());
    assert!(api_manifest_path.exists());

    let compressed_data = std::fs::read(&api_manifest_path).unwrap();
    let decompressed_data = zstd::decode_all(compressed_data.as_slice()).unwrap();
    let decoded_manifest: PackageManifest = bincode::deserialize(&decompressed_data).unwrap();

    assert_eq!(decoded_manifest.len(), 1);
    assert_eq!(
      decoded_manifest.full_names[0],
      Some("TestOwner-TestMod".to_string())
    );
  }

  #[test]
  fn test_local_last_modified() {
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();
    let rt = Runtime::new().unwrap();

    {
      let result = rt.block_on(local_last_modified(temp_dir_str));

      assert!(result.is_ok());
    }

    let test_date = "Wed, 21 Feb 2024 15:30:45 GMT";
    let mut path = PathBuf::from(temp_dir_str);
    path.push(LAST_MODIFIED_FILENAME);
    let mut file = File::create(path).unwrap();
    file.write_all(test_date.as_bytes()).unwrap();

    {
      let result = rt.block_on(local_last_modified(temp_dir_str));

      assert!(result.is_ok());
    }
  }

  #[test]
  fn test_get_manifest_from_disk() {
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let package = Package {
      name: Some("ModA".to_string()),
      full_name: Some("Owner-ModA".to_string()),
      owner: Some("Owner".to_string()),
      package_url: Some("https://example.com/ModA".to_string()),
      date_created: OffsetDateTime::now_utc(),
      date_updated: OffsetDateTime::now_utc(),
      uuid4: Some("test-uuid".to_string()),
      rating_score: Some(5),
      is_pinned: Some(false),
      is_deprecated: Some(false),
      has_nsfw_content: Some(false),
      categories: vec!["test".to_string()],
      versions: vec![],
    };

    let packages = vec![package];
    let manifest: PackageManifest = packages.into();
    let binary_data = bincode::serialize(&manifest).unwrap();
    let compressed_data = zstd::encode_all(binary_data.as_slice(), 9).unwrap();
    let mut path = PathBuf::from(temp_dir_str);
    path.push(API_MANIFEST_FILENAME);
    let mut file = File::create(path).unwrap();
    file.write_all(&compressed_data).unwrap();

    let rt = Runtime::new().unwrap();
    let manifest = rt.block_on(get_manifest_from_disk(temp_dir_str)).unwrap();

    assert_eq!(manifest.len(), 1);
    assert_eq!(manifest.full_names[0], Some("Owner-ModA".to_string()));
  }

  #[test]
  fn test_get_manifest_from_disk_error() {
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let mut path = PathBuf::from(temp_dir_str);
    path.push(API_MANIFEST_FILENAME);
    let mut file = File::create(path).unwrap();
    file
      .write_all(b"This is not valid compressed data")
      .unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(get_manifest_from_disk(temp_dir_str));

    assert!(result.is_err());
  }

  #[test]
  fn test_network_last_modified() {
    let mut server = Server::new();
    let test_date = "Wed, 21 Feb 2024 15:30:45 GMT";
    let mock = server
      .mock("HEAD", "/c/valheim/api/v1/package/")
      .with_status(200)
      .with_header("Last-Modified", test_date)
      .create();
    let api_url = format!("{}/c/valheim/api/v1/package/", server.url());
    let rt = Runtime::new().unwrap();

    let result = rt.block_on(network_last_modified(&api_url));

    assert!(result.is_ok());
    if let Ok(parsed_date) = result {
      let expected_date = DateTime::parse_from_rfc2822(test_date).unwrap();
      assert_eq!(parsed_date, expected_date);
    }

    mock.assert();
  }

  #[test]
  fn test_get_manifest_from_network() {
    let mut server = Server::new();

    let test_json = r#"[
      {
        "name": "ModA",
        "full_name": "Owner-ModA",
        "owner": "Owner",
        "package_url": "https://example.com/mods/ModA",
        "date_created": "2024-01-01T12:00:00Z",
        "date_updated": "2024-01-02T12:00:00Z",
        "uuid4": "test-uuid",
        "rating_score": 5,
        "is_pinned": false,
        "is_deprecated": false,
        "has_nsfw_content": false,
        "categories": ["category1"],
        "versions": [
          {
            "name": "ModA",
            "full_name": "Owner-ModA",
            "description": "Test description",
            "icon": "icon.png",
            "version_number": "1.0.0",
            "dependencies": [],
            "download_url": "https://example.com/mods/ModA/download",
            "downloads": 100,
            "date_created": "2024-01-01T12:00:00Z",
            "website_url": "https://example.com",
            "is_active": true,
            "uuid4": "test-version-uuid",
            "file_size": 1024
          }
        ]
      }
    ]"#;

    let test_date = "Wed, 21 Feb 2024 15:30:45 GMT";

    let mock = server
      .mock("GET", "/c/valheim/api/v1/package/")
      .with_status(200)
      .with_header("Content-Type", "application/json")
      .with_header("Last-Modified", test_date)
      .with_body(test_json)
      .create();

    let api_url = format!("{}/c/valheim/api/v1/package/", server.url());

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(get_manifest_from_network(&api_url));

    assert!(result.is_ok());
    if let Ok((packages, last_modified)) = result {
      assert_eq!(packages.len(), 1);
      assert_eq!(packages[0].full_name, Some("Owner-ModA".to_string()));
      let expected_date = DateTime::parse_from_rfc2822(test_date).unwrap();
      assert_eq!(last_modified, expected_date);
    }

    mock.assert();
  }

  #[test]
  fn test_get_manifest() {
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let package = Package {
      name: Some("CachedMod".to_string()),
      full_name: Some("CachedOwner-CachedMod".to_string()),
      owner: Some("CachedOwner".to_string()),
      package_url: Some("https://example.com/CachedMod".to_string()),
      date_created: OffsetDateTime::now_utc(),
      date_updated: OffsetDateTime::now_utc(),
      uuid4: Some("cached-uuid".to_string()),
      rating_score: Some(5),
      is_pinned: Some(false),
      is_deprecated: Some(false),
      has_nsfw_content: Some(false),
      categories: vec!["test".to_string()],
      versions: vec![],
    };

    let packages = vec![package];
    let manifest: PackageManifest = packages.into();
    let binary_data = bincode::serialize(&manifest).unwrap();
    let compressed_data = zstd::encode_all(binary_data.as_slice(), 9).unwrap();

    std::fs::create_dir_all(PathBuf::from(temp_dir_str)).unwrap();

    let mut manifest_path = PathBuf::from(temp_dir_str);
    manifest_path.push(API_MANIFEST_FILENAME);
    let mut file = File::create(manifest_path).unwrap();
    file.write_all(&compressed_data).unwrap();

    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap());
    let recent_date = now.to_rfc2822();
    let mut last_mod_path = PathBuf::from(temp_dir_str);
    last_mod_path.push(LAST_MODIFIED_FILENAME);
    let mut file = File::create(&last_mod_path).unwrap();
    file.write_all(recent_date.as_bytes()).unwrap();

    let rt = Runtime::new().unwrap();

    let result = rt.block_on(get_manifest(temp_dir_str, None));

    assert!(result.is_ok());
  }

  #[test]
  fn test_manifest_v1_to_v2_migration() {
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let package = Package {
      name: Some("OldMod".to_string()),
      full_name: Some("OldOwner-OldMod".to_string()),
      owner: Some("OldOwner".to_string()),
      package_url: Some("https://example.com/OldMod".to_string()),
      date_created: OffsetDateTime::now_utc(),
      date_updated: OffsetDateTime::now_utc(),
      uuid4: Some("old-uuid".to_string()),
      rating_score: Some(4),
      is_pinned: Some(false),
      is_deprecated: Some(false),
      has_nsfw_content: Some(false),
      categories: vec!["legacy".to_string()],
      versions: vec![],
    };

    let packages = vec![package];
    let binary_data = bincode::serialize(&packages).unwrap();
    let compressed_data = zstd::encode_all(binary_data.as_slice(), 9).unwrap();

    std::fs::create_dir_all(PathBuf::from(temp_dir_str)).unwrap();

    let mut v1_path = PathBuf::from(temp_dir_str);
    v1_path.push(API_MANIFEST_FILENAME_V1);
    let mut file = File::create(&v1_path).unwrap();
    file.write_all(&compressed_data).unwrap();

    let v2_path = PathBuf::from(temp_dir_str).join(API_MANIFEST_FILENAME);
    assert!(!v2_path.exists());
    assert!(v1_path.exists());

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(get_manifest_from_disk(temp_dir_str));

    assert!(result.is_ok());
    let manifest = result.unwrap();
    assert_eq!(manifest.len(), 1);
    assert_eq!(manifest.full_names[0], Some("OldOwner-OldMod".to_string()));

    assert!(v2_path.exists(), "v2 manifest should be created");
    assert!(!v1_path.exists(), "v1 manifest should be removed");
  }

  #[test]
  fn test_get_manifest_from_network_and_cache() {
    let mut server = Server::new();

    let test_json = r#"[
      {
        "name": "ModA",
        "full_name": "Owner-ModA",
        "owner": "Owner",
        "package_url": "https://example.com/mods/ModA",
        "date_created": "2024-01-01T12:00:00Z",
        "date_updated": "2024-01-02T12:00:00Z",
        "uuid4": "test-uuid",
        "rating_score": 5,
        "is_pinned": false,
        "is_deprecated": false,
        "has_nsfw_content": false,
        "categories": ["category1"],
        "versions": [
          {
            "name": "ModA",
            "full_name": "Owner-ModA",
            "description": "Test description",
            "icon": "icon.png",
            "version_number": "1.0.0",
            "dependencies": [],
            "download_url": "https://example.com/mods/ModA/download",
            "downloads": 100,
            "date_created": "2024-01-01T12:00:00Z",
            "website_url": "https://example.com",
            "is_active": true,
            "uuid4": "test-version-uuid",
            "file_size": 1024
          }
        ]
      }
    ]"#;

    let test_date = "Wed, 21 Feb 2024 15:30:45 GMT";

    let mock = server
      .mock("GET", "/c/valheim/api/v1/package/")
      .with_status(200)
      .with_header("Content-Type", "application/json")
      .with_header("Last-Modified", test_date)
      .with_body(test_json)
      .create();

    let api_url = format!("{}/c/valheim/api/v1/package/", server.url());
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(get_manifest_from_network_and_cache(temp_dir_str, &api_url));

    assert!(result.is_ok());
    if let Ok(manifest) = result {
      assert_eq!(manifest.len(), 1);
      assert_eq!(manifest.full_names[0], Some("Owner-ModA".to_string()));

      assert!(last_modified_path(temp_dir_str).exists());
      assert!(api_manifest_path(temp_dir_str).exists());

      let last_mod_content = std::fs::read_to_string(last_modified_path(temp_dir_str)).unwrap();
      assert!(last_mod_content.contains("21 Feb 2024 15:30:45"));
    }

    mock.assert();
  }

  #[test]
  fn test_download_file() {
    let mut server = Server::new();
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let test_data = b"This is test file content";
    let content_length = test_data.len();

    let _mock = server
      .mock("GET", "/test-file.zip")
      .with_status(200)
      .with_header("Content-Type", "application/zip")
      .with_header("Content-Length", &content_length.to_string())
      .with_body(test_data)
      .create();

    let file_url = format!("{}/test-file.zip", server.url());
    let filename = "test-file.zip".to_string();

    let multi_progress = MultiProgress::new();
    let style_template = "";
    let progress_style = ProgressStyle::with_template(style_template)
      .unwrap()
      .progress_chars("#>-");

    let rt = Runtime::new().unwrap();
    let client = rt.block_on(async {
      reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap()
    });

    let result = rt.block_on(download_file(
      client.clone(),
      &file_url,
      &filename,
      multi_progress.clone(),
      progress_style.clone(),
      temp_dir_str,
    ));

    assert!(result.is_ok());

    let downloads_dir = PathBuf::from(temp_dir_str).join("downloads");
    assert!(downloads_dir.exists());

    let downloaded_file = downloads_dir.join(&filename);
    assert!(downloaded_file.exists());

    let file_content = std::fs::read(&downloaded_file).unwrap();
    assert_eq!(file_content, test_data);

    let result2 = rt.block_on(download_file(
      client.clone(),
      &file_url,
      &filename,
      multi_progress.clone(),
      progress_style.clone(),
      temp_dir_str,
    ));

    assert!(result2.is_ok());
  }

  #[test]
  fn test_download_file_missing_header() {
    let mut server = Server::new();
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let test_data = b"This is test file content";

    let mock = server
      .mock("GET", "/test-file.zip")
      .with_status(200)
      .with_header("Content-Type", "application/zip")
      .with_body(test_data)
      .create();

    let file_url = format!("{}/test-file.zip", server.url());
    let filename = "test-file-no-header.zip".to_string();

    let multi_progress = MultiProgress::new();
    let style_template = "";
    let progress_style = ProgressStyle::with_template(style_template)
      .unwrap()
      .progress_chars("#>-");

    let rt = Runtime::new().unwrap();
    let client = rt.block_on(async {
      reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap()
    });

    let _result = rt.block_on(download_file(
      client.clone(),
      &file_url,
      &filename,
      multi_progress.clone(),
      progress_style.clone(),
      temp_dir_str,
    ));

    mock.assert();
  }

  #[test]
  fn test_download_files() {
    let mut server = Server::new();
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let test_data1 = b"This is test file 1 content";
    let content_length1 = test_data1.len();

    let test_data2 = b"This is test file 2 content - longer content";
    let content_length2 = test_data2.len();

    let mock1 = server
      .mock("GET", "/file1.zip")
      .with_status(200)
      .with_header("Content-Type", "application/zip")
      .with_header("Content-Length", &content_length1.to_string())
      .with_body(test_data1)
      .create();

    let mock2 = server
      .mock("GET", "/file2.zip")
      .with_status(200)
      .with_header("Content-Type", "application/zip")
      .with_header("Content-Length", &content_length2.to_string())
      .with_body(test_data2)
      .create();

    let mut urls = HashMap::new();
    urls.insert(
      "file1.zip".to_string(),
      format!("{}/file1.zip", server.url()),
    );
    urls.insert(
      "file2.zip".to_string(),
      format!("{}/file2.zip", server.url()),
    );

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(download_files(urls, temp_dir_str));

    assert!(result.is_ok());

    let downloads_dir = PathBuf::from(temp_dir_str).join("downloads");
    assert!(downloads_dir.exists());

    let file1 = downloads_dir.join("file1.zip");
    let file2 = downloads_dir.join("file2.zip");
    assert!(file1.exists());
    assert!(file2.exists());

    let file1_content = std::fs::read(&file1).unwrap();
    let file2_content = std::fs::read(&file2).unwrap();
    assert_eq!(file1_content, test_data1);
    assert_eq!(file2_content, test_data2);

    mock1.assert();
    mock2.assert();
  }

  #[test]
  fn test_download_files_empty() {
    let temp_dir = tempdir().unwrap();
    let temp_dir_str = temp_dir.path().to_str().unwrap();

    let urls = HashMap::new();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(download_files(urls, temp_dir_str));

    assert!(result.is_ok());
  }
}
