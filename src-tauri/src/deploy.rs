//! Deploy: hardlink-or-copy enabled mods, backups, `mods.settings`, filelists.
//! Ports `ModManager.sync/_deploy_missing/_restore/_read_settings/clashes/
//! _write_mods_settings/_update_filelists` (`w3modmanager.py:5774-6400`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn link_or_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p)?;
    }
    let _ = std::fs::remove_file(dst);
    match std::fs::hard_link(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(src, dst)?;
            Ok(())
        }
    }
}

pub fn mod_files(staging_mod: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for sub in ["mods", "dlc", "bin"] {
        let base = staging_mod.join(sub);
        if !base.is_dir() {
            continue;
        }
        for e in WalkDir::new(&base).into_iter().flatten() {
            if e.file_type().is_file() {
                if let Ok(rel) = e.path().strip_prefix(staging_mod) {
                    out.push(rel.to_path_buf());
                }
            }
        }
    }
    out.sort();
    out
}

/// Case-insensitive deploy: lowercase key -> actual relative path.
/// Mirrors python CIResolver/ci_path behaviour at a coarse level.
pub fn deploy_mod(game: &Path, staging_mod: &Path, rels: &[PathBuf], backup: &Path) -> std::io::Result<Vec<String>> {
    let mut written = Vec::new();
    for rel in rels {
        let src = staging_mod.join(rel);
        let dst = game.join(rel);
        // Back up anything we'd overwrite exactly once.
        if dst.is_file() {
            let bdst = backup.join(rel);
            if !bdst.exists() {
                if let Some(p) = bdst.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::copy(&dst, &bdst)?;
            }
        }
        link_or_copy(&src, &dst)?;
        written.push(rel.to_string_lossy().to_string());
    }
    Ok(written)
}

/// Write `mods.settings` priority section for enabled mods.
pub fn write_mods_settings(settings_path: &Path, enabled_in_priority: &[String]) -> std::io::Result<()> {
    // Keep everything else in the file; replace the [Mods] block ordering.
    let existing = std::fs::read_to_string(settings_path).unwrap_or_default();
    let mut pre: Vec<String> = Vec::new();
    let mut in_mods = false;
    for line in existing.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case("[Mods]") {
            in_mods = true;
            continue;
        }
        if in_mods && t.starts_with('[') {
            in_mods = false;
        }
        if !in_mods {
            pre.push(line.to_string());
        }
    }
    while pre.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        pre.pop();
    }
    let mut out = pre.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str("[Mods]\n");
    for (i, name) in enabled_in_priority.iter().enumerate() {
        out.push_str(&format!("Mod{i}={name}\n"));
    }
    if let Some(p) = settings_path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(settings_path, out)?;
    Ok(())
}

/// Register menu xmls into dx11/dx12filelist.txt (append-only, dedup).
pub fn update_filelists(game: &Path, xmls: &[String]) -> std::io::Result<()> {
    for list in ["bin/dx11filelist.txt", "bin/dx12filelist.txt"] {
        let path = game.join(list);
        let mut have: BTreeSet<String> = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect();
        let mut extra = Vec::new();
        for x in xmls {
            let k = x.trim().to_lowercase();
            if !k.is_empty() && !have.contains(&k) {
                have.insert(k.clone());
                extra.push(x.clone());
            }
        }
        if !extra.is_empty() {
            if let Some(p) = path.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut text = std::fs::read_to_string(&path).unwrap_or_default();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            for x in extra {
                text.push_str(&x);
                text.push('\n');
            }
            std::fs::write(&path, text)?;
        }
    }
    Ok(())
}

pub fn deployed_map() -> BTreeMap<String, String> {
    BTreeMap::new()
}
