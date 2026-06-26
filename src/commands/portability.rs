use crate::error::AppResult;
use crate::target::{GAME, Target};
use thunderstore_engine::client::ThunderstoreClient;
use thunderstore_engine::ecosystem::Ecosystem;
use thunderstore_engine::profile::portability::{self, ImportSource};

/// Writes the target's mods and config to an `.r2z` under its exports directory.
pub fn export_file(target: &Target) -> AppResult<()> {
  let path = portability::export_to_file_in(
    &target.dir,
    target.name(),
    &target.exports_dir(),
    thunderstore_engine::profile::modlist::now_millis(),
  )?;

  println!("wrote {}", path.display());

  Ok(())
}

/// Uploads the target's export and prints the shareable Thunderstore code.
pub async fn export_code(client: &ThunderstoreClient, target: &Target) -> AppResult<()> {
  let code = portability::export_code_in(&target.dir, target.name(), client).await?;

  println!("profile code: {code}");

  Ok(())
}

/// Imports mods into the target from an `.r2z` file, an existing r2modman profile
/// directory, or a Thunderstore profile code.
///
/// `source` is classified by the engine's [`portability::ImportSource`], which
/// dispatches on what it names on disk and falls back to a code, so no flag is
/// needed to disambiguate and vmm decides nothing about the routing. A file or
/// code import reinstalls each mod at
/// its **latest** version (the engine's resolution is version-agnostic and the
/// export's pinned version is not honored), so the installed versions are printed
/// to make any divergence visible. A directory import copies the profile as-is
/// and downloads nothing, with one exception: any mod loader the adopted
/// `mods.yml` names that arrives without an install record is reinstalled from
/// Thunderstore afterwards, so it gains the record that makes it manageable and
/// removable. That reinstall pulls the loader's **latest** version, the same as a
/// file or code import, so the version the source profile had pinned is not
/// preserved; [`report_reinstalled_loaders`] discloses that. Regular mods are
/// unaffected and are still adopted at the versions the source had, and a source
/// naming no unrecorded loader stays entirely offline.
///
/// A directory import is a raw copy with no pre-clean: importing into a target
/// that already holds a different set of mods overwrites `mods.yml` but leaves
/// the previous mods' files on disk, now untracked and orphaned. Import into an
/// empty target, or accept that leftover.
///
/// A directory source that overlaps the target is refused up front by the
/// engine's `ensure_disjoint_trees`, called from inside `import_r2modman_dir`
/// before anything is copied.
pub async fn import(
  client: &ThunderstoreClient,
  eco: &Ecosystem,
  target: &Target,
  source: &str,
) -> AppResult<()> {
  let outcome = portability::import_in(
    &target.dir,
    &target.base,
    eco,
    client,
    GAME,
    source,
    thunderstore_engine::profile::modlist::now_millis(),
  )
  .await?;

  super::report_installed(target, &outcome.installed)?;

  // Which route ran is the engine's answer, not a second look at `source`. Both
  // disclosures are about versions, and each route loses them differently.
  match outcome.source {
    ImportSource::R2modmanDir(_) => report_reinstalled_loaders(&outcome.reinstalled),
    ImportSource::Archive(_) | ImportSource::Code(_) => {
      println!("\nNote: an import installs each mod's latest version, not the exported one.");
    }
  }

  Ok(())
}

