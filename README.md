# Valheim Mod Manager (vmm)

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

The first time you run `vmm`, it will create a default configuration file at `vmm_config.toml` in your current directory. You can edit this file to customize:

- `mod_list`: List of mods to install (in the format `"Owner-ModName"`)
- `log_level`: Logging verbosity (`error`, `warn`, `info`, `debug`, `trace`)
- `cache_dir`: Directory to store cached mod information (default: `~/.config/vmm`)
- `install_dir`: Optional directory where unzipped mods will be copied (e.g., your Valheim mods folder)

Example configuration:

```toml
mod_list = ["denikson-BepInExPack_Valheim", "ValheimModding-Jotunn"]
log_level = "info"
cache_dir = "~/.config/vmm"
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

### Search for Mods

Searches available mods by name:

```bash
vmm search <term>
```

This performs a case-insensitive search for mods containing the specified term in their name, displaying matching mods with their owner, name, version, and description.

## How It Works

1. Reads your configuration to determine which mods to download
2. Downloads the mod manifest from Thunderstore
3. Builds a dependency graph for all required mods
4. Downloads required mods and their dependencies
5. Extracts mod files, skipping those already downloaded with matching versions
6. If `install_dir` is configured, copies the extracted mods to that directory

## Directory Structure

- `~/.config/vmm/` (or custom `cache_dir` setting in config): Cache directory for mod information
- `~/.config/vmm/downloads/` (or custom `cache_dir/downloads`): Location for downloaded mod archives and extracted files

## Advanced Features

### Troubleshooting

If you encounter issues:

1. Increase log verbosity in your config:
   ```toml
   log_level = "debug"
   ```
2. Run the command again to see more detailed output

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [Thunderstore](https://thunderstore.io) for hosting Valheim mods
- The amazing Valheim modding community
