//! The `vcs` commands in cancho (`examples/selfhost/clivcs.cho`, `docs/cli-in-cancho.md` section 28), against `cancho vcs`.

use super::*;

/// What a command said: its status, its standard output, and the first line of its standard error.
type Said = (Option<i32>, Vec<u8>, String);

fn said(done: std::process::Output) -> Said {
    let first = String::from_utf8_lossy(&done.stderr).lines().next().unwrap_or("").to_owned();
    (done.status.code(), done.stdout, first)
}

fn rust(dir: &Path, args: &[&str]) -> Said {
    said(Command::new(BIN).current_dir(dir).arg("vcs").args(args).output().expect("cancho runs"))
}

fn mine(exe: &Path, dir: &Path, args: &[&str]) -> Said {
    said(Command::new(exe).current_dir(dir).arg("vcs").args(args).output().expect("cli runs"))
}

/// A store with a manifest made of `entries`: `(sig, name, uses_std)`, where `uses_std` is left out of the file when it is `None`.
fn store_with(dir: &Path, entries: &[(&str, &str, Option<bool>)]) {
    std::fs::create_dir_all(dir).unwrap();
    let mut body = String::from("{\n  \"entries\": {");
    for (n, (sig, name, std)) in entries.iter().enumerate() {
        body.push_str(if n == 0 { "\n" } else { ",\n" });
        let flag = match std {
            Some(flag) => format!(",\n      \"uses_std\": {flag}"),
            None => String::new(),
        };
        body.push_str(&format!(
            "    \"{sig}\": {{\n      \"name\": \"{name}\",\n      \"stage_id\": \"{}\",\n      \"source_hash\": \"{}\"{flag}\n    }}",
            "b".repeat(64),
            "c".repeat(64)
        ));
    }
    body.push_str(if entries.is_empty() { "}\n}" } else { "\n  }\n}" });
    std::fs::write(dir.join("manifest.json"), body).unwrap();
}

/// `vcs log` on every store of `packages/` (the 26, and the stores inside the ones that hold several), and on stores made for what they do not have: a name given twice, one
/// longer than the 24 columns, one of two bytes to a character, a `uses_std` that is false and one that is left out, no entries, no manifest, a manifest that is not JSON and
/// one whose fields are not text. The status and the standard output are compared, and the first line of the standard error where it is not an error in the manifest, whose
/// words are the JSON reader's.
#[test]
fn vcs_log_in_cancho_lists_a_store_like_cancho_vcs_log() {
    let exe = build("cli-vcs-log", &with_front_end("cli.cho"));
    let work = scratch("selfhost-vcs-log");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages")
        .canonicalize()
        .expect("packages/ is there");
    let mut stores = Vec::new();
    for entry in std::fs::read_dir(&packages).unwrap() {
        let root = entry.unwrap().path().join(".cancho-vcs");
        if root.is_dir() {
            stores_of(&root, &mut stores);
        }
    }
    assert!(stores.len() >= 26, "only {} stores in packages/", stores.len());
    let (mut std_rows, mut rows) = (0, 0);
    let mut different = Vec::new();
    for store in &stores {
        let path = store.to_str().unwrap();
        let theirs = rust(&work, &["log", "--store", path]);
        let ours = mine(&exe, &work, &["log", "--store", path]);
        let text = String::from_utf8_lossy(&theirs.1);
        rows += text.lines().count();
        std_rows += text.lines().filter(|l| l.ends_with("  std")).count();
        if theirs != ours {
            different.push(path.to_owned());
        }
    }
    assert!(different.is_empty(), "`vcs log` differs for {different:?}");
    assert!(rows > 900 && std_rows > 100, "{rows} rows, {std_rows} with std");
    let sig = |c: char| c.to_string().repeat(64);
    let made = work.join("made");
    store_with(
        &made.join("names"),
        &[
            (&sig('f'), "zeta", Some(true)),
            (&sig('1'), "same", None),
            (&sig('2'), "same", Some(true)),
            (&sig('3'), "a_name_that_is_longer_than_twenty_four_bytes", Some(false)),
            (&sig('0'), "Alpha", None),
            (&sig('e'), "é", None),
            (&sig('d'), "ü_and_more", Some(true)),
        ],
    );
    store_with(&made.join("none"), &[]);
    std::fs::create_dir_all(made.join("text")).unwrap();
    std::fs::write(made.join("text/manifest.json"), "{\n").unwrap();
    std::fs::create_dir_all(made.join("wrong")).unwrap();
    std::fs::write(made.join("wrong/manifest.json"), "{\"entries\":{\"a\":{\"name\":1}}}").unwrap();
    std::fs::create_dir_all(made.join("empty")).unwrap();
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("names", vec!["log", "--store", "made/names"]),
        ("none", vec!["log", "--store", "made/none"]),
        ("no manifest", vec!["log", "--store", "made/empty"]),
        ("no directory", vec!["log", "--store", "made/absent"]),
        ("default store", vec!["log"]),
        ("--std", vec!["log", "--std", "--store", "made/names"]),
        ("--dir", vec!["log", "--dir", "x", "--store", "made/names"]),
        ("--requires", vec!["log", "--requires", "a:b", "--store", "made/names"]),
        ("store last", vec!["log", "--store"]),
        ("dir last", vec!["log", "--dir"]),
        ("requires last", vec!["log", "--requires"]),
        ("a file", vec!["log", "x.cho"]),
        ("an option", vec!["log", "--bogus"]),
    ];
    let mut refused = Vec::new();
    for (name, args) in &cases {
        if rust(&work, args) != mine(&exe, &work, args) {
            refused.push(*name);
        }
    }
    assert!(refused.is_empty(), "`vcs log` differs for {refused:?}");
    for bad in ["made/text", "made/wrong"] {
        let theirs = rust(&work, &["log", "--store", bad]);
        let ours = mine(&exe, &work, &["log", "--store", bad]);
        assert_eq!((theirs.0, &theirs.1), (ours.0, &ours.1), "{bad}");
        assert_eq!(theirs.0, Some(3), "{bad}");
        let prefix = format!("cannot read manifest at `{bad}`: malformed manifest");
        assert!(
            theirs.2.starts_with(&prefix) && ours.2.starts_with(&prefix),
            "{bad}: {theirs:?} {ours:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&work);
}

/// The directories of `root` that hold a manifest, as absolute paths.
fn stores_of(root: &Path, into: &mut Vec<PathBuf>) {
    let mut found = Vec::new();
    super::store::stores_in(root, root, &mut found);
    for relative in found {
        into.push(PathBuf::from(root.join(relative).to_str().unwrap().trim_end_matches('/')));
    }
}
