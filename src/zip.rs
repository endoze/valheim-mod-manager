use fs_extra::copy_items;
use fs_extra::dir::CopyOptions;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use crate::config::APP_CONFIG;
use crate::error::{AppError, AppResult};
use crate::manifest::Manifest;

/// Unzips all downloaded mod files into directories named after the mods.
///
/// For each zip file in the downloads directory, this function:
/// 1. Extracts the mod name and version from the filename
/// 2. Checks if a directory for the mod already exists and has a manifest.json
/// 3. If manifest.json exists, compares its version with the zip version
/// 4. Only extracts the zip if versions don't match or the directory doesn't exist
///
/// # Returns
///
/// `Ok(())` if all files are processed successfully, or an error if reading the directory fails.
pub fn unzip_downloaded_mods(cache_dir: &str) -> AppResult<()> {
  let expanded_path = shellexpand::tilde(cache_dir);
  let mut cache_path = PathBuf::from(expanded_path.as_ref());
  cache_path.push("downloads");
  let downloads_dir = cache_path;
  let entries = fs::read_dir(&downloads_dir)?;

  info!("Processing mod files for extraction");

  for entry in entries.flatten() {
    let file_path = entry.path();

    if file_path.extension().and_then(|ext| ext.to_str()) != Some("zip") {
      continue;
    }

    process_zip_file(&downloads_dir, &file_path)?;
  }

  info!("Finished processing all mod files");

  Ok(())
}

/// Extracts a zip archive to the specified output path with CRC32 verification.
///
/// # Parameters
///
/// * `file` - The zip file to extract
/// * `outpath` - The directory to extract the archive to
///
/// # Returns
///
/// `Ok(())` if extraction succeeds, or a `ZipError` if it fails
///
/// # Errors
///
/// Returns an error if:
/// - The file is not a valid zip archive
/// - There are permission issues with the output path
/// - There is insufficient disk space
/// - Files in the archive have invalid names or paths
/// - CRC32 verification fails for any extracted file
fn unzip<T: AsRef<std::path::Path>>(mut file: std::fs::File, outpath: T) -> AppResult<()> {
  let mut archive = zip::ZipArchive::new(&mut file)?;
  let outpath = outpath.as_ref();

  fs::create_dir_all(outpath)?;

  for i in 0..archive.len() {
    let mut file_in_archive = archive.by_index(i)?;
    let outpath = match file_in_archive.enclosed_name() {
      Some(path) => outpath.join(path),
      None => {
        warn!("Invalid file path in zip: {}", file_in_archive.name());
        continue;
      }
    };

    let comment = file_in_archive.comment();

    if !comment.is_empty() {
      info!("File {} comment: {}", i, comment);
    }

    if file_in_archive.name().ends_with('/') {
      info!("Creating directory {}", outpath.display());
      fs::create_dir_all(&outpath)?;
    } else {
      info!("Extracting file {} to {}", i, outpath.display());
      if let Some(parent) = outpath.parent() {
        if !parent.exists() {
          fs::create_dir_all(parent)?;
        }
      }

      let expected_crc32 = file_in_archive.crc32();
      let mut outfile = fs::File::create(&outpath)?;

      let mut buffer = Vec::new();
      file_in_archive.read_to_end(&mut buffer)?;

      let actual_crc32 = calculate_crc32(&buffer);
      if actual_crc32 != expected_crc32 {
        error!(
          "CRC32 verification failed for {}: expected {:08x}, got {:08x}",
          outpath.display(),
          expected_crc32,
          actual_crc32
        );

        return Err(AppError::Zip(zip::result::ZipError::Io(io::Error::new(
          io::ErrorKind::InvalidData,
          format!("CRC32 verification failed for {}", outpath.display()),
        ))));
      }

      outfile.write_all(&buffer)?;

      debug!("CRC32 verification successful for {}", outpath.display());
    }
  }

  Ok(())
}

