//! Download queue with progress events, pause/cancel.
//! Replaces the original's `NexusDownload(QThread)` + `DownloadsPanel`
//! threading with Tauri events (`download-progress` / `download-done`);
//! the frontend renders the queue natively instead of Qt widgets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

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
    pub file_title: String,
    pub version: String,
    pub category: String,
    pub speed: u64, // bytes/sec
    pub added: i64, // unix timestamp
}

/// Lock the queue, recovering from poisoning (a panicking worker must not
/// brick every later queue command).
fn qlock() -> std::sync::MutexGuard<'static, HashMap<String, QueueItem>> {
    static Q: OnceLock<Mutex<HashMap<String, QueueItem>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn enqueue(url: &str) -> Result<String, String> {
    // Same file again (same Nexus mod + file): reuse the row like panel.add.
    // NOTE: plain `let` (not `if let`) for the lookup — a guard borrowed by
    // `if let` lives through the body, and re-locking inside deadlocks the
    // thread forever (std Mutex is not reentrant). That froze the whole app
    // on every repeat download of a known file.
    if url.to_lowercase().starts_with("nxm://") {
        if let Some(link) = crate::nexus::parse_nxm(url) {
            if link.game != "witcher3" {
                return Err(format!("wrong game: {}", link.game));
            }
            let same = qlock().values().find(|i| i.mod_id == link.mod_id && i.file_id == link.file_id).cloned();
            if let Some(same) = same {
                if same.status == "active" || same.status == "starting" || same.status == "queued" || same.status == "paused" {
                    return Ok(same.id); // already fetching: caller just opens the panel
                }
                // finished/failed row reused: refresh one-time key, requeue
                if let Some(it) = qlock().get_mut(&same.id) {
                    it.url = url.to_string();
                    it.status = "queued".into();
                    it.error = String::new();
                    it.added = chrono::Utc::now().timestamp();
                }
                return Ok(same.id);
            }
        } else {
            return Err("not an nxm:// link".into());
        }
    }
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let (mod_id, file_id) = match crate::nexus::parse_nxm(url) {
        Some(link) => (link.mod_id, link.file_id),
        None => (String::new(), String::new()),
    };
    // No network here: metadata resolves on the worker thread (original
    // spawns NexusDownload first, meta arrives via the info signal).
    qlock().insert(
        id.clone(),
        QueueItem {
            id: id.clone(),
            url: url.to_string(),
            filename: String::new(),
            total: 0,
            done: 0,
            status: "queued".into(),
            error: String::new(),
            mod_id,
            file_id,
            mod_name: String::new(),
            file_title: String::new(),
            version: String::new(),
            category: String::new(),
            speed: 0,
            added: chrono::Utc::now().timestamp(),
        },
    );
    Ok(id)
}

/// Enqueue an already-resolved Nexus file (one-click updates): metadata is
/// known up front, so no network happens here. The row streams through the
/// normal worker path via its `nexus://` URL.
pub fn enqueue_resolved(mod_id: &str, target: &crate::nexus::UpdateTarget) -> String {
    // Same file already queued/finished: reuse the row instead of doubling it.
    let same = qlock()
        .values()
        .find(|i| i.mod_id == mod_id && i.file_id == target.file_id)
        .cloned();
    if let Some(same) = same {
        if let Some(it) = qlock().get_mut(&same.id) {
            it.status = "queued".into();
            it.error = String::new();
            it.added = chrono::Utc::now().timestamp();
        }
        return same.id;
    }
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    qlock().insert(
        id.clone(),
        QueueItem {
            id: id.clone(),
            url: format!("nexus://witcher3/mods/{mod_id}/files/{}", target.file_id),
            filename: target.file_name.clone(),
            total: target.size,
            done: 0,
            status: "queued".into(),
            error: String::new(),
            mod_id: mod_id.to_string(),
            file_id: target.file_id.clone(),
            mod_name: target.mod_name.clone(),
            file_title: target.file_title.clone(),
            version: target.version.clone(),
            category: target.category.clone(),
            speed: 0,
            added: chrono::Utc::now().timestamp(),
        },
    );
    id
}

/// Claim a row for a worker thread. False when already running.
pub fn try_begin(id: &str) -> bool {
    if let Some(it) = qlock().get_mut(id) {
        match it.status.as_str() {
            "active" | "starting" => false,
            _ => {
                it.status = "starting".into();
                it.error = String::new();
                true
            }
        }
    } else {
        false
    }
}

/// Resolve Nexus metadata for an nxm row (worker thread; original info.emit).
pub fn fetch_meta(id: &str, api_key: &str) -> Result<(), String> {
    let url = qlock()
        .get(id)
        .map(|i| i.url.clone())
        .ok_or("unknown download")?;
    let link = crate::nexus::parse_nxm(&url).ok_or("invalid nxm")?;
    let (fname, ftitle, size, mname, ver, cat) = nxm_meta(&link, api_key)?;
    if let Some(it) = qlock().get_mut(id) {
        it.filename = fname;
        it.file_title = ftitle;
        it.total = size;
        it.mod_name = mname;
        it.version = ver;
        it.category = cat;
    }
    Ok(())
}

