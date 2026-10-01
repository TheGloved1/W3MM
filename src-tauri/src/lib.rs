//! W3LMN backend: Tauri commands over the ported ModManager core.
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
use tauri::State;

type Shared = Mutex<Option<Manager>>;

#[tauri::command]
fn open_manager(shared: State<Shared>, game_dir: String, prefix: String) -> Result<bool, String> {
    let m = Manager::open(&game_dir, &prefix)?;
    *shared.lock().map_err(|e| e.to_string())? = Some(m);
    Ok(true)
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
        let g = shared.lock().map_err(|e| e.to_string())?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.clone()
    };
    Ok(cloned)
}

#[tauri::command]
fn set_enabled(shared: State<Shared>, ids: Vec<String>, on: bool) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.set_enabled(&ids, on);
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn set_priority(shared: State<Shared>, id: String, number: usize) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.rename_mod(&id, &state::ensure_mod_prefix(&name));
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn remove_mods(shared: State<Shared>, ids: Vec<String>) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    // Drop staged folders first so disk matches state.
    for id in &ids {
        let dir = m.home.staging.join(id);
        if dir.is_dir() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    m.state.lock().map_err(|e| e.to_string())?.remove_rows(&ids);
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn deploy(shared: State<Shared>) -> Result<Vec<String>, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    if steam::game_running() {
        return Err("Close the game before deploying".to_string());
    }
    let (enabled, order): (Vec<state::ModRow>, Vec<String>) = {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        let order = s.priority_ids();
        let mods = s.mods_only().into_iter().cloned().collect::<Vec<_>>();
        (mods, order)
    };
    let mut order_idx = std::collections::HashMap::new();
    for (i, id) in order.iter().enumerate() {
        order_idx.insert(id.clone(), i);
    }
    let mut ranked: Vec<state::ModRow> = enabled.into_iter().filter(|r| r.enabled).collect();
    ranked.sort_by_key(|r| order_idx.get(&r.id).copied().unwrap_or(usize::MAX));

    let mut all_written: Vec<String> = vec![];
    let mut menu_xmls: Vec<String> = vec![];
    for r in &ranked {
        let stage = m.home.staging.join(&r.id);
        if !stage.is_dir() {
            continue;
        }
        let rels = deploy::mod_files(&stage);
        let written = deploy::deploy_mod(&m.home.game, &stage, &rels, &m.home.backup)
            .map_err(|e| format!("{}: {e}", r.name))?;
        for w in &written {
            if w.to_lowercase().ends_with(".xml") && w.to_lowercase().contains("bin/") {
                menu_xmls.push(w.clone());
            }
        }
        all_written.extend(written);
    }
    // Anything previously deployed but no longer wanted (disabled/removed
    // mods, or files a mod update dropped) goes back to backup/vanilla.
    let stale: Vec<String> = {
        let s = m.state.lock().map_err(|e| e.to_string())?;
        let want: std::collections::HashSet<String> = all_written.iter().cloned().collect();
        s.deployed.keys().filter(|k| !want.contains(*k)).cloned().collect()
    };
    if !stale.is_empty() {
        deploy::restore_paths(&m.home.game, &m.home.backup, &stale).map_err(|e| e.to_string())?;
    }
    // mods.settings in priority order, names as deployed folder names.
    let names: Vec<String> = ranked.iter().map(|r| state::ensure_mod_prefix(&r.name)).collect();
    let settings = m.settings_dir().join("mods.settings");
    deploy::write_mods_settings(&settings, &names).map_err(|e| e.to_string())?;
    deploy::update_filelists(&m.home.game, &menu_xmls).map_err(|e| e.to_string())?;
    // Persist deployed map (path -> mod id, last-writer-wins in rank order reversed).
    {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        s.state_deployed(all_written.clone(), &ranked);
        let snapshot = s.clone();
        crate::state::save_state(&m.home.state_file, &snapshot)?;
    }
    Ok(all_written)
}

