//! `cancho vcs` as a repository (`docs/vcs-cli.md` sections 4, 5, 7 and 9,
//! slice V1): every command's JSON is pinned in `tests/vcs_pins/*.json`
//! (regenerate with `UPDATE_VCS_PINS=1 cargo test -p cancho --test
//! vcs_repo` and read the diff), every refusal tag V1 has is reached, the
//! store survives a kill at every write, two writers racing on a ref have
//! one winner, and a git history round-trips to the same commit ids.

use std::fs;
use std::path::{Path, PathBuf};
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
    /// Run, expect success, and pin the document under `name`.
    fn pin(&self, name: &str, args: &[&str]) -> Value {
        let (code, doc) = self.run(args);
        assert_eq!(code, 0, "{args:?} failed: {doc}");
        assert_eq!(doc["ok"], json!(true));
        pin(name, &doc, self.path());
        doc
    }
    fn refuse(&self, name: Option<&str>, args: &[&str], tag: &str) -> Value {
        let (code, doc) = self.run(args);
        assert_eq!(code, 1, "{args:?} should be refused: {doc}");
        assert_eq!(doc["ok"], json!(false));
        assert_eq!(doc["refused"]["tag"], json!(tag), "{doc}");
        if let Some(n) = name {
            pin(n, &doc, self.path());
        }
        doc
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

fn normalise(v: &Value, root: &Path) -> Value {
    let canon = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let text = serde_json::to_string(v)
        .unwrap()
        .replace(canon.to_str().unwrap(), "<root>")
        .replace(root.to_str().unwrap(), "<root>");
    serde_json::from_str(&text).unwrap()
}

fn pin(name: &str, doc: &Value, root: &Path) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vcs_pins")
        .join(format!("{name}.json"));
    let got = format!("{}\n", serde_json::to_string_pretty(&normalise(doc, root)).unwrap());
    if std::env::var_os("UPDATE_VCS_PINS").is_some() {
        fs::write(&path, &got).unwrap();
        return;
    }
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no pin {name}; run with UPDATE_VCS_PINS=1"));
    assert_eq!(got, want, "the JSON of `{name}` changed (UPDATE_VCS_PINS=1 to accept)");
}

/// A repository with two recorded changes (`a.cho`, `notes.md`).
fn two_changes() -> Sandbox {
    let s = Sandbox::new();
    s.write("a.cho", GOOD);
    s.write("notes.md", "# notes\none\ntwo\nthree\n");
    s.pin("init", &["init"]);
    s.pin("status_dirty", &["status"]);
    s.pin(
        "record_first",
        &[
            "record",
            "--all",
            "--intent-prompt",
            "first",
            "--intent-model",
            "m1",
            "--intent-session",
            "s1",
            "--time",
            "1000",
        ],
    );
    s.pin("status_clean", &["status"]);
    s.write("a.cho", "module a;\npub fn twice(n: int) -> [] int { return n + n + 0; }\npub fn thrice(n: int) -> [] int { return n * 3; }\n");
    s.write("notes.md", "# notes\none\n2\nthree\n");
    s
}

