//! App home layout: game dir + platform data dir.
//!
//! The manager used to keep everything in `<game>/_W3MM/`; app data now lives
//! in the platform data dir (`$XDG_DATA_HOME/w3mm/<slug>/`,
//! `%LOCALAPPDATA%/w3mm/<slug>/`), keyed per game install so multiple copies
//! of the game stay isolated. A first-run migration moves an existing
//! `<game>/_W3MM/` over (see `manager.rs`).

use std::path::{Path, PathBuf};

/// Legacy in-game data dir, kept as the migration source.
pub const MANAGER_DIRNAME: &str = "_W3MM";
pub const STATE_FILE: &str = "state.json";
pub const STAGING_DIR: &str = "staging";
pub const BACKUP_DIR: &str = "backup";
pub const TMP_DIR: &str = "tmp";
pub const DOWNLOADS_DIR: &str = "downloads";

#[derive(Debug, Clone)]
pub struct Home {
    pub game: PathBuf,
    pub prefix: PathBuf,
    /// Platform data root for this game install (`.../w3mm/<slug>/`).
    pub data: PathBuf,
    pub staging: PathBuf,
    pub backup: PathBuf,
    pub tmp: PathBuf,
    pub downloads: PathBuf,
    pub state_file: PathBuf,
}

/// Stable, filesystem-safe identity for one game install: hash + readable
/// tail. Windows canonical paths are case-insensitive (`C:\…` vs `c:\…`)
/// and carry `\\?\` prefixes, so normalize those before hashing or one game
/// yields two data dirs depending on how the folder was picked.
fn slug_for(canonical: &str, tail: &str) -> String {
    #[cfg(target_os = "windows")]
    let canonical = {
        let mut c = canonical.to_string();
        if let Some(stripped) = c.strip_prefix(r"\\?\") {
            c = stripped.to_string();
        }
        c.to_lowercase()
    };
    #[cfg(not(target_os = "windows"))]
    let canonical = canonical.to_string();
    let hex = {
        use sha1::Digest;
        let mut h = sha1::Sha1::new();
        h.update(canonical.as_bytes());
        hex::encode(h.finalize())
    };
    let safe: String = tail
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .take(32)
        .collect();
    let safe = if safe.is_empty() { "game".to_string() } else { safe };
    format!("{}-{safe}", &hex[..12])
}

/// Data root for a game install, creating nothing.
pub fn data_root_for(game_dir: &str) -> PathBuf {
    let game = PathBuf::from(game_dir);
    // Canonicalize for a stable slug; fall back to the raw path (e.g. the
    // folder was picked but not yet validated to exist).
    let (canonical, tail) = match game.canonicalize() {
        Ok(p) => (
            p.to_string_lossy().to_string(),
            p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        ),
        Err(_) => (
            game.to_string_lossy().to_string(),
            game.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        ),
    };
    data_base_dir().join("w3mm").join(slug_for(&canonical, &tail))
}

/// Platform data base dir (`$XDG_DATA_HOME` → `~/.local/share`,
/// `%LOCALAPPDATA%` → `%USERPROFILE%\AppData\Local`).
fn data_base_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            if !local.is_empty() {
                return PathBuf::from(local);
            }
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            if !home.is_empty() {
                return PathBuf::from(home).join("AppData").join("Local");
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".local").join("share");
        }
    }
    PathBuf::from(".")
}

impl Home {
    pub fn new(game_dir: &str, prefix: &str, staging_override: &str) -> Self {
        let game = PathBuf::from(game_dir);
        let prefix = normalize_prefix(prefix);
        let root = data_root_for(game_dir);
        let staging = if staging_override.trim().is_empty() {
            root.join(STAGING_DIR)
        } else {
            PathBuf::from(staging_override.trim())
        };
        Self {
            staging,
            backup: root.join(BACKUP_DIR),
            tmp: root.join(TMP_DIR),
            downloads: root.join(DOWNLOADS_DIR),
            state_file: root.join(STATE_FILE),
            data: root,
            game,
            prefix,
        }
    }

