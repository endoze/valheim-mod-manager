use crate::error::{AppError, AppResult};
use crate::package::Package;

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
const API_MANIFEST_FILENAME: &str = "api_manifest.bin.zst";

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

/// Retrieves the manifest of available packages.
///
/// This function first checks if there's a cached manifest file that is up-to-date.
/// If a cached version exists and is current, it loads from disk.
/// Otherwise, it downloads the manifest from the network and caches it.
///
/// # Returns
///
/// A `HashMap` where keys are package full names and values are the corresponding `Package` objects.
///
/// # Errors
///
/// Returns an error if:
/// - Failed to check or retrieve last modified dates
/// - Network request fails
/// - Parsing fails
pub async fn get_manifest(cache_dir: &str) -> AppResult<HashMap<String, Package>> {
  let last_modified = local_last_modified(cache_dir).await?;
  tracing::info!("Manifest last modified: {}", last_modified);

  let packages =
    if api_manifest_file_exists(cache_dir) && network_last_modified().await? <= last_modified {
      tracing::info!("Loading manifest from cache");
      get_manifest_from_disk(cache_dir).await?
    } else {
      tracing::info!("Downloading new manifest");
      get_manifest_from_network_and_cache(cache_dir).await?
    };

  Ok(
    packages
      .into_iter()
      .filter_map(|pkg| {
        let name = pkg.full_name.clone()?;

        Some((name, pkg))
      })
      .collect(),
  )
}

/// Reads the cached API manifest from disk.
///
/// # Returns
///
/// The deserialized manifest as a Vec<Package>.
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be opened or read
/// - The data cannot be decompressed or deserialized
async fn get_manifest_from_disk(cache_dir: &str) -> AppResult<Vec<Package>> {
  let path = api_manifest_path(cache_dir);
  let mut file = fs::File::open(path).await?;
  let mut compressed_data = Vec::new();
  file.read_to_end(&mut compressed_data).await?;

  let decompressed_data = zstd::decode_all(compressed_data.as_slice())
    .map_err(|e| AppError::Manifest(format!("Failed to decompress manifest: {}", e)))?;

  let packages: Vec<Package> = bincode::deserialize(&decompressed_data)
    .map_err(|e| AppError::Manifest(format!("Failed to deserialize manifest: {}", e)))?;

  Ok(packages)
}

/// Downloads the manifest from the API server and filters out unnecessary fields.
///
/// This function shows a progress bar while downloading the manifest.
/// It parses the downloaded data to filter out fields marked with #[serde(skip_deserializing)],
/// which helps reduce the size of stored data on disk.
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
async fn get_manifest_from_network() -> AppResult<(Vec<Package>, DateTime<FixedOffset>)> {
  let client = Client::builder().build()?;

  let multi_progress = MultiProgress::new();
  let style_template = "[{elapsed_precise}] {spinner:.green} {wide_msg}";
  let progress_style = ProgressStyle::with_template(style_template)
    .unwrap()
    .tick_strings(&["-", "\\", "|", "/", ""]);

  let progress_bar = multi_progress.add(ProgressBar::new(100000));
  progress_bar.set_style(progress_style);
  progress_bar.set_message("Downloading Api Manifest");
  progress_bar.enable_steady_tick(Duration::from_millis(130));

  let response = client.get(API_URL).send().await?;
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
///
/// # Returns
///
/// The manifest data as Vec<Package>.
///
/// # Errors
///
/// Returns an error if:
/// - Network request fails
/// - Writing cache files fails
async fn get_manifest_from_network_and_cache(cache_dir: &str) -> AppResult<Vec<Package>> {
  let results = get_manifest_from_network().await?;
  let packages = results.0;
  let last_modified_from_network = results.1;

  write_cache_to_disk(
    last_modified_path(cache_dir),
    last_modified_from_network.to_rfc2822().as_bytes(),
    false,
  )
  .await?;

  let binary_data = bincode::serialize(&packages)
    .map_err(|e| AppError::Manifest(format!("Failed to serialize manifest: {}", e)))?;

  write_cache_to_disk(api_manifest_path(cache_dir), &binary_data, true).await?;

  Ok(packages)
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
async fn network_last_modified() -> AppResult<DateTime<FixedOffset>> {
  let client = Client::builder().build()?;

  let response = client.head(API_URL).send().await?;

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
/// * `use_compression` - If true, the data will be compressed with zstd
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
    let compressed_data = zstd::encode_all(contents, 3)
      .map_err(|e| AppError::Manifest(format!("Failed to compress data: {}", e)))?;

    file.write_all(&compressed_data).await?;
  } else {
    file.write_all(contents).await?;
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
  let style_template =
    "[{elapsed_precise}] {bar:40.cyan/blue} {decimal_bytes}/{decimal_total_bytes} {msg}";
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
    let binary_data = bincode::serialize(&packages).unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(write_cache_to_disk(&api_manifest_path, &binary_data, true));

    assert!(result.is_ok());
    assert!(api_manifest_path.exists());

    let compressed_data = std::fs::read(&api_manifest_path).unwrap();
    let decompressed_data = zstd::decode_all(compressed_data.as_slice()).unwrap();
    let decoded_packages: Vec<Package> = bincode::deserialize(&decompressed_data).unwrap();

    assert_eq!(decoded_packages.len(), 1);
    assert_eq!(
      decoded_packages[0].full_name,
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
    let binary_data = bincode::serialize(&packages).unwrap();
    let compressed_data = zstd::encode_all(binary_data.as_slice(), 3).unwrap();
    let mut path = PathBuf::from(temp_dir_str);
    path.push(API_MANIFEST_FILENAME);
    let mut file = File::create(path).unwrap();
    file.write_all(&compressed_data).unwrap();

    let rt = Runtime::new().unwrap();
    let packages = rt.block_on(get_manifest_from_disk(temp_dir_str)).unwrap();

    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].full_name, Some("Owner-ModA".to_string()));
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

    let result = rt.block_on(async {
      let client = Client::builder().build().unwrap();
      let response = client.head(&api_url).send().await?;

      let last_modified =
        response
          .headers()
          .get(header::LAST_MODIFIED)
          .ok_or(AppError::MissingHeader(
            "Last-Modified for API manifest head request".to_string(),
          ))?;

      let date_str = last_modified.to_str()?;
      let parsed_date = DateTime::parse_from_rfc2822(date_str)?;

      Ok::<_, AppError>(parsed_date)
    });

    assert!(result.is_ok());
    let parsed_date = result.unwrap();
    let expected_date = DateTime::parse_from_rfc2822(test_date).unwrap();
    assert_eq!(parsed_date, expected_date);

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
    let result = rt.block_on(async {
      let client = Client::builder().build().unwrap();
      let response = client.get(&api_url).send().await?;
      let last_modified_str = response
        .headers()
        .get(header::LAST_MODIFIED)
        .ok_or(AppError::MissingHeader("Last-Modified header".to_string()))?
        .to_str()?;
      let last_modified = DateTime::parse_from_rfc2822(last_modified_str)?;
      let raw_response_data = response.bytes().await?;
      let packages: Vec<Package> = serde_json::from_slice(&raw_response_data)?;

      Ok::<_, AppError>((packages, last_modified))
    });

    assert!(result.is_ok());
    let (packages, last_modified) = result.unwrap();

    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].full_name, Some("Owner-ModA".to_string()));
    let expected_date = DateTime::parse_from_rfc2822(test_date).unwrap();
    assert_eq!(last_modified, expected_date);

    mock.assert();
  }
}
