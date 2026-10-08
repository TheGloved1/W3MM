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

/// One profile's full mod set, persisted at `profiles/<id>/state.json`.
/// Staged files stay shared under `staging/<mod-id>/` — only the rows are
/// per-profile, so separate sets cost no extra disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModSet {
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

/// Profile metadata as kept in the root `state.json`. The set itself lives
/// in the profile dir; counts are filled in live by `profile_views()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub updated: i64,
}

/// Profile metadata enriched with live mod counts, for the Profiles page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub updated: i64,
    #[serde(default)]
    pub mods: usize,
    #[serde(default)]
    pub enabled: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    /// The active profile's working set (persisted under its profile dir).
    #[serde(default)]
    pub mods: Vec<ModRow>,
    /// Top-priority first (python `state["priority"]`).
    #[serde(default)]
    pub priority: Vec<String>,
    /// Profile metadata mirror (persisted in the root file by `save_store`,
    /// never serialized from here).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Active profile id (same persistence story as `profiles`).
    #[serde(default)]
    pub active_profile: Option<String>,
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

/// What `list_mods` returns: the active profile's working set plus live
/// per-profile metadata. Keeps full stored sets off the IPC path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModsView {
    #[serde(default)]
    pub mods: Vec<ModRow>,
    #[serde(default)]
    pub priority: Vec<String>,
    #[serde(default)]
    pub deployed: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub filelist_added: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub resolutions: std::collections::BTreeMap<String, Vec<usize>>,
    #[serde(default)]
    pub merge_kept: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub profiles: Vec<ProfileView>,
    #[serde(default)]
    pub active_profile: Option<String>,
}

impl AppState {
    pub fn mods_view(&mut self, root: &std::path::Path) -> ModsView {
        let order = self.priority_ids();
        ModsView {
            mods: self.mods.clone(),
            priority: order,
            deployed: self.deployed.clone(),
            filelist_added: self.filelist_added.clone(),
            resolutions: self.resolutions.clone(),
            merge_kept: self.merge_kept.clone(),
            profiles: self.profile_views(root),
            active_profile: self.active_profile.clone(),
        }
    }

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

    /// Snapshot the working set (active profile's data) for persistence.
    pub fn snapshot_set(&self) -> ModSet {
        ModSet {
            mods: self.mods.clone(),
            priority: self.priority.clone(),
            deployed: self.deployed.clone(),
            filelist_added: self.filelist_added.clone(),
            resolutions: self.resolutions.clone(),
            merge_kept: self.merge_kept.clone(),
        }
    }

    /// Load a set into the working fields.
    pub fn load_set(&mut self, set: ModSet) {
        self.mods = set.mods;
        self.priority = set.priority;
        self.deployed = set.deployed;
        self.filelist_added = set.filelist_added;
        self.resolutions = set.resolutions;
        self.merge_kept = set.merge_kept;
    }

    fn unique_profile_name(&self, base: &str) -> String {
        if !self.profiles.iter().any(|p| p.name == base) {
            return base.to_string();
        }
        let mut n = 2;
        loop {
            let cand = format!("{base} {n}");
            if !self.profiles.iter().any(|p| p.name == cand) {
                return cand;
            }
            n += 1;
        }
    }

