# Valheim Mod Manager (vmm)

[![Build Status](https://github.com/endoze/valheim-mod-manager/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/endoze/valheim-mod-manager/actions?query=branch%3Amaster)
[![Coverage Status](https://coveralls.io/repos/github/endoze/valheim-mod-manager/badge.svg?branch=master)](https://coveralls.io/github/endoze/valheim-mod-manager?branch=master)
[![Crate](https://img.shields.io/crates/v/valheim-mod-manager.svg)](https://crates.io/crates/valheim-mod-manager)

A command-line tool for managing and automatically downloading Valheim mods and their dependencies.

## Features

- Automatically downloads mods from Thunderstore
- Resolves and downloads all dependencies
- Tracks downloaded mod versions
- Only updates when new versions are available
- Displays download progress with intuitive UI

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

- `mod_list`: List of mods to install (in the format `"Owner-ModName"`)
- `log_level`: Logging verbosity (`error`, `warn`, `info`, `debug`, `trace`)
- `install_dir`: Optional directory where unzipped mods will be copied (e.g., your Valheim mods folder)

Example configuration:

```toml
mod_list = ["denikson-BepInExPack_Valheim", "ValheimModding-Jotunn"]
log_level = "info"
install_dir = "~/some/path/to/Valheim/BepInEx/plugins"
```

## Usage

### Update Mod Manifest

Fetches the latest information about available mods from Thunderstore:

```bash
vmm update manifest
```

### Update Mods

Downloads all mods in your configuration, including their dependencies:

```bash
vmm update mods
```

### List Mods

Lists all mods from your configuration including resolved dependencies:

```bash
vmm list
```

By default outputs one mod per line. Use `--format json` for structured output:

```bash
vmm list --format json
```

**Text output (default):**
```
  denikson-BepInExPack_Valheim 5.4.2202
  ValheimModding-Jotunn 2.28.0
```

**JSON output:**
```json
[
  {"full_name": "denikson-BepInExPack_Valheim", "version": "5.4.2202"},
  {"full_name": "ValheimModding-Jotunn", "version": "2.28.0"}
]
```

### Search for Mods

Searches available mods by name:

```bash
vmm search <term>
```

This performs a case-insensitive search for mods containing the specified term in their name, displaying matching mods with their owner, name, version, and description.

## Global Options

### `--config <path>`

Override the config file location, bypassing the local/global lookup:

```bash
vmm --config /path/to/my/vmm_config.toml update mods
```

Downloads and cached data always go to `~/.config/vmm/` regardless of which config file is used. Respects `$XDG_CONFIG_HOME` if set.

## How It Works

1. Reads your configuration to determine which mods to download
2. Downloads the mod manifest from Thunderstore
3. Builds a dependency graph for all required mods
4. Downloads required mods and their dependencies
5. Extracts mod files, skipping those already downloaded with matching versions
6. If `install_dir` is configured, copies the extracted mods to that directory

## Directory Structure

- `~/.config/vmm/`: Config and cache directory (respects `$XDG_CONFIG_HOME`)
- `~/.config/vmm/downloads/`: Downloaded mod archives and extracted files

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
