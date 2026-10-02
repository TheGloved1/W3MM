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
use std::sync::OnceLock;
use std::io::Write;
use std::path::PathBuf;
use tauri::State;

/// XDG state dir log file (e.g. ~/.local/state/w3lmn/w3lmn.log).
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

fn log_path() -> &'static PathBuf {
    LOG_PATH.get_or_init(|| {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let mut p = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
                p.push(".local");
                p.push("state");
                p
            });
        let mut dir = base;
        dir.push("w3lmn");
        let _ = std::fs::create_dir_all(&dir);
        dir.push("w3lmn.log");
        dir
    })
}

fn log_line(level: &str, msg: &str) {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let line = format!("{stamp} [{level}] {msg}\n");
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
    // Kept merges win over every staged copy: write them over the deployed file.
    let kept: Vec<(String, String)> = {
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.merge_kept.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    let mut merged_count = 0;
    for (rel, text) in kept {
        let dst = m.home.game.join(&rel);
        let bytes = text.replace('\n', "\r\n").into_bytes();
        if dst.is_file() {
            let bdst = m.home.backup.join(&rel);
            if !bdst.exists() {
                if let Some(p) = bdst.parent() {
                    std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
                std::fs::copy(&dst, &bdst).map_err(|e| e.to_string())?;
            }
        } else if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dst, bytes).map_err(|e| e.to_string())?;
        if !all_written.contains(&rel) {
            all_written.push(rel.clone());
        }
        merged_count += 1;
    }
    let _ = merged_count;
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
    let g = shared.lock().map_err(|e| e.to_string())?;
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    // mods.settings priorities/enabled for ordering + initial state
    let settings = std::fs::read_to_string(m.settings_dir().join("mods.settings")).unwrap_or_default();
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
        let s = m.state.lock().map_err(|e| e.to_string())?;
        s.mods_only().iter().flat_map(|r| r.targets.iter().map(|t| t.to_lowercase())).collect()
    };
    let mut ids = vec![];
    for rel in sorted {
        // Defensive: never adopt a folder a managed mod already owns (stale
        // banner lists must not duplicate a just-installed mod).
        if owns_folder(&owned, &rel) {
            log_line("rust", &format!("import_unmanaged: skipping owned {rel}"));
            continue;
        }
        let src = m.home.game.join(&rel);
        if !src.is_dir() {
            continue;
        }
        let folder = src.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let dst = m.home.staging.join(&id).join("mods").join(&folder);
        std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::rename(&src, &dst).map_err(|e| format!("{}: {e}", rel))?;
        let stripped = if folder.len() > 3 && folder.to_lowercase().starts_with("mod") {
            folder[3..].to_string()
        } else {
            folder.clone()
        };
        {
            let mut s = m.state.lock().map_err(|e| e.to_string())?;
            s.mods.push(state::ModRow {
                id: id.clone(), sep: false, name: stripped, enabled: *en.get(&folder.to_lowercase()).unwrap_or(&true),
                version: String::new(), nexus: String::new(), archive: String::new(),
                section: String::new(), updated: chrono::Utc::now().timestamp(),
                collapsed: false, targets: vec![format!("mods/{folder}")],
                nexus_cat: String::new(), main_of: String::new(),
            });
            s.priority_ids();
        }
        ids.push(id);
    }
    m.save()?;
    Ok(ids)
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
fn queue_enqueue(shared: State<Shared>, url: String, filename: String, api_key: String) -> Result<String, String> {
    let id = downloads::enqueue(&url, &filename, &api_key)?;
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
    let g = shared.lock().ok()?;
    let m = g.as_ref()?;
    let dir = m.home.game.join(crate::home::MANAGER_DIRNAME).join("downloads");
    let _ = std::fs::create_dir_all(&dir);
    let hist = m.home.game.join(crate::home::MANAGER_DIRNAME).join("downloads.json");
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

/// Pump one queued download to disk, emitting `download-progress` events.
#[tauri::command]
fn queue_pump(app: tauri::AppHandle, shared: State<Shared>, id: String, dest_dir: String, api_key: String) -> Result<String, String> {
    let item = downloads::items().into_iter().find(|i| i.id == id).ok_or("unknown download")?;
    let dest = std::path::Path::new(&dest_dir).join(&item.filename);
    let target = dest.clone();
    let aid = id.clone();
    let res = downloads::pump(
        &id,
        &dest,
        &api_key,
        &|done, total, speed| {
            use tauri::Emitter;
            let _ = app.emit("download-progress", serde_json::json!({"id": aid, "done": done, "total": total, "speed": speed}));
        },
    );
    dl_save(&shared);
    let n = res?;
    use tauri::Emitter;
    let _ = app.emit("download-done", serde_json::json!({"id": id, "path": target.to_string_lossy(), "bytes": n}));
    log_line("rust", &format!("download done id={id} path={}", target.to_string_lossy()));
    Ok(target.to_string_lossy().to_string())
}

/// Absolute downloads-dir path (<game>/_W3LMN/downloads).
#[tauri::command]
fn downloads_dir_path(shared: State<Shared>) -> Result<String, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let d = m.home.game.join(crate::home::MANAGER_DIRNAME).join("downloads");
    let _ = std::fs::create_dir_all(&d);
    Ok(d.to_string_lossy().to_string())
}

/// Absolute settings-dir path for the Open menu.
#[tauri::command]
fn settings_dir_path(shared: State<Shared>) -> Result<String, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    Ok(m.settings_dir().to_string_lossy().to_string())
}

