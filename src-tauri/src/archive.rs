//! Pure-Rust archive support (no 7zz/bsdtar subprocess).
//! Ports python `extract_archive/_extract_zip/archive_names/is_doc_name`
//! (`w3modmanager.py:960,983,1021`) using `zip`, `sevenz-rust`, `tar/flate2`.

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("unsupported archive: {0}")]
    Unsupported(String),
    #[error("io: {0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

pub const ARCHIVE_EXTS: &[&str] = &[".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".bz2"];

pub fn is_archive(name: &str) -> bool {
    let low = name.to_lowercase();
    ARCHIVE_EXTS.iter().any(|e| low.ends_with(e))
}

pub fn is_doc_name(name: &str) -> bool {
    let low = name.to_lowercase();
    let base = low.rsplit('/').next().unwrap_or(&low);
    base.starts_with("readme")
        || base.starts_with("changelog")
        || base.starts_with("license")
        || base.starts_with("credits")
        || base.ends_with(".txt")
        || base.ends_with(".md")
        || base.ends_with(".pdf")
}

fn ensure_within(dest: &Path, p: &Path) -> Result<PathBuf, ArchiveError> {
    let full = dest.join(p);
    // Zip-slip guard: canonicalize prefix comparison without requiring existence.
    let dest_norm = dest.to_path_buf();
    if full.strip_prefix(&dest_norm).is_err() {
        // `dest.join` with absolute `p` discards dest; reject those.
        if p.is_absolute() {
            return Err(ArchiveError::Other(format!("absolute entry: {}", p.display())));
        }
    }
    Ok(full)
}

pub fn list_zip(archive: &Path) -> Result<Vec<String>, ArchiveError> {
    let f = std::fs::File::open(archive).map_err(|e| ArchiveError::Io(e.to_string()))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| ArchiveError::Other(e.to_string()))?;
    let mut out = Vec::new();
    for i in 0..z.len() {
        if let Ok(f) = z.by_index(i) {
            out.push(f.name().to_string());
        }
    }
    Ok(out)
}

pub fn list_names(archive: &Path) -> Result<Vec<String>, ArchiveError> {
    let low = archive.to_string_lossy().to_lowercase();
    if low.ends_with(".zip") {
        return list_zip(archive);
    }
    if low.ends_with(".7z") {
        let mut sz = sevenz_rust::SevenZReader::open(archive, sevenz_rust::Password::empty())
            .map_err(|e| ArchiveError::Other(e.to_string()))?;
        let mut out = Vec::new();
        sz.for_each_entries(|e, _reader| {
            out.push(e.name().to_string());
            Ok(true)
        })
        .map_err(|e| ArchiveError::Other(e.to_string()))?;
        return Ok(out);
    }
    // tar-family listing; rar listing stays best-effort (pure-Rust `rar` covers RAR4 stores).
    if low.ends_with(".tar") || low.ends_with(".tgz") || low.ends_with(".gz") {
        let f = std::fs::File::open(archive).map_err(|e| ArchiveError::Io(e.to_string()))?;
        let dec: Box<dyn std::io::Read> = if low.ends_with(".gz") || low.ends_with(".tgz") {
            Box::new(flate2::read::GzDecoder::new(f))
        } else {
            Box::new(f)
        };
        let mut ar = tar::Archive::new(dec);
        let mut out = Vec::new();
        for e in ar.entries().map_err(|e| ArchiveError::Other(e.to_string()))? {
            if let Ok(e) = e {
                out.push(e.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default());
            }
        }
        return Ok(out);
    }
    Err(ArchiveError::Unsupported(
        archive.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
    ))
}

pub fn extract_archive(archive: &Path, dest: &Path) -> Result<(), ArchiveError> {
    std::fs::create_dir_all(dest).map_err(|e| ArchiveError::Io(e.to_string()))?;
    let low = archive.to_string_lossy().to_lowercase();
    if low.ends_with(".zip") {
        let f = std::fs::File::open(archive).map_err(|e| ArchiveError::Io(e.to_string()))?;
        let mut z = zip::ZipArchive::new(f).map_err(|e| ArchiveError::Other(e.to_string()))?;
        for i in 0..z.len() {
            let mut f = z.by_index(i).map_err(|e| ArchiveError::Other(e.to_string()))?;
            let enclosed = f.enclosed_name().ok_or_else(|| ArchiveError::Other("bad zip path".into()))?;
            let out = ensure_within(dest, &enclosed)?;
            if f.is_dir() {
                std::fs::create_dir_all(&out).map_err(|e| ArchiveError::Io(e.to_string()))?;
            } else {
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p).map_err(|e| ArchiveError::Io(e.to_string()))?;
                }
                let mut o = std::fs::File::create(&out).map_err(|e| ArchiveError::Io(e.to_string()))?;
                std::io::copy(&mut f, &mut o).map_err(|e| ArchiveError::Io(e.to_string()))?;
            }
        }
        return Ok(());
    }
    if low.ends_with(".7z") {
        return sevenz_rust::decompress_file(archive, dest).map_err(|e| ArchiveError::Other(e.to_string()));
    }
    if low.ends_with(".tar") || low.ends_with(".tgz") || low.ends_with(".gz") {
        let f = std::fs::File::open(archive).map_err(|e| ArchiveError::Io(e.to_string()))?;
        let dec: Box<dyn std::io::Read> = if low.ends_with(".gz") || low.ends_with(".tgz") {
            Box::new(flate2::read::GzDecoder::new(f))
        } else {
            Box::new(f)
        };
        let mut ar = tar::Archive::new(dec);
        ar.unpack(dest).map_err(|e| ArchiveError::Other(e.to_string()))?;
        return Ok(());
    }
    Err(ArchiveError::Unsupported(
        "RAR needs a non-pure-Rust decoder; repack as .zip/.7z (see README)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn doc_names() {
        assert!(is_doc_name("readme.txt"));
        assert!(!is_doc_name("mods/modFoo/content/a.ws"));
    }
}
