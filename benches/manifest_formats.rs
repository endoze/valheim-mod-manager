use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
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

fn load_real_manifest_v2() -> Option<PackageManifest> {
  let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("benches")
    .join("fixtures")
    .join("api_manifest_v2.bin.zst");

  if !path.exists() {
    return None;
  }

  let compressed_data = fs::read(&path).ok()?;
  let decompressed_data = zstd::decode_all(compressed_data.as_slice()).ok()?;
  bincode::deserialize(&decompressed_data).ok()
}

fn benchmark_serialization(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found at benches/fixtures/api_manifest.bin.zst");
      eprintln!("Skipping serialization benchmarks");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();

  let mut group = c.benchmark_group("serialization");
  group.throughput(Throughput::Elements(v1_packages.len() as u64));

  group.bench_function("v1_format", |b| {
    b.iter(|| {
      let serialized = bincode::serialize(black_box(&v1_packages)).unwrap();
      black_box(serialized)
    })
  });

  group.bench_function("v2_format", |b| {
    b.iter(|| {
      let serialized = bincode::serialize(black_box(&v2_manifest)).unwrap();
      black_box(serialized)
    })
  });

  group.finish();
}

fn benchmark_deserialization(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found at benches/fixtures/api_manifest.bin.zst");
      eprintln!("Skipping deserialization benchmarks");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();

  let v1_serialized = bincode::serialize(&v1_packages).unwrap();
  let v2_serialized = bincode::serialize(&v2_manifest).unwrap();

  let mut group = c.benchmark_group("deserialization");
  group.throughput(Throughput::Elements(v1_packages.len() as u64));

  group.bench_function("v1_format", |b| {
    b.iter(|| {
      let deserialized: Vec<Package> = bincode::deserialize(black_box(&v1_serialized)).unwrap();
      black_box(deserialized)
    })
  });

  group.bench_function("v2_format", |b| {
    b.iter(|| {
      let deserialized: PackageManifest = bincode::deserialize(black_box(&v2_serialized)).unwrap();
      black_box(deserialized)
    })
  });

  group.finish();
}

fn benchmark_compression(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found at benches/fixtures/api_manifest.bin.zst");
      eprintln!("Skipping compression benchmarks");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();

  let v1_serialized = bincode::serialize(&v1_packages).unwrap();
  let v2_serialized = bincode::serialize(&v2_manifest).unwrap();

  let mut group = c.benchmark_group("compression");

  for level in [3, 9].iter() {
    group.bench_with_input(BenchmarkId::new("v1_format", level), level, |b, &level| {
      b.iter(|| {
        let compressed = zstd::encode_all(black_box(v1_serialized.as_slice()), level).unwrap();
        black_box(compressed)
      })
    });

    group.bench_with_input(BenchmarkId::new("v2_format", level), level, |b, &level| {
      b.iter(|| {
        let compressed = zstd::encode_all(black_box(v2_serialized.as_slice()), level).unwrap();
        black_box(compressed)
      })
    });
  }

  group.finish();
}

fn benchmark_decompression(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found at benches/fixtures/api_manifest.bin.zst");
      eprintln!("Skipping decompression benchmarks");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();

  let v1_serialized = bincode::serialize(&v1_packages).unwrap();
  let v2_serialized = bincode::serialize(&v2_manifest).unwrap();

  let mut group = c.benchmark_group("decompression");

  for level in [3, 9].iter() {
    let v1_compressed = zstd::encode_all(v1_serialized.as_slice(), *level).unwrap();
    let v2_compressed = zstd::encode_all(v2_serialized.as_slice(), *level).unwrap();

    group.bench_with_input(BenchmarkId::new("v1_format", level), level, |b, _level| {
      b.iter(|| {
        let decompressed = zstd::decode_all(black_box(v1_compressed.as_slice())).unwrap();
        black_box(decompressed)
      })
    });

    group.bench_with_input(BenchmarkId::new("v2_format", level), level, |b, _level| {
      b.iter(|| {
        let decompressed = zstd::decode_all(black_box(v2_compressed.as_slice())).unwrap();
        black_box(decompressed)
      })
    });
  }

  group.finish();
}

fn benchmark_conversion(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found at benches/fixtures/api_manifest.bin.zst");
      eprintln!("Skipping conversion benchmarks");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();

  let mut group = c.benchmark_group("conversion");
  group.throughput(Throughput::Elements(v1_packages.len() as u64));

  group.bench_function("vec_to_manifest", |b| {
    b.iter(|| {
      let manifest: PackageManifest = black_box(v1_packages.clone()).into();
      black_box(manifest)
    })
  });

  group.bench_function("build_name_index", |b| {
    b.iter(|| {
      let index = black_box(&v2_manifest).build_name_index();
      black_box(index)
    })
  });

  group.bench_function("get_package_by_name", |b| {
    b.iter(|| {
      let pkg = black_box(&v2_manifest).get_package_by_full_name("denikson-BepInExPack_Valheim");
      black_box(pkg)
    })
  });

  group.finish();
}

fn benchmark_size_comparison(c: &mut Criterion) {
  let v1_packages = match load_real_manifest_v1() {
    Some(packages) => packages,
    None => {
      eprintln!("Warning: No v1 manifest found at benches/fixtures/api_manifest.bin.zst");
      eprintln!("Generating size comparison report anyway...");
      return;
    }
  };

  let v2_manifest: PackageManifest = v1_packages.clone().into();

  let v1_serialized = bincode::serialize(&v1_packages).unwrap();
  let v2_serialized = bincode::serialize(&v2_manifest).unwrap();

  println!("\n=== Size Comparison Report ===");
  println!("Packages: {}", v1_packages.len());
  println!(
    "Total versions: {}",
    v1_packages.iter().map(|p| p.versions.len()).sum::<usize>()
  );
  println!("\nSerialized (bincode):");
  println!("  V1 format: {} bytes", v1_serialized.len());
  println!("  V2 format: {} bytes", v2_serialized.len());
  println!(
    "  Reduction: {:.1}%",
    (1.0 - v2_serialized.len() as f64 / v1_serialized.len() as f64) * 100.0
  );

  for level in [3, 9] {
    let v1_compressed = zstd::encode_all(v1_serialized.as_slice(), level).unwrap();
    let v2_compressed = zstd::encode_all(v2_serialized.as_slice(), level).unwrap();

    println!("\nCompressed (zstd level {}):", level);
    println!("  V1 format: {} bytes", v1_compressed.len());
    println!("  V2 format: {} bytes", v2_compressed.len());
    println!(
      "  Reduction: {:.1}%",
      (1.0 - v2_compressed.len() as f64 / v1_compressed.len() as f64) * 100.0
    );
    println!(
      "  V1 compression ratio: {:.2}x",
      v1_serialized.len() as f64 / v1_compressed.len() as f64
    );
    println!(
      "  V2 compression ratio: {:.2}x",
      v2_serialized.len() as f64 / v2_compressed.len() as f64
    );
  }

  println!("==============================\n");

  c.bench_function("size_report", |b| b.iter(|| ()));
}

criterion_group!(
  benches,
  benchmark_serialization,
  benchmark_deserialization,
  benchmark_compression,
  benchmark_decompression,
  benchmark_conversion,
  benchmark_size_comparison
);
criterion_main!(benches);
