use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::collections::HashMap;
use time::OffsetDateTime;

/// Struct-of-Arrays representation of the package manifest for efficient storage.
///
/// This structure organizes package data in a columnar format to minimize duplication
/// and improve serialization efficiency. All packages are stored with parallel vectors,
/// and version data is stored separately with indices linking back to packages.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackageManifest {
  pub names: Vec<Option<String>>,
  pub full_names: Vec<Option<String>>,
  pub owners: Vec<Option<String>>,
  pub package_urls: Vec<Option<String>>,
  pub dates_created: Vec<OffsetDateTime>,
  pub dates_updated: Vec<OffsetDateTime>,
  pub uuid4s: Vec<Option<String>>,
  pub rating_scores: Vec<Option<u32>>,
  pub is_pinned: Vec<Option<bool>>,
  pub is_deprecated: Vec<Option<bool>>,
  pub has_nsfw_content: Vec<Option<bool>>,
  pub categories: Vec<Vec<String>>,
  pub version_ranges: Vec<(usize, usize)>,
  pub versions: VersionManifest,
}

/// Struct-of-Arrays representation of version data.
///
/// Separates frequently accessed "hot" data from rarely accessed "cold" metadata
/// for better cache performance and memory efficiency.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionManifest {
  pub package_indices: Vec<usize>,
  pub version_numbers: Vec<Option<String>>,
  pub download_urls: Vec<Option<String>>,
  pub dependencies: Vec<Vec<String>>,
  pub dates_created: Vec<OffsetDateTime>,
  pub descriptions: Vec<Option<String>>,
  pub icons: Vec<Option<String>>,
  pub downloads: Vec<Option<u32>>,
  pub website_urls: Vec<Option<String>>,
  pub is_active: Vec<Option<bool>>,
  pub uuid4s: Vec<Option<String>>,
  pub file_sizes: Vec<Option<u64>>,
}

impl PackageManifest {
  pub fn len(&self) -> usize {
    self.names.len()
  }

  #[allow(dead_code)]
  pub fn is_empty(&self) -> bool {
    self.names.is_empty()
  }