#[test]
fn the_whole_cycle_is_pinned() {
    let s = two_changes();
    s.pin("diff", &["diff"]);
    s.pin(
        "record_second",
        &["record", "a.cho", "--intent-prompt", "second", "--intent-issue", "7", "--time", "1001"],
    );
    s.pin("status_one_left", &["status"]);
    s.pin(
        "record_third",
        &["record", "--all", "--intent-prompt", "third: notes", "--time", "1002"],
    );
    s.pin("log", &["log"]);
    s.pin("log_intent", &["log", "--intent", "notes"]);
    s.pin("log_path", &["log", "--path", "notes.md"]);
    s.pin("show", &["show", "HEAD"]);
    s.pin("show_patch", &["show", "HEAD~1", "--patch"]);
    s.pin("blame", &["blame", "notes.md"]);
    s.pin("blame_decl", &["blame", "a.cho", "--decl", "fn a.twice"]);
    s.pin("diff_revisions", &["diff", "HEAD~2", "HEAD"]);

    // Branches, switching, and the working copy following.
    s.pin("branch_create", &["branch", "create", "feature", "--from", "HEAD~1"]);
    s.pin("branch_list", &["branch", "list"]);
    s.pin("switch", &["switch", "feature"]);
    assert_eq!(s.read("notes.md"), "# notes\none\ntwo\nthree\n");
    s.pin("switch_again", &["switch", "feature"]);
    s.write("extra.txt", "x\n");
    s.refuse(Some("refuse_dirty_switch"), &["switch", "main"], "dirty-working-copy");
    s.pin("switch_keep", &["switch", "main", "--keep"]);
    assert_eq!(s.read("extra.txt"), "x\n");
    fs::remove_file(s.path().join("extra.txt")).unwrap();
    s.pin("branch_delete", &["branch", "delete", "feature", "--expect", "HEAD~1"]);

    // Undo: revert a change, restore a file, both idempotent.
    s.pin("revert", &["revert", "HEAD", "--time", "1003"]);
    assert_eq!(s.read("notes.md"), "# notes\none\ntwo\nthree\n");
    s.pin("revert_again", &["revert", "HEAD~1", "--time", "1004"]);
    s.write("notes.md", "scribble\n");
    s.pin("restore_dry", &["restore", "notes.md", "--dry-run"]);
    s.pin("restore", &["restore", "notes.md"]);
    assert_eq!(s.read("notes.md"), "# notes\none\ntwo\nthree\n");
    s.pin("restore_again", &["restore", "notes.md"]);
    s.pin("verify", &["verify"]);
    s.pin("unlock", &["unlock"]);

    // Idempotent record: nothing changed, nothing recorded.
    s.pin("record_unchanged", &["record", "--all", "--time", "1005"]);
    s.pin("init_again", &["init"]);
}

#[test]
fn every_refusal_has_its_tag() {
    let s = two_changes();
    s.pin_quiet(&["record", "--all", "--intent-prompt", "base", "--time", "1001"]);
    // not-a-repository
    let empty = Sandbox::new();
    empty.refuse(Some("refuse_not_a_repository"), &["status"], "not-a-repository");
    // nothing-selected, bad-ref-name, bad-revision
    s.refuse(None, &["record"], "nothing-selected");
    s.refuse(None, &["branch", "create", "bad..name"], "bad-ref-name");
    s.refuse(None, &["show", "nonesuch"], "bad-revision");
    // path-escapes
    s.refuse(Some("refuse_path_escapes"), &["restore", "../elsewhere"], "path-escapes");
    // gate-failed: the rule tag is the refusal
    s.write("a.cho", "module a;\npub fn twice(n: int) -> [] int { return true; }\n");
    s.refuse(
        Some("refuse_gate_failed"),
        &["record", "a.cho", "--intent-prompt", "bad", "--time", "1002"],
        "gate-failed",
    );
    s.write("a.cho", "module a;\npub fn twice(n: int) -> [] int { return n +; }\n");
    let d = s.refuse(None, &["record", "a.cho", "--time", "1002"], "gate-failed");
    assert_eq!(d["refused"]["path"], json!("a.cho"));
    // The same refusal is the same on the next run.
    let d2 = s.refuse(None, &["record", "a.cho", "--time", "1002"], "gate-failed");
    assert_eq!(d["refused"], d2["refused"]);
    s.pin_quiet(&["restore", "a.cho"]);
    // file-too-large
    fs::write(s.path().join("big.bin"), vec![1u8; 16 * 1024 * 1024 + 1]).unwrap();
    s.refuse(None, &["record", "big.bin"], "file-too-large");
    fs::remove_file(s.path().join("big.bin")).unwrap();
    // ref-moved: a stale --expect
    s.write("notes.md", "changed\n");
    let head = s.change(&["record", "--all", "--intent-prompt", "n", "--time", "1003"]);
    s.write("notes.md", "changed again\n");
    let (_, d) = s.run(&["record", "--all", "--expect", "HEAD~1", "--time", "1004"]);
    assert_eq!(d["refused"]["tag"], json!("ref-moved"));
    assert_eq!(
        d["refused"]["current"],
        json!(head),
        "the loser is told where the winner left the ref"
    );
    s.pin_quiet(&["restore", "notes.md"]);
    // ref-protected
    s.refuse(None, &["branch", "delete", "main", "--expect", "HEAD"], "ref-protected");
    // lock-held, and unlock clears it
    fs::write(s.path().join(".cancho/vcs/lock"), "99999 1").unwrap();
    s.write("notes.md", "locked out\n");
    let d = s.refuse(None, &["record", "--all", "--time", "1005"], "lock-held");
    assert_eq!(d["refused"]["pid"], json!(99999));
    s.pin_quiet(&["unlock"]);
    s.pin_quiet(&["restore", "notes.md"]);
    // revert-conflict and revert-unsupported
    s.refuse(None, &["revert", "HEAD~1", "--time", "1006"], "revert-conflict");
    s.refuse(None, &["revert", "HEAD~2", "--time", "1006"], "revert-unsupported");
    // object-corrupt, found by verify
    let blob = fs::read_dir(s.path().join(".cancho/vcs/objects/blob"))
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    let file = fs::read_dir(&blob).unwrap().flatten().next().unwrap().path();
    fs::write(&file, b"tampered").unwrap();
    s.refuse(None, &["verify"], "object-corrupt");
}

