//! YAWMM backend: Tauri commands over the ported ModManager core.
//!
//! Mapping to `w3modmanager.py`:
//! - list/set/priority -> `state.rs` (ModManager rows/priority)
//! - deploy/status -> `deploy.rs` (sync/_deploy_missing/mods.settings/filelists)
//! - install/preview -> `install.rs` + `archive.rs` (analyze/build_staging)
//! - merges -> `script_merge.rs` / `xml_merge.rs`
//! - nexus/check -> `nexus.rs`

mod archive;
mod bundles;
mod deploy;
mod downloads;
mod home;
mod install;
mod keybinds;
mod manager;
mod merger;
mod nexus;
mod script_merge;
mod settings;
mod state;
mod steam;
mod version;
mod xml_merge;

use manager::Manager;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::io::Write;
use std::path::PathBuf;
use tauri::State;

/// Platform state dir log file (`~/.local/state/yawmm/yawmm.log` on Linux,
/// `%LOCALAPPDATA%/yawmm/yawmm.log` on Windows).
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Writable per-user state base dir, with platform fallbacks so a missing
/// `HOME` (Windows services, odd containers) still resolves somewhere.
fn state_base_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg);
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        if !local.is_empty() {
            return PathBuf::from(local);
        }
    }
    for var in ["HOME", "USERPROFILE"] {
        if let Some(home) = std::env::var_os(var) {
            if !home.is_empty() {
                let mut p = PathBuf::from(home);
                #[cfg(target_os = "windows")]
                return p;
                #[cfg(not(target_os = "windows"))]
                {
                    p.push(".local");
                    p.push("state");
                    return p;
                }
            }
        }
    }
    PathBuf::from(".")
}

fn log_path() -> &'static PathBuf {
    LOG_PATH.get_or_init(|| {
        let base = state_base_dir();
        let mut dir = base.clone();
        dir.push("yawmm");
        let _ = std::fs::create_dir_all(&dir);
        dir.push("yawmm.log");
        // One-time brand rename: adopt the old log so history survives.
        // Runs before the first write (this init), so a rename keeps the
        // whole file; an already-migrated install skips it.
        let legacy = base.join("w3mm").join("w3mm.log");
        if !dir.exists() && legacy.is_file() {
            if std::fs::rename(&legacy, &dir).is_err() {
                let _ = std::fs::copy(&legacy, &dir).and_then(|_| std::fs::remove_file(&legacy));
            }
        }
        dir
    })
}

