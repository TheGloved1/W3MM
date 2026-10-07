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
/// ships the mod itself, not the game layout, so it should be read as if it
/// were spelled `mods/modSlimmer_Griffin_Armor`. Applies to any game dir it
/// holds (content, bin, dlc, or a nested mods/) — a mod shipping both
/// `content/` and `bin/` is still one mod, not a Content row plus a Bin row.
/// Only the extraction root itself is exempt: `content/` directly at the top
/// of an archive really is a game content drop.
fn is_mod_folder(root: &Path, extracted: &Path) -> bool {
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    name.to_lowercase().starts_with("mod") && root != extracted
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

/// `modFoo/` sitting inside a game-layout archive (one that also ships real
/// content/bin). It's a mod folder, so read it as `mods/modFoo` rather than
/// letting the caller wrap it a second time. The structural check (it must
/// contain a game dir of its own) keeps unrelated folders like `models/`
/// out of it.
fn is_nested_mod_dir(root: &Path, seg: &str) -> bool {
    if !seg.to_lowercase().starts_with("mod") {
        return false;
    }
    let dir = root.join(seg);
    dir.is_dir() && ["mods", "dlc", "bin", "content"].iter().any(|s| dir.join(s).is_dir())
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
            let seg = rel.split(['/', '\\']).next().unwrap_or("").to_string();
            let first = seg.to_lowercase();
            // Inside a bare mod folder every game dir belongs to that mod:
            // `modFoo/content/…` and `modFoo/bin/…` both become
            // `mods/modFoo/…`. A mod folder nested in a game layout gets the
            // same treatment, without the caller's second wrap.
            let rel = if mod_folder_root && ["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
                qualify_mod_folder(&root, &rel)
            } else if !mod_folder_root && is_nested_mod_dir(&root, &seg) {
                format!("mods/{rel}")
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

/// A row of the Install window's archive-contents table: one archive root the
/// user can re-kind or re-folder before installing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RootChoice {
    pub kind: String,
    pub folder: String,
    pub prefix: String,
    #[serde(default)]
    pub files: usize,
}

/// Re-map the planned destinations through the dialog's current table so the
/// Install window's file preview can follow live edits. Returns unique,
/// sorted rels — the same values `build_staging` would produce.
#[tauri::command]
pub fn remap_roots(rels: Vec<String>, roots: Vec<RootChoice>, mod_folder: String) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = rels
        .iter()
        .map(|r| remap_target(r, &roots, &mod_folder))
        .collect();
    out.sort();
    out.dedup();
    Ok(out)
}

fn kind_dir(kind: &str) -> &'static str {
    match kind {
        "DLC" => "dlc",
        "Bin" => "bin",
        "Content" => "content",
        _ => "mods",
    }
}

/// Split a planned rel into (top dir, folder, rest). The folder is empty when
/// the rel has no folder layer — `content/x.ws` is a loose file, not an
/// `x.ws` folder.
pub fn split_rel(rel: &str) -> (String, String, Vec<String>) {
    let segs: Vec<&str> = rel.split('/').collect();
    let first = segs.first().copied().unwrap_or("").to_lowercase();
    if segs.len() > 2 {
        (first, segs[1].to_string(), segs[2..].iter().map(|s| s.to_string()).collect())
    } else {
        (first, String::new(), segs[1..].iter().map(|s| s.to_string()).collect())
    }
}

fn is_game_dir(seg: &str) -> bool {
    ["mods", "dlc", "bin", "content"].contains(&seg.to_lowercase().as_str())
}

/// Stable identity of a rel's source group — how a dialog row is tied to the
/// files it owns. Derived from the path segments, never from the kind name:
/// the directory is `mods/` but the kind is "Mod", so `kind.to_lowercase()`
/// would produce `mod/` and silently fail to match.
pub fn src_prefix(rel: &str) -> String {
    let (first, folder, _rest) = split_rel(rel);
    if folder.is_empty() {
        String::new()
    } else {
        format!("{first}/{folder}")
    }
}

