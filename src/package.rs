use crate::intern::{
  InternKey, StringInterner, intern_option, intern_vec, resolve_option, resolve_vec,
};
use lasso::Key;
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

/// Struct-of-Arrays representation of the package manifest with interned strings.
///
/// This structure builds upon the PackageManifest format by adding string interning
/// to further reduce memory usage and improve serialization efficiency. Repeated strings
/// like author names, categories, and URLs are stored once in the interner and referenced
/// by integer keys throughout the structure.
///
/// String interning is particularly beneficial for the Valheim mod ecosystem where the same
/// authors maintain multiple packages and common patterns appear frequently in metadata.
#[derive(Debug, Clone)]
pub struct InternedPackageManifest {
  pub interner: StringInterner,
  pub names: Vec<Option<InternKey>>,
  pub full_names: Vec<Option<InternKey>>,
  pub owners: Vec<Option<InternKey>>,
  pub package_urls: Vec<Option<InternKey>>,
  pub dates_created: Vec<OffsetDateTime>,
  pub dates_updated: Vec<OffsetDateTime>,
  pub uuid4s: Vec<Option<InternKey>>,
  pub rating_scores: Vec<Option<u32>>,
  pub is_pinned: Vec<Option<bool>>,
  pub is_deprecated: Vec<Option<bool>>,
  pub has_nsfw_content: Vec<Option<bool>>,
  pub categories: Vec<Vec<InternKey>>,
  pub version_ranges: Vec<(usize, usize)>,
  pub versions: InternedVersionManifest,
}

/// Struct-of-Arrays representation of version data with interned strings.
///
/// Separates frequently accessed "hot" data from rarely accessed "cold" metadata
/// for better cache performance and memory efficiency. String values are stored as
/// interned keys that reference the parent manifest's string interner.
#[derive(Debug, Clone)]
pub struct InternedVersionManifest {
  pub package_indices: Vec<usize>,
  pub version_numbers: Vec<Option<InternKey>>,
  pub download_urls: Vec<Option<InternKey>>,
  pub dependencies: Vec<Vec<InternKey>>,
  pub dates_created: Vec<OffsetDateTime>,
  pub descriptions: Vec<Option<InternKey>>,
  pub icons: Vec<Option<InternKey>>,
  pub downloads: Vec<Option<u32>>,
  pub website_urls: Vec<Option<InternKey>>,
  pub is_active: Vec<Option<bool>>,
  pub uuid4s: Vec<Option<InternKey>>,
  pub file_sizes: Vec<Option<u64>>,
}

/// Serializable representation of an interned package manifest.
///
/// This structure converts the runtime InternedPackageManifest into a format that can be
/// efficiently serialized and deserialized. The string interner is converted to a simple
/// vector of strings (the string table), and all interned keys become u32 indices into
/// this table.
///
/// This separation allows for optimal serialization performance while maintaining the
/// memory efficiency benefits of string interning at runtime.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializableInternedManifest {
  pub string_table: Vec<String>,
  pub names: Vec<Option<u32>>,
  pub full_names: Vec<Option<u32>>,
  pub owners: Vec<Option<u32>>,
  pub package_urls: Vec<Option<u32>>,
  pub dates_created: Vec<OffsetDateTime>,
  pub dates_updated: Vec<OffsetDateTime>,
  pub uuid4s: Vec<Option<u32>>,
  pub rating_scores: Vec<Option<u32>>,
  pub is_pinned: Vec<Option<bool>>,
  pub is_deprecated: Vec<Option<bool>>,
  pub has_nsfw_content: Vec<Option<bool>>,
  pub categories: Vec<Vec<u32>>,
  pub version_ranges: Vec<(usize, usize)>,
  pub versions: SerializableInternedVersionManifest,
}

/// Serializable representation of interned version data.
///
/// Contains version information with string values represented as u32 indices into
/// the parent manifest's string table. This allows efficient serialization while
/// maintaining the benefits of string interning.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializableInternedVersionManifest {
  pub package_indices: Vec<usize>,
  pub version_numbers: Vec<Option<u32>>,
  pub download_urls: Vec<Option<u32>>,
  pub dependencies: Vec<Vec<u32>>,
  pub dates_created: Vec<OffsetDateTime>,
  pub descriptions: Vec<Option<u32>>,
  pub icons: Vec<Option<u32>>,
  pub downloads: Vec<Option<u32>>,
  pub website_urls: Vec<Option<u32>>,
  pub is_active: Vec<Option<bool>>,
  pub uuid4s: Vec<Option<u32>>,
  pub file_sizes: Vec<Option<u64>>,
}

