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

/// Where the store a [`Lock`] pins lives, when it is not a directory the
/// caller already has (`docs/package-system.md` §7.3).
///
/// A lock addresses exactly one store, so one origin per lock. `rev` is a
/// full commit hash and never a name: a branch or tag moves, and a lock
/// that followed one would be the automatic substitution §4.5 exists to
/// rule out. Nothing here is trusted for *content* -- every pin also
/// carries the hash of its source, and `vcs fetch` re-checks that -- the
/// commit only decides which directory to look in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    /// A git URL or local path, as `git fetch` accepts it.
    pub git: String,
    /// The commit: 40 lowercase hex digits (SHA-1) or 64 (SHA-256).
    pub rev: String,
    /// The store's directory inside the repository.
    #[serde(default = "default_origin_path")]
    pub path: String,
}

fn default_origin_path() -> String {
    ".lex-sys-vcs".to_owned()
}

impl Origin {
    /// Refuse an origin that is not a pin: a ref instead of a hash, a path
    /// that leaves the repository, or a URL that git would read as an option.
    pub fn validate(&self) -> Result<(), String> {
        let hex = |s: &str| s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !(self.rev.len() == 40 || self.rev.len() == 64) || !hex(&self.rev) {
            return Err(format!(
                "origin `rev` must be a full commit hash (40 or 64 lowercase hex digits), found `{}`; \
                 a branch or tag name moves, so a lock never holds one",
                self.rev
            ));
        }
        if self.git.is_empty() || self.git.starts_with('-') || self.git.starts_with("ext::") {
            return Err(format!("origin `git` is not a repository location: `{}`", self.git));
        }
        let path = Path::new(&self.path);
        let bad = self.path.is_empty()
            || path.is_absolute()
            || path.components().any(|c| !matches!(c, std::path::Component::Normal(_)));
        if bad {
            return Err(format!(
                "origin `path` must be a relative path inside the repository, found `{}`",
                self.path
            ));
        }
        Ok(())
    }
}

/// `name -> LockEntry`, one consumer's own record of which dependency it
/// meant when it wrote `import <name>;` — not a store-wide concept, and
/// not shared between programs the way a store is shared between
/// consumers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lock {
    /// Where the store is, when it is somewhere else (§7.3). Absent for a
    /// lock written against a local directory, as every lock was before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<Origin>,
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
    /// The file parsed, but its `origin` is not a pin ([`Origin::validate`]).
    BadOrigin(String),
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
            LockError::BadOrigin(why) => write!(f, "bad origin in lock file: {why}"),
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
        let lock: Self = serde_json::from_slice(&bytes).map_err(LockError::Malformed)?;
        if let Some(origin) = &lock.origin {
            origin.validate().map_err(LockError::BadOrigin)?;
        }
        Ok(lock)
    }

    /// Where this lock's store lives, if it says.
    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }

    /// Record where the store lives, refusing anything that is not a pin.
    pub fn set_origin(&mut self, origin: Option<Origin>) -> Result<(), String> {
        if let Some(o) = &origin {
            o.validate()?;
        }
        self.origin = origin;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(rev: &str, path: &str) -> Origin {
        Origin { git: "https://example.com/r".into(), rev: rev.into(), path: path.into() }
    }

    const SHA1: &str = "c0c3852541e927c3e893331207f1a0aaf184bc2e";

    #[test]
    fn an_origin_is_a_full_commit_hash_and_a_path_inside_the_repository() {
        assert!(origin(SHA1, ".lex-sys-vcs/log").validate().is_ok());
        assert!(origin(&"a".repeat(64), "s").validate().is_ok());
        for rev in
            ["main", "v1.0", "c0c3852", &SHA1.to_uppercase(), &SHA1[..39], &"g".repeat(40), ""]
        {
            assert!(origin(rev, "s").validate().is_err(), "`{rev}` is not a pin");
        }
        for path in ["", "/abs", "../up", "a/../b", "./x", "a//b/.."] {
            assert!(origin(SHA1, path).validate().is_err(), "`{path}` leaves the repository");
        }
        for url in ["", "-oProxyCommand=x", "ext::sh -c x"] {
            let o = Origin { git: url.into(), rev: SHA1.into(), path: "s".into() };
            assert!(o.validate().is_err(), "`{url}` is not a repository location");
        }
    }

    #[test]
    fn a_lock_without_an_origin_loads_and_saves_as_before_and_one_with_it_round_trips() {
        let dir = std::env::temp_dir().join(format!("lex-sys-lock-origin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("l.lock");
        let mut lock = Lock::default();
        lock.save(&path).unwrap();
        assert!(!fs::read_to_string(&path).unwrap().contains("origin"), "no origin, no field");
        assert!(Lock::load(&path).unwrap().origin().is_none());

        lock.set_origin(Some(origin(SHA1, "s"))).unwrap();
        lock.save(&path).unwrap();
        assert_eq!(Lock::load(&path).unwrap().origin(), Some(&origin(SHA1, "s")));

        assert!(lock.set_origin(Some(origin("main", "s"))).is_err());
        fs::write(&path, fs::read_to_string(&path).unwrap().replace(SHA1, "main")).unwrap();
        assert!(matches!(Lock::load(&path), Err(LockError::BadOrigin(_))));
        let _ = fs::remove_dir_all(&dir);
    }
}
