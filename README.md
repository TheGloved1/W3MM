# TauriTemplate

Minimal Tauri v2 + SvelteKit + Svelte 5 + Tailwind v4 template with a scripted
release workflow baked in: version sync, Conventional-Commit changelogs, and
multi-platform GitHub releases via CI.

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

shadcn-svelte (`components.json`, style nova) with bits-ui v2: the registry ships
`data-open:` / `data-checked:` / `data-horizontal:` style selectors, but bits-ui v2
emits `data-state` / `data-orientation`, so every primitive under
`src/lib/components/ui` has been rewritten (`data-open:` →
`data-[state=open]:`, etc.). `data-disabled:` / `data-highlighted:` /
`data-placeholder:` are genuinely emitted and were left alone. Re-running
`shadcn-svelte add -o` reverts the patches — re-apply them afterwards.

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
