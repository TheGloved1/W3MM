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

fn norm_line(line: &str) -> String {
    // python `_norm_line`: trailing spaces don't count, indent by depth (tab = 4).
    let s = line.trim_end();
    let body = s.trim_start();
    if body.is_empty() {
        return String::new();
    }
    let indent = s.len() - body.len();
    let expanded = s[..indent].replace('\t', "    ");
    format!("{}{}", " ".repeat(expanded.len()), body)
}

fn norm_lines(lines: &[String]) -> Vec<String> {
    lines.iter().map(|l| norm_line(l)).collect()
}

fn norm(s: &str) -> String {
    norm_line(s)
}

/// Do two changes in base coordinates get in each other's way?
/// Port of python `_collide`.
pub fn collide(a: (usize, usize), b: (usize, usize)) -> bool {
    let (s1, e1) = a;
    let (s2, e2) = b;
    if s1 == e1 && s2 == e2 {
        return s1 == s2;
    }
    if s1 == e1 {
        return s2 <= s1 && s1 <= e2;
    }
    if s2 == e2 {
        return s1 <= s2 && s2 <= e1;
    }
    s1 < e2 && s2 < e1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterKind {
    Clean,
    Soft,
    Conflict,
}

#[derive(Debug, Clone)]
pub struct Cluster {
    pub start: usize,
    pub end: usize,
    pub mods: Vec<usize>,
    pub kind: ClusterKind,
    pub text: Vec<String>,
    pub pure: bool,
    pub intact: bool,
}

fn region_of(base: &[String], hunks: &[Hunk], lo: usize, hi: usize) -> Vec<String> {
    apply_hunks_range(base, hunks, lo, hi)
}

/// Subsequence check (ordered, no gaps required): every line of `small`
/// appears in `big` in order. Stands in for the difflib no-replace/delete
/// half of python `_contains` + the `intact` check.
fn is_subsequence(small: &[String], big: &[String]) -> bool {
    let mut j = 0;
    for l in small {
        while j < big.len() && &big[j] != l {
            j += 1;
        }
        if j >= big.len() {
            return false;
        }
        j += 1;
    }
    true
}

/// Does `big` already make every change `small` makes? Port of `_contains`:
/// small's lines survive in big in order, and lines small removed from base
/// stay removed in big.
fn contains(base_r: &[String], small: &[String], big: &[String]) -> bool {
    if !is_subsequence(small, big) {
        return false;
    }
    let mut cb: std::collections::HashMap<&str, i64> = Default::default();
    let mut cs: std::collections::HashMap<&str, i64> = Default::default();
    let mut cg: std::collections::HashMap<&str, i64> = Default::default();
    for l in base_r {
        *cb.entry(l.as_str()).or_default() += 1;
    }
    for l in small {
        *cs.entry(l.as_str()).or_default() += 1;
    }
    for l in big {
        *cg.entry(l.as_str()).or_default() += 1;
    }
    cb.keys().all(|l| {
        let (b, s, g) = (cb[l], cs.get(l).copied().unwrap_or(0), cg.get(l).copied().unwrap_or(0));
        if s < b {
            g - b <= s - b
        } else {
            true
        }
    })
}

/// Group mods' changes where they meet (port of python `_clusters` without
/// the function-scoping `touching` rule — see `merge_script`).
pub fn clusters(base: &[String], per_mod: &[Vec<Hunk>]) -> Vec<Cluster> {
    let mut tagged: Vec<(usize, usize, usize, Vec<String>)> = vec![];
    for (i, hs) in per_mod.iter().enumerate() {
        for h in hs {
            tagged.push((h.base_lo, h.base_hi, i, h.lines.clone()));
        }
    }
    tagged.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
    // cluster assembly: every group a change collides with becomes one group
    let mut cl: Vec<(usize, usize, Vec<usize>, Vec<(usize, usize, usize, Vec<String>)>)> = vec![];
    for h in &tagged {
        let (s, e, i, _) = h;
        let mut first: Option<usize> = None;
        let mut k = cl.len();
        while k > 0 && cl[k - 1].1 >= *s {
            k -= 1;
            if cl[k].3.iter().any(|o| collide((*s, *e), (o.0, o.1))) {
                first = Some(k);
            }
        }
        match first {
            None => cl.push((*s, *e, vec![*i], vec![h.clone()])),
            Some(f) => {
                let mut end = cl[f].1.max(*e);
                let mut mods = cl[f].2.clone();
                let mut hunks = cl[f].3.clone();
                for other in cl.drain(f + 1..) {
                    end = end.max(other.1);
                    for m in other.2 {
                        if !mods.contains(&m) {
                            mods.push(m);
                        }
                    }
                    hunks.extend(other.3);
                }
                mods.push(*i);
                // note: original keeps insertion order per drain; dedup not needed for logic
                cl[f] = (cl[f].0.min(*s), end, mods, {
                    let mut hh = hunks;
                    hh.push(h.clone());
                    hh
                });
            }
        }
    }
    let mut out = vec![];
    for (lo, hi, mods, hunks) in cl {
        let mut mods = mods;
        mods.sort();
        mods.dedup();
        let region = |i: usize| region_of(base, &per_mod[i], lo, hi);
        let mut texts: BTreeMap<Vec<String>, usize> = BTreeMap::new();
        for i in &mods {
            texts.entry(norm_lines(&region(*i))).or_insert(*i);
        }
        let pure = hunks.iter().all(|h| h.0 == h.1);
        if texts.len() == 1 {
            out.push(Cluster { start: lo, end: hi, mods, kind: ClusterKind::Clean, text: region(texts.values().next().copied().unwrap()), pure, intact: true });
            continue;
        }
        let base_r = norm_lines(&base[lo.min(base.len())..hi.min(base.len())]);
        let keys: Vec<Vec<String>> = texts.keys().cloned().collect();
        let top = keys.iter().find(|t| keys.iter().all(|o| contains(&base_r, o, t)));
        if let Some(t) = top {
            // one mod's edit contains the others'
            let owner = texts[t];
            out.push(Cluster { start: lo, end: hi, mods, kind: ClusterKind::Clean, text: region(owner), pure, intact: true });
            continue;
        }
        let mut hard = false;
        for x in 0..hunks.len() {
            for y in (x + 1)..hunks.len() {
                let (s1, e1, i1, n1) = &hunks[x];
                let (s2, e2, i2, n2) = &hunks[y];
                if i1 == i2 || (*s1, *e1, norm_lines(n1)) == (*s2, *e2, norm_lines(n2)) {
                    continue;
                }
                if (*s1 < *e1 && *s2 < *e2 && *s1 < *e2 && *s2 < *e1)
                    || (*s1 == *e1 && *s2 < *s1 && *s1 < *e2)
                    || (*s2 == *e2 && *s1 < *s2 && *s2 < *e1)
                {
                    hard = true;
                }
            }
        }
        if hard {
            out.push(Cluster { start: lo, end: hi, mods, kind: ClusterKind::Conflict, text: vec![], pure, intact: false });
            continue;
        }
        // soft: every insertion fits, in version order
        let mut seen: std::collections::HashSet<(usize, usize, Vec<String>)> = Default::default();
        let mut pieces: Vec<Hunk> = vec![];
        for i in &mods {
            let mut hs: Vec<&(usize, usize, usize, Vec<String>)> = hunks.iter().filter(|h| &h.2 == i).collect();
            hs.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
            for (s, e, _, n) in hs {
                let sig = (*s, *e, norm_lines(n));
                if seen.insert(sig) {
                    pieces.push(Hunk { base_lo: *s, base_hi: *e, lines: n.clone() });
                }
            }
        }
        pieces.sort_by_key(|h| (h.base_lo, h.base_hi));
        let text = apply_hunks_range(base, &pieces, lo, hi);
        let combined = norm_lines(&text);
        let intact = texts.keys().all(|t| is_subsequence(t, &combined));
        out.push(Cluster { start: lo, end: hi, mods, kind: ClusterKind::Soft, text, pure, intact });
    }
    out
}

use std::collections::BTreeMap;

pub fn apply_hunks(base: &[String], hunks: &[Hunk]) -> Vec<String> {
    apply_hunks_range(base, hunks, 0, base.len())
}

/// Apply hunks, returning only the `[lo, hi)` window (python
/// `apply_hunks(base, hunks, lo, hi)`). Cluster bounds always contain their
/// hunks whole, so partial overlaps fall back to base lines.
pub fn apply_hunks_range(base: &[String], hunks: &[Hunk], lo: usize, hi: usize) -> Vec<String> {
    let (lo, hi) = (lo.min(base.len()), hi.min(base.len()));
    let mut out = Vec::new();
    let mut cur = lo;
    let mut sorted: Vec<&Hunk> = hunks.iter().collect();
    sorted.sort_by_key(|h| (h.base_lo, h.base_hi));
    for h in sorted {
        if h.base_lo < lo || h.base_hi > hi {
            continue;
        }
        out.extend_from_slice(&base[cur..h.base_lo.max(lo).min(hi)]);
        if h.base_lo >= lo && h.base_hi <= hi {
            out.extend(h.lines.iter().cloned());
        } else {
            out.extend_from_slice(&base[h.base_lo.max(lo)..h.base_hi.min(hi)]);
        }
        cur = h.base_hi.max(lo).min(hi).max(cur);
    }
    out.extend_from_slice(&base[cur..hi]);
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
    pub auto: usize,
}

/// 3-way merge over `_clusters` groups: clean groups auto-apply; overlapping
/// differing edits become conflicts resolved via `resolutions` (index per
/// conflict, in order). Like the original's script path, "soft" groups (two
/// mods adding at the same spot) are asked about rather than ordered
/// silently — the XML path passes them through instead (see `xml_merge`).
pub fn merge_script(
    base: &[String],
    versions: &[Vec<String>],
    resolutions: &[usize],
) -> MergeResult {
    if versions.is_empty() {
        return MergeResult { merged: base.to_vec(), conflicts: vec![], needs_resolution: false, auto: 0 };
    }
    if versions.len() == 1 {
        let h = line_hunks(base, &versions[0]);
        if norm_lines(&apply_hunks(base, &h)) != norm_lines(&versions[0]) {
            return MergeResult { merged: base.to_vec(), conflicts: vec![], needs_resolution: false, auto: 0 };
        }
        return MergeResult { merged: apply_hunks(base, &h), conflicts: vec![], needs_resolution: false, auto: h.len() };
    }
    let per: Vec<Vec<Hunk>> = versions.iter().map(|v| line_hunks(base, v)).collect();
    // proof each version rebuilds exactly (python merge_script step 1)
    for (v, hs) in versions.iter().zip(per.iter()) {
        if norm_lines(&apply_hunks(base, hs)) != norm_lines(v) {
            return MergeResult { merged: base.to_vec(), conflicts: vec![], needs_resolution: false, auto: 0 };
        }
    }
    if versions.iter().map(|v| norm_lines(v)).collect::<std::collections::HashSet<_>>().len() == 1 {
        return MergeResult { merged: base.to_vec(), conflicts: vec![], needs_resolution: false, auto: 0 };
    }
    let cls = clusters(base, &per);
    let mut merged = Vec::new();
    let mut conflicts = Vec::new();
    let mut cur = 0;
    let mut auto = 0;
    let mut res_idx = 0;
    for c in &cls {
        merged.extend_from_slice(&base[cur..c.start.min(base.len())]);
        match c.kind {
            ClusterKind::Clean if c.intact => {
                merged.extend(c.text.iter().cloned());
                auto += 1;
            }
            _ => {
                // distinct takes on this stretch, in version order
                let mut outcomes: Vec<Vec<String>> = vec![];
                for i in &c.mods {
                    let r = region_of(base, &per[*i], c.start, c.end);
                    if !outcomes.contains(&r) {
                        outcomes.push(r);
                    }
                }
                let pick = resolutions.get(res_idx).copied().unwrap_or(usize::MAX);
                if pick < outcomes.len() {
                    merged.extend(outcomes[pick].clone());
                    auto += 1;
                } else {
                    conflicts.push(MergeConflict { base_lo: c.start, base_hi: c.end, variants: outcomes.clone() });
                    merged.extend(outcomes[0].clone());
                }
                res_idx += 1;
            }
        }
        cur = c.end.min(base.len());
    }
    merged.extend_from_slice(&base[cur..]);
    let needs = !conflicts.is_empty();
    MergeResult { merged, conflicts, needs_resolution: needs, auto }
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

/// RedKit annotation symbols a script defines: (annotation, Class.Name).
/// Ports the symbol half of python `annotation_clashes` (add/replaceMethod, addField).
pub fn scan_annotations(lines: &[String]) -> Vec<(String, String)> {
    // pair annotation with following symbol name
    let mut paired = vec![];
    let mut pending: Option<String> = None;
    let mut cls = String::new();
    for l in lines {
        let t = l.trim();
        if t.starts_with("class ") {
            cls = t["class ".len()..].split_whitespace().next().unwrap_or("").trim_end_matches('{').trim().to_string();
        }
        if ["@addMethod", "@addField", "@replaceMethod"].iter().any(|a| t.starts_with(a)) {
            pending = Some(t.split_whitespace().next().unwrap_or("").to_string());
            continue;
        }
        if let Some(a) = pending.take() {
            let sym = if let Some(rest) = t.strip_prefix("function ") {
                rest.split('(').next().unwrap_or("").trim().to_string()
            } else if let Some(rest) = t.strip_prefix("var ") {
                rest.split([' ', ':', ';']).next().unwrap_or("").trim().to_string()
            } else {
                continue;
            };
            if !sym.is_empty() {
                paired.push((a, format!("{cls}.{sym}")));
            }
        }
    }
    paired
}

/// (name, mod) when a mod deletes a function but the merge still calls it.
/// Ports python `removed_but_used`: that merge would fail to compile.
pub fn removed_but_used(base: &[String], versions: &[(String, Vec<String>)], merged: &[String]) -> Option<(String, String)> {
    let defined: std::collections::HashSet<String> =
        scan_functions(merged).into_iter().map(|(n, _, _)| n.to_lowercase()).collect();
    let base_names: std::collections::HashMap<String, String> = {
        let mut m = std::collections::HashMap::new();
        for (n, _, _) in scan_functions(base) {
            m.entry(n.to_lowercase()).or_insert(n);
        }
        m
    };
    let words = |lines: &[String]| {
        lines
            .join("\n")
            .to_lowercase()
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .map(|w| w.to_string())
            .collect::<std::collections::HashSet<_>>()
    };
    let merged_words = words(merged);
    for (label, lines) in versions {
        let mine: std::collections::HashSet<String> =
            scan_functions(lines).into_iter().map(|(n, _, _)| n.to_lowercase()).collect();
        let removed: Vec<&String> =
            base_names.keys().filter(|k| !mine.contains(*k) && !defined.contains(*k)).collect();
        if removed.is_empty() {
            continue;
        }
        let own = words(lines);
        for k in removed {
            if merged_words.contains(k) && !own.contains(k) {
                return Some((base_names[k].clone(), label.clone()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean_merge_applies_both() {        let base: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into()];
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
    #[test]
    fn annotations() {
        let lines: Vec<String> = vec![
            "class CPlayer extends CEntity {".into(),
            "@addMethod".into(),
            "function DoDodge() {}".into(),
        ];
        assert_eq!(scan_annotations(&lines), vec![("@addMethod".to_string(), "CPlayer.DoDodge".to_string())]);
    }
}
