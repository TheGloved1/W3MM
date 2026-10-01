//! XML merger: menu/keybind `.xml` 3-way merge.
//! Ports `parse_xml/XNode/_xcanon/_xidents/_xdrops/_xloses/_xtok/_xlabel/
//! _open_tag/_xrender/_XmlMerge.merge_node/tag_item/drop_item/merge_children/
//! region_item/xml_merge` (`w3modmanager.py:3359-3982`).
//!
//! How it works, matching the original: children are matched by identity —
//! `(tag, key attr, value)` for keyed elements (`id`/`name`/`varId`), place
//! for containers, full content for the rest, repeats numbered — and the two
//! identity streams are merged with the same hunk/cluster engine as scripts.
//! Keyed elements a mod drops are asked about; attribute clashes and text
//! clashes become conflict items the resolver answers by index.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct XmlError(pub String);

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub const XML_KEYS: &[&str] = &["id", "name", "varId"];

#[derive(Debug, Clone)]
pub struct XNode {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<XNode>,
    /// whitespace/comments before each child
    pub pres: Vec<String>,
    pub texts: Vec<String>,
    pub tail: String,
    pub start: usize,
    pub open_end: usize,
    pub close_start: usize,
    pub end: usize,
    pub src: String,
}

impl XNode {
    pub fn raw(&self) -> String {
        self.src[self.start..self.end].to_string()
    }
    pub fn self_closing(&self) -> bool {
        self.src[self.start..self.open_end].trim_end().ends_with("/>")
    }
}

fn token_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| {
        regex::Regex::new(
            r#"(?s)(?P<comment><!--.*?-->)|(?P<pi><\?.*?\?>)|(?P<cdata><!\[CDATA\[.*?\]\]>)|(?P<decl><![^>]*>)|(?P<end></\s*(?P<etag>[^\s>]+)\s*>)|(?P<start><(?P<stag>[^\s/>!?]+)(?P<sattrs>(?:\s+[^\s=/>]+\s*=\s*(?:"[^"]*"|'[^']*'))*)\s*(?P<sclose>/?)>)|(?P<text>[^<]+)|(?P<bad><)"#,
        )
        .unwrap()
    })
}

fn attr_re() -> &'static regex::Regex {
    static R: OnceLock<regex::Regex> = OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r#"([^\s=/>]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap())
}

fn parse_attrs(s: &str) -> Vec<(String, String)> {
    attr_re()
        .captures_iter(s)
        .map(|c| {
            let v = if let Some(m) = c.get(2) { m.as_str() } else { c.get(3).map(|m| m.as_str()).unwrap_or("") };
            (c[1].to_string(), v.to_string())
        })
        .collect()
}

fn blank_node(tag: String, start: usize, open_end: usize) -> XNode {
    XNode {
        tag,
        attrs: vec![],
        children: vec![],
        pres: vec![],
        texts: vec![],
        tail: String::new(),
        start,
        open_end,
        close_start: open_end,
        end: open_end,
        src: String::new(),
    }
}

