# AGENTS.md

## Build & Run

```bash
cargo build --release
cargo run -- <command>
```

## Commands

- `env` - Manage environment configurations (e.g., DevEco Studio paths)
  - `add` - Add a DevEco Studio path (auto-extracts apiVersion/version)
  - `remove` - Remove a DevEco Studio path or version
  - `list` - List current environment configurations
- `build` - Build modules(s) and product(s) (supports `-m/--modules` and `--products`)
- `clean` - Clean build artifacts and uninstall application from devices (supports `--with-devices` and `--with-all-devices`)
- `lint` - Run code linter (codelinter) and fix issues (supports `--fix` and `--products`)
- `emulator` - Manage emulator instances (`start`, `stop`, `list`)
- `run` - Run application on a device or emulator (supports `--daemon`, `--app-log-level`)
- `device` - Manage device(s), include emulator and physical device (`list`)
- `completion` - Generate shell completion scripts (supports unstable-dynamic completion)
- `update` - Update heco to the latest version (auto-detects Cargo/Homebrew/Winget)

## Key Config Paths

- Global: `~/.config/heco/config.toml`

Available in config:
- `env` block containing:
  - `deveco-studios` - A mapping of API versions to their DevEco Studio config/paths
  - `default-deveco-studio` - Fallback/default DevEco Studio path

## Architecture

- **CLI Framework**: Rust CLI using `clap` with `clap_complete` dynamic shell completion
- **Adapters**: `hvigor`, `hdc` (Device interaction), `hilog` (Log streaming)
- **Configuration**: Exclusively user-level global config (no project-level `.heco` config anymore). Path expansion supports `~` (tilde)
- **Auto-Detection**: 
  - Parses `build-profile.json5` and `oh-package.json5` to ensure command execution within valid HMOS projects
  - Auto-detects DevEco Studio at default macOS/Windows paths
  - Auto-extracts SDK and toolchain paths directly from DevEco Studio installation directory