#[test]
fn an_anchored_edit_applies_once_or_not_at_all() {
    let s = Sandbox::new();
    s.pin_quiet(&["init"]);
    s.write("f.txt", "a\nb\nc\nb\nd\n");
    s.pin_quiet(&["record", "--all", "--time", "1"]);
    // Recording a modification derives the edit list.
    s.write("f.txt", "a\nb\nc\nB\nd\n");
    let rec = s.pin_quiet(&["record", "--all", "--time", "2"]);
    assert_eq!(rec["edits"], json!(1));
    let shown = s.pin("show_edits", &["show", "HEAD"]);
    let edits = shown["edits"].clone();
    assert_eq!(edits.as_array().unwrap().len(), 1);
    // Replaying it onto the older text lands exactly where it was made.
    s.pin_quiet(&["restore", "f.txt", "--from", "HEAD~1"]);
    assert_eq!(s.read("f.txt"), "a\nb\nc\nb\nd\n");
    fs::write(s.path().join("edits.json"), edits.to_string()).unwrap();
    s.pin("apply", &["apply", "edits.json"]);
    assert_eq!(s.read("f.txt"), "a\nb\nc\nB\nd\n");
    // The second time the old text is gone, and nothing is written.
    s.refuse(Some("refuse_anchor_not_found"), &["apply", "edits.json"], "anchor-not-found");
    // An anchor that occurs twice is ambiguous.
    s.write("g.txt", "x\nx\n");
    fs::write(
        s.path().join("amb.json"),
        json!([{"path": "g.txt", "old": "x\n", "new": "y\n"}]).to_string(),
    )
    .unwrap();
    s.refuse(None, &["apply", "amb.json"], "anchor-ambiguous");
    assert_eq!(s.read("g.txt"), "x\nx\n");
}

