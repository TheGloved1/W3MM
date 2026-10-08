# YAWMM — Yet Another Witcher Mod Manager

A mod manager for The Witcher 3: install, update, reorder, and remove mods with an eye on script/XML conflicts before they break your game. Runs on Linux (Steam/Proton) and Windows (native builds are in beta).

## Features

- **Mod list** — enable/disable, drag to reorder (priority), sections, multi-select with bulk actions, filter, install dates.
- **One-click installs** — `.zip` / `.7z` / `.rar` / `.tar` archives, with an Archive-contents preview and per-root Mod/DLC/Bin/Content mapping. Update, reinstall, and replace flows keep load-order position.
- **Nexus downloads** — catches `nxm://` "Mod Manager Download" links, downloads with resume/pause, and opens the Install window when done. Needs a free Nexus API key in Settings.
- **Update checks** — compares your versions against Nexus (manual checks, cached results, premium direct downloads or free slow-download pages).
- **Conflict insight** — shared-script/XML chips, overridden-file counts, RedKit annotation clashes, and made-for-game-version badges.
- **Script decisions** — built-in 3-way `.ws` merger plus XML merger, merge results kept across deploys; external Script Merger path check in Settings.
- **Safe deploys** — mods are staged outside the game folder and hardlinked in (symlink fallback across drives on Unix, copy elsewhere); vanilla originals are backed up and restored on disable/uninstall; orphaned files get reconciled and menu filelists pruned.
- **Unmanaged mods** — folders already sitting in `mods/` can be adopted with one click.

## Where mods live

App data (staging, backups, mod list, downloads) lives outside the game folder, keyed per install:

- Linux: `~/.local/share/yawmm/<game>/`
- Windows: `%LOCALAPPDATA%\yawmm\<game>\`

Settings (`mods.settings`, keybinds) stay where the game expects them: the Proton prefix's `Documents/The Witcher 3` on Linux, `%USERPROFILE%\Documents\The Witcher 3` on Windows. Keep staging on the same drive as the game — cross-drive installs silently fall back to full copies (slower, twice the disk), and the app will say so. You can point staging anywhere in Settings.

## Run it

Grab the latest release from the [releases page](https://github.com/TheGloved1/YAWMM/releases) (`.deb`, `.AppImage`, `.rpm`, or Windows `.exe`/`.msi` while native support is in beta), install it, and point it at your Witcher 3 folder — it auto-detects Steam installs. Paste a Nexus API key into Settings to enable downloads and update checks.

<details>
<summary>Running from source</summary>

Requires [Bun](https://bun.sh/), [Rust](https://www.rust-lang.org/), and the Tauri system libs:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
bun install
bun tauri dev
```
</details>

## License

MIT — see [LICENSE](LICENSE).
