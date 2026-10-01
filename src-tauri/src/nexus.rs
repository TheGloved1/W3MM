//! Nexus Mods API client.
//! Ports `nexus_get/nexus_files/version_is_newer/check_updates/allowance/
//! refresh_limits` + `NEXUS_*` regexes (`w3modmanager.py:373-466,833-936,6390+`).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

fn nexus_new() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?P<name>.+?)[ _-]*(?:v|V)?(?P<ver>\d+(?:[._]\d+)+)(?:[ _-]+(?P<tag>[^_ -]+))?$").unwrap())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusFile {
    pub file_id: i64,
    pub name: String,
    pub version: String,
    pub category: String,
}

/// Split `parse_archive_name` behaviour: (display, version, nexus_id).
pub fn parse_archive_name(filename: &str) -> (String, String, String) {
    let mut stem = filename.to_string();
    for ext in crate::archive::ARCHIVE_EXTS {
        if stem.to_lowercase().ends_with(ext) {
            stem.truncate(stem.len() - ext.len());
            break;
        }
    }
    stem = Regex::new(r"\s*\(\d+\)$").unwrap().replace(&stem, "").to_string();
    // "Name-12345-1-2-3" Nexus-stamped style
    let stamped = Regex::new(r"^(?P<name>.+?)-(?P<id>\d+)-(?P<ver>[\d._-]+)$").unwrap();
    if let Some(c) = stamped.captures(&stem) {
        let ver = c["ver"].replace(['_', '-'], ".");
        return (clean(&c["name"]), clean_version(&ver), c["id"].to_string());
    }
    if let Some(c) = nexus_new().captures(&stem) {
        return (clean(&c["name"]), clean_version(&c["ver"]), String::new());
    }
    (clean(&stem), String::new(), String::new())
}

fn clean(s: &str) -> String {
    s.replace('_', " ").trim_matches(&[' ', '-'][..]).to_string()
}

pub fn clean_version(v: &str) -> String {
    // python `clean_version`: strip only a leading v/ver/version before a digit.
    let t = v.trim();
    let low = t.to_lowercase();
    for prefix in ["version", "ver.", "ver", "v"] {
        if low.starts_with(prefix) {
            let rest = t[prefix.len()..].trim_start();
            let rest = rest.strip_prefix('.').unwrap_or(rest).trim_start();
            if rest.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                return rest.to_string();
            }
        }
    }
    t.to_string()
}

pub fn version_tuple(v: &str) -> Vec<u64> {
    v.split('.').filter_map(|p| p.parse::<u64>().ok()).collect()
}

pub fn version_is_newer(remote: &str, local: &str) -> bool {
    version_tuple(remote) > version_tuple(local)
}

pub fn nexus_get(path: &str, api_key: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("W3LMN/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("https://api.nexusmods.com/v1{path}");
    let resp = client
        .get(&url)
        .header("apikey", api_key)
        .header("Application-Name", "W3LMN")
        .header("Application-Version", env!("CARGO_PKG_VERSION"))
        .send()
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Nexus {} for {path}", resp.status()));
    }
    resp.json::<serde_json::Value>().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_names() {
        let (n, v, id) = parse_archive_name("MyMod-1234-1-0-0.zip");
        assert_eq!(n, "MyMod");
        assert_eq!(id, "1234");
        assert_eq!(v, "1.0.0");
        assert!(version_is_newer("1.1", "1.0"));
    }
}
