use crate::{api, error::AppResult, package::DependencyGraph, zip};

pub async fn run_manifest(cache_dir: &str, api_url: Option<&str>) -> AppResult<()> {
  tracing::info!("Checking for manifest updates");
  let _ = api::get_manifest(cache_dir, api_url).await?;
  Ok(())
}

pub async fn run_mods(
  cache_dir: &str,
  mod_list: Vec<String>,
  install_dir: Option<&str>,
  api_url: Option<&str>,
) -> AppResult<()> {
  let manifest = api::get_manifest(cache_dir, api_url).await?;

  tracing::info!("Building dependency graph for mods");

  let dg = DependencyGraph::new(mod_list);
  let urls = dg.resolve_interned(&manifest);

  tracing::info!("Done building dependency graph, proceeding to download mods if necessary");

  api::download_files(urls.clone(), cache_dir).await?;

  zip::unzip_downloaded_mods(cache_dir, &urls, install_dir)?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use mockito::Server;
  use std::path::PathBuf;
  use tempfile::tempdir;
  use tokio::runtime::Runtime;

  fn test_manifest_json() -> &'static str {
    r#"[{
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
      "versions": [{
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
      }]
    }]"#
  }

  #[test]
  fn test_run_manifest_success() {
    let mut server = Server::new();
    let last_modified = "Wed, 21 Feb 2024 15:30:45 GMT";

    let _mock = server
      .mock("GET", "/c/valheim/api/v1/package/")
      .with_status(200)
      .with_header("Content-Type", "application/json")
      .with_header("Last-Modified", last_modified)
      .with_body(test_manifest_json())
      .create();

    let api_url = format!("{}/c/valheim/api/v1/package/", server.url());
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run_manifest(cache_dir, Some(&api_url)));

    assert!(result.is_ok());
  }

  #[test]
  fn test_run_mods_with_empty_mod_list() {
    let mut server = Server::new();
    let last_modified = "Wed, 21 Feb 2024 15:30:45 GMT";

    let _mock = server
      .mock("GET", "/c/valheim/api/v1/package/")
      .with_status(200)
      .with_header("Content-Type", "application/json")
      .with_header("Last-Modified", last_modified)
      .with_body(test_manifest_json())
      .create();

    let api_url = format!("{}/c/valheim/api/v1/package/", server.url());
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    // download_files with an empty mod list won't create the downloads directory,
    // so pre-create it to prevent unzip_downloaded_mods from erroring on a missing dir.
    std::fs::create_dir_all(PathBuf::from(cache_dir).join("downloads")).unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run_mods(cache_dir, vec![], None, Some(&api_url)));

    assert!(result.is_ok());
  }
}
