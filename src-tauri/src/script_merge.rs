//! Script merger: Witcher `.ws` 3-way merge.
//! Ports `decode_script/encode_script/diff_hunks/apply_hunks/scan_script/
//! merge_script/function_check` (`w3modmanager.py:2013-3359`).
//!
//! Encoding behaviour is preserved: vanilla files are UTF-16LE (often without
//! BOM / with odd null placement — see `_unmangle_utf16`), merged output keeps
//! the base format. Diff is line-oriented with comment-aware spans.

use encoding_rs::{UTF_8, UTF_16LE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptEncoding {
    Utf8,
    Utf16Le,
}

pub fn decode_script(data: &[u8]) -> (Vec<String>, ScriptEncoding) {
    // BOM sniff first, then null-byte heuristic like the python version.
    if data.starts_with(&[0xFF, 0xFE]) {
        let (s, _) = UTF_16LE.decode_without_bom_handling(&data[2..]);
        return (s.lines().map(|l| l.to_string()).collect(), ScriptEncoding::Utf16Le);
    }
    if data.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (s, _) = UTF_8.decode_without_bom_handling(&data[3..]);
        return (s.lines().map(|l| l.to_string()).collect(), ScriptEncoding::Utf8);
    }
    let nulls = data.iter().filter(|&&b| b == 0).count();
    if nulls > data.len() / 8 && !data.is_empty() {
        let (s, _) = UTF_16LE.decode_without_bom_handling(data);
        return (s.lines().map(|l| l.to_string()).collect(), ScriptEncoding::Utf16Le);
    }
    let (s, _) = UTF_8.decode_without_bom_handling(data);
    // strip lone CR like the original normalisation
    let lines = s.replace("\r\n", "\n").replace('\r', "\n");
    (lines.lines().map(|l| l.to_string()).collect(), ScriptEncoding::Utf8)
}

