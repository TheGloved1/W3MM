//! Shared Tauri state: game/prefix + loaded `AppState`.

use crate::{home::{Home, MANAGER_DIRNAME}, state::{load_store, save_store, AppState}};
use std::sync::Mutex;

pub struct Manager {
    pub home: Home,
    pub state: Mutex<AppState>,
}

impl Manager {
    pub fn open(game_dir: &str, prefix: &str, staging_override: &str) -> Result<Self, String> {
        let game = std::path::PathBuf::from(game_dir);
        // One-time brand rename (W3MM -> YAWMM) for the in-game dir, then
        // adopt the old per-install roots into the single store. Both keep
        // state/downloads/backups; nothing is overwritten.
        migrate_game_dir_brand(&game)?;
        migrate_slug_roots(game_dir)?;
        let current = game.join(MANAGER_DIRNAME);
        let home = Home::new(game_dir, prefix, staging_override);
        // Relocate `<game>/_YAWMM/` into the platform data dir on first run.
        // Merge-missing covers an interrupted previous attempt; once the
        // root state file is over, the in-game dir is superseded and removed.
        if current.exists() {
            migrate_dir_contents(&current, &home.data)?;
            if home.data.join(crate::home::STATE_FILE).is_file() {
                std::fs::remove_dir_all(&current)
                    .map_err(|e| format!("could not remove migrated {}: {e}", MANAGER_DIRNAME))?;
                crate::log_line("rust", "migrated in-game _YAWMM data to platform data dir");
            }
        }
        home.ensure_dirs().map_err(|e| e.to_string())?;
        let mut state = load_store(&home.data)?;
        // Every install starts with a Default profile (adopting the current
        // working set, possibly empty on a fresh install).
        if state.ensure_default_profile(&home.data)? {
            crate::state::save_store(&home.data, &state)?;
        }
        // Staging override changed (set or cleared in Settings): move staged
        // mods between the effective dirs so nothing is orphaned, then
        // persist the new override with the state it belongs to.
        {
            let prev = effective_staging(&home, &state.staging_override);
            let next = effective_staging(&home, staging_override);
            if prev != next {
                if prev.exists() {
                    migrate_dir_contents(&prev, &next)?;
                    crate::log_line(
                        "rust",
                        &format!("moved staging {} -> {}", prev.display(), next.display()),
                    );
                }
                state.staging_override = staging_override.trim().to_string();
                crate::state::save_store(&home.data, &state)?;
            }
        }
        Ok(Self { home, state: Mutex::new(state) })
    }

    pub fn save(&self) -> Result<(), String> {
        let s = self.state.lock().map_err(|e| e.to_string())?;
        save_store(&self.home.data, &s)
    }

    /// Proton `.../pfx/drive_c/users/steamuser/Documents/The Witcher 3/gamesaves/..`
    /// parent — where `mods.settings` / `input.settings` live. Native
    /// `%USERPROFILE%\Documents\The Witcher 3` on Windows (no prefix).
    pub fn settings_dir(&self) -> std::path::PathBuf {
        settings_dir_for(&self.home.prefix)
    }
}

/// One-time brand rename (W3LMN/W3MM -> YAWMM) for the in-game data dir.
fn migrate_game_dir_brand(game: &std::path::Path) -> Result<(), String> {
    let current = game.join(MANAGER_DIRNAME);
    for legacy in [game.join("_W3LMN"), game.join("_W3MM")] {
        if !current.exists() && legacy.exists() {
            std::fs::rename(&legacy, &current)
                .map_err(|e| format!("could not migrate {} data: {e}", legacy.display()))?;
            eprintln!("[yawmm] migrated game data {} -> {}", legacy.display(), MANAGER_DIRNAME);
        }
    }
    Ok(())
}

