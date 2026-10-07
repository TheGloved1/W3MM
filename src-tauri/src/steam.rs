//! Steam / Proton path helpers.
//! Ports: `steam_libraries`, `detect_steam`, `wine_drives`,
//! `to_windows_path`, `from_windows_path`, `game_running`
//! (`w3modmanager.py:560,586,7405,7422,7441,1669`).

use std::path::{Path, PathBuf};

/// Parse `libraryfolders.vdf` search roots into candidate library dirs.
pub fn steam_libraries(extra: Option<&str>) -> Vec<PathBuf> {
    fn add(out: &mut Vec<PathBuf>, p: PathBuf) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    let mut out = Vec::new();
    if let Some(e) = extra {
        add(&mut out, PathBuf::from(e));
    }
    #[cfg(target_os = "windows")]
    {
        // Default Steam locations; per-library VDFs below add the rest.
        for var in ["ProgramFiles(x86)", "ProgramW6432", "ProgramFiles"] {
            if let Ok(pf) = std::env::var(var) {
                add(&mut out, PathBuf::from(pf).join("Steam"));
            }
        }
        for drive in ["C:", "D:", "E:"] {
            for lib in ["SteamLibrary", "Steam"] {
                add(&mut out, PathBuf::from(format!("{drive}\\{lib}")));
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    if let Ok(home) = std::env::var("HOME") {
        for cand in [
            format!("{home}/.local/share/Steam"),
            format!("{home}/.steam/steam"),
            "/mnt/storage/SteamLibrary".to_string(),
        ] {
            add(&mut out, PathBuf::from(cand));
        }
        // Parse libraryfolders.vdf for extra libraries.
        let vdf = PathBuf::from(format!("{home}/.local/share/Steam/steamapps/libraryfolders.vdf"));
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for p in library_paths_from_vdf(&text) {
                add(&mut out, p);
            }
        }
    }
    // Every candidate root may carry its own VDF pointing at more libraries.
    for root in out.clone() {
        let vdf = root.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            for p in library_paths_from_vdf(&text) {
                add(&mut out, p);
            }
        }
    }
    out.into_iter().filter(|p| p.is_dir()).collect()
}

/// Extract absolute library paths from VDF text. Handles both Unix
/// (`"path"  "/mnt/games/SteamLibrary"`) and Windows
/// (`"path"  "D:\\SteamLibrary"`) spellings: any quoted span that looks
/// like an absolute path qualifies.
fn library_paths_from_vdf(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('"') else { break };
            let cand = after[..end].trim();
            rest = &after[end + 1..];
            if cand.is_empty() {
                continue;
            }
            let b = cand.as_bytes();
            let is_abs = cand.starts_with('/')
                || (cand.len() >= 3
                    && b[0].is_ascii_alphabetic()
                    && b[1] == b':'
                    && (b[2] == b'\\' || b[2] == b'/'));
            if is_abs {
                let p = PathBuf::from(cand);
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
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
    // Native Windows runs need no Proton prefix at all.
    #[cfg(target_os = "windows")]
    {
        let _ = game_dir;
        return None;
    }
    #[cfg(not(target_os = "windows"))]
    {
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
        false
    }
    #[cfg(target_os = "windows")]
    {
        // `tasklist` needs no extra dependency and mirrors the /proc scan:
        // any process whose image name mentions witcher3 blocks deploying.
        if let Ok(out) = std::process::Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
            return text.contains("witcher3");
        }
        false
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdf_paths_unix_and_windows() {
        let text = r#""libraryfolders"
{
	"0"
	{
		"path"		"/home/gloves/.local/share/Steam"
	}
	"1"
	{
		"path"		"/mnt/games/SteamLibrary"
	}
	"2"
	{
		"path"		"D:\SteamLibrary"
	}
	"3"
	{
		"path"		"E:/Games/Steam"
	}
}"#;
        let got = library_paths_from_vdf(text);
        for want in [
            "/home/gloves/.local/share/Steam",
            "/mnt/games/SteamLibrary",
            "D:\\SteamLibrary",
            "E:/Games/Steam",
        ] {
            assert!(got.contains(&PathBuf::from(want)), "missing {want}: {got:?}");
        }
        // Key names and relative junk never qualify.
        assert!(!got.iter().any(|p| p.to_string_lossy() == "path"));
    }
}
