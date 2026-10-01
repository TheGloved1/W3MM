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
}

type Queue = Arc<Mutex<HashMap<String, QueueItem>>>;

fn queue() -> Queue {
    static Q: OnceLock<Queue> = OnceLock::new();
    Q.get_or_init(|| Arc::new(Mutex::new(HashMap::new()))).clone()
}

pub fn enqueue(url: &str, filename: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    queue().lock().unwrap().insert(
        id.clone(),
        QueueItem { id: id.clone(), url: url.to_string(), filename: filename.to_string(), total: 0, done: 0, status: "queued".into(), error: String::new() },
    );
    id
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
    {
        let qq = queue();
        let mut q = qq.lock().unwrap();
        let it = q.get_mut(id).ok_or("unknown download")?;
        if it.status == "cancelled" {
            return Err("cancelled".into());
        }
        it.status = "active".into();
    }
    let client = reqwest::blocking::Client::builder().user_agent("W3LMN/1.0").build().map_err(|e| e.to_string())?;
    let mut req = client.get(&items().into_iter().find(|i| i.id == id).map(|i| i.url).unwrap_or_default());
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
    let total = resp.content_length().unwrap_or(0);
    {
        if let Some(it) = queue().lock().unwrap().get_mut(id) {
            it.total = total;
        }
    }
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let mut f = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 1 << 16];
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
    }
    Ok(done)
}
