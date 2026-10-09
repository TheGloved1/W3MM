# [26.10.20] - 2026-10-09

## Added

- profiles:
```
- single store with saved enable-selections
- seed a Default profile on first open
- NMM-style active/duplicate/rename flow
- fully separate mod sets per profile dir (NMM-style)
```

- ui:
```
- add Open App data menu entry
- return to Mods when an install finishes
```

## Fixed

- **install**: ```add entry and failure logging to reinstall flow```
- **ui**: ```pin downloads badge to icon in collapsed sidebar```
- **import**: ```release state lock before save to prevent self-deadlock```

- profiles:
```
- add active_profile to frontend AppState type
- ignore stale active-profile set when checking mod references
```

- downloads:
```
- stop duplicate nxm handling from listeners and tool windows
- load history once at boot, never clobber live rows
- reinstall button opens the install window again
- collapse done downloads by default, non-installed no longer auto-expanded
- resolve download paths without requiring an open manager
```

## Changed

- **frontend**: ```route all Tauri invokes through $lib/api```

## Chores

- **build**: ```drop release-status script and type appVersion as string```

# [26.10.18] - 2026-10-08

## Added

- **release**: ```single-shot undo with recency guard```

- windows-compat:
```
- platform path layer and rel canonicalization
- Windows Steam roots, VDF parsing, process guard
- native merger path mapping
- hide prefix field on native Windows
- re-enable Windows CI builds, gate Unix inode check
```

- xdg-data:
```
- relocate app data out of the game folder
- user staging override with split-drive notice
- prefill staging field with the effective default
- symlink fallback for cross-device deploys on unix
- require an explicit split-drive staging choice
```

## Fixed

- **release**: ```correct undo log path in success message```

- windows-compat:
```
- compare volume roots instead of unstable serial API
- bind canonicalized path before borrowing components
```

# [26.10.17] - 2026-10-07

## Fixed

- **install**: ```move picked archives into downloads before installing```
- **release**: ```handle push failures without a stack dump```

# [26.10.16] - 2026-10-07

## Fixed

- **install**: ```recover when the recorded archive is gone```

# [26.10.15] - 2026-10-07

## Fixed

- **install**: ```preserve genuine bin/content trees, normalize top-dir case```
- **deploy**: ```track filelist entries and prune on removal```

# [26.10.14] - 2026-10-06

## Fixed

- **deploy**: ```reconcile orphans lost by deployed map, prune filelists```

# [26.10.13] - 2026-10-06

## Fixed

- **ui**: ```lift the "..." menu above the mod list header```

# [26.10.12] - 2026-10-05

## Fixed

- ui:
```
- stick mod list header via cells and collapse duplicate borders
- restore mod list scrolling and hide column resize borders
- constrain main column height and drop backdrop blur on sticky header
- move mod list header out of the scroll container
- lock header columns to measured body widths
- read raw items from SvGrid callbacks, destructure cell params
- pin every mod list column except Mod to its declared width
- show priority numbers on disabled mods without shifting the column
- stop priority cell height changing when a mod is toggled
- stop the priority number encoding enabled state
- render priority as one input element in both states
```

## Changed

- ui:
```
- rebuild mod list on @svgrid/grid
- bump mod list row height to 42px
- drop cell-level selection affordances from the mod list
- left-align priority numbers and slim the status column
- move status column after the installed date
- attach selection toolbar flush to the mod list
- tint selection toolbar just below the list background
- dim the name and priority of a disabled mod
```

# [26.10.11] - 2026-10-05

## Fixed

- install:
```
- honor edited root kind and route game dirs to the game folder
- live file preview and folder-free rows for game dir kinds
- derive dialog row prefix from path so kind edits apply
```

# [26.10.10] - 2026-10-05

## Fixed

- install:
```
- treat bare mod folders as mods/<mod> instead of game content
- qualify all game dirs inside bare mod folders as one mod
- normalize mod folders nested inside game layouts
```

# [26.10.9] - 2026-10-05

## Added

- **shell**: ```log AppImage status, launch env, and resolved open handler```
- **release**: ```group chore commits under Chores section```

## Fixed

- release:
```
- use one level shallower changelog headers
- keep scope subheaders at ####
- dash scope subheaders in changelogs
- version headers use single hash
- detect legacy headers when rebuilding changelog
- cancel reverted commits instead of echoing them in Other
```