/// Calculates the CRC32 checksum for a byte buffer.
///
/// This function is used for verifying file integrity during extraction.
///
/// # Parameters
///
/// * `data` - The byte slice to calculate the checksum for
///
/// # Returns
///
/// The 32-bit CRC32 checksum value
fn calculate_crc32(data: &[u8]) -> u32 {
  let mut hasher = crc32fast::Hasher::new();
  hasher.update(data);
  hasher.finalize()
}

/// Processes a single zip file by extracting mod name and version from the filename,
/// then extracts the contents if needed and copies to install directory if configured.
///
/// # Parameters
///
/// * `downloads_dir` - Path to the downloads directory
/// * `file_path` - Path to the zip file being processed
///
/// # Returns
///
/// `Ok(())` if the file is processed successfully, or an error if:
/// - The filename doesn't match the expected format
/// - Directory creation fails
/// - The zip file cannot be opened
/// - Extraction fails
/// - Copying to install directory fails
fn process_zip_file(downloads_dir: &Path, file_path: &PathBuf) -> AppResult<()> {
  let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
    Some(name) => name,
    None => {
      return Err(AppError::Other(format!(
        "Failed to get filename for {:?}",
        file_path
      )));
    }
  };

  let parts: Vec<&str> = file_name.split('-').collect();

  if parts.len() < 3 {
    return Err(AppError::Other(format!(
      "Invalid filename format: {}",
      file_name
    )));
  }

  let mod_name = format!("{}-{}", parts[0], parts[1]);
  let zip_version = parts[2].trim_end_matches(".zip");
  let mod_dir = downloads_dir.join(&mod_name);
  let should_extract = should_extract_mod(&mod_dir, &mod_name, zip_version)?;

  fs::create_dir_all(&mod_dir).map_err(|e| {
    AppError::Io(io::Error::new(
      io::ErrorKind::Other,
      format!("Failed to create directory for {}: {}", mod_name, e),
    ))
  })?;

  if should_extract {
    debug!("Extracting {} to {}", file_name, mod_dir.display());

    let file = fs::File::open(file_path).map_err(|e| {
      AppError::Io(io::Error::new(
        io::ErrorKind::Other,
        format!("Failed to open {} for unzipping: {}", file_name, e),
      ))
    })?;

    unzip(file, &mod_dir)?;
  }

  if let Some(install_dir) = &APP_CONFIG.install_dir {
    copy_mod_to_install_dir(&mod_dir, install_dir, &mod_name)?;
  }

  Ok(())
}

/// Determines whether a mod should be extracted based on version comparison.
///
/// # Parameters
///
/// * `mod_dir` - Path to the mod directory
/// * `mod_name` - Name of the mod
/// * `zip_version` - Version of the mod in the zip file
///
/// # Returns
///
/// * `Ok(true)` if:
///   - The mod directory doesn't exist
///   - No manifest.json exists
///   - The manifest exists but has a different version
///   - The manifest can't be parsed
/// * `Ok(false)` if the mod already exists with the same version
fn should_extract_mod(mod_dir: &Path, mod_name: &str, zip_version: &str) -> AppResult<bool> {
  let manifest_path = mod_dir.join("manifest.json");

  if !manifest_path.exists() {
    debug!(
      "Mod {} not found or no manifest, extracting version {}",
      mod_name, zip_version
    );

    return Ok(true);
  }

  match Manifest::from_file(&manifest_path) {
    Ok(manifest) => {
      if manifest.version_number == zip_version {
        debug!(
          "Mod {} already has version {}, skipping",
          mod_name, zip_version
        );

        Ok(false)
      } else {
        debug!(
          "Mod {} version changed from {} to {}, extracting",
          mod_name, manifest.version_number, zip_version
        );

        Ok(true)
      }
    }
    Err(err) => {
      warn!("Failed to parse manifest for {}: {}", mod_name, err);

      Ok(true)
    }
  }
}

