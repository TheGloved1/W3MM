//! Bundle I/O: Witcher `.bundle` table-of-contents + read.
//! Ports `bundle_toc/bundle_entries/bundle_read/_bundle_unpack/_lz4_block/
//! _snappy_raw/_doboz/build_bundle/build_store` (`w3modmanager.py:1305-1649`).

use std::path::Path;

#[derive(Debug, Clone)]
pub struct BundleEntry {
    pub path: String,
    pub offset: u64,
    pub size: u64,
    pub zsize: u64,
    pub compression: u8,
}

pub fn bundle_entries(path: &Path) -> Result<Vec<BundleEntry>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    parse_toc(&data).ok_or_else(|| "not a Witcher bundle".to_string())
}

fn u32le(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn u64le(b: &[u8], o: usize) -> Option<u64> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}

fn parse_toc(data: &[u8]) -> Option<Vec<BundleEntry>> {
    if data.len() < 16 {
        return None;
    }
    let n = data.len();
    let count = u32le(data, n - 8)? as usize;
    let table = u64le(data, n - 16)? as usize;
    if count == 0 || count > 200_000 || table >= n {
        return None;
    }
    let mut out = Vec::new();
    let mut o = table;
    for _ in 0..count {
        let nlen = u32le(data, o)? as usize;
        o += 4;
        let name = std::str::from_utf8(data.get(o..o + nlen)?).ok()?.to_string();
        o += nlen;
        let offset = u64le(data, o)?;
        o += 8;
        let size = u64le(data, o)?;
        o += 8;
        let zsize = u64le(data, o)?;
        o += 8;
        let comp = *data.get(o)?;
        o += 1;
        out.push(BundleEntry { path: name, offset, size, zsize, compression: comp });
    }
    Some(out)
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
        let start = dst.len() - off;
        // overlapping copy
        for k in 0..ml {
            let b = dst[start + k % (dst.len() - start).max(1)];
            // careful: dst grows; index into the sliding window
            let idx = dst.len() - off;
            dst.push(dst[idx]);
            let _ = b;
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
    for k in 0..ln {
        let b = dst[dst.len() - off];
        dst.push(b);
        let _ = k;
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

pub fn bundle_read(path: &Path, want: &str) -> Result<Vec<u8>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let toc = parse_toc(&data).ok_or("not a Witcher bundle")?;
    let e = toc.iter().find(|e| e.path.eq_ignore_ascii_case(want)).ok_or("not in bundle")?;
    let s = e.offset as usize;
    let z = e.zsize as usize;
    let blob = data.get(s..s + z).ok_or("bundle truncated")?;
    match e.compression {
        0 => Ok(blob.to_vec()),
        // compression ids follow the game's bundle writer: 1 = lz4, 2 = snappy, 3 = doboz
        1 => lz4_block(blob, e.size as usize),
        2 => snappy_raw(blob, e.size as usize),
        3 => doboz_decode(blob, e.size as usize),
        _ => lz4_block(blob, e.size as usize)
            .or_else(|_| snappy_raw(blob, e.size as usize))
            .or_else(|_| doboz_decode(blob, e.size as usize)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lz4_literal_only() {
        // token 0x50 = 5 literals "hello", then end.
        let src = [0x50u8, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(lz4_block(&src, 5).unwrap(), b"hello");
    }
    #[test]
    fn snappy_literal_only() {
        // total=5, tag literal len 5, "hello"
        let src = [0x05u8, 0x10u8, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(snappy_raw(&src, 5).unwrap(), b"hello");
    }
    #[test]
    fn doboz_stored() {
        // attrs: width=1 (bits3-5 = 0), stored bit set. usize=3, csize=6, "abc".
        let src = [0x80u8, 0x03, 0x06, b'a', b'b', b'c'];
        assert_eq!(doboz_decode(&src, 3).unwrap(), b"abc");
    }
}
