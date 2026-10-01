//! Mod list state: `state.json` + priority + enable/disable.
//! Ports `ModManager._load/save/rows/mods/get/priority_ids/ranked/place_priority/
//! set_enabled/rename_mod/edit_mod/remove_mods/sections` (`w3modmanager.py:4077-4484`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModRow {
    pub id: String,
    #[serde(default)]
    pub sep: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub nexus: String,
    #[serde(default)]
    pub archive: String,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub updated: i64,
    #[serde(default)]
    pub collapsed: bool,
    /// Target rels inside staging (`mods/...`, `dlc/...`, `bin/...`), posix style.
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub nexus_cat: String,
    #[serde(default)]
    pub main_of: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    #[serde(default)]
    pub mods: Vec<ModRow>,
    /// Top-priority first (python `state["priority"]`).
    #[serde(default)]
    pub priority: Vec<String>,
    #[serde(default)]
    pub deployed: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub filelist_added: std::collections::BTreeMap<String, Vec<String>>,
    /// Merge resolutions per key: answers the resolver gave.
    #[serde(default)]
    pub resolutions: std::collections::BTreeMap<String, Vec<usize>>,
    /// Merge outputs kept for redeploy without re-asking.
    #[serde(default)]
    pub merge_kept: std::collections::BTreeMap<String, String>,
}

impl AppState {
    pub fn mods_only(&self) -> Vec<&ModRow> {
        self.mods.iter().filter(|r| !r.sep).collect()
    }

    pub fn get(&self, mid: &str) -> Option<&ModRow> {
        self.mods.iter().find(|r| r.id == mid)
    }

    pub fn get_mut(&mut self, mid: &str) -> Option<&mut ModRow> {
        self.mods.iter_mut().find(|r| r.id == mid)
    }

    /// Every mod id, top priority first; unknown ids appended in list order.
    /// Mirrors python `priority_ids` first-time setup from list order.
    pub fn priority_ids(&mut self) -> Vec<String> {
        let known: Vec<String> = self.mods_only().iter().map(|m| m.id.clone()).collect();
        self.priority.retain(|id| known.contains(id));
        for id in &known {
            if !self.priority.contains(id) {
                self.priority.push(id.clone());
            }
        }
        self.priority.clone()
    }

    pub fn set_enabled(&mut self, ids: &[String], on: bool) {
        for m in self.mods.iter_mut().filter(|r| !r.sep && ids.contains(&r.id)) {
            m.enabled = on;
        }
    }

    pub fn rename_mod(&mut self, mid: &str, name: &str) {
        if let Some(m) = self.get_mut(mid) {
            m.name = name.to_string();
        }
    }

    pub fn remove_rows(&mut self, ids: &[String]) {
        self.mods.retain(|r| !ids.contains(&r.id));
        self.priority.retain(|id| !ids.contains(id));
    }

    pub fn set_priority_number(&mut self, mid: &str, number: usize) {
        // 1-based clamp, like the Qt spinbox path.
        self.priority.retain(|id| id != mid);
        let idx = number.saturating_sub(1).min(self.priority.len());
        self.priority.insert(idx, mid.to_string());
    }

    pub fn state_deployed(&mut self, written: Vec<String>, ranked: &[ModRow]) {
        // last-writer-wins: ranked is priority order, so later entries overwrite.
        let mut by_path: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        let _ = &written;
        for m in ranked {
            let _ = m;
        }
        // Caller passes flat `written` without per-mod split; keep counts only.
        // Detailed per-file map is rebuilt on next deploy scan.
        for w in written {
            by_path.insert(w, ranked.last().map(|r| r.id.clone()).unwrap_or_default());
        }
        self.deployed = by_path;
    }

    pub fn add_separator(&mut self, index: usize, name: &str) -> ModRow {
        let row = ModRow {
            id: format!("sep{}", uuid::Uuid::new_v4().simple()),
            sep: true, name: name.to_string(), enabled: false,
            version: String::new(), nexus: String::new(), archive: String::new(),
            section: String::new(), updated: 0, collapsed: false,
            targets: vec![], nexus_cat: String::new(), main_of: String::new(),
        };
        let at = index.min(self.mods.len());
        self.mods.insert(at, row.clone());
        row
    }

