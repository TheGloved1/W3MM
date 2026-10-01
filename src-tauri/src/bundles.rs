//! Bundle I/O: Witcher `.bundle` read + write.
//! Ports `bundle_toc/bundle_entries/bundle_read/_bundle_unpack/_lz4_block/
//! _snappy_raw/_doboz/build_bundle/build_store/_vlq/_fnv1a64/_lz4_stored`
//! (`w3modmanager.py:1295-1649`).
//!
//! Layout (verified against wcc_lite): `POTATO70` magic, 0x20 header with the
//! table size at 0x10, then 0x140-byte entries: 0x100 latin-1 path
//! (backslash, lower-cased on read), usize/zsize/doff at +0x114, CRC/comp at
//! +0x138. Packing: 0 = stored, 1 = zlib, 2 = snappy, 3 = doboz, 4/5 = LZ4.
//! Mod bundles are written unpacked (compression 0), like the game's own XML
//! bundles — no compressor needed.

use std::path::Path;

pub const BUNDLE_MAGIC: &[u8; 8] = b"POTATO70";
pub const TOC_ENTRY: usize = 0x140;

#[derive(Debug, Clone)]
pub struct BundleEntry {
    pub path: String,
    pub offset: u64,
    pub size: u64,
    pub zsize: u64,
    pub compression: u8,
    pub crc: u32,
}

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_toc(data: &[u8]) -> Option<Vec<BundleEntry>> {
    let head = data.get(..0x20)?;
    if &head[..8] != BUNDLE_MAGIC {
        return None;
    }
    let toc = u32le(data, 0x10)? as usize;
    let toc = toc.min(data.len().saturating_sub(0x20));
    let body = data.get(0x20..0x20 + toc)?;
    let mut out = Vec::new();
    let mut off = 0;
    while off + TOC_ENTRY <= body.len() {
        let raw = body.get(off..off + 0x100)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(0x100);
        if end > 0 {
            let name = String::from_utf8_lossy(&raw[..end]).replace('\\', "/").to_lowercase();
            let usize_ = u32le(body, off + 0x114)? as u64;
            let zsize = u32le(body, off + 0x118)? as u64;
            let doff = u32le(body, off + 0x11C)? as u64;
            let crc = u32le(body, off + 0x138)?;
            let comp = *body.get(off + 0x13C)?;
            out.push(BundleEntry { path: name, offset: doff, size: usize_, zsize, compression: comp, crc });
        }
        off += TOC_ENTRY;
    }
    Some(out)
}

pub fn bundle_toc(path: &Path) -> Vec<BundleEntry> {
    std::fs::read(path).ok().and_then(|d| read_toc(&d)).unwrap_or_default()
}

pub fn bundle_entries(path: &Path) -> Result<Vec<BundleEntry>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    read_toc(&data).ok_or_else(|| "not a Witcher bundle".to_string())
}

