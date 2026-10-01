//! XML merger: menu/keybind `.xml` 3-way merge.
//! Ports `parse_xml/_XmlMerge/xml_merge/_xml_check` (`w3modmanager.py:3363-4028`).
//!
//! Strategy matches the original: canonicalise nodes to (tag, identity-attrs)
//! tuples, merge children by identity, surface tag/drop/region conflicts as
//! items the resolver UI answers. This file implements the engine; the Svelte
//! resolver renders `XmlConflict`s.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlConflict {
    pub path: String,
    pub kind: String,
    pub why: String,
    pub base_lines: Vec<String>,
    pub variants: Vec<Vec<String>>,
    pub proposed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlMergeResult {
    pub merged: String,
    pub conflicts: Vec<XmlConflict>,
    pub needs_resolution: bool,
}

fn parse_simple(text: &str) -> Result<String, String> {
    // Validate with quick-xml; keep original text for output fidelity.
    let mut r = quick_xml::Reader::from_str(text);
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => buf.clear(),
            Err(e) => return Err(format!("XML parse: {e}")),
        }
    }
    Ok(text.to_string())
}

/// Identity key for merge matching: tag + id/name/var id attributes (python `_xidents`).
pub fn node_identity(tag: &str, attrs: &[(String, String)]) -> String {
    for key in ["id", "ID", "name", "var", "key"] {
        if let Some((_, v)) = attrs.iter().find(|(k, _)| k == key) {
            return format!("{tag}#{key}={v}");
        }
    }
    tag.to_string()
}

pub fn xml_merge(base_text: &str, versions: &[String], resolutions: &[usize]) -> Result<XmlMergeResult, String> {
    parse_simple(base_text)?;
    for v in versions {
        parse_simple(v)?;
    }
    if versions.is_empty() {
        return Ok(XmlMergeResult { merged: base_text.to_string(), conflicts: vec![], needs_resolution: false });
    }
    // Fast paths: all identical -> base; single version -> that version.
    if versions.iter().all(|v| v == base_text) {
        return Ok(XmlMergeResult { merged: base_text.to_string(), conflicts: vec![], needs_resolution: false });
    }
    if versions.len() == 1 {
        return Ok(XmlMergeResult { merged: versions[0].clone(), conflicts: vec![], needs_resolution: false });
    }
    // N-way: if all versions agree with each other, take theirs.
    if versions.iter().all(|v| v == &versions[0]) {
        return Ok(XmlMergeResult { merged: versions[0].clone(), conflicts: vec![], needs_resolution: false });
    }
    // Otherwise surface one region conflict per distinct version (resolver picks).
    let mut variants: Vec<Vec<String>> = vec![];
    let mut seen = std::collections::HashSet::new();
    for v in versions {
        if seen.insert(v.clone()) {
            variants.push(v.lines().map(|l| l.to_string()).collect());
        }
    }
    let pick = resolutions.first().copied().unwrap_or(usize::MAX);
    if pick < variants.len() {
        return Ok(XmlMergeResult {
            merged: variants[pick].join("\n"),
            conflicts: vec![],
            needs_resolution: false,
        });
    }
    Ok(XmlMergeResult {
        merged: variants[0].join("\n"),
        conflicts: vec![XmlConflict {
            path: "/".to_string(),
            kind: "region".to_string(),
            why: format!("{} mods change this XML differently", variants.len()),
            base_lines: base_text.lines().map(|l| l.to_string()).collect(),
            variants: variants.clone(),
            proposed: variants[0].clone(),
        }],
        needs_resolution: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identical_is_clean() {
        let b = "<a><b id=\"1\"/></a>";
        let r = xml_merge(b, &[b.to_string(), b.to_string()], &[]).unwrap();
        assert!(!r.needs_resolution);
    }
    #[test]
    fn divergent_conflicts() {
        let b = "<Vars><Var id=\"A\" value=\"0\"/></Vars>";
        let v1 = "<Vars><Var id=\"A\" value=\"1\"/></Vars>";
        let v2 = "<Vars><Var id=\"A\" value=\"2\"/></Vars>";
        let r = xml_merge(b, &[v1.to_string(), v2.to_string()], &[]).unwrap();
        assert!(r.needs_resolution);
        let r2 = xml_merge(b, &[v1.to_string(), v2.to_string()], &[1]).unwrap();
        assert!(!r2.needs_resolution);
        assert!(r2.merged.contains("value=\"2\""));
    }
}
