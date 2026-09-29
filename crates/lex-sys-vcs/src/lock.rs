//! A working copy's own pins into someone else's store.
//!
//! `docs/package-system.md` §4.5: a name is chosen once, when a
//! dependency is first added, and resolved by hash forever after — this
//! is that choice, recorded. Keyed by **name**, unlike [`crate::Manifest`],
//! which is keyed by [`SigId`]: a consumer's own `import` refers to a
//! dependency by the name it was locked under, never by the hash it
//! happens to pin today, so the lock has to be look-up-able the same way.
//!
//! Addresses a single file the caller names, not a directory under a
//! store root the way [`crate::OpLog`]/[`crate::Manifest`]/[`crate::Blobs`]
//! do: a lock belongs to the *consumer*, one file per program, not to the
//! dependency's own store.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::operation::{SigId, StageId};

/// One dependency pin: the exact declaration a name was locked to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    pub sig_id: SigId,
    pub stage_id: StageId,
    /// The [`crate::Blobs`] hash of the source this declaration was
    /// locked from — the same field [`crate::ManifestEntry`] carries, and
    /// for the same reason: a pin without its source cannot be
    /// re-typechecked, only trusted.
    pub source_hash: String,
}

/// `name -> LockEntry`, one consumer's own record of which dependency it
/// meant when it wrote `import <name>;` — not a store-wide concept, and
/// not shared between programs the way a store is shared between
/// consumers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lock {
    entries: BTreeMap<String, LockEntry>,
}

/// Why [`Lock::load`] refused a file that was actually on disk.
#[derive(Debug)]
pub enum LockError {
    Io(io::Error),
    /// The bytes parsed as JSON but not as a `Lock` -- on-disk
    /// corruption or a hand-edited file, the same failure mode
    /// `ManifestError::Malformed` names for a manifest.
    Malformed(serde_json::Error),
}

impl From<io::Error> for LockError {
    fn from(e: io::Error) -> Self {
        LockError::Io(e)
    }
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Io(e) => write!(f, "{e}"),
            LockError::Malformed(e) => write!(f, "malformed lock file: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

impl Lock {
    /// Load a lock file at `path`, or an empty lock if none exists yet --
    /// the ordinary case for the first `vcs lock` a working copy ever
    /// runs, the same reason [`crate::Manifest::load`] treats an absent
    /// file as empty rather than as an error.
    pub fn load(path: &Path) -> Result<Self, LockError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        serde_json::from_slice(&bytes).map_err(LockError::Malformed)
    }

    /// Persist this lock to `path`. Atomic via a same-directory temp file
    /// plus rename, the same mechanics [`crate::Manifest::save`] uses.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = PathBuf::from(format!("{}.tmp", path.display()));
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// What this lock already pins `name` to, if anything.
    pub fn get(&self, name: &str) -> Option<&LockEntry> {
        self.entries.get(name)
    }

    /// Record (or overwrite) what `name` is locked to.
    pub fn insert(&mut self, name: String, entry: LockEntry) {
        self.entries.insert(name, entry);
    }

    /// Every name this lock pins, for `vcs resolve --lock` to read back.
    pub fn entries(&self) -> impl Iterator<Item = (&String, &LockEntry)> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
