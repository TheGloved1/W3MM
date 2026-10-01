//! Install pipeline: archive -> staging plan.
//! Ports `analyze/_find_roots/_plan_for/build_staging/planned_targets/
//! remove_junk` (`w3modmanager.py:1130-1305`).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallPlan {
    /// (source rel inside extracted tmp, target rel inside `staging/<id>/`)
    pub moves: Vec<(String, String)>,
    pub docs: Vec<String>,
}

fn has_game_files(d: &Path) -> bool {
    for sub in ["mods", "dlc", "bin", "content"] {
        if d.join(sub).is_dir() {
            return true;
        }
    }
    false
}

fn find_roots(tmp: &Path) -> Vec<PathBuf> {
    let mut roots = vec![];
    let mut stack = vec![tmp.to_path_buf()];
    let mut depth = 0;
    while let Some(d) = stack.pop() {
        if has_game_files(&d) {
            roots.push(d);
            continue;
        }
        if depth > 4 {
            continue;
        }
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push(e.path());
                }
            }
        }
        depth += 1;
    }
    if roots.is_empty() {
        vec![tmp.to_path_buf()]
    } else {
        roots
    }
}

/// Remove junk: __MACOSX, .DS_Store, Thumbs.db (python `remove_junk`).
pub fn remove_junk(root: &Path) {
    for e in walkdir::WalkDir::new(root).into_iter().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n == "__MACOSX" || n == ".DS_Store" || n.eq_ignore_ascii_case("Thumbs.db") {
            let p = e.path().to_path_buf();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

pub fn analyze(extracted: &Path) -> InstallPlan {
    remove_junk(extracted);
    let roots = find_roots(extracted);
    let mut moves = vec![];
    let mut docs = vec![];
    for root in roots {
        for e in walkdir::WalkDir::new(&root).into_iter().flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            let rel = e.path().strip_prefix(&root).unwrap_or(e.path()).to_string_lossy().to_string();
            let low = rel.to_lowercase();
            // Top-level grouping: mods|dlc|bin|content/...
            let first = rel.split(['/', '\\']).next().unwrap_or("").to_lowercase();
            if ["mods", "dlc", "bin"].contains(&first.as_str()) {
                moves.push((e.path().to_string_lossy().to_string(), rel));
            } else if crate::archive::is_doc_name(&low) {
                docs.push(e.path().to_string_lossy().to_string());
            } else if first == "content" {
                // Content-only drops (redkit assets) — stage under content/ as-is.
                moves.push((e.path().to_string_lossy().to_string(), rel));
            } else {
                // Single-folder mod without a mods/ wrapper: wrap later by caller.
                moves.push((e.path().to_string_lossy().to_string(), rel));
            }
        }
    }
    moves.sort();
    docs.sort();
    InstallPlan { moves, docs }
}

/// Copy plan sources into `staging/<id>/`, wrapping bare files under `mods/<mod>/`.
/// Returns target rels for state + doc rels.
pub fn build_staging(plan: &InstallPlan, stage: &Path, mod_folder: &str) -> Result<(Vec<String>, Vec<String>), String> {
    std::fs::create_dir_all(stage).map_err(|e| e.to_string())?;
    let mut targets = vec![];
    for (src, rel) in &plan.moves {
        let first = rel.split('/').next().unwrap_or("").to_lowercase();
        let target_rel = if ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
            rel.clone()
        } else {
            format!("mods/{mod_folder}/{rel}")
        };
        let dst = stage.join(&target_rel);
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        std::fs::copy(src, &dst).map_err(|e| e.to_string())?;
        targets.push(target_rel);
    }
    targets.sort();
    targets.dedup();
    let mut docs = vec![];
    let docs_dir = stage.join("docs");
    for src in &plan.docs {
        let name = Path::new(src).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or("readme.txt".into());
        std::fs::create_dir_all(&docs_dir).map_err(|e| e.to_string())?;
        let dst = docs_dir.join(&name);
        let _ = std::fs::copy(src, &dst);
        docs.push(format!("docs/{name}"));
    }
    Ok((targets, docs))
}
