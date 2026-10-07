//! Shared Tauri state: game/prefix + loaded `AppState`.

use crate::{home::{Home, MANAGER_DIRNAME}, state::{load_state, save_state, AppState}};
use std::sync::Mutex;

pub struct Manager {
    pub home: Home,
    pub state: Mutex<AppState>,
}

impl Manager {
    pub fn open(game_dir: &str, prefix: &str, staging_override: &str) -> Result<Self, String> {
        // One-time move from the old data dir name; keeps state/downloads/backups.
        let game = std::path::PathBuf::from(game_dir);
        let legacy = game.join("_W3LMN");
        let current = game.join(MANAGER_DIRNAME);
        if !current.exists() && legacy.exists() {
            std::fs::rename(&legacy, &current)
                .map_err(|e| format!("could not migrate _W3LMN data: {e}"))?;
            eprintln!("[w3mm] migrated game data _W3LMN -> {}", MANAGER_DIRNAME);
        }
        let home = Home::new(game_dir, prefix, staging_override);
        // Relocate `<game>/_W3MM/` into the platform data dir on first run.
        // Merge-missing covers an interrupted previous attempt; once the
        // state file is over, the in-game dir is superseded and removed.
        if current.exists() {
            migrate_dir_contents(&current, &home.data)?;
            if home.state_file.is_file() {
                std::fs::remove_dir_all(&current)
                    .map_err(|e| format!("could not remove migrated {}: {e}", MANAGER_DIRNAME))?;
                crate::log_line("rust", "migrated in-game _W3MM data to platform data dir");
            }
        }
        home.ensure_dirs().map_err(|e| e.to_string())?;
        let mut state = load_state(&home.state_file)?;
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
                crate::state::save_state(&home.state_file, &state)?;
            }
        }
        Ok(Self { home, state: Mutex::new(state) })
    }

    pub fn save(&self) -> Result<(), String> {
        let s = self.state.lock().map_err(|e| e.to_string())?;
        save_state(&self.home.state_file, &s)
    }

    /// Proton `.../pfx/drive_c/users/steamuser/Documents/The Witcher 3/gamesaves/..`
    /// parent — where `mods.settings` / `input.settings` live. Native
    /// `%USERPROFILE%\Documents\The Witcher 3` on Windows (no prefix).
    pub fn settings_dir(&self) -> std::path::PathBuf {
        settings_dir_for(&self.home.prefix)
    }
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
        let p = std::env::temp_dir().join(format!("w3mm-migrate-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn migrate_moves_contents_over() {
        let base = tmpdir("move");
        let src = base.join("_W3MM");
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
        let src = base.join("_W3MM");
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
