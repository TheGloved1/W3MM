//! Settings-file writers: `input.settings` keybinds + `user.settings` snippets.
//! Ports `ModManager._write_input_settings/_write_user_settings/
//! _read_settings_text/_write_settings_text` (`w3modmanager.py:6541-6860`) at a
//! faithful-but-compact level: preserve unknown lines, replace managed blocks.

use std::collections::BTreeMap;
use std::path::Path;

const MANAGED_MARK: &str = "; YAWMM managed below — edits above this line are kept";
/// Previous brand's marker: recognized on read so old managed blocks are
/// replaced (not duplicated); always rewritten to `MANAGED_MARK`.
const LEGACY_MANAGED_MARK: &str = "; W3MM managed below — edits above this line are kept";

fn is_managed_mark(line: &str) -> bool {
    let t = line.trim();
    t == MANAGED_MARK || t == LEGACY_MANAGED_MARK
}

/// Merge keybind lines into input.settings: keep user content, replace the
/// managed block at the end per section.
pub fn write_input_settings(path: &Path, keybinds: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    // Drop previous managed block.
    let mut kept: Vec<String> = vec![];
    for line in existing.lines() {
        if is_managed_mark(line) {
            break;
        }
        kept.push(line.to_string());
    }
    while kept.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        kept.pop();
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("{MANAGED_MARK}\n"));
    for (section, lines) in keybinds {
        out.push_str(&format!("\n[{section}]\n"));
        for l in lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(())
}

/// Merge user.settings snippets similarly.
pub fn write_user_settings(path: &Path, snippets: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut kept: Vec<String> = vec![];
    for line in existing.lines() {
        if is_managed_mark(line) {
            break;
        }
        kept.push(line.to_string());
    }
    while kept.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        kept.pop();
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("{MANAGED_MARK}\n"));
    for (section, lines) in snippets {
        out.push_str(&format!("\n[{section}]\n"));
        for l in lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, out).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_user_content() {
        let dir = std::env::temp_dir().join("yawmm-settings-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("input.settings");
        std::fs::write(&p, "[HUD]\nHudSize=2\n").unwrap();
        let mut kb = BTreeMap::new();
        kb.insert("Exploration".into(), vec!["IK_E=(Action=X)".into()]);
        write_input_settings(&p, &kb).unwrap();
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains("HudSize=2"));
        assert!(t.contains("IK_E"));
        // second write is idempotent (no duplication of kept part)
        write_input_settings(&p, &kb).unwrap();
        let t2 = std::fs::read_to_string(&p).unwrap();
        assert_eq!(t2.matches("HudSize=2").count(), 1);
    }

    #[test]
    fn legacy_marker_upgraded_not_duplicated() {
        // File managed under the old brand: the old block is replaced and
        // the marker is rewritten to the new one — never appended twice.
        let dir = std::env::temp_dir().join("yawmm-settings-legacy-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("input.settings");
        std::fs::write(&p, "[HUD]\nHudSize=2\n; W3MM managed below — edits above this line are kept\n[Exploration]\nIK_OLD=(Action=X)\n").unwrap();
        let mut kb = BTreeMap::new();
        kb.insert("Exploration".into(), vec!["IK_E=(Action=X)".into()]);
        write_input_settings(&p, &kb).unwrap();
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains("HudSize=2"));
        assert!(!t.contains("IK_OLD"), "old managed block must be replaced");
        assert!(t.contains("IK_E"));
        assert!(!t.contains("; W3MM managed below"), "legacy marker must be rewritten");
        assert!(t.contains("; YAWMM managed below"));
        assert_eq!(t.matches("HudSize=2").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
