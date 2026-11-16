use crate::error::AppResult;
use thunderstore_engine::client::ThunderstoreClient;
use thunderstore_engine::models::PackageIndex;
use thunderstore_engine::query::{self, BrowseQuery, PackageRow, SortDirection, SortKey};

pub async fn run(client: &ThunderstoreClient, term: &str) -> AppResult<()> {
  let manifest = client.get_manifest().await?;
  let results = ranked_matches(&manifest, term);

  if results.is_empty() {
    println!("No mods found matching '{term}'");

    return Ok(());
  }

  println!("Found {} mods matching '{term}':\n", results.len());

  for idx in results {
    let Some(row) = PackageRow::new(&manifest, idx) else {
      continue;
    };

    let version = row.version_number().unwrap_or("Unknown");
    let description = row.description().unwrap_or_default();
    let identifier = package_identifier(row.full_name(), row.owner(), row.name());

    println!("{identifier} ({version})");

    if !description.is_empty() {
      println!("  {description}");
    }

    println!();
  }

  Ok(())
}

/// The indices of the packages in `index` matching `term`, best match first.
///
/// Uses the engine's best-match ranking with r2modman's defaults, so deprecated
/// and NSFW packages are left out and `valheim plus` finds `ValheimPlus`.
fn ranked_matches(index: &PackageIndex, term: &str) -> Vec<usize> {
  let query = BrowseQuery::new()
    .search(term)
    .sort(SortKey::BestMatch, SortDirection::Descending);

  query::browse(index, &query)
}

/// The `Owner-ModName` identifier to print for one search result.
///
/// `full_name` already *is* that identifier, so when the index carries one it is
/// used whole rather than treated as a half to prefix: a package indexed with
/// only a `full_name` can still match, and prefixing it with the owner would
/// print `Owner-Owner-ModName`.
///
/// Only when `full_name` is absent is the identifier composed from its two
/// halves, each falling back to `Unknown` so a partial record still prints a
/// recognisable line instead of being dropped.
fn package_identifier(full_name: Option<&str>, owner: Option<&str>, name: Option<&str>) -> String {
  match full_name {
    Some(full_name) => full_name.to_string(),
    None => format!(
      "{}-{}",
      owner.unwrap_or("Unknown"),
      name.unwrap_or("Unknown")
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::tempdir;
  use thunderstore_engine::test_support::mockito::{self, Server};
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

  fn test_client(
    server: &mut mockito::ServerGuard,
    cache_dir: &std::path::Path,
  ) -> ThunderstoreClient {
    let last_modified = "Wed, 21 Feb 2024 15:30:45 GMT";
    server
      .mock("GET", "/c/valheim/api/v1/package/")
      .with_status(200)
      .with_header("Content-Type", "application/json")
      .with_header("Last-Modified", last_modified)
      .with_body(test_manifest_json())
      .create();

    ThunderstoreClient::builder()
      .package_index_url(format!("{}/c/valheim/api/v1/package/", server.url()))
      .cache_dir(cache_dir)
      .build()
      .unwrap()
  }

  #[test]
  fn test_run_search_no_results() {
    let mut server = Server::new();
    let temp_dir = tempdir().unwrap();
    let client = test_client(&mut server, temp_dir.path());

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run(&client, "nonexistent_mod_xyz"));

    assert!(result.is_ok());
  }

  #[test]
  fn test_run_search_with_results() {
    let mut server = Server::new();
    let temp_dir = tempdir().unwrap();
    let client = test_client(&mut server, temp_dir.path());

    let rt = Runtime::new().unwrap();
    // "moda" matches "ModA" (case-insensitive), exercising the result display loop
    let result = rt.block_on(run(&client, "moda"));

    assert!(result.is_ok());
  }

  #[test]
  fn test_run_search_case_insensitive() {
    let mut server = Server::new();
    let temp_dir = tempdir().unwrap();
    let client = test_client(&mut server, temp_dir.path());

    let rt = Runtime::new().unwrap();
    let result = rt.block_on(run(&client, "MODA"));

    assert!(result.is_ok());
  }

  #[test]
  fn a_full_name_is_the_whole_identifier_not_a_half_to_prefix() {
    // The case `search` can match on `full_name` alone. Composing here would
    // print `Owner-Owner-ModA`, which is not installable and not what the index
    // holds.
    assert_eq!(
      package_identifier(Some("Owner-ModA"), Some("Owner"), None),
      "Owner-ModA"
    );
    // A complete record composes to the same string either way, so preferring
    // `full_name` changes nothing for well-formed packages.
    assert_eq!(
      package_identifier(Some("Owner-ModA"), Some("Owner"), Some("ModA")),
      "Owner-ModA"
    );
  }

  #[test]
  fn a_missing_full_name_is_composed_from_its_halves() {
    assert_eq!(
      package_identifier(None, Some("Owner"), Some("ModA")),
      "Owner-ModA"
    );
    // Neither half is guaranteed, and a partial record is still worth printing
    // rather than silently skipping.
    assert_eq!(package_identifier(None, None, Some("ModA")), "Unknown-ModA");
    assert_eq!(
      package_identifier(None, Some("Owner"), None),
      "Owner-Unknown"
    );
    assert_eq!(package_identifier(None, None, None), "Unknown-Unknown");
  }

  /// A package index built from `(name, downloads, deprecated)` rows, all owned
  /// by `Owner`, in the order given.
  fn index_of(rows: &[(&str, u32, bool)]) -> PackageIndex {
    let packages: Vec<serde_json::Value> = rows
      .iter()
      .map(|(name, downloads, deprecated)| {
        serde_json::json!({
          "name": name,
          "full_name": format!("Owner-{name}"),
          "owner": "Owner",
          "date_created": "2024-01-01T12:00:00Z",
          "date_updated": "2024-01-02T12:00:00Z",
          "is_deprecated": deprecated,
          "categories": [],
          "versions": [{
            "name": name,
            "full_name": format!("Owner-{name}"),
            "description": "",
            "version_number": "1.0.0",
            "dependencies": [],
            "downloads": downloads,
            "date_created": "2024-01-01T12:00:00Z",
          }],
        })
      })
      .collect();
    let packages: Vec<thunderstore_engine::models::Package> =
      serde_json::from_value(serde_json::Value::Array(packages)).unwrap();

    PackageIndex::from(packages)
  }

  fn matched_names(index: &PackageIndex, term: &str) -> Vec<String> {
    ranked_matches(index, term)
      .into_iter()
      .filter_map(|idx| PackageRow::new(index, idx)?.name().map(str::to_string))
      .collect()
  }

  #[test]
  fn the_exact_name_ranks_above_a_more_downloaded_partial_match() {
    let index = index_of(&[("BetterModA", 50_000, false), ("ModA", 10, false)]);

    assert_eq!(matched_names(&index, "moda"), ["ModA", "BetterModA"]);
  }

  #[test]
  fn search_words_match_a_camel_case_name() {
    let index = index_of(&[("ValheimPlus", 10, false), ("Unrelated", 10, false)]);

    assert_eq!(matched_names(&index, "valheim plus"), ["ValheimPlus"]);
  }

  #[test]
  fn a_deprecated_package_is_left_out() {
    let index = index_of(&[("ModA", 10, true), ("ModAPlus", 10, false)]);

    assert_eq!(matched_names(&index, "moda"), ["ModAPlus"]);
  }
}