fn log_line(level: &str, msg: &str) {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let tid = format!("{:?}", std::thread::current().id()).replace("ThreadId(", "").replace(')', "");
    let line = format!("{stamp} [{level}:t{tid}] {msg}\n");
    eprintln!("{}", line.trim_end());
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// Frontend can push a line into the same file.
#[tauri::command]
fn frontend_log(msg: String) {
    log_line("frontend", &msg);
}

type Shared = Mutex<Option<Manager>>;

/// Lock the global state, logging whenever we have to wait on it — a stuck
/// holder freezes every command behind it, so waits are always suspicious.
fn lock_shared<'a>(shared: &'a State<Shared>, ctx: &str) -> Result<std::sync::MutexGuard<'a, Option<Manager>>, String> {
    let start = std::time::Instant::now();
    let mut warned = false;
    loop {
        match shared.try_lock() {
            Ok(g) => {
                if warned {
                    log_line("rust", &format!("lock {ctx}: acquired after {:.1}s", start.elapsed().as_secs_f32()));
                }
                return Ok(g);
            }
            Err(std::sync::TryLockError::Poisoned(_)) => return Err("state lock poisoned".into()),
            Err(std::sync::TryLockError::WouldBlock) => {
                if !warned && start.elapsed().as_secs_f32() >= 1.0 {
                    warned = true;
                    log_line("rust", &format!("lock {ctx}: WAITING on state lock..."));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

#[tauri::command]
fn open_manager(shared: State<Shared>, game_dir: String, prefix: String, staging_dir: Option<String>) -> Result<bool, String> {
    let m = Manager::open(&game_dir, &prefix, staging_dir.as_deref().unwrap_or(""))?;
    crate::nexus::load_version_cache(std::path::Path::new(&game_dir));
    log_launch_env();
    *lock_shared(&shared, "open_manager")? = Some(m);
    Ok(true)
}

/// Storage layout report: where staging lives vs the game, and whether the
/// two share a filesystem (hardlinks work) or not (deploys silently fall
/// back to full copies: slower, 2x disk). Powers the move-staging nudge.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageReport {
    pub game: String,
    pub staging: String,
    pub default_staging: String,
    pub same_device: Option<bool>,
}

#[tauri::command]
fn storage_report(shared: State<Shared>) -> Result<StorageReport, String> {
    let g = lock_shared(&shared, "storage_report")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    // Ensure both sides exist so metadata comparison never answers None
    // just because staging hasn't been created yet.
    let _ = std::fs::create_dir_all(&m.home.staging);
    Ok(StorageReport {
        game: m.home.game.to_string_lossy().to_string(),
        staging: m.home.staging.to_string_lossy().to_string(),
        default_staging: m.home.default_staging().to_string_lossy().to_string(),
        same_device: crate::home::same_filesystem(&m.home.game, &m.home.staging),
    })
}

#[tauri::command]
fn detect_game() -> Option<String> {
    steam::detect_game_dir()
}

#[tauri::command]
fn default_prefix(game_dir: String) -> Option<String> {
    steam::default_prefix_for(&game_dir)
}

#[tauri::command]
fn is_game_dir(path: String) -> bool {
    home::is_game_dir(&path)
}

#[tauri::command]
fn list_mods(shared: State<Shared>) -> Result<state::AppState, String> {
    let cloned = {
        let g = lock_shared(&shared, "list_mods")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        let mut st = s.clone();
        // Normalize the copy so the UI never shows a stale priority order
        // even before the next mutating command persists the heal.
        st.priority_ids();
        st
    };
    Ok(cloned)
}

#[tauri::command]
fn set_enabled(shared: State<Shared>, ids: Vec<String>, on: bool) -> Result<bool, String> {
    let g = lock_shared(&shared, "set_enabled")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.set_enabled(&ids, on);
    m.save()?;
    Ok(true)
}

/// Snapshot the currently enabled mods as a named profile.
#[tauri::command]
fn save_profile(shared: State<Shared>, name: String) -> Result<state::Profile, String> {
    if name.trim().is_empty() {
        return Err("name the profile first".into());
    }
    let g = lock_shared(&shared, "save_profile")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let p = {
        m.state.lock().map_err(|e| e.to_string())?.save_profile(&name)
    };
    m.save()?;
    Ok(p)
}

/// Exact restore of a profile's saved selection (frontend redeploys after).
#[tauri::command]
fn apply_profile(shared: State<Shared>, id: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "apply_profile")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let ok = {
        m.state.lock().map_err(|e| e.to_string())?.apply_profile(&id)
    };
    if !ok {
        return Err("profile not found".into());
    }
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn delete_profile(shared: State<Shared>, id: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "delete_profile")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let ok = {
        m.state.lock().map_err(|e| e.to_string())?.delete_profile(&id)
    };
    if !ok {
        return Err("profile not found".into());
    }
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn rename_profile(shared: State<Shared>, id: String, name: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "rename_profile")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let ok = {
        m.state.lock().map_err(|e| e.to_string())?.rename_profile(&id, &name)
    };
    if !ok {
        return Err("profile not found (or blank name)".into());
    }
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn set_priority(shared: State<Shared>, id: String, number: usize) -> Result<bool, String> {
    let g = lock_shared(&shared, "set_priority")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        s.priority_ids();
        s.set_priority_number(&id, number);
    }
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn rename_mod(shared: State<Shared>, id: String, name: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "rename_mod")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.rename_mod(&id, &state::ensure_mod_prefix(&name));
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn remove_mods(shared: State<Shared>, ids: Vec<String>) -> Result<bool, String> {
    // Drop staged folders first (slow) without the lock so disk matches state.
    let staging: std::path::PathBuf = {
        let g = lock_shared(&shared, "remove_mods")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        m.home.staging.clone()
    };
    for id in &ids {
        let dir = staging.join(id);
        if dir.is_dir() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    let g = lock_shared(&shared, "remove_mods")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.remove_rows(&ids);
    m.save()?;
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeployReport {
    pub deployed: Vec<String>,
    pub removed: Vec<String>,
}

/// Snapshot taken under lock for one deploy's commit phase: stale deployed
/// paths, kept merges, every known target, managed top folders, and the
/// filelist entries previously added.
type DeploySnapshot = (
    Vec<String>,
    Vec<(String, String)>,
    Vec<String>,
    std::collections::HashSet<String>,
    std::collections::BTreeMap<String, Vec<String>>,
);

#[tauri::command]
fn deploy(shared: State<Shared>) -> Result<DeployReport, String> {
    if steam::game_running() {
        return Err("Close the game before deploying".to_string());
    }
    // Snapshot under lock; every slow file op below runs lock-free so other
    // commands (starting a download, listing mods) never queue behind a big
    // deploy. State is re-read and committed under a fresh lock at the end.
    let (home, ranked): (crate::home::Home, Vec<state::ModRow>) = {
        let g = lock_shared(&shared, "deploy")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        let order = s.priority_ids();
        let mods = s.mods_only().into_iter().cloned().collect::<Vec<_>>();
        let mut order_idx = std::collections::HashMap::new();
        for (i, id) in order.iter().enumerate() {
            order_idx.insert(id.clone(), i);
        }
        let mut ranked: Vec<state::ModRow> = mods.into_iter().filter(|r| r.enabled).collect();
        ranked.sort_by_key(|r| order_idx.get(&r.id).copied().unwrap_or(usize::MAX));
        (m.home.clone(), ranked)
    };
    let t0 = std::time::Instant::now();
    log_line("rust", &format!("deploy: start, {} ranked mods", ranked.len()));
    let mut all_written: Vec<String> = vec![];
    // Per-file on-disk winner: deploy order is rank order with later writes
    // overwriting, so last insert is what is actually on disk.
    let mut written_by: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut menu_xmls: Vec<String> = vec![];
    for r in &ranked {
        let stage = home.staging.join(&r.id);
        if !stage.is_dir() {
            continue;
        }
        let rels = deploy::mod_files(&stage);
        let written = deploy::deploy_mod(&home.game, &stage, &rels, &home.backup)
            .map_err(|e| format!("{}: {e}", r.name))?;
        log_line("rust", &format!("deploy: {} ({} files)", r.name, written.len()));
        for w in &written {
            if w.to_lowercase().ends_with(".xml") && w.to_lowercase().contains("bin/") {
                menu_xmls.push(w.clone());
            }
        }
        for w in written {
            written_by.insert(w.clone(), r.id.clone());
            all_written.push(w);
        }
    }
    // Anything previously deployed but no longer wanted (disabled/removed
    // mods, or files a mod update dropped) goes back to backup/vanilla.
    // Commit phase: snapshot what to restore/write, do it lock-free, then
    // persist state under a fresh lock. Also snapshot every known target so
    // orphans the deployed map lost can still be reconciled.
    let (stale, kept, known_targets, managed_folders, filelist_added): DeploySnapshot = {
        let g = lock_shared(&shared, "deploy")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        let want: std::collections::HashSet<String> = all_written.iter().cloned().collect();
        let stale = s.deployed.keys().filter(|k| !want.contains(*k)).cloned().collect();
        let kept = s.merge_kept.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut known_targets = vec![];
        let mut managed_folders = std::collections::HashSet::new();
        for row in s.mods_only() {
            for t in &row.targets {
                known_targets.push(t.clone());
                let low = t.to_lowercase();
                if let Some(slash) = low.find('/') {
                    let first = &low[..slash];
                    if first == "mods" || first == "dlc" {
                        if let Some(rest) = low[slash + 1..].split('/').next() {
                            managed_folders.insert(format!("{first}/{rest}"));
                        }
                    }
                }
            }
        }
        (stale, kept, known_targets, managed_folders, s.filelist_added.clone())
    };
    // Reconcile orphans the deployed map lost: files under managed
    // mods/<name>/ dlc/<name>/ folders that nothing wanted, plus known
    // bin/content targets with no backup-tracked stale entry. Kept merges
    // count as wanted so they are never swept up here.
    let mut want_lower: std::collections::HashSet<String> =
        all_written.iter().map(|w| w.to_lowercase()).collect();
    for (rel, _) in &kept {
        want_lower.insert(rel.to_lowercase());
    }
    let mut orphans = deploy::scan_tree_orphans(&home.game, &want_lower, &managed_folders);
    for t in &known_targets {
        let low = t.to_lowercase();
        let first = low.split('/').next().unwrap_or("");
        if first != "bin" && first != "content" {
            continue;
        }
        if want_lower.contains(&low) {
            continue;
        }
        if home.game.join(t).is_file() && !orphans.iter().any(|o| o.to_lowercase() == low) {
            orphans.push(t.clone());
        }
    }
    orphans.sort();
    orphans.dedup();
    // Merge with the deployed-map stale set; track which removals came only
    // from reconciliation for the report.
    let stale_set: std::collections::HashSet<String> = stale.into_iter().collect();
    let mut to_restore: Vec<String> = stale_set.iter().cloned().collect();
    let mut reconciled: Vec<String> = vec![];
    for o in orphans {
        if !stale_set.contains(&o) && !stale_set.contains(&o.to_lowercase()) {
            // Case-insensitive dedupe against stale (stale keys are exact-case
            // from a previous deploy; orphans are disk-exact).
            let low = o.to_lowercase();
            if stale_set.iter().any(|s| s.to_lowercase() == low) {
                continue;
            }
            reconciled.push(o.clone());
        }
        if !to_restore.iter().any(|s| s.to_lowercase() == o.to_lowercase()) {
            to_restore.push(o);
        }
    }
    to_restore.sort();
    // Menu xmls swept this deploy that predate entry tracking: pruned from
    // the filelists even though no recorded entry claims them.
    let stale_xmls_lower: std::collections::HashSet<String> = to_restore
        .iter()
        .map(|r| r.to_lowercase())
        .filter(|l| l.ends_with(".xml") && l.contains("bin/"))
        .collect();
    if !to_restore.is_empty() {
        deploy::restore_paths(&home.game, &home.backup, &to_restore).map_err(|e| e.to_string())?;
        log_line(
            "rust",
            &format!("deploy: removed {} stale files ({} reconciled orphans)", to_restore.len(), reconciled.len()),
        );
    }
    // Kept merges win over every staged copy: write them over the deployed file.
    let mut merged_count = 0;
    for (rel, text) in &kept {
        deploy::apply_merge(&home.game, &home.backup, rel, text).map_err(|e| e.to_string())?;
        if !all_written.contains(rel) {
            all_written.push(rel.clone());
        }
        merged_count += 1;
    }
    let _ = merged_count;
    // mods.settings in priority order, names as deployed folder names.
    let names: Vec<String> = ranked.iter().map(|r| state::ensure_mod_prefix(&r.name)).collect();
    let settings = crate::manager::settings_dir_for(&home.prefix).join("mods.settings");
    deploy::write_mods_settings(&settings, &names).map_err(|e| e.to_string())?;
    let filelist_added =
        deploy::update_filelists(&home.game, &menu_xmls, &stale_xmls_lower, &filelist_added)
            .map_err(|e| e.to_string())?;
    // Persist deployed map (path -> winning mod id in deploy order) plus the
    // filelist entries we added (powers pruning after row removal/replace).
    {
        let g = lock_shared(&shared, "deploy")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        s.filelist_added = filelist_added;
        s.state_deployed(&written_by);
        let snapshot = s.clone();
        crate::state::save_state(&m.home.state_file, &snapshot)?;
    }
    log_line("rust", &format!("deploy: done, {} files in {:.1}s", all_written.len(), t0.elapsed().as_secs_f32()));
    Ok(DeployReport { deployed: all_written, removed: reconciled })
}

#[tauri::command]
fn preview_archive(path: String) -> Result<install::InstallPlan, String> {
    use std::path::Path;
    let archive = Path::new(&path);
    // Extract to a temp dir, analyze, return plan (caller confirms before staging).
    let tmp = std::env::temp_dir().join(format!("yawmm-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    archive::extract_archive(archive, &tmp).map_err(|e| e.to_string())?;
    Ok(install::analyze(&tmp))
}

#[tauri::command]
fn list_archive(path: String) -> Result<Vec<String>, String> {
    archive::list_names(std::path::Path::new(&path)).map_err(|e| e.to_string())
}

#[tauri::command]
fn merge_scripts(
    base_b64: Vec<String>,
    versions_b64: Vec<Vec<String>>,
    resolutions: Vec<usize>,
) -> Result<script_merge::MergeResult, String> {
    // Lines are passed as plain strings (no base64 needed); names kept for clarity.
    let base: Vec<String> = base_b64;
    let versions: Vec<Vec<String>> = versions_b64;
    Ok(script_merge::merge_script(&base, &versions, &resolutions))
}

#[tauri::command]
fn merge_xml(base: String, versions: Vec<String>, resolutions: Vec<usize>) -> Result<xml_merge::XmlMergeResult, String> {
    xml_merge::xml_merge(&base, &versions, &resolutions)
}

#[tauri::command]
fn scan_keybinds(root: String) -> Result<std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<String>>>, String> {
    Ok(keybinds::scan_keybind_files(std::path::Path::new(&root)))
}

#[tauri::command]
fn scan_snippets(root: String) -> Result<std::collections::BTreeMap<String, keybinds::Snippets>, String> {
    Ok(keybinds::scan_snippets(std::path::Path::new(&root)))
}

#[tauri::command]
fn nexus_status(api_key: String) -> Result<serde_json::Value, String> {
    nexus::nexus_get("/v1/users/validate.json", &api_key)
}

/// Whether an API key belongs to a premium account (direct downloads work).
#[tauri::command]
fn nexus_premium(api_key: String) -> bool {
    nexus::is_premium(&api_key)
}

#[tauri::command]
fn parse_archive_name(filename: String) -> (String, String, String) {
    nexus::parse_archive_name(&filename)
}

#[tauri::command]
fn parse_nxm(url: String) -> Option<nexus::NxmLink> {
    nexus::parse_nxm(&url)
}

#[tauri::command]
fn download_nxm(url: String, api_key: String, dest_dir: String) -> Result<String, String> {
    let link = nexus::parse_nxm(&url).ok_or("not an nxm:// link")?;
    if link.game != "witcher3" {
        return Err(format!("wrong game: {}", link.game));
    }
    let links = nexus::download_links(&link.mod_id, &link.file_id, &api_key, &link.key, &link.expires)?;
    let first = links.into_iter().next().ok_or("no links")?;
    // filename from URL tail or fallback
    let fname = first.split('?').next().unwrap_or(&first).rsplit('/').next().unwrap_or("download.zip");
    let fname = if fname.is_empty() { format!("{}.zip", link.file_id) } else { fname.to_string() };
    let dest = std::path::Path::new(&dest_dir).join(fname);
    nexus::download_url(&first, &dest, &api_key)?;
    Ok(dest.to_string_lossy().to_string())
}

#[tauri::command]
fn install_archive(shared: State<Shared>, path: String, name: String, version: String, nexus_id: String) -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("yawmm-install-{}", uuid::Uuid::new_v4().simple()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let res: Result<String, String> = (|| {
        // Manually picked archives live wherever the user left them: move
        // the file into our downloads first so the row's recorded path stays
        // valid for reinstalls and updates.
        let dl: std::path::PathBuf = {
            let g = lock_shared(&shared, "install_archive")?;
            let m = g.as_ref().ok_or("open a game folder first")?;
            m.home.downloads.clone()
        };
        let path = downloads::ensure_in_downloads(std::path::Path::new(&path), &dl)?
            .to_string_lossy()
            .to_string();
        archive::extract_archive(std::path::Path::new(&path), &tmp).map_err(|e| e.to_string())?;
        let plan = install::analyze(&tmp);
        let staging: std::path::PathBuf = {
            let g = lock_shared(&shared, "install_archive")?;
            let m = g.as_ref().ok_or("open a game folder first")?;
            m.home.staging.clone()
        };
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let folder = state::ensure_mod_prefix(&name);
        let stage = staging.join(&id);
        let (targets, _docs) = install::build_staging(&plan, &stage, &folder)?;
        {
            let g = lock_shared(&shared, "install_archive")?;
            let m = g.as_ref().ok_or("open a game folder first")?;
            // NOTE: the state guard must drop before save(): save() locks the
            // same non-reentrant mutex and would deadlock this thread forever.
            {
                let mut s = m.state.lock().map_err(|e| e.to_string())?;
                s.mods.push(state::ModRow {
                    id: id.clone(), sep: false, name: name.clone(), enabled: true,
                    version: version.clone(), nexus: nexus_id.clone(), archive: path.clone(),
                    section: String::new(), updated: chrono::Utc::now().timestamp(),
                    collapsed: false, targets, nexus_cat: String::new(), main_of: String::new(),
                });
                s.priority_ids();
            }
            m.save()?;
        }
        Ok(id)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

#[tauri::command]
fn add_separator(shared: State<Shared>, index: usize, name: String) -> Result<state::ModRow, String> {
    let g = lock_shared(&shared, "add_separator")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let row = m.state.lock().map_err(|e| e.to_string())?.add_separator(index, &name);
    m.save()?;
    Ok(row)
}

#[tauri::command]
fn edit_mod(shared: State<Shared>, id: String, name: String, version: String, nexus_id: String, section: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "edit_mod")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.edit_mod(&id, &name, &version, &nexus_id, &section);
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn find_collisions(shared: State<Shared>, targets: Vec<String>) -> Result<(Vec<String>, Vec<String>), String> {
    let out = {
        let g = lock_shared(&shared, "find_collisions")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.find_collisions(&targets)
    };
    Ok(out)
}

#[tauri::command]
fn clashes(shared: State<Shared>) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let out = {
        let g = lock_shared(&shared, "clashes")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        s.clashes()
    };
    Ok(out)
}

#[tauri::command]
fn merge_check(lines: Vec<String>) -> Vec<String> {
    script_merge::function_check(&lines)
}

/// RedKit annotation clashes across staged mods: {(annotation, symbol): [mod names]}.
/// Python `annotation_clashes` core (symbol ownership, no arrival-order blame).
#[tauri::command]
fn annotation_clashes(shared: State<Shared>) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    // Snapshot under lock; script reads/parses run lock-free.
    let (staging, rows): (std::path::PathBuf, Vec<state::ModRow>) = {
        let g = lock_shared(&shared, "annotation_clashes")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        (m.home.staging.clone(), s.mods_only().into_iter().cloned().collect())
    };
    let mut owners: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for r in rows.iter().filter(|r| r.enabled) {
        let stage = staging.join(&r.id);
        for t in &r.targets {
            if !t.to_lowercase().ends_with(".ws") {
                continue;
            }
            let data = std::fs::read(stage.join(t)).unwrap_or_default();
            if data.is_empty() {
                continue;
            }
            let (lines, _) = script_merge::decode_script(&data);
            for (ann, sym) in script_merge::scan_annotations(&lines) {
                owners.entry(format!("{ann} {sym}")).or_default().push(r.name.clone());
            }
        }
    }
    owners.retain(|_, v| {
        v.sort();
        v.dedup();
        v.len() > 1
    });
    Ok(owners)
}

#[tauri::command]
fn save_resolutions(shared: State<Shared>, key: String, answers: Vec<usize>) -> Result<bool, String> {
    let g = lock_shared(&shared, "save_resolutions")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.save_resolutions(&key, answers);
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn bundle_list(path: String) -> Result<Vec<String>, String> {
    Ok(bundles::bundle_entries(std::path::Path::new(&path))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|e| e.path)
        .collect())
}

#[tauri::command]
fn game_running() -> bool {
    steam::game_running()
}

#[tauri::command]
fn undeploy_removed(shared: State<Shared>, rels: Vec<String>) -> Result<bool, String> {
    let g = lock_shared(&shared, "undeploy_removed")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    deploy::restore_paths(&m.home.game, &m.home.backup, &rels).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
fn write_input_settings(shared: State<Shared>, keybinds: std::collections::BTreeMap<String, Vec<String>>) -> Result<bool, String> {
    let g = lock_shared(&shared, "write_input_settings")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    settings::write_input_settings(&m.settings_dir().join("input.settings"), &keybinds)?;
    Ok(true)
}

#[tauri::command]
fn write_user_settings(shared: State<Shared>, snippets: std::collections::BTreeMap<String, Vec<String>>) -> Result<bool, String> {
    let g = lock_shared(&shared, "write_user_settings")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    settings::write_user_settings(&m.settings_dir().join("user.settings"), &snippets)?;
    Ok(true)
}

#[tauri::command]
fn read_mods_settings(shared: State<Shared>) -> Result<Vec<String>, String> {
    let g = lock_shared(&shared, "read_mods_settings")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    Ok(deploy::read_mods_settings(&m.settings_dir().join("mods.settings")))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateHit {
    pub id: String,
    pub name: String,
    pub local: String,
    pub remote: String,
}

#[tauri::command]
fn check_updates(app: tauri::AppHandle, shared: State<Shared>, api_key: String, ids: Option<Vec<String>>) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("Set a Nexus API key first".into());
    }
    let (mods, game_dir): (Vec<state::ModRow>, std::path::PathBuf) = {
        let g = lock_shared(&shared, "check_updates")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        let v: Vec<state::ModRow> = s.mods_only().into_iter().cloned().collect();
        (v, m.home.game.clone())
    };
    // Scoped re-checks (single mod / selection) only scan the given ids.
    let wanted = ids.unwrap_or_default();
    let scoped = !wanted.is_empty();
    let mods: Vec<state::ModRow> = if scoped {
        mods.into_iter().filter(|m| wanted.contains(&m.id)).collect()
    } else {
        mods
    };
    // Network work happens on a worker thread; the command returns at once and
    // the list refreshes from events (same shape as the download queue).
    std::thread::spawn(move || {
        use tauri::Emitter;
        let total = mods.len();
        let mut hits = vec![];
        for (i, m) in mods.iter().enumerate() {
            let nid = if m.nexus.trim().is_empty() { crate::nexus::extract_nexus_id(&m.name) } else { m.nexus.clone() };
            if nid.is_empty() {
                let _ = app.emit("updates-progress", serde_json::json!({"done": i + 1, "total": total}));
                continue;
            }
            // Recently seen versions skip the network entirely.
            if let Some(remote) = crate::nexus::cached_remote(&nid) {
                if !remote.is_empty() && !m.version.is_empty() && crate::nexus::version_is_newer(&remote, &m.version) {
                    hits.push(UpdateHit { id: m.id.clone(), name: m.name.clone(), local: m.version.clone(), remote });
                }
                let _ = app.emit("updates-progress", serde_json::json!({"done": i + 1, "total": total}));
                continue;
            }
            let v = match crate::nexus::nexus_get(&format!("/games/witcher3/mods/{nid}.json"), &api_key) {
                Ok(v) => v,
                Err(_) => {
                    let _ = app.emit("updates-progress", serde_json::json!({"done": i + 1, "total": total}));
                    continue;
                }
            };
            let remote = v.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if !remote.is_empty() {
                crate::nexus::store_remote(&nid, &remote);
            }
            if !remote.is_empty() && !m.version.is_empty() && crate::nexus::version_is_newer(&remote, &m.version) {
                hits.push(UpdateHit { id: m.id.clone(), name: m.name.clone(), local: m.version.clone(), remote });
                // Prefetch the file target now so the Update click is instant.
                if let Ok(Some(t)) = crate::nexus::resolve_update(&nid, &m.version, &api_key) {
                    crate::nexus::store_target(&nid, &t);
                }
            }
            let _ = app.emit("updates-progress", serde_json::json!({"done": i + 1, "total": total}));
        }
        let _ = app.emit("updates-done", serde_json::json!({ "hits": hits, "ids": wanted, "partial": scoped }));
        crate::nexus::save_version_cache(&game_dir);
    });
    Ok(())
}

/// Update hits derivable from the persisted version cache alone (no
/// network): powers the boot-time banner so cached updates show immediately.
#[tauri::command]
fn cached_updates(shared: State<Shared>) -> Result<Vec<UpdateHit>, String> {
    let g = lock_shared(&shared, "cached_updates")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let s = m.state.lock().map_err(|e| e.to_string())?;
    let mut out = vec![];
    for r in s.mods_only() {
        let nid = if r.nexus.trim().is_empty() { crate::nexus::extract_nexus_id(&r.name) } else { r.nexus.clone() };
        if nid.is_empty() {
            continue;
        }
        if let Some(remote) = crate::nexus::cached_remote(&nid) {
            if !remote.is_empty() && !r.version.is_empty() && crate::nexus::version_is_newer(&remote, &r.version) {
                out.push(UpdateHit { id: r.id.clone(), name: r.name.clone(), local: r.version.clone(), remote });
            }
        }
    }
    Ok(out)
}

/// One-line launch environment report, once per process: AppImage status +
/// the vars that decide what opened children look like. The log file itself
/// is append-only (`yawmm.log` is never truncated), so this marks sessions.
fn log_launch_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let in_appimage =
            std::env::var("APPIMAGE").map(|v| !v.is_empty()).unwrap_or(false)
                || std::env::var("APPDIR").map(|v| !v.is_empty()).unwrap_or(false);
        log_line(
            "rust",
            &format!(
                "launch: appimage={} appdir={:?} desktop={:?} session_type={:?} gdk_backend={:?} gtk_theme={:?} display={:?}/{:?} path={:?} data_dirs={:?} ld_library_path={}",
                if in_appimage { "yes" } else { "no" },
                std::env::var("APPDIR").ok(),
                std::env::var("XDG_CURRENT_DESKTOP").ok(),
                std::env::var("XDG_SESSION_TYPE").ok(),
                std::env::var("GDK_BACKEND").ok(),
                std::env::var("GTK_THEME").ok(),
                std::env::var("DISPLAY").ok(),
                std::env::var("WAYLAND_DISPLAY").ok(),
                std::env::var("PATH").ok(),
                std::env::var("XDG_DATA_DIRS").ok(),
                if std::env::var("LD_LIBRARY_PATH").is_ok() { "set" } else { "unset" },
            ),
        );
    });
}

#[cfg(target_os = "linux")]
fn xdg_output(args: &[&str]) -> Option<String> {
    let tool = host_tool("xdg-mime")?;
    let mut cmd = std::process::Command::new(tool);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    scrub_env(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// The Exec line of a .desktop handler as seen through the scrubbed data
/// dirs: shows exactly which binary xdg-open would launch.
#[cfg(target_os = "linux")]
fn handler_exec(handler: &str) -> Option<String> {
    let appdir = std::env::var("APPDIR").ok();
    let mut dirs = vec![];
    if let Some(dd) = std::env::var_os("XDG_DATA_DIRS") {
        for d in std::env::split_paths(&dd) {
            if !is_sandbox_path(&d.to_string_lossy(), appdir.as_deref()) {
                dirs.push(d);
            }
        }
    } else {
        dirs.push(std::path::PathBuf::from("/usr/local/share"));
        dirs.push(std::path::PathBuf::from("/usr/share"));
    }
    for d in dirs {
        let f = d.join("applications").join(handler);
        if let Ok(text) = std::fs::read_to_string(&f) {
            for line in text.lines() {
                if let Some(exec) = line.strip_prefix("Exec=") {
                    return Some(format!("{} ({})", exec, f.display()));
                }
            }
            return Some(format!("<no Exec> ({})", f.display()));
        }
    }
    None
}
#[cfg(target_os = "linux")]
fn default_handler(target: &str) -> Option<String> {
    let p = std::path::Path::new(target);
    let mime = if p.is_dir() {
        "inode/directory".to_string()
    } else {
        xdg_output(&["query", "filetype", target])?
    };
    xdg_output(&["query", "default", &mime])
}
/// or `$APPDIR/usr/bin`): host tools resolved through them are the mount's
/// broken copies (see tauri#10617), so they must not leak into children.
fn is_sandbox_path(entry: &str, appdir: Option<&str>) -> bool {
    if entry.contains(".mount_") {
        return true;
    }
    if let Some(dir) = appdir {
        if !dir.is_empty() && (entry == dir || entry.starts_with(&format!("{dir}/"))) {
            return true;
        }
    }
    false
}

/// Mount entries removed from one path-list value; order preserved.
fn scrub_entries(val: &std::ffi::OsStr, appdir: Option<&str>) -> Vec<std::path::PathBuf> {
    std::env::split_paths(val)
        .filter(|e| !is_sandbox_path(&e.to_string_lossy(), appdir))
        .collect()
}

/// Host PATH with sandbox entries stripped; `None` when nothing usable left.
fn sanitized_path() -> Option<std::ffi::OsString> {
    let appdir = std::env::var("APPDIR").ok();
    let path = std::env::var_os("PATH")?;
    let kept = scrub_entries(&path, appdir.as_deref());
    if kept.is_empty() {
        return None;
    }
    std::env::join_paths(kept).ok()
}

/// Resolve a tool against the sanitized PATH so the child never picks up the
/// AppImage mount's copy.
fn host_tool(name: &str) -> Option<std::path::PathBuf> {
    let path = sanitized_path()?;
    for entry in std::env::split_paths(&path) {
        let cand = entry.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Strip AppImage mount entries from one env value; `None` means remove it.
fn scrub_value(val: &std::ffi::OsStr, appdir: Option<&str>) -> Option<std::ffi::OsString> {
    let kept = scrub_entries(val, appdir);
    if kept.is_empty() {
        None
    } else {
        std::env::join_paths(kept).ok()
    }
}

/// Give the child a host environment: every inherited var carrying mount
/// entries (PATH-like or scalar, e.g. PYTHONHOME/LD_LIBRARY_PATH) is
/// cleaned, so a launched file manager or terminal never loads the bundle's
/// libs, themes, or Python. Vars without mount entries pass through
/// byte-identical, so the session (display server, D-Bus, desktop hints) is
/// untouched. Returns the names that were changed, for diagnostics.
#[cfg(target_os = "linux")]
fn scrub_env(cmd: &mut std::process::Command) -> Vec<String> {
    let appdir = std::env::var("APPDIR").ok();
    // Snapshot first: `cmd.env_*` only affects the child.
    let vars: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os().collect();
    let mut changed = vec![];
    for (k, v) in &vars {
        let polluted = std::env::split_paths(v)
            .any(|p| is_sandbox_path(&p.to_string_lossy(), appdir.as_deref()));
        if !polluted {
            continue;
        }
        changed.push(k.to_string_lossy().to_string());
        match scrub_value(v, appdir.as_deref()) {
            Some(clean) => {
                cmd.env(k, clean);
            }
            None => {
                cmd.env_remove(k);
            }
        }
    }
    // The bundle forces GTK_THEME=Adwaita:dark (never set in a host session);
    // libadwaita apps fight it and render off-theme, so drop it under AppImage.
    if appdir.is_some() && std::env::var("GTK_THEME").is_ok() {
        changed.push("GTK_THEME".to_string());
        cmd.env_remove("GTK_THEME");
    }
    changed
}

/// Open a folder or URL without blocking: the child is spawned detached with
/// a host-sanitized environment, so the command returns at once. Besides the
/// PATH fix above, the AppImage's bundled lib/theme paths are scrubbed — a
/// host file manager inheriting them loads the wrong libs and renders
/// unstyled.
#[tauri::command]
fn open_path(target: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let tool = host_tool("xdg-open").ok_or("xdg-open not found outside the app sandbox")?;
        log_launch_env();
        log_line(
            "rust",
            &format!(
                "open_path: target={} tool={} desktop={:?} handler={:?}",
                target,
                tool.display(),
                std::env::var("XDG_CURRENT_DESKTOP").ok(),
                default_handler(&target),
            ),
        );
        let mut cmd = std::process::Command::new(tool);
        cmd.arg(&target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        let scrubbed = scrub_env(&mut cmd);
        let handler = default_handler(&target);
        log_line(
            "rust",
            &format!(
                "open_path: scrubbed={:?} exec={:?}",
                scrubbed,
                handler.as_deref().and_then(handler_exec),
            ),
        );
        let mut child = cmd.spawn().map_err(|e| format!("open failed: {e}"))?;
        // xdg-open's diagnostics would otherwise vanish: reap stderr on a
        // thread (fire-and-forget, never blocks this command).
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = String::new();
                if std::io::BufReader::new(stderr).read_to_string(&mut buf).is_ok() {
                    let t = buf.trim().to_string();
                    if !t.is_empty() {
                        log_line("rust", &format!("open_path: xdg-open said: {t}"));
                    }
                }
            });
        }
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &target])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
    }
    Ok(())
}

/// Read-only import preview from a legacy `_ModManager/state.json`
/// (clean-break rule: never writes into `_ModManager`).
#[tauri::command]
fn import_legacy_preview(game_dir: String) -> Result<state::AppState, String> {
    let p = std::path::Path::new(&game_dir).join("_ModManager").join("state.json");
    crate::state::load_state(&p.to_path_buf())
}

fn kind_of(rel: &str) -> Option<&'static str> {
    let low = rel.to_lowercase();
    if low.ends_with(".ws") || low.ends_with(".wss") {
        Some("script")
    } else if low.ends_with(".xml") {
        Some("xml")
    } else if low.ends_with(".csv") {
        Some("csv")
    } else {
        None
    }
}

/// Which game versions a mod's game-file copies were made for
/// (python `made_for`/`compare_made_for`, fingerprints in `version.rs`).
#[tauri::command]
fn made_for(shared: State<Shared>, id: String) -> Result<version::MadeFor, String> {
    // Snapshot under lock; bundle/file scans run lock-free (they can take
    // seconds on big bundles and must not stall other commands).
    let (game, stage, targets): (std::path::PathBuf, std::path::PathBuf, Vec<String>) = {
        let g = lock_shared(&shared, "made_for")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let row = {
            let s = m.state.lock().map_err(|e| e.to_string())?;
            s.get(&id).cloned().ok_or("unknown mod")?
        };
        (m.home.game.clone(), m.home.staging.join(&id), row.targets.clone())
    };
    let mut files: Vec<(String, String, Vec<String>)> = vec![];
    for rel in &targets {
        let Some(kind) = kind_of(rel) else { continue };
        let data = std::fs::read(stage.join(rel)).unwrap_or_default();
        if data.is_empty() {
            continue;
        }
        if let Some(v) = version::file_versions(kind, rel, &data) {
            files.push((kind.to_string(), rel.clone(), v));
        }
    }
    if files.is_empty() {
        return Ok(version::MadeFor { label: String::new(), short: String::new(), status: String::new() });
    }
    let game_of = |kind: &str, rel: &str| -> Vec<String> {
        // Prefer loose game file, else first bundle hit.
        let path = game.join(rel);
        if let Ok(data) = std::fs::read(&path) {
            if let Some(v) = version::file_versions(kind, rel, &data) {
                return v;
            }
        }
        // Scan bundles under content/ for the file (best-effort).
        let content = game.join("content");
        if let Ok(rd) = std::fs::read_dir(&content) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "bundle").unwrap_or(false) {
                    if let Ok(entries) = bundles::bundle_entries(&p) {
                        if entries.iter().any(|en| en.path.eq_ignore_ascii_case(rel)) {
                            if let Ok(data) = bundles::bundle_read(&p, rel) {
                                if let Some(v) = version::file_versions(kind, rel, &data) {
                                    return v;
                                }
                            }
                        }
                    }
                }
            }
        }
        vec![]
    };
    Ok(version::compare_made_for(&files, &game_of))
}

