//! The object store: immutable, content-addressed, plain files
//! (`docs/vcs-cli.md` section 3). An object is named by BLAKE3 of
//! `kind, 0x00, bytes` and lives in one of three places, read in this order:
//!
//! * `objects/<kind>/<2 hex>/<62 hex>.z`: one zlib stream of the bytes
//!   (what `put` writes; `deflate.rs`);
//! * `objects/<kind>/<2 hex>/<62 hex>`: the bytes and nothing else (what
//!   V1 wrote; still read, never written);
//! * a pack, `objects/pack/pack-<h>.{pack,idx}` (what `gc` writes; `pack.rs`).
//!
//! A read re-hashes whatever it found, so a damaged file is reported as
//! `object-corrupt` and never trusted.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::error::{Refusal, Result};
use crate::fsx;
use crate::pack::{self, Pack};

/// Files larger than this are refused (`file-too-large`), not silently stored.
pub const MAX_BLOB: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Blob,
    Tree,
    Change,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Blob => "blob",
            Kind::Tree => "tree",
            Kind::Change => "change",
        }
    }
    pub const ALL: [Kind; 3] = [Kind::Blob, Kind::Tree, Kind::Change];
}

pub fn object_id(kind: Kind, bytes: &[u8]) -> String {
    let mut h = blake3::Hasher::new();
    h.update(kind.name().as_bytes());
    h.update(&[0]);
    h.update(bytes);
    h.finalize().to_hex().to_string()
}

pub fn is_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The packs as last loaded: `None` until first needed.
type Packs = Arc<Mutex<Option<Arc<Result<Vec<Pack>>>>>>;

#[derive(Clone, Debug)]
pub struct Store {
    pub(crate) vcs: PathBuf,
    packs: Packs,
}

pub(crate) fn id_bytes(id: &str) -> Option<[u8; 32]> {
    fsx::unhex(id)?.try_into().ok()
}

impl Store {
    pub fn new(vcs: &Path) -> Store {
        Store { vcs: vcs.to_path_buf(), packs: Arc::new(Mutex::new(None)) }
    }

    pub(crate) fn path(&self, kind: Kind, id: &str) -> PathBuf {
        self.vcs.join("objects").join(kind.name()).join(&id[..2]).join(&id[2..])
    }

    /// The compressed loose file of an object.
    pub(crate) fn zpath(&self, kind: Kind, id: &str) -> PathBuf {
        self.vcs.join("objects").join(kind.name()).join(&id[..2]).join(format!("{}.z", &id[2..]))
    }

    pub(crate) fn tmp(&self) -> PathBuf {
        self.vcs.join("tmp")
    }

    /// The packs, loaded on first use; `reload` reads the directory again
    /// (after `gc`, or when another process may have packed what we miss).
    pub(crate) fn packs(&self, reload: bool) -> Arc<Result<Vec<Pack>>> {
        let mut g = match self.packs.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        match &*g {
            Some(p) if !reload => p.clone(),
            _ => {
                let p = Arc::new(pack::load_all(&self.vcs));
                *g = Some(p.clone());
                p
            }
        }
    }

    fn in_packs(&self, kind: Kind, id: &str) -> bool {
        let Some(raw) = id_bytes(id) else { return false };
        match &*self.packs(false) {
            Ok(ps) => ps.iter().any(|p| p.find(kind, &raw).is_some()),
            Err(_) => false,
        }
    }

    pub fn has(&self, kind: Kind, id: &str) -> bool {
        is_id(id)
            && (self.zpath(kind, id).is_file()
                || self.path(kind, id).is_file()
                || self.in_packs(kind, id))
    }

