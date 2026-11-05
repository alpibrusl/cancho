//! The object store: immutable, content-addressed, plain files
//! (`docs/vcs-cli.md` section 3). Layout `objects/<kind>/<2 hex>/<62 hex>`,
//! the bytes of the object and nothing else, named by BLAKE3 of
//! `kind, 0x00, bytes`. A read re-hashes, so a damaged file is reported as
//! `object-corrupt` and never trusted.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Refusal, Result};
use crate::fsx;

/// Files larger than this are refused (`file-too-large`), not silently stored.
pub const MAX_BLOB: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug)]
pub struct Store {
    pub(crate) vcs: PathBuf,
}

impl Store {
    pub fn new(vcs: &Path) -> Store {
        Store { vcs: vcs.to_path_buf() }
    }

    fn path(&self, kind: Kind, id: &str) -> PathBuf {
        self.vcs.join("objects").join(kind.name()).join(&id[..2]).join(&id[2..])
    }

    pub(crate) fn tmp(&self) -> PathBuf {
        self.vcs.join("tmp")
    }

    pub fn has(&self, kind: Kind, id: &str) -> bool {
        is_id(id) && self.path(kind, id).is_file()
    }

    /// Store `bytes`; writing what is already there is a no-op (idempotent).
    pub fn put(&self, kind: Kind, bytes: &[u8]) -> Result<String> {
        if kind == Kind::Blob && bytes.len() > MAX_BLOB {
            return Err(Refusal::new(
                "file-too-large",
                format!("{} bytes is over the {MAX_BLOB} byte bound", bytes.len()),
            ));
        }
        let id = object_id(kind, bytes);
        let dest = self.path(kind, &id);
        if dest.is_file() {
            return Ok(id);
        }
        fsx::write_atomic(&self.tmp(), &dest, bytes, false, true)
            .map_err(|e| Refusal::io(format!("cannot write object {id}"), &e))?;
        Ok(id)
    }

    pub fn get(&self, kind: Kind, id: &str) -> Result<Vec<u8>> {
        if !is_id(id) {
            return Err(Refusal::new("bad-id", format!("`{id}` is not an object id")));
        }
        let bytes = fs::read(self.path(kind, id)).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Refusal::new("object-missing", format!("no {} `{id}`", kind.name()))
            } else {
                Refusal::io(format!("cannot read {} {id}", kind.name()), &e)
            }
        })?;
        if object_id(kind, &bytes) != id {
            return Err(Refusal::new(
                "object-corrupt",
                format!("{} {id} does not hash to its name", kind.name()),
            ));
        }
        Ok(bytes)
    }

    /// Every id of `kind` on disk (for `verify`), sorted.
    pub fn list(&self, kind: Kind) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let dir = self.vcs.join("objects").join(kind.name());
        let Ok(rd) = fs::read_dir(&dir) else { return Ok(ids) };
        for sub in rd.flatten() {
            let prefix = sub.file_name().to_string_lossy().into_owned();
            if let Ok(inner) = fs::read_dir(sub.path()) {
                for f in inner.flatten() {
                    ids.push(format!("{prefix}{}", f.file_name().to_string_lossy()));
                }
            }
        }
        ids.sort();
        Ok(ids)
    }
}