/// Nexus rate-limit quota (python `limits_text`).
#[tauri::command]
fn quota() -> (String, String, i64) {
    let (text, tip) = nexus::limits_text();
    (text, tip, nexus::allowance())
}

/// External Script Merger path check (python `merger_path_check`).
#[tauri::command]
fn merger_check(shared: State<Shared>, exe_path: String) -> Result<merger::MergerReport, String> {
    let g = lock_shared(&shared, "merger_check")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    merger::merger_path_check(&m.home.prefix.to_string_lossy(), &m.home.game.to_string_lossy(), &exe_path)
}

/// Staged file list for one mod (powers per-mod file view + clash details).
#[tauri::command]
fn staged_files(shared: State<Shared>, id: String) -> Result<Vec<String>, String> {
    let g = lock_shared(&shared, "staged_files")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let stage = m.home.staging.join(&id);
    Ok(deploy::mod_files(&stage).into_iter().map(|p| p.to_string_lossy().to_string()).collect())
}

/// Folder-level ownership: a game dir counts as managed when a target equals
/// it or lives under it. Installed rows carry per-file targets
/// (`mods/foo/...`) while imported rows carry the folder itself (`mods/foo`);
/// both must mark the folder owned (original `unmanaged_mods`).
fn owns_folder(owned: &std::collections::HashSet<String>, rel: &str) -> bool {
    let low = rel.to_lowercase();
    let prefix = format!("{low}/");
    owned.contains(&low) || owned.iter().any(|t| t.starts_with(&prefix))
}

