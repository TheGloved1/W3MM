//! Keybinds + settings snippets.
//! Ports `parse_keybinds/parse_snippets/scan_keybind_files/readme_text`
//! (`w3modmanager.py:599,630,749,776`).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

fn ini_section() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*\[([^\]]+)\]\s*$").unwrap())
}
fn ik_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^\s*IK_[A-Za-z0-9_]+\s*=").unwrap())
}
fn setting_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*([A-Za-z_][\w.\-]*)\s*=(.*)$").unwrap())
}
fn comment_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\s*(;|#|//|--)").unwrap())
}
fn filelist_line() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)^\s*([\w.-]+\.xml)\s*;\s*$").unwrap())
}
fn optional_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\boptional\b").unwrap())
}

/// Input.settings contexts: blocks under these are keybinds, never user settings.
pub const INPUT_CONTEXTS: &[&str] = &[
    "exploration", "horse", "swimming", "boat", "boatpassenger", "combat", "diving", "death",
    "jumpclimb", "fistfight", "scene", "exploration_replacer_ciri", "combat_replacer_ciri",
    "swimming_replacer_ciri", "radialmenu",
];

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snippets {
    pub user: BTreeMap<String, Vec<String>>,
    pub input_xml: Vec<String>,
    pub filelist: Vec<String>,
    pub optional: BTreeSet<String>,
}

fn is_heading(line: &str) -> bool {
    let t = line.trim();
    if !t.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    t.starts_with('#') || t.contains("---") || t.contains("===") || t.contains("***")
}

/// Port of python `parse_snippets`: tell user.settings / input.xml / filelist
/// blocks apart by what the lines are, not by filename.
pub fn parse_snippets(text: &str, name: &str) -> Snippets {
    let lines: Vec<&str> = text.lines().collect();
    let low_name = name.to_lowercase();
    let mut user: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut xml: Vec<String> = vec![];
    let mut filelist: Vec<String> = vec![];
    let mut optional: BTreeSet<String> = BTreeSet::new();

    // chapter bookkeeping for "optional" detection
    let mut chapter_opt: Vec<bool> = vec![];
    let mut part: Vec<usize> = vec![];
    let mut opt = false;
    let mut n = 0usize;
    for line in &lines {
        if is_heading(line) {
            if optional_re().is_match(line) {
                opt = true;
            }
            n += 1;
        }
        chapter_opt.push(opt);
        part.push(n);
    }
    let user_re = Regex::new(r"(?i)user\.settings").unwrap();
    let other_re = Regex::new(r"(?i)mods\.settings|\b[\w-]+\.(?:ini|conf|cfg)\b").unwrap();
    let input_settings_re = Regex::new(r"(?i)input\.settings").unwrap();
    let mut names_user: BTreeSet<usize> = BTreeSet::new();
    let mut names_other: BTreeSet<usize> = BTreeSet::new();
    for (k, line) in lines.iter().enumerate() {
        if user_re.is_match(line) {
            names_user.insert(part[k]);
        } else if other_re.is_match(line) && !input_settings_re.is_match(line) {
            names_other.insert(part[k]);
        }
    }
    let file_is_user = low_name.contains("user.settings");
    let heading_optional = |at: usize| -> bool {
        if *chapter_opt.get(at).unwrap_or(&false) {
            return true;
        }
        let mut seen = 0;
        let mut k = at as isize - 1;
        while k >= 0 {
            let l = lines[k as usize].trim();
            if !l.is_empty() {
                if optional_re().is_match(l) {
                    return true;
                }
                if is_heading(l) {
                    break;
                }
                seen += 1;
                if seen >= 6 {
                    break;
                }
            }
            k -= 1;
        }
        false
    };

    let ik_sections: BTreeSet<String> = parse_keybinds(text).keys().map(|s| s.to_lowercase()).collect();
    let mut section: Option<String> = None;
    let mut block_start = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some(c) = ini_section().captures(line) {
            section = Some(c[1].trim().to_string());
            block_start = i;
            i += 1;
            continue;
        }
        if let Some(sec) = section.clone() {
            if let Some(kv) = setting_line().captures(line) {
                let key = kv[1].to_string();
                let low = sec.to_lowercase();
                if key.to_uppercase().starts_with("IK_") {
                    if heading_optional(block_start) {
                        optional.insert("input".into());
                    }
                } else if !INPUT_CONTEXTS.contains(&low.as_str())
                    && !ik_sections.contains(&low)
                    && !low.contains(".fx")
                    && (file_is_user || names_user.contains(&part[block_start]) || !names_other.contains(&part[block_start]))
                {
                    let tidy = format!("{}={}", key, kv[2].trim());
                    let e = user.entry(sec.clone()).or_default();
                    if !e.contains(&tidy) {
                        e.push(tidy);
                    }
                    if heading_optional(block_start) {
                        optional.insert("user".into());
                    }
                }
                i += 1;
                continue;
            }
            if !line.trim().is_empty() && !comment_line().is_match(line) {
                section = None;
            }
        }
        if line.trim_start().starts_with("<Var") {
            let mut one = line.to_string();
            let mut k = i;
            while !one.contains("/>") && k + 1 < lines.len() && k - i < 8 {
                k += 1;
                one.push(' ');
                one.push_str(lines[k].trim());
            }
            let flat = one.split_whitespace().collect::<Vec<_>>().join(" ");
            let var_re = Regex::new(r"<Var\b[^>]*/>").unwrap();
            if let Some(m) = var_re.find(&flat) {
                let tag = m.as_str();
                let low = tag.to_lowercase();
                if tag.contains("/>") && (low.contains("builder=\"input\"") || low.contains("inputpc")) {
                    if !xml.contains(&tag.to_string()) {
                        xml.push(tag.to_string());
                    }
                    if heading_optional(i) {
                        optional.insert("input_xml".into());
                    }
                    i = k + 1;
                    continue;
                }
            }
        }
        if let Some(f) = filelist_line().captures(line) {
            let v = f[1].to_string();
            if !filelist.contains(&v) {
                filelist.push(v);
            }
        }
        i += 1;
    }
    // mods.settings examples are not user settings
    user.retain(|_, v| {
        let keys: BTreeSet<String> = v
            .iter()
            .filter_map(|l| setting_line().captures(l).map(|c| c[1].to_lowercase()))
            .collect();
        !(keys.iter().all(|k| k == "enabled" || k == "priority") && !keys.is_empty())
    });
    Snippets { user, input_xml: xml, filelist, optional }
}

