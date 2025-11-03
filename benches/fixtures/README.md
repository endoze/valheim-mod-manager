# Benchmark Fixtures

This directory contains compressed manifest files used for benchmarking the performance differences between v1 (Array-of-Structs) and v2 (Struct-of-Arrays) manifest formats.

## Why Are Fixtures Not Committed?

The fixture files are large (15+ MB total) and would bloat the git repository history. Instead, they are generated on-demand from live production data.

## Generating Fixtures

To generate fresh fixtures from the Thunderstore API:

```bash
cargo run --example generate-fixtures
```

This will:
1. Download the latest manifest from the production Thunderstore API
2. Save it as `api_manifest_v2.bin.zst` (current v2 format)
3. Convert it to v1 format and save as `api_manifest.bin.zst`

Both fixtures use the same source data, ensuring fair performance comparisons.

## Running Benchmarks

After generating fixtures, run benchmarks with:

```bash
cargo bench --features bench
```

If fixtures are missing, benchmarks will skip with a warning message.

## CI/CD

In CI environments, fixtures should be generated before running benchmarks:

```bash
cargo run --example generate-fixtures
cargo bench --features bench
```

## File Contents

- `api_manifest.bin.zst` - v1 format (Vec<Package>, zstd level 3) - baseline comparison using original compression
- `api_manifest_v2.bin.zst` - v2 format (PackageManifest SoA, zstd level 9) - optimized format with higher compression

Both files contain identical package data from the same source. The different compression levels reflect the actual production configuration: v1 used level 3, while v2 uses level 9 to better exploit the repetitive patterns in the SoA structure. This ensures benchmarks accurately measure the real-world performance impact of the optimization.