/// Script-merger output folders are never unmanaged (original MERGED_NAME /
/// AUTO_MERGE_NAME exclusions).
fn is_merger_output(name: &str) -> bool {
    matches!(name.to_lowercase().as_str(), "mod0000_mergedfiles" | "mod0000_automerged")
}

/// Mod folders sitting in the game that no managed mod owns
/// (python `unmanaged_mods`): `mods/<name>` dirs, not symlinks, not ours.
#[tauri::command]
fn unmanaged_mods(shared: State<Shared>) -> Result<Vec<String>, String> {
    let g = lock_shared(&shared, "unmanaged_mods")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let owned: std::collections::HashSet<String> = {
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.mods_only().iter().flat_map(|r| r.targets.iter().map(|t| t.to_lowercase())).collect()
    };
    let mods_dir = m.home.game.join("mods");
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(&mods_dir) {
        for e in rd.flatten() {
            let ft = e.file_type().map_err(|e| e.to_string())?;
            if !ft.is_dir() || ft.is_symlink() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if is_merger_output(&name) {
                continue;
            }
            let rel = format!("mods/{name}");
            if owns_folder(&owned, &rel) {
                continue;
            }
            out.push(rel);
        }
    }
    out.sort();
    Ok(out)
}

/// Adopt unmanaged game folders as mods (python `import_unmanaged`): moves
/// each folder into staging and creates an enabled row for it.
#[tauri::command]
fn import_unmanaged(shared: State<Shared>, rels: Vec<String>) -> Result<Vec<String>, String> {
    // Snapshot paths + settings first; folder moves happen lock-free.
    let (game, staging, settings_path): (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) = {
        let g = lock_shared(&shared, "import_unmanaged")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        (m.home.game.clone(), m.home.staging.clone(), m.settings_dir().join("mods.settings"))
    };
    // mods.settings priorities/enabled for ordering + initial state
    let settings = std::fs::read_to_string(&settings_path).unwrap_or_default();
    let mut prio: std::collections::HashMap<String, i64> = Default::default();
    let mut en: std::collections::HashMap<String, bool> = Default::default();
    {
        let mut order = 0i64;
        for line in settings.lines() {
            let t = line.trim();
            if t.eq_ignore_ascii_case("[Mods]") {
                continue;
            }
            if t.starts_with('[') {
                break;
            }
            if let Some((k, v)) = t.split_once('=') {
                let folder = k.trim().trim_start_matches("Mod").trim();
                let _ = order;
                // entries look like `Mod0=modName`; priority = file order
                prio.entry(v.trim().to_lowercase()).or_insert(order);
                order += 1;
                let _ = folder;
            }
        }
        // Enabled flags live per-mod-row; default on.
        for v in prio.keys() {
            en.entry(v.clone()).or_insert(true);
        }
    }
    let mut sorted = rels;
    sorted.sort_by_key(|r| (prio.get(&r.split('/').nth(1).unwrap_or("").to_lowercase()).copied().unwrap_or(9999), r.clone()));
    let owned: std::collections::HashSet<String> = {
        let g = lock_shared(&shared, "import_unmanaged")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.mods_only().iter().flat_map(|r| r.targets.iter().map(|t| t.to_lowercase())).collect()
    };
    // (id, folder, enabled) staged lock-free, committed in one lock at the end.
    let mut staged: Vec<(String, String, bool)> = vec![];
    for rel in sorted {
        // Defensive: never adopt a folder a managed mod already owns (stale
        // banner lists must not duplicate a just-installed mod).
        if owns_folder(&owned, &rel) {
            log_line("rust", &format!("import_unmanaged: skipping owned {rel}"));
            continue;
        }
        let src = game.join(&rel);
        if !src.is_dir() {
            continue;
        }
        let folder = src.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let dst = staging.join(&id).join("mods").join(&folder);
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::rename(&src, &dst).map_err(|e| format!("{}: {e}", rel))?;
        staged.push((id, folder.clone(), *en.get(&folder.to_lowercase()).unwrap_or(&true)));
    }
    let mut ids = vec![];
    {
        let g = lock_shared(&shared, "import_unmanaged")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        for (id, folder, enabled) in &staged {
            let stripped = if folder.len() > 3 && folder.to_lowercase().starts_with("mod") {
                folder[3..].to_string()
            } else {
                folder.clone()
            };
            s.mods.push(state::ModRow {
                id: id.clone(), sep: false, name: stripped, enabled: *enabled,
                version: String::new(), nexus: String::new(), archive: String::new(),
                section: String::new(), updated: chrono::Utc::now().timestamp(),
                collapsed: false, targets: vec![format!("mods/{folder}")],
                nexus_cat: String::new(), main_of: String::new(),
            });
            ids.push(id.clone());
        }
        s.priority_ids();
        m.save()?;
    }
    Ok(ids)
}

