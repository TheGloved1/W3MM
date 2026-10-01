//! Steam / Proton path helpers.
//! Ports: `steam_libraries`, `detect_steam`, `wine_drives`,
//! `to_windows_path`, `from_windows_path`, `game_running`
//! (`w3modmanager.py:560,586,7405,7422,7441,1669`).

use std::path::{Path, PathBuf};

/// Parse `libraryfolders.vdf` search roots into candidate library dirs.
pub fn steam_libraries(extra: Option<&str>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut add = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    if let Some(e) = extra {
        add(PathBuf::from(e));
    }
    if let Ok(home) = std::env::var("HOME") {
        for cand in [
            format!("{home}/.local/share/Steam"),
            format!("{home}/.steam/steam"),
            format!("/mnt/storage/SteamLibrary"),
        ] {
            add(PathBuf::from(cand));
        }
        // Parse libraryfolders.vdf for extra libraries.
        let vdf = PathBuf::from(format!("{home}/.local/share/Steam/steamapps/libraryfolders.vdf"));
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for line in text.lines() {
                // lines look like: "path"  "/mnt/games/SteamLibrary"
                let t = line.trim().trim_matches('"');
                for token in ["/mnt/", "/media/", "/run/media/"] {
                    if line.contains(token) {
                        if let Some(start) = line.find('/') {
                            let mut end = line[start..].find('"').map(|i| start + i).unwrap_or(line.len());
                            // also handle trailing quote style
                            let raw = line[start..end].trim();
                            let _ = &mut end;
                            add(PathBuf::from(raw));
                        }
                        let _ = t;
                    }
                }
            }
        }
    }
    out.into_iter().filter(|p| p.is_dir()).collect()
}

pub fn detect_game_dir() -> Option<String> {
    for lib in steam_libraries(None) {
        let cand = lib.join("steamapps/common/The Witcher 3");
        if crate::home::is_game_dir(&cand.to_string_lossy()) {
            return Some(cand.to_string_lossy().to_string());
        }
    }
    None
}

pub fn default_prefix_for(game_dir: &str) -> Option<String> {
    // `<steam>/steamapps/compatdata/292030/pfx` when game is under a library.
    let game = Path::new(game_dir);
    let s = game.to_string_lossy();
    if let Some(idx) = s.find("steamapps/common/The Witcher 3") {
        let steam = &s[..idx];
        let pfx = PathBuf::from(steam).join("steamapps/compatdata/292030/pfx");
        if pfx.is_dir() {
            return Some(pfx.to_string_lossy().to_string());
        }
    }
    None
}

/// Wine drives inside a prefix: `dosdevices/c:` .. plus drive_c symlink handling.
pub fn wine_drives(prefix: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let dd = Path::new(prefix).join("dosdevices");
    if let Ok(rd) = std::fs::read_dir(&dd) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.len() == 2 && name.ends_with(':') {
                out.push((name, e.path()));
            }
        }
    }
    out.sort();
    out
}

/// Map a linux path to a windows path via the prefix (`to_windows_path`).
pub fn to_windows_path(prefix: &str, linux_path: &str) -> Option<String> {
    let lp = Path::new(linux_path);
    // drive_c direct mapping first
    let drive_c = Path::new(prefix).join("drive_c");
    if let Ok(rel) = lp.strip_prefix(&drive_c) {
        return Some(format!("C:\\{}", rel.to_string_lossy().replace('/', "\\")));
    }
    // dosdevices symlinks
    for (letter, target) in wine_drives(prefix) {
        let resolved = std::fs::read_link(&target).unwrap_or(target.clone());
        let base = if resolved.is_absolute() {
            resolved
        } else {
            Path::new(prefix).join("dosdevices").join(resolved)
        };
        if let Ok(rel) = lp.strip_prefix(&base) {
            let drive = letter.to_uppercase();
            return Some(format!("{drive}\\{}", rel.to_string_lossy().replace('/', "\\")));
        }
    }
    None
}

pub fn game_running() -> bool {
    // python `game_running`: look for witcher3.exe / wine processes.
    #[cfg(target_os = "linux")]
    {
        if let Ok(rd) = std::fs::read_dir("/proc") {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !name.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let cmd = std::fs::read_to_string(format!("/proc/{name}/comm")).unwrap_or_default();
                let c = cmd.to_lowercase();
                if c.contains("witcher3") {
                    return true;
                }
            }
        }
    }
    false
}