#[allow(dead_code)]
impl PackageManifest {
  /// Returns the number of packages in the manifest.
  ///
  /// # Returns
  ///
  /// The total count of packages
  pub fn len(&self) -> usize {
    self.names.len()
  }

  /// Returns true if the manifest contains no packages.
  ///
  /// # Returns
  ///
  /// `true` if empty, `false` otherwise
  pub fn is_empty(&self) -> bool {
    self.names.is_empty()
  }

  /// Validates the internal consistency of the manifest structure.
  ///
  /// Checks that all parallel vectors have matching lengths and that version
  /// ranges are valid.
  ///
  /// # Returns
  ///
  /// Ok if the manifest is valid, otherwise an error describing the problem.
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

  /// Retrieves a package by its full name.
  ///
  /// # Parameters
  ///
  /// * `full_name` - The full name of the package to find
  ///
  /// # Returns
  ///
  /// The package if found, otherwise None.
  #[allow(dead_code)]
  pub fn get_package_by_full_name(&self, full_name: &str) -> Option<Package> {
    let idx = self.find_index_by_full_name(full_name)?;
    Some(self.get_package_at(idx))
  }

  /// Finds the index of a package by its full name.
  ///
  /// # Parameters
  ///
  /// * `full_name` - The full name to search for
  ///
  /// # Returns
  ///
  /// The index of the package if found, otherwise None.
  #[allow(dead_code)]
  pub fn find_index_by_full_name(&self, full_name: &str) -> Option<usize> {
    self
      .full_names
      .iter()
      .position(|name| name.as_ref().map(|n| n.as_str()) == Some(full_name))
  }

  /// Reconstructs a Package struct from the manifest data at the given index.
  ///
  /// This method gathers all package data and its versions from the SoA structure
  /// and reconstructs a complete Package object.
  ///
  /// # Parameters
  ///
  /// * `idx` - The index of the package in the manifest
  ///
  /// # Returns
  ///
  /// A fully materialized Package struct.
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

  /// Returns the index of the latest version for a package.
  ///
  /// The latest version is determined by comparing the date_created field of all versions.
  ///
  /// # Parameters
  ///
  /// * `idx` - The index of the package in the manifest
  ///
  /// # Returns
  ///
  /// The index of the latest version, or None if the package has no versions.
  pub fn get_latest_version_at(&self, idx: usize) -> Option<usize> {
    let (ver_start, ver_end) = self.version_ranges[idx];

    if ver_start >= ver_end {
      return None;
    }

    (ver_start..ver_end).max_by_key(|&ver_idx| self.versions.dates_created[ver_idx])
  }

  /// Builds an index mapping full package names to their indices.
  ///
  /// This is useful for fast lookups of packages by name.
  ///
  /// # Returns
  ///
  /// A HashMap mapping full names to package indices.
  pub fn build_name_index(&self) -> HashMap<String, usize> {
    self
      .full_names
      .iter()
      .enumerate()
      .filter_map(|(idx, name)| Some((name.as_ref()?.clone(), idx)))
      .collect()
  }
}

/// Implementation block for VersionManifest.
///
/// Currently empty but reserved for future version-specific methods.
impl VersionManifest {}

impl InternedPackageManifest {
  /// Returns the number of packages in the manifest.
  pub fn len(&self) -> usize {
    self.names.len()
  }

  /// Returns true if the manifest contains no packages.
  #[allow(dead_code)]
  pub fn is_empty(&self) -> bool {
    self.names.is_empty()
  }

  /// Resolves the package name at the given index from the string interner.
  ///
  /// # Returns
  ///
  /// The resolved string if a name exists at the index, otherwise None.
  pub fn resolve_name_at(&self, idx: usize) -> Option<String> {
    resolve_option(&self.interner, self.names[idx])
  }

  /// Resolves the full package name at the given index from the string interner.
  ///
  /// # Returns
  ///
  /// The resolved string if a full name exists at the index, otherwise None.
  pub fn resolve_full_name_at(&self, idx: usize) -> Option<String> {
    resolve_option(&self.interner, self.full_names[idx])
  }

  /// Resolves the package owner at the given index from the string interner.
  ///
  /// # Returns
  ///
  /// The resolved string if an owner exists at the index, otherwise None.
  pub fn resolve_owner_at(&self, idx: usize) -> Option<String> {
    resolve_option(&self.interner, self.owners[idx])
  }

