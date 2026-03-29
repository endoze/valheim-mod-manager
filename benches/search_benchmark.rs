use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::fs;
use std::path::PathBuf;
use valheim_mod_manager::package::{InternedPackageManifest, Package, PackageManifest};

fn load_real_manifest_v1() -> Option<Vec<Package>> {
  let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("benches")
    .join("fixtures")
    .join("api_manifest.bin.zst");

  if !path.exists() {
    return None;
  }

  let compressed_data = fs::read(&path).ok()?;
  let decompressed_data = zstd::decode_all(compressed_data.as_slice()).ok()?;
  bincode::deserialize(&decompressed_data).ok()
}

fn search_v2_direct(manifest: &PackageManifest, term: &str) -> Vec<usize> {
  (0..manifest.len())
    .filter(|&idx| {
      if let Some(name) = &manifest.names[idx] {
        if name.to_lowercase().contains(term) {
          return true;
        }
      }

      if let Some(full_name) = &manifest.full_names[idx] {
        if full_name.to_lowercase().contains(term) {
          return true;
        }
      }

      false
    })
    .collect()
}

fn search_v3_direct(manifest: &InternedPackageManifest, term: &str) -> Vec<usize> {
  (0..manifest.len())
    .filter(|&idx| {
      if let Some(name) = manifest.resolve_name_at(idx) {
        if name.to_lowercase().contains(term) {
          return true;
        }
      }

      if let Some(full_name) = manifest.resolve_full_name_at(idx) {
        if full_name.to_lowercase().contains(term) {
          return true;
        }
      }

      false
    })
    .collect()
}

fn benchmark_search_common_term(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();
  let v3_manifest: InternedPackageManifest = v1_packages.into();

  let search_term = "mod";

  let mut group = c.benchmark_group("search_common_term");
  group.throughput(Throughput::Elements(v2_manifest.len() as u64));

  group.bench_function("v2_direct", |b| {
    b.iter(|| {
      let results = search_v2_direct(black_box(&v2_manifest), black_box(search_term));
      black_box(results)
    })
  });

  group.bench_function("v3_direct", |b| {
    b.iter(|| {
      let results = search_v3_direct(black_box(&v3_manifest), black_box(search_term));
      black_box(results)
    })
  });

  group.finish();
}

fn benchmark_search_rare_term(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();
  let v3_manifest: InternedPackageManifest = v1_packages.into();

  let search_term = "xyzzyx";

  let mut group = c.benchmark_group("search_rare_term");
  group.throughput(Throughput::Elements(v2_manifest.len() as u64));

  group.bench_function("v2_direct", |b| {
    b.iter(|| {
      let results = search_v2_direct(black_box(&v2_manifest), black_box(search_term));
      black_box(results)
    })
  });

  group.bench_function("v3_direct", |b| {
    b.iter(|| {
      let results = search_v3_direct(black_box(&v3_manifest), black_box(search_term));
      black_box(results)
    })
  });

  group.finish();
}

fn benchmark_search_with_display(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();
  let v3_manifest: InternedPackageManifest = v1_packages.into();

  let search_term = "tool";

  let mut group = c.benchmark_group("search_with_display");

  group.bench_function("v2_search_and_access", |b| {
    b.iter(|| {
      let results: Vec<_> = (0..v2_manifest.len())
        .filter(|&idx| {
          if let Some(name) = &v2_manifest.names[idx] {
            if name.to_lowercase().contains(search_term) {
              return true;
            }
          }

          if let Some(full_name) = &v2_manifest.full_names[idx] {
            if full_name.to_lowercase().contains(search_term) {
              return true;
            }
          }

          false
        })
        .map(|idx| {
          let (ver_start, ver_end) = v2_manifest.version_ranges[idx];

          let latest_ver_idx = (ver_start..ver_end)
            .max_by_key(|&ver_idx| v2_manifest.versions.dates_created[ver_idx])
            .unwrap_or(ver_start);

          let version = v2_manifest.versions.version_numbers[latest_ver_idx]
            .as_deref()
            .unwrap_or("Unknown");

          let name = v2_manifest.names[idx].as_deref().unwrap_or("Unknown");

          let owner = v2_manifest.owners[idx].as_deref().unwrap_or("Unknown");

          (owner, name, version)
        })
        .collect();

      black_box(results)
    })
  });

  group.bench_function("v3_search_and_access", |b| {
    b.iter(|| {
      let results: Vec<_> = (0..v3_manifest.len())
        .filter(|&idx| {
          if let Some(name) = v3_manifest.resolve_name_at(idx) {
            if name.to_lowercase().contains(search_term) {
              return true;
            }
          }

          if let Some(full_name) = v3_manifest.resolve_full_name_at(idx) {
            if full_name.to_lowercase().contains(search_term) {
              return true;
            }
          }

          false
        })
        .map(|idx| {
          let (ver_start, ver_end) = v3_manifest.version_ranges[idx];

          let latest_ver_idx = (ver_start..ver_end)
            .max_by_key(|&ver_idx| v3_manifest.versions.dates_created[ver_idx])
            .unwrap_or(ver_start);

          let version = v3_manifest.versions.version_numbers[latest_ver_idx]
            .map(|key| v3_manifest.interner.resolve(&key).to_string())
            .unwrap_or_else(|| "Unknown".to_string());

          let name = v3_manifest
            .resolve_name_at(idx)
            .unwrap_or_else(|| "Unknown".to_string());

          let owner = v3_manifest
            .resolve_owner_at(idx)
            .unwrap_or_else(|| "Unknown".to_string());

          (owner, name, version)
        })
        .collect();

      black_box(results)
    })
  });

  group.finish();
}

criterion_group!(
  benches,
  benchmark_search_common_term,
  benchmark_search_rare_term,
  benchmark_search_with_display
);
criterion_main!(benches);