pub fn encode_script(lines: &[String], fmt: ScriptEncoding) -> Vec<u8> {
    let text = lines.join("\r\n");
    match fmt {
        ScriptEncoding::Utf8 => text.into_bytes(),
        ScriptEncoding::Utf16Le => {
            let mut out = Vec::with_capacity(text.len() * 2);
            for u in text.encode_utf16() {
                out.extend_from_slice(&u.to_le_bytes());
            }
            out
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub base_lo: usize,
    pub base_hi: usize,
    pub lines: Vec<String>,
}

/// Line hunks of `lines` vs `base` (python `line_hunks`): multiple hunks
/// split on matching runs, so disjoint edits in one file merge independently.
pub fn line_hunks(base: &[String], lines: &[String]) -> Vec<Hunk> {
    let bn: Vec<String> = base.iter().map(|s| norm(s)).collect();
    let ln: Vec<String> = lines.iter().map(|s| norm(s)).collect();
    // index base positions per normalized line
    let mut pos: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    for (i, l) in bn.iter().enumerate() {
        pos.entry(l.as_str()).or_default().push(i);
    }
    // longest matching run greedily (simple patience over repeats)
    let mut matches: Vec<(usize, usize, usize)> = vec![]; // (base, lines, len)
    let mut used_b = vec![false; base.len()];
    let mut used_l = vec![false; lines.len()];
    // seed with unique-line anchors first, then fill
    for pass in 0..2 {
        for (j, l) in ln.iter().enumerate() {
            if used_l[j] {
                continue;
            }
            let Some(cands) = pos.get(l.as_str()) else { continue };
            let free: Vec<usize> = cands.iter().filter(|&&i| !used_b[i]).cloned().collect();
            if free.is_empty() {
                continue;
            }
            if pass == 0 && free.len() != 1 {
                continue;
            }
            // extend run around (free[0], j)
            let mut bi = free[0];
            let mut li = j;
            while bi > 0 && li > 0 && !used_b[bi - 1] && !used_l[li - 1] && bn[bi - 1] == ln[li - 1] {
                bi -= 1;
                li -= 1;
            }
            let mut len = 0;
            while bi + len < base.len() && li + len < lines.len()
                && !used_b[bi + len] && !used_l[li + len] && bn[bi + len] == ln[li + len]
            {
                len += 1;
            }
            if len == 0 {
                continue;
            }
            for k in 0..len {
                used_b[bi + k] = true;
                used_l[li + k] = true;
            }
            matches.push((bi, li, len));
        }
    }
    matches.sort();
    let mut hunks = vec![];
    let mut cb = 0;
    let mut cl = 0;
    for (mb, ml, len) in matches {
        if cb != mb || cl != ml {
            hunks.push(Hunk { base_lo: cb, base_hi: mb, lines: lines[cl..ml].to_vec() });
        }
        cb = mb + len;
        cl = ml + len;
    }
    if cb < base.len() || cl < lines.len() {
        hunks.push(Hunk { base_lo: cb, base_hi: base.len(), lines: lines[cl..].to_vec() });
    }
    hunks.into_iter().filter(|h| h.base_lo != h.base_hi || !h.lines.is_empty()).collect()
}

fn norm(s: &str) -> String {
    s.trim().to_string()
}

pub fn apply_hunks(base: &[String], hunks: &[Hunk]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = 0;
    let mut sorted: Vec<&Hunk> = hunks.iter().collect();
    sorted.sort_by_key(|h| h.base_lo);
    for h in sorted {
        out.extend_from_slice(&base[cur..h.base_lo.min(base.len())]);
        out.extend(h.lines.iter().cloned());
        cur = h.base_hi.min(base.len());
    }
    out.extend_from_slice(&base[cur..]);
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConflict {
    pub base_lo: usize,
    pub base_hi: usize,
    pub variants: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub merged: Vec<String>,
    pub conflicts: Vec<MergeConflict>,
    pub needs_resolution: bool,
}

/// 3-way merge: non-overlapping hunks auto-apply; overlapping differing
/// hunks become conflicts resolved via `resolutions` (index per conflict).
pub fn merge_script(
    base: &[String],
    versions: &[Vec<String>],
    resolutions: &[usize],
) -> MergeResult {
    if versions.is_empty() {
        return MergeResult { merged: base.to_vec(), conflicts: vec![], needs_resolution: false };
    }
    if versions.len() == 1 {
        let h = line_hunks(base, &versions[0]);
        return MergeResult { merged: apply_hunks(base, &h), conflicts: vec![], needs_resolution: false };
    }
    // Collect per-version hunks; cluster overlapping ranges.
    let per: Vec<Vec<Hunk>> = versions.iter().map(|v| line_hunks(base, v)).collect();
    // Flatten to events: (lo, hi, version_idx)
    let mut events: Vec<(usize, usize, usize)> = vec![];
    for (vi, hs) in per.iter().enumerate() {
        for h in hs {
            events.push((h.base_lo, h.base_hi, vi));
        }
    }
    events.sort();
    // Cluster touching/overlapping ranges.
    let mut clusters: Vec<Vec<(usize, usize, usize)>> = vec![];
    for e in events {
        if let Some(last) = clusters.last_mut() {
            let max_hi = last.iter().map(|c| c.1.max(c.0 + 1)).max().unwrap_or(0);
            if e.0 <= max_hi {
                last.push(e);
                continue;
            }
        }
        clusters.push(vec![e]);
    }
    let mut merged = Vec::new();
    let mut conflicts = Vec::new();
    let mut cur = 0;
    let mut res_idx = 0;
    for cl in clusters {
        let lo = cl.iter().map(|c| c.0).min().unwrap();
        let hi = cl.iter().map(|c| c.1.max(c.0 + 1)).max().unwrap_or(lo);
        merged.extend_from_slice(&base[cur..lo.min(base.len())]);
        // Distinct outcomes across involved versions?
        let mut outcomes: Vec<Vec<String>> = vec![];
        for (_, _, vi) in &cl {
            let h = per[*vi].iter().find(|h| h.base_lo <= lo && lo <= h.base_hi.max(h.base_lo)).cloned();
            let applied = h.map(|h| h.lines).unwrap_or_else(|| base[lo..hi.min(base.len())].to_vec());
            if !outcomes.contains(&applied) {
                outcomes.push(applied);
            }
        }
        if outcomes.len() <= 1 {
            merged.extend(outcomes.into_iter().next().unwrap_or_default());
        } else {
            let pick = resolutions.get(res_idx).copied().unwrap_or(usize::MAX);
            if pick < outcomes.len() {
                merged.extend(outcomes[pick].clone());
            } else {
                conflicts.push(MergeConflict { base_lo: lo, base_hi: hi, variants: outcomes.clone() });
                // provisional: first variant, UI replaces after resolution
                merged.extend(outcomes[0].clone());
            }
            res_idx += 1;
        }
        cur = hi.min(base.len());
    }
    merged.extend_from_slice(&base[cur..]);
    let needs = !conflicts.is_empty();
    MergeResult { merged, conflicts, needs_resolution: needs }
}

/// Very small function scanner (python `scan_script` subset): `function NAME(` lines.
pub fn scan_functions(lines: &[String]) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, usize)> = None;
    let mut depth = 0i32;
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if t.starts_with("function ") || t.contains(" function ") {
            if let Some((n, s)) = cur.take() {
                out.push((n, s, i.saturating_sub(1)));
            }
            let name = t.split_whitespace().nth(1).unwrap_or("?").split('(').next().unwrap_or("?").to_string();
            cur = Some((name, i));
        }
        depth += l.chars().filter(|&c| c == '{').count() as i32;
        depth -= l.chars().filter(|&c| c == '}').count() as i32;
        let _ = depth;
    }
    if let Some((n, s)) = cur {
        out.push((n, s, lines.len().saturating_sub(1)));
    }
    out
}