    /// Clone the working set into a new profile (NMM: new profiles copy the
    /// active one). Does not switch.
    pub fn create_profile(&mut self, root: &std::path::Path, name: &str) -> Result<Profile, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("name the profile first".into());
        }
        let p = Profile {
            id: format!("prof{}", uuid::Uuid::new_v4().simple()),
            name: self.unique_profile_name(name),
            updated: chrono::Local::now().timestamp(),
        };
        write_profile_set(root, &p.id, &self.snapshot_set())?;
        self.profiles.push(p.clone());
        Ok(p)
    }

    /// Switch the working set to another profile: park the current set into
    /// the outgoing profile dir, load the target. No-op when already active.
    pub fn switch_profile(&mut self, root: &std::path::Path, id: &str) -> Result<(), String> {
        if self.active_profile.as_deref() == Some(id) {
            return Ok(());
        }
        if !self.profiles.iter().any(|p| p.id == id) {
            return Err("profile not found".into());
        }
        if let Some(cur) = self.active_profile.clone() {
            write_profile_set(root, &cur, &self.snapshot_set())?;
        }
        let set = read_profile_set(root, id)?;
        self.load_set(set);
        self.active_profile = Some(id.to_string());
        Ok(())
    }

    /// Copy any profile's set under "<name>_copy" (suffixed further while
    /// taken). Works on inactive profiles too, unlike create.
    pub fn duplicate_profile(&mut self, root: &std::path::Path, id: &str) -> Result<Profile, String> {
        let src = self.profiles.iter().find(|p| p.id == id).cloned().ok_or("profile not found")?;
        let set = read_profile_set(root, id)?;
        let p = Profile {
            id: format!("prof{}", uuid::Uuid::new_v4().simple()),
            name: self.unique_profile_name(&format!("{}_copy", src.name)),
            updated: chrono::Local::now().timestamp(),
        };
        write_profile_set(root, &p.id, &set)?;
        self.profiles.push(p.clone());
        Ok(p)
    }

    /// Delete a profile's dir and metadata. The active profile cannot be
    /// deleted — switch away first (NMM guards the last profile the same way).
    pub fn delete_profile(&mut self, root: &std::path::Path, id: &str) -> Result<(), String> {
        if self.active_profile.as_deref() == Some(id) {
            return Err("switch to another profile before deleting this one".into());
        }
        if !self.profiles.iter().any(|p| p.id == id) {
            return Err("profile not found".into());
        }
        self.profiles.retain(|p| p.id != id);
        let dir = profile_dir(root, id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        Ok(())
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
                p.updated = chrono::Local::now().timestamp();
                true
            }
            None => false,
        }
    }

    /// Guarantee the store starts with a Default profile. Seeds an (initially
    /// empty or working-set-cloned) Default dir; repeated runs converge.
    /// Returns true when anything was created.
    pub fn ensure_default_profile(&mut self, root: &std::path::Path) -> Result<bool, String> {
        if self.profiles.iter().any(|p| p.id == "default") {
            return Ok(false);
        }
        let set = if self.profiles.is_empty() {
            // Fresh store (or legacy flat state): Default adopts the working set.
            self.snapshot_set()
        } else {
            ModSet::default()
        };
        write_profile_set(root, "default", &set)?;
        self.profiles.insert(
            0,
            Profile { id: "default".to_string(), name: "Default".to_string(), updated: chrono::Local::now().timestamp() },
        );
        if self.active_profile.is_none() {
            self.active_profile = Some("default".to_string());
            self.load_set(set);
        }
        Ok(true)
    }

    /// Live per-profile mod/enabled counts for the Profiles page. The active
    /// profile's counts come from the working set; the rest are read from
    /// their dirs (missing/unreadable files count as empty).
    pub fn profile_views(&self, root: &std::path::Path) -> Vec<ProfileView> {
        self.profiles
            .iter()
            .map(|p| {
                let (mods, enabled) = if self.active_profile.as_deref() == Some(p.id.as_str()) {
                    let ms: Vec<&ModRow> = self.mods_only();
                    (ms.len(), ms.iter().filter(|m| m.enabled).count())
                } else {
                    match read_profile_set(root, &p.id) {
                        Ok(set) => {
                            let ms: Vec<&ModRow> = set.mods.iter().filter(|r| !r.sep).collect();
                            (ms.len(), ms.iter().filter(|m| m.enabled).count())
                        }
                        Err(_) => (0, 0),
                    }
                };
                ProfileView { id: p.id.clone(), name: p.name.clone(), updated: p.updated, mods, enabled }
            })
            .collect()
    }

    /// True when a mod id is referenced by the working set (minus `ignore`)
    /// or any stored profile set. Guards shared staged-file deletion.
    pub fn is_mod_referenced(&self, root: &std::path::Path, id: &str, ignore: &[String]) -> bool {
        if self.mods.iter().any(|m| !m.sep && m.id == id && !ignore.contains(&m.id)) {
            return true;
        }
        self.profiles.iter().any(|p| {
            read_profile_set(root, &p.id)
                .map(|set| set.mods.iter().any(|m| !m.sep && m.id == id))
                .unwrap_or(false)
        })
    }
}

