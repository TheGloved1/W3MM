//! Shared Tauri state: game/prefix + loaded `AppState`.

use crate::{home::{Home, MANAGER_DIRNAME}, state::{load_state, save_state, AppState}};
use std::sync::Mutex;

pub struct Manager {
    pub home: Home,
    pub state: Mutex<AppState>,
}

impl Manager {
    pub fn open(game_dir: &str, prefix: &str) -> Result<Self, String> {
        // One-time move from the old data dir name; keeps state/downloads/backups.
        let game = std::path::PathBuf::from(game_dir);
        let legacy = game.join("_W3LMN");
        let current = game.join(MANAGER_DIRNAME);
        if !current.exists() && legacy.exists() {
            std::fs::rename(&legacy, &current)
                .map_err(|e| format!("could not migrate _W3LMN data: {e}"))?;
            eprintln!("[w3mm] migrated game data _W3LMN -> {}", MANAGER_DIRNAME);
        }
        let home = Home::new(game_dir, prefix);
        home.ensure_dirs().map_err(|e| e.to_string())?;
        let state = load_state(&home.state_file)?;
        Ok(Self { home, state: Mutex::new(state) })
    }

    pub fn save(&self) -> Result<(), String> {
        let s = self.state.lock().map_err(|e| e.to_string())?;
        save_state(&self.home.state_file, &s)
    }

    /// Proton `.../pfx/drive_c/users/steamuser/Documents/The Witcher 3/gamesaves/..`
    /// parent — where `mods.settings` / `input.settings` live.
    pub fn settings_dir(&self) -> std::path::PathBuf {
        self.home.prefix.join("drive_c/users/steamuser/Documents/The Witcher 3")
    }
}