/// (prolog, root, epilog). Mirrors python `parse_xml` errors.
pub fn parse_xml(text: &str) -> Result<(String, XNode, String), XmlError> {
    let re = token_re();
    let mut root: Option<XNode> = None;
    let mut stack: Vec<XNode> = vec![];
    let mut gaps: Vec<String> = vec![]; // pending whitespace/comments per open element
    let mut last = 0;
    let mut pos = 0;
    while pos < text.len() {
        let m = re.find_at(text, pos).ok_or_else(|| XmlError("unreadable XML".into()))?;
        if m.start() != pos {
            return Err(XmlError(format!("stray character at offset {pos}")));
        }
        let whole = &text[m.start()..m.end()];
        let c = re.captures(whole).unwrap();
        pos = m.end();
        let kind = ["comment", "pi", "cdata", "decl", "end", "start", "text", "bad"]
            .into_iter()
            .find(|k| c.name(k).is_some())
            .unwrap_or("bad");
        match kind {
            "bad" => return Err(XmlError(format!("stray “<” at character {}", m.start()))),
            "pi" | "decl" => {
                if !stack.is_empty() {
                    return Err(XmlError("declaration inside an element".into()));
                }
                continue;
            }
            "comment" => {
                if stack.is_empty() {
                    continue;
                }
                gaps.last_mut().unwrap().push_str(whole);
                last = m.end();
                continue;
            }
            "cdata" => {
                if stack.is_empty() {
                    continue;
                }
                stack.last_mut().unwrap().texts.push(whole[9..whole.len() - 3].to_string());
                continue;
            }
            "start" => {
                let tag = c.name("stag").unwrap().as_str().to_string();
                let attrs = parse_attrs(c.name("sattrs").map(|x| x.as_str()).unwrap_or(""));
                let self_close = c.name("sclose").map(|x| !x.as_str().is_empty()).unwrap_or(false);
                let mut node = blank_node(tag, m.start(), m.end());
                node.attrs = attrs;
                if self_close {
                    node.close_start = m.end();
                    node.end = m.end();
                }
                if let Some(top) = stack.last_mut() {
                    let gap = std::mem::take(gaps.last_mut().unwrap());
                    top.pres.push(format!("{gap}{}", &text[last..m.start()]));
                    top.children.push(node);
                } else {
                    if root.is_some() {
                        return Err(XmlError("more than one root element".into()));
                    }
                    if self_close {
                        root = Some(node);
                    } else {
                        stack.push(node);
                        gaps.push(String::new());
                    }
                    last = m.end();
                    continue;
                }
                if !self_close {
                    // move the just-pushed child onto the stack as the open element
                    let top = stack.last_mut().unwrap();
                    let child = top.children.pop().unwrap();
                    stack.push(child);
                    gaps.push(String::new());
                }
                last = m.end();
                continue;
            }
            "end" => {
                let etag = c.name("etag").unwrap().as_str().to_string();
                let mut node = stack.pop().ok_or_else(|| XmlError(format!("“</{etag}>” with nothing to close")))?;
                let gap = gaps.pop().unwrap();
                if node.tag != etag {
                    return Err(XmlError(format!("“<{}>” closed by “</{etag}>”", node.tag)));
                }
                node.tail = format!("{gap}{}", &text[last..m.start()]);
                node.close_start = m.start();
                node.end = m.end();
                last = m.end();
                if let Some(top) = stack.last_mut() {
                    top.children.push(node);
                } else {
                    if root.is_some() {
                        return Err(XmlError("more than one root element".into()));
                    }
                    root = Some(node);
                }
                continue;
            }
            _ => {
                // text
                if stack.is_empty() {
                    if !whole.trim().is_empty() {
                        return Err(XmlError("text outside the root element".into()));
                    }
                    continue;
                }
                stack.last_mut().unwrap().texts.push(whole.to_string());
                continue;
            }
        }
    }
    if !stack.is_empty() {
        return Err(XmlError("an element is never closed".into()));
    }
    let mut r = root.ok_or_else(|| XmlError("no root element".into()))?;
    fill_src(&mut r, text);
    let (s, e) = (r.start, r.end);
    Ok((text[..s].to_string(), r, text[e..].to_string()))
}

fn fill_src(n: &mut XNode, text: &str) {
    n.src = text.to_string();
    for c in &mut n.children {
        fill_src(c, text);
    }
}