/// Per-mod overlap report: shared scripts/xmls + beaten-file counts
/// (python `analysis` core: sharing + lost, minus merge-registry detail).
#[tauri::command]
fn analysis_summary(shared: State<Shared>) -> Result<serde_json::Value, String> {
    let g = lock_shared(&shared, "analysis_summary")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let (rows, order) = {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        let order = s.priority_ids();
        let rows: Vec<state::ModRow> = s.mods_only().into_iter().cloned().collect();
        (rows, order)
    };
    let mut idx = std::collections::HashMap::new();
    for (i, id) in order.iter().enumerate() {
        idx.insert(id.clone(), i);
    }
    let mut ranked = rows.clone();
    ranked.sort_by_key(|r| idx.get(&r.id).copied().unwrap_or(usize::MAX));
    // winner per path (top priority wins)
    let mut winner: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for r in ranked.iter().rev() {
        if !r.enabled {
            continue;
        }
        for t in &r.targets {
            winner.insert(t.to_lowercase(), r.id.clone());
        }
    }
    let mut info = serde_json::Map::new();
    for r in &rows {
        let mut shared_scripts = vec![];
        let mut shared_xmls = vec![];
        let mut lost = 0;
        for t in &r.targets {
            let low = t.to_lowercase();
            let holders: Vec<String> = rows
                .iter()
                .filter(|o| o.enabled && o.targets.iter().any(|x| x.to_lowercase() == low))
                .map(|o| o.name.clone())
                .collect();
            if holders.len() > 1 {
                if low.ends_with(".xml") && low.contains("bin/") {
                    shared_xmls.push(serde_json::json!({"file": t, "with": holders}));
                } else {
                    shared_scripts.push(serde_json::json!({"file": t, "with": holders}));
                }
            }
            if r.enabled && winner.get(&low).map(|w| w != &r.id).unwrap_or(false) {
                lost += 1;
            }
        }
        info.insert(r.id.clone(), serde_json::json!({"scripts": shared_scripts, "xmls": shared_xmls, "lost": lost}));
    }
    Ok(serde_json::Value::Object(info))
}

#[tauri::command]
fn queue_enqueue(shared: State<Shared>, url: String) -> Result<String, String> {
    log_line("rust", &format!("queue_enqueue enter url={url}"));
    let id = downloads::enqueue(&url)?;
    dl_save(&shared);
    log_line("rust", &format!("enqueued download id={id} url={url}"));
    Ok(id)
}

#[tauri::command]
fn queue_list() -> Vec<downloads::QueueItem> {
    downloads::items()
}

/// downloads.json path next to the downloads folder (None when no game open).
fn dl_paths(shared: &State<Shared>) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let g = lock_shared(&shared, "dl_paths").ok()?;
    let m = g.as_ref()?;
    let dir = m.home.downloads.clone();
    let _ = std::fs::create_dir_all(&dir);
    let hist = m.home.data.join("downloads.json");
    Some((dir, hist))
}

fn dl_save(shared: &State<Shared>) {
    if let Some((_, hist)) = dl_paths(shared) {
        downloads::save_history(&hist);
    }
}

#[tauri::command]
fn queue_cancel(shared: State<Shared>, id: String) {
    downloads::cancel(&id);
    dl_save(&shared);
}

/// Remove a row from the list, keeping the file (original Remove button).
#[tauri::command]
fn queue_remove(shared: State<Shared>, id: String) {
    downloads::remove(&id);
    dl_save(&shared);
}

/// Move a row's file to the trash, then drop the row (original trash icon).
#[tauri::command]
fn queue_trash(shared: State<Shared>, id: String, dest_dir: String) -> Result<bool, String> {
    let item = downloads::items().into_iter().find(|i| i.id == id);
    if let Some(it) = item {
        if !it.filename.is_empty() && !dest_dir.is_empty() {
            let p = std::path::Path::new(&dest_dir).join(&it.filename);
            if p.is_file() {
                let _ = trash::delete(&p);
            }
            let part = std::path::Path::new(&dest_dir).join(format!("{}.part", it.filename));
            if part.is_file() {
                let _ = trash::delete(&part);
            }
        }
    }
    downloads::remove(&id);
    dl_save(&shared);
    Ok(true)
}

#[tauri::command]
fn queue_pause(shared: State<Shared>, id: String, paused: bool) {
    downloads::set_paused(&id, paused);
    dl_save(&shared);
}

/// Load download history from last time (call after open_manager).
#[tauri::command]
fn downloads_history(shared: State<Shared>) -> Vec<downloads::QueueItem> {
    if let Some((dir, hist)) = dl_paths(&shared) {
        downloads::load_history(&hist, &dir);
    }
    downloads::items()
}

