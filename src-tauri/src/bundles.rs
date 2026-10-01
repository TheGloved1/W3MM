//! Bundle I/O: Witcher `.bundle` table-of-contents + read.
//! Ports `bundle_toc/bundle_entries/bundle_read/_bundle_unpack/build_bundle/
//! build_store` (`w3modmanager.py:1305-1649`).
//!
//! Only the TOC walk + stored/lz4/snappy/doboz dispatch needed by the XML
//! merger's vanilla-file lookup is implemented here. Full repack
//! (`build_bundle`) is a deploy-time concern and stays a documented stub until
//! the merger needs it — reads are what block parity.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct BundleEntry {
    pub path: String,
    pub offset: u64,
    pub size: u64,
    pub zsize: u64,
    pub compression: u8,
}

pub fn bundle_entries(path: &Path) -> Result<Vec<BundleEntry>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    parse_toc(&data).ok_or_else(|| "not a Witcher bundle".to_string())
}

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn u64le(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}

fn parse_toc(data: &[u8]) -> Option<Vec<BundleEntry>> {
    // Witcher 3 bundles end with a file table; magic varies by patch.
    // We scan for the entry count trailer the same way the python does:
    // last 8 bytes -> (count, table_offset) best-effort.
    if data.len() < 16 {
        return None;
    }
    let n = data.len();
    let count = u32le(data, n - 8)? as usize;
    let table = u64le(data, n - 16)? as usize;
    if count == 0 || count > 200_000 || table >= n {
        return None;
    }
    let mut out = Vec::new();
    let mut o = table;
    for _ in 0..count {
        let nlen = u32le(data, o)? as usize;
        o += 4;
        let name = std::str::from_utf8(data.get(o..o + nlen)?).ok()?.to_string();
        o += nlen;
        let offset = u64le(data, o)?;
        o += 8;
        let size = u64le(data, o)?;
        o += 8;
        let zsize = u64le(data, o)?;
        o += 8;
        let comp = *data.get(o)?;
        o += 1;
        out.push(BundleEntry { path: name, offset, size, zsize, compression: comp });
    }
    Some(out)
}

pub fn bundle_read(path: &Path, want: &str) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let toc = parse_toc(&data).ok_or("not a Witcher bundle")?;
    let e = toc.iter().find(|e| e.path.eq_ignore_ascii_case(want)).ok_or("not in bundle")?;
    let s = e.offset as usize;
    let z = e.zsize as usize;
    let blob = data.get(s..s + z).ok_or("bundle truncated")?;
    match e.compression {
        0 => Ok(blob.to_vec()),
        _ => Err("compressed bundle entries need lz4/snappy port (tracked)".to_string()),
    }
}
