//! New-app home layout (clean break from `_ModManager`).
//!
//! Python original (`w3modmanager.py`): `CONFIG_NAME`, `MANAGER_DIRNAME`,
//! `_runtime_dir`, `load_config/save_config`, `home_for`.
//! Here the home is `<game>/_W3MM/` so users can switch back and forth
//! without the two apps touching each other's staging/backups/state.

use std::path::{Path, PathBuf};

pub const MANAGER_DIRNAME: &str = "_W3MM";
pub const STATE_FILE: &str = "state.json";
pub const STAGING_DIR: &str = "staging";
pub const BACKUP_DIR: &str = "backup";
pub const TMP_DIR: &str = "tmp";

#[derive(Debug, Clone)]
pub struct Home {
    pub game: PathBuf,
    pub prefix: PathBuf,
    pub staging: PathBuf,
    pub backup: PathBuf,
    pub tmp: PathBuf,
    pub state_file: PathBuf,
}

impl Home {
    pub fn new(game_dir: &str, prefix: &str) -> Self {
        let game = PathBuf::from(game_dir);
        let prefix = normalize_prefix(prefix);
        let root = game.join(MANAGER_DIRNAME);
        Self {
            staging: root.join(STAGING_DIR),
            backup: root.join(BACKUP_DIR),
            tmp: root.join(TMP_DIR),
            state_file: root.join(STATE_FILE),
            game,
            prefix,
        }
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