/// Canonical meaning of an element, ignoring comments/whitespace.
fn xcanon(n: &XNode) -> String {
    let mut attrs: Vec<String> = n.attrs.iter().map(|(a, v)| format!("{a}={v}")).collect();
    attrs.sort();
    let text: String = n.texts.join("").split_whitespace().collect::<Vec<_>>().join(" ");
    let kids: Vec<String> = n.children.iter().map(xcanon).collect();
    format!("{}({})[{}]{{{}}}", n.tag, attrs.join(","), text, kids.join(";"))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Ident {
    Key(String, String, String, usize),
    Place(String, usize),
    Content(String, usize),
}

impl Ident {
    fn kind(&self) -> char {
        match self {
            Ident::Key(..) => 'k',
            Ident::Place(..) => 'p',
            Ident::Content(..) => 'u',
        }
    }
    fn label(&self) -> String {
        match self {
            Ident::Key(tag, k, v, n) => format!("{tag}#{k}={v}#{n}"),
            Ident::Place(tag, n) => format!("{tag}@{n}"),
            Ident::Content(c, n) => format!("u{}#{n}", &c[..c.len().min(12)]),
        }
    }
}

fn xidents(children: &[XNode], containers: &HashSet<String>) -> Vec<Ident> {
    let mut out = vec![];
    let mut seen: HashMap<String, usize> = HashMap::new();
    for c in children {
        let n = if let Some(key) = XML_KEYS.iter().find(|k| c.attrs.iter().any(|(a, _)| a == *k)) {
            let v = c.attrs.iter().find(|(a, _)| a == *key).unwrap().1.clone();
            let base = format!("k:{}:{key}:{v}", c.tag);
            let r = *seen.entry(base).or_insert(0);
            Ident::Key(c.tag.clone(), key.to_string(), v, r)
        } else if containers.contains(&c.tag) {
            let base = format!("p:{}", c.tag);
            let r = *seen.entry(base).or_insert(0);
            Ident::Place(c.tag.clone(), r)
        } else {
            let canon = xcanon(c);
            let r = *seen.entry(format!("u:{canon}")).or_insert(0);
            Ident::Content(canon, r)
        };
        // bump the matching counter
        let base = match &n {
            Ident::Key(t, k, v, _) => format!("k:{t}:{k}:{v}"),
            Ident::Place(t, _) => format!("p:{t}"),
            Ident::Content(c, _) => format!("u:{c}"),
        };
        *seen.entry(base).or_insert(0) += 1;
        out.push(n);
    }
    out
}

fn xtok(ident: &Ident) -> String {
    use sha1::Digest;
    let mut h = sha1::Sha1::new();
    h.update(format!("{ident:?}").as_bytes());
    hex::encode(h.finalize())[..20].to_string()
}

fn open_tag(node: &XNode, attrs: &[(String, String)]) -> String {
    let raw = node.src[node.start..node.open_end].to_string();
    if node.attrs == attrs {
        return raw;
    }
    let head = node.tag.len() + 1;
    let mut out = vec![raw[..head].to_string()];
    let mut pos = head;
    for m in attr_re().captures_iter(&raw[head..]) {
        let whole = m.get(0).unwrap();
        let gs = whole.start() + head;
        let ge = whole.end() + head;
        let name = m.get(1).unwrap().as_str();
        out.push(raw[pos..gs].to_string());
        if let Some((_, val)) = attrs.iter().find(|(a, _)| a == name) {
            let q = if val.contains('"') { '\'' } else { '"' };
            out.push(format!("{name}={q}{val}{q}"));
        }
        pos = ge;
    }
    let rest = raw[pos..].to_string();
    let have: Vec<&str> = attr_re().captures_iter(&raw[head..]).map(|m| m.get(1).unwrap().as_str()).collect();
    let extra: String = attrs.iter().filter(|(n, _)| !have.contains(&n.as_str())).map(|(n, v)| format!(r#" {n}="{v}""#)).collect();
    let close = if rest.trim_end().ends_with("/>") { "/>" } else { ">" };
    let body = rest.trim_end();
    let body = &body[..body.len() - close.len()];
    out.push(format!("{extra}{body}{close}"));
    out.join("")
}

#[derive(Debug, Clone)]
enum Merged {
    Src(XNode),
    Made { open_src: XNode, attrs: Vec<(String, String)>, kids: Vec<(String, Box<Merged>)>, tail: String },
}

fn render(res: &Merged) -> String {
    match res {
        Merged::Src(n) => n.raw(),
        Merged::Made { open_src, attrs, kids, tail } => {
            let mut opening = open_tag(open_src, attrs);
            let body: String = kids.iter().map(|(pre, k)| format!("{pre}{}", render(k))).collect::<String>() + tail;
            if open_src.self_closing() {
                if kids.is_empty() && tail.is_empty() {
                    return opening;
                }
                opening = opening.trim_end().trim_end_matches("/>").trim_end().to_string() + ">";
                return format!("{opening}{body}</{}>", open_src.tag);
            }
            format!("{opening}{body}{}", open_src.src[open_src.close_start..open_src.end].to_string())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlVariant {
    pub mods: Vec<String>,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlConflict {
    pub path: String,
    pub kind: String,
    pub why: String,
    pub base_lines: Vec<String>,
    pub variants: Vec<XmlVariant>,
    pub proposed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XmlMergeResult {
    pub status: String,
    pub merged: String,
    pub conflicts: Vec<XmlConflict>,
    pub needs_resolution: bool,
    pub reason: String,
    pub auto: usize,
}

struct Merger<'a> {
    labels: Vec<String>,
    resolutions: &'a [usize],
    res_idx: usize,
    items: Vec<XmlConflict>,
    pending: usize,
    auto: usize,
}

impl<'a> Merger<'a> {
    /// Next user answer, if one was given for this conflict in order.
    fn pick(&mut self, n_opts: usize) -> Option<usize> {
        let r = *self.resolutions.get(self.res_idx)?;
        self.res_idx += 1;
        if r < n_opts {
            Some(r)
        } else {
            None
        }
    }

    fn merge_node(&mut self, path: &[String], b: Option<&XNode>, vs: &[(usize, &XNode)]) -> Result<Merged, XmlError> {
        let cb = b.map(xcanon);
        let changed: Vec<(usize, &XNode)> = vs.iter().filter(|(_, n)| Some(xcanon(n)) != cb).cloned().collect();
        if changed.is_empty() {
            let src = b.or(vs.first().map(|(_, n)| *n)).unwrap();
            return Ok(Merged::Src(src.clone()));
        }
        if changed.iter().map(|(_, n)| xcanon(n)).collect::<HashSet<_>>().len() == 1 {
            let (_, n) = changed[0];
            // all versions agree: take theirs unless it drops vanilla content
            if b.map(|bb| xloses(bb, n)).unwrap_or(false) {
                // falls through to the drop review below
            } else {
                self.auto += 1;
                return Ok(Merged::Src(n.clone()));
            }
        }
        // attributes, one by one
        let bd: BTreeMap<&str, &str> =
            b.map(|x| x.attrs.iter().map(|(a, v)| (a.as_str(), v.as_str())).collect()).unwrap_or_default();
        let mut names: Vec<String> = bd.keys().map(|s| s.to_string()).collect();
        for (_, n) in vs {
            for (a, _) in &n.attrs {
                if !names.contains(a) {
                    names.push(a.clone());
                }
            }
        }
        let mut final_attrs: Vec<(String, String)> = vec![];
        let mut clash: BTreeMap<String, BTreeMap<usize, Option<String>>> = BTreeMap::new();
        for name in &names {
            let bv = bd.get(name.as_str()).map(|s| s.to_string());
            let mut vals: BTreeMap<usize, Option<String>> = BTreeMap::new();
            for (i, n) in vs {
                vals.insert(*i, n.attrs.iter().find(|(a, _)| a == name).map(|(_, v)| v.clone()));
            }
            let mut ch: Vec<Option<String>> = vals.values().filter(|v| *v != &bv).cloned().collect();
            ch.sort();
            ch.dedup();
            if ch.len() > 1 {
                clash.insert(name.clone(), vals);
            }
            let val = if ch.len() == 1 { ch[0].clone() } else { bv };
            if let Some(v) = val {
                final_attrs.push((name.clone(), v));
            }
        }
        // text content
        let norm = |n: &XNode| n.texts.join("").split_whitespace().collect::<Vec<_>>().join(" ");
        let btext = b.map(norm);
        let mut texts: Vec<String> = vs.iter().map(|(_, n)| norm(n)).collect();
        texts.retain(|t| Some(t) != btext.as_ref());
        texts.sort();
        texts.dedup();
        if texts.len() > 1 {
            return Err(XmlError(format!("two mods change the text inside {} differently", path.join("/"))));
        }
        if !clash.is_empty() {
            let src = b.or(vs.first().map(|(_, n)| *n)).unwrap();
            // one variant per distinct value signature
            let bsign: Vec<Option<String>> = clash.keys().map(|k| bd.get(k.as_str()).map(|s| s.to_string())).collect();
            let mut groups: BTreeMap<Vec<Option<String>>, Vec<usize>> = BTreeMap::new();
            for (i, _) in vs {
                let sig: Vec<Option<String>> = clash.keys().map(|k| clash[k][i].clone()).collect();
                if Some(&sig) != Some(&bsign) {
                    groups.entry(sig).or_default().push(*i);
                }
            }
            let variants: Vec<XmlVariant> = groups
                .values()
                .map(|who| XmlVariant {
                    mods: {
                        let mut mm: Vec<String> = who.iter().map(|i| self.labels[*i].clone()).collect();
                        mm.sort();
                        mm
                    },
                    lines: open_tag(src, &final_attrs).lines().map(|l| l.to_string()).collect(),
                })
                .collect();
            let base_lines = open_tag(src, &b.map(|x| x.attrs.clone()).unwrap_or_default())
                .lines()
                .map(|l| l.to_string())
                .collect();
            match self.pick(variants.len().max(1)) {
                Some(p) => {
                    let sig = groups.keys().nth(p).cloned().unwrap_or_default();
                    for (k, name) in clash.keys().enumerate() {
                        match sig.get(k) {
                            Some(Some(v)) => {
                                if let Some(slot) = final_attrs.iter_mut().find(|(a, _)| a == name) {
                                    slot.1 = v.clone();
                                } else {
                                    final_attrs.push((name.clone(), v.clone()));
                                }
                            }
                            _ => final_attrs.retain(|(a, _)| a != name),
                        }
                    }
                    self.auto += 1;
                }
                None => {
                    self.pending += 1;
                    self.items.push(XmlConflict {
                        path: path.join("/"),
                        kind: "conflict".into(),
                        why: "attributes".into(),
                        base_lines,
                        variants,
                        proposed: vec![],
                    });
                }
            }
        } else if b.map(|bb| vs.iter().any(|(_, n)| !xdrops(bb, n).is_empty())).unwrap_or(false) {
            // a mod only takes vanilla attribute content away: review, keep proposed
            let src = b.unwrap();
            let mut restored = final_attrs.clone();
            for (n, v) in &src.attrs {
                if !restored.iter().any(|(a, _)| a == n) {
                    restored.push((n.clone(), v.clone()));
                }
            }
            let base_lines = src.raw().lines().map(|l| l.to_string()).collect();
            let proposed = open_tag(src, &restored).lines().map(|l| l.to_string()).collect();
            match self.pick(2) {
                Some(1) => {
                    final_attrs = restored;
                    self.auto += 1;
                }
                Some(_) => self.auto += 1,
                None => {
                    self.pending += 1;
                    self.items.push(XmlConflict {
                        path: path.join("/"),
                        kind: "review".into(),
                        why: "removes".into(),
                        base_lines,
                        variants: vec![XmlVariant {
                            mods: vec![],
                            lines: open_tag(src, &final_attrs).lines().map(|l| l.to_string()).collect(),
                        }],
                        proposed,
                    });
                }
            }
        }
        let (kids, tail) = self.merge_children(path, b, vs)?;
        let open_src = b.or(vs.first().map(|(_, n)| *n)).unwrap().clone();
        Ok(Merged::Made { open_src, attrs: final_attrs, kids, tail })
    }

    fn fragment(&self, kids: &[XNode], lo: usize, hi: usize) -> Vec<String> {
        kids[lo.min(kids.len())..hi.min(kids.len())].iter().map(|n| n.raw()).collect()
    }

    fn merge_children(
        &mut self,
        path: &[String],
        b: Option<&XNode>,
        vs: &[(usize, &XNode)],
    ) -> Result<(Vec<(String, Box<Merged>)>, String), XmlError> {
        let bkids: &[XNode] = b.map(|x| x.children.as_slice()).unwrap_or(&[]);
        let mut containers: HashSet<String> = HashSet::new();
        if let Some(bb) = b {
            for c in &bb.children {
                if !c.children.is_empty() {
                    containers.insert(c.tag.clone());
                }
            }
        }
        for (_, n) in vs {
            for c in &n.children {
                if !c.children.is_empty() {
                    containers.insert(c.tag.clone());
                }
            }
        }
        let bids = xidents(bkids, &containers);
        let btok: Vec<String> = bids.iter().map(xtok).collect();
        let mut ident_of: HashMap<String, Ident> = HashMap::new();
        for (t, id) in btok.iter().zip(bids.iter()) {
            ident_of.insert(t.clone(), id.clone());
        }
        // per-version token streams + node/pre lookups
        let mut per: Vec<Vec<crate::script_merge::Hunk>> = vec![];
        let mut vnodes: Vec<HashMap<String, &XNode>> = vec![];
        let mut vpres: Vec<HashMap<String, String>> = vec![];
        for (_, n) in vs {
            let ids = xidents(&n.children, &containers);
            let vt: Vec<String> = ids.iter().map(xtok).collect();
            for (t, id) in vt.iter().zip(ids.iter()) {
                ident_of.entry(t.clone()).or_insert_with(|| id.clone());
            }
            per.push(crate::script_merge::line_hunks(&btok, &vt));
            let mut nm = HashMap::new();
            let mut pm = HashMap::new();
            for (t, c) in vt.iter().zip(n.children.iter()) {
                nm.insert(t.clone(), c);
            }
            for (k, t) in vt.iter().enumerate() {
                pm.insert(t.clone(), n.pres.get(k).cloned().unwrap_or_default());
            }
            vnodes.push(nm);
            vpres.push(pm);
        }
        let bnode: HashMap<String, &XNode> = btok.iter().zip(bkids.iter()).map(|(t, c)| (t.clone(), c)).collect();
        let bpre: HashMap<String, String> = {
            let pres = b.map(|x| x.pres.clone()).unwrap_or_default();
            btok.iter().enumerate().map(|(k, t)| (t.clone(), pres.get(k).cloned().unwrap_or_default())).collect()
        };
        // group changes where they meet (same engine as scripts).
        // XML rule, matching the original's merge_children: clean and soft
        // groups auto-apply; only true conflicts are asked about.
        let cls = crate::script_merge::clusters(&btok, &per);
        let mut node_of: HashMap<String, &XNode> = HashMap::new();
        for m in vnodes.iter().rev() {
            for (t, n) in m {
                node_of.insert(t.clone(), *n);
            }
        }
        for (t, n) in &bnode {
            node_of.insert(t.clone(), *n);
        }
        // resolve each group to token pieces
        let mut pieces: Vec<(usize, usize, Vec<String>)> = vec![];
        for c in &cls {
            let (lo, hi) = (c.start, c.end);
            let apply = |vi: usize| -> Vec<String> {
                crate::script_merge::apply_hunks_range(&btok, &per[vi], lo, hi)
            };
            match c.kind {
                crate::script_merge::ClusterKind::Clean => {
                    pieces.push((lo, hi, c.text.clone()));
                    self.auto += 1;
                    continue;
                }
                crate::script_merge::ClusterKind::Soft if c.intact => {
                    pieces.push((lo, hi, c.text.clone()));
                    self.auto += 1;
                    continue;
                }
                _ => {}
            }
            let mut outcomes: Vec<Vec<String>> = vec![];
            for i in &c.mods {
                let a = apply(*i);
                if !outcomes.contains(&a) {
                    outcomes.push(a);
                }
            }
            if outcomes.len() <= 1 {
                pieces.push((lo, hi, outcomes.into_iter().next().unwrap_or_default()));
                self.auto += 1;
                continue;
            }
            // a dropped keyed/container element nobody changed: auto-remove
            let kept: HashSet<String> = outcomes.iter().flatten().cloned().collect();
            let gone: Vec<String> = btok[lo.min(btok.len())..hi.min(btok.len())]
                .iter()
                .filter(|t| ident_of.get(*t).map(|i| i.kind()).unwrap_or('u') != 'u' && !kept.contains(*t))
                .cloned()
                .collect();
            if !gone.is_empty() {
                let touched = gone.iter().any(|t| {
                    vnodes.iter().any(|nodes| {
                        nodes.get(t).map(|n| xcanon(n) != bnode.get(t).map(|bb| xcanon(bb)).unwrap_or_default()).unwrap_or(false)
                    })
                });
                if !touched {
                    pieces.push((lo, hi, outcomes.into_iter().next().unwrap().clone()));
                    self.auto += 1;
                    continue;
                }
            }
            // conflict over this stretch: one variant per distinct outcome
            let base_lines = self.fragment(bkids, lo, hi);
            let variants: Vec<XmlVariant> = outcomes
                .iter()
                .map(|o| {
                    let mut mods: Vec<String> = c
                        .mods
                        .iter()
                        .filter(|vi| &apply(**vi) == o)
                        .map(|vi| self.labels[vs[*vi].0].clone())
                        .collect();
                    mods.sort();
                    mods.dedup();
                    let lines: Vec<String> = o.iter().filter_map(|t| node_of.get(t)).map(|n| n.raw()).collect();
                    XmlVariant { mods, lines }
                })
                .collect();
            let n_opts = variants.len() + 1; // + base
            let chosen: Vec<String> = match self.pick(n_opts) {
                Some(p) if p < outcomes.len() => {
                    self.auto += 1;
                    outcomes[p].clone()
                }
                Some(_) => {
                    self.auto += 1;
                    btok[lo.min(btok.len())..hi.min(btok.len())].to_vec()
                }
                None => {
                    self.pending += 1;
                    self.items.push(XmlConflict {
                        path: path.join("/"),
                        kind: "conflict".into(),
                        why: if gone.is_empty() { "entries".into() } else { "removes".into() },
                        base_lines,
                        variants,
                        proposed: outcomes[0].iter().filter_map(|t| node_of.get(t)).map(|n| n.raw()).collect(),
                    });
                    btok[lo.min(btok.len())..hi.min(btok.len())].to_vec()
                }
            };
            pieces.push((lo, hi, chosen));
        }
        pieces.sort_by_key(|p| p.0);
        let mut merged: Vec<String> = vec![];
        let mut cur = 0;
        for (lo, hi, toks) in &pieces {
            merged.extend_from_slice(&btok[cur..(*lo).min(btok.len())]);
            merged.extend(toks.iter().cloned());
            cur = (*hi).min(btok.len());
        }
        merged.extend_from_slice(&btok[cur..]);
        // render each element once, merged over every version that has it
        let mut kids: Vec<(String, Box<Merged>)> = vec![];
        let mut seen: HashSet<String> = HashSet::new();
        for t in &merged {
            if !seen.insert(t.clone()) {
                continue;
            }
            let ident = ident_of.get(t).cloned().unwrap_or(Ident::Content(String::new(), 0));
            let holders: Vec<(usize, &XNode)> = vs
                .iter()
                .enumerate()
                .filter_map(|(pos, (i, _))| vnodes.get(pos)?.get(t).map(|n| (*i, *n)))
                .collect();
            let mut sub = path.to_vec();
            sub.push(ident.label());
            let kid = self.merge_node(&sub, bnode.get(t).copied(), &holders)?;
            let base_pre = bpre.get(t).cloned();
            let mut changed_pre: Option<String> = None;
            for pm in &vpres {
                if let Some(p) = pm.get(t) {
                    if Some(p) != base_pre.as_ref() {
                        changed_pre = Some(p.clone());
                        break;
                    }
                }
            }
            kids.push((changed_pre.or(base_pre).unwrap_or_else(|| "\n".to_string()), Box::new(kid)));
        }
        let tails: Vec<&str> = vs.iter().map(|(_, n)| n.tail.as_str()).collect();
        let btail = b.map(|x| x.tail.as_str());
        let tail = tails.into_iter().find(|t| Some(*t) != btail).or(btail).unwrap_or("").to_string();
        Ok((kids, tail))
    }
}

fn xloses(b: &XNode, n: &XNode) -> bool {
    if xcanon(b) == xcanon(n) {
        return false;
    }
    if !xdrops(b, n).is_empty() {
        return true;
    }
    let mut containers = HashSet::new();
    for c in b.children.iter().chain(n.children.iter()) {
        if !c.children.is_empty() {
            containers.insert(c.tag.clone());
        }
    }
    let theirs: HashMap<String, &XNode> = xidents(&n.children, &containers)
        .into_iter()
        .zip(n.children.iter())
        .map(|(id, c)| (xtok(&id), c))
        .collect();
    for (id, c) in xidents(&b.children, &containers).into_iter().zip(b.children.iter()) {
        if id.kind() == 'u' {
            continue;
        }
        match theirs.get(&xtok(&id)) {
            Some(o) if !xloses(c, o) => {}
            _ => return true,
        }
    }
    false
}

fn xdrops(b: &XNode, n: &XNode) -> Vec<String> {
    let mut out = vec![];
    let nd: HashMap<&str, &str> = n.attrs.iter().map(|(a, v)| (a.as_str(), v.as_str())).collect();
    for (name, bv) in &b.attrs {
        match nd.get(name.as_str()) {
            None => out.push(name.clone()),
            Some(nv) if nv != bv => {
                let split = |s: &str| -> std::collections::HashSet<String> {
                    s.split(|c| c == ';' || c == ',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
                };
                let bi = split(bv);
                let ni = split(nv);
                if bi.len() > 1 && ni.is_subset(&bi) {
                    out.push(name.clone());
                }
            }
            _ => {}
        }
    }
    out
}

/// Merge entry point. `versions`: texts, top priority first.
/// `resolutions`: picked variant index per conflict, in order.
pub fn xml_merge(base_text: &str, versions: &[String], resolutions: &[usize]) -> Result<XmlMergeResult, String> {
    let labels: Vec<String> = versions.iter().enumerate().map(|(i, _)| format!("mod{i}")).collect();
    let base = if base_text.is_empty() {
        None
    } else {
        Some(parse_xml(base_text).map_err(|e| e.to_string())?)
    };
    let mut docs: Vec<XNode> = vec![];
    for v in versions {
        docs.push(parse_xml(v).map_err(|e| e.to_string())?.1);
    }
    if docs.iter().map(xcanon).collect::<HashSet<_>>().len() == 1 {
        return Ok(XmlMergeResult {
            status: "same".into(),
            merged: versions[0].clone(),
            conflicts: vec![],
            needs_resolution: false,
            reason: String::new(),
            auto: 0,
        });
    }
    let tags: HashSet<&str> = docs.iter().map(|d| d.tag.as_str()).collect();
    if tags.len() > 1 {
        return Err("the copies don't share a root element".into());
    }
    if let Some(bb) = base.as_ref() {
        if bb.1.tag != docs[0].tag {
            return Err("the copies don't share a root element".into());
        }
    }
    let mut m = Merger { labels, resolutions, res_idx: 0, items: vec![], pending: 0, auto: 0 };
    let holders: Vec<(usize, &XNode)> = docs.iter().enumerate().collect();
    let res = m.merge_node(&[format!("root:{}", docs[0].tag)], base.as_ref().map(|b| &b.1), &holders).map_err(|e| e.to_string())?;
    let (prolog, epilog) = base.as_ref().map(|b| (b.0.clone(), b.2.clone())).unwrap_or_default();
    let text = format!("{prolog}{}{epilog}", render(&res));
    // safety net: must parse
    let reparsed = parse_xml(&text).map_err(|e| format!("merged XML doesn't parse ({e}) — left unmerged to be safe"))?;
    let merged_canon = xcanon(&reparsed.1);
    let mut status = "merged".to_string();
    for d in &docs {
        if xcanon(d) == merged_canon {
            status = "superset".to_string();
            break;
        }
    }
    if m.pending > 0 {
        status = "pending".to_string();
    }
    Ok(XmlMergeResult {
        status,
        merged: text,
        conflicts: m.items,
        needs_resolution: m.pending > 0,
        reason: String::new(),
        auto: m.auto,
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
        assert_eq!(r.status, "same");
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
    #[test]
    fn disjoint_adds_merge_clean() {
        let b = "<Vars><Var id=\"A\" value=\"0\"/></Vars>";
        let v1 = "<Vars><Var id=\"A\" value=\"0\"/><Var id=\"B\" value=\"1\"/></Vars>";
        let v2 = "<Vars><Var id=\"A\" value=\"0\"/><Var id=\"C\" value=\"2\"/></Vars>";
        let r = xml_merge(b, &[v1.to_string(), v2.to_string()], &[]).unwrap();
        assert!(!r.needs_resolution, "disjoint adds should auto-merge: {:?}", r.conflicts);
        assert!(r.merged.contains("id=\"B\"") && r.merged.contains("id=\"C\""));
    }
    #[test]
    fn bad_xml_fails() {
        assert!(xml_merge("<a>", &["<a/>".to_string()], &[]).is_err());
    }
    #[test]
    fn comments_and_prolog_survive() {
        let b = "<?xml version=\"1.0\"?><Vars><!-- hi --><Var id=\"A\" value=\"0\"/></Vars>";
        let r = xml_merge(b, &[b.to_string()], &[]).unwrap();
        assert!(r.merged.contains("<?xml"));
        assert!(r.merged.contains("<!-- hi -->"));
    }
}
