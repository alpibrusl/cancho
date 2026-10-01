//! What a working copy's last publish already logged.
//!
//! `docs/vcs-publish.md` §4: a first publish, against an empty log, needs
//! no diffing at all -- every declaration is new by definition. The moment
//! a second publish runs, something has to answer "is this declaration
//! already logged, and if so, at what `StageId`" -- not to skip a
//! redundant write (`OpLog::put` is already idempotent on identical
//! content), but so a real change to an already-known declaration is
//! refused with a located, honest reason rather than silently logged as a
//! second, disconnected `AddFunction` with no relation to the first.
//!
//! This is deliberately not `lex-vcs`'s head: no branches, no merge, no
//! shared pointer two agents could race on advancing. It is the one fact
//! a single working copy's own next publish needs, at the same additive
//! discipline `editions.md` already applies to `Operation` itself, so a
//! real head can attach to this format later without it needing to
//! change.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::operation::{SigId, StageId};

/// What `publish` last recorded about one declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// For a human reading `lex-sys vcs log`; never part of any hash
    /// (`docs/vcs.md`'s own `ModuleRef` doc comment makes the same call).
    pub name: String,
    pub stage_id: StageId,
    /// The [`crate::Blobs`] hash of the file this declaration was
    /// published from — `docs/package-system.md` §4.2's prerequisite for
    /// `lex-sys vcs resolve`: a hash alone cannot be re-typechecked, only
    /// the source behind it can. Required rather than `Option`: unlike
    /// `Operation`, a `ManifestEntry` is not content-addressed and not a
    /// store-wide format (this file's own opening comment), so it carries
    /// no hash-stability obligation to stay additive.
    pub source_hash: String,
    /// The source `import`s `std.*`, so a consumer must build it with
    /// `--std` (`docs/package-system.md` §7). Informational: `resolve`
    /// and `fetch` re-derive it from the source they verify, never from
    /// this flag, so a hand-edited manifest cannot make a package look
    /// std-free. `#[serde(default)]` keeps a manifest written before the
    /// field readable.
    #[serde(default)]
    pub uses_std: bool,
}

/// `SigId -> ManifestEntry`, one working copy's own record of what it has
/// already published. Not a store-wide, multi-writer concept.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    entries: BTreeMap<SigId, ManifestEntry>,
}

/// Why [`Manifest::load`] refused a file that was actually on disk.
#[derive(Debug)]
pub enum ManifestError {
    Io(io::Error),
    /// The bytes at `manifest.json` parsed as JSON but not as a `Manifest`
    /// -- on-disk corruption or a hand-edited file, the same failure mode
    /// `OpLogError::Malformed` names for the op log.
    Malformed(serde_json::Error),
}

impl From<io::Error> for ManifestError {
    fn from(e: io::Error) -> Self {
        ManifestError::Io(e)
    }
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "{e}"),
            ManifestError::Malformed(e) => write!(f, "malformed manifest: {e}"),
        }
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    fn path(root: &Path) -> PathBuf {
        root.join("manifest.json")
    }

    /// Load `<root>/manifest.json`, or an empty manifest if the working
    /// copy has never published before -- the ordinary case, not an
    /// error: `docs/vcs-publish.md` §3's whole point is that a first
    /// publish starts here.
    pub fn load(root: &Path) -> Result<Self, ManifestError> {
        let bytes = match fs::read(Self::path(root)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        serde_json::from_slice(&bytes).map_err(ManifestError::Malformed)
    }

    /// Persist this manifest. Atomic via a same-directory temp file plus
    /// rename, the same mechanics `OpLog::put` already uses and for the
    /// same reason: a concurrent reader never observes a partial write.
    pub fn save(&self, root: &Path) -> io::Result<()> {
        fs::create_dir_all(root)?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let path = Self::path(root);
        let tmp = root.join("manifest.json.tmp");
        fs::write(&tmp, &bytes)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// What this manifest already knows about `sig_id`, if anything.
    pub fn get(&self, sig_id: &SigId) -> Option<&ManifestEntry> {
        self.entries.get(sig_id)
    }

    /// Record (or overwrite) what `publish` just logged for `sig_id`.
    pub fn insert(&mut self, sig_id: SigId, entry: ManifestEntry) {
        self.entries.insert(sig_id, entry);
    }

    /// Every declaration this manifest knows about, by `SigId`, for
    /// `lex-sys vcs log` to read back.
    pub fn entries(&self) -> impl Iterator<Item = (&SigId, &ManifestEntry)> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