/// Dir name looks like a per-install slug (`<12 hex>-<tail>`); used to spot
/// orphaned roots left behind by the single-store migration.
fn looks_like_slug(name: &std::ffi::OsStr) -> bool {
    let s = name.to_string_lossy();
    let mut parts = s.splitn(2, '-');
    matches!(parts.next(), Some(hex) if hex.len() == 12 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        && parts.next().is_some_and(|t| !t.is_empty())
}

/// First-run adoption into the single store: move the current game's old
/// per-install roots (`yawmm/<slug>/`, then legacy `w3mm/<slug>/`) up into
/// the base dir — earlier arrivals never clobber, so yawmm content wins
/// ties. A half-moved source is left in place and retried next launch. Other slug-like siblings belong to other installs: left alone,
/// logged as orphaned (migrate-current-only).
fn migrate_slug_roots(game_dir: &str) -> Result<(), String> {
    let base = crate::home::data_root_for(game_dir);
    let slug = crate::home::legacy_data_root_for(game_dir)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if slug.is_empty() {
        return Ok(());
    }
    let w3mm_base = crate::home::data_base_dir().join("w3mm");
    for cand in [base.join(&slug), w3mm_base.join(&slug)] {
        if !cand.exists() {
            continue;
        }
        // Never merge the store into itself (paranoia: slug is never empty,
        // but a self-move would drain the live store).
        if cand == base {
            continue;
        }
        migrate_dir_contents(&cand, &base)?;
        if std::fs::read_dir(&cand).map(|mut r| r.next().is_none()).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(&cand);
        }
    }
    if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && p.file_name().map(looks_like_slug).unwrap_or(false) {
                crate::log_line("rust", &format!("orphaned per-install data left in place: {}", p.display()));
            }
        }
    }
    Ok(())
}

/// Effective staging dir for an override value: the override itself, or the
/// default under the data root when empty.
fn effective_staging(home: &Home, staging_override: &str) -> std::path::PathBuf {
    if staging_override.trim().is_empty() {
        home.default_staging()
    } else {
        std::path::PathBuf::from(staging_override.trim())
    }
}

