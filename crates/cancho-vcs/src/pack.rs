//! Pack files (`docs/vcs-cli.md` section 3 and R4): many objects in one
//! file, each zlib-compressed and most stored as a delta against an
//! earlier one, with an index that names every object's place.
//!
//! `objects/pack/pack-<h>.pack`:
//! `"CVPK"`, version `1` (u32 LE), the entry count (u32 LE), the entries,
//! then BLAKE3 of everything before it (32 bytes); `<h>` is that hash in
//! hex. An entry is a type byte (`0` whole, `1` delta), for a delta the
//! base's entry number (u32 LE, always an earlier entry), the object's size
//! and the payload's size (varints), and the payload as one zlib stream.
//!
//! `objects/pack/pack-<h>.idx`:
//! `"CVIX"`, version `1`, the count, then per entry in pack order its kind
//! (u8), its id (32 bytes), its offset (u64 LE), its length (u32 LE) and
//! its size (u64 LE); then BLAKE3 of everything before it. The index is
//! written after the pack and its rename is what makes a pack exist.
//!
//! A read is a hash-map lookup, then at most [`MAX_DEPTH`] base reads;
//! every object read is re-hashed against its id, so a damaged pack or
//! index is `object-corrupt`, never a wrong object.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::error::{Refusal, Result};
use crate::object::{Kind, object_id};

/// The longest delta chain a pack is written with.
pub(crate) const MAX_DEPTH: usize = 50;
/// A chain longer than this when reading is a corrupt pack.
const READ_DEPTH: usize = 64;
const IDX_ENTRY: usize = 1 + 32 + 8 + 4 + 8;
/// Decoded delta bases kept per pack (bytes), so a chain is not re-read.
const CACHE_BYTES: usize = 64 << 20;
/// No object is larger than this (trees can exceed `MAX_BLOB`).
pub(crate) const MAX_OBJECT: usize = 1 << 30;

pub(crate) fn kind_byte(k: Kind) -> u8 {
    match k {
        Kind::Blob => 0,
        Kind::Tree => 1,
        Kind::Change => 2,
    }
}

fn kind_of(b: u8) -> Option<Kind> {
    Kind::ALL.into_iter().find(|&k| kind_byte(k) == b)
}

fn corrupt(pack: &str, why: impl std::fmt::Display) -> Refusal {
    Refusal::new("object-corrupt", format!("pack {pack}: {why}"))
}

#[derive(Clone, Debug)]
pub(crate) struct IdxEntry {
    pub kind: Kind,
    pub id: [u8; 32],
    pub offset: u64,
    pub len: u32,
    pub size: u64,
}

/// Decoded delta bases by entry number, and their total bytes.
type BaseCache = (HashMap<u32, Arc<Vec<u8>>>, usize);

pub(crate) struct Pack {
    pub name: String,
    pub entries: Vec<IdxEntry>,
    by_id: HashMap<(u8, [u8; 32]), u32>,
    file: Mutex<File>,
    cache: Mutex<BaseCache>,
}

impl std::fmt::Debug for Pack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pack({}, {} objects)", self.name, self.entries.len())
    }
}

pub(crate) fn pack_dir(vcs: &Path) -> PathBuf {
    vcs.join("objects").join("pack")
}

/// The packs that exist: every `pack-<h>.idx` (a pack without its index is
/// an interrupted write and does not exist yet), sorted by name.
pub(crate) fn pack_names(vcs: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(pack_dir(vcs))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().into_owned();
                    let h = n.strip_prefix("pack-")?.strip_suffix(".idx")?;
                    (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
                        .then(|| h.to_owned())
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

pub(crate) fn load_all(vcs: &Path) -> Result<Vec<Pack>> {
    pack_names(vcs).iter().map(|n| Pack::load(vcs, n)).collect()
}

fn u32_at(b: &[u8], p: usize) -> u32 {
    let mut a = [0u8; 4];
    a.copy_from_slice(&b[p..p + 4]);
    u32::from_le_bytes(a)
}

fn u64_at(b: &[u8], p: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[p..p + 8]);
    u64::from_le_bytes(a)
}

pub(crate) fn varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn read_varint(d: &[u8], p: &mut usize) -> Option<u64> {
    let mut n = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *d.get(*p)?;
        *p += 1;
        n |= u64::from(b & 0x7f).checked_shl(shift)?;
        if b & 0x80 == 0 {
            return Some(n);
        }
    }
    None
}