  pub fn validate(&self) -> Result<(), String> {
    let pkg_count = self.names.len();

    if self.full_names.len() != pkg_count {
      return Err(format!(
        "full_names length {} != {}",
        self.full_names.len(),
        pkg_count
      ));
    }

    if self.owners.len() != pkg_count {
      return Err(format!(
        "owners length {} != {}",
        self.owners.len(),
        pkg_count
      ));
    }

    if self.package_urls.len() != pkg_count {
      return Err(format!(
        "package_urls length {} != {}",
        self.package_urls.len(),
        pkg_count
      ));
    }

    if self.dates_created.len() != pkg_count {
      return Err(format!(
        "dates_created length {} != {}",
        self.dates_created.len(),
        pkg_count
      ));
    }

    if self.dates_updated.len() != pkg_count {
      return Err(format!(
        "dates_updated length {} != {}",
        self.dates_updated.len(),
        pkg_count
      ));
    }

    if self.uuid4s.len() != pkg_count {
      return Err(format!(
        "uuid4s length {} != {}",
        self.uuid4s.len(),
        pkg_count
      ));
    }

    if self.rating_scores.len() != pkg_count {
      return Err(format!(
        "rating_scores length {} != {}",
        self.rating_scores.len(),
        pkg_count
      ));
    }

    if self.is_pinned.len() != pkg_count {
      return Err(format!(
        "is_pinned length {} != {}",
        self.is_pinned.len(),
        pkg_count
      ));
    }

    if self.is_deprecated.len() != pkg_count {
      return Err(format!(
        "is_deprecated length {} != {}",
        self.is_deprecated.len(),
        pkg_count
      ));
    }

    if self.has_nsfw_content.len() != pkg_count {
      return Err(format!(
        "has_nsfw_content length {} != {}",
        self.has_nsfw_content.len(),
        pkg_count
      ));
    }

    if self.categories.len() != pkg_count {
      return Err(format!(
        "categories length {} != {}",
        self.categories.len(),
        pkg_count
      ));
    }

    if self.version_ranges.len() != pkg_count {
      return Err(format!(
        "version_ranges length {} != {}",
        self.version_ranges.len(),
        pkg_count
      ));
    }

    let version_count = self.versions.version_numbers.len();

    if self.versions.package_indices.len() != version_count {
      return Err(format!(
        "versions.package_indices length {} != {}",
        self.versions.package_indices.len(),
        version_count
      ));
    }

    if self.versions.download_urls.len() != version_count {
      return Err(format!(
        "versions.download_urls length {} != {}",
        self.versions.download_urls.len(),
        version_count
      ));
    }

    if self.versions.dependencies.len() != version_count {
      return Err(format!(
        "versions.dependencies length {} != {}",
        self.versions.dependencies.len(),
        version_count
      ));
    }

    if self.versions.dates_created.len() != version_count {
      return Err(format!(
        "versions.dates_created length {} != {}",
        self.versions.dates_created.len(),
        version_count
      ));
    }

    if self.versions.descriptions.len() != version_count {
      return Err(format!(
        "versions.descriptions length {} != {}",
        self.versions.descriptions.len(),
        version_count
      ));
    }

    if self.versions.icons.len() != version_count {
      return Err(format!(
        "versions.icons length {} != {}",
        self.versions.icons.len(),
        version_count
      ));
    }

    if self.versions.downloads.len() != version_count {
      return Err(format!(
        "versions.downloads length {} != {}",
        self.versions.downloads.len(),
        version_count
      ));
    }

    if self.versions.website_urls.len() != version_count {
      return Err(format!(
        "versions.website_urls length {} != {}",
        self.versions.website_urls.len(),
        version_count
      ));
    }

    if self.versions.is_active.len() != version_count {
      return Err(format!(
        "versions.is_active length {} != {}",
        self.versions.is_active.len(),
        version_count
      ));
    }

    if self.versions.uuid4s.len() != version_count {
      return Err(format!(
        "versions.uuid4s length {} != {}",
        self.versions.uuid4s.len(),
        version_count
      ));
    }

    if self.versions.file_sizes.len() != version_count {
      return Err(format!(
        "versions.file_sizes length {} != {}",
        self.versions.file_sizes.len(),
        version_count
      ));
    }

    for (idx, (start, end)) in self.version_ranges.iter().enumerate() {
      if start > end {
        return Err(format!(
          "Invalid version range at package {}: {} > {}",
          idx, start, end
        ));
      }

      if *end > version_count {
        return Err(format!(
          "Version range at package {} ends at {} but only {} versions exist",
          idx, end, version_count
        ));
      }
    }

    Ok(())
  }

  #[allow(dead_code)]
  pub fn get_package_by_full_name(&self, full_name: &str) -> Option<Package> {
    let idx = self.find_index_by_full_name(full_name)?;
    Some(self.get_package_at(idx))
  }

  #[allow(dead_code)]
  pub fn find_index_by_full_name(&self, full_name: &str) -> Option<usize> {
    self
      .full_names
      .iter()
      .position(|name| name.as_ref().map(|n| n.as_str()) == Some(full_name))
  }

  #[allow(dead_code)]
  pub fn get_package_at(&self, idx: usize) -> Package {
    let (version_start, version_end) = self.version_ranges[idx];

    let mut versions = Vec::with_capacity(version_end - version_start);

    for ver_idx in version_start..version_end {
      versions.push(Version {
        name: self.names[idx].clone(),
        full_name: self.full_names[idx].clone(),
        description: self.versions.descriptions[ver_idx].clone(),
        icon: self.versions.icons[ver_idx].clone(),
        version_number: self.versions.version_numbers[ver_idx].clone(),
        dependencies: self.versions.dependencies[ver_idx].clone(),
        download_url: self.versions.download_urls[ver_idx].clone(),
        downloads: self.versions.downloads[ver_idx],
        date_created: self.versions.dates_created[ver_idx],
        website_url: self.versions.website_urls[ver_idx].clone(),
        is_active: self.versions.is_active[ver_idx],
        uuid4: self.versions.uuid4s[ver_idx].clone(),
        file_size: self.versions.file_sizes[ver_idx],
      });
    }

    Package {
      name: self.names[idx].clone(),
      full_name: self.full_names[idx].clone(),
      owner: self.owners[idx].clone(),
      package_url: self.package_urls[idx].clone(),
      date_created: self.dates_created[idx],
      date_updated: self.dates_updated[idx],
      uuid4: self.uuid4s[idx].clone(),
      rating_score: self.rating_scores[idx],
      is_pinned: self.is_pinned[idx],
      is_deprecated: self.is_deprecated[idx],
      has_nsfw_content: self.has_nsfw_content[idx],
      categories: self.categories[idx].clone(),
      versions,
    }
  }