pub const PROFILES_DIR: &str = "profiles";
const PROFILE_STATE_FILE: &str = "state.json";

/// Root store file: metadata only. The working set and per-profile sets live
/// under `profiles/<id>/`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoreMeta {
    #[serde(default)]
    staging_override: String,
    #[serde(default)]
    active_profile: Option<String>,
    #[serde(default)]
    profiles: Vec<Profile>,
}

pub fn profile_dir(root: &std::path::Path, id: &str) -> PathBuf {
    root.join(PROFILES_DIR).join(id)
}

fn write_json_atomic(path: &PathBuf, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read one profile's set. A missing file means an empty set (fresh profile);
/// an unreadable one is damage.
pub fn read_profile_set(root: &std::path::Path, id: &str) -> Result<ModSet, String> {
    let p = profile_dir(root, id).join(PROFILE_STATE_FILE);
    match std::fs::read_to_string(&p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ModSet::default()),
        Err(e) => Err(e.to_string()),
        Ok(text) => serde_json::from_str::<ModSet>(&text)
            .map_err(|e| format!("profile data for '{id}' is damaged and couldn't be read: {e}")),
    }
}

pub fn write_profile_set(root: &std::path::Path, id: &str, set: &ModSet) -> Result<(), String> {
    write_json_atomic(&profile_dir(root, id).join(PROFILE_STATE_FILE).to_path_buf(), set)
}

/// Load the whole store: root metadata plus the active profile's set into
/// the working fields. Legacy single-file layouts (flat mod list, or the
/// enable-selection profiles era) are migrated to profile dirs on the spot;
/// the migration is idempotent. Missing files mean a fresh store.
pub fn load_store(root: &std::path::Path) -> Result<AppState, String> {
    let meta_path = root.join(crate::home::STATE_FILE);
    let bak_path = meta_path.with_extension("json.bak");
    let text = match std::fs::read_to_string(&meta_path)
        .or_else(|_| std::fs::read_to_string(&bak_path))
    {
        Ok(t) => t,
        Err(_) => return Ok(AppState::default()),
    };
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| format!("The mod list in {} is damaged and couldn't be read. Nothing was changed.", meta_path.display()))?;
    if is_selection_era(&v) {
        return migrate_selection_store(root, &v);
    }
    let meta: StoreMeta = serde_json::from_value(v).map_err(|_| {
        format!("The mod list in {} is damaged and couldn't be read. Nothing was changed.", meta_path.display())
    })?;
    let mut st = AppState {
        staging_override: meta.staging_override,
        active_profile: meta.active_profile,
        profiles: meta.profiles,
        ..Default::default()
    };
    st.priority_ids();
    if let Some(active) = st.active_profile.clone() {
        if profile_dir(root, &active).join(PROFILE_STATE_FILE).is_file() {
            st.load_set(read_profile_set(root, &active)?);
            st.priority_ids();
        }
    }
    Ok(st)
}

/// The enable-selection era stored `{id,name,enabled[],updated}` profiles.
fn is_selection_era(v: &serde_json::Value) -> bool {
    v.get("profiles").and_then(|p| p.as_array()).map(|arr| {
        arr.iter().any(|p| p.get("enabled").and_then(|e| e.as_array()).is_some())
    }).unwrap_or(false)
}

