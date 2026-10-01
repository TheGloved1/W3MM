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
mod home;
mod install;
mod keybinds;
mod manager;
mod nexus;
mod script_merge;
mod settings;
mod state;
mod steam;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
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
            read_mods_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