  /// Retrieves a package by its full name.
  ///
  /// # Parameters
  ///
  /// * `full_name` - The full name of the package to find
  ///
  /// # Returns
  ///
  /// The package if found, otherwise None.
  #[allow(dead_code)]
  pub fn get_package_by_full_name(&self, full_name: &str) -> Option<Package> {
    let idx = self.find_index_by_full_name(full_name)?;
    Some(self.get_package_at(idx))
  }

  /// Finds the index of a package by its full name.
  ///
  /// # Parameters
  ///
  /// * `full_name` - The full name to search for
  ///
  /// # Returns
  ///
  /// The index of the package if found, otherwise None.
  #[allow(dead_code)]
  pub fn find_index_by_full_name(&self, full_name: &str) -> Option<usize> {
    self.full_names.iter().position(|key| {
      key
        .map(|k| self.interner.resolve(&k) == full_name)
        .unwrap_or(false)
    })
  }

  /// Reconstructs a Package struct from the manifest data at the given index.
  ///
  /// This method resolves all interned strings and constructs a complete Package
  /// with all its versions.
  ///
  /// # Parameters
  ///
  /// * `idx` - The index of the package in the manifest
  ///
  /// # Returns
  ///
  /// A fully materialized Package struct.
  #[allow(dead_code)]
  pub fn get_package_at(&self, idx: usize) -> Package {
    let (version_start, version_end) = self.version_ranges[idx];
    let mut versions = Vec::with_capacity(version_end - version_start);

    for ver_idx in version_start..version_end {
      versions.push(Version {
        name: resolve_option(&self.interner, self.names[idx]),
        full_name: resolve_option(&self.interner, self.full_names[idx]),
        description: resolve_option(&self.interner, self.versions.descriptions[ver_idx]),
        icon: resolve_option(&self.interner, self.versions.icons[ver_idx]),
        version_number: resolve_option(&self.interner, self.versions.version_numbers[ver_idx]),
        dependencies: resolve_vec(&self.interner, &self.versions.dependencies[ver_idx]),
        download_url: resolve_option(&self.interner, self.versions.download_urls[ver_idx]),
        downloads: self.versions.downloads[ver_idx],
        date_created: self.versions.dates_created[ver_idx],
        website_url: resolve_option(&self.interner, self.versions.website_urls[ver_idx]),
        is_active: self.versions.is_active[ver_idx],
        uuid4: resolve_option(&self.interner, self.versions.uuid4s[ver_idx]),
        file_size: self.versions.file_sizes[ver_idx],
      });
    }

    Package {
      name: resolve_option(&self.interner, self.names[idx]),
      full_name: resolve_option(&self.interner, self.full_names[idx]),
      owner: resolve_option(&self.interner, self.owners[idx]),
      package_url: resolve_option(&self.interner, self.package_urls[idx]),
      date_created: self.dates_created[idx],
      date_updated: self.dates_updated[idx],
      uuid4: resolve_option(&self.interner, self.uuid4s[idx]),
      rating_score: self.rating_scores[idx],
      is_pinned: self.is_pinned[idx],
      is_deprecated: self.is_deprecated[idx],
      has_nsfw_content: self.has_nsfw_content[idx],
      categories: resolve_vec(&self.interner, &self.categories[idx]),
      versions,
    }
  }

  /// Returns the index of the latest version for a package.
  ///
  /// The latest version is determined by comparing the date_created field of all versions.
  ///
  /// # Parameters
  ///
  /// * `idx` - The index of the package in the manifest
  ///
  /// # Returns
  ///
  /// The index of the latest version, or None if the package has no versions.
  pub fn get_latest_version_at(&self, idx: usize) -> Option<usize> {
    let (ver_start, ver_end) = self.version_ranges[idx];

    if ver_start >= ver_end {
      return None;
    }

    (ver_start..ver_end).max_by_key(|&ver_idx| self.versions.dates_created[ver_idx])
  }

  /// Builds an index mapping full package names to their indices.
  ///
  /// This is useful for fast lookups of packages by name.
  ///
  /// # Returns
  ///
  /// A HashMap mapping resolved full names to package indices.
  pub fn build_name_index(&self) -> HashMap<String, usize> {
    self
      .full_names
      .iter()
      .enumerate()
      .filter_map(|(idx, key)| {
        let name = resolve_option(&self.interner, *key)?;
        Some((name, idx))
      })
      .collect()
  }

  /// Validates the internal consistency of the manifest structure.
  ///
  /// Checks that all parallel vectors have matching lengths and that version
  /// ranges are valid.
  ///
  /// # Returns
  ///
  /// Ok if the manifest is valid, otherwise an error describing the problem.
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

