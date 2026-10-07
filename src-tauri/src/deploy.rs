//! Deploy: hardlink-or-copy enabled mods, backups, `mods.settings`, filelists.
//! Ports `ModManager.sync/_deploy_missing/_restore/_read_settings/clashes/
//! _write_mods_settings/_update_filelists` (`w3modmanager.py:5774-6400`).

use std::collections::BTreeSet;
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
    for sub in ["mods", "dlc", "bin", "content"] {
        let base = staging_mod.join(sub);
        if !base.is_dir() {
            continue;
        }
        for e in WalkDir::new(&base).into_iter().flatten() {
            if e.file_type().is_file() {
                // Canonical `/` rels: deployed keys, want-sets, and folder
                // matching all split on `/` (see install::canonical_rel).
                if let Ok(rel) = e.path().strip_prefix(staging_mod) {
                    out.push(std::path::PathBuf::from(crate::install::canonical_rel(
                        &rel.to_string_lossy(),
                    )));
                }
            }
        }
    }
    out.sort();
    out
}

/// Already our own deployed copy? Same inode (hardlink) or byte-identical
/// content. The original checks `os.path.samefile(src, dst)` so re-deploys
/// over unchanged files don't snapshot our own output as an "original" —
/// without this, disabling later restores our own files from backup and the
/// mod folder wrongly survives. Content equality is observationally equivalent
/// (restoring would put back identical bytes) and also heals copies.
fn is_ours(src: &Path, dst: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let (sm, dm) = match (std::fs::metadata(src), std::fs::metadata(dst)) {
        (Ok(s), Ok(d)) => (s, d),
        _ => return false,
    };
    if sm.dev() == dm.dev() && sm.ino() == dm.ino() {
        return true;
    }
    if sm.len() != dm.len() {
        return false;
    }
    if sm.len() > 64 << 20 {
        return false; // don't hash huge files; back up instead
    }
    let hash = |p: &Path| {
        use sha1::Digest;
        use std::io::Read;
        let mut h = sha1::Sha1::new();
        let mut f = std::fs::File::open(p).ok()?;
        let mut buf = [0u8; 65536];
        loop {
            let n = f.read(&mut buf).ok()?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Some(h.finalize().to_vec())
    };
    match (hash(src), hash(dst)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Case-insensitive deploy: lowercase key -> actual relative path.
/// Mirrors python CIResolver/ci_path behaviour at a coarse level.
pub fn deploy_mod(game: &Path, staging_mod: &Path, rels: &[PathBuf], backup: &Path) -> std::io::Result<Vec<String>> {
    let mut written = Vec::new();
    for rel in rels {
        let src = staging_mod.join(rel);
        let dst = game.join(rel);
        // Back up anything we'd overwrite exactly once — but never our own
        // output (see is_ours): that isn't an original worth restoring.
        if dst.is_file() && !is_ours(&src, &dst) {
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

/// Normalize a filelist line for comparison: trim, drop a trailing `;`
/// (the original manager's entry format), compare case-insensitively.
fn norm_filelist_line(l: &str) -> String {
    l.trim().trim_end_matches(';').trim().to_lowercase()
}

/// Register menu xmls into dx11/dx12filelist.txt: add missing wanted entries
/// and prune entries that are no longer managed. Like the original
/// (`_update_filelists`), what *we* added is tracked per file in `added` so a
/// removed mod's entries are dropped even after its row (and targets) are
/// gone; `stale_xmls` additionally covers paths swept this deploy that were
/// never recorded. Foreign lines (vanilla entries, hand edits) are kept.
pub fn update_filelists(
    game: &Path,
    xmls: &[String],
    stale_xmls_lower: &std::collections::HashSet<String>,
    added: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::io::Result<std::collections::BTreeMap<String, Vec<String>>> {
    let want_lower: std::collections::HashSet<String> =
        xmls.iter().map(|x| norm_filelist_line(x)).filter(|x| !x.is_empty()).collect();
    // Original-case spellings for the record, keyed by normalized form.
    let mut spell: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for x in xmls {
        spell.entry(norm_filelist_line(x)).or_insert(x.trim());
    }
    let mut out_added = added.clone();
    for list in ["bin/dx11filelist.txt", "bin/dx12filelist.txt"] {
        let path = game.join(list);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let recorded: std::collections::HashSet<String> = added
            .get(list)
            .map(|v| v.iter().map(|x| norm_filelist_line(x)).collect())
            .unwrap_or_default();
        // Entries we added that nothing wants anymore, plus swept paths that
        // predate tracking. Never drop a wanted entry.
        let drop: std::collections::HashSet<String> = recorded
            .iter()
            .filter(|x| !want_lower.contains(*x))
            .cloned()
            .chain(stale_xmls_lower.iter().filter(|x| !want_lower.contains(*x)).cloned())
            .collect();
        // Preserve the on-disk spelling of kept lines; collapse dupes.
        let mut kept: Vec<String> = Vec::new();
        let mut have: BTreeSet<String> = BTreeSet::new();
        let mut pruned = false;
        for line in existing.lines() {
            let k = norm_filelist_line(line);
            if k.is_empty() {
                continue;
            }
            if drop.contains(&k) {
                pruned = true;
                continue;
            }
            if have.insert(k) {
                kept.push(line.trim().to_string());
            } else {
                pruned = true; // collapsed a duplicate
            }
        }
        let mut extra = Vec::new();
        for x in xmls {
            let k = norm_filelist_line(x);
            if !k.is_empty() && !have.contains(&k) {
                have.insert(k);
                extra.push(x.trim().to_string());
            }
        }
        // Record the managed set for this file: after this run it holds
        // exactly the wanted entries (+foreign lines), so recording `want`
        // is both accurate and self-healing. This also bootstraps the record
        // for entries added before tracking existed (the append-only era),
        // which add-only tracking would never capture: such entries are
        // present but unrecorded, so a removal would leave them behind.
        let mut managed: Vec<String> = want_lower
            .iter()
            .filter_map(|k| spell.get(k))
            .map(|s| s.to_string())
            .collect();
        managed.sort();
        out_added.insert(list.to_string(), managed);
        if extra.is_empty() && !pruned {
            continue;
        }
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut text = kept.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        for x in extra {
            text.push_str(&x);
            text.push('\n');
        }
        std::fs::write(&path, text)?;
    }
    Ok(out_added)
}

/// Managed top folder for a rel: `mods/<name>` / `dlc/<name>`, lowercased.
/// `bin/` and `content/` are shared vanilla space and have no owner folder.
fn managed_folder(lower_rel: &str) -> Option<String> {
    let mut segs = lower_rel.split('/');
    match segs.next()? {
        "mods" | "dlc" => segs.next().map(|n| format!("{}/{}", &lower_rel[..lower_rel.find('/').unwrap()], n)),
        _ => None,
    }
}

/// Find game files the deployed-map diff missed: anything under a managed
/// `mods/<name>` / `dlc/<name>` folder that isn't wanted anymore. Unmanaged
/// folders (no known mod claims them) are left alone for the import banner.
/// Symlinks are never touched. `bin/`/`content/` orphans are handled by the
/// caller via exact known-target checks (vanilla space is never scanned).
pub fn scan_tree_orphans(
    game: &Path,
    want_lower: &std::collections::HashSet<String>,
    managed_folders: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut orphans = Vec::new();
    for top in ["mods", "dlc"] {
        let base = game.join(top);
        if !base.is_dir() {
            continue;
        }
        for e in WalkDir::new(&base).into_iter().flatten() {
            if e.file_type().is_symlink() || !e.file_type().is_file() {
                continue;
            }
            let rel = match e.path().strip_prefix(game) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            let low = rel.to_lowercase();
            if want_lower.contains(&low) {
                continue;
            }
            match managed_folder(&low) {
                Some(f) if managed_folders.contains(&f) => orphans.push(rel),
                _ => {}
            }
        }
    }
    orphans.sort();
    orphans
}

/// Restore backups for paths no longer wanted; drop empty dirs up to game root.
pub fn restore_paths(game: &Path, backup: &Path, rels: &[String]) -> std::io::Result<()> {
    for rel in rels {
        let dst = game.join(rel);
        let bsrc = backup.join(rel);
        let _ = std::fs::remove_file(&dst);
        if bsrc.is_file() {
            if let Some(p) = dst.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::copy(&bsrc, &dst)?;
            let _ = std::fs::remove_file(&bsrc);
        }
        // prune newly-empty parents
        let mut cur = dst.parent().map(|p| p.to_path_buf());
        while let Some(d) = cur {
            if d == game || !d.starts_with(game) {
                break;
            }
            match std::fs::remove_dir(d.clone()) {
                Ok(()) => cur = d.parent().map(|p| p.to_path_buf()),
                Err(_) => break,
            }
        }
    }
    Ok(())
}

/// Parse existing mods.settings `[Mods]` block into ordered names.
pub fn read_mods_settings(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut in_mods = false;
    let mut out = vec![];
    for line in text.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case("[Mods]") {
            in_mods = true;
            continue;
        }
        if in_mods {
            if t.starts_with('[') {
                break;
            }
            if let Some((_, v)) = t.split_once('=') {
                let v = v.trim();
                if !v.is_empty() {
                    out.push(v.to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn setup_tree(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("w3mm-deploy-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let game = base.join("game");
        let staging = base.join("staging").join("mod1");
        let backup = base.join("backup");
        std::fs::create_dir_all(staging.join("mods/modFoo/content")).unwrap();
        let mut f = std::fs::File::create(staging.join("mods/modFoo/content/a.ws")).unwrap();
        f.write_all(b"function f() {}").unwrap();
        (game, staging, backup)
    }

    #[test]
    fn disable_removes_deployed_files() {
        let (game, staging, backup) = setup_tree("a");
        let rels = mod_files(&staging);
        assert_eq!(rels.len(), 1);
        // first deploy: nothing pre-exists, so no backup may be taken
        let w1 = deploy_mod(&game, &staging, &rels, &backup).unwrap();
        assert_eq!(w1.len(), 1);
        assert!(game.join(&w1[0]).is_file());
        assert!(!backup.join(&w1[0]).exists(), "no original existed: no backup expected");
        // re-deploy over our own output must NOT snapshot our files as originals
        let _ = deploy_mod(&game, &staging, &rels, &backup).unwrap();
        assert!(!backup.join(&w1[0]).exists(), "own output must never be backed up");
        // disable: stale restore removes the file and prunes the mod folder
        restore_paths(&game, &backup, &w1).unwrap();
        assert!(!game.join(&w1[0]).exists(), "disabled mod file must be gone");
        assert!(!game.join("mods/modFoo").exists(), "empty mod folder must be pruned");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    #[test]
    fn disable_restores_genuine_originals() {
        let (game, staging, backup) = setup_tree("b");
        // a foreign file pre-exists in the game dir
        std::fs::create_dir_all(game.join("mods/modFoo/content")).unwrap();
        std::fs::write(game.join("mods/modFoo/content/a.ws"), b"vanilla").unwrap();
        let rels = mod_files(&staging);
        let _ = deploy_mod(&game, &staging, &rels, &backup).unwrap();
        assert!(backup.join(&rels[0].to_string_lossy().to_string()).is_file(), "genuine original must be backed up");
        restore_paths(&game, &backup, &["mods/modFoo/content/a.ws".to_string()]).unwrap();
        assert_eq!(std::fs::read(game.join("mods/modFoo/content/a.ws")).unwrap(), b"vanilla");
        let _ = std::fs::remove_dir_all(game.parent().unwrap());
    }

    #[test]
    fn reconcile_finds_orphans_lost_by_deployed_map() {
        // Two mods deployed, then the deployed map is wiped (simulating the
        // tracking loss seen in the wild) and one mod disabled: the scan must
        // still find the disabled mod's files via managed-folder ownership.
        let base = std::env::temp_dir().join(format!("w3mm-deploy-test-reconcile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let game = base.join("game");
        let backup = base.join("backup");
        for (moddir, file) in [("modFoo", "a.ws"), ("modBar", "b.ws")] {
            let p = base.join("staging").join(moddir).join(format!("mods/{moddir}/content/{file}"));
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        let stage_foo = base.join("staging").join("modFoo");
        let stage_bar = base.join("staging").join("modBar");
        let rels_foo = mod_files(&stage_foo);
        let rels_bar = mod_files(&stage_bar);
        deploy_mod(&game, &stage_foo, &rels_foo, &backup).unwrap();
        deploy_mod(&game, &stage_bar, &rels_bar, &backup).unwrap();
        // Only modFoo stays wanted; deployed map knows nothing (wiped).
        let want: std::collections::HashSet<String> = rels_foo
            .iter()
            .map(|r| r.to_string_lossy().to_lowercase())
            .collect();
        let managed: std::collections::HashSet<String> =
            ["mods/modfoo", "mods/modbar"].iter().map(|s| s.to_string()).collect();
        // An unmanaged user folder must be ignored.
        std::fs::create_dir_all(game.join("mods/modMine/content")).unwrap();
        std::fs::write(game.join("mods/modMine/content/c.ws"), b"mine").unwrap();
        let orphans = scan_tree_orphans(&game, &want, &managed);
        assert_eq!(orphans, vec!["mods/modBar/content/b.ws".to_string()]);
        restore_paths(&game, &backup, &orphans).unwrap();
        assert!(!game.join("mods/modBar").exists(), "orphan folder must be pruned");
        assert!(game.join("mods/modFoo/content/a.ws").is_file(), "wanted file must survive");
        assert!(game.join("mods/modMine/content/c.ws").is_file(), "unmanaged content must survive");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn filelists_prune_recorded_and_swept_xmls_but_keep_vanilla() {
        let base = std::env::temp_dir().join(format!("w3mm-deploy-test-filelist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let game = base.join("game");
        std::fs::create_dir_all(game.join("bin")).unwrap();
        let stale_recorded = "bin/r4game/user_config_matrix/pc/modOld.xml".to_string();
        let stale_swept = "bin/r4game/user_config_matrix/pc/modGone.xml".to_string();
        let kept_xml = "bin/r4game/user_config_matrix/pc/modNew.xml".to_string();
        let vanilla = "bin/config/r4game/user_config_matrix/pc/gameplay.xml".to_string();
        for list in ["bin/dx11filelist.txt", "bin/dx12filelist.txt"] {
            std::fs::write(game.join(list), format!("{vanilla}\n{stale_recorded}\n{stale_swept}\n")).unwrap();
        }
        // modOld was recorded when added; modGone predates tracking but was
        // swept this deploy (its file is gone from the game).
        let mut added: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for list in ["bin/dx11filelist.txt", "bin/dx12filelist.txt"] {
            added.insert(list.to_string(), vec![stale_recorded.clone(), kept_xml.clone()]);
        }
        let swept: std::collections::HashSet<String> =
            [stale_swept.to_lowercase()].into_iter().collect();
        let out = update_filelists(&game, std::slice::from_ref(&kept_xml), &swept, &added).unwrap();
        for list in ["bin/dx11filelist.txt", "bin/dx12filelist.txt"] {
            let text = std::fs::read_to_string(game.join(list)).unwrap();
            assert!(text.contains(&vanilla), "vanilla entry must be kept in {list}");
            assert!(text.contains(&kept_xml), "wanted entry must be present in {list}");
            assert!(!text.contains(&stale_recorded), "recorded-but-unwanted entry must be pruned from {list}");
            assert!(!text.contains(&stale_swept), "swept-but-untracked entry must be pruned from {list}");
            // The record follows: dropped entries forgotten, kept one retained.
            assert_eq!(out.get(list), Some(&vec![kept_xml.clone()]), "record must track {list}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn filelists_bootstrap_record_for_pretracking_entries() {
        // Entries added before tracking existed are present but unrecorded;
        // the record must still capture them, or their later removal would
        // leave the lines behind.
        let base = std::env::temp_dir().join(format!("w3mm-deploy-test-filelist-boot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let game = base.join("game");
        std::fs::create_dir_all(game.join("bin")).unwrap();
        let xml = "bin/r4game/user_config_matrix/pc/modOld.xml".to_string();
        std::fs::write(game.join("bin/dx11filelist.txt"), format!("{xml}\n")).unwrap();
        let empty: std::collections::HashSet<String> = Default::default();
        let added: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        let out = update_filelists(&game, std::slice::from_ref(&xml), &empty, &added).unwrap();
        let text = std::fs::read_to_string(game.join("bin/dx11filelist.txt")).unwrap();
        assert!(text.contains(&xml), "present wanted entry must stay");
        assert_eq!(out.get("bin/dx11filelist.txt"), Some(&vec![xml]), "record must bootstrap");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn filelists_never_drop_wanted_entries() {
        // A wanted xml is kept even if it also appears in the swept set
        // (e.g. reinstalled at the same path it was just swept from), and a
        // tracked entry that is still wanted stays tracked.
        let base = std::env::temp_dir().join(format!("w3mm-deploy-test-filelist-want-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let game = base.join("game");
        std::fs::create_dir_all(game.join("bin")).unwrap();
        let xml = "bin/r4game/user_config_matrix/pc/modSame.xml".to_string();
        std::fs::write(game.join("bin/dx11filelist.txt"), format!("{xml}\n")).unwrap();
        let swept: std::collections::HashSet<String> = [xml.to_lowercase()].into_iter().collect();
        let mut added: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        added.insert("bin/dx11filelist.txt".to_string(), vec![xml.clone()]);
        let out = update_filelists(&game, std::slice::from_ref(&xml), &swept, &added).unwrap();
        let text = std::fs::read_to_string(game.join("bin/dx11filelist.txt")).unwrap();
        assert!(text.contains(&xml), "wanted entry must survive its own sweep");
        assert_eq!(out.get("bin/dx11filelist.txt"), Some(&vec![xml]), "tracked wanted entry stays tracked");
        let _ = std::fs::remove_dir_all(&base);
    }
}