/// Discloses that an adopted mod loader was reinstalled at its latest version.
///
/// The engine has to reinstall a loader to get an exact file list for it, since
/// a copied `BepInEx/core` file cannot be attributed by shape (see
/// [`portability::adopt_r2modman_dir_in`]), and that pulls the latest version
/// rather than the one the source profile recorded. Saying so is the point:
/// everything else about a directory import is adopted as-is, which sets an
/// expectation this one step breaks. Printed per loader, because the loader
/// registry is keyed on package identity rather than the game, so nothing rules
/// out more than one.
fn report_reinstalled_loaders(reinstalled: &[String]) {
  for loader in reinstalled {
    println!(
      "\nreinstalled {loader} at its latest version so it can be managed and \
       uninstalled; the version your source profile recorded is not preserved"
    );
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::Fixture;
  use base64::Engine as _;
  use thunderstore_engine::profile::modlist;
  use tokio::runtime::Runtime;

  /// A `mods.yml` naming `denikson-BepInExPack_Valheim` at 5.4.2200, shaped the
  /// way r2modman actually leaves one on disk.
  ///
  /// Matches the shape `modlist::write` produces rather than a minimal guess:
  /// `ProfileMod` has no `#[serde(default)]` on most fields, so a sparser
  /// document fails to parse. The shape was confirmed by generating a
  /// reference file with `modlist::write` in a scratch test and reading back
  /// what it wrote.
  fn adopted_loader_mods_yml() -> &'static str {
    "- manifestVersion: 1\n  name: denikson-BepInExPack_Valheim\n  \
     authorName: denikson\n  websiteUrl: ''\n  displayName: BepInExPack_Valheim\n  \
     description: A mod\n  gameVersion: '0'\n  networkMode: both\n  \
     packageType: other\n  installMode: managed\n  installedAtTime: 1700000000000\n  \
     loaders: []\n  dependencies: []\n  incompatibilities: []\n  \
     optionalDependencies: []\n  versionNumber: {major: 5, minor: 4, patch: 2200}\n  \
     enabled: true\n  onlineSource: true\n  trustedPackage: false\n"
  }

  #[test]
  fn export_writes_an_r2z_into_the_exports_dir() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    export_file(&target).unwrap();

    let exports: Vec<_> = std::fs::read_dir(target.exports_dir())
      .unwrap()
      .flatten()
      .map(|entry| entry.file_name().to_string_lossy().to_string())
      .collect();

    assert_eq!(exports.len(), 1);
    assert!(exports[0].starts_with("default_"), "got: {:?}", exports[0]);
    assert!(exports[0].ends_with(".r2z"));
  }

  #[test]
  fn an_export_imports_into_another_target() {
    let fixture = Fixture::new();
    let source = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &source,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    export_file(&source).unwrap();

    let archive = std::fs::read_dir(source.exports_dir())
      .unwrap()
      .flatten()
      .next()
      .unwrap()
      .path();

    let destination = fixture.profile_target("imported");

    runtime
      .block_on(import(
        &fixture.client,
        &eco,
        &destination,
        archive.to_str().unwrap(),
      ))
      .unwrap();

    let mods = modlist::read(&destination.dir).unwrap();

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name, "Owner-ModA");
    assert!(
      destination
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn importing_a_profile_code_installs_the_shared_mods() {
    let mut fixture = Fixture::new();
    let source = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &source,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    export_file(&source).unwrap();

    let archive = std::fs::read_dir(source.exports_dir())
      .unwrap()
      .flatten()
      .next()
      .unwrap()
      .path();
    let zip_bytes = std::fs::read(&archive).unwrap();
    let payload = format!(
      "#r2modman\n{}",
      base64::engine::general_purpose::STANDARD.encode(zip_bytes)
    );

    fixture
      .server
      .mock("GET", "/api/experimental/legacyprofile/get/shared-code/")
      .with_status(200)
      .with_body(payload)
      .create();

    // `fixture.client` only overrides `package_index_url`, so its `base_url`
    // is still the real Thunderstore host. The profile-code endpoints are
    // built from `base_url`, so a client for this path must point it at the
    // mock server too.
    let url = fixture.server.url();
    let index_url = format!("{url}/pkg/");
    let client = ThunderstoreClient::builder()
      .base_url(url)
      .package_index_url(index_url)
      .cache_dir(fixture.base.path())
      .build()
      .unwrap();

    let destination = fixture.profile_target("shared");

    runtime
      .block_on(import(&client, &eco, &destination, "shared-code"))
      .unwrap();

    let mods = modlist::read(&destination.dir).unwrap();

    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name, "Owner-ModA");
    assert!(
      destination
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn importing_the_target_directory_itself_is_refused_before_anything_is_copied() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &target,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    // Stands in for the files a game-dir-mode import would destroy: the loader
    // proxy beside the executable and a hand-edited config.
    let proxy = target.dir.join("winhttp.dll");
    let config = target.dir.join("BepInEx/config/mod.cfg");

    std::fs::write(&proxy, b"loader-proxy-bytes").unwrap();
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, b"hand-tuned = true").unwrap();

    let result = runtime.block_on(import(
      &fixture.client,
      &eco,
      &target,
      target.dir.to_str().unwrap(),
    ));

    assert!(
      result.is_err(),
      "importing the install root into itself must be refused"
    );
    // `fs::copy(p, p)` opens the destination with `O_TRUNC` and returns Ok, so a
    // zero-length file here is the signature of the bug this guards.
    for path in [
      &proxy,
      &config,
      &target.dir.join("BepInEx/plugins/Owner-ModA/ModA.dll"),
      &target.mods_yml(),
    ] {
      let size = std::fs::metadata(path).unwrap().len();

      assert!(
        size > 0,
        "{} was truncated to {} bytes",
        path.display(),
        size
      );
    }
  }

  #[test]
  fn importing_a_directory_adopts_it_as_is() {
    let fixture = Fixture::new();
    let source = fixture.target();
    let eco = Ecosystem::bundled();

    Runtime::new()
      .unwrap()
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &source,
        &["Owner-ModA".to_string()],
      ))
      .unwrap();

    let destination = fixture.profile_target("adopted");

    // A loader-less directory import (this source names only Owner-ModA) must
    // never touch the network. A fresh server (not `fixture.server`, whose
    // mocks are permissive and would mask a regression) carries
    // zero-expectation mocks on the endpoints a `.r2z` or code import would
    // hit, so `.assert()` fails loudly if either is called.
    let mut trap = mockito::Server::new();
    let no_manifest = trap.mock("GET", "/pkg/").expect(0).create();
    let no_download = trap.mock("GET", "/dl/ModA.zip").expect(0).create();
    let trap_client = ThunderstoreClient::builder()
      .package_index_url(format!("{}/pkg/", trap.url()))
      .cache_dir(fixture.base.path())
      .build()
      .unwrap();

    Runtime::new()
      .unwrap()
      .block_on(import(
        &trap_client,
        &eco,
        &destination,
        source.dir.to_str().unwrap(),
      ))
      .unwrap();

    no_manifest.assert();
    no_download.assert();

    // A loader-less directory import is a copy, not a reinstall.
    assert_eq!(modlist::read(&destination.dir).unwrap().len(), 1);
    assert!(
      destination
        .dir
        .join("BepInEx/plugins/Owner-ModA/ModA.dll")
        .exists()
    );
  }

  #[test]
  fn a_directory_import_gives_the_loader_an_install_record() {
    let fixture = Fixture::new();
    let target = fixture.profile_target("destination");
    let eco = Ecosystem::bundled();

    // A source profile shaped the way r2modman leaves one: a mods.yml naming
    // the loader, the loader's files on disk, and no `_state` tracker, because
    // `_state` is this engine's own invention.
    let source = tempfile::TempDir::new().unwrap();

    std::fs::write(source.path().join("mods.yml"), adopted_loader_mods_yml()).unwrap();
    std::fs::create_dir_all(source.path().join("BepInEx/core")).unwrap();
    std::fs::write(source.path().join("winhttp.dll"), b"proxy").unwrap();

    Runtime::new()
      .unwrap()
      .block_on(import(
        &fixture.client,
        &eco,
        &target,
        source.path().to_str().unwrap(),
      ))
      .unwrap();

    // The loader now has an install record, so uninstalling it can remove its
    // files exactly rather than orphaning them.
    assert!(
      thunderstore_engine::install::state_file_path(&target.dir, "denikson-BepInExPack_Valheim")
        .exists(),
      "the adopted loader must end up with a _state tracker"
    );
  }

  #[test]
  fn reinstalling_the_loader_is_skipped_when_it_already_has_a_state_tracker() {
    let fixture = Fixture::new();
    let destination = fixture.profile_target("already-installed");
    let eco = Ecosystem::bundled();
    let runtime = Runtime::new().unwrap();

    // The destination already has the loader properly installed through the
    // normal pipeline, so it already carries a `_state` tracker before the
    // directory import runs.
    runtime
      .block_on(crate::commands::install::run(
        &fixture.client,
        &eco,
        &destination,
        &["denikson-BepInExPack_Valheim".to_string()],
      ))
      .unwrap();
    assert!(
      thunderstore_engine::install::state_file_path(
        &destination.dir,
        "denikson-BepInExPack_Valheim"
      )
      .exists()
    );

    // A source profile that also names the loader, r2modman-shaped as above.
    let source = tempfile::TempDir::new().unwrap();

    std::fs::write(source.path().join("mods.yml"), adopted_loader_mods_yml()).unwrap();
    std::fs::create_dir_all(source.path().join("BepInEx/core")).unwrap();
    std::fs::write(source.path().join("winhttp.dll"), b"proxy").unwrap();

    // A fresh server with zero-expectation mocks on the endpoints a reinstall
    // would hit, so `.assert()` fails loudly if the already-tracked loader is
    // reinstalled anyway. Matches the pattern in
    // `importing_a_directory_adopts_it_as_is`.
    let mut trap = mockito::Server::new();
    let no_manifest = trap.mock("GET", "/pkg/").expect(0).create();
    let no_download = trap
      .mock("GET", "/dl/BepInExPack_Valheim.zip")
      .expect(0)
      .create();
    let trap_client = ThunderstoreClient::builder()
      .package_index_url(format!("{}/pkg/", trap.url()))
      .cache_dir(fixture.base.path())
      .build()
      .unwrap();

    runtime
      .block_on(import(
        &trap_client,
        &eco,
        &destination,
        source.path().to_str().unwrap(),
      ))
      .unwrap();

    no_manifest.assert();
    no_download.assert();
  }
}
