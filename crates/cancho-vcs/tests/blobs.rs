//! `docs/package-system.md` §4.2's prerequisite: a hash alone cannot be
//! re-typechecked, only the source behind it can. Checked for the same
//! property `op_log.rs` checks for an operation -- what comes back is what
//! was written, named by what it hashes to.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use cancho_vcs::{BlobError, Blobs};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh, empty directory this test owns alone -- the same reason
/// `op_log.rs`'s own `scratch_dir` exists.
fn scratch_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("cancho-vcs-blobs-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir under the system temp dir");
    dir
}

#[test]
fn a_put_blob_reads_back_unchanged() {
    let dir = scratch_dir();
    let blobs = Blobs::open(&dir).expect("open creates the sources/ dir");
    let source = "fn seven() -> [] int { return 7; }\n";

    let hash = blobs.put(source).expect("put persists a fresh blob");
    let back = blobs.get(&hash).expect("get succeeds").expect("the blob is there");
    assert_eq!(back, source);
}

#[test]
fn an_absent_hash_reads_back_as_none_not_an_error() {
    let dir = scratch_dir();
    let blobs = Blobs::open(&dir).expect("open creates the sources/ dir");
    assert!(blobs.get(&"c".repeat(64)).expect("a missing blob is not an error").is_none());
}

#[test]
fn writing_the_same_text_twice_is_a_no_op_and_the_same_hash() {
    let dir = scratch_dir();
    let blobs = Blobs::open(&dir).expect("open creates the sources/ dir");
    let source = "fn seven() -> [] int { return 7; }\n";

    let first = blobs.put(source).expect("first put succeeds");
    let second = blobs.put(source).expect("second put is idempotent, not an error");
    assert_eq!(first, second);
}

#[test]
fn two_different_files_get_two_different_hashes() {
    let dir = scratch_dir();
    let blobs = Blobs::open(&dir).expect("open creates the sources/ dir");

    let a = blobs.put("fn a() -> [] int { return 1; }\n").expect("put succeeds");
    let b = blobs.put("fn b() -> [] int { return 2; }\n").expect("put succeeds");
    assert_ne!(a, b);
}

#[test]
fn a_blob_whose_bytes_disagree_with_its_own_name_is_refused() {
    let dir = scratch_dir();
    let blobs = Blobs::open(&dir).expect("open creates the sources/ dir");
    let real_hash = blobs.put("fn a() -> [] int { return 1; }\n").expect("put succeeds");

    // Hand-corrupt the stored bytes without touching the filename they are
    // addressed by -- the one case `put` cannot produce (it always hashes
    // what it writes) but a hand-edited or bit-rotted file on disk could.
    std::fs::write(
        dir.join("sources").join(format!("{real_hash}.cho")),
        "fn a() -> [] int { return 999; }\n",
    )
    .unwrap();

    match blobs.get(&real_hash) {
        Err(BlobError::IdentityMismatch { claimed, .. }) => assert_eq!(claimed, real_hash),
        other => panic!("expected IdentityMismatch, got {other:?}"),
    }
}
