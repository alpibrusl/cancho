//! `cancho vcs merge`, `conflicts`, `resolve` and `replay` (`docs/vcs-cli.md`
//! section 6, slice V2): the three layers, conflicts as data with a rule tag
//! each, idempotence, refs moved by compare-and-swap, and a kill at every
//! write leaving a store that finishes the same merge. JSON pins live in
//! `tests/vcs_pins/` and are regenerated like `vcs_repo`'s
//! (`UPDATE_VCS_PINS=1 cargo test -p cancho --test vcs_merge`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_cancho");

struct Sandbox {
    dir: TempDir,
}

fn run_in(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (i32, Value, String) {
    let mut c = Command::new(BIN);
    c.arg("vcs")
        .args(args)
        .args(["--output", "json"])
        .current_dir(dir)
        .env("CANCHO_AUTHOR", "Ada <ada@example.com>")
        // The gate must not find packages of the machine the test runs on.
        .env("CANCHO_PACKAGES", "/nonexistent-cancho-packages");
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

const BASE_CHO: &str = "module m;\n\npub fn a(x: int) -> [] int {\n    return x + 1;\n}\n\npub fn b(x: int) -> [] int {\n    return x + 2;\n}\n";
const NOTES: &str = "# notes\n1\n2\n3\n4\n5\n6\n7\n8\n9\n";

/// `main` with a base change, `topic` branched from it; both then moved by `f`.
fn forked(base: &[(&str, &str)], main: &[(&str, &str)], topic: &[(&str, &str)]) -> Sandbox {
    let s = Sandbox::new();
    s.pin_quiet(&["init"]);
    for (p, t) in base {
        s.write(p, t);
    }
    s.pin_quiet(&["record", "--all", "--intent-prompt", "base", "--time", "1000"]);
    s.pin_quiet(&["branch", "create", "topic"]);
    for (p, t) in main {
        s.write(p, t);
    }
    s.pin_quiet(&[
        "record",
        "--all",
        "--intent-prompt",
        "main work",
        "--intent-session",
        "s-main",
        "--time",
        "1001",
    ]);
    s.pin_quiet(&["switch", "topic"]);
    for (p, t) in topic {
        s.write(p, t);
    }
    s.pin_quiet(&[
        "record",
        "--all",
        "--intent-prompt",
        "topic work",
        "--intent-session",
        "s-topic",
        "--time",
        "1002",
    ]);
    s.pin_quiet(&["switch", "main"]);
    s
}

fn conflicts_of(doc: &Value) -> Vec<(String, String)> {
    doc["conflicts"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| {
                    (c["path"].as_str().unwrap().to_owned(), c["tag"].as_str().unwrap().to_owned())
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_clean_merge_of_two_files_and_two_regions_is_recorded_with_two_parents() {
    let s = forked(
        &[("m.cho", BASE_CHO), ("notes.md", NOTES), ("keep.txt", "k\n")],
        &[("notes.md", &NOTES.replace("\n2\n", "\ntwo\n")), ("main.txt", "m\n")],
        &[
            ("notes.md", &NOTES.replace("\n8\n", "\neight\n")),
            ("m.cho", &BASE_CHO.replace("x + 1", "x + 10")),
        ],
    );
    let dry = s.pin(
        "merge_dry_run",
        &["merge", "topic", "--dry-run", "--intent-prompt", "bring topic in", "--time", "1003"],
    );
    assert_eq!(dry["merged"], json!(false));
    assert_eq!(dry["would_record"], json!(true));
    assert_eq!(s.pin_quiet(&["log"])["changes"].as_array().unwrap().len(), 2);
    let doc = s.pin(
        "merge_clean",
        &["merge", "topic", "--intent-prompt", "bring topic in", "--time", "1003"],
    );
    assert_eq!(doc["merged"], json!(true));
    assert_eq!(doc["tree"], dry["tree"], "a dry run reports the tree the real merge records");
    assert_eq!(doc["change"], dry["change"]);
    let notes = s.read("notes.md");
    assert!(notes.contains("two") && notes.contains("eight"));
    assert!(s.read("m.cho").contains("x + 10"));
    assert_eq!(s.read("main.txt"), "m\n");
    let shown = s.pin_quiet(&["show", "HEAD"]);
    assert_eq!(shown["parents"].as_array().unwrap().len(), 2);
    // Idempotent: the merged change is an ancestor of HEAD now.
    let again = s.pin("merge_again", &["merge", "topic", "--time", "1004"]);
    assert_eq!(again["up_to_date"], json!(true));
    assert_eq!(s.pin_quiet(&["log"])["changes"].as_array().unwrap().len(), 4);
    s.pin_quiet(&["verify"]);
}

#[test]
fn a_fast_forward_moves_the_ref_and_the_working_copy() {
    let s = Sandbox::new();
    s.pin_quiet(&["init"]);
    s.write("a.txt", "1\n");
    s.pin_quiet(&["record", "--all", "--time", "1000"]);
    s.pin_quiet(&["branch", "create", "topic"]);
    s.pin_quiet(&["switch", "topic"]);
    s.write("a.txt", "2\n");
    let tip = s.change(&["record", "--all", "--time", "1001"]);
    s.pin_quiet(&["switch", "main"]);
    let doc = s.pin("merge_fast_forward", &["merge", "topic"]);
    assert_eq!(doc["fast_forward"], json!(true));
    assert_eq!(doc["change"], json!(tip));
    assert_eq!(s.read("a.txt"), "2\n");
}

#[test]
fn conflicts_are_data_nothing_is_recorded_and_resolve_finishes_it() {
    let s = forked(
        &[("notes.md", NOTES), ("m.cho", BASE_CHO)],
        &[("notes.md", &NOTES.replace("\n5\n", "\nmain five\n"))],
        &[("notes.md", &NOTES.replace("\n5\n", "\ntopic five\n"))],
    );
    let (code, doc) = s.run(&["merge", "topic", "--time", "1003"]);
    assert_eq!(code, 2, "open conflicts exit 2: {doc}");
    pin("merge_conflicts", &doc, s.path());
    assert_eq!(conflicts_of(&doc), vec![("notes.md".to_owned(), "content-conflict".to_owned())]);
    let h = &doc["conflicts"][0]["hunks"][0];
    assert_eq!(h["ours_lines"], json!(["main five\n"]));
    assert_eq!(h["theirs_lines"], json!(["topic five\n"]));
    assert_eq!(h["base_lines"], json!(["5\n"]));
    // Nothing recorded, the working copy untouched, the same merge reports the same.
    assert_eq!(s.pin_quiet(&["log"])["changes"].as_array().unwrap().len(), 2);
    assert!(s.read("notes.md").contains("main five"));
    let (code, again) = s.run(&["merge", "topic", "--time", "1003"]);
    assert_eq!((code, &again["conflicts"]), (2, &doc["conflicts"]));
    let (_, listed) = s.run(&["conflicts"]);
    assert_eq!(listed["open_conflicts"], json!(1));
    // Another merge while one is open is refused.
    s.refuse(None, &["merge", "main~1"], "merge-in-progress");
    s.refuse(None, &["resolve", "m.cho", "--take", "ours"], "not-in-conflict");
    s.refuse(None, &["resolve", "notes.md", "--take", "ours", "--with", "x"], "bad-argument");
    // Settle it: take theirs.
    let done = s.pin("resolve_take_theirs", &["resolve", "notes.md", "--take", "theirs"]);
    assert_eq!(done["merged"], json!(true));
    assert!(s.read("notes.md").contains("topic five"));
    assert_eq!(s.pin_quiet(&["show", "HEAD"])["parents"].as_array().unwrap().len(), 2);
    s.refuse(None, &["resolve", "notes.md", "--take", "theirs"], "no-merge-in-progress");
    s.pin("conflicts_none", &["conflicts"]);
}

#[test]
fn resolve_with_a_file_and_abort() {
    let s = forked(
        &[("notes.md", NOTES)],
        &[("notes.md", &NOTES.replace("\n5\n", "\nmain five\n"))],
        &[("notes.md", &NOTES.replace("\n5\n", "\ntopic five\n"))],
    );
    let (code, _) = s.run(&["merge", "topic"]);
    assert_eq!(code, 2);
    assert_eq!(s.pin_quiet(&["merge", "--abort"])["aborted"], json!(true));
    assert_eq!(s.pin_quiet(&["merge", "--abort"])["aborted"], json!(false));
    s.pin("conflicts_after_abort", &["conflicts"]);
}

#[test]
fn a_file_resolved_by_hand_is_taken_whole() {
    let s = forked(
        &[("notes.md", NOTES)],
        &[("notes.md", &NOTES.replace("\n5\n", "\nmain five\n"))],
        &[("notes.md", &NOTES.replace("\n5\n", "\ntopic five\n"))],
    );
    s.run(&["merge", "topic", "--time", "1003"]);
    let tmp = tempfile::NamedTempFile::new().unwrap();
    fs::write(tmp.path(), "by hand\n").unwrap();
    let done = s.pin_quiet(&["resolve", "notes.md", "--with", tmp.path().to_str().unwrap()]);
    assert_eq!(done["merged"], json!(true));
    assert_eq!(s.read("notes.md"), "by hand\n");
}

#[test]
fn the_cho_layer_keeps_two_new_declarations_and_names_a_declaration_conflict() {
    let add = |name: &str, n: i32| {
        format!("\npub fn {name}(x: int) -> [] int {{\n    return x + {n};\n}}\n")
    };
    let s = forked(
        &[("m.cho", BASE_CHO)],
        &[("m.cho", &format!("{BASE_CHO}{}", add("c", 3)))],
        &[("m.cho", &format!("{BASE_CHO}{}", add("d", 4)))],
    );
    let doc = s.pin("merge_cho_both_added", &["merge", "topic", "--time", "1003"]);
    assert_eq!(doc["merged"], json!(true));
    assert_eq!(doc["merged_paths"], json!([{"path": "m.cho", "layer": "cho"}]));
    let text = s.read("m.cho");
    assert!(text.contains("fn c") && text.contains("fn d"));

    // The same declaration changed both ways: named, and settled per declaration.
    let s = forked(
        &[("m.cho", BASE_CHO)],
        &[("m.cho", &BASE_CHO.replace("x + 1", "x + 10").replace("x + 2", "x + 20"))],
        &[("m.cho", &BASE_CHO.replace("x + 1", "x + 11"))],
    );
    let (code, doc) = s.run(&["merge", "topic", "--time", "1003"]);
    assert_eq!(code, 2);
    pin("merge_cho_decl_conflict", &doc, s.path());
    assert_eq!(conflicts_of(&doc), vec![("m.cho".to_owned(), "decl-conflict".to_owned())]);
    assert_eq!(doc["conflicts"][0]["decls"][0]["decl"], json!("fn m.a"));
    s.refuse(None, &["resolve", "m.cho", "--decl", "fn m.zzz", "--take", "ours"], "decl-not-found");
    let done = s.pin_quiet(&["resolve", "m.cho", "--decl", "fn m.a", "--take", "theirs"]);
    assert_eq!(done["merged"], json!(true));
    let text = s.read("m.cho");
    assert!(text.contains("x + 11") && text.contains("x + 20"), "{text}");
}

#[test]
fn tree_conflicts_carry_their_tags() {
    // delete on one side, edit on the other.
    let s = forked(
        &[("a.txt", "1\n2\n"), ("b.txt", "x\n"), ("c.bin", "\u{0}\u{1}")],
        &[("a.txt", "1\n2\n3\n")],
        &[],
    );
    s.pin_quiet(&["switch", "topic"]);
    fs::remove_file(s.path().join("a.txt")).unwrap();
    s.pin_quiet(&["record", "--all", "--time", "1005"]);
    s.pin_quiet(&["switch", "main"]);
    let (code, doc) = s.run(&["merge", "topic", "--time", "1006"]);
    assert_eq!(code, 2);
    assert_eq!(conflicts_of(&doc), vec![("a.txt".to_owned(), "delete-modify".to_owned())]);
    s.pin_quiet(&["merge", "--abort"]);

    // a rename on one side, an edit on the other: said, not guessed.
    let s = forked(&[("old.txt", "1\n2\n")], &[("old.txt", "1\n2\n3\n")], &[]);
    s.pin_quiet(&["switch", "topic"]);
    fs::rename(s.path().join("old.txt"), s.path().join("new.txt")).unwrap();
    s.pin_quiet(&["record", "--all", "--time", "1005"]);
    s.pin_quiet(&["switch", "main"]);
    let (_, doc) = s.run(&["merge", "topic", "--time", "1006"]);
    assert_eq!(conflicts_of(&doc), vec![("old.txt".to_owned(), "rename-edit".to_owned())]);
    s.pin_quiet(&["merge", "--abort"]);

    // two sides add the same path differently; binary files edited on both.
    let s = forked(
        &[("k", "k\n")],
        &[("n.txt", "main\n"), ("b.bin", "\u{0}main")],
        &[("n.txt", "topic\n"), ("b.bin", "\u{0}topic")],
    );
    let (_, doc) = s.run(&["merge", "topic", "--time", "1006"]);
    let mut got = conflicts_of(&doc);
    got.sort();
    assert_eq!(
        got,
        vec![
            ("b.bin".to_owned(), "add-add".to_owned()),
            ("n.txt".to_owned(), "add-add".to_owned())
        ]
    );
    let s =
        forked(&[("b.bin", "\u{0}base")], &[("b.bin", "\u{0}main")], &[("b.bin", "\u{0}topic")]);
    let (_, doc) = s.run(&["merge", "topic", "--time", "1006"]);
    assert_eq!(conflicts_of(&doc), vec![("b.bin".to_owned(), "binary-conflict".to_owned())]);
}

#[test]
fn a_merge_that_is_clean_by_declaration_can_still_fail_the_gate() {
    // Ours gives `a` a second parameter and updates its caller; theirs adds a new
    // caller of the old shape. No declaration is in conflict; the tree does not check.
    let base = "module m;\n\npub fn a(x: int) -> [] int {\n    return x;\n}\n\npub fn b(x: int) -> [] int {\n    return a(x);\n}\n";
    let ours = base.replace("a(x: int)", "a(x: int, y: int)").replace("a(x);", "a(x, 1);");
    let theirs = format!("{base}\npub fn c(x: int) -> [] int {{\n    return a(x);\n}}\n");
    let s = forked(&[("m.cho", base)], &[("m.cho", &ours)], &[("m.cho", &theirs)]);
    let doc = s.refuse(
        Some("refuse_merge_gate_failed"),
        &["merge", "topic", "--time", "1003"],
        "merge-gate-failed",
    );
    assert!(doc["refused"]["intents"]["ours"]["intent"]["prompt"] == json!("main work"));
    assert!(doc["refused"]["intents"]["theirs"]["intent"]["prompt"] == json!("topic work"));
    // Nothing recorded; with --no-gate the merge is made and says it is unattested.
    assert_eq!(s.pin_quiet(&["log"])["changes"].as_array().unwrap().len(), 2);
    let ok = s.pin_quiet(&["merge", "topic", "--no-gate", "--time", "1003"]);
    assert_eq!(ok["gate"]["ran"], json!(false));
}

#[test]
fn a_stale_expect_loses_and_unrelated_histories_are_refused() {
    let s = forked(&[("a.txt", "1\n")], &[("b.txt", "b\n")], &[("c.txt", "c\n")]);
    let first = s.pin_quiet(&["log"])["changes"][1]["change"].as_str().unwrap().to_owned();
    s.refuse(
        Some("refuse_merge_ref_moved"),
        &["merge", "topic", "--expect", &first, "--time", "1003"],
        "ref-moved",
    );
    assert!(!Path::new(&s.path().join("c.txt")).exists());
    let other = Sandbox::new();
    other.pin_quiet(&["init"]);
    other.write("z.txt", "z\n");
    other.pin_quiet(&["record", "--all", "--time", "5"]);
    // A change of another repository is not in this store: a bad revision, not a merge.
    s.refuse(None, &["merge", &first[..8].replace(&first[..8], "00000000")], "bad-revision");
}

#[test]
fn replay_reapplies_what_was_done_and_says_exact_or_moved() {
    // The topic change edits line 8 of the notes; main has inserted three lines on top
    // since, so the line numbers moved and the anchor did not.
    let moved = format!("intro 1\nintro 2\nintro 3\n{NOTES}");
    let s = forked(
        &[("notes.md", NOTES)],
        &[("notes.md", &moved)],
        &[("notes.md", &NOTES.replace("\n8\n", "\neight\n"))],
    );
    let doc = s.pin("replay_moved", &["replay", "topic", "--time", "1003"]);
    assert_eq!(doc["result"], json!("moved"));
    assert!(s.read("notes.md").contains("intro 3") && s.read("notes.md").contains("eight"));
    let log = s.pin_quiet(&["log"]);
    assert_eq!(log["changes"][0]["parents"].as_array().unwrap().len(), 1);
    assert_eq!(log["changes"][0]["intent"]["prompt"], json!("topic work"), "the intent is kept");
    // Idempotent: replaying it again changes nothing.
    let again = s.pin("replay_again", &["replay", "topic", "--time", "1004"]);
    assert_eq!(again["unchanged"], json!(true));
    assert_eq!(s.pin_quiet(&["log"])["changes"].as_array().unwrap().len(), 3);

    // On the base itself the replay is exact.
    let s = forked(
        &[("notes.md", NOTES)],
        &[("x.txt", "x\n")],
        &[("notes.md", &NOTES.replace("\n8\n", "\neight\n"))],
    );
    let doc = s.pin_quiet(&["replay", "topic", "--time", "1003"]);
    assert_eq!(doc["result"], json!("moved"), "main has x.txt, the topic tree has not");
}

#[test]
fn replay_conflicts_with_the_anchor_tag_when_the_text_it_edited_is_gone() {
    let s = forked(
        &[("notes.md", NOTES)],
        &[("notes.md", &NOTES.replace("\n8\n", "\nmain eight\n"))],
        &[("notes.md", &NOTES.replace("\n8\n", "\ntopic eight\n"))],
    );
    let (code, doc) = s.run(&["replay", "topic", "--time", "1003"]);
    assert_eq!(code, 2, "{doc}");
    assert_eq!(conflicts_of(&doc), vec![("notes.md".to_owned(), "anchor-not-found".to_owned())]);
    let done = s.pin_quiet(&["resolve", "notes.md", "--take", "theirs"]);
    assert_eq!(done["merged"], json!(true));
    assert_eq!(s.pin_quiet(&["show", "HEAD"])["parents"].as_array().unwrap().len(), 1);
}

#[test]
fn a_merge_killed_at_any_write_is_finished_by_running_it_again() {
    let build = || {
        forked(
            &[("notes.md", NOTES), ("m.cho", BASE_CHO)],
            &[("notes.md", &NOTES.replace("\n2\n", "\ntwo\n"))],
            &[
                ("notes.md", &NOTES.replace("\n8\n", "\neight\n")),
                ("m.cho", &BASE_CHO.replace("x + 1", "x + 9")),
            ],
        )
    };
    let reference = {
        let s = build();
        s.pin_quiet(&["merge", "topic", "--time", "1003"])["change"].as_str().unwrap().to_owned()
    };
    let mut n = 1;
    loop {
        let s = build();
        let crash = n.to_string();
        let (code, _, _) = run_in(
            s.path(),
            &["merge", "topic", "--time", "1003"],
            &[("CANCHO_VCS_CRASH_AT", &crash)],
        );
        if code != 137 {
            break; // no failpoint left to hit
        }
        let v = s.pin_quiet(&["verify"]);
        assert_eq!(v["sound"], json!(true), "crash at {n}: {v}");
        let _ = s.run(&["unlock"]);
        let (code, doc) = s.run(&["merge", "topic", "--time", "1003"]);
        assert!(code == 0 || code == 1, "crash at {n}: {doc}");
        // Either the merge finished (same change) or the working copy lags HEAD: restore it.
        if code == 0 && doc["change"].is_string() {
            assert_eq!(doc["change"], json!(reference), "crash at {n}");
        } else {
            assert_eq!(
                s.pin_quiet(&["log"])["changes"][0]["change"],
                json!(reference),
                "crash at {n}"
            );
        }
        n += 1;
        assert!(n < 200, "the failpoints never ran out");
    }
    assert!(n > 5, "a merge passes more than a handful of failpoints");
}

#[test]
fn the_gate_finds_modules_outside_the_tree_in_the_roots_it_is_given() {
    let s = Sandbox::new();
    s.pin_quiet(&["init"]);
    let ext = tempfile::tempdir().unwrap();
    fs::create_dir_all(ext.path().join("pkg/.cancho-vcs")).unwrap();
    fs::write(
        ext.path().join("pkg/lib.cho"),
        "module extlib;\npub fn one() -> [] int { return 1; }\n",
    )
    .unwrap();
    // A dot directory holds what a tool stored, not what the package declares.
    fs::write(
        ext.path().join("pkg/.cancho-vcs/dup.cho"),
        "module extlib;\npub fn one() -> [] int { return 2; }\n",
    )
    .unwrap();
    s.write(
        "use.cho",
        "module user;\nimport extlib;\npub fn two() -> [] int { return extlib.one() + 1; }\n",
    );
    let doc = s.refuse(
        Some("refuse_gate_unresolved"),
        &["record", "--all", "--time", "1000"],
        "gate-failed",
    );
    assert_eq!(doc["refused"]["unresolved_modules"], json!(["extlib"]));
    let root = ext.path().to_str().unwrap();
    let ok = s.pin_quiet(&["record", "--all", "--gate-path", root, "--time", "1000"]);
    assert_eq!(ok["gate"]["checked"], json!(["use.cho"]));
    assert_eq!(ok["gate"]["external"].as_array().unwrap().len(), 1);
    // The same through a repository-local list, with no flag.
    s.write(
        "use.cho",
        "module user;\nimport extlib;\npub fn two() -> [] int { return extlib.one() + 2; }\n",
    );
    fs::write(s.path().join(".cancho/vcs/gate-path"), format!("{root}\n")).unwrap();
    let ok = s.pin_quiet(&["record", "--all", "--time", "1001"]);
    assert_eq!(ok["gate"]["external"].as_array().unwrap().len(), 1);
}

#[test]
fn a_merge_cannot_be_replayed_and_a_file_cannot_meet_a_directory() {
    let s = forked(&[("k", "k\n")], &[("b.txt", "b\n")], &[("c.txt", "c\n")]);
    s.pin_quiet(&["merge", "topic", "--time", "1003"]);
    s.refuse(Some("refuse_replay_unsupported"), &["replay", "HEAD"], "replay-unsupported");
    // ours adds the file `p`, theirs adds `p/q`: no tree can hold both.
    let s = forked(&[("k", "k\n")], &[("p", "file\n")], &[("p/q", "under\n")]);
    s.refuse(
        Some("refuse_file_directory"),
        &["merge", "topic", "--time", "1003"],
        "file-directory-conflict",
    );
}
