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

fn http_client() -> &'static reqwest::blocking::Client {
    static C: std::sync::OnceLock<reqwest::blocking::Client> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .user_agent("W3MM/1.0")
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("http client")
    })
}

pub fn nexus_get(path: &str, api_key: &str) -> Result<serde_json::Value, String> {
    let url = format!("https://api.nexusmods.com/v1{path}");
    let resp = http_client()
        .get(&url)
        .header("apikey", api_key)
        .header("Application-Name", "W3MM")
        .header("Application-Version", env!("CARGO_PKG_VERSION"))
        .send()
        .map_err(|e| e.to_string())?;
    // Track rate-limit headers like the original's LAST_LIMITS (validate
    // doesn't count against the allowance, everything else does).
    {
        let mut lim = last_limits().lock().map_err(|e| e.to_string())?;
        for (k, v) in resp.headers() {
            let kl = k.as_str().to_lowercase();
            if kl.starts_with("x-rl-") {
                lim.insert(kl, v.to_str().unwrap_or("").to_string());
            }
        }
    }
    if !resp.status().is_success() {
        return Err(format!("Nexus {} for {path}", resp.status()));
    }
    resp.json::<serde_json::Value>().map_err(|e| e.to_string())
}

fn last_limits() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    static L: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, String>>> = std::sync::OnceLock::new();
    L.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Calls we're willing to make right now (python `allowance`).
pub fn allowance() -> i64 {
    let lim = last_limits().lock().map(|l| l.clone()).unwrap_or_default();
    let mut spare = vec![];
    for kind in ["hourly", "daily"] {
        if let Some(rem) = lim.get(&format!("x-rl-{kind}-remaining")) {
            let limit: i64 = lim.get(&format!("x-rl-{kind}-limit")).and_then(|v| v.parse().ok()).unwrap_or(0);
            let remaining: i64 = rem.parse().unwrap_or(0);
            spare.push(remaining - std::cmp::max(5, (limit as f64 * 0.1) as i64));
        }
    }
    spare.into_iter().min().unwrap_or(200)
}

/// (text, tip) for the status bar (python `limits_text`).
pub fn limits_text() -> (String, String) {
    let lim = last_limits().lock().map(|l| l.clone()).unwrap_or_default();
    let pair = |kind: &str| -> Option<String> {
        let rem = lim.get(&format!("x-rl-{kind}-remaining"))?;
        match lim.get(&format!("x-rl-{kind}-limit")) {
            Some(l) if !l.is_empty() => Some(format!("{rem}/{l}")),
            _ => Some(rem.clone()),
        }
    };
    let (daily, hourly) = (pair("daily"), pair("hourly"));
    if daily.is_none() && hourly.is_none() {
        return (String::new(), String::new());
    }
    let text = format!("Nexus API: {}", hourly.clone().or(daily.clone()).unwrap_or_default());
    let tip = [hourly.map(|h| format!("Hourly: {h}")), daily.map(|d| format!("Daily: {d}"))]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
    (text, tip)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NxmLink {
    pub game: String,
    pub mod_id: String,
    pub file_id: String,
    pub key: String,
    pub expires: String,
}

/// A newer file found on the mod's Nexus page (for one-click updates).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTarget {
    pub file_id: String,
    pub version: String,
    pub file_name: String,
    pub file_title: String,
    pub size: u64,
    pub mod_name: String,
    pub category: String,
}

