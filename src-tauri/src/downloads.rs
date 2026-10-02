//! Download queue with progress events, pause/cancel.
//! Replaces the original's `NexusDownload(QThread)` + `DownloadsPanel`
//! threading with Tauri events (`download-progress` / `download-done`);
//! the frontend renders the queue natively instead of Qt widgets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub total: u64,
    pub done: u64,
    pub status: String, // queued|active|paused|done|error|cancelled
    pub error: String,
    // Nexus-specific metadata when the URL is an nxm:// link
    pub mod_id: String,
    pub file_id: String,
    pub mod_name: String,
    pub version: String,
    pub category: String,
    pub speed: u64, // bytes/sec
}

type Queue = Arc<Mutex<HashMap<String, QueueItem>>>;

fn queue() -> Queue {
    static Q: OnceLock<Queue> = OnceLock::new();
    Q.get_or_init(|| Arc::new(Mutex::new(HashMap::new()))).clone()
}

pub fn enqueue(url: &str, filename: &str, api_key: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let mut mod_id = String::new();
    let mut file_id = String::new();
    let mut version = String::new();
    let mut mod_name = String::new();
    let mut final_filename = filename.to_string();
    let mut total = 0u64;
    if url.to_lowercase().starts_with("nxm://") {
        if let Some(link) = crate::nexus::parse_nxm(url) {
            mod_id = link.mod_id.clone();
            file_id = link.file_id.clone();
            // resolve metadata now so the row has a useful name and size
            if let Ok((fname, size, mname, ver)) = nxm_meta(&link, api_key) {
                final_filename = fname;
                total = size;
                mod_name = mname;
                version = ver;
            }
        }
    }
    queue().lock().unwrap().insert(
        id.clone(),
        QueueItem {
            id: id.clone(),
            url: url.to_string(),
            filename: final_filename,
            total,
            done: 0,
            status: "queued".into(),
            error: String::new(),
            mod_id,
            file_id,
            mod_name,
            version,
            category: String::new(),
            speed: 0,
        },
    );
    id
}

/// Best-effort (file_name, size_bytes, mod_name, version) for an nxm link.
fn nxm_meta(link: &crate::nexus::NxmLink, api_key: &str) -> Result<(String, u64, String, String), String> {
    let f = crate::nexus::nexus_get(
        &format!("/games/witcher3/mods/{}/files/{}.json", link.mod_id, link.file_id),
        api_key,
    )?;
    let m = crate::nexus::nexus_get(
        &format!("/games/witcher3/mods/{}.json", link.mod_id),
        api_key,
    ).ok();
    let mut name = String::new();
    let mut version = String::new();
    let mut total = 0u64;
    if let Some(f) = f.as_object() {
        name = f
            .get("file_name")
            .and_then(|v| v.as_str())
            .unwrap_or("download.zip")
            .to_string();
        version = f
            .get("version")
            .or_else(|| f.get("mod_version"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(size) = f.get("size_in_bytes").and_then(|v| v.as_u64()) {
            total = size;
        } else if let Some(kb) = f.get("size_kb").and_then(|v| v.as_u64()) {
            total = kb * 1024;
        } else if let Some(sz) = f.get("size").and_then(|v| v.as_u64()) {
            total = sz * 1024;
        }
    }
    let mod_name = m
        .as_ref()
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.get("name").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    Ok((name, total, mod_name, version))
}

pub fn items() -> Vec<QueueItem> {
    let mut v: Vec<QueueItem> = queue().lock().unwrap().values().cloned().collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

pub fn cancel(id: &str) {
    if let Some(it) = queue().lock().unwrap().get_mut(id) {
        if it.status == "queued" || it.status == "active" || it.status == "paused" {
            it.status = "cancelled".into();
        }
    }
}

pub fn set_paused(id: &str, paused: bool) {
    if let Some(it) = queue().lock().unwrap().get_mut(id) {
        if paused && it.status == "active" {
            it.status = "paused".into();
        } else if !paused && it.status == "paused" {
            it.status = "queued".into();
        }
    }
}

/// Blocking pump for one item; emits progress through `emit`. Cancellation
/// and pause are polled between chunks (replaces QThread signals).
pub fn pump(id: &str, dest: &std::path::Path, api_key: &str, emit: &dyn Fn(u64, u64)) -> Result<u64, String> {
    let item = {
        let qq = queue();
        let mut q = qq.lock().unwrap();
        let it = q.get_mut(id).ok_or("unknown download")?;
        if it.status == "cancelled" {
            return Err("cancelled".into());
        }
        it.status = "active".into();
        it.clone()
    };
    let url = if item.url.to_lowercase().starts_with("nxm://") {
        let link = crate::nexus::parse_nxm(&item.url).ok_or("invalid nxm")?;
        let links = crate::nexus::download_links(&link.mod_id, &link.file_id, api_key, &link.key, &link.expires)?;
        links.into_iter().next().ok_or("no download links")?
    } else {
        item.url.clone()
    };
    let client = reqwest::blocking::Client::builder().user_agent("W3LMN/1.0").build().map_err(|e| e.to_string())?;
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.header("apikey", api_key);
    }
    let mut resp = req.send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let e = format!("download {}", resp.status());
        if let Some(it) = queue().lock().unwrap().get_mut(id) {
            it.status = "error".into();
            it.error = e.clone();
        }
        return Err(e);
    }
    let fallback_total = if item.total > 0 { item.total } else { 0 };
    let total = resp.content_length().unwrap_or(fallback_total);
    {
        if let Some(it) = queue().lock().unwrap().get_mut(id) {
            it.total = total;
        }
    }
    let part = dest.with_extension("part");
    if let Some(p) = part.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 1 << 16];
    let start = std::time::Instant::now();
    let mut done = 0u64;
    use std::io::Read;
    loop {
        {
            let qq = queue();
            let q = qq.lock().unwrap();
            match q.get(id).map(|i| i.status.clone()).as_deref() {
                Some("cancelled") => return Err("cancelled".into()),
                Some("paused") => {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }
                _ => {}
            }
        }
        match resp.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                use std::io::Write;
                f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                done += n as u64;
                if let Some(it) = queue().lock().unwrap().get_mut(id) {
                    it.done = done;
                }
                emit(done, total);
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    if let Some(it) = queue().lock().unwrap().get_mut(id) {
        it.status = "done".into();
        it.done = done;
        let elapsed = start.elapsed().as_secs().max(1);
        it.speed = done / elapsed;
    }
    if part != dest {
        let _ = std::fs::rename(&part, dest);
    }
    Ok(done)
}
