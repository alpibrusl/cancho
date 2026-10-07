//! `docs/package-system.md` §4.5: a name is chosen once, when a
//! dependency is first added, and resolved by hash forever after --
//! checked here the way `manifest.rs`'s own tests would be, if it had
//! any at this level (its coverage lives in `crates/cancho/tests/
//! conformance/vcs.rs`, through the CLI; `Lock` gets the same treatment,
//! plus this file, because it is addressed by a bare file path rather
//! than a store root and that path-handling is worth its own proof).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use cancho_vcs::{Lock, LockEntry, LockError};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn scratch_path(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cancho-vcs-lock-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch dir under the system temp dir");
    dir.join(name)
}

fn sample_entry() -> LockEntry {
    LockEntry { sig_id: "a".repeat(64), stage_id: "b".repeat(64), source_hash: "c".repeat(64) }
}

#[test]
fn a_fresh_path_loads_as_an_empty_lock_not_an_error() {
    let path = scratch_path("does-not-exist.json");
    let lock = Lock::load(&path).expect("a missing lock file is not an error");
    assert!(lock.is_empty());
}

#[test]
fn an_inserted_entry_reads_back_unchanged_after_a_save_and_load() {
    let path = scratch_path("lock.json");
    let mut lock = Lock::load(&path).expect("a fresh load");
    lock.insert("lex-nt".to_owned(), sample_entry());
    lock.save(&path).expect("a writable lock file");

    let back = Lock::load(&path).expect("a readable lock file");
    assert_eq!(back.get("lex-nt"), Some(&sample_entry()));
    assert_eq!(back.len(), 1);
}

#[test]
fn an_absent_name_reads_back_as_none_not_an_error() {
    let path = scratch_path("lock.json");
    let lock = Lock::load(&path).expect("a fresh load");
    assert!(lock.get("nothing-here").is_none());
}

#[test]
fn inserting_under_an_existing_name_overwrites_it() {
    let path = scratch_path("lock.json");
    let mut lock = Lock::load(&path).expect("a fresh load");
    lock.insert("lex-nt".to_owned(), sample_entry());
    let second =
        LockEntry { sig_id: "d".repeat(64), stage_id: "e".repeat(64), source_hash: "f".repeat(64) };
    lock.insert("lex-nt".to_owned(), second.clone());
    assert_eq!(lock.get("lex-nt"), Some(&second));
    assert_eq!(lock.len(), 1, "one name, one entry, even after a second insert");
}

#[test]
fn a_lock_file_whose_bytes_are_not_json_is_refused_not_panicked_on() {
    let path = scratch_path("lock.json");
    std::fs::write(&path, b"not json at all").expect("a writable file");
    match Lock::load(&path) {
        Err(LockError::Malformed(_)) => {}
        other => panic!("expected Malformed, got {other:?}"),
    }
}