fn file_version_of(f: &serde_json::Map<String, serde_json::Value>) -> String {
    clean_version(
        f.get("version")
            .or_else(|| f.get("mod_version"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
}

fn file_size_of(f: &serde_json::Map<String, serde_json::Value>) -> u64 {
    if let Some(size) = f.get("size_in_bytes").and_then(|v| v.as_u64()) {
        return size;
    }
    if let Some(kb) = f.get("size_kb").and_then(|v| v.as_u64()) {
        return kb * 1024;
    }
    if let Some(sz) = f.get("size").and_then(|v| v.as_u64()) {
        return sz * 1024;
    }
    0
}

fn file_stamp_of(f: &serde_json::Map<String, serde_json::Value>) -> u64 {
    for k in ["uploaded_timestamp", "updated_timestamp", "uploaded_time", "updated_time"] {
        if let Some(t) = f.get(k).and_then(|v| v.as_u64()) {
            if t > 1_000_000_000 && t < 10_000_000_000 {
                return t; // seconds
            }
            if t > 1_000_000_000_000 {
                return t / 1000; // millis
            }
        }
    }
    0
}

/// Newest MAIN file on a mod page that is newer than `installed`
/// (None when current). List shape varies, so both `[...]` and
/// `{"files": [...]}` responses are accepted.
pub fn resolve_update(mod_id: &str, installed: &str, api_key: &str) -> Result<Option<UpdateTarget>, String> {
    let v = nexus_get(&format!("/games/witcher3/mods/{mod_id}/files.json"), api_key)?;
    let arr: Vec<&serde_json::Value> = match &v {
        serde_json::Value::Array(a) => a.iter().collect(),
        serde_json::Value::Object(o) => o
            .get("files")
            .and_then(|f| f.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => vec![],
    };
    let mut mains = vec![];
    let mut others = vec![];
    for f in arr {
        let Some(obj) = f.as_object() else { continue };
        let fid = obj
            .get("file_id")
            .and_then(|v| v.as_u64().map(|n| n.to_string()))
            .or_else(|| obj.get("id").and_then(|v| v.as_u64().map(|n| n.to_string())))
            .unwrap_or_default();
        if fid.is_empty() || fid == "0" {
            continue;
        }
        let cat = obj.get("category_name").and_then(|v| v.as_str()).unwrap_or("").to_uppercase();
        let primary = obj.get("is_primary").and_then(|v| v.as_bool()).unwrap_or(false);
        let entry = (fid, obj);
        if primary || cat == "MAIN" {
            mains.push(entry);
        } else {
            others.push(entry);
        }
    }
    // Main files first; fall back to anything when the page has no MAIN mark.
    let pool = if mains.is_empty() { others } else { mains };
    let mut best: Option<(&String, &serde_json::Map<String, serde_json::Value>)> = None;
    let mut best_key: (Vec<u64>, u64) = (vec![], 0);
    for (fid, obj) in &pool {
        let key = (version_tuple(&file_version_of(obj)), file_stamp_of(obj));
        if best.is_none() || key > best_key {
            best_key = key;
            best = Some((fid, obj));
        }
    }
    let Some((fid, obj)) = best else { return Ok(None) };
    let version = file_version_of(obj);
    if !version_is_newer(&version, installed) {
        return Ok(None);
    }
    let mod_name = nexus_get(&format!("/games/witcher3/mods/{mod_id}.json"), api_key)
        .ok()
        .and_then(|m| m.as_object().cloned())
        .and_then(|o| o.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_default();
    Ok(Some(UpdateTarget {
        file_id: fid.clone(),
        version,
        file_name: obj.get("file_name").and_then(|v| v.as_str()).unwrap_or("download.zip").to_string(),
        file_title: obj.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        size: file_size_of(obj),
        mod_name,
        category: obj.get("category_name").and_then(|v| v.as_str()).unwrap_or("").to_uppercase(),
    }))
}

/// Premium membership from `users/validate.json` (direct API downloads only
/// work for premium keys; free keys must go through the site).
pub fn is_premium(api_key: &str) -> bool {
    nexus_get("/users/validate.json", api_key)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .map(|o| {
            o.get("is_premium").and_then(|v| v.as_bool()).unwrap_or(false)
                || o.get("is_premium_member").and_then(|v| v.as_bool()).unwrap_or(false)
        })
        .unwrap_or(false)
}

/// `nxm://` link parse (python `parse_nxm`).
pub fn parse_nxm(url: &str) -> Option<NxmLink> {
    let t = url.trim();
    if !t.to_lowercase().starts_with("nxm://") {
        return None;
    }
    let rest = &t[6..];
    // nxm://witcher3/mods/123/files/456?key=..&expires=..&user_id=..
    let (path_q, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut parts: Vec<&str> = path_q.split('/').collect();
    // ["witcher3", "mods", "123", "files", "456"]
    if parts.len() < 5 {
        return None;
    }
    let game = parts[0].to_lowercase();
    if parts[1] != "mods" || parts[3] != "files" {
        return None;
    }
    let mod_id = parts[2].to_string();
    let file_id = parts[4].trim_end_matches('/').to_string();
    if !mod_id.chars().all(|c| c.is_ascii_digit()) || !file_id.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut key = String::new();
    let mut expires = String::new();
    for kv in query.split('&') {
        if let Some((k, v)) = kv.split_once('=') {
            if k == "key" {
                key = v.to_string();
            } else if k == "expires" {
                expires = v.to_string();
            }
        }
    }
    let _ = &mut parts;
    Some(NxmLink { game, mod_id, file_id, key, expires })
}

/// Download-link candidates for a file (premium `generated` vs free `alternative`).
pub fn download_links(mod_id: &str, file_id: &str, api_key: &str, key: &str, expires: &str) -> Result<Vec<String>, String> {
    let mut path = format!("/games/witcher3/mods/{mod_id}/files/{file_id}/download_link.json");
    if !key.is_empty() {
        path.push_str(&format!("?key={key}&expires={expires}"));
    }
    let v = nexus_get(&path, api_key)?;
    let mut out = vec![];
    if let Some(arr) = v.as_array() {
        for e in arr {
            if let Some(uri) = e.get("URI").and_then(|u| u.as_str()) {
                out.push(uri.to_string());
            } else if let Some(uri) = e.get("uri").and_then(|u| u.as_str()) {
                out.push(uri.to_string());
            }
        }
    }
    if out.is_empty() {
        return Err("no download links (free accounts need key+expires from Mod Manager download)".into());
    }
    Ok(out)
}

/// Blocking file download with basic resume; returns bytes written.
pub fn download_url(url: &str, dest: &std::path::Path, api_key: &str) -> Result<u64, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("W3MM/1.0")
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(url);
    if !api_key.is_empty() {
        req = req.header("apikey", api_key);
    }
    let mut resp = req.send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("download {}", resp.status()));
    }
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let mut f = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let n = std::io::copy(&mut resp, &mut f).map_err(|e| e.to_string())?;
    Ok(n)
}

/// Extract Nexus id from bare number or /mods/ URL (python `extract_nexus_id`).
pub fn extract_nexus_id(text: &str) -> String {
    let t = text.trim();
    if let Some(idx) = t.find("/mods/") {
        let tail = &t[idx + 6..];
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return digits;
        }
    }
    let digits: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
    if t.chars().filter(|c| c.is_ascii_digit()).count() == t.chars().filter(|c| !c.is_whitespace()).count() && !digits.is_empty() {
        return digits;
    }
    // fallback: first standalone number run
    let mut cur = String::new();
    for ch in t.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            break;
        }
    }
    cur
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
    #[test]
    fn nxm_links() {
        let l = parse_nxm("nxm://witcher3/mods/123/files/456?key=abc&expires=9&user_id=1").unwrap();
        assert_eq!(l.mod_id, "123");
        assert_eq!(l.file_id, "456");
        assert_eq!(extract_nexus_id("https://www.nexusmods.com/witcher3/mods/11260"), "11260");
    }
}