    let version_count = self.versions.package_indices.len();

    if self.versions.version_numbers.len() != version_count {
      return Err(format!(
        "versions.version_numbers length {} != {}",
        self.versions.version_numbers.len(),
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
}

/// Implementation block for InternedVersionManifest.
///
/// Currently empty but reserved for future version-specific methods.
impl InternedVersionManifest {}

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
  #[allow(dead_code)]
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
  #[allow(dead_code)]
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

  /// Resolves all dependencies for the requested packages using an interned manifest.
  ///
  /// This method:
  /// 1. Starts with the user-requested packages
  /// 2. For each package, adds its dependencies to the list to process
  /// 3. Continues until all dependencies are resolved
  ///
  /// # Parameters
  ///
  /// * `manifest` - An InternedPackageManifest of all available packages
  ///
  /// # Returns
  ///
  /// A HashMap where keys are filenames and values are download URLs for all required packages
  pub fn resolve_interned(&self, manifest: &InternedPackageManifest) -> HashMap<String, String> {
    let name_index = manifest.build_name_index();
    let mut sorted_set: BTreeSet<String> = self.install_packages.iter().cloned().collect();
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
        let deps = resolve_vec(
          &manifest.interner,
          &manifest.versions.dependencies[latest_ver_idx],
        );

        for dep in deps {
          tracing::debug!(
            "Found dependency of {:#?} for {:#?} mod",
            dep,
            manifest.resolve_name_at(pkg_idx).unwrap_or_default(),
          );
          sorted_set.insert(dep);
        }
      }

      install_indices.push(pkg_idx);
    }

    Self::package_indices_to_urls_interned(manifest, &install_indices)
  }