/// Raw LZ4 block decompressor (python `_lz4_block`).
pub fn lz4_block(src: &[u8], size: usize) -> Result<Vec<u8>, String> {
    let mut dst: Vec<u8> = Vec::with_capacity(size);
    let mut i = 0;
    let n = src.len();
    while i < n {
        let token = src[i];
        i += 1;
        let mut lit = (token >> 4) as usize;
        if lit == 15 {
            loop {
                let b = *src.get(i).ok_or("bad LZ4 data")?;
                i += 1;
                lit += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        let end = i + lit;
        dst.extend_from_slice(src.get(i..end).ok_or("bad LZ4 data")?);
        i = end;
        if i >= n {
            break;
        }
        let off = (src[i] as usize) | ((src[i + 1] as usize) << 8);
        i += 2;
        let mut ml = (token & 15) as usize;
        if ml == 15 {
            loop {
                let b = *src.get(i).ok_or("bad LZ4 data")?;
                i += 1;
                ml += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        ml += 4;
        if off == 0 || off > dst.len() {
            return Err("bad LZ4 data".into());
        }
        for _ in 0..ml {
            let idx = dst.len() - off;
            dst.push(dst[idx]);
        }
    }
    if dst.len() != size {
        return Err("bad LZ4 data".into());
    }
    Ok(dst)
}

/// Raw Snappy decompressor (python `_snappy_raw`).
pub fn snappy_raw(src: &[u8], size: usize) -> Result<Vec<u8>, String> {
    let mut i = 0;
    let mut shift = 0;
    let mut total: usize = 0;
    loop {
        let b = *src.get(i).ok_or("bad snappy data")?;
        i += 1;
        total |= ((b & 0x7F) as usize) << shift;
        shift += 7;
        if b & 0x80 == 0 {
            break;
        }
    }
    let mut dst: Vec<u8> = Vec::new();
    let n = src.len();
    while i < n {
        let tag = src[i];
        i += 1;
        match tag & 3 {
            0 => {
                let mut ln = (tag >> 2) as usize;
                if ln >= 60 {
                    let extra = ln - 59;
                    let mut v = 0usize;
                    for k in 0..extra {
                        v |= (*src.get(i + k).ok_or("bad snappy data")? as usize) << (8 * k);
                    }
                    i += extra;
                    ln = v;
                }
                ln += 1;
                dst.extend_from_slice(src.get(i..i + ln).ok_or("bad snappy data")?);
                i += ln;
            }
            1 => {
                let ln = (((tag >> 2) & 7) as usize) + 4;
                let off = ((((tag >> 5) as usize) << 8) | (*src.get(i).ok_or("bad snappy data")? as usize));
                i += 1;
                copy_from(&mut dst, off, ln)?;
            }
            2 => {
                let ln = ((tag >> 2) as usize) + 1;
                let lo = *src.get(i).ok_or("bad snappy data")? as usize;
                let hi = *src.get(i + 1).ok_or("bad snappy data")? as usize;
                i += 2;
                copy_from(&mut dst, lo | (hi << 8), ln)?;
            }
            _ => {
                let ln = ((tag >> 2) as usize) + 1;
                let mut off = 0usize;
                for k in 0..4 {
                    off |= (*src.get(i + k).ok_or("bad snappy data")? as usize) << (8 * k);
                }
                i += 4;
                copy_from(&mut dst, off, ln)?;
            }
        }
    }
    if dst.len() != size || total != size {
        return Err("bad snappy data".into());
    }
    Ok(dst)
}

fn copy_from(dst: &mut Vec<u8>, off: usize, ln: usize) -> Result<(), String> {
    if off == 0 || off > dst.len() {
        return Err("bad snappy data".into());
    }
    for _ in 0..ln {
        let b = dst[dst.len() - off];
        dst.push(b);
    }
    Ok(())
}

/// Doboz decoder (Attila T. Afra), port of python `_doboz`.
pub fn doboz_decode(src: &[u8], size: usize) -> Result<Vec<u8>, String> {
    const LUT: [(u32, u32, u32, u32, usize); 8] = [
        (0xFF, 2, 0, 0, 1),
        (0xFFFF, 2, 0, 0, 2),
        (0xFFFF, 6, 15, 2, 2),
        (0xFFFFFF, 8, 31, 3, 3),
        (0xFF, 2, 0, 0, 1),
        (0xFFFF, 2, 0, 0, 2),
        (0xFFFF, 6, 15, 2, 2),
        (0xFFFFFFFF, 11, 255, 3, 4),
    ];
    let err = || "bad doboz data".to_string();
    let attrs = *src.first().ok_or_else(err)?;
    let width = (((attrs >> 3) & 7) + 1) as usize;
    if attrs & 7 != 0 || ![1, 2, 4, 8].contains(&width) {
        return Err(err());
    }
    let le = |b: &[u8]| {
        let mut v = 0u64;
        for (k, byte) in b.iter().enumerate() {
            v |= (*byte as u64) << (8 * k);
        }
        v as usize
    };
    let usize_ = le(src.get(1..1 + width).ok_or_else(err)?);
    let csize = le(src.get(1 + width..1 + 2 * width).ok_or_else(err)?);
    let mut i = 1 + 2 * width;
    if attrs & 128 != 0 {
        let out = src.get(i..i + usize_).ok_or_else(err)?.to_vec();
        if out.len() != size {
            return Err(err());
        }
        return Ok(out);
    }
    let mut buf = src.get(..csize.min(src.len())).ok_or_else(err)?.to_vec();
    buf.extend_from_slice(&[0u8; 8]);
    let src = buf;
    let mut dst: Vec<u8> = Vec::with_capacity(usize_);
    let mut control = 1u32;
    while dst.len() < usize_ {
        if control == 1 {
            let b = src.get(i..i + 4).ok_or_else(err)?;
            control = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            i += 4;
        }
        if control & 1 == 0 {
            dst.push(*src.get(i).ok_or_else(err)?);
            i += 1;
        } else {
            let b = src.get(i..i + 4).ok_or_else(err)?;
            let word = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            let (mask, oshift, lmask, lshift, n) = LUT[(word & 7) as usize];
            let offset = ((word & mask) >> oshift) as usize;
            let length = (((word >> lshift) & lmask) as usize) + 3;
            i += n;
            if offset == 0 || offset > dst.len() {
                return Err(err());
            }
            let start = dst.len() - offset;
            if offset >= length {
                dst.extend_from_slice(&dst[start..start + length].to_vec());
            } else {
                for k in 0..length {
                    let v = dst[start + k];
                    dst.push(v);
                }
            }
        }
        control >>= 1;
    }
    if dst.len() != size || usize_ != size {
        return Err(err());
    }
    Ok(dst)
}

fn unpack_entry(data: &[u8], e: &BundleEntry) -> Result<Vec<u8>, String> {
    let s = e.offset as usize;
    let z = e.zsize as usize;
    let raw = data.get(s..s + z).ok_or("bundle truncated")?;
    let out = match e.compression {
        0 => raw[..(e.size as usize).min(raw.len())].to_vec(),
        1 => {
            use std::io::Read;
            let mut dec = flate2::read::ZlibDecoder::new(raw);
            let mut v = Vec::with_capacity(e.size as usize);
            dec.read_to_end(&mut v).map_err(|e| e.to_string())?;
            v
        }
        2 => snappy_raw(raw, e.size as usize)?,
        3 => doboz_decode(raw, e.size as usize)?,
        4 | 5 => lz4_block(raw, e.size as usize)?,
        c => return Err(format!("unknown packing ({c})")),
    };
    if out.len() != e.size as usize || (e.crc != 0 && crc32fast::hash(&out) != e.crc) {
        return Err("it doesn't unpack to what the bundle says".into());
    }
    Ok(out)
}

pub fn bundle_read(path: &Path, want: &str) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let toc = read_toc(&data).ok_or("not a Witcher bundle")?;
    let e = toc.iter().find(|e| e.path.eq_ignore_ascii_case(want)).ok_or("not in bundle")?;
    unpack_entry(&data, e)
}

/// VLQ for metadata.store (python `_vlq`).
pub fn vlq(mut v: i64) -> Vec<u8> {
    let neg = v < 0;
    let mut m = v.unsigned_abs();
    let mut b = ((m & 0x3F) as u8) | (0x80 * neg as u8) | (0x40 * ((m >> 6 != 0) as u8));
    let mut out = vec![b];
    m >>= 6;
    while m != 0 {
        b = (m & 0x7F) as u8;
        m >>= 7;
        out.push(b | (0x80 * (m != 0) as u8));
    }
    let _ = v;
    out
}

/// FNV-1a 64 for the store hash table (python `_fnv1a64`).
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut h = 0xCBF29CE484222325u64;
    for &c in data {
        h = h ^ (c as u64);
        h = h.wrapping_mul(0x100000001B3);
    }
    h
}

/// A valid LZ4 block holding data as literals (python `_lz4_stored`).
pub fn lz4_stored(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut head = if n < 15 {
        vec![(n << 4) as u8]
    } else {
        let rest = n - 15;
        let mut h = vec![0xF0u8];
        h.extend(std::iter::repeat(0xFFu8).take(rest / 255));
        h.push((rest % 255) as u8);
        h
    };
    head.extend_from_slice(data);
    head
}

fn align(x: usize, to: usize) -> usize {
    (x + to - 1) / to * to
}

/// Build a bundle + store rows (python `build_bundle`). Files are stored
/// unpacked (compression 0), like the game's own XML bundles.
pub fn build_bundle(files: &[(String, Vec<u8>)], lz4: bool) -> Result<(Vec<u8>, Vec<(Vec<u8>, u32, u32, u32, u32)>), String> {
    let mut sorted: Vec<(Vec<u8>, Vec<u8>)> = files
        .iter()
        .map(|(p, d)| (p.replace('/', "\\").to_lowercase().as_bytes().to_vec(), d.clone()))
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, _) in &sorted {
        if name.len() >= 0x100 {
            return Err(format!("path too long for a bundle: {}", String::from_utf8_lossy(name)));
        }
    }
    let tsz = sorted.len() * TOC_ENTRY;
    let mut pos = align(0x20 + tsz, 0x1000);
    let mut rows = vec![];
    let mut data: Vec<u8> = vec![];
    for (name, blob) in &sorted {
        let (packed, comp) = if lz4 { (lz4_stored(blob), 5u32) } else { (blob.clone(), 0u32) };
        rows.push((name.clone(), blob.len() as u32, packed.len() as u32, pos as u32, comp));
        data.extend(std::iter::repeat(0u8).take(pos - (0x20 + tsz) - data.len()));
        data.extend_from_slice(&packed);
        pos = align(pos + packed.len(), 0x1000);
    }
    let end = 0x20 + tsz + data.len();
    let size = align(end, 0x1000) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(BUNDLE_MAGIC);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(tsz as u32).to_le_bytes());
    out.extend_from_slice(&0x00010003u32.to_le_bytes());
    out.push(0x00);
    out.extend(std::iter::repeat(0x13u8).take(7));
    for ((name, usize_, zsize, doff, comp), (_, blob)) in rows.iter().zip(sorted.iter()) {
        let mut entry = vec![0u8; TOC_ENTRY];
        entry[..name.len()].copy_from_slice(name);
        entry[0x114..0x118].copy_from_slice(&usize_.to_le_bytes());
        entry[0x118..0x11C].copy_from_slice(&zsize.to_le_bytes());
        entry[0x11C..0x120].copy_from_slice(&doff.to_le_bytes());
        entry[0x138..0x13C].copy_from_slice(&crc32fast::hash(blob).to_le_bytes());
        entry[0x13C..0x140].copy_from_slice(&comp.to_le_bytes());
        out.extend_from_slice(&entry);
    }
    out.extend_from_slice(&data);
    Ok((out, rows))
}

