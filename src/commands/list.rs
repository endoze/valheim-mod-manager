use crate::{api, cli::ListFormat, error::AppResult, package::DependencyGraph};

pub async fn run(
  cache_dir: &str,
  mod_list: Vec<String>,
  format: &ListFormat,
  api_url: Option<&str>,
) -> AppResult<()> {
  let manifest = api::get_manifest(cache_dir, api_url).await?;
  let dg = DependencyGraph::new(mod_list);
  let urls = dg.resolve_interned(&manifest);

  let mut entries: Vec<&String> = urls.keys().collect();
  entries.sort();

  match format {
    ListFormat::Text => {
      for entry in entries {
        let name = entry.strip_suffix(".zip").unwrap_or(entry);
        if let Some((full_name, version)) = name.rsplit_once('-') {
          println!("{} {}", full_name, version);
        }
      }
    }
    ListFormat::Json => {
      let json_entries: Vec<serde_json::Value> = entries
        .iter()
        .filter_map(|entry| {
          let name = entry.strip_suffix(".zip").unwrap_or(entry);
          let (full_name, version) = name.rsplit_once('-')?;
          Some(serde_json::json!({
            "full_name": full_name,
            "version": version,
          }))
        })
        .collect();
      println!("{}", serde_json::to_string_pretty(&json_entries)?);
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
  fn test_run_list_text_format_empty_mod_list() {
    let mut server = Server::new();
    let api_url = setup_mock_server(&mut server);
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run(cache_dir, vec![], &ListFormat::Text, Some(&api_url)));

    assert!(result.is_ok());
  }

  #[test]
  fn test_run_list_text_format_with_mods() {
    let mut server = Server::new();
    let api_url = setup_mock_server(&mut server);
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();
    let mod_list = vec!["Owner-ModA".to_string()];

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run(cache_dir, mod_list, &ListFormat::Text, Some(&api_url)));

    assert!(result.is_ok());
  }

  #[test]
  fn test_run_list_json_format_with_mods() {
    let mut server = Server::new();
    let api_url = setup_mock_server(&mut server);
    let temp_dir = tempdir().unwrap();
    let cache_dir = temp_dir.path().to_str().unwrap();
    let mod_list = vec!["Owner-ModA".to_string()];

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run(cache_dir, mod_list, &ListFormat::Json, Some(&api_url)));

    assert!(result.is_ok());
  }
}