#[tauri::command]
fn preview_archive(path: String) -> Result<install::InstallPlan, String> {
    use std::path::Path;
    let archive = Path::new(&path);
    // Extract to a temp dir, analyze, return plan (caller confirms before staging).
    let tmp = std::env::temp_dir().join(format!("w3lmn-preview-{}", std::process::id()));
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
    let tmp = std::env::temp_dir().join(format!("w3lmn-install-{}", uuid::Uuid::new_v4().simple()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let res: Result<String, String> = (|| {
        archive::extract_archive(std::path::Path::new(&path), &tmp).map_err(|e| e.to_string())?;
        let plan = install::analyze(&tmp);
        let g = shared.lock().map_err(|e| e.to_string())?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let folder = state::ensure_mod_prefix(&name);
        let stage = m.home.staging.join(&id);
        let (targets, _docs) = install::build_staging(&plan, &stage, &folder)?;
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
        Ok(id)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

#[tauri::command]
fn add_separator(shared: State<Shared>, index: usize, name: String) -> Result<state::ModRow, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let row = m.state.lock().map_err(|e| e.to_string())?.add_separator(index, &name);
    m.save()?;
    Ok(row)
}

#[tauri::command]
fn edit_mod(shared: State<Shared>, id: String, name: String, version: String, nexus_id: String, section: String) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    m.state.lock().map_err(|e| e.to_string())?.edit_mod(&id, &name, &version, &nexus_id, &section);
    m.save()?;
    Ok(true)
}

#[tauri::command]
fn find_collisions(shared: State<Shared>, targets: Vec<String>) -> Result<(Vec<String>, Vec<String>), String> {
    let out = {
        let g = shared.lock().map_err(|e| e.to_string())?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.find_collisions(&targets)
    };
    Ok(out)
}

#[tauri::command]
fn clashes(shared: State<Shared>) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let out = {
        let g = shared.lock().map_err(|e| e.to_string())?;
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let rows: Vec<state::ModRow> = {
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.mods_only().into_iter().cloned().collect()
    };
    let mut owners: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for r in rows.iter().filter(|r| r.enabled) {
        let stage = m.home.staging.join(&r.id);
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
    let g = shared.lock().map_err(|e| e.to_string())?;
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    deploy::restore_paths(&m.home.game, &m.home.backup, &rels).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
fn write_input_settings(shared: State<Shared>, keybinds: std::collections::BTreeMap<String, Vec<String>>) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    settings::write_input_settings(&m.settings_dir().join("input.settings"), &keybinds)?;
    Ok(true)
}

#[tauri::command]
fn write_user_settings(shared: State<Shared>, snippets: std::collections::BTreeMap<String, Vec<String>>) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    settings::write_user_settings(&m.settings_dir().join("user.settings"), &snippets)?;
    Ok(true)
}

#[tauri::command]
fn read_mods_settings(shared: State<Shared>) -> Result<Vec<String>, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
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
fn check_updates(shared: State<Shared>, api_key: String) -> Result<Vec<UpdateHit>, String> {
    if api_key.trim().is_empty() {
        return Err("Set a Nexus API key first".into());
    }
    let mods: Vec<state::ModRow> = {
        let g = shared.lock().map_err(|e| e.to_string())?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let s = m.state.lock().map_err(|e| e.to_string())?;
        let v: Vec<state::ModRow> = s.mods_only().into_iter().cloned().collect();
        v
    };
    let mut hits = vec![];
    for m in mods {
        let nid = if m.nexus.trim().is_empty() { crate::nexus::extract_nexus_id(&m.name) } else { m.nexus.clone() };
        if nid.is_empty() {
            continue;
        }
        let v = match crate::nexus::nexus_get(&format!("/games/witcher3/mods/{nid}.json"), &api_key) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let remote = v.get("version").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if !remote.is_empty() && !m.version.is_empty() && crate::nexus::version_is_newer(&remote, &m.version) {
            hits.push(UpdateHit { id: m.id.clone(), name: m.name.clone(), local: m.version.clone(), remote });
        }
    }
    Ok(hits)
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let game = m.home.game.clone();
    let prefix = m.home.prefix.clone();
    let row = {
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.get(&id).cloned().ok_or("unknown mod")?
    };
    let _ = prefix;
    let mut files: Vec<(String, String, Vec<String>)> = vec![];
    let stage = m.home.staging.join(&id);
    for rel in &row.targets {
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    merger::merger_path_check(&m.home.prefix.to_string_lossy(), &m.home.game.to_string_lossy(), &exe_path)
}

/// Staged file list for one mod (powers per-mod file view + clash details).
#[tauri::command]
fn staged_files(shared: State<Shared>, id: String) -> Result<Vec<String>, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let stage = m.home.staging.join(&id);
    Ok(deploy::mod_files(&stage).into_iter().map(|p| p.to_string_lossy().to_string()).collect())
}

/// Per-mod overlap report: shared scripts/xmls + beaten-file counts
/// (python `analysis` core: sharing + lost, minus merge-registry detail).
#[tauri::command]
fn analysis_summary(shared: State<Shared>) -> Result<serde_json::Value, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
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
fn queue_enqueue(url: String, filename: String) -> String {
    downloads::enqueue(&url, &filename)
}

#[tauri::command]
fn queue_list() -> Vec<downloads::QueueItem> {
    downloads::items()
}

#[tauri::command]
fn queue_cancel(id: String) {
    downloads::cancel(&id);
}

#[tauri::command]
fn queue_pause(id: String, paused: bool) {
    downloads::set_paused(&id, paused);
}

/// Pump one queued download to disk, emitting `download-progress` events.
#[tauri::command]
fn queue_pump(app: tauri::AppHandle, id: String, dest_dir: String, api_key: String) -> Result<String, String> {
    let item = downloads::items().into_iter().find(|i| i.id == id).ok_or("unknown download")?;
    let dest = std::path::Path::new(&dest_dir).join(&item.filename);
    let target = dest.clone();
    let aid = id.clone();
    let n = downloads::pump(
        &id,
        &dest,
        &api_key,
        &|done, total| {
            use tauri::Emitter;
            let _ = app.emit("download-progress", serde_json::json!({"id": aid, "done": done, "total": total}));
        },
    )?;
    use tauri::Emitter;
    let _ = app.emit("download-done", serde_json::json!({"id": id, "path": target.to_string_lossy(), "bytes": n}));
    Ok(target.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_deep_link::init())
        // Single instance: second launches (e.g. nxm:// clicks) focus this
        // window and forward argv URLs as events — replaces the hand-rolled
        // Unix-socket handoff in the original (`send_to_running_app`).
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            use tauri::{Emitter, Manager};
            let _ = app.get_webview_window("main").map(|w| {
                let _ = w.set_focus();
                for arg in argv.iter().filter(|a| a.starts_with("nxm://")) {
                    let _ = w.emit("nxm-url", arg.clone());
                }
            });
        }))
        .setup(|app| {
            // Forward OS deep-link opens (nxm://…) to the frontend as events.
            // The .desktop MimeType registration comes from the
            // `security.deepLinkProtocols` entry in tauri.conf.json — no
            // hand-edited mimeapps.list code needed.
            #[cfg(desktop)]
            {
                use tauri::Manager;
                use tauri_plugin_deep_link::DeepLinkExt;
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    use tauri::Emitter;
                    for url in event.urls() {
                        let s = url.to_string();
                        if s.starts_with("nxm://") {
                            if let Some(w) = handle.get_webview_window("main") {
                                let _ = w.emit("nxm-url", s.clone());
                            }
                        }
                    }
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
            import_legacy_preview,
            made_for,
            quota,
            merger_check,
            staged_files,
            analysis_summary,
            queue_enqueue,
            queue_list,
            queue_cancel,
            queue_pause,
            queue_pump
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
