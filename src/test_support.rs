//! Shared fixtures for command tests: the engine's mock Thunderstore plus the
//! directories a command needs, so the install pipeline runs end to end with no
//! network access.

use crate::target::Target;
use tempfile::TempDir;
use thunderstore_engine::client::ThunderstoreClient;
use thunderstore_engine::test_support::MockThunderstore;

/// A mock Thunderstore plus a client, base directory, and game directory.
pub struct Fixture {
  /// The mock Thunderstore backing this fixture's client.
  pub mock: MockThunderstore,
  /// A client pointed at `mock`, caching under `base`.
  pub client: ThunderstoreClient,
  /// The engine base directory (`<base>/<GAME>/{cache,exports,profiles}`).
  pub base: TempDir,
  /// The simulated game directory.
  pub game_dir: TempDir,
}

impl Fixture {
  /// Builds a fixture whose index and downloads are served from the mock.
  pub fn new() -> Self {
    let mock = MockThunderstore::start();
    let base = TempDir::new().unwrap();
    let game_dir = TempDir::new().unwrap();
    let client = mock
      .client_builder()
      .cache_dir(base.path())
      .build()
      .unwrap();

    Self {
      mock,
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