/// Readme as plain text whatever it was saved as (UTF-8/16).
pub fn readme_text(path: &std::path::Path) -> String {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let (lines, _) = crate::script_merge::decode_script(&data);
    lines.join("\n")
}

/// Scan a mod folder for keybind/snippet-bearing files.
pub fn scan_keybind_files(root: &std::path::Path) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    let mut out = BTreeMap::new();
    for e in walkdir::WalkDir::new(root).into_iter().flatten() {
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
        let text = readme_text(e.path());
        if text.is_empty() {
            continue;
        }
        let kb = parse_keybinds(&text);
        if !kb.is_empty() {
            out.insert(e.path().to_string_lossy().to_string(), kb);
        }
    }
    out
}

pub fn scan_snippets(root: &std::path::Path) -> BTreeMap<String, Snippets> {
    let mut out = BTreeMap::new();
    for e in walkdir::WalkDir::new(root).into_iter().flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let low = name.to_lowercase();
        if !(low.ends_with(".txt") || low.ends_with(".md") || low.ends_with(".nfo") || low.contains("readme")) {
            continue;
        }
        let text = readme_text(e.path());
        if text.trim().is_empty() {
            continue;
        }
        let s = parse_snippets(&text, &name);
        if !s.user.is_empty() || !s.input_xml.is_empty() || !s.filelist.is_empty() {
            out.insert(e.path().to_string_lossy().to_string(), s);
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
    #[test]
    fn user_vs_input_context() {
        let t = "[Exploration]\nIK_E=(Action=X)\n[HUD]\nHudSize=2\n";
        let s = parse_snippets(t, "readme.txt");
        assert!(s.user.contains_key("HUD"));
        assert!(!s.user.contains_key("Exploration"));
    }
    #[test]
    fn input_xml_vars() {
        let t = "<Var builder=\"Input\" id=\"X\" displayName=\"Y\" />\n";
        let s = parse_snippets(t, "readme.txt");
        assert_eq!(s.input_xml.len(), 1);
    }
}
