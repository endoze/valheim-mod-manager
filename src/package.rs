use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::collections::HashMap;

/// Represents a mod package from the Thunderstore API.
///
/// This struct contains information about a mod package, including its metadata
/// and all available versions.
#[derive(Serialize, Deserialize)]
pub struct Package {
  /// The short name of the package
  pub name: Option<String>,
  /// The full name of the package, typically in format "Owner-Name"
  pub full_name: Option<String>,
  /// The username of the package owner
  pub owner: Option<String>,
  /// The URL to the package's page on Thunderstore
  pub package_url: Option<String>,
  /// When the package was first published
  #[serde(with = "time::serde::rfc3339")]
  pub date_created: time::OffsetDateTime,
  /// When the package was last updated
  #[serde(with = "time::serde::rfc3339")]
  pub date_updated: time::OffsetDateTime,
  /// Unique identifier for the package
  pub uuid4: Option<String>,
  /// User rating score for the package
  pub rating_score: Option<u32>,
  /// Whether the package is pinned by Thunderstore
  pub is_pinned: Option<bool>,
  /// Whether the package is marked as deprecated
  pub is_deprecated: Option<bool>,
  /// Whether the package contains NSFW content
  pub has_nsfw_content: Option<bool>,
  /// List of categories the package belongs to
  pub categories: Vec<String>,
  /// All available versions of the package
  pub versions: Vec<Version>,
}

/// Represents a specific version of a mod package.
///
/// This struct contains information about one version of a package,
/// including its version number, dependencies, and download information.
#[derive(Serialize, Deserialize, Debug)]
pub struct Version {
  /// The name of this version
  pub name: Option<String>,
  /// The full name of this version
  pub full_name: Option<String>,
  /// The description of this version
  pub description: Option<String>,
  /// URL to the icon for this version
  pub icon: Option<String>,
  /// The version number (e.g., "1.0.0")
  pub version_number: Option<String>,
  /// List of dependencies required by this version
  pub dependencies: Vec<String>,
  /// URL to download this version
  pub download_url: Option<String>,
  /// Number of times this version has been downloaded
  pub downloads: Option<u32>,
  /// When this version was published
  #[serde(with = "time::serde::rfc3339")]
  pub date_created: time::OffsetDateTime,
  /// URL to the website for this version
  pub website_url: Option<String>,
  /// Whether this version is active
  pub is_active: Option<bool>,
  /// Unique identifier for this version
  pub uuid4: Option<String>,
  /// Size of the download file in bytes
  pub file_size: Option<u64>,
}

impl Package {
  /// Returns the latest version of the package based on creation date.
  ///
  /// # Returns
  ///
  /// The most recent version of the package, or `None` if there are no versions.
  pub fn latest_version(&self) -> Option<&Version> {
    self
      .versions
      .iter()
      .max_by(|a, b| a.date_created.partial_cmp(&b.date_created).unwrap())
  }

  /// Constructs a filename and download URL for the latest version of the package.
  ///
  /// # Returns
  ///
  /// A tuple containing:
  /// - The formatted zip filename (e.g., "Owner-PackageName-1.0.0.zip")
  /// - The download URL
  ///
  /// Returns `None` if any required information is missing (latest version, download URL, etc.)
  pub fn zip_and_url(&self) -> Option<(String, String)> {
    let pkg = self.latest_version()?;
    let url: String = pkg.download_url.clone()?;
    let version_number = pkg.version_number.as_ref()?;
    let package_name = &self.full_name.as_ref()?;
    let zip_name = format!("{}-{}.zip", package_name, version_number);

    Some((zip_name, url))
  }
}

/// Custom debug implementation for Package to show relevant information.
impl std::fmt::Debug for Package {
  /// Formats the Package for debugging, including key information like name and dependencies.
  ///
  /// # Parameters
  ///
  /// * `f` - The formatter to write to
  ///
  /// # Returns
  ///
  /// A Result indicating whether the formatting succeeded
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Package")
      .field("full_name", &self.full_name)
      .field(
        "dependencies",
        &self.latest_version().map(|v| &v.dependencies),
      )
      .field(
        "download_url",
        &self.latest_version().map(|v| &v.download_url),
      )
      .finish()
  }
}

/// Represents a graph of package dependencies to be resolved and installed.
///
/// This structure helps determine which packages need to be installed,
/// including any dependencies they require.
#[derive(Debug, Deserialize, Serialize)]
pub struct DependencyGraph {
  /// The list of package names that the user wants to install
  install_packages: Vec<String>,
}

impl DependencyGraph {
  /// Creates a new DependencyGraph with the specified packages to install.
  ///
  /// # Parameters
  ///
  /// * `install_packages` - List of package names to install
  ///
  /// # Returns
  ///
  /// A new `DependencyGraph` instance
  pub fn new(install_packages: Vec<String>) -> Self {
    Self { install_packages }
  }

