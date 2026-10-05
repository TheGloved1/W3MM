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

/// A bare mod folder like `modSlimmer_Griffin_Armor/content/...`: the archive
/// ships the mod itself, not the game layout. Its single `content/` child
/// would otherwise be read as the game content dir (kind Content → files land
/// in the game's own `content/`), so it must be wrapped as a mod instead.
/// Real top-level content drops still qualify: they sit next to `bin`/
/// mods/dlc, or at the extraction root.
fn is_mod_folder(root: &Path, extracted: &Path) -> bool {
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if !name.to_lowercase().starts_with("mod") {
        return false;
    }
    if root == extracted {
        return false;
    }
    // Only a lone content/ (no sibling game dirs) — that's a mod folder.
    !root.join("mods").is_dir() && !root.join("dlc").is_dir() && !root.join("bin").is_dir()
}

/// Rewrite a bare mod folder's files as if the folder were spelled
/// `mods/<folder>` — e.g. `modFoo/content/x.ws` becomes
/// `mods/modFoo/content/x.ws`. Everything downstream (preview kind, install
/// remap, staging) then treats it exactly like a wrapped archive.
fn qualify_mod_folder(root: &Path, rel: &str) -> String {
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if name.is_empty() {
        return rel.to_string();
    }
    format!("mods/{name}/{rel}")
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
        // `modFoo/content/...` is a mod folder, not the game content dir.
        let mod_folder_root = is_mod_folder(&root, extracted);
        for e in walkdir::WalkDir::new(&root).into_iter().flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            let rel = e.path().strip_prefix(&root).unwrap_or(e.path()).to_string_lossy().to_string();
            let rel = rel.trim_end_matches('/').trim_end_matches('\\').to_string();
            let low = rel.to_lowercase();
            // Top-level grouping: mods|dlc|bin|content/...
            let first = rel.split(['/', '\\']).next().unwrap_or("").to_lowercase();
            let rel = if mod_folder_root && first == "content" {
                qualify_mod_folder(&root, &rel)
            } else {
                rel
            };
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
    crate::log_line("rust", &format!("build_staging: stage={} mod_folder={} moves={}", stage.display(), mod_folder, plan.moves.len()));
    for (src, rel) in &plan.moves {
        let rel_trim = rel.trim_end_matches('/').trim_end_matches('\\');
        let first = rel_trim.split('/').next().unwrap_or("").to_lowercase();
        let target_rel = if ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
            rel_trim.to_string()
        } else {
            format!("mods/{mod_folder}/{}", rel_trim)
        };
        let dst = stage.join(&target_rel);
        crate::log_line("rust", &format!("build_staging: copy src='{}' rel='{}' target_rel='{}' dst='{}'", src, rel, target_rel, dst.display()));
        if let Some(p) = dst.parent() {
            crate::log_line("rust", &format!("build_staging: ensure parent '{}'", p.display()));
            std::fs::create_dir_all(p).map_err(|e| {
                crate::log_line("rust", &format!("build_staging: create_dir_all failed for '{}': {}", p.display(), e));
                e.to_string()
            })?;
        }
        if dst.exists() {
            crate::log_line("rust", &format!("build_staging: dst exists '{}' is_dir={}", dst.display(), dst.is_dir()));
        }
        std::fs::copy(src, &dst).map_err(|e| {
            crate::log_line("rust", &format!("build_staging: copy failed src='{}' dst='{}' error='{}'", src, dst.display(), e));
            e.to_string()
        })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("w3mm-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn write(rel: &Path, body: &str) {
        if let Some(d) = rel.parent() {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(rel, body).unwrap();
    }

    #[test]
    fn bare_mod_folder_with_content_installs_as_mod() {
        // modSlimmer_Griffin_Armor/content/... must stage under
        // mods/<mod>/content/..., not the game's own content/.
        let tmp = tmpdir("bare-mod");
        let root = tmp.join("modSlimmer_Griffin_Armor");
        write(&root.join("content/script.test.ws"), "x");
        write(&root.join("content/script.test.xml"), "y");
        let plan = analyze(&tmp);
        assert!(
            plan.moves.iter().all(|(_, rel)| rel.starts_with("mods/modSlimmer_Griffin_Armor/content/")),
            "expected mods/-qualified rels, got {:?}",
            plan.moves
        );
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "modSlimmer_Griffin_Armor").unwrap();
        assert_eq!(
            targets,
            vec![
                "mods/modSlimmer_Griffin_Armor/content/script.test.ws",
                "mods/modSlimmer_Griffin_Armor/content/script.test.xml",
            ]
        );
        assert!(stage.join("mods/modSlimmer_Griffin_Armor/content/script.test.ws").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn top_level_content_drop_stays_content() {
        // A real content drop (loose archive at the extraction root) keeps
        // staging under the game's content/.
        let tmp = tmpdir("top-content");
        write(&tmp.join("content/script.test.ws"), "x");
        let plan = analyze(&tmp);
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "modLoose").unwrap();
        assert_eq!(targets, vec!["content/script.test.ws"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mods_wrapper_layout_unchanged() {
        // The normal mods/modAutoApplyOils/content shape is untouched.
        let tmp = tmpdir("mods-wrapper");
        write(&tmp.join("mods/modAutoApplyOils/content/script.test.ws"), "x");
        let plan = analyze(&tmp);
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "whatever").unwrap();
        assert_eq!(targets, vec!["mods/modAutoApplyOils/content/script.test.ws"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mod_folder_next_to_bin_never_leaks_into_game_content() {
        // Extraction root is a game layout (content/ + bin/) containing
        // `modFoo/`: the mod's files must not land in the game's content/.
        // (Nested mod folders still wrap under the caller's mod_folder —
        // a pre-existing shape this bug report doesn't cover.)
        let tmp = tmpdir("mod-plus-bin");
        write(&tmp.join("modFoo/content/a.ws"), "x");
        write(&tmp.join("bin/x.dll"), "y");
        let plan = analyze(&tmp);
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "modFoo").unwrap();
        assert!(
            targets.iter().all(|t| !t.starts_with("content/")),
            "mod folder leaked into game content: {targets:?}"
        );
        assert!(
            targets.iter().any(|t| t.ends_with("content/a.ws")),
            "mod folder file missing: {targets:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