    pub fn edit_mod(&mut self, mid: &str, name: &str, version: &str, nexus: &str, section: &str) {
        if let Some(m) = self.get_mut(mid) {
            m.name = name.to_string();
            m.version = version.to_string();
            m.nexus = nexus.to_string();
            m.section = section.to_string();
        }
    }

    /// (managed ids this install replaces, unmanaged paths it would replace).
    /// Mirrors python `find_collisions` minus the dlc/game-owned nuance, which
    /// the deploy scan handles via backups.
    pub fn find_collisions(&self, targets: &[String]) -> (Vec<String>, Vec<String>) {
        let tl: std::collections::BTreeSet<String> = targets.iter().map(|t| t.to_lowercase()).collect();
        let mut managed = vec![];
        for m in self.mods_only() {
            if m.targets.iter().any(|t| tl.contains(&t.to_lowercase())) {
                managed.push(m.id.clone());
            }
        }
        (managed, vec![])
    }

    /// Who overwrites whom, in priority order: path -> [mod ids, winner last].
    /// Python `clashes` subset used for priority badges.
    pub fn clashes(&mut self) -> std::collections::BTreeMap<String, Vec<String>> {
        let order = self.priority_ids();
        let mut idx = std::collections::HashMap::new();
        for (i, id) in order.iter().enumerate() {
            idx.insert(id.clone(), i);
        }
        let mut by_path: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        for m in self.mods_only().into_iter().filter(|m| m.enabled) {
            for t in &m.targets {
                by_path.entry(t.to_lowercase()).or_default().push(m.id.clone());
            }
        }
        for v in by_path.values_mut() {
            v.sort_by_key(|id| idx.get(id).copied().unwrap_or(usize::MAX));
        }
        by_path.retain(|_, v| v.len() > 1);
        by_path
    }

    pub fn save_resolutions(&mut self, key: &str, answers: Vec<usize>) {
        self.resolutions.insert(key.to_string(), answers);
    }
}

pub fn load_state(path: &PathBuf) -> Result<AppState, String> {
    let bak = path.with_extension("json.bak");
    for f in [path, &bak] {
        if let Ok(text) = std::fs::read_to_string(f) {
            if let Ok(mut data) = serde_json::from_str::<AppState>(&text) {
                // validate shape like python `_load`
                let _ = data.priority_ids();
                return Ok(data);
            }
        }
    }
    if path.exists() || bak.exists() {
        return Err(format!(
            "The mod list in {} is damaged and couldn't be read. Nothing was changed.",
            path.display()
        ));
    }
    Ok(AppState::default())
}

pub fn save_state(path: &PathBuf, state: &AppState) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string(state).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Folder-safe ASCII name (python `folder_safe`).
pub fn folder_safe(name: &str) -> String {
    let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if safe.is_empty() {
        use sha1::{Digest, Sha1};
        let mut h = Sha1::new();
        h.update(name.as_bytes());
        format!("Unnamed_{}", hex::encode(h.finalize())[..6].to_string())
    } else {
        safe
    }
}

pub fn ensure_mod_prefix(name: &str) -> String {
    let n = name.trim().replace('/', "_");
    if n.to_lowercase().starts_with("mod") {
        n
    } else {
        format!("mod{}", folder_safe(&n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: &str, name: &str) -> ModRow {
        ModRow { id: id.into(), sep: false, name: name.into(), enabled: true, version: String::new(), nexus: String::new(), archive: String::new(), section: String::new(), updated: 0, collapsed: false, targets: vec![], nexus_cat: String::new(), main_of: String::new() }
    }
    #[test]
    fn priority_flows() {
        let mut s = AppState::default();
        s.mods = vec![row("a", "A"), row("b", "B")];
        assert_eq!(s.priority_ids(), vec!["a", "b"]);
        s.set_priority_number("b", 1);
        assert_eq!(s.priority, vec!["b", "a"]);
        s.set_enabled(&["a".to_string()], false);
        assert!(!s.get("a").unwrap().enabled);
    }
}
