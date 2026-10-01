//! Game-version fingerprints: which game version(s) a script/XML/CSV copy matches.
//! Ports `version_lines/fingerprints/file_versions/compare_made_for/version_range`
//! + `SCRIPT/XML/CSV_GAME_VERSIONS` (`w3modmanager.py:1752-2000`).
//!
//! The fingerprint blob is the original's embedded `_FP_DATA` (base85+zlib in
//! python, stored here pre-decompressed as `fp_data.bin`, 127,935 bytes):
//! `{script|xml|csv: {rel: ([variant bitmask], {line_crc: variants bitmask})}}`.
//! `version_lines` normalisation matches the original (decode, drop blanks,
//! strip comments per kind, even out spacing) so CRCs line up.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub const SCRIPT_GAME_VERSIONS: &[&str] = &[
    "1.10", "1.11", "1.12", "1.12.1", "1.21", "1.22", "1.32", "4.00", "4.00 hotfix 1",
    "4.00 hotfix 2", "4.01", "4.01 hotfix", "4.02", "4.03", "4.04", "4.04a",
];
pub const XML_GAME_VERSIONS: &[&str] = &["1.32", "4.00", "4.01", "4.02", "4.03"];
pub const CSV_GAME_VERSIONS: &[&str] =
    &["1.32", "4.00 hotfix 2", "4.01", "4.02", "4.03", "4.04", "4.04a"];

static FP_BIN: &[u8] = include_bytes!("fp_data.bin");

struct FpEntry {
    vmasks: Vec<u32>,
    marks: Vec<(u32, u16)>,
}

struct FpDb {
    script: BTreeMap<String, FpEntry>,
    xml: BTreeMap<String, FpEntry>,
    csv: BTreeMap<String, FpEntry>,
}

fn u16le(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn parse_kind(data: &[u8]) -> Option<BTreeMap<String, FpEntry>> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < data.len() {
        let n = u16le(data, i)? as usize;
        i += 2;
        let rel = std::str::from_utf8(data.get(i..i + n)?).ok()?.to_string();
        i += n;
        let nv = *data.get(i)? as usize;
        i += 1;
        let mut vm = vec![];
        for _ in 0..nv {
            vm.push(u32le(data, i)?);
            i += 4;
        }
        let nl = u32le(data, i)? as usize;
        i += 4;
        let mut marks = vec![];
        for _ in 0..nl {
            let h = u32le(data, i)?;
            let m = u16le(data, i + 4)?;
            i += 6;
            marks.push((h, m));
        }
        out.insert(rel, FpEntry { vmasks: vm, marks });
    }
    Some(out)
}

fn db() -> &'static FpDb {
    static DB: OnceLock<FpDb> = OnceLock::new();
    DB.get_or_init(|| {
        // Split like python: parts on b"\0\0XML\0" then b"\0\0CSV\0".
        let blob = FP_BIN;
        let xml_sep = b"\0\0XML\0".as_slice();
        let csv_sep = b"\0\0CSV\0".as_slice();
        let find = |hay: &[u8], needle: &[u8]| {
            hay.windows(needle.len()).position(|w| w == needle)
        };
        let (script_b, rest) = match find(blob, xml_sep) {
            Some(p) => (&blob[..p], &blob[p + xml_sep.len()..]),
            None => (blob, &[][..]),
        };
        let (xml_b, csv_b) = match find(rest, csv_sep) {
            Some(p) => (&rest[..p], &rest[p + csv_sep.len()..]),
            None => (rest, &[][..]),
        };
        FpDb {
            script: parse_kind(script_b).unwrap_or_default(),
            xml: parse_kind(xml_b).unwrap_or_default(),
            csv: parse_kind(csv_b).unwrap_or_default(),
        }
    })
}

