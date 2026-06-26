//! Shared fixtures for command tests: a mock Thunderstore index plus a real mod
//! archive, so the install pipeline runs end to end with no network access.

use crate::target::Target;
use std::io::{Cursor, Write};
use tempfile::TempDir;
use thunderstore_engine::client::ThunderstoreClient;
use zip::write::{FileOptions, ZipWriter};

/// A mod archive carrying a manifest and one plugin DLL, which the install rules
/// route to `BepInEx/plugins/<Owner-Name>/`.
pub fn mod_zip(version: &str) -> Vec<u8> {
  mod_zip_with(version, "ModA.dll")
}

/// [`mod_zip`] with the plugin DLL named, so each fixture package installs a file
/// identifiable as its own rather than every package shipping `ModA.dll`.
pub fn mod_zip_with(version: &str, dll_name: &str) -> Vec<u8> {
  let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
  let options: FileOptions<'_, ()> =
    FileOptions::default().compression_method(zip::CompressionMethod::Stored);

  for (name, body) in [
    (
      "manifest.json".to_string(),
      format!("{{\"version_number\":\"{version}\"}}").into_bytes(),
    ),
    (format!("plugins/{dll_name}"), b"dll-bytes".to_vec()),
  ] {
    zip.start_file(name, options).unwrap();
    zip.write_all(&body).unwrap();
  }

  zip.finish().unwrap().into_inner()
}

/// The package index for `Owner-ModA`, `Owner-ModB`, `Owner-ModC`, and the
/// `denikson-BepInExPack_Valheim` loader, with downloads pointed at `server_url`.
///
/// `ModA` and `ModB` depend on nothing, so each installs as a single package.
/// `ModC` depends on `Owner-ModA-1.0.0`, so installing it exercises a real
/// dependency closure, the engine's resolver discards the version suffix
/// (`full_name_prefix`) and installs `Owner-ModA` alongside it.
pub fn index_json(server_url: &str) -> String {
  let package = |owner: &str, name: &str, version: &str, dependencies: &str| {
    format!(
      r#"{{"name":"{name}","full_name":"{owner}-{name}","owner":"{owner}",
      "package_url":"https://example.com/{name}",
      "date_created":"2024-01-01T12:00:00Z","date_updated":"2024-01-02T12:00:00Z",
      "uuid4":"pkg-{name}","rating_score":1,"is_pinned":false,
      "is_deprecated":false,"has_nsfw_content":false,"categories":[],
      "versions":[{{"name":"{name}","full_name":"{owner}-{name}",
      "description":"A mod","icon":"icon.png","version_number":"{version}",
      "dependencies":[{dependencies}],"download_url":"{server_url}/dl/{name}.zip",
      "downloads":1,"date_created":"2024-01-01T12:00:00Z",
      "website_url":"","is_active":true,"uuid4":"ver-{name}","file_size":1024}}]}}"#
    )
  };

  format!(
    "[{},{},{},{}]",
    package("Owner", "ModA", "1.0.0", ""),
    package("Owner", "ModB", "1.0.0", ""),
    package("Owner", "ModC", "1.0.0", "\"Owner-ModA-1.0.0\""),
    // A real loader identifier, so `Ecosystem::modloader_package` recognises it
    // and the loader-specific paths are reachable from tests.
    package("denikson", "BepInExPack_Valheim", "5.4.2200", "")
  )
}

/// A mod-loader archive shaped like `denikson-BepInExPack_Valheim`: its payload
/// nests under the `rootFolder` the ecosystem snapshot names for that package, so
/// the install strips the wrapper and the contents land at the target root.
///
/// The two files are the ones that make a loader recognisable on disk: the
/// doorstop proxy at the root, and a core assembly under `BepInEx/core/`. The
/// second matters because `BepInEx/core` is a declared namespaced route, so a
/// loader file landing there directly (rather than in an `<Owner-Name>/` folder)
/// is exactly the case that makes attributing files by shape unsafe.
pub fn loader_zip() -> Vec<u8> {
  let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
  let options: FileOptions<'_, ()> =
    FileOptions::default().compression_method(zip::CompressionMethod::Stored);

  for (name, body) in [
    ("manifest.json", &b"{\"version_number\":\"5.4.2200\"}"[..]),
    ("BepInExPack_Valheim/winhttp.dll", b"proxy"),
    ("BepInExPack_Valheim/BepInEx/core/BepInEx.dll", b"core"),
  ] {
    zip.start_file(name, options).unwrap();
    zip.write_all(body).unwrap();
  }

  zip.finish().unwrap().into_inner()
}

/// A mock Thunderstore plus a client, base directory, and game directory.
pub struct Fixture {
  /// The mock Thunderstore HTTP server backing this fixture's client.
  pub server: mockito::ServerGuard,
  /// A client configured to talk to `server`.
  pub client: ThunderstoreClient,
  /// The engine base directory (`<base>/<GAME>/{cache,exports,profiles}`).
  pub base: TempDir,
  /// The simulated game directory.
  pub game_dir: TempDir,
}

impl Fixture {
  /// Builds a fixture whose index and downloads are served from a mock server.
  pub fn new() -> Self {
    let mut server = mockito::Server::new();
    let url = server.url();

    server
      .mock("GET", "/pkg/")
      .with_status(200)
      .with_header("Content-Type", "application/json")
      .with_header("Last-Modified", "Wed, 21 Feb 2024 15:30:45 GMT")
      .with_body(index_json(&url))
      .create();

    // Each package ships its own DLL name, so a test can tell whose files landed
    // where, which matters for `ModC`, whose install also pulls in `ModA`.
    for (name, archive) in [
      ("ModA", mod_zip("1.0.0")),
      ("ModB", mod_zip_with("1.0.0", "ModB.dll")),
      ("ModC", mod_zip_with("1.0.0", "ModC.dll")),
      ("BepInExPack_Valheim", loader_zip()),
    ] {
      server
        .mock("GET", format!("/dl/{name}.zip").as_str())
        .with_status(200)
        .with_header("Content-Type", "application/zip")
        .with_body(archive)
        .create();
    }

    let base = TempDir::new().unwrap();
    let game_dir = TempDir::new().unwrap();

    let client = ThunderstoreClient::builder()
      .package_index_url(format!("{url}/pkg/"))
      .cache_dir(base.path())
      .build()
      .unwrap();

    Self {
      server,
      client,
      base,
      game_dir,
    }
  }

  /// A game-dir-mode target over this fixture's directories.
  pub fn target(&self) -> Target {
    Target::resolve(
      self.base.path().to_path_buf(),
      self.game_dir.path().to_path_buf(),
      crate::target::GAME,
      None,
    )
    .unwrap()
  }

  /// A profile-mode target, creating the profile directory first.
  pub fn profile_target(&self, name: &str) -> Target {
    thunderstore_engine::profile::layout::create(self.base.path(), crate::target::GAME, name)
      .unwrap();

    Target::resolve(
      self.base.path().to_path_buf(),
      self.game_dir.path().to_path_buf(),
      crate::target::GAME,
      Some(name),
    )
    .unwrap()
  }
}