    /// Store `bytes`, compressed; writing what is already there (loose,
    /// compressed or not, or packed) is a no-op (idempotent).
    pub fn put(&self, kind: Kind, bytes: &[u8]) -> Result<String> {
        if kind == Kind::Blob && bytes.len() > MAX_BLOB {
            return Err(Refusal::new(
                "file-too-large",
                format!("{} bytes is over the {MAX_BLOB} byte bound", bytes.len()),
            ));
        }
        let id = object_id(kind, bytes);
        if self.has(kind, &id) {
            return Ok(id);
        }
        let z = crate::deflate::zlib_compress(bytes);
        fsx::write_atomic(&self.tmp(), &self.zpath(kind, &id), &z, false, true)
            .map_err(|e| Refusal::io(format!("cannot write object {id}"), &e))?;
        Ok(id)
    }

    pub fn get(&self, kind: Kind, id: &str) -> Result<Vec<u8>> {
        if !is_id(id) {
            return Err(Refusal::new("bad-id", format!("`{id}` is not an object id")));
        }
        let corrupt = || {
            Refusal::new(
                "object-corrupt",
                format!("{} {id} does not hash to its name", kind.name()),
            )
        };
        let bytes = match read_opt(&self.zpath(kind, id), kind, id)? {
            Some(z) => {
                let (bytes, used) =
                    crate::inflate::zlib_decode(&z, pack::MAX_OBJECT).map_err(|_| corrupt())?;
                if used != z.len() {
                    return Err(corrupt());
                }
                bytes
            }
            None => match read_opt(&self.path(kind, id), kind, id)? {
                Some(b) => b,
                None => return self.get_packed(kind, id),
            },
        };
        if object_id(kind, &bytes) != id {
            return Err(corrupt());
        }
        Ok(bytes)
    }

    /// From a pack; a miss reloads the packs once, since `gc` in another
    /// process may have moved the object there after this one looked.
    fn get_packed(&self, kind: Kind, id: &str) -> Result<Vec<u8>> {
        let raw = id_bytes(id).ok_or_else(|| Refusal::new("bad-id", format!("`{id}`")))?;
        for reload in [false, true] {
            let packs = self.packs(reload);
            let ps = packs.as_ref().as_ref().map_err(Clone::clone)?;
            for p in ps {
                if let Some(k) = p.find(kind, &raw) {
                    return Ok(p.read(k)?.as_ref().clone());
                }
            }
        }
        Err(Refusal::new("object-missing", format!("no {} `{id}`", kind.name())))
    }

    /// Every id of `kind` in the store, loose or packed (for `verify`),
    /// sorted and without repeats. A damaged pack index is `object-corrupt`.
    pub fn list(&self, kind: Kind) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for (_, id, _) in self.loose(kind) {
            ids.push(id);
        }
        let packs = self.packs(true);
        for p in packs.as_ref().as_ref().map_err(Clone::clone)? {
            ids.extend(p.entries.iter().filter(|e| e.kind == kind).map(|e| fsx::hex(&e.id)));
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// The loose files of `kind`: (file, name as an id, compressed). A name
    /// that is not an id is listed as it is, so `verify` can report it.
    pub(crate) fn loose(&self, kind: Kind) -> Vec<(PathBuf, String, bool)> {
        let mut out = Vec::new();
        let dir = self.vcs.join("objects").join(kind.name());
        let Ok(rd) = fs::read_dir(&dir) else { return out };
        for sub in rd.flatten() {
            let prefix = sub.file_name().to_string_lossy().into_owned();
            if let Ok(inner) = fs::read_dir(sub.path()) {
                for f in inner.flatten() {
                    let name = f.file_name().to_string_lossy().into_owned();
                    let (rest, z) = match name.strip_suffix(".z") {
                        Some(r) => (r.to_owned(), true),
                        None => (name, false),
                    };
                    out.push((f.path(), format!("{prefix}{rest}"), z));
                }
            }
        }
        out
    }
}

/// A file's bytes, `None` if it does not exist.
fn read_opt(path: &Path, kind: Kind, id: &str) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Refusal::io(format!("cannot read {} {id}", kind.name()), &e)),
    }
}