impl Pack {
    pub(crate) fn load(vcs: &Path, name: &str) -> Result<Pack> {
        let dir = pack_dir(vcs);
        let idx_path = dir.join(format!("pack-{name}.idx"));
        let idx = fs::read(&idx_path)
            .map_err(|e| Refusal::io(format!("cannot read {}", idx_path.display()), &e))?;
        let bad = |why: &str| corrupt(name, why);
        if idx.len() < 12 + 32 || &idx[..4] != b"CVIX" || u32_at(&idx, 4) != 1 {
            return Err(bad("index header"));
        }
        let count = u32_at(&idx, 8) as usize;
        if idx.len() != 12 + count * IDX_ENTRY + 32 {
            return Err(bad("index length"));
        }
        let body = idx.len() - 32;
        if blake3::hash(&idx[..body]).as_bytes() != &idx[body..] {
            return Err(bad("index checksum"));
        }
        let pack_path = dir.join(format!("pack-{name}.pack"));
        let mut file = File::open(&pack_path)
            .map_err(|e| Refusal::io(format!("cannot open {}", pack_path.display()), &e))?;
        let pack_len = file
            .metadata()
            .map_err(|e| Refusal::io(format!("cannot read {}", pack_path.display()), &e))?
            .len();
        let mut head = [0u8; 12];
        file.read_exact(&mut head).map_err(|_| bad("pack header"))?;
        if &head[..4] != b"CVPK" || u32_at(&head, 4) != 1 || u32_at(&head, 8) as usize != count {
            return Err(bad("pack header"));
        }
        let mut entries = Vec::with_capacity(count);
        let mut by_id = HashMap::with_capacity(count);
        let mut expect = 12u64;
        for k in 0..count {
            let p = 12 + k * IDX_ENTRY;
            let kind = kind_of(idx[p]).ok_or_else(|| bad("index kind"))?;
            let mut id = [0u8; 32];
            id.copy_from_slice(&idx[p + 1..p + 33]);
            let e = IdxEntry {
                kind,
                id,
                offset: u64_at(&idx, p + 33),
                len: u32_at(&idx, p + 41),
                size: u64_at(&idx, p + 45),
            };
            // Entries are contiguous, in order, and end where the trailer starts.
            if e.offset != expect || e.size > MAX_OBJECT as u64 {
                return Err(bad("index offsets"));
            }
            expect = e.offset + u64::from(e.len);
            by_id.insert((idx[p], id), k as u32);
            entries.push(e);
        }
        if expect + 32 != pack_len {
            return Err(bad("pack length"));
        }
        Ok(Pack {
            name: name.to_owned(),
            entries,
            by_id,
            file: Mutex::new(file),
            cache: Mutex::new((HashMap::new(), 0)),
        })
    }

    pub(crate) fn find(&self, kind: Kind, id: &[u8; 32]) -> Option<u32> {
        self.by_id.get(&(kind_byte(kind), *id)).copied()
    }

    fn raw(&self, k: u32) -> Result<Vec<u8>> {
        let e = &self.entries[k as usize];
        let mut buf = vec![0u8; e.len as usize];
        let mut f = self.file.lock().map_err(|_| corrupt(&self.name, "lock poisoned"))?;
        f.seek(SeekFrom::Start(e.offset))
            .and_then(|_| f.read_exact(&mut buf))
            .map_err(|e| Refusal::io(format!("cannot read pack {}", self.name), &e))?;
        Ok(buf)
    }

    /// The bytes of entry `k`, re-hashed against its id.
    pub(crate) fn read(&self, k: u32) -> Result<Arc<Vec<u8>>> {
        self.read_at(k, 0)
    }