  pub fn get_latest_version_at(&self, idx: usize) -> Option<usize> {
    let (ver_start, ver_end) = self.version_ranges[idx];

    if ver_start >= ver_end {
      return None;
    }

    (ver_start..ver_end).max_by_key(|&ver_idx| self.versions.dates_created[ver_idx])
  }

  pub fn build_name_index(&self) -> HashMap<String, usize> {
    self
      .full_names
      .iter()
      .enumerate()
      .filter_map(|(idx, name)| Some((name.as_ref()?.clone(), idx)))
      .collect()
  }
}

impl VersionManifest {}

/// Represents a mod package from the Thunderstore API.
///
/// This struct contains information about a mod package, including its metadata
/// and all available versions.
#[derive(Serialize, Deserialize, Clone)]
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
#[derive(Serialize, Deserialize, Debug, Clone)]
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
  #[cfg(test)]
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
  /// * `manifest` - A PackageManifest of all available packages
  ///
  /// # Returns
  ///
  /// A HashMap where keys are filenames and values are download URLs for all required packages
  pub fn resolve(&self, manifest: &PackageManifest) -> HashMap<String, String> {
    let name_index = manifest.build_name_index();
    let mut sorted_set: BTreeSet<&str> = self.install_packages.iter().map(String::as_str).collect();
    let mut install_indices = Vec::new();

    tracing::debug!("Starting with mod list of: {:#?}", sorted_set);

    while !sorted_set.is_empty() {
      let Some(item) = sorted_set.pop_first() else {
        continue;
      };

      let item = item.split('-').take(2).collect::<Vec<&str>>().join("-");
      let Some(&pkg_idx) = name_index.get(&item) else {
        continue;
      };

      if let Some(latest_ver_idx) = manifest.get_latest_version_at(pkg_idx) {
        manifest.versions.dependencies[latest_ver_idx]
          .iter()
          .for_each(|dep| {
            let dep_str = dep.as_str();

            tracing::debug!(
              "Found dependency of {:#?} for {:#?} mod",
              dep_str,
              manifest.names[pkg_idx].as_ref().unwrap(),
            );
            sorted_set.insert(dep_str);
          });
      }

      install_indices.push(pkg_idx);
    }

    Self::package_indices_to_urls(manifest, &install_indices)
  }

  /// Converts package indices to a HashMap of filenames and download URLs.
  ///
  /// # Parameters
  ///
  /// * `manifest` - The PackageManifest
  /// * `indices` - Vector of package indices
  ///
  /// # Returns
  ///
  /// A HashMap mapping filenames to download URLs
  fn package_indices_to_urls(
    manifest: &PackageManifest,
    indices: &[usize],
  ) -> HashMap<String, String> {
    indices
      .iter()
      .filter_map(|&idx| {
        let latest_ver_idx = manifest.get_latest_version_at(idx)?;
        let url = manifest.versions.download_urls[latest_ver_idx].as_ref()?;
        let version_number = manifest.versions.version_numbers[latest_ver_idx].as_ref()?;
        let package_name = manifest.full_names[idx].as_ref()?;
        let zip_name = format!("{}-{}.zip", package_name, version_number);

        Some((zip_name, url.clone()))
      })
      .collect()
  }
}