/// Move a mod under a section separator (or to the unsectioned end).
#[tauri::command]
fn move_to_section(shared: State<Shared>, id: String, sep_id: String) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
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
    }
    m.save()?;
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
        "setup" | "settings" => ("Settings", 700.0, 640.0),
        "resolver" => ("Script decisions", 1150.0, 760.0),
        _ => {
            log_line("rust", &format!("open_tool_window: unknown kind={kind:?}"));
            return Err("unknown window".into());
        }
    };
    // Window labels are stable ids; routes are what SvelteKit builds.
    let route = match kind.as_str() {
        "setup" | "settings" => "settings",
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
    #[cfg(not(dev))]
    let webview_url = tauri::WebviewUrl::App(url.into());
    #[cfg(dev)]
    let webview_url = tauri::WebviewUrl::External(url.parse().expect("valid dev url"));
    log_line("rust", &format!("open_tool_window: building window label={kind:?} title={title:?} route={route:?} url={url:?}"));
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    let kind = if rel.to_lowercase().ends_with(".xml") { "xml" } else { "script" }.to_string();
    let (rows, order) = {
        let mut s = m.state.lock().map_err(|e| e.to_string())?;
        let order = s.priority_ids();
        (s.mods_only().into_iter().cloned().collect::<Vec<_>>(), order)
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
    let mut base_bytes: Option<Vec<u8>> = std::fs::read(m.home.game.join(&rel)).ok();
    if base_bytes.is_none() {
        let content = m.home.game.join("content");
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
        let data = std::fs::read(m.home.staging.join(&r.id).join(&rel)).unwrap_or_default();
        let (lines, _) = script_merge::decode_script(&data);
        versions.push(MergeVersion { label: r.name.clone(), mod_id: r.id.clone(), text: lines.join("\n") });
    }
    Ok(MergeInputs { rel, kind, base: base_lines.join("\n"), base_encoding: enc, versions })
}

/// Keep a resolved merge: stored under the rel, applied on top of deploys.
#[tauri::command]
fn save_merge(shared: State<Shared>, rel: String, text: String, answers: Vec<usize>) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
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
    let g = shared.lock().map_err(|e| e.to_string())?;
    let m = g.as_ref().ok_or("open a game folder first")?;
    Ok(m.home.staging.join(&id).to_string_lossy().to_string())
}