  /// Resolves all dependencies for the requested packages.
  ///
  /// This method:
  /// 1. Starts with the user-requested packages
  /// 2. For each package, adds its dependencies to the list to process
  /// 3. Continues until all dependencies are resolved
  ///
  /// # Parameters
  ///
  /// * `manifest` - A HashMap of all available packages
  ///
  /// # Returns
  ///
  /// A HashMap where keys are filenames and values are download URLs for all required packages
  pub fn resolve(&self, manifest: &HashMap<String, Package>) -> HashMap<String, String> {
    let mut sorted_set: BTreeSet<&str> = self.install_packages.iter().map(String::as_str).collect();
    let mut install = HashMap::new();

    tracing::debug!("Starting with mod list of: {:#?}", sorted_set);

    while !sorted_set.is_empty() {
      let Some(item) = sorted_set.pop_first() else {
        continue;
      };

      let item = item.split('-').take(2).collect::<Vec<&str>>().join("-");
      let Some(package) = manifest.get(&item) else {
        continue;
      };

      if let Some(version) = package.latest_version() {
        version.dependencies.iter().for_each(|dep| {
          let dep_str = dep.as_str();

          tracing::debug!(
            "Found dependency of {:#?} for {:#?} mod",
            dep_str,
            package.name.as_ref().unwrap(),
          );
          sorted_set.insert(dep_str);
        });
      }

      install.insert(item, package);
    }

    Self::packages_and_urls(install)
  }

  /// Converts a HashMap of packages to a HashMap of filenames and download URLs.
  ///
  /// # Parameters
  ///
  /// * `results` - HashMap of package names and references
  ///
  /// # Returns
  ///
  /// A HashMap mapping filenames to download URLs
  fn packages_and_urls(results: HashMap<String, &Package>) -> HashMap<String, String> {
    results
      .iter()
      .filter_map(|(_, pkg)| pkg.zip_and_url())
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use time::OffsetDateTime;

  fn create_test_package(name: &str, owner: &str, version: &str) -> Package {
    let full_name = format!("{}-{}", owner, name);
    Package {
      name: Some(name.to_string()),
      full_name: Some(full_name.clone()),
      owner: Some(owner.to_string()),
      package_url: Some(format!("https://example.com/{}", name)),
      date_created: OffsetDateTime::now_utc(),
      date_updated: OffsetDateTime::now_utc(),
      uuid4: Some("test-uuid".to_string()),
      rating_score: Some(5),
      is_pinned: Some(false),
      is_deprecated: Some(false),
      has_nsfw_content: Some(false),
      categories: vec!["category1".to_string()],
      versions: vec![Version {
        name: Some(name.to_string()),
        full_name: Some(full_name.clone()),
        description: Some("Test description".to_string()),
        icon: Some("icon.png".to_string()),
        version_number: Some(version.to_string()),
        dependencies: vec![],
        download_url: Some(format!("https://example.com/{}/download", name)),
        downloads: Some(100),
        date_created: OffsetDateTime::now_utc(),
        website_url: Some("https://example.com".to_string()),
        is_active: Some(true),
        uuid4: Some("test-version-uuid".to_string()),
        file_size: Some(1024),
      }],
    }
  }

  fn create_test_package_with_dependencies(
    name: &str,
    owner: &str,
    version: &str,
    dependencies: Vec<String>,
  ) -> Package {
    let mut pkg = create_test_package(name, owner, version);
    if let Some(latest_version) = pkg.versions.first_mut() {
      latest_version.dependencies = dependencies;
    }
    pkg
  }

  #[test]
  fn test_latest_version() {
    let mut pkg = create_test_package("TestMod", "TestOwner", "1.0.0");

    let older_version = Version {
      name: Some("TestMod".to_string()),
      full_name: Some("TestOwner-TestMod".to_string()),
      description: Some("Older version".to_string()),
      icon: Some("icon.png".to_string()),
      version_number: Some("0.9.0".to_string()),
      dependencies: vec![],
      download_url: Some("https://example.com/TestMod/download-old".to_string()),
      downloads: Some(50),
      date_created: OffsetDateTime::now_utc().saturating_sub(time::Duration::days(30)),
      website_url: Some("https://example.com".to_string()),
      is_active: Some(true),
      uuid4: Some("old-version-uuid".to_string()),
      file_size: Some(512),
    };
    pkg.versions.push(older_version);

    let newer_version = Version {
      name: Some("TestMod".to_string()),
      full_name: Some("TestOwner-TestMod".to_string()),
      description: Some("Newer version".to_string()),
      icon: Some("icon.png".to_string()),
      version_number: Some("1.1.0".to_string()),
      dependencies: vec![],
      download_url: Some("https://example.com/TestMod/download-new".to_string()),
      downloads: Some(150),
      date_created: OffsetDateTime::now_utc().saturating_add(time::Duration::days(30)),
      website_url: Some("https://example.com".to_string()),
      is_active: Some(true),
      uuid4: Some("new-version-uuid".to_string()),
      file_size: Some(2048),
    };
    pkg.versions.push(newer_version);

    let latest = pkg.latest_version().unwrap();
    assert_eq!(latest.version_number, Some("1.1.0".to_string()));
    assert_eq!(latest.description, Some("Newer version".to_string()));
  }

  #[test]
  fn test_zip_and_url() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let (filename, url) = pkg.zip_and_url().unwrap();

    assert_eq!(filename, "TestOwner-TestMod-1.0.0.zip");
    assert_eq!(url, "https://example.com/TestMod/download");
  }

