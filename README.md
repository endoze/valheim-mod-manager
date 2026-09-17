# Valheim Mod Manager (vmm)

[![Build Status](https://github.com/endoze/valheim-mod-manager/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/endoze/valheim-mod-manager/actions?query=branch%3Amaster)
[![Coverage Status](https://coveralls.io/repos/github/endoze/valheim-mod-manager/badge.svg?branch=master)](https://coveralls.io/github/endoze/valheim-mod-manager?branch=master)
[![Crate](https://img.shields.io/crates/v/valheim-mod-manager.svg)](https://crates.io/crates/valheim-mod-manager)

A command-line tool for managing and automatically downloading Valheim mods and their dependencies.

## Features

- Installs mods and their full dependency closure from Thunderstore
- Tracks what is installed in an r2modman-compatible `mods.yml`, so an install
  can be uninstalled exactly, without guessing from directory contents
- Enables and disables mods in place, without uninstalling them
- Installs into your game directory by default; named profiles are opt-in
- Launches the game with a profile's mods applied
- Exports and imports profiles as `.r2z` files or Thunderstore profile codes
- Shares one download and extraction cache across every target
- Shows live progress while it works: a spinner for the package index fetch and
  a byte bar per mod download

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/Endoze/valheim-mod-manager.git
cd valheim-mod-manager

# Install the application
cargo install --path .

# The binary will be installed in your Cargo bin directory
```

## Configuration

`vmm` looks for configuration in the following order:

1. A `vmm_config.toml` in the current directory (local config)
2. `~/.config/vmm/vmm_config.toml` (global XDG config)

If neither exists, a default config is created at the global location on first run.

You can also specify a config file directly with the `--config` flag, which bypasses the above lookup entirely.

The config file supports the following settings:

- `log_level`: Logging verbosity (`error`, `warn`, `info`, `debug`, `trace`)
- `game_dir`: Your Valheim game folder, the directory holding the game
  executable, where the mod loader is installed
- `data_dir`: Optional. Where the package cache, exports, and profiles live.
  Defaults to `~/.config/vmm`
- `[launch] store`: `steam`, `steam-direct`, or `other`
- `[launch] runtime`: `native` or `proton`
- `[launch] extra_args`: Extra arguments passed to the game on every launch
- `mod_list`: **Deprecated.** Superseded by `mods.yml`. Run `vmm migrate` to
  adopt it, then delete the key
- `install_dir`: **Deprecated.** Superseded by `game_dir`. If only `install_dir`
  is set, `vmm` refuses to install and prints migration guidance instead, since
  the on-disk layout changed along with the key

`game_dir` and `data_dir` both expand a leading `~` and environment variables,
so `~/.steam/...` and `$HOME/.steam/...` are equivalent. A variable that is not
set is left as written. `vmm` refuses to run rather than installing into a
`game_dir` that does not exist, so a typo is reported instead of silently
creating a fresh mod tree somewhere else.

Example configuration:

```toml
log_level = "info"
game_dir = "~/.steam/steam/steamapps/common/Valheim"

[launch]
store = "steam"
runtime = "native"
extra_args = []
```

### Upgrading from a `mod_list` config

`mods.yml` inside your target directory is now the record of what is installed,
replacing `mod_list`. To adopt an existing list:

```bash
vmm migrate
```

This installs every mod in `mod_list`, writes `mods.yml`, and removes the
folders of any mods you had dropped from the list under the old installer. Then
delete `mod_list` from your config.

## Usage

### Managing mods

```bash
# Install mods and their dependencies
vmm install denikson-BepInExPack_Valheim ValheimModding-Jotunn

# List what is installed
vmm list
vmm list --format json

# Take a mod out of play without uninstalling it
vmm disable ValheimModding-Jotunn
vmm enable ValheimModding-Jotunn

# Disable or enable every installed mod at once (the mod loader is left alone)
vmm disable --all
vmm enable --all

# Remove a mod, its files, and its tracked state
vmm uninstall ValheimModding-Jotunn

# Remove it even though unrecorded mod folders are present (deletes those too)
vmm uninstall --force ValheimModding-Jotunn

# Remove every installed mod, including the mod loader, leaving a vanilla install
vmm uninstall --all
vmm uninstall --all --yes

# Update every installed mod to its latest version
vmm update mods

# Refresh the cached package index
vmm update manifest

# Search Thunderstore
vmm search jotunn
```

`vmm enable --all` and `vmm disable --all` apply to every mod recorded in the
current target's `mods.yml`; the mod loader is left out of the batch, since it
has no disabled state. A mod already in the requested state is counted
separately as unchanged rather than as a failure. If one mod's toggle fails,
the rest of the batch still applies and `vmm` exits non-zero, naming what
failed. Like every other command, `--all` respects `--profile` and
`--no-profile`, applying to that target instead of `game_dir`.

`vmm update mods` reinstalls every recorded mod on every run, not only when a
mod's version changes. That overwrites any file a mod packages inside its own
folder under `BepInEx/plugins/`, including hand edits you've made to those
files. Files you place directly under `BepInEx/config/` are left alone if they
already exist. Mods you have disabled stay disabled.

#### `mods.yml` owns the whole mod tree

`mods.yml` is authoritative over every per-mod folder under the install routes
(`BepInEx/plugins/<Owner-ModName>/` and the other namespaced routes), not just
over the mods `vmm` happens to have installed. An uninstall reconciles that whole
tree against `mods.yml`, so **any `<Owner-ModName>` folder it does not record can
be removed by an uninstall**, including mods you installed by hand, mods
r2modman put there, and mods left by an older `vmm` whose `mod_list` was never
migrated. In `game_dir` mode that tree is your live game directory.

`vmm uninstall` therefore checks first: if it finds mod folders `mods.yml` does
not record, it names them and refuses without changing anything. Bring them under
management (`vmm install <Owner-ModName>`, or `vmm migrate` for a deprecated
`mod_list`), or re-run with `--force` to remove the named mods and accept that
those folders go too. Folders that are not `<Owner-ModName>`-shaped, such as
loader directories like `BepInEx/plugins/MMHOOK` and anything directly under
`BepInEx/config/`, are never touched.

`vmm uninstall --all` removes every mod recorded for the current target,
including the mod loader, leaving a vanilla install; files under
`BepInEx/config` are left alone regardless. It asks for confirmation before
removing anything, listing what will go, and `--yes` skips that prompt. With no
terminal available and no `--yes`, it refuses rather than assuming an answer.

Untracked mod folders are handled a little differently here than for a
single-mod uninstall: the confirmation prompt lists them too, so accepting it
covers them without needing `--force`. `--yes` skips that same prompt, though,
so on that path `--force` is still required to accept those folders being
removed alongside everything else. As with any other target-scoped command,
`--profile` and `--no-profile` choose which target `--all` operates on.

### Profiles

Mods install into `game_dir` by default. Named profiles keep separate mod sets
outside the game directory, in a layout r2modman can read:

```bash
vmm profile create experiment
vmm profile list
vmm --profile experiment install ValheimModding-Jotunn

# Select a profile for subsequent commands
vmm profile use experiment
vmm install SomeOther-Mod          # goes to `experiment`
vmm profile clear                  # back to game_dir
vmm --no-profile list              # ignore the selection for one command

vmm profile duplicate experiment experiment-2
vmm profile rename experiment-2 stable
vmm profile delete stable
```

### Launching

A profile lives outside the game directory, so the loader has to be pointed at
it. In `game_dir` mode the loader is already installed beside the game and Steam
launches it normally; `vmm launch` is only needed for profiles.

```bash
vmm launch
vmm launch --vanilla
vmm launch -- -console
```

On Linux with a native (non-Proton) Steam copy, injection goes through a wrapper
script that Steam must invoke. Steam only reads launch options from its own
config, so pasting them is a one-time manual step:

```bash
vmm launch --print-steam-options
```

Paste the printed string into Valheim's Steam launch options.

### Sharing

```bash
# Write an .r2z into <data_dir>/valheim/exports/
vmm export

# Upload and print a shareable Thunderstore profile code
vmm export --code

# Import from a file, a profile code, or an r2modman profile directory
vmm import ./default_1753488000.r2z
vmm import a1b2c3d4-0000-0000-0000-000000000000
vmm import ~/.config/r2modmanPlus-local/Valheim/profiles/Default
```

A file or profile-code import installs each mod's **latest** version, not the
version the export pinned, and prints what it installed so any difference is
visible.

Importing a directory adopts it as-is at the versions the source recorded, and
downloads nothing, with one exception: if the adopted `mods.yml` names a mod
loader, the loader is reinstalled from Thunderstore afterward (at its
**latest** version, same as above) so it gains the install record that makes
it manageable and removable; a source naming no loader stays fully offline. It
is also a raw copy with no pre-clean: if the destination already has different
mods installed, `mods.yml` is overwritten while the previous mods' files stay
on disk, now untracked and orphaned. Import into an empty target, or accept
that leftover.

## Global Options

### `--config <path>`

Override the config file location, bypassing the local/global lookup:

```bash
vmm --config /path/to/my/vmm_config.toml update mods
```

### `--profile <name>` / `--no-profile`

Operate on a named profile, or force game-dir mode, for a single invocation,
overriding any selection made with `vmm profile use`. The two flags conflict
and cannot be combined:

```bash
vmm --profile experiment install ValheimModding-Jotunn
vmm --no-profile list
```

Downloads and cached data always go to `data_dir` (or `~/.config/vmm` if unset)
regardless of which config file is used. Respects `$XDG_CONFIG_HOME` when
`data_dir` is not set.

## How It Works

1. Resolves the target to operate on: `game_dir`, or a selected profile under
   `data_dir`
2. Downloads the mod manifest from Thunderstore, caching it under `data_dir`
3. Resolves the full dependency closure for the requested mods
4. Downloads and extracts each package into the shared package cache, skipping
   anything already fetched at the required version
5. Installs each package into the target using the mod loader's install rules,
   and records it in the target's `mods.yml`

## Directory Structure

- `data_dir` (or `~/.config/vmm` if unset, respecting `$XDG_CONFIG_HOME`):
  holds the downloaded package cache, `.r2z` exports, and named profiles,
  under `data_dir/valheim/`
- Your target directory (`game_dir`, or a profile directory under
  `data_dir/valheim/profiles/<name>/`) holds the installed mod files,
  `mods.yml`, and any loader state

## Troubleshooting

If you encounter issues, increase log verbosity in your config:

```toml
log_level = "debug"
```

Then run the command again to see more detailed output.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [Thunderstore](https://thunderstore.io) for hosting Valheim mods
- The amazing Valheim modding community
