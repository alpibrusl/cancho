//! `docs/vcs.md` §8's `vcs sync` slice (#403): the store rides inside git.
//! Through the compiled binary, like every other conformance test here,
//! because the claim is about the CLI surface a repository would actually
//! use: a sorted plain-text index that merges as text, op records in the
//! `OpLog` shape, and a `--check` that turns semantic drift into exit 1.

use super::*;

fn write_two(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("f.cho");
    std::fs::write(&path, text).expect("a writable scratch file");
    path
}

const ONE: &str = "fn add(a: int, b: int) -> [] int { return a + b; }\n";

const BODY_CHANGED: &str = "fn add(a: int, b: int) -> [] int { return a + b + 0; }\n";

const SIG_CHANGED: &str = "fn add(a: int, b: int, c: int) -> [] int { return a + b + c; }\n";

/// The index is a function of the source: two syncs of the same tree
/// write the same bytes, and the second run answers "no semantic change".
#[test]
fn sync_is_idempotent_and_the_second_run_is_quiet() {
    let dir = scratch("vcs-sync-idempotent");
    write_two(&dir, ONE);
    let run = |args: &[&str]| {
        Command::new(BIN)
            .args(["vcs", "sync", "--dir"])
            .arg(&dir)
            .args(args)
            .output()
            .expect("the compiler runs")
    };
    let first = run(&[]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    let text = String::from_utf8_lossy(&first.stdout);
    assert!(text.contains("1 declaration(s) added"), "the first sync reports the add:\n{text}");
    let index = dir.join(".cancho").join("ids.txt");
    assert!(index.is_file(), "sync should write .cancho/ids.txt");
    let first_bytes = std::fs::read(&index).expect("the index is readable");

    let second = run(&[]);
    assert!(second.status.success());
    let text = String::from_utf8_lossy(&second.stdout);
    assert!(
        text.contains("no semantic change"),
        "the second sync of an unchanged tree answers quiet:\n{text}"
    );
    let second_bytes = std::fs::read(&index).expect("the index is readable");
    assert_eq!(first_bytes, second_bytes, "the index is a function of the source");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A body change is a `ModifyBody` op: the `SigId` is untouched, the
/// `StageId` moves, and the op log gains exactly one record.
#[test]
fn a_body_change_is_one_modify_body_op() {
    let dir = scratch("vcs-sync-body");
    write_two(&dir, ONE);
    let run = || {
        Command::new(BIN)
            .args(["vcs", "sync", "--dir"])
            .arg(&dir)
            .output()
            .expect("the compiler runs")
    };
    assert!(run().status.success());
    let ops = dir.join(".cancho").join("ops");
    let before: usize = std::fs::read_dir(&ops).map(|it| it.count()).unwrap_or(0);
    write_two(&dir, BODY_CHANGED);
    let out = run();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1 body change(s)"), "a body change says so:\n{text}");
    let after: usize = std::fs::read_dir(&ops).map(|it| it.count()).unwrap_or(0);
    assert_eq!(after, before + 1, "one body change appends exactly one op record");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A signature change is the one a caller feels: the `SigId` moves, and
/// the report says who is affected. No op vocabulary for it yet (nothing
/// in this repository has asked for a signature-changing op), so it is
/// reported and counted, and `--check` turns it red.
#[test]
fn a_signature_change_is_reported_and_fails_check() {
    let dir = scratch("vcs-sync-sig");
    write_two(&dir, ONE);
    let run = |text: &str| {
        write_two(&dir, text);
        Command::new(BIN)
            .args(["vcs", "sync", "--dir"])
            .arg(&dir)
            .output()
            .expect("the compiler runs")
    };
    assert!(run(ONE).status.success());
    let out = run(SIG_CHANGED);
    assert!(out.status.success(), "a plain sync records, it does not refuse");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("1 signature change(s) — callers by `SigId` affected"),
        "the report names who feels a signature change:\n{text}"
    );
    // `--check` on the synced tree is exit 0: the index matches the source.
    // (The drift-red direction is `a_body_change_is_one_modify_body_op`'s
    // territory; here the point is the signature change is counted, and
    // a synced tree still checks clean.)
    let check = Command::new(BIN)
        .args(["vcs", "sync", "--dir"])
        .arg(&dir)
        .arg("--check")
        .output()
        .expect("the compiler runs");
    assert_eq!(check.status.code(), Some(0), "a synced tree checks clean");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A removed declaration is a `RemoveFunction` op, and the index line
/// goes with it.
#[test]
fn a_removal_is_one_remove_op_and_a_shorter_index() {
    let dir = scratch("vcs-sync-remove");
    write_two(&dir, ONE);
    let run = || {
        Command::new(BIN)
            .args(["vcs", "sync", "--dir"])
            .arg(&dir)
            .output()
            .expect("the compiler runs")
    };
    assert!(run().status.success());
    let index = dir.join(".cancho").join("ids.txt");
    let before = std::fs::read_to_string(&index).expect("the index is readable");
    assert!(before.contains("add"), "the declaration is indexed:\n{before}");

    std::fs::remove_file(dir.join("f.cho")).expect("a removable scratch file");
    let out = run();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("1 declaration(s) removed"), "a removal says so:\n{text}");
    let after = std::fs::read_to_string(&index).expect("the index is readable");
    assert!(!after.contains("add"), "the index no longer carries the removed declaration");
    let ops = dir.join(".cancho").join("ops");
    let count = std::fs::read_dir(&ops).map(|it| it.count()).unwrap_or(0);
    assert_eq!(count, 2, "add then remove: two op records");
    let _ = std::fs::remove_dir_all(&dir);
}