  #[test]
  fn test_zip_and_url_missing_data() {
    let mut pkg = create_test_package("TestMod", "TestOwner", "1.0.0");

    if let Some(version) = pkg.versions.first_mut() {
      version.version_number = None;
    }

    assert!(pkg.zip_and_url().is_none());

    let mut pkg = create_test_package("TestMod", "TestOwner", "1.0.0");

    if let Some(version) = pkg.versions.first_mut() {
      version.download_url = None;
    }

    assert!(pkg.zip_and_url().is_none());

    let mut pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    pkg.full_name = None;

    assert!(pkg.zip_and_url().is_none());
  }

  #[test]
  fn test_dependency_graph_resolve() {
    let mut manifest = HashMap::new();

    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");
    let pkg2 = create_test_package_with_dependencies(
      "ModB",
      "Owner2",
      "2.0.0",
      vec!["Owner3-ModC".to_string()],
    );
    let pkg3 = create_test_package_with_dependencies(
      "ModC",
      "Owner3",
      "1.5.0",
      vec!["Owner4-ModD".to_string()],
    );
    let pkg4 = create_test_package("ModD", "Owner4", "0.9.0");

    manifest.insert("Owner1-ModA".to_string(), pkg1);
    manifest.insert("Owner2-ModB".to_string(), pkg2);
    manifest.insert("Owner3-ModC".to_string(), pkg3);
    manifest.insert("Owner4-ModD".to_string(), pkg4);

    let dg1 = DependencyGraph::new(vec!["Owner1-ModA".to_string()]);
    let result1 = dg1.resolve(&manifest);
    assert_eq!(result1.len(), 1);
    assert!(result1.contains_key("Owner1-ModA-1.0.0.zip"));

    let dg2 = DependencyGraph::new(vec!["Owner2-ModB".to_string()]);
    let result2 = dg2.resolve(&manifest);
    assert_eq!(result2.len(), 3);
    assert!(result2.contains_key("Owner2-ModB-2.0.0.zip"));
    assert!(result2.contains_key("Owner3-ModC-1.5.0.zip"));
    assert!(result2.contains_key("Owner4-ModD-0.9.0.zip"));

    let dg3 = DependencyGraph::new(vec!["Owner1-ModA".to_string(), "Owner3-ModC".to_string()]);
    let result3 = dg3.resolve(&manifest);
    assert_eq!(result3.len(), 3);
    assert!(result3.contains_key("Owner1-ModA-1.0.0.zip"));
    assert!(result3.contains_key("Owner3-ModC-1.5.0.zip"));
    assert!(result3.contains_key("Owner4-ModD-0.9.0.zip"));

    let dg4 = DependencyGraph::new(vec!["Owner5-ModE".to_string()]);
    let result4 = dg4.resolve(&manifest);
    assert_eq!(result4.len(), 0);
  }

  #[test]
  fn test_package_debug() {
    let pkg = create_test_package("ModTest", "OwnerTest", "1.2.3");
    let debug_output = format!("{:?}", pkg);

    assert!(debug_output.contains("Package"));
    assert!(debug_output.contains("full_name: Some(\"OwnerTest-ModTest\")"));
    assert!(debug_output.contains("dependencies: Some([])"));
    assert!(
      debug_output.contains("download_url: Some(Some(\"https://example.com/ModTest/download\"))")
    );

    let mut pkg_with_deps = create_test_package_with_dependencies(
      "ModWithDeps",
      "OwnerTest",
      "2.0.0",
      vec!["Dep1-Mod1".to_string(), "Dep2-Mod2".to_string()],
    );
    let debug_with_deps = format!("{:?}", pkg_with_deps);

    assert!(debug_with_deps.contains("dependencies: Some([\"Dep1-Mod1\", \"Dep2-Mod2\"])"));

    pkg_with_deps.full_name = None;
    let debug_missing_name = format!("{:?}", pkg_with_deps);
    assert!(debug_missing_name.contains("full_name: None"));

    let mut pkg_no_version = create_test_package("NoVersion", "TestOwner", "1.0.0");
    pkg_no_version.versions.clear();
    let debug_no_version = format!("{:?}", pkg_no_version);

    assert!(debug_no_version.contains("dependencies: None"));
    assert!(debug_no_version.contains("download_url: None"));
  }
}
