//! A content-addressed operation log for lex-sys programs.
//!
//! `docs/vcs.md` is the design. Built: the `Operation` vocabulary and its
//! canonical, content-addressed identity (§8's foundation slice); the
//! write-time gate (`gate::check_candidate`); a loose-file op log
//! (`OpLog`); a hash-chained, Ed25519-sealable attestation log (`Chain`);
//! and, per `docs/vcs-publish.md`, a working copy's own manifest of what
//! it last published (`Manifest`) — the smallest piece that turns the rest
//! of this into something a real `.ls` file can actually go through
//! (`crates/lex-sys/src/vcs_cli.rs`). Per `docs/package-system.md` §4.2, a
//! content-addressed store for a published file's raw source (`Blobs`) —
//! the manifest and the op log both address a declaration's *shape*, and
//! neither can turn a pin back into text to re-check it. Per §4.5, a
//! consumer's own name-keyed pin file (`Lock`) — a `Manifest` says what a
//! *store* has; a `Lock` says what *one program* meant when it wrote a
//! name down. Per §4.6, a store's own dependency on another store
//! (`Requirement`) — a `Lock` says what *one program* meant; a
//! `Requirement` says what *the package itself* meant, chosen once at
//! publish time the same way. Not built, and named in `vcs.md` §8: whole-function merge,
//! multi-file merge sessions, typed issues, predicate branches, the op
//! log's own history index.
//!
//! This crate shares `lex-vcs`'s *scheme* — `String`-keyed ids, canonical
//! JSON, a content hash of `(kind, sorted parents, edition)` — and no code,
//! the same relationship `lex-os` and `lex-sys` already committed to
//! (`README.md`'s "Where this sits").

mod attestation;
mod blobs;
mod canonical;
mod gate;
mod lock;
mod manifest;
mod op_log;
mod operation;
mod requirement;

pub use attestation::{
    AttestationEvent, BrokenAt, Chain, ChainPayload, Entry as AttestationEntry, GENESIS, Seal,
};
pub use blobs::{BlobError, Blobs};
pub use canonical::canonical_bytes;
pub use ed25519_dalek::{SigningKey, VerifyingKey};
pub use gate::{GateDiagnostic, check_candidate};
pub use lock::{Lock, LockEntry, LockError, Origin};
pub use manifest::{Manifest, ManifestEntry, ManifestError};
pub use op_log::{OpLog, OpLogError};
pub use operation::{
    EffectSet, ModuleRef, OpId, Operation, OperationKind, OperationRecord, SigId, StageId,
};
pub use requirement::{
    Requirement, RequirementError, load_all as load_requirements, save_all as save_requirements,
};