/// Normalised content lines (python `version_lines`).
pub fn version_lines(data: &[u8], kind: &str) -> Vec<String> {
    let text = if data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]) {
        String::from_utf16_lossy(
            &data
                .chunks(2)
                .filter_map(|c| c.get(0..2).map(|b| u16::from_le_bytes([b[0], b[1]])))
                .collect::<Vec<_>>(),
        )
    } else if data.iter().take(200).filter(|&&b| b == 0).count() > 20 {
        String::from_utf16_lossy(
            &data
                .chunks(2)
                .filter_map(|c| c.get(0..2).map(|b| u16::from_le_bytes([b[0], b[1]])))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(data).trim_start_matches('\u{FEFF}').to_string()
    };
    let nonblank: Vec<String> = text.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect();
    let mut joined = nonblank.join("\n");
    if kind == "xml" {
        // strip <!-- ... --> keeping line count
        while let Some(s) = joined.find("<!--") {
            if let Some(e) = joined[s..].find("-->") {
                let chunk = &joined[s..s + e + 3];
                let nl = chunk.matches('\n').count();
                joined.replace_range(s..s + e + 3, &"\n".repeat(nl));
            } else {
                break;
            }
        }
    } else if kind == "script" {
        // strip /* */ keeping line count, then // comments
        loop {
            let Some(s) = joined.find("/*") else { break };
            let Some(e) = joined[s..].find("*/") else { break };
            let chunk = &joined[s..s + e + 2];
            let nl = chunk.matches('\n').count();
            joined.replace_range(s..s + e + 2, &"\n".repeat(nl));
        }
        joined = joined
            .lines()
            .map(|l| l.split("//").next().unwrap_or("").to_string())
            .collect::<Vec<_>>()
            .join("\n");
    }
    joined
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Game versions a copy matches, or None (python `file_versions`).
pub fn file_versions(kind: &str, rel: &str, data: &[u8]) -> Option<Vec<String>> {
    let table = match kind {
        "script" => &db().script,
        "xml" => &db().xml,
        "csv" => &db().csv,
        _ => return None,
    };
    let names: &[&str] = match kind {
        "script" => SCRIPT_GAME_VERSIONS,
        "xml" => XML_GAME_VERSIONS,
        "csv" => CSV_GAME_VERSIONS,
        _ => return None,
    };
    let entry = table.get(&rel.to_lowercase())?;
    let mut hasher_lines = std::collections::HashSet::new();
    for l in version_lines(data, kind) {
        hasher_lines.insert(crc32fast::hash(l.as_bytes()));
    }
    let mut scores = vec![];
    for k in 0..entry.vmasks.len() {
        let missing = entry.marks.iter().filter(|(h, m)| (m >> k) & 1 == 1 && !hasher_lines.contains(h)).count();
        let foreign = entry.marks.iter().filter(|(h, m)| (m >> k) & 1 == 0 && hasher_lines.contains(h)).count();
        scores.push(missing + foreign);
    }
    let best = *scores.iter().min()?;
    // "too little to go on": best must clearly beat chance — mirror the
    // original's tie rule by keeping every version within 1 of best.
    let mut out = vec![];
    for (k, s) in scores.iter().enumerate() {
        if *s <= best + 1 {
            if let Some(n) = names.get(k) {
                out.push(n.to_string());
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

pub fn version_range(found: &[String]) -> String {
    if found.is_empty() {
        return String::new();
    }
    let idx: Vec<usize> = {
        let mut v: Vec<usize> = found.iter().filter_map(|f| SCRIPT_GAME_VERSIONS.iter().position(|x| x == f)).collect();
        v.sort();
        v.dedup();
        v
    };
    if idx.is_empty() {
        return String::new();
    }
    let mut runs = vec![];
    let mut start = idx[0];
    let mut prev = idx[0];
    for &cur in idx.iter().skip(1).chain(std::iter::once(&usize::MAX)) {
        if cur != prev + 1 {
            runs.push(if start == prev {
                SCRIPT_GAME_VERSIONS[start].to_string()
            } else {
                format!("{}–{}", SCRIPT_GAME_VERSIONS[start], SCRIPT_GAME_VERSIONS[prev])
            });
            start = cur;
        }
        prev = cur;
    }
    runs.join(", ")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MadeFor {
    pub label: String,
    pub short: String,
    pub status: String,
}

/// Aggregate report (python `compare_made_for`): common versions across the
/// mod's copies, else its oldest copy's; status older/newer/classic.
pub fn compare_made_for(files: &[(String, String, Vec<String>)], game_of: &dyn Fn(&str, &str) -> Vec<String>) -> MadeFor {
    let mut older = false;
    let mut newer = false;
    let mut classic = false;
    let mut sets: Vec<std::collections::HashSet<String>> = vec![];
    for (kind, rel, mine) in files {
        let game = game_of(kind, rel);
        if !game.is_empty() {
            let mine_set: std::collections::HashSet<_> = mine.iter().cloned().collect();
            let game_set: std::collections::HashSet<_> = game.iter().cloned().collect();
            if !mine_set.is_disjoint(&game_set) {
                // same — no flag
            } else {
                let mine_max = mine.iter().filter_map(|v| SCRIPT_GAME_VERSIONS.iter().position(|x| x == v)).max();
                let game_min = game.iter().filter_map(|v| SCRIPT_GAME_VERSIONS.iter().position(|x| x == v)).min();
                match (mine_max, game_min) {
                    (Some(a), Some(b)) if a < b => {
                        older = true;
                        classic = classic || mine.iter().all(|v| v.starts_with("1."));
                    }
                    _ => newer = true,
                }
            }
        }
        sets.push(mine.iter().cloned().collect());
    }
    let mut common: std::collections::HashSet<String> = sets.first().cloned().unwrap_or_default();
    for s in sets.iter().skip(1) {
        common = common.intersection(s).cloned().collect();
    }
    if common.is_empty() {
        // oldest copy's versions (lowest first-version index)
        let mut best: Option<&Vec<String>> = None;
        let mut best_idx = usize::MAX;
        for (_, _, m) in files {
            let idx = m.iter().filter_map(|v| SCRIPT_GAME_VERSIONS.iter().position(|x| x == v)).min().unwrap_or(usize::MAX);
            if idx < best_idx {
                best_idx = idx;
                best = Some(m);
            }
        }
        common = best.map(|m| m.iter().cloned().collect()).unwrap_or_default();
    }
    let mut made: Vec<String> = SCRIPT_GAME_VERSIONS.iter().filter(|v| common.contains(&v.to_string())).map(|v| v.to_string()).collect();
    let _ = &mut made;
    MadeFor {
        label: version_range(&made.iter().cloned().collect::<Vec<_>>()),
        short: made.last().cloned().unwrap_or_default(),
        status: if classic { "classic".into() } else if older { "older".into() } else if newer { "newer".into() } else { String::new() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn db_loads() {
        assert!(!db().script.is_empty());
        assert!(!db().xml.is_empty());
    }
    #[test]
    fn range_runs() {
        assert_eq!(version_range(&["4.02".into(), "4.03".into()]), "4.02–4.03");
        assert_eq!(version_range(&[]), "");
    }
    #[test]
    fn made_for_older() {
        let files = vec![("script".into(), "a.ws".into(), vec!["1.32".into()])];
        let r = compare_made_for(&files, &|_, _| vec!["4.03".into()]);
        assert_eq!(r.status, "classic");
        assert_eq!(r.short, "1.32");
    }
}