/// Copy one in-game data dir's contents into the platform data dir, skipping
/// files that already arrived (resumes an interrupted migration). Fast path
/// is same-filesystem entry renames; cross-device falls back to copy, and the
/// caller only drops the source once the state file made it over.
fn migrate_dir_contents(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    // Same-filesystem probe: rename a probe file across the boundary. A
    // whole-dir rename can't merge with a resumed partial dst, so move
    // entries one by one instead — still instant, and pre-existing dst
    // files are left alone rather than clobbered.
    let dst_probe = dst.join(".migrate-probe");
    let same_fs = std::fs::write(&dst_probe, b"1")
        .and_then(|_| std::fs::rename(&dst_probe, src.join(".migrate-probe-back")))
        .and_then(|_| std::fs::remove_file(src.join(".migrate-probe-back")))
        .is_ok();
    let _ = std::fs::remove_file(&dst_probe);
    if same_fs {
        let entries: Vec<std::path::PathBuf> = std::fs::read_dir(src)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .collect();
        for entry in entries {
            let name = entry.file_name().ok_or("bad entry migrating app data")?;
            let target = dst.join(name);
            if target.exists() {
                let _ = std::fs::remove_dir_all(&entry);
                let _ = std::fs::remove_file(&entry);
                continue;
            }
            std::fs::rename(&entry, &target).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    // Cross-device: recursive copy of missing files.
    fn copy_missing(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<(usize, u64)> {
        let mut files = 0;
        let mut bytes = 0u64;
        for e in walkdir::WalkDir::new(src).into_iter().flatten() {
            let rel = e.path().strip_prefix(src).unwrap_or(e.path());
            if rel.as_os_str().is_empty() {
                continue;
            }
            let target = dst.join(rel);
            if e.file_type().is_dir() {
                std::fs::create_dir_all(&target)?;
            } else if e.file_type().is_file() && !target.exists() {
                if let Some(p) = target.parent() {
                    std::fs::create_dir_all(p)?;
                }
                bytes += std::fs::copy(e.path(), &target)?;
                files += 1;
            }
        }
        Ok((files, bytes))
    }
    let (files, bytes) = copy_missing(src, dst).map_err(|e| e.to_string())?;
    crate::log_line("rust", &format!("migrated {files} files ({bytes} bytes) across filesystems"));
    Ok(())
}

/// Where the game keeps `mods.settings` / `input.settings` for a prefix.
/// Centralized so deploy and settings writers agree; platform split lives
/// here instead of scattered `drive_c` joins.
pub fn settings_dir_for(prefix: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        let _ = prefix;
        let home = std::env::var_os("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        home.join("Documents/The Witcher 3")
    }
    #[cfg(not(target_os = "windows"))]
    {
        prefix.join("drive_c/users/steamuser/Documents/The Witcher 3")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("yawmm-migrate-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn game_dir_brand_renames() {
        // <game>/_W3MM (and the older _W3LMN) become _YAWMM; an existing
        // _YAWMM is never clobbered.
        let base = tmpdir("brand-game");
        let game = base.join("game");
        std::fs::create_dir_all(game.join("_W3MM/staging")).unwrap();
        std::fs::write(game.join("_W3MM/state.json"), b"{}").unwrap();
        migrate_game_dir_brand(&game).unwrap();
        assert!(game.join("_YAWMM/state.json").is_file());
        assert!(!game.join("_W3MM").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn slug_roots_merge_into_base_yawmm_wins() {
        // migrate_slug_roots reads XDG_DATA_HOME, which is process-global:
        // keep every env-touching scenario in this one test so parallel
        // test threads can't interleave different values.
        let xdg = tmpdir("xdg");
        let game = tmpdir("game-dir");
        std::env::set_var("XDG_DATA_HOME", &xdg);
        let slug = crate::home::legacy_data_root_for(game.to_str().unwrap())
            .file_name().unwrap().to_string_lossy().to_string();
        // Both legacy roots exist: content merges up, yawmm wins ties
        // (moves never clobber), drained shelves are dropped.
        let w3 = xdg.join("w3mm").join(&slug);
        let yw = xdg.join("yawmm").join(&slug);
        std::fs::create_dir_all(w3.join("downloads")).unwrap();
        std::fs::write(w3.join("state.json"), b"old").unwrap();
        std::fs::create_dir_all(&yw).unwrap();
        std::fs::write(yw.join("state.json"), b"new").unwrap();
        std::fs::write(yw.join("extra.txt"), b"y").unwrap();
        migrate_slug_roots(game.to_str().unwrap()).unwrap();
        let base = xdg.join("yawmm");
        assert_eq!(std::fs::read(base.join("state.json")).unwrap(), b"new");
        assert!(base.join("downloads").is_dir());
        assert!(base.join("extra.txt").is_file());
        assert!(!w3.exists() && !yw.exists(), "drained shelves must be removed");
        // A slug that is NOT the current game's is logged, never touched.
        let other = base.join("deadbeef1234-OtherGame");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("state.json"), b"{}").unwrap();
        migrate_slug_roots(game.to_str().unwrap()).unwrap();
        assert!(other.join("state.json").is_file(), "orphan must survive");
        assert!(base.join("state.json").is_file(), "store must survive re-run");
        std::env::remove_var("XDG_DATA_HOME");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn migrate_moves_contents_over() {
        let base = tmpdir("move");
        let src = base.join("_YAWMM");
        let dst = base.join("data").join("slug");
        std::fs::create_dir_all(src.join("staging/abc")).unwrap();
        std::fs::write(src.join("state.json"), b"{}").unwrap();
        std::fs::write(src.join("staging/abc/f.ws"), b"x").unwrap();
        migrate_dir_contents(&src, &dst).unwrap();
        assert!(dst.join("state.json").is_file());
        assert!(dst.join("staging/abc/f.ws").is_file());
        assert!(std::fs::read_dir(&src).unwrap().next().is_none(), "source must be drained");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn migrate_resumes_without_clobbering() {
        // Interrupted run: dst already holds state.json; src still has the
        // rest. Missing files arrive, present ones are never overwritten,
        // and stale src entries are dropped.
        let base = tmpdir("resume");
        let src = base.join("_YAWMM");
        let dst = base.join("data").join("slug");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("state.json"), b"new").unwrap();
        std::fs::create_dir_all(src.join("staging")).unwrap();
        std::fs::write(src.join("state.json"), b"old").unwrap();
        std::fs::write(src.join("staging/f.ws"), b"x").unwrap();
        migrate_dir_contents(&src, &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("state.json")).unwrap(), b"new");
        assert!(dst.join("staging/f.ws").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod effective_tests {
    use super::*;

    #[test]
    fn override_wins_blank_falls_back() {
        let home = Home::new("/games/w3", "", "");
        assert_eq!(effective_staging(&home, ""), home.default_staging());
        assert_eq!(effective_staging(&home, "   "), home.default_staging());
        assert_eq!(
            effective_staging(&home, "/mnt/fast/staging"),
            std::path::PathBuf::from("/mnt/fast/staging")
        );
    }
}