    fn read_at(&self, k: u32, depth: usize) -> Result<Arc<Vec<u8>>> {
        if let Ok(c) = self.cache.lock()
            && let Some(v) = c.0.get(&k)
        {
            return Ok(v.clone());
        }
        if depth > READ_DEPTH {
            return Err(corrupt(&self.name, "delta chain too long"));
        }
        let e = self.entries.get(k as usize).ok_or_else(|| corrupt(&self.name, "entry"))?;
        let raw = self.raw(k)?;
        let bad = |why: &str| corrupt(&self.name, format!("entry {k}: {why}"));
        let mut p = 1;
        let base = match raw.first() {
            Some(0) => None,
            Some(1) => {
                let b = raw.get(1..5).ok_or_else(|| bad("truncated"))?;
                p = 5;
                let b = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                if b >= k {
                    return Err(bad("base is not an earlier entry"));
                }
                Some(b)
            }
            _ => return Err(bad("entry type")),
        };
        let size = read_varint(&raw, &mut p).ok_or_else(|| bad("truncated"))?;
        let plen = read_varint(&raw, &mut p).ok_or_else(|| bad("truncated"))?;
        if size != e.size || plen > MAX_OBJECT as u64 {
            return Err(bad("size"));
        }
        let (payload, used) =
            crate::inflate::zlib_decode(&raw[p..], plen as usize).map_err(|x| bad(x.0))?;
        if payload.len() as u64 != plen || p + used != raw.len() {
            return Err(bad("payload length"));
        }
        let bytes = match base {
            None => payload,
            Some(b) => {
                let base = self.read_at(b, depth + 1)?;
                crate::delta::apply(&base, &payload, size as usize).map_err(bad)?
            }
        };
        if bytes.len() as u64 != size || object_id(e.kind, &bytes) != crate::fsx::hex(&e.id) {
            return Err(bad("does not hash to its name"));
        }
        let bytes = Arc::new(bytes);
        if depth > 0
            && let Ok(mut c) = self.cache.lock()
        {
            if c.1 + bytes.len() > CACHE_BYTES {
                c.0.clear();
                c.1 = 0;
            }
            c.1 += bytes.len();
            c.0.insert(k, bytes.clone());
        }
        Ok(bytes)
    }
}

/// One object for [`write`]: its kind, id, bytes, and the entry number of
/// its delta base with the delta, if it has one.
pub(crate) struct NewEntry {
    pub kind: Kind,
    pub id: [u8; 32],
    pub size: u64,
    pub base: Option<u32>,
    /// The bytes stored: the object, or the delta against `base`.
    pub payload: Vec<u8>,
}

/// Encode a pack and its index: (name, pack bytes, index bytes).
pub(crate) fn encode(entries: &[NewEntry]) -> (String, Vec<u8>, Vec<u8>) {
    let mut pack = Vec::new();
    pack.extend_from_slice(b"CVPK");
    pack.extend_from_slice(&1u32.to_le_bytes());
    pack.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    let mut idx = Vec::with_capacity(12 + entries.len() * IDX_ENTRY + 32);
    idx.extend_from_slice(b"CVIX");
    idx.extend_from_slice(&1u32.to_le_bytes());
    idx.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        let offset = pack.len() as u64;
        match e.base {
            None => pack.push(0),
            Some(b) => {
                pack.push(1);
                pack.extend_from_slice(&b.to_le_bytes());
            }
        }
        varint(&mut pack, e.size);
        varint(&mut pack, e.payload.len() as u64);
        pack.extend_from_slice(&crate::deflate::zlib_compress(&e.payload));
        idx.push(kind_byte(e.kind));
        idx.extend_from_slice(&e.id);
        idx.extend_from_slice(&offset.to_le_bytes());
        idx.extend_from_slice(&((pack.len() as u64 - offset) as u32).to_le_bytes());
        idx.extend_from_slice(&e.size.to_le_bytes());
    }
    let h = blake3::hash(&pack);
    pack.extend_from_slice(h.as_bytes());
    let ih = blake3::hash(&idx);
    idx.extend_from_slice(ih.as_bytes());
    (h.to_hex().to_string(), pack, idx)
}

#[cfg(test)]
#[path = "tests/pack.rs"]
pub(crate) mod tests;