impl From<Vec<Package>> for PackageManifest {
  fn from(packages: Vec<Package>) -> Self {
    let num_packages = packages.len();

    let total_versions: usize = packages.iter().map(|p| p.versions.len()).sum();

    let mut names = Vec::with_capacity(num_packages);
    let mut full_names = Vec::with_capacity(num_packages);
    let mut owners = Vec::with_capacity(num_packages);
    let mut package_urls = Vec::with_capacity(num_packages);
    let mut dates_created = Vec::with_capacity(num_packages);
    let mut dates_updated = Vec::with_capacity(num_packages);
    let mut uuid4s = Vec::with_capacity(num_packages);
    let mut rating_scores = Vec::with_capacity(num_packages);
    let mut is_pinned = Vec::with_capacity(num_packages);
    let mut is_deprecated = Vec::with_capacity(num_packages);
    let mut has_nsfw_content = Vec::with_capacity(num_packages);
    let mut categories = Vec::with_capacity(num_packages);
    let mut version_ranges = Vec::with_capacity(num_packages);

    let mut package_indices = Vec::with_capacity(total_versions);
    let mut version_numbers = Vec::with_capacity(total_versions);
    let mut download_urls = Vec::with_capacity(total_versions);
    let mut dependencies = Vec::with_capacity(total_versions);
    let mut version_dates_created = Vec::with_capacity(total_versions);
    let mut descriptions = Vec::with_capacity(total_versions);
    let mut icons = Vec::with_capacity(total_versions);
    let mut downloads = Vec::with_capacity(total_versions);
    let mut website_urls = Vec::with_capacity(total_versions);
    let mut is_active = Vec::with_capacity(total_versions);
    let mut version_uuid4s = Vec::with_capacity(total_versions);
    let mut file_sizes = Vec::with_capacity(total_versions);

    let mut version_offset = 0;

    for (pkg_idx, package) in packages.into_iter().enumerate() {
      names.push(package.name);
      full_names.push(package.full_name);
      owners.push(package.owner);
      package_urls.push(package.package_url);
      dates_created.push(package.date_created);
      dates_updated.push(package.date_updated);
      uuid4s.push(package.uuid4);
      rating_scores.push(package.rating_score);
      is_pinned.push(package.is_pinned);
      is_deprecated.push(package.is_deprecated);
      has_nsfw_content.push(package.has_nsfw_content);
      categories.push(package.categories);

      let version_count = package.versions.len();
      version_ranges.push((version_offset, version_offset + version_count));

      for version in package.versions {
        package_indices.push(pkg_idx);
        version_numbers.push(version.version_number);
        download_urls.push(version.download_url);
        dependencies.push(version.dependencies);
        version_dates_created.push(version.date_created);
        descriptions.push(version.description);
        icons.push(version.icon);
        downloads.push(version.downloads);
        website_urls.push(version.website_url);
        is_active.push(version.is_active);
        version_uuid4s.push(version.uuid4);
        file_sizes.push(version.file_size);
      }

      version_offset += version_count;
    }

    PackageManifest {
      names,
      full_names,
      owners,
      package_urls,
      dates_created,
      dates_updated,
      uuid4s,
      rating_scores,
      is_pinned,
      is_deprecated,
      has_nsfw_content,
      categories,
      version_ranges,
      versions: VersionManifest {
        package_indices,
        version_numbers,
        download_urls,
        dependencies,
        dates_created: version_dates_created,
        descriptions,
        icons,
        downloads,
        website_urls,
        is_active,
        uuid4s: version_uuid4s,
        file_sizes,
      },
    }
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

    let packages = vec![pkg1, pkg2, pkg3, pkg4];
    let manifest: PackageManifest = packages.into();

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

  #[test]
  fn test_package_to_manifest_conversion() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");
    let pkg2 = create_test_package_with_dependencies(
      "ModB",
      "Owner2",
      "2.0.0",
      vec!["Owner3-ModC".to_string()],
    );

    let packages = vec![pkg1, pkg2];
    let manifest: PackageManifest = packages.clone().into();

    assert_eq!(manifest.len(), 2);
    assert_eq!(manifest.names[0], Some("ModA".to_string()));
    assert_eq!(manifest.names[1], Some("ModB".to_string()));
    assert_eq!(manifest.full_names[0], Some("Owner1-ModA".to_string()));
    assert_eq!(manifest.full_names[1], Some("Owner2-ModB".to_string()));

    assert_eq!(manifest.versions.version_numbers.len(), 2);

    assert_eq!(manifest.version_ranges[0], (0, 1));
    assert_eq!(manifest.version_ranges[1], (1, 2));

    assert_eq!(
      manifest.versions.version_numbers[0],
      Some("1.0.0".to_string())
    );
    assert_eq!(
      manifest.versions.version_numbers[1],
      Some("2.0.0".to_string())
    );

    assert_eq!(manifest.versions.dependencies[0], Vec::<String>::new());
    assert_eq!(
      manifest.versions.dependencies[1],
      vec!["Owner3-ModC".to_string()]
    );
  }

  #[test]
  fn test_manifest_get_package_methods() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");
    let pkg2 = create_test_package_with_dependencies(
      "ModB",
      "Owner2",
      "2.0.0",
      vec!["Owner3-ModC".to_string()],
    );

