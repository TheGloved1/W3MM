//! External Script Merger integration.
//! Ports `MergerSettings` (settings-file read/write beside the exe),
//! `setting_role`, `merger_config`, `merger_path_check`
//! (`w3modmanager.py:7453-7669`).
//!
//! The merger itself stays external (a .NET exe run under Proton on Linux,
//! natively on Windows); this module reads its config, compares stored paths
//! with what the prefix needs, and writes fixes back — the UI surfaces them
//! instead of failing deploys later.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergerReport {
    pub config: String,
    pub fixes: BTreeMap<String, String>,
    pub wrong: Vec<(String, String, String)>,
    pub unfixable: Vec<(String, String)>,
}

/// Which game path a setting key holds: game | mods | scripts | None.
pub fn setting_role(key: &str) -> Option<&'static str> {
    let k = key.to_lowercase();
    if k.contains("gamedir") || (k.contains("game") && k.contains("dir")) || k == "gamepath" {
        Some("game")
    } else if k.contains("modsdir") || k.contains("modsfolder") || (k.contains("mod") && k.contains("dir")) {
        Some("mods")
    } else if k.contains("script") {
        Some("scripts")
    } else {
        None
    }
}

fn read_pairs(text: &str) -> BTreeMap<String, String> {
    // .NET app.config XML <add key= value=> / <setting name=><value>, plus INI/JSON fallbacks.
    let mut out = BTreeMap::new();
    let add_re = regex::Regex::new(r#"(?i)<add\s+key="([^"]+)"\s+value="([^"]*)""#).unwrap();
    for c in add_re.captures_iter(text) {
        out.insert(c[1].to_string(), c[2].to_string());
    }
    let set_re = regex::Regex::new(r#"(?is)<setting\s+name="([^"]+)"[^>]*>.*?<value>(.*?)</value>"#).unwrap();
    for c in set_re.captures_iter(text) {
        out.insert(c[1].to_string(), c[2].trim().to_string());
    }
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('<') || t.starts_with('[') || t.starts_with(';') || t.starts_with('#') || t.is_empty() {
            continue;
        }
        if let Some((k, v)) = t.split_once('=').or_else(|| t.split_once(':')) {
            let (k, v) = (k.trim().trim_matches('"'), v.trim().trim_matches(',').trim().trim_matches('"'));
            if !k.is_empty() && out.get(k).is_none() {
                out.insert(k.to_string(), v.to_string());
            }
        }
    }
    out
}

fn merger_config_file(exe_path: &Path) -> Option<PathBuf> {
    let dir = exe_path.parent()?;
    let named = [
        format!("{}.config", exe_path.file_name()?.to_string_lossy()),
        format!("{}.dll.config", exe_path.file_stem()?.to_string_lossy()),
        format!("{}.config", exe_path.file_stem()?.to_string_lossy()),
    ];
    let mut cands: Vec<PathBuf> = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.to_lowercase().ends_with(".original") {
                continue;
            }
            if n.to_lowercase().ends_with(".config") || n.to_lowercase().ends_with(".json") {
                cands.push(e.path());
            }
        }
    }
    cands.sort();
    let mut best: Option<(bool, bool, PathBuf)> = None;
    for f in cands {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let pairs = read_pairs(&text);
        if pairs.is_empty() {
            continue;
        }
        let has_role = pairs.keys().any(|k| setting_role(k).is_some());
        let rank = (named.iter().any(|x| x.to_lowercase() == f.file_name().unwrap_or_default().to_string_lossy().to_lowercase()), has_role);
        if !rank.0 && !rank.1 && pairs.is_empty() {
            continue;
        }
        if best.is_none() || rank > (best.as_ref().unwrap().0, best.as_ref().unwrap().1) {
            best = Some((rank.0, rank.1, f));
        }
    }
    best.map(|b| b.2)
}