- shell:
```
- Revert: open folders and links via detached backend command
- sandbox-aware detached opener bypassing AppImage xdg-open
- scrub AppImage theme and data paths for opened apps
- scrub mount entries from every inherited env var
- drop bundle-forced GTK_THEME for opened apps
```

## Chores

- release:
```
- fenced scope groups with retroactive changelog rebuild
- drop changelog preamble
```

- shell:
```
- log resolved opener tool and desktop env
- capture xdg-open stderr instead of discarding it
- log scrubbed vars and handler Exec line
- log session type, backend, and GTK theme at launch
```

# [26.10.8] - 2026-10-04

## Added

- **release**: ```group duplicate scopes under subheaders in changelogs```

- mods:
```
- add multi-select with selection actions toolbar
- scoped update checks with selection-aware update actions
- retain load-order spot on replace install and auto-prune update banner
- cache known versions and make update banner a dismissible popup
- persist update version cache and keep checks fully manual
- update count badge on menu with Updates row and boolean banner dismissal
- boot banner from cache with dismissed-only update badge
- regrow known updates on refresh and install updates from local downloads
```

## Fixed

- **shell**: ```open folders and links via detached backend command```

- ui:
```
- disable broken update-check progress bar
- move updates banner chevron to the start
- shorten updates banner to count only
- shorten update chip tooltip to version range
- shorten dismiss updates tooltip
- give downloads cards more horizontal room
- flatten nested download rows to reclaim card width
- only expand downloads needing attention on startup
```

- mods:
```
- keep priority mirroring list order across moves and replaces
- fuzzy local update matching by queue row and archive stamp
```

- downloads:
```
- settle multi-version groups when installed file is present
- label installed file rows Installed instead of Downloaded
```

## Changed

- mods:
```
- instant update feedback with cached premium check and progress
- resolve updates on background thread with update-resolved event
- cache update file targets for instant zero-network updates
- unify update checks and pop banner on new finds
- fully async updates with premium resolved in worker
```

## Other

- Revert "fix(ui): only expand downloads needing attention on startup"

# [26.10.7] - 2026-10-04

## Fixed

- **ui**: ```remove stray bracket next to Install mods button```

## Changed

- **ui**: ```unify buttons into single Button component and fix ModForm grid stretch```

# [26.10.6] - 2026-10-04

## Fixed

- **mods**: ```show content-kind files in Edit and retain metadata on reinstall```

- ui:
```
- restore drag-and-drop and reduce header border thickness
- unify PopoverButton padding and align Edit/Install buttons
```

## Changed

- ui:
```
- extract CodeViewer and SecondaryButton components
- extract FormInput component for consistent input styling
- extract Panel component and use in merges view
- extract FormSelect component and use consistently
- extract ExpandableSection component
- extract PrimaryButton component and adopt across routes
- reuse ExpandableSection in install view
- extract PopoverButton and replace duplicated popover button markup
- extract Badge component
- extract ModForm component shared by Edit and Install
- extract FormActions, ArchiveRootsList and reuse across Edit/Install/Settings
- extract LabeledField and apply to Settings form rows
- extract FilesSection component and reuse in Edit/Install
- extract MergerSection component from Settings
- extract PageHeader component and apply to Edit/Install/Settings
- extract WarningBanner component and reuse across forms
- extract CollisionNotice component from Install page
```

# [26.10.5] - 2026-10-03

## Fixed

- **install**: ```normalize relative paths```

## Other

- **logging**: ```add detailed debug logs for install staging and root mapping```

# [26.10.4] - 2026-10-03

## Added

- **scripts**: ```show release assets links after successful workflow```

## Other

- **repo**: ```update owner URL placeholders```

# [26.10.3] - 2026-10-03

## Added

- updates:
```
- one-click mod updates from Nexus
- free accounts open the Nexus Files page for Slow Download
- open exact file for free accounts, refresh banner, optimistic hits removal
```

## Fixed

- **downloads**: ```avoid spurious download-meta for pre-resolved updates; keep progress live```

- scripts:
```
- concurrent polling with live animation in release-status
- restore sync fetchers for snapshot mode
```

# [26.10.2] - 2026-10-03