  /// Converts package indices to a HashMap of filenames and download URLs using an interned manifest.
  ///
  /// # Parameters
  ///
  /// * `manifest` - The InternedPackageManifest
  /// * `indices` - Vector of package indices
  ///
  /// # Returns
  ///
  /// A HashMap mapping filenames to download URLs
  fn package_indices_to_urls_interned(
    manifest: &InternedPackageManifest,
    indices: &[usize],
  ) -> HashMap<String, String> {
    indices
      .iter()
      .filter_map(|&idx| {
        let latest_ver_idx = manifest.get_latest_version_at(idx)?;
        let url = resolve_option(
          &manifest.interner,
          manifest.versions.download_urls[latest_ver_idx],
        )?;
        let version_number = resolve_option(
          &manifest.interner,
          manifest.versions.version_numbers[latest_ver_idx],
        )?;
        let package_name = manifest.resolve_full_name_at(idx)?;
        let zip_name = format!("{}-{}.zip", package_name, version_number);

        Some((zip_name, url))
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

impl From<Vec<Package>> for InternedPackageManifest {
  fn from(packages: Vec<Package>) -> Self {
    let num_packages = packages.len();
    let total_versions: usize = packages.iter().map(|p| p.versions.len()).sum();

    let mut interner = StringInterner::default();

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
      names.push(intern_option(&mut interner, package.name.as_deref()));
      full_names.push(intern_option(&mut interner, package.full_name.as_deref()));
      owners.push(intern_option(&mut interner, package.owner.as_deref()));
      package_urls.push(intern_option(&mut interner, package.package_url.as_deref()));
      dates_created.push(package.date_created);
      dates_updated.push(package.date_updated);
      uuid4s.push(intern_option(&mut interner, package.uuid4.as_deref()));
      rating_scores.push(package.rating_score);
      is_pinned.push(package.is_pinned);
      is_deprecated.push(package.is_deprecated);
      has_nsfw_content.push(package.has_nsfw_content);
      categories.push(intern_vec(&mut interner, &package.categories));

      let version_count = package.versions.len();
      version_ranges.push((version_offset, version_offset + version_count));

      for version in package.versions {
        package_indices.push(pkg_idx);
        version_numbers.push(intern_option(
          &mut interner,
          version.version_number.as_deref(),
        ));
        download_urls.push(intern_option(
          &mut interner,
          version.download_url.as_deref(),
        ));
        dependencies.push(intern_vec(&mut interner, &version.dependencies));
        version_dates_created.push(version.date_created);
        descriptions.push(intern_option(&mut interner, version.description.as_deref()));
        icons.push(intern_option(&mut interner, version.icon.as_deref()));
        downloads.push(version.downloads);
        website_urls.push(intern_option(&mut interner, version.website_url.as_deref()));
        is_active.push(version.is_active);
        version_uuid4s.push(intern_option(&mut interner, version.uuid4.as_deref()));
        file_sizes.push(version.file_size);
      }

      version_offset += version_count;
    }

    InternedPackageManifest {
      interner,
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
      versions: InternedVersionManifest {
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

impl From<PackageManifest> for InternedPackageManifest {
  fn from(manifest: PackageManifest) -> Self {
    let mut interner = StringInterner::default();

    let names: Vec<Option<InternKey>> = manifest
      .names
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let full_names: Vec<Option<InternKey>> = manifest
      .full_names
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let owners: Vec<Option<InternKey>> = manifest
      .owners
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let package_urls: Vec<Option<InternKey>> = manifest
      .package_urls
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let uuid4s: Vec<Option<InternKey>> = manifest
      .uuid4s
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let categories: Vec<Vec<InternKey>> = manifest
      .categories
      .iter()
      .map(|c| intern_vec(&mut interner, c))
      .collect();

    let version_numbers: Vec<Option<InternKey>> = manifest
      .versions
      .version_numbers
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let download_urls: Vec<Option<InternKey>> = manifest
      .versions
      .download_urls
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let dependencies: Vec<Vec<InternKey>> = manifest
      .versions
      .dependencies
      .iter()
      .map(|d| intern_vec(&mut interner, d))
      .collect();

    let descriptions: Vec<Option<InternKey>> = manifest
      .versions
      .descriptions
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let icons: Vec<Option<InternKey>> = manifest
      .versions
      .icons
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let website_urls: Vec<Option<InternKey>> = manifest
      .versions
      .website_urls
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    let version_uuid4s: Vec<Option<InternKey>> = manifest
      .versions
      .uuid4s
      .iter()
      .map(|s| intern_option(&mut interner, s.as_deref()))
      .collect();

    InternedPackageManifest {
      interner,
      names,
      full_names,
      owners,
      package_urls,
      dates_created: manifest.dates_created,
      dates_updated: manifest.dates_updated,
      uuid4s,
      rating_scores: manifest.rating_scores,
      is_pinned: manifest.is_pinned,
      is_deprecated: manifest.is_deprecated,
      has_nsfw_content: manifest.has_nsfw_content,
      categories,
      version_ranges: manifest.version_ranges,
      versions: InternedVersionManifest {
        package_indices: manifest.versions.package_indices,
        version_numbers,
        download_urls,
        dependencies,
        dates_created: manifest.versions.dates_created,
        descriptions,
        icons,
        downloads: manifest.versions.downloads,
        website_urls,
        is_active: manifest.versions.is_active,
        uuid4s: version_uuid4s,
        file_sizes: manifest.versions.file_sizes,
      },
    }
  }
}

/// Converts an InternKey to its u32 index representation for serialization.
///
/// # Parameters
///
/// * `_interner` - The StringInterner (unused but kept for API symmetry)
/// * `key` - The InternKey to convert
///
/// # Returns
///
/// The u32 index corresponding to the key
fn key_to_index(_interner: &StringInterner, key: InternKey) -> u32 {
  key.into_usize() as u32
}

/// Converts a u32 index back to an InternKey for deserialization.
///
/// # Parameters
///
/// * `index` - The u32 index to convert
///
/// # Returns
///
/// The InternKey corresponding to the index
///
/// # Panics
///
/// Panics if the index cannot be converted to a valid InternKey
fn index_to_key(index: u32) -> InternKey {
  InternKey::try_from_usize(index as usize).expect("Invalid intern key index")
}

impl From<&InternedPackageManifest> for SerializableInternedManifest {
  fn from(manifest: &InternedPackageManifest) -> Self {
    let string_table: Vec<String> = manifest.interner.strings().map(|s| s.to_string()).collect();

    let names: Vec<Option<u32>> = manifest
      .names
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let full_names: Vec<Option<u32>> = manifest
      .full_names
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let owners: Vec<Option<u32>> = manifest
      .owners
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let package_urls: Vec<Option<u32>> = manifest
      .package_urls
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let uuid4s: Vec<Option<u32>> = manifest
      .uuid4s
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let categories: Vec<Vec<u32>> = manifest
      .categories
      .iter()
      .map(|keys| {
        keys
          .iter()
          .map(|k| key_to_index(&manifest.interner, *k))
          .collect()
      })
      .collect();

    let version_numbers: Vec<Option<u32>> = manifest
      .versions
      .version_numbers
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let download_urls: Vec<Option<u32>> = manifest
      .versions
      .download_urls
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let dependencies: Vec<Vec<u32>> = manifest
      .versions
      .dependencies
      .iter()
      .map(|keys| {
        keys
          .iter()
          .map(|k| key_to_index(&manifest.interner, *k))
          .collect()
      })
      .collect();

    let descriptions: Vec<Option<u32>> = manifest
      .versions
      .descriptions
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let icons: Vec<Option<u32>> = manifest
      .versions
      .icons
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let website_urls: Vec<Option<u32>> = manifest
      .versions
      .website_urls
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    let version_uuid4s: Vec<Option<u32>> = manifest
      .versions
      .uuid4s
      .iter()
      .map(|k| k.map(|key| key_to_index(&manifest.interner, key)))
      .collect();

    SerializableInternedManifest {
      string_table,
      names,
      full_names,
      owners,
      package_urls,
      dates_created: manifest.dates_created.clone(),
      dates_updated: manifest.dates_updated.clone(),
      uuid4s,
      rating_scores: manifest.rating_scores.clone(),
      is_pinned: manifest.is_pinned.clone(),
      is_deprecated: manifest.is_deprecated.clone(),
      has_nsfw_content: manifest.has_nsfw_content.clone(),
      categories,
      version_ranges: manifest.version_ranges.clone(),
      versions: SerializableInternedVersionManifest {
        package_indices: manifest.versions.package_indices.clone(),
        version_numbers,
        download_urls,
        dependencies,
        dates_created: manifest.versions.dates_created.clone(),
        descriptions,
        icons,
        downloads: manifest.versions.downloads.clone(),
        website_urls,
        is_active: manifest.versions.is_active.clone(),
        uuid4s: version_uuid4s,
        file_sizes: manifest.versions.file_sizes.clone(),
      },
    }
  }
}

impl From<SerializableInternedManifest> for InternedPackageManifest {
  fn from(manifest: SerializableInternedManifest) -> Self {
    let mut interner = StringInterner::default();

    for s in &manifest.string_table {
      interner.get_or_intern(s);
    }

    let names: Vec<Option<InternKey>> = manifest
      .names
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let full_names: Vec<Option<InternKey>> = manifest
      .full_names
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let owners: Vec<Option<InternKey>> = manifest
      .owners
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let package_urls: Vec<Option<InternKey>> = manifest
      .package_urls
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let uuid4s: Vec<Option<InternKey>> = manifest
      .uuid4s
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let categories: Vec<Vec<InternKey>> = manifest
      .categories
      .iter()
      .map(|indices| indices.iter().map(|&i| index_to_key(i)).collect())
      .collect();

    let version_numbers: Vec<Option<InternKey>> = manifest
      .versions
      .version_numbers
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let download_urls: Vec<Option<InternKey>> = manifest
      .versions
      .download_urls
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let dependencies: Vec<Vec<InternKey>> = manifest
      .versions
      .dependencies
      .iter()
      .map(|indices| indices.iter().map(|&i| index_to_key(i)).collect())
      .collect();

    let descriptions: Vec<Option<InternKey>> = manifest
      .versions
      .descriptions
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let icons: Vec<Option<InternKey>> = manifest
      .versions
      .icons
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let website_urls: Vec<Option<InternKey>> = manifest
      .versions
      .website_urls
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    let version_uuid4s: Vec<Option<InternKey>> = manifest
      .versions
      .uuid4s
      .iter()
      .map(|idx| idx.map(index_to_key))
      .collect();

    InternedPackageManifest {
      interner,
      names,
      full_names,
      owners,
      package_urls,
      dates_created: manifest.dates_created,
      dates_updated: manifest.dates_updated,
      uuid4s,
      rating_scores: manifest.rating_scores,
      is_pinned: manifest.is_pinned,
      is_deprecated: manifest.is_deprecated,
      has_nsfw_content: manifest.has_nsfw_content,
      categories,
      version_ranges: manifest.version_ranges,
      versions: InternedVersionManifest {
        package_indices: manifest.versions.package_indices,
        version_numbers,
        download_urls,
        dependencies,
        dates_created: manifest.versions.dates_created,
        descriptions,
        icons,
        downloads: manifest.versions.downloads,
        website_urls,
        is_active: manifest.versions.is_active,
        uuid4s: version_uuid4s,
        file_sizes: manifest.versions.file_sizes,
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
    assert!(result.unwrap_err().contains("versions.dependencies length"));
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
    assert!(result.unwrap_err().contains("versions.descriptions length"));
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
    assert!(result.unwrap_err().contains("versions.website_urls length"));
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

  #[test]
  fn test_interned_manifest_basic_operations() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let interned: InternedPackageManifest = packages.into();

    assert_eq!(interned.len(), 1);

    assert!(!interned.is_empty());

    assert_eq!(interned.resolve_name_at(0), Some("TestMod".to_string()));

    assert_eq!(
      interned.resolve_full_name_at(0),
      Some("TestOwner-TestMod".to_string())
    );

    assert_eq!(interned.resolve_owner_at(0), Some("TestOwner".to_string()));
  }

  #[test]
  fn test_interned_manifest_get_package_by_full_name() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let interned: InternedPackageManifest = packages.into();

    let found = interned.get_package_by_full_name("TestOwner-TestMod");

    assert!(found.is_some());

    let package = found.unwrap();

    assert_eq!(package.name, Some("TestMod".to_string()));

    assert_eq!(package.owner, Some("TestOwner".to_string()));

    assert_eq!(package.versions.len(), 1);
  }

  #[test]
  fn test_interned_manifest_find_index_by_full_name() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");
    let pkg2 = create_test_package("ModB", "Owner2", "2.0.0");
    let packages = vec![pkg1, pkg2];
    let interned: InternedPackageManifest = packages.into();

    assert_eq!(interned.find_index_by_full_name("Owner1-ModA"), Some(0));

    assert_eq!(interned.find_index_by_full_name("Owner2-ModB"), Some(1));

    assert_eq!(interned.find_index_by_full_name("NonExistent"), None);
  }

  #[test]
  fn test_interned_manifest_get_package_at() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let interned: InternedPackageManifest = packages.into();

    let package = interned.get_package_at(0);

    assert_eq!(package.name, Some("TestMod".to_string()));

    assert_eq!(package.full_name, Some("TestOwner-TestMod".to_string()));

    assert_eq!(package.owner, Some("TestOwner".to_string()));

    assert_eq!(package.versions.len(), 1);

    assert_eq!(
      package.versions[0].version_number,
      Some("1.0.0".to_string())
    );
  }

  #[test]
  fn test_interned_manifest_get_latest_version_at() {
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
    pkg.versions.insert(0, older_version);

    let packages = vec![pkg];
    let interned: InternedPackageManifest = packages.into();

    let latest_idx = interned.get_latest_version_at(0);

    assert!(latest_idx.is_some());

    let version_idx = latest_idx.unwrap();
    let version_number = interned.versions.version_numbers[version_idx]
      .map(|key| interned.interner.resolve(&key).to_string());

    assert_eq!(version_number, Some("1.0.0".to_string()));
  }

  #[test]
  fn test_interned_manifest_get_latest_version_at_empty_range() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.version_ranges[0] = (0, 0);

    let latest = interned.get_latest_version_at(0);

    assert!(latest.is_none());
  }

  #[test]
  fn test_interned_manifest_build_name_index() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");
    let pkg2 = create_test_package("ModB", "Owner2", "2.0.0");
    let packages = vec![pkg1, pkg2];
    let interned: InternedPackageManifest = packages.into();

    let index = interned.build_name_index();

    assert_eq!(index.len(), 2);

    assert_eq!(index.get("Owner1-ModA"), Some(&0));

    assert_eq!(index.get("Owner2-ModB"), Some(&1));
  }

  #[test]
  fn test_interned_manifest_validate_valid() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let interned: InternedPackageManifest = packages.into();

    let result = interned.validate();

    assert!(result.is_ok());
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_full_names_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.full_names.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("full_names length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_owners_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.owners.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("owners length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_package_urls_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.package_urls.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("package_urls length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_dates_created_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.dates_created.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("dates_created length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_dates_updated_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.dates_updated.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("dates_updated length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_uuid4s_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.uuid4s.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("uuid4s length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_rating_scores_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.rating_scores.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("rating_scores length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_is_pinned_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.is_pinned.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("is_pinned length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_is_deprecated_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.is_deprecated.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("is_deprecated length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_has_nsfw_content_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.has_nsfw_content.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("has_nsfw_content length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_categories_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.categories.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("categories length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_ranges_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.version_ranges.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("version_ranges length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_numbers_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.version_numbers.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("version_numbers length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_download_urls_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.download_urls.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("download_urls length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_dependencies_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.dependencies.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("dependencies length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_dates_created_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.dates_created.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("dates_created length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_descriptions_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.descriptions.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("descriptions length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_icons_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.icons.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("icons length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_downloads_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.downloads.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("downloads length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_website_urls_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.website_urls.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("website_urls length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_is_active_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.is_active.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("is_active length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_uuid4s_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.uuid4s.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("uuid4s length"));
  }

  #[test]
  fn test_interned_manifest_validate_mismatched_version_file_sizes_length() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.versions.file_sizes.pop();

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("file_sizes length"));
  }

  #[test]
  fn test_interned_manifest_validate_invalid_version_range() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.version_ranges[0] = (5, 3);

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("Invalid version range"));
  }

  #[test]
  fn test_interned_manifest_validate_version_range_out_of_bounds() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let mut interned: InternedPackageManifest = packages.into();

    interned.version_ranges[0] = (0, 999);

    let result = interned.validate();

    assert!(result.is_err());

    assert!(result.unwrap_err().contains("ends at"));
  }

  #[test]
  fn test_interned_manifest_serialization_round_trip() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg.clone()];
    let interned: InternedPackageManifest = packages.into();

    let serializable: SerializableInternedManifest = (&interned).into();

    let binary = bincode::serialize(&serializable).unwrap();

    let deserialized: SerializableInternedManifest = bincode::deserialize(&binary).unwrap();

    let recovered: InternedPackageManifest = deserialized.into();

    assert_eq!(recovered.len(), 1);

    assert_eq!(recovered.resolve_name_at(0), Some("TestMod".to_string()));

    assert_eq!(
      recovered.resolve_full_name_at(0),
      Some("TestOwner-TestMod".to_string())
    );

    let recovered_pkg = recovered.get_package_at(0);

    assert_eq!(recovered_pkg.name, pkg.name);

    assert_eq!(recovered_pkg.full_name, pkg.full_name);

    assert_eq!(recovered_pkg.versions.len(), pkg.versions.len());
  }

  #[test]
  fn test_interned_manifest_conversion_from_package_manifest() {
    let pkg = create_test_package("TestMod", "TestOwner", "1.0.0");
    let packages = vec![pkg];
    let v2_manifest: PackageManifest = packages.into();

    let interned: InternedPackageManifest = v2_manifest.clone().into();

    assert_eq!(interned.len(), v2_manifest.len());

    assert_eq!(interned.resolve_name_at(0), v2_manifest.names[0].clone());

    assert_eq!(
      interned.resolve_full_name_at(0),
      v2_manifest.full_names[0].clone()
    );
  }

  #[test]
  fn test_interned_manifest_dependency_graph_resolve() {
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
    let interned: InternedPackageManifest = packages.into();

    let dg1 = DependencyGraph::new(vec!["Owner1-ModA".to_string()]);
    let result1 = dg1.resolve_interned(&interned);

    assert_eq!(result1.len(), 1);

    assert!(result1.contains_key("Owner1-ModA-1.0.0.zip"));

    let dg2 = DependencyGraph::new(vec!["Owner2-ModB".to_string()]);
    let result2 = dg2.resolve_interned(&interned);

    assert_eq!(result2.len(), 3);

    assert!(result2.contains_key("Owner2-ModB-2.0.0.zip"));

    assert!(result2.contains_key("Owner3-ModC-1.5.0.zip"));

    assert!(result2.contains_key("Owner4-ModD-0.9.0.zip"));
  }

  #[test]
  fn test_interned_manifest_dependency_graph_resolve_with_missing_dependency() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");
    let pkg2 = create_test_package_with_dependencies(
      "ModB",
      "Owner2",
      "2.0.0",
      vec!["Owner3-NonExistent".to_string()],
    );

    let packages = vec![pkg1, pkg2];
    let interned: InternedPackageManifest = packages.into();

    let dg = DependencyGraph::new(vec!["Owner2-ModB".to_string()]);
    let result = dg.resolve_interned(&interned);

    assert_eq!(result.len(), 1);

    assert!(result.contains_key("Owner2-ModB-2.0.0.zip"));
  }

  #[test]
  fn test_interned_manifest_dependency_graph_resolve_with_version_suffix() {
    let pkg1 = create_test_package("ModA", "Owner1", "1.0.0");

    let packages = vec![pkg1];
    let interned: InternedPackageManifest = packages.into();

    let dg = DependencyGraph::new(vec!["Owner1-ModA-1.0.0".to_string()]);
    let result = dg.resolve_interned(&interned);

    assert_eq!(result.len(), 1);

    assert!(result.contains_key("Owner1-ModA-1.0.0.zip"));
  }
}
