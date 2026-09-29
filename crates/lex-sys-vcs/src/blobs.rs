//! Content-addressed storage for a published file's raw source.
//!
//! `docs/package-system.md` §4.2's own prerequisite: re-checking a pinned
//! declaration under today's compiler needs the *source* behind the pin,
//! not only its hash. Neither `OpLog` nor `Manifest` store it — an
//! operation's identity is about a declaration's shape, never about bytes
//! to render one back, and a manifest entry is a `SigId`/`StageId` pair
//! for the same reason. This is the missing third piece, kept as narrow as
//! `OpLog` is: one file per blob, named by its own hash, nothing else.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Why [`Blobs::get`] refused a blob that was actually on disk.
#[derive(Debug)]
pub enum BlobError {
    Io(io::Error),
    /// The bytes at `<hash>.ls` are not valid UTF-8 — lex-sys source
    /// always is, so this is on-disk corruption or a hand-edited file,
    /// never a program this crate wrote.
    NotUtf8(std::string::FromUtf8Error),
    /// The bytes at `<hash>.ls` do not hash to the name they are stored
    /// under — the same failure mode `OpLogError::IdentityMismatch` names
    /// for an operation, and the same reason it must never be silently
    /// trusted: a blob that lies about its own identity is not the blob
    /// its filename claims to be.
    IdentityMismatch {
        claimed: String,
        recomputed: String,
    },
}

impl From<io::Error> for BlobError {
    fn from(e: io::Error) -> Self {
        BlobError::Io(e)
    }
}

impl std::fmt::Display for BlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlobError::Io(e) => write!(f, "{e}"),
            BlobError::NotUtf8(e) => write!(f, "blob is not valid UTF-8: {e}"),
            BlobError::IdentityMismatch { claimed, recomputed } => {
                write!(f, "blob claims hash {claimed} but its bytes hash to {recomputed}")
            }
        }
    }
}

impl std::error::Error for BlobError {}

pub struct Blobs {
    dir: PathBuf,
}

impl Blobs {
    /// Open (creating if absent) the blob store under `<root>/sources/`.
    pub fn open(root: &Path) -> io::Result<Self> {
        let dir = root.join("sources");
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, hash: &str) -> PathBuf {
        self.dir.join(format!("{hash}.ls"))
    }

    /// The content address `put` would store `text` under, without writing
    /// anything — BLAKE3 of the raw bytes, lowercase hex. The same
    /// algorithm [`crate::canonical::op_id`] uses, for the reason that file
    /// already states: one hash function in this crate, not one per
    /// concept (`docs/vcs.md` §5's "shares the idea and no code" applies
    /// to which hash function as much as to which crate).
    pub fn hash_of(text: &str) -> String {
        blake3::hash(text.as_bytes()).to_hex().to_string()
    }

    /// Store `text`, addressed by its own hash, and return that hash.
    ///
    /// Idempotent: writing an already-present hash is a no-op, the same
    /// reason [`crate::OpLog::put`] is — content addressing means the
    /// bytes can only ever match.
    pub fn put(&self, text: &str) -> io::Result<String> {
        let hash = Self::hash_of(text);
        let path = self.path(&hash);
        if path.exists() {
            return Ok(hash);
        }
        let tmp = self.dir.join(format!("{hash}.tmp"));
        fs::write(&tmp, text.as_bytes())?;
        fs::rename(&tmp, &path)?;
        Ok(hash)
    }

    /// Read a blob back, or `Ok(None)` if no such hash is in the store.
    ///
    /// Recomputes the hash from the bytes and refuses a disagreement
    /// (`BlobError::IdentityMismatch`) rather than returning text under a
    /// name it may not actually have — [`crate::OpLog::get`]'s own rule,
    /// applied here.
    pub fn get(&self, hash: &str) -> Result<Option<String>, BlobError> {
        let bytes = match fs::read(self.path(hash)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let text = String::from_utf8(bytes).map_err(BlobError::NotUtf8)?;
        let recomputed = Self::hash_of(&text);
        if recomputed != hash {
            return Err(BlobError::IdentityMismatch { claimed: hash.to_owned(), recomputed });
        }
        Ok(Some(text))
    }
}
