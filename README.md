# W3LMN

Witcher 3 Legacy Mod Nexus manager for Linux (Proton/Wine).

Tauri 2 + Rust + Svelte 5 port of `w3modmanager.py` (full-parity track: script merger + XML merger + Nexus + keybinds). Clean break: home is `<game>/_W3LMN/` (state/staging/backup), never touches `_ModManager`. Linux-only bundles (deb/appimage/rpm). Extraction: pure-Rust `zip`/`sevenz-rust`/`tar`/`flate2` plus RAR via the vendored unrar library (statically linked, no system tools needed; RAR 1.5–5.x including multipart).

Ships with: collapsible sidebar layout, shared `PageHeader` / `DataList` /
`SortHeader` components, shadcn-style UI components, 8 themes + 6 fonts with
instant switching, Tauri-store-backed settings, and a Settings page demo.

## Setup (TODO)

1. Rename the app everywhere `TauriTemplate` / `tauri-template` appears:
   - `package.json` (name, repository, homepage, bugs, author)
   - `index.html` (`<title>`)
   - `src-tauri/Cargo.toml` (`name`, `description`, `authors`, `[lib] name`)
   - `src-tauri/tauri.conf.json` (`productName`, `identifier`, `bundle.homepage`, window `title`)
   - `.github/workflows/release.yml` (`APP_NAME`)
   - `scripts/generate-updater-json.sh` (`APP_NAME`, `GITHUB_REPO` — only if you enable the updater)
2. Fill in `LICENSE` (`<YEAR>`, `<YOUR NAME>`).
3. Replace `src-tauri/icons/` — generate from your own 1024×1024 PNG:
   ```bash
   bunx tauri icon /path/to/icon.png
   ```
4. Replace the placeholder UI in `index.html` / `src/` and the `greet` command in `src-tauri/src/lib.rs`.
5. `bun install && bun tauri dev`

Requires [Bun](https://bun.sh/) and [Rust](https://www.rust-lang.org/).
Linux needs the Tauri system deps:
```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

Frontend notes: SvelteKit with `adapter-static` SPA fallback (`src/routes/+layout.ts`
sets `ssr = false`) — required for Tauri. Run `bun run check` for type checks.

UI foundation: daisyUI v5 (`@plugin "daisyui"` in `src/app.css`, dark theme as
the fallback so the template's own `[data-theme="…"]` blocks keep winning) for
styled components, plus bits-ui v2 directly for real interactivity. The five
primitives under `src/lib/components/ui` (`button`, `card`, `label`,
`separator`, `select`, `table`) are thin hand-written wrappers with stable
import paths — button/card/label/separator/table are daisyUI classes, select is
bits-ui with selectors matching its v2 output (`data-[state=…]`,
`data-[side=…]`, `data-highlighted:`). No registry, nothing to patch: add more
as needed following the same pattern.

## Releasing

Versions use `YY.MM.PATCH` (e.g. `26.1.0`) — `scripts/release.ts` only parses
that scheme, so don't start at `0.1.0`. Write Conventional Commits
(`feat:`, `fix:`, …) — the changelog is generated from them.

```bash
bun run release patch     # 26.1.0 -> 26.1.1 (bumps, changelogs, commits, tags, pushes)
bun run release --dry-run # preview without changing anything
bun run release --undo    # revert the most recent release
bun scripts/release.ts changelog patch  # preview changelog only
```

Pushing a `v*` tag triggers CI: version check → `sync-version`
(`package.json` → `Cargo.toml`/`Cargo.lock`/`tauri.conf.json`) → multi-platform
`tauri build` → canonical bundle names → draft release → per-file asset upload
with retries → publish. Release notes come from `changelogs/vX.Y.Z.md`.

No `.env` file is needed to build. Signing keys (updater only) come from env
vars / GitHub Secrets and are never committed (see `.gitignore`).

## Enabling the updater (optional)

Everything you need is stubbed and marked `UPDATER`. Steps:

1. **Keys**: generate once with `bun tauri signer generate -w ~/.tauri/myapp.key`,
   then add `TAURI_SIGNING_PRIVATE_KEY` (contents of the `.key` file) as a
   GitHub repo secret. Keep the `.pubkey` value for step 3.
2. **`src-tauri/Cargo.toml`**: uncomment the `tauri-plugin-updater = "2"` line.
3. **`src-tauri/src/lib.rs`**: uncomment the updater `.plugin(...)` line.
4. **`src-tauri/capabilities/default.json`**: add `"updater:default"` to `permissions`.
5. **`src-tauri/tauri.conf.json`**: add inside `"bundle"`:
   ```json
   "createUpdaterArtifacts": true
   ```
   and add a top-level `"plugins"` section:
   ```json
   "plugins": {
     "updater": {
       "endpoints": [
         "https://github.com/<OWNER>/<REPO>/releases/latest/download/updater.json"
       ],
       "pubkey": "<PASTE .pubkey CONTENT HERE>"
     }
   }
   ```
   (`createUpdaterArtifacts` is what produces the macOS `.app.tar.gz` the
   updater requires — never the `.dmg`.)
6. **`.github/workflows/release.yml`**: uncomment the three `UPDATER` blocks
   (private-key env on the build step, fragment generation, fragment combining)
   and flip the expected file count 6 → 7.
7. **`scripts/generate-updater-json.sh`**: set `APP_NAME` / `GITHUB_REPO` (or pass
   as env vars) — already wired, just needs the values.

## License

MIT — see [LICENSE](LICENSE).
