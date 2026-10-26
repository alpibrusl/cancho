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

/// Every file beneath `dir` with its bytes, by path relative to `dir`.
fn tree(dir: &Path, at: &Path, into: &mut Vec<(String, Vec<u8>)>) {
    let Ok(read) = std::fs::read_dir(at) else { return };
    let mut paths: Vec<PathBuf> = read.map(|e| e.unwrap().path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            tree(dir, &path, into);
        } else {
            into.push((
                path.strip_prefix(dir).unwrap().display().to_string(),
                std::fs::read(&path).unwrap(),
            ));
        }
    }
}

/// One `vcs lock` case: the files it starts with (relative to its directory), and its arguments (those after `lock`).
struct Lock {
    name: &'static str,
    files: Vec<(&'static str, String)>,
    args: Vec<String>,
}

fn manifest_text(entries: &[(&str, &str)]) -> String {
    let mut body = String::from("{\n  \"entries\": {");
    for (n, (sig, name)) in entries.iter().enumerate() {
        body.push_str(if n == 0 { "\n" } else { ",\n" });
        body.push_str(&format!(
            "    \"{sig}\": {{\n      \"name\": {name},\n      \"stage_id\": \"{}\",\n      \"source_hash\": \"{}\"\n    }}",
            "b".repeat(64),
            "c".repeat(64)
        ));
    }
    body.push_str(if entries.is_empty() { "}\n}" } else { "\n  }\n}" });
    body
}

/// `vcs lock` run by both commands in two fresh directories made alike: the status, the standard output, the first line of the standard error, and every file left.
fn lock_both(
    exe: &Path,
    work: &Path,
    case: &Lock,
    cache: &Path,
    rust_cache: &Path,
) -> Result<(), String> {
    let mut results = Vec::new();
    for (who, dir) in [("rust", work.join("rust")), ("cho", work.join("cho"))] {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (path, text) in &case.files {
            let at = dir.join(path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(at, text).unwrap();
        }
        let said = if who == "rust" {
            said(
                Command::new(BIN)
                    .current_dir(&dir)
                    .env("CANCHO_CACHE", rust_cache)
                    .args(["vcs", "lock"])
                    .args(&case.args)
                    .output()
                    .expect("cancho runs"),
            )
        } else {
            let mut args: Vec<String> = case.args.clone();
            if args.iter().any(|a| a == "--git") && args.last().is_some_and(|a| a != "--git") {
                args.push("--cache".to_owned());
                args.push(cache.display().to_string());
            }
            said(
                Command::new(exe)
                    .current_dir(&dir)
                    .args(["vcs", "lock"])
                    .args(&args)
                    .output()
                    .expect("cli runs"),
            )
        };
        let mut files = Vec::new();
        tree(&dir, &dir, &mut files);
        // The words after the path of a file that could not be read are the JSON reader's, or the operating system's.
        let mut said = said;
        if said.2.starts_with("cannot read ") {
            let ticks: Vec<usize> = said.2.match_indices('`').map(|(i, _)| i).collect();
            if let Some(end) = ticks.get(1) {
                said.2.truncate(end + 1);
            }
        }
        results.push((said, files));
    }
    if results[0] == results[1] {
        Ok(())
    } else {
        Err(format!("{}: rust {:?}, cho {:?}", case.name, results[0], results[1]))
    }
}

/// `vcs lock` by cancho and by Rust: the stores of `packages/` pinned whole and by name, an existing lock extended and refused, the stores that are not there or not what they should be, a store
/// in a git repository (by `--rev` and by `--ref`), and every command line the Rust command refuses.
#[test]
fn vcs_lock_in_cancho_pins_like_cancho_vcs_lock() {
    let exe = build("cli-vcs-lock", &with_front_end("cli.cho"));
    let work = scratch("selfhost-vcs-lock");
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
    let cache = work.join("mine-cache");
    let rust_cache = work.join("rust-cache");
    let mut failures = Vec::new();
    let s = |t: &str| t.to_owned();
    // Every store of `packages/`, whole.
    for store in &stores {
        let case = Lock {
            name: "a store of packages/, all",
            files: vec![],
            args: vec![
                s("--store"),
                store.display().to_string(),
                s("-o"),
                s("out.lock"),
                s("--all"),
            ],
        };
        if let Err(e) = lock_both(&exe, &work, &case, &cache, &rust_cache) {
            failures.push(e);
        }
    }
    let sig = |c: char| c.to_string().repeat(64);
    let names = manifest_text(&[
        (&sig('1'), "\"alpha\""),
        (&sig('2'), "\"beta\""),
        (&sig('3'), "\"same\""),
        (&sig('4'), "\"same\""),
        (&sig('5'), "\"q\\\"uote\\\\back\\u0001é\""),
        (&sig('6'), "\"Zed\""),
    ]);
    let store_files = |lock: Option<&str>| {
        let mut files = vec![("st/manifest.json", names.clone())];
        if let Some(text) = lock {
            files.push(("out.lock", text.to_owned()));
        }
        files
    };
    let args = |list: &[&str]| list.iter().map(|a| (*a).to_owned()).collect::<Vec<String>>();
    let origin_lock = |git: &str, rev: &str, path: &str, entries: &str| {
        format!(
            "{{\n  \"origin\": {{\n    \"git\": \"{git}\",\n    \"rev\": \"{rev}\",\n    \"path\": \"{path}\"\n  }},\n  \"entries\": {entries}\n}}"
        )
    };
    let plain_lock = |entries: &str| format!("{{\n  \"entries\": {entries}\n}}");
    let pinned = "{\n    \"old\": {\n      \"sig_id\": \"s\",\n      \"stage_id\": \"t\",\n      \"source_hash\": \"u\"\n    },\n    \"alpha\": {\n      \"sig_id\": \"stale\",\n      \"stage_id\": \"stale\",\n      \"source_hash\": \"stale\"\n    }\n  }";
    let good_rev = "a".repeat(40);
    let cases = vec![
        Lock {
            name: "names",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock", "alpha", "beta", "Zed"]),
        },
        Lock {
            name: "escapes",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock", "q\"uote\\back\u{1}é"]),
        },
        Lock {
            name: "ambiguous",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock", "alpha", "same"]),
        },
        Lock {
            name: "ambiguous all",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock", "--all"]),
        },
        Lock {
            name: "not published",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock", "alpha", "nothing"]),
        },
        Lock {
            name: "a name twice",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock", "alpha", "alpha", "beta"]),
        },
        Lock {
            name: "no manifest",
            files: vec![("st/other", s("x"))],
            args: args(&["--store", "st", "-o", "out.lock", "--all"]),
        },
        Lock {
            name: "no store",
            files: vec![],
            args: args(&["--store", "absent", "-o", "out.lock", "alpha"]),
        },
        Lock {
            name: "empty store, name",
            files: vec![("st/manifest.json", manifest_text(&[]))],
            args: args(&["--store", "st", "-o", "out.lock", "alpha"]),
        },
        Lock {
            name: "bad manifest",
            files: vec![("st/manifest.json", s("{"))],
            args: args(&["--store", "st", "-o", "out.lock", "alpha"]),
        },
        Lock {
            name: "extend a lock",
            files: store_files(Some(&plain_lock(pinned))),
            args: args(&["--store", "st", "-o", "out.lock", "beta", "alpha"]),
        },
        Lock {
            name: "extend, all",
            files: store_files(Some(&plain_lock(pinned))),
            args: args(&["--store", "st", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "a lock with an origin, a store",
            files: store_files(Some(&origin_lock("https://x/y", &good_rev, ".cancho-vcs", pinned))),
            args: args(&["--store", "st", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "an empty lock with an origin",
            files: store_files(Some(&origin_lock("https://x/y", &good_rev, ".cancho-vcs", "{}"))),
            args: args(&["--store", "st", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "a lock with a bad origin",
            files: store_files(Some(&origin_lock("https://x/y", "main", ".cancho-vcs", pinned))),
            args: args(&["--store", "st", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "a lock that is not JSON",
            files: store_files(Some("{")),
            args: args(&["--store", "st", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "a lock without entries",
            files: store_files(Some("{}")),
            args: args(&["--store", "st", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "-o in a new directory",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "a/b/out.lock", "beta"]),
        },
        Lock {
            name: "-o a directory",
            files: vec![("st/manifest.json", names.clone()), ("out.lock/x", s("y"))],
            args: args(&["--store", "st", "-o", "out.lock", "beta"]),
        },
        // The command lines.
        Lock { name: "no -o", files: store_files(None), args: args(&["--store", "st", "beta"]) },
        Lock {
            name: "no name",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock"]),
        },
        Lock {
            name: "--all and a name",
            files: store_files(None),
            args: args(&["--store", "st", "-o", "out.lock", "--all", "beta"]),
        },
        Lock {
            name: "--git and --store",
            files: store_files(None),
            args: args(&["--store", "st", "--git", "x", "-o", "out.lock", "beta"]),
        },
        Lock { name: "neither", files: store_files(None), args: args(&["-o", "out.lock", "beta"]) },
        Lock {
            name: "--rev with --store",
            files: store_files(None),
            args: args(&["--store", "st", "--rev", "a", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "--ref with --store",
            files: store_files(None),
            args: args(&["--store", "st", "--ref", "a", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "--path with --store",
            files: store_files(None),
            args: args(&["--store", "st", "--path", "a", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "--git alone",
            files: vec![],
            args: args(&["--git", "x", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "--rev and --ref",
            files: vec![],
            args: args(&["--git", "x", "--rev", "a", "--ref", "b", "-o", "out.lock", "beta"]),
        },
        Lock { name: "-o last", files: vec![], args: args(&["--store", "st", "-o"]) },
        Lock { name: "--store last", files: vec![], args: args(&["-o", "x", "--store"]) },
        Lock { name: "--git last", files: vec![], args: args(&["--git"]) },
        Lock { name: "--rev last", files: vec![], args: args(&["--rev"]) },
        Lock { name: "--ref last", files: vec![], args: args(&["--ref"]) },
        Lock { name: "--path last", files: vec![], args: args(&["--path"]) },
        Lock { name: "an option", files: vec![], args: args(&["--bogus"]) },
        Lock { name: "nothing", files: vec![], args: vec![] },
        Lock {
            name: "a bad rev",
            files: vec![],
            args: args(&["--git", "https://x/y", "--rev", "main", "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "a bad git",
            files: vec![],
            args: args(&["--git", "--bogus", "--rev", &good_rev, "-o", "out.lock", "beta"]),
        },
        Lock {
            name: "a bad path",
            files: vec![],
            args: args(&[
                "--git",
                "https://x/y",
                "--rev",
                &good_rev,
                "--path",
                "/abs",
                "-o",
                "out.lock",
                "beta",
            ]),
        },
    ];
    for case in &cases {
        if let Err(e) = lock_both(&exe, &work, case, &cache, &rust_cache) {
            failures.push(e);
        }
    }
    // A store in a git repository: by commit, by tag and by head, the lock's origin written, an existing lock of the same origin extended and of another refused.
    let repo = super::store::repository(&work, &packages, "repository");
    let (url, rev) = (repo.url.clone(), repo.rev.clone());
    let path = "packages/net-sockets/.cancho-vcs";
    let other = "packages/http-response/.cancho-vcs";
    let git_lock = origin_lock(&url, &rev, path, "{}");
    let gits = vec![
        Lock {
            name: "git, rev, all",
            files: vec![],
            args: args(&["--git", &url, "--rev", &rev, "--path", path, "-o", "out.lock", "--all"]),
        },
        Lock {
            name: "git, ref HEAD",
            files: vec![],
            args: args(&[
                "--git", &url, "--ref", "HEAD", "--path", path, "-o", "out.lock", "bind", "listen",
            ]),
        },
        Lock {
            name: "git, same origin extended",
            files: vec![("out.lock", git_lock.clone())],
            args: args(&["--git", &url, "--rev", &rev, "--path", path, "-o", "out.lock", "bind"]),
        },
        Lock {
            name: "git, another path",
            files: vec![(
                "out.lock",
                origin_lock(
                    &url,
                    &rev,
                    path,
                    "{\n    \"old\": {\n      \"sig_id\": \"s\",\n      \"stage_id\": \"t\",\n      \"source_hash\": \"u\"\n    }\n  }",
                ),
            )],
            args: args(&["--git", &url, "--rev", &rev, "--path", other, "-o", "out.lock", "--all"]),
        },
        Lock {
            name: "git, a path that is not there",
            files: vec![],
            args: args(&[
                "--git",
                &url,
                "--rev",
                &rev,
                "--path",
                "packages/nope",
                "-o",
                "out.lock",
                "--all",
            ]),
        },
        Lock {
            name: "git, a ref that is not there",
            files: vec![],
            args: args(&[
                "--git",
                &url,
                "--ref",
                "no-such-branch",
                "--path",
                path,
                "-o",
                "out.lock",
                "--all",
            ]),
        },
        Lock {
            name: "git, a commit that is not there",
            files: vec![],
            args: args(&[
                "--git",
                &url,
                "--rev",
                &"1".repeat(40),
                "--path",
                path,
                "-o",
                "out.lock",
                "--all",
            ]),
        },
    ];
    for case in &gits {
        if let Err(e) = lock_both(&exe, &work, case, &cache, &rust_cache) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "`vcs lock` differs in {} cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let _ = std::fs::remove_dir_all(&work);
}