    /// Default staging dir for this data root (ignores any override).
    pub fn default_staging(&self) -> PathBuf {
        self.data.join(STAGING_DIR)
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in [&self.staging, &self.backup, &self.tmp] {
            std::fs::create_dir_all(d)?;
        }
        // Clear crash leftovers in tmp (python: `kids(self.tmp)` rmtree).
        if let Ok(rd) = std::fs::read_dir(&self.tmp) {
            for e in rd.flatten() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
        Ok(())
    }
}

pub fn normalize_prefix(prefix: &str) -> PathBuf {
    let p = prefix.trim();
    // python `normalize_prefix`: "/.../pfx/drive_c" -> "/.../pfx"
    let path = Path::new(p);
    if let Some(s) = path.to_str() {
        let low = s.to_lowercase();
        if let Some(idx) = low.find("/drive_c") {
            return PathBuf::from(&s[..idx]);
        }
    }
    PathBuf::from(p)
}

/// Whether two dirs live on the same filesystem (hardlinks possible between
/// them). Read-only metadata comparison — no probe files. `None` when either
/// side can't be stated or the platform exposes no volume identity.
pub fn same_filesystem(a: &Path, b: &Path) -> Option<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let (ma, mb) = (std::fs::metadata(a).ok()?, std::fs::metadata(b).ok()?);
        Some(ma.dev() == mb.dev())
    }
    #[cfg(target_os = "windows")]
    {
        // No stable volume-serial API exists (`volume_serial_number` is
        // still behind `windows_by_handle`, rust#63010), so compare volume
        // roots instead: same drive letter (or UNC share) means hardlinks
        // work. Edge cases err safe — subst drives and volume mount points
        // may report "split" (an extra dialog) or "same" (silent copy
        // fallback, the pre-existing behavior either way).
        use std::path::{Component, Prefix};
        fn vol_root(p: &Path) -> Option<String> {
            let mut comps = p.canonicalize().ok()?.components();
            let root = match comps.next()? {
                Component::Prefix(pre) => match pre.kind() {
                    Prefix::Disk(d) | Prefix::VerbatimDisk(d) => {
                        format!("disk:{}", char::from(d).to_ascii_lowercase())
                    }
                    Prefix::UNC(srv, shr) | Prefix::VerbatimUNC(srv, shr) => format!(
                        "unc:{}\\{}",
                        srv.to_string_lossy().to_lowercase(),
                        shr.to_string_lossy().to_lowercase()
                    ),
                    _ => return None,
                },
                _ => return None,
            };
            Some(root)
        }
        match (vol_root(a), vol_root(b)) {
            (Some(x), Some(y)) => Some(x == y),
            _ => None,
        }
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = (a, b);
        None
    }
}
/// Mirrors python `is_game_dir`: content dir + launcher or exe present.
pub fn is_game_dir(path: &str) -> bool {
    let base = Path::new(path);
    if !base.is_dir() {
        return false;
    }
    let has_content = base.join("content").is_dir();
    let has_bin = base.join("bin").is_dir();
    has_content && has_bin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_stable_and_unique() {
        let a = slug_for("/mnt/games/SteamLibrary/steamapps/common/The Witcher 3", "The Witcher 3");
        assert_eq!(a, slug_for("/mnt/games/SteamLibrary/steamapps/common/The Witcher 3", "The Witcher 3"));
        let b = slug_for("/other/steamapps/common/The Witcher 3", "The Witcher 3");
        assert_ne!(a, b, "distinct installs must not share a data dir");
        assert!(a.len() > 13 && !a.contains('/'), "slug must be a single dir name: {a}");
    }

    #[test]
    fn same_filesystem_tmp() {
        // Two fresh tmp dirs share a filesystem; a bogus path yields None.
        let a = std::env::temp_dir().join(format!("w3mm-fs-a-{}", std::process::id()));
        let b = std::env::temp_dir().join(format!("w3mm-fs-b-{}", std::process::id()));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        assert_eq!(same_filesystem(&a, &b), Some(true));
        assert_eq!(same_filesystem(&a, &std::path::PathBuf::from("/definitely/not/here-12345")), None);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn slug_windows_case_folding() {
        // Drive-letter case and \\?\ prefixes must not fork the data dir.
        // These assertions hold on every platform: the folding itself is
        // cfg-gated, so spell out both expectations.
        let lower = slug_for("c:\\games\\witcher 3", "game");
        let upper = slug_for("C:\\Games\\Witcher 3", "game");
        #[cfg(target_os = "windows")]
        {
            assert_eq!(lower, upper);
            assert_eq!(lower, slug_for("\\\\?\\C:\\Games\\Witcher 3", "game"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert_ne!(lower, upper, "case-sensitive filesystems keep exact paths");
        }
    }
}