/// Duplicate function definitions in merged output (python `duplicate_functions`).
pub fn duplicate_functions(merged: &[String]) -> Vec<String> {
    let fns = scan_functions(merged);
    let mut seen = std::collections::HashMap::new();
    let mut dups = vec![];
    for (name, _, _) in fns {
        let c = seen.entry(name.clone()).or_insert(0);
        *c += 1;
        if *c == 2 {
            dups.push(name);
        }
    }
    dups.sort();
    dups
}

/// Brace-balance faults per function (python `structure_problems` subset).
pub fn structure_problems(merged: &[String]) -> Vec<(String, String)> {
    let mut out = vec![];
    for (name, s, e) in scan_functions(merged) {
        let mut depth = 0i32;
        for l in &merged[s..=e.min(merged.len().saturating_sub(1))] {
            // strip line comments for balance check
            let code = l.split("//").next().unwrap_or("");
            depth += code.chars().filter(|&c| c == '{').count() as i32;
            depth -= code.chars().filter(|&c| c == '}').count() as i32;
            if depth < 0 {
                out.push((name.clone(), "extra closing brace".into()));
                break;
            }
        }
        if depth > 0 {
            out.push((name, "missing closing brace".into()));
        }
    }
    out
}

/// Full merge diagnostics for the resolver UI: dups + structure.
pub fn function_check(merged: &[String]) -> Vec<String> {
    let mut issues = vec![];
    for d in duplicate_functions(merged) {
        issues.push(format!("duplicate function {d}"));
    }
    for (f, w) in structure_problems(merged) {
        issues.push(format!("{f}: {w}"));
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean_merge_applies_both() {
        let base: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let v1: Vec<String> = vec!["a".into(), "B1".into(), "c".into(), "d".into()];
        let v2: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "D2".into()];
        let r = merge_script(&base, &[v1, v2], &[]);
        assert!(!r.needs_resolution);
        assert_eq!(r.merged, vec!["a", "B1", "c", "D2"]);
    }
    #[test]
    fn disjoint_multi_hunk() {
        let base: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into(), "f".into()];
        let v1: Vec<String> = vec!["a".into(), "B".into(), "c".into(), "d".into(), "e".into(), "F".into()];
        let h = line_hunks(&base, &v1);
        assert_eq!(h.len(), 2);
        let r = merge_script(&base, &[v1, base.clone()], &[]);
        assert!(!r.needs_resolution);
        assert_eq!(r.merged, vec!["a", "B", "c", "d", "e", "F"]);
    }
    #[test]
    fn conflict_detected() {        let base: Vec<String> = vec!["a".into(), "b".into()];
        let v1: Vec<String> = vec!["a".into(), "X".into()];
        let v2: Vec<String> = vec!["a".into(), "Y".into()];
        let r = merge_script(&base, &[v1, v2], &[]);
        assert!(r.needs_resolution);
        assert_eq!(r.conflicts.len(), 1);
        let r2 = merge_script(&base, &[vec!["a".into(), "X".into()], vec!["a".into(), "Y".into()]], &[1]);
        assert!(!r2.needs_resolution);
        assert_eq!(r2.merged, vec!["a", "Y"]);
    }
    #[test]
    fn roundtrip_utf16() {
        let lines = vec!["function foo() {".to_string(), "}".to_string()];
        let enc = encode_script(&lines, ScriptEncoding::Utf16Le);
        let (dec, fmt) = decode_script(&enc);
        assert_eq!(fmt, ScriptEncoding::Utf16Le);
        assert_eq!(dec, lines);
    }
    #[test]
    fn diagnostics() {
        let m: Vec<String> = vec!["function a() {".into(), "}".into(), "function a() {".into(), "}".into()];
        assert_eq!(duplicate_functions(&m), vec!["a".to_string()]);
        let bad: Vec<String> = vec!["function b() {".into(), "if (x) {".into()];
        assert!(!structure_problems(&bad).is_empty());
    }
}