/// Remove a section; its mods move out to the unsectioned end of the list in
/// order, priorities unchanged (python `remove_section`).
#[tauri::command]
fn remove_section_cmd(shared: State<Shared>, sep_id: String) -> Result<bool, String> {
    let g = shared.lock().map_err(|e| e.to_string())?;
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
    let tmp = std::env::temp_dir().join(format!("w3lmn-roots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let plan = (|| {
        archive::extract_archive(archive, &tmp).map_err(|e| e.to_string())?;
        Ok::<_, String>(install::analyze(&tmp))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    let plan = plan?;
    // group staged targets by their top two segments (kind + folder)
    let mut groups: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for (_src, rel) in &plan.moves {
        let mut parts = rel.split('/');
        let first = parts.next().unwrap_or("").to_lowercase();
        if ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
            let folder = parts.next().unwrap_or("").to_string();
            let kind = match first.as_str() {
                "dlc" => "DLC",
                "bin" => "Bin",
                "content" => "Content",
                _ => "Mod",
            }
            .to_string();
            *groups.entry((kind, folder)).or_default() += 1;
        } else {
            *groups.entry(("Mod".to_string(), String::new())).or_default() += 1;
        }
    }
    Ok(groups
        .into_iter()
        .map(|((kind, folder), files)| PlanRoot {
            prefix: if folder.is_empty() { String::new() } else { format!("{}/{}", kind.to_lowercase(), folder) },
            kind,
            folder,
            files,
        })
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
    let tmp = std::env::temp_dir().join(format!("w3lmn-preview-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let plan = (|| {
        archive::extract_archive(archive, &tmp).map_err(|e| e.to_string())?;
        Ok::<_, String>(install::analyze(&tmp))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    let plan = plan?;
    // group staged targets by their top two segments (kind + folder)
    let mut groups: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for (_src, rel) in &plan.moves {
        let mut parts = rel.split('/');
        let first = parts.next().unwrap_or("").to_lowercase();
        if ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
            let folder = parts.next().unwrap_or("").to_string();
            let kind = match first.as_str() {
                "dlc" => "DLC",
                "bin" => "Bin",
                "content" => "Content",
                _ => "Mod",
            }
            .to_string();
            *groups.entry((kind, folder)).or_default() += 1;
        } else {
            *groups.entry(("Mod".to_string(), String::new())).or_default() += 1;
        }
    }
    let roots = groups
        .into_iter()
        .map(|((kind, folder), files)| PlanRoot {
            prefix: if folder.is_empty() { String::new() } else { format!("{}/{}", kind.to_lowercase(), folder) },
            kind,
            folder,
            files,
        })
        .collect();
    Ok(InstallPreview { roots, moves: plan.moves })
}

/// Install with per-root kind/folder mapping from the dialog's table.
#[tauri::command]
fn install_roots(
    shared: State<Shared>,
    path: String,
    name: String,
    version: String,
    nexus_id: String,
    section: String,
    roots: Vec<PlanRoot>,
) -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("w3lmn-install-{}", uuid::Uuid::new_v4().simple()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let res: Result<String, String> = (|| {
        archive::extract_archive(std::path::Path::new(&path), &tmp).map_err(|e| e.to_string())?;
        let plan = install::analyze(&tmp);
        let g = shared.lock().map_err(|e| e.to_string())?;
        let m = g.as_ref().ok_or("open a game folder first")?;
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let stage = m.home.staging.join(&id);
        // remap each planned move through the dialog's root table
        let mut remapped: Vec<(String, String)> = vec![];
        for (src, rel) in &plan.moves {
            let mut parts = rel.split('/');
            let first = parts.next().unwrap_or("").to_lowercase();
            let mapped = if ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
                let folder = parts.next().unwrap_or("").to_string();
                let kind = match first.as_str() {
                    "dlc" => "DLC",
                    "bin" => "Bin",
                    "content" => "Content",
                    _ => "Mod",
                };
                let row = roots.iter().find(|r| r.kind == kind && r.folder == folder);
                match row {
                    Some(r) if !r.folder.is_empty() => {
                        let rest: Vec<&str> = rel.split('/').skip(2).collect();
                        format!("{}/{}/{}", kind_dir(&r.kind), r.folder, rest.join("/"))
                    }
                    _ => rel.clone(),
                }
            } else {
                let row = roots.iter().find(|r| r.folder.is_empty() || r.prefix.is_empty());
                match row {
                    Some(r) if !r.folder.is_empty() => format!("{}/{rel}", kind_dir(&r.kind)),
                    Some(r) => format!("{}/{}", kind_dir(&r.kind), state::folder_safe(&name)),
                    None => format!("mods/{}/{}", state::folder_safe(&name), rel),
                }
            };
            remapped.push((src.clone(), mapped));
        }
        let sub = install::InstallPlan { moves: remapped, docs: plan.docs.clone() };
        let folder = state::ensure_mod_prefix(&name);
        let (targets, _docs) = install::build_staging(&sub, &stage, &folder)?;
        {
            let mut s = m.state.lock().map_err(|e| e.to_string())?;
            s.mods.push(state::ModRow {
                id: id.clone(), sep: false, name: name.clone(), enabled: true,
                version: version.clone(), nexus: nexus_id.clone(), archive: path.clone(),
                section: section.clone(), updated: chrono::Utc::now().timestamp(),
                collapsed: false, targets: targets.clone(), nexus_cat: String::new(), main_of: String::new(),
            });
            s.priority_ids();
        }
        m.save()?;
        Ok(id)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    res
}

fn kind_dir(kind: &str) -> &'static str {
    match kind {
        "DLC" => "dlc",
        "Bin" => "bin",
        "Content" => "content",
        _ => "mods",
    }
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
            unmanaged_mods,
            import_unmanaged,
            analysis_summary,
            queue_enqueue,
            queue_list,
            queue_cancel,
            queue_remove,
            queue_trash,
            queue_pause,
            queue_pump,
            downloads_history,
            downloads_dir_path,
            settings_dir_path,
            open_tool_window,
            merger_apply,
            merge_inputs,
            save_merge,
            mod_dir,
            remove_section_cmd,
            move_to_section,
            preview_roots,
            install_preview,
            install_roots,
            frontend_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