/// Update file already sitting in the downloads dir: exact filename first,
/// then the finished queue row for this Nexus file (covers renames), then any
/// archive in the folder stamped with the same mod id + version (covers
/// browser downloads saved under a different name). Install straight from it
/// instead of redownloading.
fn local_download_file(
    hist: Option<&std::path::PathBuf>,
    nexus: &str,
    target: &crate::nexus::UpdateTarget,
) -> Option<String> {
    let dir = hist?.parent()?;
    let hit = |name: &str| -> Option<String> {
        if name.is_empty() {
            return None;
        }
        let p = dir.join(name);
        p.is_file().then(|| p.to_string_lossy().to_string())
    };
    // 1. Exact filename from the Nexus file entry.
    if let Some(p) = hit(&target.file_name) {
        return Some(p);
    }
    // 2. Whatever finished download row matches this mod + file.
    if let Some(name) = crate::downloads::finished_filename(nexus, &target.file_id) {
        if let Some(p) = hit(&name) {
            return Some(p);
        }
    }
    // 3. Any archive in the folder stamped for this mod + version.
    let want_ver = crate::nexus::version_tuple(&target.version);
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let low = name.to_lowercase();
            if !crate::archive::ARCHIVE_EXTS.iter().any(|ext| low.ends_with(ext)) {
                continue;
            }
            let (_, ver, nid) = crate::nexus::parse_archive_name(&name);
            if nid == nexus && !ver.is_empty() && crate::nexus::version_tuple(&ver) == want_ver {
                if let Some(p) = hit(&name) {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Build the `update-resolved` payload for a target: a local file on disk
/// wins, then premium direct-enqueue, else a bare file reference for the
/// browser flow. Emits the event; never touches the network.
fn emit_update_target(
    app: &tauri::AppHandle,
    id: &str,
    prem: bool,
    nexus: &str,
    t: &crate::nexus::UpdateTarget,
    hist: Option<&std::path::PathBuf>,
) {
    use tauri::Emitter;
    if let Some(local) = local_download_file(hist, nexus, t) {
        log_line("rust", &format!("update_mod: using local {} for {nexus}", t.file_name));
        let _ = app.emit(
            "update-resolved",
            serde_json::json!({ "id": id, "premium": prem, "row_id": "", "version": t.version, "file_id": t.file_id, "local_path": local }),
        );
        return;
    }
    if prem {
        let row_id = downloads::enqueue_resolved(nexus, t);
        if let Some(h) = hist {
            downloads::save_history(h);
        }
        log_line("rust", &format!("update_mod: queued {nexus} -> {} ({})", t.file_id, t.version));
        let _ = app.emit(
            "update-resolved",
            serde_json::json!({ "id": id, "premium": prem, "row_id": row_id, "version": t.version, "file_id": t.file_id }),
        );
    } else {
        let _ = app.emit(
            "update-resolved",
            serde_json::json!({ "id": id, "premium": prem, "row_id": "", "version": t.version, "file_id": t.file_id }),
        );
    }
}

/// Resolve one installed mod's update without ever blocking the caller:
/// the command snapshots state, returns at once, and the result arrives as
/// `update-resolved` (`{ id, premium, row_id?, file_id?, version?, local_path? }`,
/// `row_id == ""` when current or superseded by `local_path`; `{ id, error }`
/// on failure). Premium is resolved in-thread when unknown, so Update clicks
/// never await network. The finished download flows through the normal
/// offer-install path (labeled Update via version verdict).
#[tauri::command]
fn update_mod(app: tauri::AppHandle, shared: State<Shared>, id: String, api_key: String, premium: Option<bool>) -> Result<bool, String> {
    // Snapshot everything the worker needs; only owned data crosses threads.
    let (nexus, version, hist): (String, String, Option<std::path::PathBuf>) = {
        let g = lock_shared(&shared, "update_mod")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        let row = s.get(&id).cloned().ok_or("unknown mod")?;
        let nid = if row.nexus.trim().is_empty() {
            crate::nexus::extract_nexus_id(&row.name)
        } else {
            row.nexus.clone()
        };
        if nid.is_empty() {
            return Err("no Nexus ID on this mod — set one in Edit…".into());
        }
        let dir = m.home.downloads.clone();
        let _ = std::fs::create_dir_all(&dir);
        let hist = m.home.data.join("downloads.json");
        (nid, row.version.clone(), Some(hist))
    };
    log_line("rust", &format!("update_mod: resolving {nexus} (installed {version})"));
    // Zero-network fast path: cached target + known premium.
    if let Some(prem) = premium {
        if let Some(t) = crate::nexus::cached_target(&nexus) {
            if crate::nexus::version_is_newer(&t.version, &version) {
                emit_update_target(&app, &id, prem, &nexus, &t, hist.as_ref());
                return Ok(true);
            }
        }
    }
    std::thread::spawn(move || {
        let prem = premium.unwrap_or_else(|| crate::nexus::is_premium(&api_key));
        // Cached target avoids the files.json fetch; otherwise resolve live.
        let resolved = match crate::nexus::cached_target(&nexus) {
            Some(t) if crate::nexus::version_is_newer(&t.version, &version) => Ok(Some(t)),
            _ => crate::nexus::resolve_update(&nexus, &version, &api_key),
        };
        match resolved {
            Err(e) => {
                use tauri::Emitter;
                let _ = app.emit("update-resolved", serde_json::json!({ "id": id, "premium": prem, "error": e }));
            }
            Ok(None) => {
                use tauri::Emitter;
                let _ = app.emit("update-resolved", serde_json::json!({ "id": id, "premium": prem, "row_id": "", "version": version }));
            }
            Ok(Some(t)) => {
                crate::nexus::store_target(&nexus, &t);
                emit_update_target(&app, &id, prem, &nexus, &t, hist.as_ref());
            }
        }
    });
    Ok(true)
}

/// Start (or resume) a queued download on a worker thread and return
/// immediately — the UI never blocks on network (original NexusDownload
/// QThread; progress/completion arrive as events).
#[tauri::command]
fn queue_start(app: tauri::AppHandle, shared: State<Shared>, id: String, dest_dir: String, api_key: String) -> Result<bool, String> {
    log_line("rust", &format!("queue_start enter id={id}"));
    if !downloads::try_begin(&id) {
        log_line("rust", &format!("queue_start id={id}: already running"));
        return Ok(false); // already running
    }
    dl_save(&shared);
    let hist = dl_paths(&shared).map(|(_, h)| h);
    std::thread::spawn(move || {
        use tauri::Emitter;
        let aid = id.clone();
        // 1. metadata (original info.emit); row shows "Asking Nexus…" meanwhile.
        // Rows enqueued pre-resolved (one-click updates) already carry it.
        let needs_meta = downloads::items()
            .into_iter()
            .find(|i| i.id == id)
            .map(|i| i.filename.is_empty())
            .unwrap_or(false);
        if needs_meta {
            log_line("rust", &format!("worker {aid}: fetching meta"));
            if let Err(e) = downloads::fetch_meta(&id, &api_key) {
                downloads::fail(&id, &e);
                let _ = app.emit("download-meta", serde_json::json!({"id": aid}));
                let _ = app.emit("download-done", serde_json::json!({"id": aid, "path": "", "bytes": 0, "error": e}));
                if let Some(h) = hist {
                    downloads::save_history(&h);
                }
                log_line("rust", &format!("download meta failed id={aid}: {e}"));
                return;
            }
            let _ = app.emit("download-meta", serde_json::json!({"id": aid}));
        }
        let filename = downloads::items().into_iter().find(|i| i.id == id).map(|i| i.filename).unwrap_or_default();
        log_line("rust", &format!("worker {aid}: meta ok file={filename}"));
        let dest = std::path::Path::new(&dest_dir).join(&filename);
        let target = dest.clone();
        let aid2 = id.clone();
        let res = downloads::pump_file(
            &id,
            &dest,
            &api_key,
            &|done, total, speed| {
                let _ = app.emit("download-progress", serde_json::json!({"id": aid2, "done": done, "total": total, "speed": speed}));
            },
        );
        if let Some(h) = hist {
            downloads::save_history(&h);
        }
        match res {
            Ok(n) => {
                let _ = app.emit("download-done", serde_json::json!({"id": id, "path": target.to_string_lossy(), "bytes": n, "error": ""}));
                log_line("rust", &format!("download done id={id} path={}", target.to_string_lossy()));
            }
            Err(e) => {
                let _ = app.emit("download-done", serde_json::json!({"id": id, "path": "", "bytes": 0, "error": e}));
                log_line("rust", &format!("download failed id={id}: {e}"));
            }
        }
    });
    Ok(true)
}

/// Absolute downloads-dir path (platform data dir).
#[tauri::command]
fn downloads_dir_path(shared: State<Shared>) -> Result<String, String> {
    let g = lock_shared(&shared, "downloads_dir_path")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let d = m.home.downloads.clone();
    let _ = std::fs::create_dir_all(&d);
    Ok(d.to_string_lossy().to_string())
}

/// Absolute settings-dir path for the Open menu.
#[tauri::command]
fn settings_dir_path(shared: State<Shared>) -> Result<String, String> {
    let g = lock_shared(&shared, "settings_dir_path")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    Ok(m.settings_dir().to_string_lossy().to_string())
}

/// Move a mod under a section separator (or to the unsectioned end).
#[tauri::command]
fn move_to_section(shared: State<Shared>, id: String, sep_id: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "move_to_section")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        let at = s.mods.iter().position(|r| r.id == id && !r.sep);
        let Some(at) = at else { return Ok(false) };
        let row = s.mods.remove(at);
        if sep_id.is_empty() {
            s.mods.push(row);
        } else {
            let dest = s.mods.iter().position(|r| r.id == sep_id && r.sep);
            match dest {
                Some(d) => {
                    let mut ins = d + 1;
                    while ins < s.mods.len() && !s.mods[ins].sep {
                        ins += 1;
                    }
                    s.mods.insert(ins, row);
                }
                None => s.mods.push(row),
            }
        }
        // Keep list order and priority in sync (same rebuild the other
        // row-moving writers do).
        s.priority_ids();
    }
    m.save()?;
    Ok(true)
}

/// Drag-drop reorder: move a mod before another row (empty `before` = end of
/// list). List order and priority stay in sync; section membership follows
/// position. Redeploys afterwards so file winners follow the new order.
#[tauri::command]
fn move_mod(shared: State<Shared>, id: String, before: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "move_mod")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        s.move_before(&id, if before.is_empty() { None } else { Some(before.as_str()) });
    }
    m.save()?;
    log_line("rust", &format!("move_mod: {id} before {before:?}"));
    Ok(true)
}