/// metadata.store for one content folder (python `build_store`).
pub fn build_store(bundles: &[(String, (u32, u32, Vec<(Vec<u8>, u32, u32, u32, u32)>))]) -> Vec<u8> {
    let mut strings = vec![0u8];
    let mut add = |s: &[u8], strings: &mut Vec<u8>| {
        let o = strings.len() as u32;
        strings.extend_from_slice(s);
        strings.push(0);
        o
    };
    struct B {
        name: u32,
        num: u32,
        size: u32,
        off: u32,
        items: Vec<(Vec<u8>, u32, u32, u32, u32)>,
    }
    let mut binfo: Vec<B> = vec![];
    for (bname, (size, tsz, items)) in bundles {
        let name = add(bname.as_bytes(), &mut strings);
        binfo.push(B { name, num: items.len() as u32, size: size - (0x20 + tsz), off: 0x20 + tsz, items: items.clone() });
    }
    // files: [name, ?, zsize, usize, first_ent, comp, 0, 0]
    let mut files: Vec<[u32; 8]> = vec![];
    let mut index: std::collections::HashMap<Vec<u8>, usize> = Default::default();
    for b in &binfo {
        for (name, usize_, zsize, _doff, comp) in &b.items {
            if !index.contains_key(name) {
                index.insert(name.clone(), files.len());
                files.push([add(name, &mut strings), 0, *zsize, *usize_, 0, *comp, 0, 0]);
            }
        }
    }
    // ents: [file+1, bundle+1, doff, zsize, next]
    let mut ents: Vec<[u32; 5]> = vec![];
    let mut last: std::collections::HashMap<usize, usize> = Default::default();
    let mut first: Vec<u32> = vec![0; binfo.len()];
    for (bi, b) in binfo.iter().enumerate() {
        first[bi] = ents.len() as u32 + 1;
        for (name, _usize_, zsize, doff, _comp) in &b.items {
            let fi = index[name];
            ents.push([(fi + 1) as u32, (bi + 1) as u32, *doff, *zsize, 0]);
            if let Some(prev) = last.get(&fi) {
                ents[prev - 1][4] = ents.len() as u32;
            } else {
                files[fi][4] = ents.len() as u32;
            }
            last.insert(fi, ents.len());
        }
    }
    let mut paths: Vec<Vec<u8>> = index.keys().cloned().collect();
    paths.sort_by_key(|n| index[n]);
    let root = add(b"", &mut strings);
    let mut dirs: Vec<(u32, i32, Option<Vec<u8>>)> = vec![(root, 0, None)];
    let mut dirid: std::collections::HashMap<Vec<Vec<u8>>, usize> = Default::default();
    dirid.insert(vec![], 0);
    for name in &paths {
        let parts: Vec<&[u8]> = name.split(|&b| b == b'\\').collect();
        for k in 1..parts.len() {
            let key: Vec<Vec<u8>> = parts[..k].iter().map(|p| p.to_vec()).collect();
            if !dirid.contains_key(&key) {
                let parent = dirid[&key[..k - 1]];
                dirid.insert(key.clone(), dirs.len());
                dirs.push((0, parent as i32, Some(parts[k - 1].to_vec())));
            }
        }
    }
    for d in dirs.iter_mut().skip(1) {
        let nm = d.2.clone().unwrap();
        d.0 = add(&nm, &mut strings);
    }
    let finit: Vec<(i32, i32, u32)> = paths
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let parts: Vec<&[u8]> = name.split(|&b| b == b'\\').collect();
            let key: Vec<Vec<u8>> = parts[..parts.len() - 1].iter().map(|p| p.to_vec()).collect();
            ((i + 1) as i32, dirid[&key] as i32, add(parts[parts.len() - 1], &mut strings))
        })
        .collect();
    let mut hashes: Vec<(u64, u32)> = paths.iter().enumerate().map(|(i, n)| (fnv1a64(n), (i + 1) as u32)).collect();
    hashes.sort();
    let mut out = vec![0x03u8, b'V', b'T', b'M'];
    out.extend_from_slice(&6i32.to_le_bytes());
    out.extend_from_slice(&(files.iter().map(|f| f[2]).max().unwrap_or(0) as i32).to_le_bytes());
    out.extend_from_slice(&(files.iter().map(|f| f[3]).max().unwrap_or(0) as i32).to_le_bytes());
    out.extend_from_slice(&vlq(strings.len() as i64));
    out.extend_from_slice(&strings);
    out.extend_from_slice(&vlq((files.len() + 1) as i64));
    out.extend_from_slice(&[0u8; 32]);
    for f in &files {
        for v in f {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&vlq((ents.len() + 1) as i64));
    out.extend_from_slice(&[0u8; 20]);
    for e in &ents {
        for v in e {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&vlq((binfo.len() + 1) as i64));
    out.extend_from_slice(&[0u8; 24]);
    for (bi, b) in binfo.iter().enumerate() {
        for v in [b.name, first[bi], b.num, b.size, b.off, 0u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.push(0x80);
    out.extend_from_slice(&vlq(dirs.len() as i64));
    for d in &dirs {
        out.extend_from_slice(&d.0.to_le_bytes());
        out.extend_from_slice(&d.1.to_le_bytes());
    }
    out.extend_from_slice(&vlq(finit.len() as i64));
    for (a, b, c) in &finit {
        out.extend_from_slice(&a.to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&vlq(hashes.len() as i64));
    for (h, i) in &hashes {
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lz4_literal_only() {
        let src = [0x50u8, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(lz4_block(&src, 5).unwrap(), b"hello");
    }
    #[test]
    fn snappy_literal_only() {
        let src = [0x05u8, 0x10u8, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(snappy_raw(&src, 5).unwrap(), b"hello");
    }
    #[test]
    fn doboz_stored() {
        let src = [0x80u8, 0x03, 0x06, b'a', b'b', b'c'];
        assert_eq!(doboz_decode(&src, 3).unwrap(), b"abc");
    }
    #[test]
    fn bundle_roundtrip() {
        let files = vec![
            ("Xml/Game/stuff.xml".to_string(), b"<a/>".to_vec()),
            ("Scripts/a.ws".to_string(), "function f() {}".as_bytes().to_vec()),
        ];
        let (bytes, rows) = build_bundle(&files, false).unwrap();
        assert_eq!(&bytes[..8], BUNDLE_MAGIC);
        let toc = read_toc(&bytes).expect("read back");
        assert_eq!(toc.len(), 2);
        // stored paths are lower-case backslash on read
        assert!(toc.iter().any(|e| e.path == "xml\\game\\stuff.xml".replace('\\', "/")));
        // write to temp file and read entries through the file API
        let p = std::env::temp_dir().join("w3lmn-bundle-test.bundle");
        std::fs::write(&p, &bytes).unwrap();
        let back = bundle_read(&p, "xml/game/stuff.xml").unwrap();
        assert_eq!(back, b"<a/>");
        let lz = build_bundle(&files, true).unwrap().0;
        let t2 = read_toc(&lz).unwrap();
        assert!(t2.iter().all(|e| e.compression == 5));
        let back2 = {
            let t = read_toc(&lz).unwrap();
            let e = t.iter().find(|e| e.path == "scripts/a.ws").unwrap();
            unpack_entry(&lz, e).unwrap()
        };
        assert_eq!(back2, "function f() {}".as_bytes());
        let _ = rows;
        let store = build_store(&[("mod.bundle".to_string(), (bytes.len() as u32, 0, vec![]))]);
        assert!(store.starts_with(&[0x03, b'V', b'T', b'M']));
    }
    #[test]
    fn vlq_vectors() {
        assert_eq!(vlq(0), vec![0]);
        assert_eq!(fnv1a64(b""), 0xCBF29CE484222325);
    }
}
