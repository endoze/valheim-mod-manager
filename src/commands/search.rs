use crate::{api, error::AppResult, intern};

pub async fn run(cache_dir: &str, term: &str, api_url: Option<&str>) -> AppResult<()> {
  let manifest = api::get_manifest(cache_dir, api_url).await?;
  let search_term = term.to_lowercase();

  let search_results: Vec<usize> = (0..manifest.len())
    .filter(|&idx| {
      if let Some(name) = manifest.resolve_name_at(idx) {
        if name.to_lowercase().contains(&search_term) {
          return true;
        }
      }

      if let Some(full_name) = manifest.resolve_full_name_at(idx) {
        if full_name.to_lowercase().contains(&search_term) {
          return true;
        }
      }

      false
    })
    .collect();

  if search_results.is_empty() {
    println!("No mods found matching '{}'", term);
  } else {
    println!("Found {} mods matching '{}':\n", search_results.len(), term);

    for idx in search_results {
      let version = manifest
        .get_latest_version_at(idx)
        .and_then(|ver_idx| {
          intern::resolve_option(
            &manifest.interner,
            manifest.versions.version_numbers[ver_idx],
          )
        })
        .unwrap_or_else(|| "Unknown".to_string());

      let description = manifest
        .get_latest_version_at(idx)
        .and_then(|ver_idx| {
          intern::resolve_option(&manifest.interner, manifest.versions.descriptions[ver_idx])
        })
        .unwrap_or_default();

      let name = manifest
        .resolve_name_at(idx)
        .or_else(|| manifest.resolve_full_name_at(idx))
        .unwrap_or_else(|| "Unknown".to_string());

      let owner = manifest
        .resolve_owner_at(idx)
        .unwrap_or_else(|| "Unknown".to_string());

      println!("{}-{} ({})", owner, name, version);

      if !description.is_empty() {
        println!("  {}", description);
      }

      println!();
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use mockito::Server;
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

  fn setup_mock_server(server: &mut mockito::ServerGuard) -> String {
    let last_modified = "Wed, 21 Feb 2024 15:30:45 GMT";
    server
      .mock("GET", "/c/valheim/api/v1/package/")
      .with_status(200)
      .with_header("Content-Type", "application/json")
      .with_header("Last-Modified", last_modified)
      .with_body(test_manifest_json())
      .create();
    format!("{}/c/valheim/api/v1/package/", server.url())
  }

  #[test]
  fn test_run_search_no_results() {
    let mut server = Server::new();
    let api_url = setup_mock_server(&mut server);
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run(cache_dir, "nonexistent_mod_xyz", Some(&api_url)));

    assert!(result.is_ok());
  }

  #[test]
  fn test_run_search_with_results() {
    let mut server = Server::new();
    let api_url = setup_mock_server(&mut server);
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    let rt = Runtime::new().unwrap();
    // "moda" matches "ModA" (case-insensitive) — exercises the result display loop
    let result = rt.block_on(run(cache_dir, "moda", Some(&api_url)));

    assert!(result.is_ok());
  }

  #[test]
  fn test_run_search_case_insensitive() {
    let mut server = Server::new();
    let api_url = setup_mock_server(&mut server);
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run(cache_dir, "MODA", Some(&api_url)));

    assert!(result.is_ok());
  }
}