## Added

- **scripts**: ```live release-status progress UI for the Release workflow```

## Other

- Revert "chore: release v26.10.2"

- release:
```
- build Linux only until Windows/macOS are supported
- discover bundle dirs dynamically instead of hardcoding platforms
- handle single-artifact download layout
```

# [26.10.1] - 2026-10-03

## Fixed

- **windows**: ```load route paths in release so SvelteKit resolves them```

## Other

- readme:
```
- simple app overview and run instructions
- releases page as the official install path
```

# [26.10.0] - 2026-10-03

## Added

- **template**: ```full SvelteKit app scaffold with theming and settings```
- **app**: ```scaffold W3LMN from tauri-template with Rust core and Svelte UI```
- **native**: ```tauri plugins for dialog, deep-link nxm, single-instance, notifications, shell; xml cluster merge; update checks; legacy import```
- **windows**: ```install/setup/resolver as native sub-windows; setup dialog rebuild; merge pipeline```
- **priority**: ```drag-drop reorder rows, priority follows list order```

- ui:
```
- full shadcn-svelte set with bits-ui v2 selector patches
- daisyUI v5 foundation with bits-ui select, drop shadcn registry
- rebuild main window to match original — stone/bronze theme, 5-column table, chips, banners, downloads slide-over
- match original look — mono UI, enabled-count subtitle, context menu, hover tips, edit/install dialogs
- enable column resizing in DataList component
- file/folder tree for install and edit windows
- lucide icons everywhere; non-blocking update check
```

- port:
```
- bundles decoders, snippets, nxm downloads, sections, clashes, settings writers
- fingerprints/made-for, quota, download queue, merger check, multi-hunk diff, analysis, native queue UI
- doboz decoder, deploy restore-on-disable, annotation clashes, made-for UI, queue UI
- rar via vendored unrar, real bundle layout+zlib+repack, xml identity-tree merge, clusters engine
```

## Fixed

- **dev**: ```exclude svelte-source deps from esbuild optimizer```
- **deep-link**: ```enable single-instance deep-link feature, use onOpenUrl JS listener```
- **unmanaged**: ```folder-level ownership so deployed mods aren't flagged```
- **deploy**: ```never back up our own output; disable removes cleanly```
- **ui**: ```restore mod and section context menus via DataList```

- ci:
```
- underscore spaces in canonical bundle filenames
- per-platform bundle targets (nsis+msi, deb+appimage+rpm, dmg)
```

- windows:
```
- map tool kinds to real route files (setup→settings, resolver→merges)
- use external devUrl for tool windows; map routes correctly
- accept 'settings' kind, add debug logging for tool windows
- allow store plugin on all tool windows
```

- downloads:
```
- match original panel cards, flow, and dirs
- full original flow — meta, resume, history, install defaults
- worker-thread downloads with timeouts, no UI blocking
- break queue-mutex self-deadlock on repeat downloads
- gate auto-install on file identity, not version verdict
```

- locks:
```
- release global state lock during slow file I/O
- drop state guard before save; lock-free scans in made_for/merge_inputs/annotation_clashes
```

- shell:
```
- allow opening local paths; log open failures
- use open-path regex allowlist instead of ineffective flag
```

## Changed

- downloads:
```
- nxm queue with metadata, resume, speed, grouped UI
- throttle progress events, single-extract install preview
```

- ui:
```
- make downloads sidebar look more like original Qt panel
- always show priority for enabled mods
- copy DataList component from NMM into W3LMN
- remove drag-to-reorder tooltip
- soften forced column layout on DataList
- keep header borders only, widen resize handles
- attach file tree to its toggle with no gap
```

- priority:
```
- remove pill highlight and native spinner arrows
- hide browser number spinner arrows
- move spinner CSS to global stylesheet
```

## Other

- initial tauri-template scaffold
- **version**: ```bump to 26.9.0 and sync manifests```
- **lint**: ```remove dead code and fix clippy warnings```
- **downloads**: ```poison-safe queue lock, entry/step logging to isolate freeze```
- **rename**: ```W3LMN to W3MM with game-data migration```

- logging:
```
- write tool-window debug logs to xdg state log file
- startup marker so running build is identifiable in log
- lock-wait tracing, deploy/install phases, refresh/deploy timing
```