/// Resolve the staging rel for one planned file through the dialog's root
/// table.
///
/// Rows are identified by their *source* group (`prefix`), which the dialog
/// carries through untouched, so editing Type or the folder name still maps
/// the right files. Which kinds keep a folder: Mod and DLC (both name a
/// directory in the game); content/ and bin/ take files directly, so those
/// strip the mod folder and the source's inner wrapper —
/// `mods/modFoo/content/x` as Content becomes `content/x`, not the inert
/// `content/modFoo/x`.
pub fn remap_target(rel: &str, roots: &[RootChoice], mod_folder: &str) -> String {
    let (first, folder, mut rest) = split_rel(rel);
    if !["mods", "dlc", "bin", "content"].contains(&first.as_str()) {
        // Bare file(s) with no game dir: the catch-all row owns them.
        let row = roots.iter().find(|r| r.folder.is_empty() || r.prefix.is_empty());
        return match row {
            Some(r) if !r.folder.is_empty() => format!("{}/{rel}", kind_dir(&r.kind)),
            Some(r) => format!("{}/{}/{}", kind_dir(&r.kind), crate::state::folder_safe(mod_folder), rel),
            None => format!("mods/{}/{}", crate::state::folder_safe(mod_folder), rel),
        };
    }
    let implied = match first.as_str() {
        "dlc" => "DLC",
        "bin" => "Bin",
        "content" => "Content",
        _ => "Mod",
    };
    // Identify the row by its *source* group (prefix, which the dialog carries
    // through untouched) so editing Type or folder name still maps the right
    // files. Fall back to the original kind+folder pairing.
    let row = if folder.is_empty() {
        roots
            .iter()
            .find(|r| r.folder.is_empty() && r.prefix.is_empty())
            .or_else(|| roots.iter().find(|r| r.kind == implied && r.folder.is_empty()))
    } else {
        let prefix = src_prefix(rel);
        roots
            .iter()
            .find(|r| r.prefix == prefix)
            .or_else(|| roots.iter().find(|r| r.kind == implied && r.folder == folder))
    };
    let Some(row) = row else {
        return rel.to_string();
    };
    // DLC folders are named in the game (dlc/<name>/…) so DLC keeps one, and
    // so does Mod. content/ and bin/ take files directly, so those kinds drop
    // the folder entirely — `(loose files)` in the dialog.
    let needs_folder = matches!(row.kind.as_str(), "Mod" | "DLC");
    if needs_folder && row.folder.is_empty() {
        return rel.to_string();
    }
    let dir = kind_dir(&row.kind);
    if !needs_folder {
        // Already a genuine game-dir path (`bin/x64_dx12/y.dll` kept as Bin,
        // `content/scripts/x.ws` as Content): the "folder" segment is a real
        // subdirectory, not a mod wrapper, so there is nothing to strip.
        // Stripping only applies when re-homing across top dirs
        // (`mods/modFoo/bin/x.dll` as Bin -> `bin/x.dll`). The original
        // manager staged `bin/` verbatim; this restores that parity.
        if first.as_str() == dir {
            return rel.to_string();
        }
        // Drop the source's own wrapper (content/, dlc/, …) too, or we'd get
        // content/modFoo/content/x — inert.
        if rest.first().map(|s| is_game_dir(s)).unwrap_or(false) {
            rest.remove(0);
        }
        return format!("{dir}/{}", rest.join("/"));
    }
    if row.kind == "DLC" {
        if rest.first().map(|s| is_game_dir(s)).unwrap_or(false) {
            rest.remove(0);
        }
        // Already carries its folder (modFoo/dlc/dlcFoo/…): don't repeat it.
        if rest.first().map(|s| s.eq_ignore_ascii_case(&row.folder)).unwrap_or(false) {
            return format!("{dir}/{}", rest.join("/"));
        }
    }
    format!("{dir}/{}/{}", row.folder, rest.join("/"))
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
        // Normalize the top game-dir segment (`Mods/` from Windows zips):
        // the deploy scan matches literal lowercase dirs and Linux is
        // case-sensitive, so an unnormalized top dir would stage files the
        // deployer never sees. Deeper segments keep their case — mod folder
        // names are identity.
        let target_rel = match target_rel.split_once('/') {
            Some((top, rest)) if ["mods", "dlc", "bin", "content"].contains(&top.to_lowercase().as_str()) => {
                format!("{}/{}", top.to_lowercase(), rest)
            }
            _ => target_rel,
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

    /// A dialog row: `kind`/`folder` are the user's (editable) outputs,
    /// `prefix` is the untouched source group the files come from.
    fn choice(kind: &str, folder: &str, prefix: &str) -> RootChoice {
        RootChoice {
            kind: kind.to_string(),
            folder: folder.to_string(),
            prefix: prefix.to_string(),
            files: 0,
        }
    }
    const AXII: &str = "mods/modAxiiDelusion";

    #[test]
    fn remap_default_mod_keeps_folder() {
        let roots = [choice("Mod", "modAxiiDelusion", AXII)];
        assert_eq!(
            remap_target("mods/modAxiiDelusion/content/x.ws", &roots, "Axii Delusion"),
            "mods/modAxiiDelusion/content/x.ws"
        );
    }

    #[test]
    fn remap_content_kind_targets_game_content_dir() {
        // User picked Content: files go straight to the game's content/,
        // without the mod folder layer.
        let roots = [choice("Content", "modAxiiDelusion", AXII)];
        assert_eq!(
            remap_target("mods/modAxiiDelusion/content/x.ws", &roots, "Axii Delusion"),
            "content/x.ws"
        );
    }

    #[test]
    fn remap_bin_kind_targets_game_bin_dir() {
        let roots = [choice("Bin", "modAxiiDelusion", AXII)];
        assert_eq!(
            remap_target("mods/modAxiiDelusion/bin/a.dll", &roots, "Axii Delusion"),
            "bin/a.dll"
        );
    }

    #[test]
    fn remap_genuine_bin_layout_preserves_subdirs() {
        // A real `bin/` tree (ASI loader, redscript tooling, …) keeps its
        // full subpath: the "folder" segment is a genuine directory, not a
        // mod wrapper. Dropping it strands files in `bin/` root.
        let roots = [choice("Bin", "x64_dx12", "bin/x64_dx12")];
        assert_eq!(
            remap_target("bin/x64_dx12/winmm.dll", &roots, "Grass"),
            "bin/x64_dx12/winmm.dll"
        );
        assert_eq!(
            remap_target("bin/x64_dx12/plugins/W3GrassWaves.asi", &roots, "Grass"),
            "bin/x64_dx12/plugins/W3GrassWaves.asi"
        );
    }

    #[test]
    fn remap_genuine_bin_menu_xml_keeps_config() {
        // `bin/config/r4game/…xml` as Bin must not lose the `config/` level.
        let roots = [choice("Bin", "config", "bin/config")];
        assert_eq!(
            remap_target("bin/config/r4game/user_config_matrix/pc/modFoo.xml", &roots, "Foo"),
            "bin/config/r4game/user_config_matrix/pc/modFoo.xml"
        );
    }

    #[test]
    fn remap_genuine_content_stays_put() {
        let roots = [choice("Content", "scripts", "content/scripts")];
        assert_eq!(
            remap_target("content/scripts/x.ws", &roots, "Loose"),
            "content/scripts/x.ws"
        );
    }

    #[test]
    fn build_staging_normalizes_top_dir_case() {
        // `Mods/` from a Windows zip must stage under lowercase `mods/`,
        // or the deploy scan (literal dir match, case-sensitive fs) misses it.
        let tmp = tmpdir("top-case");
        write(&tmp.join("src/a.ws"), "x");
        let plan = InstallPlan {
            moves: vec![(tmp.join("src/a.ws").to_string_lossy().to_string(), "Mods/modFoo/content/a.ws".to_string())],
            docs: vec![],
        };
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "whatever").unwrap();
        assert_eq!(targets, vec!["mods/modFoo/content/a.ws"]);
        assert!(stage.join("mods/modFoo/content/a.ws").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn grass_layout_end_to_end_mixed_roots() {
        // Dynamic Grass Overhaul shape: capital-`Mods` mod folder plus a
        // genuine `bin/x64_dx12` tree. Mod rows normalize the top dir;
        // the Bin tree passes through with subdirs intact.
        let tmp = tmpdir("grass-layout");
        write(&tmp.join("Mods/mod0000_W3GrassCleanup/content/blob0.bundle"), "x");
        write(&tmp.join("bin/x64_dx12/winmm.dll"), "y");
        write(&tmp.join("bin/x64_dx12/plugins/W3GrassWaves.asi"), "z");
        let plan = analyze(&tmp);
        let mut groups: std::collections::BTreeMap<(String, String), (usize, String)> = Default::default();
        for (_src, rel) in &plan.moves {
            let (first, folder, _rest) = split_rel(rel);
            let kind = match first.as_str() {
                "dlc" => "DLC",
                "bin" => "Bin",
                "content" => "Content",
                _ => "Mod",
            };
            let e = groups.entry((kind.to_string(), folder)).or_insert((0, String::new()));
            e.0 += 1;
            e.1 = src_prefix(rel);
        }
        let rows: Vec<RootChoice> = groups
            .into_iter()
            .map(|((kind, folder), (files, prefix))| RootChoice { prefix, kind, folder, files })
            .collect();
        let out: Vec<String> = plan.moves.iter().map(|(_, r)| remap_target(r, &rows, "Grass")).collect();
        for o in &out {
            if o.contains("W3GrassCleanup") {
                assert!(o.starts_with("mods/mod0000_W3GrassCleanup/content/"), "mod tree kept: {o}");
            } else {
                assert!(o.starts_with("bin/x64_dx12/"), "bin tree intact: {o}");
            }
        }
        assert_eq!(out.len(), 3);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn remap_dlc_keeps_its_folder() {
        // dlc/ dirs are named in the game, so the folder stays.
        let roots = [choice("DLC", "dlcFoo", "mods/modFoo")];
        assert_eq!(
            remap_target("mods/modFoo/dlc/dlcFoo/x.bin", &roots, "Foo"),
            "dlc/dlcFoo/x.bin"
        );
    }

    #[test]
    fn remap_top_level_dlc_keeps_its_folder() {
        // A plain dlc/<name>/… archive must not lose its directory.
        let roots = [choice("DLC", "dlcFoo", "dlc/dlcFoo")];
        assert_eq!(remap_target("dlc/dlcFoo/x.bin", &roots, "Foo"), "dlc/dlcFoo/x.bin");
    }

    #[test]
    fn remap_dlc_without_folder_leaves_rel_alone() {
        let roots = [choice("DLC", "", "dlc/dlcFoo")];
        assert_eq!(remap_target("dlc/dlcFoo/x.bin", &roots, "Foo"), "dlc/dlcFoo/x.bin");
    }

    #[test]
    fn remap_content_ignores_stale_folder_name() {
        // Even if a folder name lingers in the row, Content drops it.
        let roots = [choice("Content", "modAxiiDelusion", AXII)];
        assert_eq!(
            remap_target("mods/modAxiiDelusion/content/x.ws", &roots, "Axii"),
            "content/x.ws"
        );
    }

    #[test]
    fn remap_top_level_content_drop_untouched() {
        let roots = [choice("Content", "", "content/x.ws")];
        assert_eq!(remap_target("content/x.ws", &roots, "Loose"), "content/x.ws");
    }

    #[test]
    fn remap_honours_edited_folder_name() {
        let roots = [choice("Mod", "renamed", AXII)];
        assert_eq!(
            remap_target("mods/modAxiiDelusion/content/x.ws", &roots, "Axii"),
            "mods/renamed/content/x.ws"
        );
    }

    #[test]
    fn remap_unmatched_root_leaves_rel_alone() {
        let roots = [choice("Mod", "someOtherMod", "mods/somethingElse")];
        assert_eq!(
            remap_target("mods/modAxiiDelusion/content/x.ws", &roots, "Axii"),
            "mods/modAxiiDelusion/content/x.ws"
        );
    }

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
    fn mod_folder_with_content_and_bin_stays_one_mod() {
        // A bare mod folder shipping both content/ and bin/ is ONE mod, not a
        // Content row plus a Bin row.
        let tmp = tmpdir("mod-content-and-bin");
        write(&tmp.join("modFoo/content/a.ws"), "x");
        write(&tmp.join("modFoo/bin/a.dll"), "y");
        let plan = analyze(&tmp);
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "modFoo").unwrap();
        assert_eq!(
            targets,
            vec!["mods/modFoo/bin/a.dll", "mods/modFoo/content/a.ws"],
            "both dirs should stage inside the mod folder"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn mod_folder_next_to_bin_is_not_double_wrapped() {
        // A game layout that *contains* a mod folder: the mod's files land in
        // mods/modFoo/… (not mods/modFoo/modFoo/…), and real game files keep
        // their own place.
        let tmp = tmpdir("mod-inside-layout");
        write(&tmp.join("modFoo/content/a.ws"), "x");
        write(&tmp.join("bin/x.dll"), "y");
        let plan = analyze(&tmp);
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "modFoo").unwrap();
        assert_eq!(
            targets,
            vec!["bin/x.dll", "mods/modFoo/content/a.ws"],
            "mod folder must not be wrapped twice"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn content_selection_end_to_end_from_bare_mod_archive() {
        // Mirrors the whole Install-window flow: a bare `modAxiiDelusion`
        // archive, grouped exactly like install_preview does, then the user
        // picks Content (folder cleared) in the table.
        let tmp = tmpdir("e2e-content");
        write(
            &tmp.join("modAxiiDelusion/content/scripts/game/player/playerWitcher.ws"),
            "x",
        );
        let plan = analyze(&tmp);
        assert!(
            plan.moves.iter().all(|(_, r)| r.starts_with("mods/modAxiiDelusion/content/")),
            "analyze should qualify the bare mod folder: {:?}",
            plan.moves
        );
        // Preview grouping: (kind, folder) per rel, with the source prefix
        // derived from the path (mods/, not "mod").
        let mut groups: std::collections::BTreeMap<(String, String), (usize, String)> = Default::default();
        for (_src, rel) in &plan.moves {
            let (first, folder, _rest) = split_rel(rel);
            let kind = match first.as_str() {
                "dlc" => "DLC",
                "bin" => "Bin",
                "content" => "Content",
                _ => "Mod",
            };
            let e = groups.entry((kind.to_string(), folder)).or_insert((0, String::new()));
            e.0 += 1;
            e.1 = src_prefix(rel);
        }
        let rows: Vec<RootChoice> = groups
            .into_iter()
            .map(|((kind, folder), (files, prefix))| RootChoice {
                prefix,
                kind,
                folder,
                files,
            })
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].prefix, "mods/modAxiiDelusion");

        // Default (Mod) keeps the mod folder.
        let as_mod = remap_target(
            "mods/modAxiiDelusion/content/scripts/game/player/playerWitcher.ws",
            &rows,
            "Axii Delusion",
        );
        assert_eq!(
            as_mod,
            "mods/modAxiiDelusion/content/scripts/game/player/playerWitcher.ws"
        );

        // User picks Content: folder is cleared and disabled in the dialog.
        let rows = [RootChoice {
            kind: "Content".to_string(),
            folder: String::new(),
            prefix: "mods/modAxiiDelusion".to_string(),
            files: 1,
        }];
        assert_eq!(
            remap_target(
                "mods/modAxiiDelusion/content/scripts/game/player/playerWitcher.ws",
                &rows,
                "Axii Delusion"
            ),
            "content/scripts/game/player/playerWitcher.ws",
            "Content selection must land in the game's content/ dir"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn plain_dirs_named_like_mods_are_left_alone() {
        // `models/` isn't a mod folder: no game dir inside, so it must not be
        // promoted to mods/models/.
        let tmp = tmpdir("models-dir");
        write(&tmp.join("models/asset.dds"), "x");
        write(&tmp.join("bin/x.dll"), "y");
        let plan = analyze(&tmp);
        let stage = tmp.join("stage");
        let (targets, _) = build_staging(&plan, &stage, "modPack").unwrap();
        assert_eq!(targets, vec!["bin/x.dll", "mods/modPack/models/asset.dds"]);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