    let original_packages = vec![pkg1, pkg2];
    let manifest: PackageManifest = original_packages.clone().into();

    assert_eq!(manifest.len(), 2);

    let pkg_a = manifest.get_package_by_full_name("Owner1-ModA").unwrap();
    assert_eq!(pkg_a.name, Some("ModA".to_string()));
    assert_eq!(pkg_a.owner, Some("Owner1".to_string()));
    assert_eq!(pkg_a.versions.len(), 1);
    assert_eq!(pkg_a.versions[0].version_number, Some("1.0.0".to_string()));

    let pkg_b = manifest.get_package_by_full_name("Owner2-ModB").unwrap();
    assert_eq!(pkg_b.name, Some("ModB".to_string()));
    assert_eq!(pkg_b.owner, Some("Owner2".to_string()));
    assert_eq!(pkg_b.versions.len(), 1);
    assert_eq!(pkg_b.versions[0].version_number, Some("2.0.0".to_string()));
    assert_eq!(
      pkg_b.versions[0].dependencies,
      vec!["Owner3-ModC".to_string()]
    );

    let idx = manifest.find_index_by_full_name("Owner1-ModA").unwrap();
    assert_eq!(manifest.names[idx], Some("ModA".to_string()));
  }

  #[test]
  fn test_round_trip_conversion() {
    let mut pkg = create_test_package("TestMod", "TestOwner", "1.0.0");

    pkg.versions.push(Version {
      name: Some("TestMod".to_string()),
      full_name: Some("TestOwner-TestMod".to_string()),
      description: Some("Version 2".to_string()),
      icon: Some("icon2.png".to_string()),
      version_number: Some("2.0.0".to_string()),
      dependencies: vec!["Dep1-Mod1".to_string()],
      download_url: Some("https://example.com/TestMod/download-v2".to_string()),
      downloads: Some(200),
      date_created: OffsetDateTime::now_utc(),
      website_url: Some("https://example.com".to_string()),
      is_active: Some(true),
      uuid4: Some("test-version-uuid-2".to_string()),
      file_size: Some(2048),
    });

    let original_full_name = pkg.full_name.clone().unwrap();
    let original_version_count = pkg.versions.len();

    let packages = vec![pkg];
    let manifest: PackageManifest = packages.into();

    let reconstructed = manifest
      .get_package_by_full_name(&original_full_name)
      .unwrap();
    assert_eq!(reconstructed.full_name, Some(original_full_name));
    assert_eq!(reconstructed.versions.len(), original_version_count);
    assert_eq!(
      reconstructed.versions[0].version_number,
      Some("1.0.0".to_string())
    );
    assert_eq!(
      reconstructed.versions[1].version_number,
      Some("2.0.0".to_string())
    );
    assert_eq!(
      reconstructed.versions[1].dependencies,
      vec!["Dep1-Mod1".to_string()]
    );
  }

  #[test]
  fn test_validate_valid_manifest() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let manifest: PackageManifest = packages.into();

    assert!(manifest.validate().is_ok());
  }

  #[test]
  fn test_validate_mismatched_full_names_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.full_names.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("full_names length"));
  }

  #[test]
  fn test_validate_mismatched_owners_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.owners.push(Some("Extra".to_string()));

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("owners length"));
  }

  #[test]
  fn test_validate_invalid_version_range() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.version_ranges[0] = (5, 3);

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid version range"));
  }

  #[test]
  fn test_validate_version_range_out_of_bounds() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.version_ranges[0] = (0, 999);

    let result = manifest.validate();
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("only"));
    assert!(err_msg.contains("versions exist"));
  }

  #[test]
  fn test_validate_mismatched_version_arrays() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.download_urls.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(
      result
        .unwrap_err()
        .contains("versions.download_urls length")
    );
  }

  #[test]
  fn test_validate_empty_manifest() {
    let manifest = PackageManifest {
      names: vec![],
      full_names: vec![],
      owners: vec![],
      package_urls: vec![],
      dates_created: vec![],
      dates_updated: vec![],
      uuid4s: vec![],
      rating_scores: vec![],
      is_pinned: vec![],
      is_deprecated: vec![],
      has_nsfw_content: vec![],
      categories: vec![],
      version_ranges: vec![],
      versions: VersionManifest {
        package_indices: vec![],
        version_numbers: vec![],
        download_urls: vec![],
        dependencies: vec![],
        dates_created: vec![],
        descriptions: vec![],
        icons: vec![],
        downloads: vec![],
        website_urls: vec![],
        is_active: vec![],
        uuid4s: vec![],
        file_sizes: vec![],
      },
    };

    assert!(manifest.validate().is_ok());
    assert!(manifest.is_empty());
  }

  #[test]
  fn test_validate_mismatched_package_urls_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.package_urls.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("package_urls length"));
  }

  #[test]
  fn test_validate_mismatched_dates_created_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.dates_created.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("dates_created length"));
  }

  #[test]
  fn test_validate_mismatched_dates_updated_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.dates_updated.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("dates_updated length"));
  }

  #[test]
  fn test_validate_mismatched_uuid4s_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.uuid4s.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("uuid4s length"));
  }

  #[test]
  fn test_validate_mismatched_rating_scores_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.rating_scores.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("rating_scores length"));
  }

  #[test]
  fn test_validate_mismatched_is_pinned_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.is_pinned.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("is_pinned length"));
  }

  #[test]
  fn test_validate_mismatched_is_deprecated_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.is_deprecated.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("is_deprecated length"));
  }

  #[test]
  fn test_validate_mismatched_has_nsfw_content_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.has_nsfw_content.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("has_nsfw_content length"));
  }

  #[test]
  fn test_validate_mismatched_categories_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.categories.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("categories length"));
  }

  #[test]
  fn test_validate_mismatched_version_ranges_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.version_ranges.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("version_ranges length"));
  }

  #[test]
  fn test_validate_mismatched_version_package_indices_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.package_indices.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(
      result
        .unwrap_err()
        .contains("versions.package_indices length")
    );
  }

  #[test]
  fn test_validate_mismatched_version_dependencies_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.dependencies.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(
      result
        .unwrap_err()
        .contains("versions.dependencies length")
    );
  }

  #[test]
  fn test_validate_mismatched_version_dates_created_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.dates_created.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(
      result
        .unwrap_err()
        .contains("versions.dates_created length")
    );
  }

  #[test]
  fn test_validate_mismatched_version_descriptions_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.descriptions.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(
      result
        .unwrap_err()
        .contains("versions.descriptions length")
    );
  }

  #[test]
  fn test_validate_mismatched_version_icons_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.icons.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("versions.icons length"));
  }

  #[test]
  fn test_validate_mismatched_version_downloads_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.downloads.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("versions.downloads length"));
  }

  #[test]
  fn test_validate_mismatched_version_website_urls_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.website_urls.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(
      result
        .unwrap_err()
        .contains("versions.website_urls length")
    );
  }

  #[test]
  fn test_validate_mismatched_version_is_active_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.is_active.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("versions.is_active length"));
  }

  #[test]
  fn test_validate_mismatched_version_uuid4s_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.uuid4s.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("versions.uuid4s length"));
  }

  #[test]
  fn test_validate_mismatched_version_file_sizes_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.versions.file_sizes.pop();

    let result = manifest.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("versions.file_sizes length"));
  }

  #[test]
  fn test_get_latest_version_at_empty_range() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut manifest: PackageManifest = packages.into();

    manifest.version_ranges[0] = (0, 0);

    let result = manifest.get_latest_version_at(0);
    assert!(result.is_none());
  }

  #[test]
  fn test_dependency_graph_resolve_with_missing_dependency() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");
    let pkg2 = create_test_package_with_dependencies(
      "ModB",
      "Owner2",
      "2.0.0",
      vec!["Owner3-NonExistent".to_string()],
    );

    let packages = vec![pkg1, pkg2];
    let manifest: PackageManifest = packages.into();

    let dg = DependencyGraph::new(vec!["Owner2-ModB".to_string()]);
    let result = dg.resolve(&manifest);

    assert_eq!(result.len(), 1);
    assert!(result.contains_key("Owner2-ModB-2.0.0.zip"));
  }

  #[test]
  fn test_dependency_graph_resolve_with_version_suffix() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");

    let packages = vec![pkg1];
    let manifest: PackageManifest = packages.into();

    let dg = DependencyGraph::new(vec!["Owner1-ModA-1.0.0".to_string()]);
    let result = dg.resolve(&manifest);

    assert_eq!(result.len(), 1);
    assert!(result.contains_key("Owner1-ModA-1.0.0.zip"));
  }
}
