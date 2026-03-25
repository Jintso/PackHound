# PackHound

A native GTK4/libadwaita GUI for managing World of Warcraft addons on Linux.
Download and update addons from GitHub releases and CurseForge — for all WoW versions.

## Features

- **Install addons** from GitHub repository URLs or CurseForge URLs
- **All WoW flavors:** Retail, Classic Era, Classic (progression)
- **Automatic update checking** on launch with one-click "Update All"
- **Multi-folder addon support** — addons like BigWigs that extract multiple folders are consolidated automatically
- **Smart display names** from `.toc` files with WoW UI color/escape code stripping
- **Dependency info** parsed from `.toc` `RequiredDeps`/`Dependencies` fields
- **Release dates** from GitHub, shown per addon
- **Asset picker** — when multiple zip assets match a release, choose the right one
- **Right-click context menus:**
  - Open addon's source page in browser
  - Change source URL
  - Mark as externally tracked (managed by another app)
  - Remove from list
  - Track untracked addons
- **Sort & filter** — sort by name or update status; toggle to hide externally tracked addons
- **Auto-detection** of Wine, Lutris, Bottles, and Heroic Game Launcher WoW installations
- **Manual path configuration** with per-flavor overrides

## Requirements

- Rust (stable toolchain)
- GTK4 ≥ 4.12
- libadwaita ≥ 1.4
- `pkg-config`

On Arch/CachyOS: `sudo pacman -S gtk4 libadwaita pkgconf`

## Build & Run

```sh
cargo build --release
./target/release/addon-manager
```

## Install

```sh
# Build
cargo build --release

# Copy binary
install -Dm755 target/release/addon-manager ~/.local/bin/addon-manager

# Install .desktop file
install -Dm644 com.github.packhound.desktop \
    ~/.local/share/applications/com.github.packhound.desktop
```

## Configuration

Config is stored in `~/.config/addon-manager/`:
- `config.toml` — WoW root path, optional GitHub token, and CurseForge API key
- `addons.json` — installed addon registry

Open **Preferences** (⚙) to set your WoW installation path, a GitHub
personal access token (increases API rate limit from 60 to 5,000 req/hr),
or your CurseForge API key.

## WoW Installation Path

Point PackHound at your WoW root directory — the one that contains
`_retail_/`, `_classic_/`, `_classic_era_/` subdirectories. Common locations:

| Setup | Typical path |
|-------|-------------|
| Lutris | `~/Games/world-of-warcraft/drive_c/Program Files (x86)/World of Warcraft` |
| Heroic | `~/Games/Heroic/Prefixes/default/Battle.net/drive_c/Program Files (x86)/World of Warcraft` |
| Plain Wine | `~/.wine/drive_c/Program Files (x86)/World of Warcraft` |
| Bottles | `~/.local/share/bottles/bottles/World of Warcraft/drive_c/Program Files (x86)/World of Warcraft` |

Use **Auto-detect** in Preferences to try common paths automatically.

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
