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

/// A saved selection of activated mods. Snapshots which mods are enabled;
// order and sections live on (they belong to the mod list, not the profile).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Enabled mod ids at save time.
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub updated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    #[serde(default)]
    pub mods: Vec<ModRow>,
    /// Saved enable-selections.
    #[serde(default)]
    pub profiles: Vec<Profile>,    /// Top-priority first (python `state["priority"]`).
    #[serde(default)]
    pub priority: Vec<String>,
    /// User staging-dir override (empty = default under the data root).
    /// Applied to `Home` on open; moving contents happens there too.
    #[serde(default)]
    pub staging_override: String,
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

    /// Priority always mirrors list order (mods only, separators skipped):
    /// every writer that moves rows rebuilds it, so readers never see a
    /// stale order. Any previously desynced state heals on next call.
    /// Mirrors python `priority_ids` first-time setup from list order.
    pub fn priority_ids(&mut self) -> Vec<String> {
        self.priority = self.mods_only().iter().map(|m| m.id.clone()).collect();
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
        // 1-based clamp, like the Qt spinbox path. Moves the row itself so
        // list order and priority order stay one and the same (original
        // derives priority from list order).
        let at = self.mods.iter().position(|r| r.id == mid && !r.sep);
        let Some(at) = at else { return };
        let row = self.mods.remove(at);
        let slots: Vec<usize> = self.mods.iter().enumerate().filter(|(_, r)| !r.sep).map(|(i, _)| i).collect();
        let idx = number.saturating_sub(1).min(slots.len());
        let pos = slots.get(idx).copied().unwrap_or(self.mods.len());
        self.mods.insert(pos, row);
        self.priority = self.mods_only().iter().map(|m| m.id.clone()).collect();
    }

    /// Drag-drop reorder: move a mod before another row (or to the end).
    /// `before` may be a mod or a separator id; separators themselves stay put
    /// and section membership follows position, like the original list.
    pub fn move_before(&mut self, mid: &str, before: Option<&str>) {
        let at = self.mods.iter().position(|r| r.id == mid && !r.sep);
        let Some(at) = at else { return };
        let row = self.mods.remove(at);
        let pos = before
            .and_then(|b| self.mods.iter().position(|r| r.id == b))
            .unwrap_or(self.mods.len());
        self.mods.insert(pos.min(self.mods.len()), row);
        self.priority = self.mods_only().iter().map(|m| m.id.clone()).collect();
    }

    pub fn state_deployed(&mut self, written_by: &std::collections::BTreeMap<String, String>) {
        // Per-file writer: the caller passes path -> winning mod id in rank
        // order, so shared files attribute to the highest-priority owner.
        self.deployed = written_by.clone();
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

    /// Snapshot the currently enabled mods as a new profile.
    pub fn save_profile(&mut self, name: &str) -> Profile {
        let p = Profile {
            id: format!("prof{}", uuid::Uuid::new_v4().simple()),
            name: name.trim().to_string(),
            enabled: self.mods_only().iter().filter(|m| m.enabled).map(|m| m.id.clone()).collect(),
            updated: chrono::Local::now().timestamp(),
        };
        self.profiles.push(p.clone());
        p
    }

    /// Exact restore: enable exactly the profile's still-installed mods,
    /// disable everything else. Unknown ids (uninstalled since) are ignored.
    /// Returns false when the profile doesn't exist.
    pub fn apply_profile(&mut self, id: &str) -> bool {
        let Some(p) = self.profiles.iter().find(|p| p.id == id) else {
            return false;
        };
        let want: std::collections::BTreeSet<&str> =
            p.enabled.iter().map(|s| s.as_str()).collect();
        for m in self.mods.iter_mut().filter(|r| !r.sep) {
            m.enabled = want.contains(m.id.as_str());
        }
        true
    }

    pub fn delete_profile(&mut self, id: &str) -> bool {
        let n = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        self.profiles.len() != n
    }

    /// Returns false for unknown ids and blank names.
    pub fn rename_profile(&mut self, id: &str, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        match self.profiles.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.name = name.to_string();
                true
            }
            None => false,
        }
    }

    /// Guarantee the list starts with a Default profile: seed one snapshotting
    /// the currently enabled mods when the list is empty. The id is fixed
    /// (`"default"`, not a uuid) so repeated runs converge instead of
    /// duplicating. Note this also re-seeds after the user deletes their last
    /// profile — an empty list always means "no profiles yet".
    /// Returns true when a profile was created.
    pub fn ensure_default_profile(&mut self) -> bool {
        if !self.profiles.is_empty() {
            return false;
        }
        let p = Profile {
            id: "default".to_string(),
            name: "Default".to_string(),
            enabled: self.mods_only().iter().filter(|m| m.enabled).map(|m| m.id.clone()).collect(),
            updated: chrono::Local::now().timestamp(),
        };
        self.profiles.push(p);
        true
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
    #[test]
    fn replace_insert_keeps_rank() {
        // Update/reinstall flow: new row takes the old row's list slot, and
        // the priority rank must follow instead of dropping to the end.
        let mut s = AppState::default();
        s.mods = vec![row("a", "A"), row("b", "B"), row("c", "C")];
        assert_eq!(s.priority_ids(), vec!["a", "b", "c"]);
        let at = s.mods.iter().position(|r| r.id == "b").unwrap();
        s.remove_rows(&["b".to_string()]);
        s.mods.insert(at.min(s.mods.len()), row("b2", "B2"));
        assert_eq!(s.priority_ids(), vec!["a", "b2", "c"]);
    }
    #[test]
    fn desynced_priority_heals() {
        // A stale priority vec (e.g. from an older section move) snaps back
        // to list order on next call.
        let mut s = AppState::default();
        s.mods = vec![row("a", "A"), row("b", "B"), row("c", "C")];
        s.priority = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        assert_eq!(s.priority_ids(), vec!["a", "b", "c"]);
    }
    #[test]
    fn profiles_save_apply_delete_rename() {
        let mut s = AppState::default();
        s.mods = vec![row("a", "A"), row("b", "B"), row("c", "C")];
        s.set_enabled(&["b".to_string()], false);
        let p = s.save_profile("  questing  ");
        assert_eq!(p.name, "questing", "names are trimmed");
        assert_eq!(p.enabled, vec!["a".to_string(), "c".to_string()]);
        // Diverge, then exact-restore.
        s.set_enabled(&["a".to_string(), "b".to_string(), "c".to_string()], true);
        assert!(s.apply_profile(&p.id));
        assert!(s.get("a").unwrap().enabled);
        assert!(!s.get("b").unwrap().enabled);
        assert!(s.get("c").unwrap().enabled);
        // Unknown ids (uninstalled since saving) are ignored, not enabled.
        s.profiles[0].enabled.push("ghost".to_string());
        assert!(s.apply_profile(&p.id));
        assert!(!s.get("b").unwrap().enabled);
        assert!(!s.apply_profile("nope"));
        assert!(s.rename_profile(&p.id, "boss fight"));
        assert_eq!(s.profiles[0].name, "boss fight");
        assert!(!s.rename_profile(&p.id, "   "));
        assert!(!s.rename_profile("nope", "x"));
        assert!(s.delete_profile(&p.id));
        assert!(!s.delete_profile(&p.id));
        assert!(s.profiles.is_empty());
    }
    #[test]
    fn default_profile_seeds_once() {
        let mut s = AppState::default();
        s.mods = vec![row("a", "A"), row("b", "B")];
        s.set_enabled(&["b".to_string()], false);
        assert!(s.ensure_default_profile());
        assert_eq!(s.profiles.len(), 1);
        assert_eq!(s.profiles[0].id, "default");
        assert_eq!(s.profiles[0].name, "Default");
        assert_eq!(s.profiles[0].enabled, vec!["a".to_string()]);
        // Second call converges: no duplicate.
        assert!(!s.ensure_default_profile());
        assert_eq!(s.profiles.len(), 1);
        // An existing list (even without Default) is left alone.
        let mut t = AppState::default();
        t.mods = vec![row("a", "A")];
        t.save_profile("mine");
        assert!(!t.ensure_default_profile());
        assert_eq!(t.profiles.len(), 1);
    }
}