/// Open (or focus) a tool window: install | setup | resolver.
/// The original is a multi-window app (Install/Settings/Script-decisions
/// dialogs); each tool is a Svelte route rendered in its own native window.
#[tauri::command]
fn open_tool_window(app: tauri::AppHandle, kind: String, query: String, path: String) -> Result<(), String> {
    use tauri::Manager;
    log_line("rust", &format!("open_tool_window called kind={kind:?} query={query:?} path={path:?}"));
    let (title, w, h) = match kind.as_str() {
        "install" => ("Install mod", 820.0, 660.0),
        "edit" => ("Edit mod", 860.0, 520.0),
        "resolver" => ("Script decisions", 1150.0, 760.0),
        _ => {
            log_line("rust", &format!("open_tool_window: unknown kind={kind:?}"));
            return Err("unknown window".into());
        }
    };
    // Window labels are stable ids; routes are what SvelteKit builds.
    // Note: settings lives in the main sidebar now, not a tool window.
    let route = match kind.as_str() {
        "resolver" => "merges",
        _ => kind.as_str(),
    };
    if let Some(win) = app.get_webview_window(&kind) {
        log_line("rust", &format!("open_tool_window: window {kind:?} already exists, focusing"));
        win.set_focus().map_err(|e| e.to_string())?;
        if !path.is_empty() {
            use tauri::Emitter;
            let _ = win.emit(format!("tool-open-{kind}").as_str(), serde_json::json!({"path": path, "query": query}));
        }
        return Ok(());
    }
    #[allow(unused_mut)]
    let mut url = if query.is_empty() { format!("{route}.html") } else { format!("{route}.html?{query}") };
    // Dev server routes by path, the bundled app by file.
    #[cfg(dev)]
    {
        url = if query.is_empty() { format!("http://localhost:1420/{route}") } else { format!("http://localhost:1420/{route}?{query}") };
    }
    log_line("rust", &format!("open_tool_window: building window label={kind:?} title={title:?} route={route:?} url={url:?}"));
    // Release: load the ROUTE path, not the file. Tauri serves settings.html
    // for it via its asset fallback chain, and SvelteKit then sees the same
    // pathname as in dev (/settings). Loading settings.html directly boots
    // SvelteKit with pathname /settings.html, which matches no route, so the
    // client renders its 404 page in release builds only.
    #[cfg(not(dev))]
    let webview_url = {
        let route_url = if query.is_empty() { route.to_string() } else { format!("{route}?{query}") };
        tauri::WebviewUrl::App(route_url.into())
    };
    #[cfg(dev)]
    let webview_url = tauri::WebviewUrl::External(url.parse().expect("valid dev url"));
    let _win = tauri::WebviewWindowBuilder::new(&app, kind.clone(), webview_url)
        .title(title)
        .inner_size(w, h)
        .center()
        .focused(true)
        .build()
        .map_err(|e| {
            log_line("rust", &format!("open_tool_window: build failed: {e}"));
            e.to_string()
        })?;
    log_line("rust", &format!("open_tool_window: window {kind:?} created"));
    Ok(())
}