/// Convert a selection-era store: every old profile becomes a full set
/// holding just its enabled rows (working-set order, forced enabled); the
/// working set becomes the migrated active profile's set.
fn migrate_selection_store(root: &std::path::Path, v: &serde_json::Value) -> Result<AppState, String> {
    #[derive(Deserialize)]
    struct OldProfile {
        id: String,
        name: String,
        #[serde(default)]
        enabled: Vec<String>,
        #[serde(default)]
        updated: i64,
    }
    let mut st = AppState::default();
    st.mods = v.get("mods").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    st.priority = v.get("priority").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    st.deployed = v.get("deployed").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    st.filelist_added = v.get("filelist_added").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    st.resolutions = v.get("resolutions").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    st.merge_kept = v.get("merge_kept").and_then(|m| serde_json::from_value(m.clone()).ok()).unwrap_or_default();
    st.staging_override = v.get("staging_override").and_then(|s| s.as_str()).unwrap_or_default().to_string();
    let old_profiles: Vec<OldProfile> = v
        .get("profiles")
        .and_then(|p| serde_json::from_value(p.clone()).ok())
        .unwrap_or_default();
    let order: Vec<String> = st.mods.iter().filter(|r| !r.sep).map(|r| r.id.clone()).collect();
    for old in &old_profiles {
        let wanted: std::collections::BTreeSet<&str> = old.enabled.iter().map(|s| s.as_str()).collect();
        let rows: Vec<ModRow> = st
            .mods
            .iter()
            .filter(|r| !r.sep && wanted.contains(r.id.as_str()))
            .map(|r| {
                let mut c = r.clone();
                c.enabled = true;
                c
            })
            .collect();
        let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        let prio: Vec<String> = order.iter().filter(|id| ids.contains(id)).cloned().collect();
        write_profile_set(
            root,
            &old.id,
            &ModSet { mods: rows, priority: prio, ..Default::default() },
        )?;
        st.profiles.push(Profile { id: old.id.clone(), name: old.name.clone(), updated: old.updated });
    }
    let old_active = v.get("active_profile").and_then(|a| a.as_str()).map(|s| s.to_string());
    st.active_profile = match old_active {
        Some(id) if st.profiles.iter().any(|p| p.id == id) => Some(id),
        _ => st.profiles.iter().find(|p| p.id == "default").or(st.profiles.first()).map(|p| p.id.clone()),
    };
    if let Some(active) = st.active_profile.clone() {
        st.load_set(read_profile_set(root, &active)?);
        st.priority_ids();
    }
    Ok(st)
}