fn from_windows_path(prefix: &str, win: &str) -> Option<PathBuf> {
    // Native Windows runs the merger directly: its stored paths already name
    // real filesystem locations, so the mapping is the identity.
    #[cfg(target_os = "windows")]
    {
        let _ = prefix;
        let t = win.trim();
        if t.len() >= 3 && t.as_bytes()[1] == b':' && (t.as_bytes()[2] == b'\\' || t.as_bytes()[2] == b'/') {
            return Some(PathBuf::from(t));
        }
        return None;
    }
    #[cfg(not(target_os = "windows"))]
    {
        crate::steam::wine_drives(prefix);
        // C:\... -> prefix/drive_c/...
        let t = win.trim();
        if t.len() >= 3 && t.as_bytes()[1] == b':' && (t.as_bytes()[2] == b'\\' || t.as_bytes()[2] == b'/') {
            let drive = t[..1].to_lowercase();
            let rest = t[3..].replace('\\', "/");
            if drive == "c" {
                return Some(Path::new(prefix).join("drive_c").join(rest));
            }
            let dd = Path::new(prefix).join("dosdevices").join(format!("{drive}:"));
            let target = std::fs::read_link(&dd).unwrap_or(dd);
            return Some(target.join(rest));
        }
        // Z:\... commonly maps to / in Wine
        if t.to_lowercase().starts_with("z:\\") || t.to_lowercase().starts_with("z:/") {
            return Some(PathBuf::from(format!("/{}", t[3..].replace('\\', "/"))));
        }
        None
    }
}

/// Compare stored merger paths with what this prefix needs.
pub fn merger_path_check(prefix: &str, game_dir: &str, exe_path: &str) -> Result<MergerReport, String> {
    let exe = Path::new(exe_path);
    let cfg = merger_config_file(exe).ok_or("no merger settings file found beside the exe")?;
    let text = std::fs::read_to_string(&cfg).map_err(|e| e.to_string())?;
    let stored = read_pairs(&text);
    let game = PathBuf::from(game_dir);
    let mods = game.join("mods");
    let scripts = game.join("content/content0/scripts");
    // What the merger's stored paths should look like: reachable Windows
    // paths through the prefix on Linux, native paths on Windows where the
    // merger runs directly.
    #[cfg(target_os = "windows")]
    let want_win = |p: &PathBuf| Some(p.to_string_lossy().to_string());
    #[cfg(not(target_os = "windows"))]
    let want_win = |p: &PathBuf| crate::steam::to_windows_path(prefix, &p.to_string_lossy());
    let windows = [
        ("game", want_win(&game)),
        ("mods", want_win(&mods)),
        ("scripts", want_win(&scripts)),
    ]
    .into_iter()
    .map(|(k, v)| (k, v))
    .collect::<BTreeMap<_, _>>();
    if windows["game"].is_none() {
        return Err("game folder is not reachable as a Windows path in this prefix".into());
    }
    let mut fixes = BTreeMap::new();
    let mut wrong = vec![];
    let mut unfixable = vec![];
    let mut seen_roles = std::collections::HashSet::new();
    for (key, value) in &stored {
        match setting_role(key) {
            None => {
                if from_windows_path(prefix, value).map(|p| !p.exists()).unwrap_or(true) {
                    unfixable.push((key.clone(), value.clone()));
                }
            }
            Some(role) => {
                seen_roles.insert(role);
                let ok = from_windows_path(prefix, value).map(|p| p.exists()).unwrap_or(false);
                if role == "game" {
                    let same = from_windows_path(prefix, value)
                        .map(|p| p.to_string_lossy().to_lowercase() == game.to_string_lossy().to_lowercase())
                        .unwrap_or(false);
                    if ok && same {
                        continue;
                    }
                } else if ok {
                    continue;
                }
                match windows.get(role).cloned().flatten() {
                    Some(want) => {
                        fixes.insert(key.clone(), want.clone());
                        wrong.push((key.clone(), value.clone(), want));
                    }
                    None => unfixable.push((key.clone(), value.clone())),
                }
            }
        }
    }
    if !seen_roles.contains("game") {
        let key = stored.keys().find(|k| setting_role(k) == Some("game")).cloned().unwrap_or("GameDirectory".into());
        let prev = stored.get(&key).cloned().unwrap_or_default();
        let want = windows["game"].clone().unwrap();
        fixes.insert(key.clone(), want.clone());
        wrong.push((key, prev, want));
    }
    Ok(MergerReport { config: cfg.to_string_lossy().to_string(), fixes, wrong, unfixable })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roles() {
        assert_eq!(setting_role("GameDirectory"), Some("game"));
        assert_eq!(setting_role("ModsDirectory"), Some("mods"));
        assert_eq!(setting_role("Scripts"), Some("scripts"));
        assert_eq!(setting_role("Theme"), None);
    }
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn proton_drive_c_mapping() {
        // C:\... inside a fake prefix resolves under drive_c.
        let base = std::env::temp_dir().join(format!("w3mm-merger-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("drive_c/Games")).unwrap();
        let got = from_windows_path(&base.to_string_lossy(), "C:\\Games\\Witcher3").unwrap();
        assert_eq!(got, base.join("drive_c/Games/Witcher3"));
        assert!(from_windows_path(&base.to_string_lossy(), "relative\\path").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }
}
