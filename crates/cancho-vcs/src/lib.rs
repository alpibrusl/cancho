//! A content-addressed operation log for cancho programs.
//!
//! `docs/vcs.md` is the design. Built: the `Operation` vocabulary and its
//! canonical, content-addressed identity (§8's foundation slice); the
//! write-time gate (`gate::check_candidate`); a loose-file op log
//! (`OpLog`); a hash-chained, Ed25519-sealable attestation log (`Chain`);
//! and, per `docs/vcs-publish.md`, a working copy's own manifest of what
//! it last published (`Manifest`) — the smallest piece that turns the rest
//! of this into something a real `.cho` file can actually go through
//! (`crates/cancho/src/vcs_cli.rs`). Per `docs/package-system.md` §4.2, a
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
//! the same relationship `lex-os` and `cancho` already committed to
//! (`README.md`'s "Where this sits").
#![forbid(unsafe_code)]

mod anchor;
mod attestation;
mod blobs;
mod bridge;
mod canonical;
mod change;
mod deflate;
mod delta;
mod diff_text;
mod error;
mod fsx;
mod gate;
mod gitread;
mod inflate;
mod lock;
mod manifest;
mod merge;
mod merge3;
mod merge_cho;
mod mergestate;
mod object;
mod op_log;
mod operation;
mod pack;
mod refs;
mod repack;
mod repo;
mod requirement;
mod sha1;
mod tree;

pub use anchor::{MAX_EDIT_BYTES, apply_edits, derive_edits};
pub use attestation::{
    AttestationEvent, BrokenAt, Chain, ChainPayload, Entry as AttestationEntry, GENESIS, Seal,
};
pub use blobs::{BlobError, Blobs};
pub use bridge::{ExportReport, ImportReport, export_git, import_git, load_gitmap};
pub use canonical::canonical_bytes;
pub use change::{Bytes, Change, DeclChange, Edit, GitCommit, Intent};
pub use diff_text::{Hunk, UnifiedHunk, diff_lines, split_lines, unified};
pub use ed25519_dalek::{SigningKey, VerifyingKey};
pub use error::{Refusal, Result as VcsResult};
pub use fsx::failpoints_passed;
pub use gate::{GateDiagnostic, check_candidate};
pub use gitread::GitRepo;
pub use lock::{Lock, LockEntry, LockError, Origin};
pub use manifest::{Manifest, ManifestEntry, ManifestError};
pub use merge::{
    Base, Conflict, Inputs, Resolutions, Settled, TreeMerge, file_directory_clash, is_ancestor,
    merge_base, merge_trees,
};
pub use merge_cho::{ChoMerge, Choice, DeclConflict, merge_cho};
pub use merge3::{ConflictHunk, Merged, Settle, merge_text, merge_with};
pub use mergestate::MergeState;
pub use object::{Kind, MAX_BLOB, Store, is_id, object_id};
pub use op_log::{OpLog, OpLogError};
pub use operation::{
    EffectSet, ModuleRef, OpId, Operation, OperationKind, OperationRecord, SigId, StageId,
};
pub use refs::{
    Expect, LockGuard, check_ref_name, delete_ref, list_refs, locks, now, read_log, read_ref,
    update_ref, work_lock,
};
pub use repack::{GcReport, gc};
pub use repo::{FORMAT, Head, Ignore, Repo, StatusEntry};
pub use requirement::{
    Requirement, RequirementError, load_all as load_requirements, save_all as save_requirements,
};
pub use tree::{ChangeKind, Entry, Mode, PathChange, Tree, check_path, diff_trees};