/// Persist the store: working set into the active profile's dir, metadata
/// (staging override, active id, profile list) into the root file.
pub fn save_store(root: &std::path::Path, state: &AppState) -> Result<(), String> {
    std::fs::create_dir_all(root.join(PROFILES_DIR)).map_err(|e| e.to_string())?;
    if let Some(active) = &state.active_profile {
        write_profile_set(root, active, &state.snapshot_set())?;
    }
    write_json_atomic(
        &root.join(crate::home::STATE_FILE).to_path_buf(),
        &StoreMeta {
            staging_override: state.staging_override.clone(),
            active_profile: state.active_profile.clone(),
            profiles: state.profiles.clone(),
        },
    )
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
    fn test_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("yawmm-profiles-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn profile_dirs_round_trip() {
        let root = test_root("roundtrip");
        let mut s = AppState::default();
        s.mods = vec![row("a", "A"), row("b", "B"), row("c", "C")];
        s.set_enabled(&["b".to_string()], false);
        // Create clones the working set; blank names rejected.
        let p = s.create_profile(&root, "  questing  ").unwrap();
        assert_eq!(p.name, "questing", "names are trimmed");
        assert!(s.create_profile(&root, "   ").is_err());
        assert!(s.switch_profile(&root, "nope").is_err());
        // Diverge the working set, then switch back: exact restore.
        s.set_enabled(&["a".to_string(), "b".to_string(), "c".to_string()], true);
        s.switch_profile(&root, &p.id).unwrap();
        assert_eq!(s.active_profile.as_deref(), Some(p.id.as_str()));
        assert!(s.get("a").unwrap().enabled);
        assert!(!s.get("b").unwrap().enabled);
        // Switching to the active profile is a no-op success.
        assert!(s.switch_profile(&root, &p.id).is_ok());
        // Diverge again, then duplicate: the copy holds the STORED set.
        s.set_enabled(&["b".to_string()], true);
        let q = s.duplicate_profile(&root, &p.id).unwrap();
        assert_eq!(q.name, "questing_copy");
        s.switch_profile(&root, &q.id).unwrap();
        assert!(!s.get("b").unwrap().enabled, "duplicate holds the stored selection");
        let q2 = s.duplicate_profile(&root, &p.id).unwrap();
        assert_eq!(q2.name, "questing_copy 2");
        assert!(s.duplicate_profile(&root, "nope").is_err());
        // Rename ok/blank/unknown.
        assert!(s.rename_profile(&q.id, "boss fight"));
        assert_eq!(s.profiles.iter().find(|x| x.id == q.id).unwrap().name, "boss fight");
        assert!(!s.rename_profile(&q.id, "   "));
        assert!(!s.rename_profile("nope", "x"));
        // Active profile cannot be deleted; others can (twice = gone).
        assert!(s.delete_profile(&root, &q2.id).is_ok());
        assert!(s.delete_profile(&root, &q.id).is_err(), "active is protected");
        assert!(s.delete_profile(&root, "nope").is_err());
        // Reference gating: shared staged files survive partial uninstalls.
        assert!(s.is_mod_referenced(&root, "a", &[]));
        assert!(s.is_mod_referenced(&root, "a", &["a".to_string()]), "other profiles still hold it");
        assert!(!s.is_mod_referenced(&root, "zzz", &[]));
        assert_eq!(s.profile_views(&root).len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn default_seeds_into_dir() {
        let root = test_root("default");
        let mut s = AppState::default();
        s.mods = vec![row("a", "A")];
        assert!(s.ensure_default_profile(&root).unwrap());
        assert_eq!(s.profiles.len(), 1);
        assert_eq!(s.profiles[0].id, "default");
        assert_eq!(s.active_profile.as_deref(), Some("default"));
        // The seeded set adopts the working set (legacy-flat upgrade path).
        let set = read_profile_set(&root, "default").unwrap();
        assert_eq!(set.mods.len(), 1);
        // Second call converges: no duplicate.
        assert!(!s.ensure_default_profile(&root).unwrap());
        assert_eq!(s.profiles.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn selection_era_migrates_to_dirs() {
        // Old enable-selection store: Default + one selection, active set.
        let root = test_root("migrate");
        let old = serde_json::json!({
            "mods": [
                {"id": "a", "name": "A", "enabled": true},
                {"id": "b", "name": "B", "enabled": false},
                {"id": "c", "name": "C", "enabled": true}
            ],
            "priority": ["a", "b", "c"],
            "profiles": [
                {"id": "default", "name": "Default", "enabled": ["a", "c"], "updated": 1},
                {"id": "p1", "name": "Speed", "enabled": ["c", "ghost"], "updated": 2}
            ],
            "active_profile": "p1"
        });
        std::fs::write(root.join("state.json"), serde_json::to_string(&old).unwrap()).unwrap();
        let st = load_store(&root).unwrap();
        assert_eq!(st.active_profile.as_deref(), Some("p1"));
        // Speed holds just its (existing) enabled rows, forced enabled.
        let speed = read_profile_set(&root, "p1").unwrap();
        assert_eq!(speed.mods.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["c"]);
        assert!(speed.mods.iter().all(|m| m.enabled));
        // Ghost ids vanish; Default keeps its snapshot.
        let def = read_profile_set(&root, "default").unwrap();
        assert_eq!(def.mods.len(), 2);
        // Working set is the active profile's set.
        assert_eq!(st.mods.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["c"]);
        // Second load is a plain new-layout load (idempotent).
        let st2 = load_store(&root).unwrap();
        assert_eq!(st2.profiles.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn store_round_trips() {
        let root = test_root("roundtrip-store");
        let mut s = AppState::default();
        s.mods = vec![row("a", "A")];
        s.staging_override = "/tmp/stage".to_string();
        s.ensure_default_profile(&root).unwrap();
        s.create_profile(&root, "alt").unwrap();
        save_store(&root, &s).unwrap();
        let back = load_store(&root).unwrap();
        assert_eq!(back.active_profile, s.active_profile);
        assert_eq!(back.profiles.len(), 2);
        assert_eq!(back.staging_override, "/tmp/stage");
        assert_eq!(back.mods.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
