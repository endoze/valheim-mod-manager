use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::fs;
use std::path::PathBuf;
use valheim_mod_manager::package::{Package, PackageManifest};

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

fn search_soa_direct(manifest: &PackageManifest, term: &str) -> Vec<usize> {
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

fn benchmark_search_common_term(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found");
      return;
    }
  };

  let soa_manifest: PackageManifest = v1_packages.into();

  let search_term = "mod";

  let mut group = c.benchmark_group("search_common_term");
  group.throughput(Throughput::Elements(soa_manifest.len() as u64));

  group.bench_function("soa_direct", |b| {
    b.iter(|| {
      let results = search_soa_direct(black_box(&soa_manifest), black_box(search_term));
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

  let soa_manifest: PackageManifest = v1_packages.into();

  let search_term = "xyzzyx";

  let mut group = c.benchmark_group("search_rare_term");
  group.throughput(Throughput::Elements(soa_manifest.len() as u64));

  group.bench_function("soa_direct", |b| {
    b.iter(|| {
      let results = search_soa_direct(black_box(&soa_manifest), black_box(search_term));
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

  let soa_manifest: PackageManifest = v1_packages.into();

  let search_term = "tool";

  let mut group = c.benchmark_group("search_with_display");

  group.bench_function("soa_search_and_access", |b| {
    b.iter(|| {
      let results: Vec<_> = (0..soa_manifest.len())
        .filter(|&idx| {
          if let Some(name) = &soa_manifest.names[idx] {
            if name.to_lowercase().contains(search_term) {
              return true;
            }
          }

          if let Some(full_name) = &soa_manifest.full_names[idx] {
            if full_name.to_lowercase().contains(search_term) {
              return true;
            }
          }

          false
        })
        .map(|idx| {
          let (ver_start, ver_end) = soa_manifest.version_ranges[idx];

          let latest_ver_idx = (ver_start..ver_end)
            .max_by_key(|&ver_idx| soa_manifest.versions.dates_created[ver_idx])
            .unwrap_or(ver_start);

          let version = soa_manifest.versions.version_numbers[latest_ver_idx]
            .as_deref()
            .unwrap_or("Unknown");

          let name = soa_manifest.names[idx].as_deref().unwrap_or("Unknown");

          let owner = soa_manifest.owners[idx].as_deref().unwrap_or("Unknown");

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