#[test]
fn a_binary_file_is_a_snapshot_and_a_symlink_is_a_link() {
    let s = Sandbox::new();
    s.pin_quiet(&["init"]);
    fs::write(s.path().join("blob.bin"), [0u8, 159, 146, 150, 0, 255]).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("blob.bin", s.path().join("link")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::write(s.path().join("run.sh"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(s.path().join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    s.pin_quiet(&["record", "--all", "--time", "1"]);
    fs::write(s.path().join("blob.bin"), [1u8, 2, 3, 0]).unwrap();
    let d = s.pin_quiet(&["diff"]);
    assert_eq!(d["files"][0]["binary"], json!(true));
    s.pin_quiet(&["restore", "."]);
    assert_eq!(fs::read(s.path().join("blob.bin")).unwrap(), vec![0u8, 159, 146, 150, 0, 255]);
    #[cfg(unix)]
    {
        assert_eq!(fs::read_link(s.path().join("link")).unwrap(), PathBuf::from("blob.bin"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(s.path().join("run.sh")).unwrap().permissions().mode() & 0o111,
            0o111
        );
    }
    assert!(s.pin_quiet(&["status"])["clean"].as_bool().unwrap());
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

/// `gc` (`docs/vcs-cli.md` R4): every object moves into one pack, the store
/// reads the same before and after, a second `gc` does nothing, and a kill
/// at every write of a `gc` leaves a store `verify` calls sound that a
/// re-run packs to the same pack.
#[test]
fn gc_packs_the_store_and_survives_a_kill_at_every_write() {
    let s = two_changes();
    s.pin_quiet(&["record", "--all", "--intent-prompt", "second", "--time", "1001"]);
    let log_before = s.pin_quiet(&["log"]);
    let verify_before = s.pin_quiet(&["verify"]);
    let dry = s.pin_quiet(&["gc", "--dry-run"]);
    assert_eq!(dry["dry_run"], json!(true));
    assert!(s.path().join(".cancho/vcs/objects/blob").read_dir().unwrap().next().is_some());
    let base = Sandbox::new();
    copy_dir(s.path(), base.path());

    let done = s.pin("gc", &["gc"]);
    let pack = done["pack"].clone();
    assert!(pack.is_string());
    assert_eq!(done["removed_loose"], done["loose_before"]);
    assert_eq!(s.pin_quiet(&["log"]), log_before, "the same history, read from the pack");
    assert_eq!(s.pin_quiet(&["verify"])["objects"], verify_before["objects"]);
    assert!(s.pin_quiet(&["status"])["clean"].as_bool().unwrap());
    let again = s.pin_quiet(&["gc"]);
    assert_eq!((again["pack"].clone(), again["removed_loose"].clone()), (pack.clone(), json!(0)));
    // A record after gc writes loose objects beside the pack; both are read.
    s.write("notes.md", "after gc\n");
    s.pin_quiet(&["record", "--all", "--intent-prompt", "after", "--time", "1002"]);
    assert_eq!(s.pin_quiet(&["verify"])["sound"], json!(true));
    assert_eq!(s.pin_quiet(&["show", "HEAD~1"])["change"], log_before["changes"][0]["change"]);

    let mut killed = 0;
    for n in 1..500 {
        let work = Sandbox::new();
        copy_dir(base.path(), work.path());
        let (code, _, _) = run_in(work.path(), &["gc"], &[("CANCHO_VCS_CRASH_AT", &n.to_string())]);
        if code != 137 {
            assert_eq!(code, 0, "gc at n={n}");
            break;
        }
        killed += 1;
        work.pin_quiet(&["unlock"]);
        let v = work.pin_quiet(&["verify"]);
        assert_eq!(v["sound"], json!(true), "gc killed at {n}: {v}");
        assert_eq!(v["objects"], verify_before["objects"], "gc killed at {n}");
        assert_eq!(work.pin_quiet(&["log"]), log_before, "gc killed at {n}");
        let r = work.pin_quiet(&["gc"]);
        assert_eq!(r["pack"], pack, "gc killed at {n}, run again");
        assert_eq!(work.pin_quiet(&["verify"])["sound"], json!(true));
        assert_eq!(work.pin_quiet(&["gc"])["removed_loose"], json!(0), "idempotent");
    }
    assert!(killed >= 10, "gc has at least 10 write points, saw {killed}");
}

/// Two agents move one ref from the same base at the same moment.
#[test]
fn two_writers_racing_one_ref_have_one_winner() {
    for round in 0..8 {
        let s = Sandbox::new();
        s.pin_quiet(&["init"]);
        s.write("f.txt", "base\n");
        let base = s.change(&["record", "--all", "--time", "1"]);
        s.write("f.txt", "edit\n");
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let dir = s.path().to_path_buf();
                let base = base.clone();
                std::thread::spawn(move || {
                    let prompt = format!("writer {i}");
                    let t = (10 + i).to_string();
                    run_in(
                        &dir,
                        &[
                            "record",
                            "--all",
                            "--expect",
                            &base,
                            "--intent-prompt",
                            &prompt,
                            "--time",
                            &t,
                        ],
                        &[],
                    )
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners: Vec<_> = results.iter().filter(|r| r.0 == 0).collect();
        // The loser's reply is either the ref check or the lock; never success, never silence.
        for (code, doc, _) in results.iter().filter(|r| r.0 != 0) {
            assert_eq!(*code, 1);
            let tag = doc["refused"]["tag"].as_str().unwrap();
            assert!(["ref-moved", "lock-held"].contains(&tag), "round {round}: {doc}");
        }
        // Writers that read the tree before the winner moved it see "no change"
        // (recorded: false) only after the winner; at least one change landed.
        assert!(!winners.is_empty(), "round {round}: nobody won");
        let won: Vec<_> = winners.iter().filter(|w| w.1["recorded"] == json!(true)).collect();
        assert_eq!(
            won.len(),
            1,
            "round {round}: exactly one writer moves the ref from the base: {results:?}"
        );
        assert_eq!(s.pin_quiet(&["verify"])["sound"], json!(true));
        let log = s.pin_quiet(&["log"]);
        assert_eq!(
            log["changes"].as_array().unwrap().len(),
            2,
            "round {round}: base and the winner"
        );
    }
}

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Gita")
        .env("GIT_AUTHOR_EMAIL", "g@example.com")
        .env("GIT_COMMITTER_NAME", "Gita")
        .env("GIT_COMMITTER_EMAIL", "g@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The fixture is built with git (a test may; the product may not): loose
/// objects, a merge, an executable, a symlink, a non-UTF-8 message, an
/// annotated tag, and then `git gc` so the history is in a pack with deltas.
fn git_fixture(gc: bool) -> Sandbox {
    let g = Sandbox::new();
    let d = g.path();
    git(d, &["init", "-q", "-b", "main"]);
    let commit = |msg: &str, when: &str| {
        git(d, &["add", "-A"]);
        let out = Command::new("git")
            .args(["commit", "-q", "--allow-empty", "-m", msg])
            .current_dir(d)
            .env("GIT_AUTHOR_NAME", "Gita")
            .env("GIT_AUTHOR_EMAIL", "g@example.com")
            .env("GIT_COMMITTER_NAME", "Gita")
            .env("GIT_COMMITTER_EMAIL", "g@example.com")
            .env("GIT_AUTHOR_DATE", when)
            .env("GIT_COMMITTER_DATE", when)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    };
    let big: String = (0..400).map(|i| format!("line {i}\n")).collect();
    g.write("big.txt", &big);
    g.write("src/a.cho", GOOD);
    g.write("run.sh", "#!/bin/sh\necho hi\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(d.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("big.txt", d.join("link")).unwrap();
    }
    commit("one", "1700000000 +0100");
    for i in 0..6 {
        g.write("big.txt", &big.replace("line 7\n", &format!("line seven {i}\n")));
        commit(&format!("edit {i}"), &format!("{} +0000", 1700000100 + i));
    }
    git(d, &["checkout", "-q", "-b", "side", "HEAD~3"]);
    g.write("side.txt", "side\n");
    commit("side work", "1700000500 -0500");
    git(d, &["checkout", "-q", "main"]);
    g.write("main-only.txt", "m\n");
    commit("main work", "1700000600 +0000");
    git(d, &["merge", "-q", "--no-ff", "-m", "merge side", "side"]);
    git(d, &["tag", "-a", "-m", "release", "v1"]);
    git(d, &["tag", "light"]);
    // A message that is not UTF-8 survives byte for byte.
    g.write("latin.txt", "caf\u{e9}\n");
    git(d, &["add", "-A"]);
    let out = Command::new("git")
        .args(["-c", "i18n.commitEncoding=latin1", "commit", "-q", "-F", "-"])
        .current_dir(d)
        .env("GIT_AUTHOR_NAME", "Gita")
        .env("GIT_AUTHOR_EMAIL", "g@example.com")
        .env("GIT_COMMITTER_NAME", "Gita")
        .env("GIT_COMMITTER_EMAIL", "g@example.com")
        .env("GIT_AUTHOR_DATE", "1700000700 +0000")
        .env("GIT_COMMITTER_DATE", "1700000700 +0000")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(b"caf\xe9 message\n").unwrap();
            c.wait_with_output()
        })
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    if gc {
        git(d, &["gc", "-q", "--aggressive"]);
    }
    g
}

fn round_trip(gc: bool) {
    if !git_available() {
        eprintln!("git is not installed; the round-trip fixture is skipped");
        return;
    }
    let g = git_fixture(gc);
    let work = Sandbox::new();
    let imp = work.run(&["import-git", g.path().to_str().unwrap()]).1;
    assert_eq!(imp["ok"], json!(true), "{imp}");
    assert_eq!(imp["self_check_mismatches"], json!([]));
    assert_eq!(
        imp["commits"],
        json!(git(g.path(), &["rev-list", "--all", "--count"]).trim().parse::<u64>().unwrap())
    );
    // The working copy is the checked-out tree.
    assert_eq!(work.read("src/a.cho"), GOOD);
    assert!(work.pin_quiet(&["status"])["clean"].as_bool().unwrap());
    // Idempotent: a second import adds nothing.
    let again = work.pin_quiet(&["import-git", g.path().to_str().unwrap()]);
    assert_eq!(again["commits"], json!(0));
    let out = TempDir::new().unwrap();
    let exp = work.run(&["export-git", out.path().to_str().unwrap()]).1;
    assert_eq!(exp["ok"], json!(true), "{exp}");
    assert_eq!(exp["mismatches"], json!([]));
    assert_eq!(exp["equal"], exp["compared_with_import"]);
    // The independent check: git's own view of both histories.
    let ids = |d: &Path| {
        let mut v: Vec<String> =
            git(d, &["rev-list", "--all"]).lines().map(str::to_owned).collect();
        v.sort();
        v
    };
    assert_eq!(ids(g.path()), ids(out.path()));
    git(out.path(), &["fsck", "--strict"]);
    assert_eq!(
        git(g.path(), &["rev-parse", "main", "side", "v1^{commit}"]),
        git(out.path(), &["rev-parse", "main", "side", "v1^{commit}"])
    );
    // log and blame open the imported history.
    let log = work.pin_quiet(&["log"]);
    assert_eq!(log["changes"].as_array().unwrap().len(), 11);
    let blame = work.pin_quiet(&["blame", "big.txt"]);
    assert_eq!(blame["lines"].as_array().unwrap().len(), 400);
    assert_eq!(work.pin_quiet(&["verify"])["sound"], json!(true));
}

#[test]
fn import_then_export_reproduces_every_commit_id_from_loose_objects() {
    round_trip(false);
}

#[test]
fn import_then_export_reproduces_every_commit_id_from_a_pack() {
    round_trip(true);
}

#[test]
fn a_directory_that_is_not_git_is_refused_with_its_tag() {
    let work = Sandbox::new();
    let not_git = Sandbox::new();
    work.refuse(None, &["import-git", not_git.path().to_str().unwrap()], "git-unreadable");
}

#[test]
fn the_old_publish_log_is_still_the_log_when_no_repository_is_near() {
    let d = TempDir::new().unwrap();
    let out = Command::new(BIN).args(["vcs", "log"]).current_dir(d.path()).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing published yet"));
}
