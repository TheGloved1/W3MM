# W3MM

A mod manager for The Witcher 3 on Linux (Steam/Proton). Install, update, reorder, and remove mods with an eye on script/XML conflicts before they break your game.

## Features

- **Mod list** — enable/disable, drag to reorder (priority), sections, filter, install dates.
- **One-click installs** — `.zip` / `.7z` / `.rar` archives, with an Archive-contents preview and per-root Mod/DLC/Bin/Content mapping.
- **Nexus downloads** — catches `nxm://` "Mod Manager Download" links, downloads with resume, and opens the Install window when done. Needs a free Nexus API key in Settings.
- **Update checks** — compares your versions against Nexus.
- **Conflict insight** — shared-script/XML chips, overridden-file counts, RedKit annotation clashes, and made-for-game-version badges.
- **Script decisions** — built-in 3-way `.ws` merger plus XML merger, no external Script Merger needed.
- **Safe deploys** — mods are staged under `<game>/_W3MM/` and hardlinked into the game; originals are backed up and restored on disable/uninstall.

## Run it

Requires [Bun](https://bun.sh/), [Rust](https://www.rust-lang.org/), and the Tauri system libs:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
bun install
bun tauri dev
```

Point it at your Witcher 3 folder (it auto-detects Steam installs), paste a Nexus API key into Settings, and install mods. Switching from the old Python manager? Your `_ModManager` data is never touched.

## License

MIT — see [LICENSE](LICENSE).