/// Write fixed merger paths back into its config (Setup check → Apply).
#[tauri::command]
fn merger_apply(config_path: String, fixes: std::collections::BTreeMap<String, String>) -> Result<bool, String> {
    let mut text = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    for (key, want) in &fixes {
        // .NET appSettings first: <add key="K" value="V" />
        let pat = format!(r#"(?i)(<add\s+key="{}"\s+value=")[^"]*(")"#, regex::escape(key));
        let re = regex::Regex::new(&pat).unwrap();
        let new_text = re.replace_all(&text, format!("$1{want}$2")).to_string();
        if new_text != text {
            text = new_text;
            continue;
        }
        // <setting name="K">…<value>V</value>
        let pat2 = format!(r#"(?is)(<setting\s+name="{}"[^>]*>.*?<value>).*?(</value>)"#, regex::escape(key));
        let re2 = regex::Regex::new(&pat2).unwrap();
        let new_text2 = re2.replace_all(&text, format!("$1{want}$2")).to_string();
        if new_text2 != text {
            text = new_text2;
            continue;
        }
        // INI/key=value fallback: replace the line, or append it
        let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        let mut done = false;
        for l in lines.iter_mut() {
            if let Some((k, _)) = l.split_once('=').or_else(|| l.split_once(':')) {
                if k.trim().trim_matches('"') == key {
                    *l = format!("{key}={want}");
                    done = true;
                }
            }
        }
        if !done {
            lines.push(format!("{key}={want}"));
        }
        text = lines.join("\n") + "\n";
    }
    // backup like the original never overwrites blindly
    let bak = format!("{config_path}.original");
    if std::path::Path::new(&bak).exists() == false {
        std::fs::copy(&config_path, &bak).map_err(|e| e.to_string())?;
    }
    std::fs::write(&config_path, text).map_err(|e| e.to_string())?;
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MergeInputs {
    pub rel: String,
    pub kind: String,
    pub base: String,
    pub base_encoding: String,
    pub versions: Vec<MergeVersion>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MergeVersion {
    pub label: String,
    pub mod_id: String,
    pub text: String,
}

/// Real merge inputs for one shared file: vanilla base (loose game file,
/// else first bundle hit) + every enabled staged copy, top priority first.
#[tauri::command]
fn merge_inputs(shared: State<Shared>, rel: String) -> Result<MergeInputs, String> {
    let kind = if rel.to_lowercase().ends_with(".xml") { "xml" } else { "script" }.to_string();
    // Snapshot under lock; bundle scans run lock-free (see made_for).
    let (game, staging, rows, order): (std::path::PathBuf, std::path::PathBuf, Vec<state::ModRow>, Vec<String>) = {
        let g = lock_shared(&shared, "merge_inputs")?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        let order = s.priority_ids();
        (m.home.game.clone(), m.home.staging.clone(), s.mods_only().into_iter().cloned().collect::<Vec<_>>(), order)
    };
    let mut idx = std::collections::HashMap::new();
    for (i, id) in order.iter().enumerate() {
        idx.insert(id.clone(), i);
    }
    let mut holders: Vec<&state::ModRow> = rows
        .iter()
        .filter(|r| r.enabled && r.targets.iter().any(|t| t.to_lowercase() == rel.to_lowercase()))
        .collect();
    holders.sort_by_key(|r| idx.get(&r.id).copied().unwrap_or(usize::MAX));
    // base: loose file, else bundle
    let mut base_bytes: Option<Vec<u8>> = std::fs::read(game.join(&rel)).ok();
    if base_bytes.is_none() {
        let content = game.join("content");
        if let Ok(rd) = std::fs::read_dir(&content) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "bundle").unwrap_or(false) {
                    if let Ok(entries) = bundles::bundle_entries(&p) {
                        if entries.iter().any(|en| en.path.eq_ignore_ascii_case(&rel)) {
                            base_bytes = bundles::bundle_read(&p, &rel).ok();
                            if base_bytes.is_some() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    let (base_lines, enc) = base_bytes
        .as_deref()
        .map(|b| {
            let (l, e) = script_merge::decode_script(b);
            let s = match e {
                script_merge::ScriptEncoding::Utf16Le => "utf16",
                _ => "utf8",
            };
            (l, s.to_string())
        })
        .unwrap_or((vec![], "utf8".into()));
    let mut versions = vec![];
    for r in holders {
        let data = std::fs::read(staging.join(&r.id).join(&rel)).unwrap_or_default();
        let (lines, _) = script_merge::decode_script(&data);
        versions.push(MergeVersion { label: r.name.clone(), mod_id: r.id.clone(), text: lines.join("\n") });
    }
    Ok(MergeInputs { rel, kind, base: base_lines.join("\n"), base_encoding: enc, versions })
}

/// Keep a resolved merge: stored under the rel, applied on top of deploys.
#[tauri::command]
fn save_merge(shared: State<Shared>, rel: String, text: String, answers: Vec<usize>) -> Result<bool, String> {
    let g = lock_shared(&shared, "save_merge")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        s.merge_kept.insert(rel.clone(), text);
        s.save_resolutions(&rel, answers);
    }
    m.save()?;
    Ok(true)
}

/// Staged dir of one mod (context-menu Open folder).
#[tauri::command]
fn mod_dir(shared: State<Shared>, id: String) -> Result<String, String> {
    let g = lock_shared(&shared, "mod_dir")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    Ok(m.home.staging.join(&id).to_string_lossy().to_string())
}

/// Remove a section; its mods move out to the unsectioned end of the list in
/// order, priorities unchanged (python `remove_section`).
#[tauri::command]
fn remove_section_cmd(shared: State<Shared>, sep_id: String) -> Result<bool, String> {
    let g = lock_shared(&shared, "remove_section_cmd")?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        let at = s.mods.iter().position(|r| r.id == sep_id && r.sep);
        let Some(at) = at else { return Ok(false) };
        let mut end = at + 1;
        while end < s.mods.len() && !s.mods[end].sep {
            end += 1;
        }
        let members: Vec<state::ModRow> = s.mods.drain(at..end).filter(|r| !r.sep).collect();
        s.mods.extend(members);
    }
    m.save()?;
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanRoot {
    pub prefix: String,
    pub kind: String,
    pub folder: String,
    pub files: usize,
}

/// Top-level roots of an archive for the Install dialog's Archive-contents
/// table: (source prefix, kind, folder name, file count).
#[tauri::command]
fn preview_roots(path: String) -> Result<Vec<PlanRoot>, String> {
    use std::path::Path;
    let archive = Path::new(&path);
    let tmp = std::env::temp_dir().join(format!("yawmm-roots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let plan = (|| {
        archive::extract_archive(archive, &tmp).map_err(|e| e.to_string())?;
        Ok::<_, String>(install::analyze(&tmp))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    let plan = plan?;
    // group staged targets by their top two segments (kind + folder)
    let mut groups: std::collections::BTreeMap<(String, String), (usize, String)> = Default::default();
    for (_src, rel) in &plan.moves {
        let (first, folder, _rest) = install::split_rel(rel);
        if ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
            let kind = match first.as_str() {
                "dlc" => "DLC",
                "bin" => "Bin",
                "content" => "Content",
                _ => "Mod",
            }
            .to_string();
            let e = groups.entry((kind, folder)).or_insert((0, String::new()));
            e.0 += 1;
            e.1 = install::src_prefix(rel);
        } else {
            groups.entry(("Mod".to_string(), String::new())).or_insert((0, String::new())).0 += 1;
        }
    }
    Ok(groups
        .into_iter()
        .map(|((kind, folder), (files, prefix))| PlanRoot { prefix, kind, folder, files })
        .collect())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallPreview {
    pub roots: Vec<PlanRoot>,
    pub moves: Vec<(String, String)>,
}

/// Archive-contents table + target list from ONE extraction (the Install
/// window used to extract the archive twice via preview_roots +
/// preview_archive before showing anything).
#[tauri::command]
fn install_preview(path: String) -> Result<InstallPreview, String> {
    use std::path::Path;
    let archive = Path::new(&path);
    let tmp = std::env::temp_dir().join(format!("yawmm-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let plan = (|| {
        archive::extract_archive(archive, &tmp).map_err(|e| e.to_string())?;
        Ok::<_, String>(install::analyze(&tmp))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    let plan = plan?;
    // group staged targets by their top two segments (kind + folder)
    let mut groups: std::collections::BTreeMap<(String, String), (usize, String)> = Default::default();
    for (_src, rel) in &plan.moves {
        let (first, folder, _rest) = install::split_rel(rel);
        if ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
            let kind = match first.as_str() {
                "dlc" => "DLC",
                "bin" => "Bin",
                "content" => "Content",
                _ => "Mod",
            }
            .to_string();
            let e = groups.entry((kind, folder)).or_insert((0, String::new()));
            e.0 += 1;
            e.1 = install::src_prefix(rel);
        } else {
            groups.entry(("Mod".to_string(), String::new())).or_insert((0, String::new())).0 += 1;
        }
    }
    let roots = groups
        .into_iter()
        .map(|((kind, folder), (files, prefix))| PlanRoot { prefix, kind, folder, files })
        .collect();
    Ok(InstallPreview { roots, moves: plan.moves })
}

/// Install with per-root kind/folder mapping from the dialog's table.
/// When `replace_ids` is non-empty the replaced rows are removed and the new
/// row takes the first replaced row's list position (and section, unless the
/// dialog picked one), so updates/reinstalls keep their load-order spot
/// instead of dropping to the end of the list.
#[tauri::command]
fn install_roots(
    shared: State<Shared>,
    path: String,
    name: String,
    version: String,
    nexus_id: String,
    section: String,
    roots: Vec<PlanRoot>,
    replace_ids: Vec<String>,
) -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("yawmm-install-{}", uuid::Uuid::new_v4().simple()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let res: Result<String, String> = (|| {
        // Manually picked archives live wherever the user left them: move
        // the file into our downloads first so the row's recorded path stays
        // valid for reinstalls and updates.
        let dl: std::path::PathBuf = {
            let g = lock_shared(&shared, "install_roots")?;
            let m = g.as_ref().ok_or("open a game folder first")?;
            m.home.downloads.clone()
        };
        let path = downloads::ensure_in_downloads(std::path::Path::new(&path), &dl)?
            .to_string_lossy()
            .to_string();
        log_line("rust", &format!("install_roots: extracting {path}"));
        archive::extract_archive(std::path::Path::new(&path), &tmp).map_err(|e| {
            log_line("rust", &format!("install_roots: extract_archive failed: {}", e));
            e.to_string()
        })?;
        log_line("rust", &format!("install_roots: extraction complete tmp={}", tmp.display()));
        let plan = install::analyze(&tmp);
        log_line("rust", &format!("install_roots: plan has {} moves", plan.moves.len()));
        // Staging dir first; the lock is only taken to commit the row, so a
        // big archive extraction never blocks other commands.
        let staging: std::path::PathBuf = {
            let g = lock_shared(&shared, "install_roots")?;
            let m = g.as_ref().ok_or("open a game folder first")?;
            m.home.staging.clone()
        };
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let stage = staging.join(&id);
        // Remap each planned move through the dialog's root table (pure fn,
        // unit-tested in install.rs).
        let choices: Vec<install::RootChoice> = roots
            .iter()
            .map(|r| install::RootChoice {
                kind: r.kind.clone(),
                folder: r.folder.clone(),
                prefix: r.prefix.clone(),
                files: r.files,
            })
            .collect();
        let remapped: Vec<(String, String)> = plan
            .moves
            .iter()
            .map(|(src, rel)| (src.clone(), install::remap_target(rel, &choices, &name)))
            .collect();
        let sub = install::InstallPlan { moves: remapped, docs: plan.docs.clone() };
        let folder = state::ensure_mod_prefix(&name);
        crate::log_line("rust", &format!("install_roots: building staging for '{}' with {} moves into '{}'", name, sub.moves.len(), stage.display()));
        for (src, rel) in &sub.moves {
            crate::log_line("rust", &format!("install_roots: planned move src='{}' -> rel='{}'", src, rel));
        }
        let (targets, _docs) = install::build_staging(&sub, &stage, &folder)?;
        // Replacement installs clean the old staged folders lock-free first
        // (same pattern as remove_mods), then swap rows atomically below.
        for rid in &replace_ids {
            let dir = staging.join(rid);
            if dir.is_dir() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        {
            let g = lock_shared(&shared, "install_roots")?;
            let m = g.as_ref().ok_or("open a game folder first")?;
            // NOTE: the state guard must drop before save(): save() locks the
            // same non-reentrant mutex and would deadlock this thread forever.
            {
                let mut s = m.state.lock().map_err(|e| e.to_string())?;
                // Replacement installs (update / reinstall / replace) keep
                // the first replaced row's position so load order survives.
                let at = replace_ids.iter()
                    .filter_map(|rid| s.mods.iter().position(|r| r.id == *rid && !r.sep))
                    .min();
                let keep_section = at
                    .and_then(|i| s.mods.get(i))
                    .map(|r| r.section.clone())
                    .unwrap_or_default();
                if !replace_ids.is_empty() {
                    s.remove_rows(&replace_ids);
                }
                let row_section = if section.is_empty() { keep_section } else { section.clone() };
                let row = state::ModRow {
                    id: id.clone(), sep: false, name: name.clone(), enabled: true,
                    version: version.clone(), nexus: nexus_id.clone(), archive: path.clone(),
                    section: row_section, updated: chrono::Utc::now().timestamp(),
                    collapsed: false, targets: targets.clone(), nexus_cat: String::new(), main_of: String::new(),
                };
                match at {
                    Some(i) => {
                        let len = s.mods.len();
                        s.mods.insert(i.min(len), row);
                    }
                    None => s.mods.push(row),
                }
                s.priority_ids();
            }
            m.save()?;
        }
        log_line("rust", &format!("install_roots: committed {name} ({})", targets.len()));
        Ok(id)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        // Single instance: second launches (e.g. nxm:// clicks) focus this
        // window and forward argv as deep-link events — replaces the
        // hand-rolled Unix-socket handoff in the original.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            use tauri::Manager;
            let _ = app.get_webview_window("main").map(|w| {
                let _ = w.set_focus();
            });
            log_line("rust", &format!("single-instance argv: {argv:?}"));
        }))
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            log_line("rust", &format!("YAWMM v{} starting, worker-thread downloads", env!("CARGO_PKG_VERSION")));
            // Forward OS deep-link opens (nxm://…) to the frontend as events.
            // The .desktop MimeType registration comes from the
            // `security.deepLinkProtocols` entry in tauri.conf.json — no
            // hand-edited mimeapps.list code needed.
            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                // In dev mode the scheme isn't installed yet, so register it.
                #[cfg(any(windows, target_os = "linux"))]
                if let Err(e) = app.deep_link().register_all() {
                    log_line("rust", &format!("deep_link register_all failed: {e}"));
                }
                app.deep_link().on_open_url(move |event| {
                    log_line("rust", &format!("deep_link event urls={:?}", event.urls().iter().map(|u| u.to_string()).collect::<Vec<_>>()));
                });
            }
            Ok(())
        })
        .manage(Shared::new(None))
        .invoke_handler(tauri::generate_handler![
            open_manager,
            detect_game,
            default_prefix,
            is_game_dir,
            list_mods,
            set_enabled,
            save_profile,
            apply_profile,
            delete_profile,
            rename_profile,
            set_priority,
            rename_mod,
            remove_mods,
            deploy,
            preview_archive,
            list_archive,
            install_archive,
            add_separator,
            edit_mod,
            find_collisions,
            clashes,
            merge_scripts,
            merge_xml,
            merge_check,
            annotation_clashes,
            save_resolutions,
            scan_keybinds,
            scan_snippets,
            nexus_status,
            nexus_premium,
            parse_archive_name,
            parse_nxm,
            download_nxm,
            bundle_list,
            game_running,
            undeploy_removed,
            write_input_settings,
            write_user_settings,
            read_mods_settings,
            check_updates,
            cached_updates,
            import_legacy_preview,
            made_for,
            quota,
            merger_check,
            staged_files,
            unmanaged_mods,
            import_unmanaged,
            analysis_summary,
            queue_enqueue,
            queue_list,
            queue_cancel,
            queue_remove,
            queue_trash,
            queue_pause,
            queue_start,
            update_mod,
            downloads_history,
            downloads_dir_path,
            settings_dir_path,
            storage_report,
            open_tool_window,
            merger_apply,
            merge_inputs,
            save_merge,
            mod_dir,
            open_path,
            remove_section_cmd,
            move_to_section,
            move_mod,
            preview_roots,
            install_preview,
            install_roots,
            install::remap_roots,
            frontend_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_paths_detected() {        assert!(is_sandbox_path("/tmp/.mount_abc123/usr/bin", None));
        assert!(is_sandbox_path("/tmp/.mount_abc123/usr/bin", Some("/tmp/.mount_abc123")));
        assert!(is_sandbox_path("/opt/app/usr/bin", Some("/opt/app")));
        assert!(is_sandbox_path("/opt/app", Some("/opt/app")));
        assert!(!is_sandbox_path("/usr/bin", Some("/opt/app")));
        assert!(!is_sandbox_path("/usr/bin", None));
        assert!(!is_sandbox_path("/home/u/.local/bin", None));
        // A normal dir that merely shares a prefix is kept.
        assert!(!is_sandbox_path("/opt/application/bin", Some("/opt/app")));
    }

    #[test]
    fn scrub_entries_keeps_host_order() {
        use std::ffi::OsStr;
        let v = scrub_entries(
            OsStr::new("/tmp/.mount_x/usr/share:/usr/share:/home/u/.local/share:/opt/app/share"),
            Some("/opt/app"),
        );
        let s: Vec<_> = v.iter().map(|p| p.to_string_lossy().to_string()).collect();
        assert_eq!(s, vec!["/usr/share", "/home/u/.local/share"]);
    }

    #[test]
    fn scrub_value_removes_mount_scalars() {
        use std::ffi::OsStr;
        // Pure mount value -> remove the var entirely.
        assert_eq!(scrub_value(OsStr::new("/tmp/.mount_x/usr"), Some("/opt/app")), None);
        // Mixed list -> mount entries dropped, rest intact.
        assert_eq!(
            scrub_value(
                OsStr::new("/tmp/.mount_x/usr/share/pyshared/:/usr/lib/python3"),
                Some("/opt/app")
            ),
            Some(std::ffi::OsString::from("/usr/lib/python3"))
        );
        // Clean values pass through unchanged.
        assert_eq!(
            scrub_value(OsStr::new("unix:path=/run/user/1000/bus"), None),
            Some(std::ffi::OsString::from("unix:path=/run/user/1000/bus"))
        );
    }
}