/// Best-effort (file_name, file_title, size_bytes, mod_name, version, category) for an nxm link.
fn nxm_meta(link: &crate::nexus::NxmLink, api_key: &str) -> Result<(String, String, u64, String, String, String), String> {
    let f = crate::nexus::nexus_get(
        &format!("/games/witcher3/mods/{}/files/{}.json", link.mod_id, link.file_id),
        api_key,
    )
    .map_err(|e| {
        let t = e.to_string();
        if t.contains("404") {
            format!("Nexus doesn't have that file (it may have been removed).")
        } else {
            t
        }
    })?;
    let m = crate::nexus::nexus_get(
        &format!("/games/witcher3/mods/{}.json", link.mod_id),
        api_key,
    ).ok();
    let mut name = String::new();
    let mut version = String::new();
    let mut total = 0u64;
    let mut file_title = String::new();
    let mut category = String::new();
    if let Some(f) = f.as_object() {
        name = f
            .get("file_name")
            .and_then(|v| v.as_str())
            .unwrap_or("download.zip")
            .to_string();
        file_title = f
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        version = crate::nexus::clean_version(
            f.get("version")
                .or_else(|| f.get("mod_version"))
                .and_then(|v| v.as_str())
                .unwrap_or(""),
        );
        category = f
            .get("category_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_uppercase();
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
    Ok((name, file_title, total, mod_name, version, category))
}

pub fn items() -> Vec<QueueItem> {
    let mut v: Vec<QueueItem> = qlock().values().cloned().collect();
    v.sort_by(|a, b| b.added.cmp(&a.added));
    v
}

/// downloads.json next to the downloads folder (python DownloadsPanel history).
pub fn save_history(path: &std::path::Path) {
    let rows: Vec<QueueItem> = qlock().values().cloned().collect();
    let data = serde_json::to_string_pretty(&rows).unwrap_or_default();
    if data.is_empty() {
        return;
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, &data).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Bring back rows from last time. Finished rows whose file is gone are left
/// out; anything mid-flight comes back as failed with its .part kept for Retry.
pub fn load_history(path: &std::path::Path, dest_dir: &std::path::Path) {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    if text.is_empty() {
        return;
    }
    let records: Vec<QueueItem> = serde_json::from_str(&text).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut q = qlock();
    for mut r in records {
        if r.mod_id.is_empty() && r.file_id.is_empty() && r.filename.is_empty() {
            continue;
        }
        let key = (r.mod_id.clone(), r.file_id.clone());
        if !seen.insert(key) {
            continue; // duplicate: newest one stays
        }
        if r.status == "done" {
            let p = if r.filename.is_empty() { None } else { Some(dest_dir.join(&r.filename)) };
            if p.as_ref().map(|p| p.is_file()).unwrap_or(false) {
                q.insert(r.id.clone(), r);
            }
            // else: file deleted outside the app — leave it out
        } else {
            r.status = "error".into();
            if r.error.is_empty() {
                r.error = "Stopped. Retry continues where it left off.".into();
            }
            r.speed = 0;
            q.insert(r.id.clone(), r);
        }
    }
}

pub fn cancel(id: &str) {
    if let Some(it) = qlock().get_mut(id) {
        if it.status == "queued" || it.status == "starting" || it.status == "active" || it.status == "paused" {
            it.status = "cancelled".into();
        }
    }
}

/// Mark a row failed with a message (worker-thread terminal state).
pub fn fail(id: &str, err: &str) {
    if let Some(it) = qlock().get_mut(id) {
        it.status = "error".into();
        it.error = err.to_string();
        it.speed = 0;
    }
}

pub fn remove(id: &str) {
    qlock().remove(id);
}

pub fn set_paused(id: &str, paused: bool) {
    if let Some(it) = qlock().get_mut(id) {
        if paused && it.status == "active" {
            it.status = "paused".into();
        } else if !paused && it.status == "paused" {
            it.status = "queued".into();
        }
    }
}

/// Blocking file fetch for one row (runs on a worker thread; replaces
/// QThread signals with `emit` progress callbacks).
pub fn pump_file(id: &str, dest: &std::path::Path, api_key: &str, emit: &dyn Fn(u64, u64, u64)) -> Result<u64, String> {
    let item = {
        let mut q = qlock();
        let it = q.get_mut(id).ok_or("unknown download")?;
        if it.status == "cancelled" {
            return Err("cancelled".into());
        }
        if it.filename.is_empty() {
            return Err("no file yet — still asking Nexus".into());
        }
        it.status = "active".into();
        it.clone()
    };
    let url = if item.url.to_lowercase().starts_with("nxm://") {
        let link = crate::nexus::parse_nxm(&item.url).ok_or("invalid nxm")?;
        let links = crate::nexus::download_links(&link.mod_id, &link.file_id, api_key, &link.key, &link.expires)
            .map_err(|e| {
                let t = e.to_string();
                if t.contains("API key") {
                    "Nexus refused the download link. Links from the website expire after a while — click “Mod Manager Download” again.".to_string()
                } else {
                    t
                }
            })?;
        links.into_iter().next().ok_or("Nexus didn't give a download address for this file.".to_string())?
    } else if let Some(rest) = item.url.strip_prefix("nexus://") {
        // One-click update rows: mod/file ids straight from the API, no
        // one-time browser key. Free accounts may be refused here — the row
        // then shows the reason and keeps Retry.
        let mut parts = rest.split('/');
        let game = parts.next().unwrap_or("");
        let mod_id = parts.nth(1).unwrap_or("");
        let file_id = parts.nth(1).unwrap_or("");
        if game != "witcher3" || mod_id.is_empty() || file_id.is_empty() {
            return Err("invalid nexus link".into());
        }
        let links = crate::nexus::download_links(mod_id, file_id, api_key, "", "")
            .map_err(|e| format!("Nexus refused a direct download link ({e}). Premium accounts can retry; free accounts use “Mod Manager Download” on the mod page instead."))?;
        links.into_iter().next().ok_or("Nexus didn't give a download address for this file.".to_string())?
    } else {
        item.url.clone()
    };
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    // Already downloaded earlier: same size as Nexus said — nothing to fetch.
    if item.total > 0 {
        if let Ok(md) = std::fs::metadata(dest) {
            if md.len() == item.total {
                if let Some(it) = qlock().get_mut(id) {
                    it.status = "done".into();
                    it.done = item.total;
                }
                return Ok(item.total);
            }
        }
    }
    // Resume: .part file from a stopped download continues where it left off.
    let part = dest.with_extension("part");
    let mut have = if part.is_file() { std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) } else { 0 };
    let client = reqwest::blocking::Client::builder()
        .user_agent("W3MM/1.0")
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.header("apikey", api_key);
    }
    if have > 0 {
        req = req.header("Range", format!("bytes={have}-"));
    }
    let mut resp = req.send().map_err(|e| format!("Couldn't reach the download server: {e}"))?;
    if resp.status() == 416 && have > 0 {
        // the partial file is already complete
        std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
        let done = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(have);
        if let Some(it) = qlock().get_mut(id) {
            it.status = "done".into();
            it.done = done;
            if it.total == 0 {
                it.total = done;
            }
        }
        return Ok(done);
    }
    if !resp.status().is_success() {
        let e = format!("The download server returned HTTP {}.", resp.status());
        if let Some(it) = qlock().get_mut(id) {
            it.status = "error".into();
            it.error = e.clone();
        }
        return Err(e);
    }
    let fallback_total = if item.total > 0 { item.total } else { 0 };
    let total = resp.content_length().unwrap_or(0);
    let total = if have > 0 && resp.status() == 206 { have + total } else { if total == 0 { fallback_total } else { total } };
    {
        if let Some(it) = qlock().get_mut(id) {
            it.total = total;
        }
    }
    if have > 0 && resp.status() != 206 {
        have = 0; // the server started over
    }
    let mut f = if have > 0 {
        std::fs::OpenOptions::new().append(true).open(&part).map_err(|e| e.to_string())?
    } else {
        std::fs::File::create(&part).map_err(|e| e.to_string())?
    };
    let mut buf = [0u8; 1 << 16];
    let start = std::time::Instant::now();
    let mut done = have;
    if let Some(it) = qlock().get_mut(id) {
        it.done = done;
    }
    let mut win: std::collections::VecDeque<(std::time::Instant, u64)> = std::collections::VecDeque::new();
    win.push_back((start, done));
    // Like the original (progress.emit at most every 0.2s): emitting per
    // chunk floods the webview with re-renders and freezes the main window.
    let mut last_emit = start;
    use std::io::Read;
    loop {
        {
            let q = qlock();
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
                let now = std::time::Instant::now();
                win.push_back((now, done));
                while win.len() > 2 && now.duration_since(win[0].0).as_secs_f32() > 3.0 {
                    win.pop_front();
                }
                let speed = if win.len() >= 2 {
                    let dt = now.duration_since(win[0].0).as_secs_f32();
                    if dt > 0.0 { ((done - win[0].1) as f32 / dt) as u64 } else { 0 }
                } else { 0 };
                if let Some(it) = qlock().get_mut(id) {
                    it.done = done;
                    it.speed = speed;
                }
                if now.duration_since(last_emit).as_secs_f32() >= 0.2 {
                    last_emit = now;
                    emit(done, total, speed);
                }
            }
            Err(e) => return Err(format!("The download stopped: {e}")),
        }
    }
    if total > 0 && done < total {
        let e = "The download ended early — Retry picks up where it stopped.".to_string();
        if let Some(it) = qlock().get_mut(id) {
            it.status = "error".into();
            it.error = e.clone();
        }
        return Err(e);
    }
    if let Some(it) = qlock().get_mut(id) {
        it.status = "done".into();
        it.done = done;
        let elapsed = start.elapsed().as_secs().max(1);
        it.speed = done / elapsed;
    }
    emit(done, total, 0);
    if part != dest {
        let _ = std::fs::rename(&part, dest);
    }
    Ok(done)
}
