//! `cancho vcs` as a repository, what stays in Rust (`docs/gate.md` G4, wave 4): the store survives a kill at every write (`docs/vcs-cli.md` section 5).
//!
//! The rest of this file moved to `tests/commands/vcs_cycle.sh` (the pinned JSON of every command, every refusal tag, the anchored edit, the binary file
//! and the link, two writers racing on a ref, the old publish log) and `tests/commands/vcs_roundtrip.sh` (a git history round-trips to the same commit
//! ids); the pins are `tests/vcs_fixtures/pins/`. This test stays because it needs a failpoint: `CANCHO_VCS_CRASH_AT=n` makes the Rust command exit 137 at its
//! n-th write, and the command written in cancho has none (issue 687: `exit` at a chosen point; `docs/vcs-native.md` section 3). It goes when that exists, or
//! with the Rust command.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_cancho");
const GOOD: &str = "module a;\npub fn twice(n: int) -> [] int { return n + n; }\n";

struct Sandbox {
    dir: TempDir,
}

fn run_in(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (i32, Value, String) {
    let mut c = Command::new(BIN);
    c.arg("vcs")
        .args(args)
        .args(["--output", "json"])
        .current_dir(dir)
        .env("CANCHO_AUTHOR", "Ada <ada@example.com>");
    for (k, v) in env {
        c.env(k, v);
    }
    let out = c.output().expect("cancho runs");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let doc = serde_json::from_str(&stdout).unwrap_or(Value::Null);
    (out.status.code().unwrap_or(-1), doc, String::from_utf8_lossy(&out.stderr).into_owned())
}

impl Sandbox {
    fn new() -> Sandbox {
        Sandbox { dir: tempfile::tempdir().expect("tempdir") }
    }
    fn path(&self) -> &Path {
        self.dir.path()
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.path().join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.path().join(rel)).unwrap_or_default()
    }
    fn run(&self, args: &[&str]) -> (i32, Value) {
        let (code, doc, err) = run_in(self.path(), args, &[]);
        assert!(!doc.is_null(), "`{args:?}` printed no JSON (exit {code}): {err}");
        (code, doc)
    }
    fn change(&self, args: &[&str]) -> String {
        self.pin_quiet(args)["change"].as_str().unwrap().to_owned()
    }
    fn pin_quiet(&self, args: &[&str]) -> Value {
        let (code, doc) = self.run(args);
        assert_eq!(code, 0, "{args:?} failed: {doc}");
        doc
    }
}

/// Copy a directory tree (the crash test needs a fresh store per run).
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type().unwrap().is_dir() {
            copy_dir(&src, &dst);
        } else {
            fs::copy(&src, &dst).unwrap();
        }
    }
}

/// `docs/vcs-cli.md` section 5: kill the process at every write of a
/// `record` and of a `switch`; the store is sound after each, the ref is at
/// the old change or the new one (never between), and re-running the same
/// command finishes with the same result as a run that was never killed.
#[test]
fn a_kill_at_every_write_leaves_a_sound_store() {
    let base = Sandbox::new();
    base.pin_quiet(&["init"]);
    base.write("a.cho", GOOD);
    base.write("notes.md", "one\n");
    let first = base.change(&["record", "--all", "--intent-prompt", "first", "--time", "1"]);
    base.write("notes.md", "two\n");
    base.write("new/dir/file.txt", "hello\n");
    let record: Vec<&str> = vec!["record", "--all", "--intent-prompt", "second", "--time", "2"];

    // The reference: a record that is never killed.
    let reference = Sandbox::new();
    copy_dir(base.path(), reference.path());
    let want = reference.change(&record);
    assert_ne!(want, first);

    let mut killed = 0;
    for n in 1..200 {
        let work = Sandbox::new();
        copy_dir(base.path(), work.path());
        let (code, _, _) = run_in(work.path(), &record, &[("CANCHO_VCS_CRASH_AT", &n.to_string())]);
        if code != 137 {
            // The command finished before the n-th write: nothing left to kill.
            assert_eq!(code, 0, "record at n={n}");
            break;
        }
        killed += 1;
        // After the kill: a held lock is reported (never broken silently); clear it explicitly.
        let (vcode, v, _) = run_in(work.path(), &["verify"], &[]);
        assert_eq!(vcode, 0, "verify after a kill at write {n}: {v}");
        let head = work.pin_quiet(&["status"])["head"].clone();
        assert!(
            head == json!(first) || head == json!(want),
            "write {n}: HEAD is {head}, neither old nor new"
        );
        if v["locks"].as_array().is_some_and(|l| !l.is_empty()) {
            // The lock is reported, or the kill came after the ref moved and
            // there is nothing left to record; never a silent second write.
            let (_, d) = work.run(&["record", "--all", "--time", "2"]);
            assert!(
                d["refused"]["tag"] == json!("lock-held") || d["change"] == json!(want),
                "write {n}: {d}"
            );
            work.pin_quiet(&["unlock"]);
        }
        // Re-running finishes the job, to the same change.
        let again = work.pin_quiet(&record);
        assert_eq!(again["change"], json!(want), "write {n}");
        assert_eq!(work.pin_quiet(&["verify"])["sound"], json!(true));
        assert!(work.pin_quiet(&["status"])["clean"].as_bool().unwrap(), "write {n}");
    }
    assert!(killed >= 10, "the record has at least 10 write points, saw {killed}");

    // The same for switching branches (which rewrites the working copy and HEAD).
    let sw = Sandbox::new();
    copy_dir(reference.path(), sw.path());
    sw.pin_quiet(&["branch", "create", "old", "--from", "HEAD~1"]);
    for n in 1..200 {
        let work = Sandbox::new();
        copy_dir(sw.path(), work.path());
        let (code, _, _) =
            run_in(work.path(), &["switch", "old"], &[("CANCHO_VCS_CRASH_AT", &n.to_string())]);
        if code != 137 {
            assert_eq!(code, 0, "switch at n={n}");
            break;
        }
        work.pin_quiet(&["unlock"]);
        assert_eq!(work.pin_quiet(&["verify"])["sound"], json!(true), "switch killed at {n}");
        work.pin_quiet(&["switch", "old", "--discard"]);
        assert_eq!(work.read("notes.md"), "one\n", "switch killed at {n}");
        assert!(work.pin_quiet(&["status"])["clean"].as_bool().unwrap());
    }
}
