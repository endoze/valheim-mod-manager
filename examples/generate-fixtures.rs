use std::fs;
use std::path::PathBuf;
use valheim_mod_manager::api;
use valheim_mod_manager::package::Package;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("Generating benchmark fixtures from production data...");
  println!();

  let temp_cache = tempfile::tempdir()?;
  let cache_dir = temp_cache.path().to_str().unwrap();

  println!("Downloading manifest from Thunderstore API...");
  let manifest = api::get_manifest(cache_dir, None).await?;

  println!("  ✓ Downloaded {} packages", manifest.len());
  let version_count: usize = manifest.version_ranges.iter().map(|(s, e)| e - s).sum();
  println!("  ✓ Total versions: {}", version_count);
  println!();

  let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("benches")
    .join("fixtures");

  fs::create_dir_all(&fixtures_dir)?;

  println!("Generating v2 format fixture...");
  let v2_data = bincode::serialize(&manifest)?;
  let v2_compressed = zstd::encode_all(v2_data.as_slice(), 9)?;
  let v2_path = fixtures_dir.join("api_manifest_v2.bin.zst");
  fs::write(&v2_path, v2_compressed)?;
  println!("  ✓ Written to benches/fixtures/api_manifest_v2.bin.zst");
  println!("  ✓ Size: {} bytes", fs::metadata(&v2_path)?.len());
  println!();

  println!("Generating v1 format fixture (converting from v2)...");
  let v1_packages: Vec<Package> = (0..manifest.len())
    .map(|idx| manifest.get_package_at(idx))
    .collect();
  let v1_data = bincode::serialize(&v1_packages)?;
  let v1_compressed = zstd::encode_all(v1_data.as_slice(), 3)?;
  let v1_path = fixtures_dir.join("api_manifest.bin.zst");
  fs::write(&v1_path, v1_compressed)?;
  println!("  ✓ Written to benches/fixtures/api_manifest.bin.zst");
  println!("  ✓ Size: {} bytes", fs::metadata(&v1_path)?.len());
  println!();

  println!("✓ Fixtures generated successfully!");
  println!();
  println!("You can now run benchmarks with:");
  println!("  cargo bench --features bench");

  Ok(())
}
