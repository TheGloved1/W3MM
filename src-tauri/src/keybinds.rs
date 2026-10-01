//! Keybinds + settings snippets.
//! Ports `parse_keybinds/parse_snippets/scan_keybind_files`
//! (`w3modmanager.py:599,630,776`) — read-only analysis side. Writers for
//! `input.settings` live with the settings-file helpers in `manager.rs`.

use regex::Regex;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn ini_section() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*\[([^\]]+)\]\s*$").unwrap())
}
fn ik_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^\s*IK_[A-Za-z0-9_]+\s*=").unwrap())
}

/// Pull '[Section] / IK_Key=(Action=…)' blocks out of readme/snippet text.
pub fn parse_keybinds(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut section: Option<String> = None;
    for line in text.lines() {
        if let Some(c) = ini_section().captures(line) {
            section = Some(c[1].trim().to_string());
            continue;
        }
        if section.is_some() && ik_line().is_match(line) {
            out.entry(section.clone().unwrap()).or_default().push(line.trim().to_string());
        }
    }
    out
}

/// Scan a mod folder for keybind-bearing files (input.settings, *.txt/.md readmes).
pub fn scan_keybind_files(root: &std::path::Path) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let mut out = BTreeMap::new();
    let mut walker = walkdir::WalkDir::new(root).into_iter();
    while let Some(Ok(e)) = walker.next() {
        if !e.file_type().is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        let interesting = name == "input.settings"
            || name.ends_with(".txt")
            || name.ends_with(".md")
            || name.contains("keybind")
            || name.contains("hotkey");
        if !interesting {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(e.path()) {
            let kb = parse_keybinds(&text);
            if !kb.is_empty() {
                out.insert(e.path().to_string_lossy().to_string(), kb);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_sections() {
        let t = "[Exploration]\nIK_E=(Action=DoDodge)\n[Combat]\nIK_Q=(Action=CastSign)\n";
        let kb = parse_keybinds(t);
        assert_eq!(kb["Exploration"], vec!["IK_E=(Action=DoDodge)"]);
        assert_eq!(kb["Combat"], vec!["IK_Q=(Action=CastSign)"]);
    }
}