/// Copies a mod to the configured install directory using optimized copy options.
///
/// This function will:
/// - Create install and target directories if they don't exist
/// - Copy all mod files to the target directory
/// - Overwrite existing files if they exist
/// - Copy directory contents inside the target directory
/// - Use optimized buffer size for faster copying
///
/// # Parameters
///
/// * `mod_dir` - Path to the mod directory
/// * `install_dir` - Path to the installation directory
/// * `mod_name` - Name of the mod
///
/// # Returns
///
/// `Ok(())` if the copy succeeds, or an error if:
/// - The install directory doesn't exist or can't be created
/// - Files can't be copied due to permission issues
/// - Other I/O errors occur
fn copy_mod_to_install_dir(mod_dir: &Path, install_dir: &str, mod_name: &str) -> AppResult<()> {
  let expanded_install_dir = shellexpand::tilde(install_dir);
  let install_path = PathBuf::from(expanded_install_dir.as_ref());

  if !install_path.exists() {
    debug!(
      "Install directory {} does not exist, creating it",
      install_dir
    );

    fs::create_dir_all(&install_path).map_err(|e| {
      AppError::Io(io::Error::new(
        io::ErrorKind::Other,
        format!("Failed to create install directory {}: {}", install_dir, e),
      ))
    })?;
  }

  info!(
    "Copying mod {} to install directory {}",
    mod_name, install_dir
  );

  let target_dir = install_path.join(mod_name);

  if !target_dir.exists() {
    fs::create_dir_all(&target_dir).map_err(|e| {
      AppError::Io(io::Error::new(
        io::ErrorKind::Other,
        format!(
          "Failed to create target directory {}: {}",
          target_dir.display(),
          e
        ),
      ))
    })?;
  }

  let mut options = CopyOptions::new();
  options.overwrite = true;
  options.skip_exist = false;
  options.copy_inside = true;
  options.buffer_size = 64000;

  let entries = fs::read_dir(mod_dir)?
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .collect::<Vec<_>>();

  if !entries.is_empty() {
    debug!(
      "Copying {} items to {}",
      entries.len(),
      target_dir.display()
    );

    copy_items(&entries, &target_dir, &options).map_err(|e| {
      AppError::Io(io::Error::new(
        io::ErrorKind::Other,
        format!("Failed to copy mod files: {}", e),
      ))
    })?;
  }

  debug!(
    "Successfully copied mod {} to {}",
    mod_name,
    target_dir.display()
  );

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs::File;
  use std::io::Write;
  use tempfile::tempdir;
  use zip::write::{FileOptions, ZipWriter};

  #[test]
  fn test_unzip_downloaded_mods() {
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();
    let downloads_dir = PathBuf::from(cache_dir).join("downloads");
    fs::create_dir_all(&downloads_dir).unwrap();

    let zip_path = downloads_dir.join("Owner-ModName-1.0.0.zip");
    let mut file = File::create(&zip_path).unwrap();
    file.write_all(b"This is not a valid zip file").unwrap();

    let result = unzip_downloaded_mods(cache_dir);

    assert!(result.is_err());
  }

  fn create_test_zip(dir: &Path, filename: &str, manifest_content: &str) -> PathBuf {
    let zip_path = dir.join(filename);
    let file = File::create(&zip_path).unwrap();
    let mut zip = ZipWriter::new(file);

    let options: FileOptions<'_, ()> =
      FileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(manifest_content.as_bytes()).unwrap();

    zip.start_file("README.md", options).unwrap();
    zip.write_all(b"# Test Mod\nThis is a test mod.").unwrap();

    zip.finish().unwrap();

    zip_path
  }

  #[test]
  fn test_calculate_crc32() {
    let data = b"test data for crc32 calculation";
    let crc = calculate_crc32(data);

    let expected_crc: u32 = 1707861357;

    assert_eq!(crc, expected_crc);
  }

  #[test]
  fn test_process_zip_file() {
    // We need to set APP_CONFIG.install_dir to None for this test
    // Since we can't modify the global static, we'll mock the function behavior
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();
    let mut cache_path = PathBuf::from(cache_dir);
    cache_path.push("downloads");
    let downloads_dir = cache_path;
    fs::create_dir_all(&downloads_dir).unwrap();

    // Create a manifest.json content
    let manifest_content = r#"{
      "version_number": "1.0.0",
      "name": "TestMod",
      "description": "Test mod description"
    }"#;

    // Create a test zip file
    let zip_path = create_test_zip(&downloads_dir, "Owner-ModName-1.0.0.zip", manifest_content);

    // Process the zip file
    // We need to mock/test the parts individually since we can't modify APP_CONFIG
    let file_name = "Owner-ModName-1.0.0.zip";
    let parts: Vec<&str> = file_name.split('-').collect();
    let mod_name = format!("{}-{}", parts[0], parts[1]);
    let _zip_version = parts[2].trim_end_matches(".zip"); // Not used but included for completeness
    let mod_dir = downloads_dir.join(&mod_name);

    // Create mod directory
    fs::create_dir_all(&mod_dir).unwrap();

    // Extract zip
    let file = fs::File::open(&zip_path).unwrap();
    let extract_result = unzip(file, &mod_dir);
    assert!(extract_result.is_ok());

    // Check that the mod directory was created
    assert!(mod_dir.exists());

    // Check that manifest.json was extracted
    let extracted_manifest = mod_dir.join("manifest.json");
    assert!(extracted_manifest.exists());

    // Read and verify the extracted manifest content
    let content = fs::read_to_string(extracted_manifest).unwrap();
    assert!(content.contains("1.0.0"));
    assert!(content.contains("TestMod"));
  }

  #[test]
  fn test_should_extract_mod() {
    let temp_dir = tempdir().unwrap();
    let mod_dir = temp_dir.path().join("Owner-ModName");
    fs::create_dir_all(&mod_dir).unwrap();

    let result = should_extract_mod(&mod_dir, "Owner-ModName", "1.0.0");
    assert!(result.unwrap());
    let manifest_content = r#"{
      "version_number": "1.0.0",
      "name": "TestMod",
      "description": "Test mod description"
    }"#;

    let manifest_path = mod_dir.join("manifest.json");
    let mut file = File::create(&manifest_path).unwrap();
    file.write_all(manifest_content.as_bytes()).unwrap();

    let result = should_extract_mod(&mod_dir, "Owner-ModName", "1.0.0");
    assert!(!result.unwrap());

    let result = should_extract_mod(&mod_dir, "Owner-ModName", "2.0.0");
    assert!(result.unwrap());
    let mut file = File::create(&manifest_path).unwrap();
    file.write_all(b"invalid json content").unwrap();

    let result = should_extract_mod(&mod_dir, "Owner-ModName", "1.0.0");
    assert!(result.unwrap());
  }

  #[test]
  fn test_copy_mod_to_install_dir() {
    let src_dir = tempdir().unwrap();
    let install_dir = tempdir().unwrap();
    let mod_name = "TestMod";

    let test_file1 = src_dir.path().join("test1.txt");
    let mut file1 = File::create(&test_file1).unwrap();
    file1.write_all(b"test file 1 content").unwrap();

    let test_file2 = src_dir.path().join("test2.txt");
    let mut file2 = File::create(&test_file2).unwrap();
    file2.write_all(b"test file 2 content").unwrap();

    let subdir = src_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();

    let test_file3 = subdir.join("test3.txt");
    let mut file3 = File::create(&test_file3).unwrap();
    file3.write_all(b"test file 3 content").unwrap();

    let result = copy_mod_to_install_dir(
      src_dir.path(),
      install_dir.path().to_str().unwrap(),
      mod_name,
    );

    match &result {
      Err(error) => println!("Copy error: {:?}", error),
      Ok(_) => println!("Copy succeeded"),
    }

    assert!(result.is_ok());

    let target_dir = install_dir.path().join(mod_name);
    let dest_file1 = target_dir.join("test1.txt");
    let dest_file2 = target_dir.join("test2.txt");
    let dest_subdir = target_dir.join("subdir");
    let dest_file3 = dest_subdir.join("test3.txt");

    assert!(target_dir.exists());
    assert!(dest_file1.exists());
    assert!(dest_file2.exists());
    assert!(dest_subdir.exists());
    assert!(dest_file3.exists());

    let content1 = fs::read_to_string(&dest_file1).unwrap();
    let content3 = fs::read_to_string(&dest_file3).unwrap();

    assert_eq!(content1, "test file 1 content");
    assert_eq!(content3, "test file 3 content");
  }
}
