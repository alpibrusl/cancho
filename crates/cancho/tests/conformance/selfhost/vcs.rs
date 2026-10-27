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
    steps_both(exe, work, case.name, &case.files, &[case.args.clone()], cache, rust_cache)
}

/// A command run by both commands, one after the other in a directory each, made alike: each step is the arguments after `vcs` (`lock` unless the first is `fetch`, `resolve` or
/// `publish`), or `@write`, a path and a text, which changes a file between two commands. `{dir}` in an argument is the directory. What is compared: the status, the standard
/// output and the first line of the standard error of each command, and every file left.
fn steps_both(
    exe: &Path,
    work: &Path,
    name: &str,
    files: &[(&'static str, String)],
    steps: &[Vec<String>],
    cache: &Path,
    rust_cache: &Path,
) -> Result<(), String> {
    let mut results = Vec::new();
    for (who, dir) in [("rust", work.join("rust")), ("cho", work.join("cho"))] {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (path, text) in files {
            if let Some(target) = path.strip_prefix("@tree:") {
                super::store::copy_dir(Path::new(text), &dir.join(target));
                continue;
            }
            let at = dir.join(path);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(at, text).unwrap();
        }
        let mut said_all = Vec::new();
        for step in steps {
            let step: Vec<String> =
                step.iter().map(|a| a.replace("{dir}", &dir.display().to_string())).collect();
            if step.first().is_some_and(|a| a == "@write") {
                let at = dir.join(&step[1]);
                std::fs::create_dir_all(at.parent().unwrap()).unwrap();
                std::fs::write(at, &step[2]).unwrap();
                continue;
            }
            let (command, rest) = match step.first().map(String::as_str) {
                Some(first @ ("fetch" | "resolve" | "publish")) => (first, &step[1..]),
                _ => ("lock", &step[..]),
            };
            let said = if who == "rust" {
                said(
                    Command::new(BIN)
                        .current_dir(&dir)
                        .env("CANCHO_CACHE", rust_cache)
                        .args(["vcs", command])
                        .args(rest)
                        .output()
                        .expect("cancho runs"),
                )
            } else {
                let mut args: Vec<String> = rest.to_vec();
                if command == "lock" {
                    if args.iter().any(|a| a == "--git")
                        && args.last().is_some_and(|a| a != "--git")
                    {
                        args.push("--cache".to_owned());
                        args.push(cache.display().to_string());
                    }
                } else {
                    // The library and the cache are named, where the Rust command has them inside it.
                    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std");
                    let mut extra = vec![
                        "--cache".to_owned(),
                        cache.display().to_string(),
                        "--std-dir".to_owned(),
                        std_dir.canonicalize().unwrap().display().to_string(),
                    ];
                    extra.extend(args);
                    args = extra;
                }
                said(
                    Command::new(exe)
                        .current_dir(&dir)
                        .args(["vcs", command])
                        .args(&args)
                        .output()
                        .expect("cli runs"),
                )
            };
            // The words after the path of a file that could not be read are the JSON reader's, or the operating system's.
            let mut said = said;
            if said.2.starts_with("cannot ") {
                let ticks: Vec<usize> = said.2.match_indices('`').map(|(i, _)| i).collect();
                if let Some(end) = ticks.get(1) {
                    said.2.truncate(end + 1);
                }
            }
            // A pin that does not hold, or a program that is refused, is said with the words of the compiler that found it, which differ.
            if command != "lock" && said.0 == Some(1) && !said.2.contains("was not given `--std`") {
                said.2.clear();
            }
            said_all.push(said);
        }
        let mut found = Vec::new();
        tree(&dir, &dir, &mut found);
        results.push((said_all, found));
    }
    if results[0] == results[1] {
        Ok(())
    } else {
        Err(format!("{name}: rust {:?}, cho {:?}", results[0], results[1]))
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
    // A repository whose store is at the default place, `.cancho-vcs` at its root: `--git` with no `--path`, and a lock whose origin does not say `path` (read as the default).
    let plain = work.join("plain");
    let _ = std::fs::remove_dir_all(&plain);
    std::fs::create_dir_all(&plain).unwrap();
    super::store::git(&plain, &["init", "--quiet", "."]);
    super::store::copy_dir(&packages.join("net-sockets/.cancho-vcs"), &plain.join(".cancho-vcs"));
    super::store::git(&plain, &["add", "-A"]);
    super::store::git(&plain, &["commit", "--quiet", "-m", "store"]);
    let plain_rev = super::store::git(&plain, &["rev-parse", "HEAD"]);
    let plain_url = format!("file://{}", plain.display());
    let no_path = format!(
        "{{\n  \"origin\": {{\n    \"git\": \"{plain_url}\",\n    \"rev\": \"{plain_rev}\"\n  }},\n  \"entries\": {{\n    \"old\": {{\n      \"sig_id\": \"s\",\n      \"stage_id\": \"t\",\n      \"source_hash\": \"u\"\n    }}\n  }}\n}}"
    );
    let mut gits = gits;
    gits.push(Lock {
        name: "git, the default path",
        files: vec![],
        args: args(&["--git", &plain_url, "--rev", &plain_rev, "-o", "out.lock", "--all"]),
    });
    gits.push(Lock {
        name: "git, the default path, ref",
        files: vec![],
        args: args(&["--git", &plain_url, "--ref", "HEAD", "-o", "out.lock", "bind"]),
    });
    gits.push(Lock {
        name: "an origin with no path, the same",
        files: vec![("out.lock", no_path.clone())],
        args: args(&["--git", &plain_url, "--rev", &plain_rev, "-o", "out.lock", "bind"]),
    });
    gits.push(Lock {
        name: "an origin with no path, another path",
        files: vec![("out.lock", no_path)],
        args: args(&[
            "--git", &plain_url, "--rev", &plain_rev, "--path", "other", "-o", "out.lock", "bind",
        ]),
    });
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

/// The lock the Rust command makes of `args` in `work`, as text.
fn rust_lock(work: &Path, cache: &Path, args: &[&str]) -> String {
    let dir = work.join("make-lock");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let done = Command::new(BIN)
        .current_dir(&dir)
        .env("CANCHO_CACHE", cache)
        .args(["vcs", "lock", "-o", "made.lock"])
        .args(args)
        .output()
        .expect("cancho runs");
    assert_eq!(done.status.code(), Some(0), "{done:?}");
    std::fs::read_to_string(dir.join("made.lock")).unwrap()
}

/// `vcs resolve` and `vcs fetch` by cancho and by Rust: four stores of `packages/` (the ones `store.rs` samples: no dependency, one dependency, a chain that imports `std`, four sources), whole and
/// by lock, a lock that names a store by its origin, locks and stores that are not what they should be, and every command line the Rust command refuses. The status, the standard output, the
/// first line of the standard error (not for a pin that does not hold) and every file written are compared.
#[test]
fn vcs_fetch_and_resolve_in_cancho_check_a_store_like_cancho_vcs() {
    let exe = build("cli-vcs-fetch", &with_front_end("cli.cho"));
    let work = scratch("selfhost-vcs-fetch");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages")
        .canonicalize()
        .expect("packages/ is there");
    // One cache for both commands: the words of the answer name it.
    let cache = work.join("cache");
    let rust_cache = cache.clone();
    let mut failures = Vec::new();
    let args = |list: &[&str]| list.iter().map(|a| (*a).to_owned()).collect::<Vec<String>>();
    let s = |t: &str| t.to_owned();
    // The sampled stores.
    for relative in [
        "net-sockets/.cancho-vcs",
        "http-response/.cancho-vcs",
        "x509/.cancho-vcs/x509_verify",
        "tls/.cancho-vcs/tls_hello",
    ] {
        let store = packages.join(relative).display().to_string();
        let lock = rust_lock(&work, &rust_cache, &["--store", &store, "--all"]);
        let cases = vec![
            Lock { name: "resolve, the store", files: vec![], args: args(&["resolve", &store]) },
            Lock {
                name: "resolve, its lock",
                files: vec![("a.lock", lock.clone())],
                args: args(&["resolve", "--lock", "a.lock", &store]),
            },
            Lock {
                name: "fetch",
                files: vec![("a.lock", lock.clone())],
                args: args(&["fetch", "--lock", "a.lock", "--store", &store, "-o", "out"]),
            },
            Lock {
                name: "fetch into a directory that is there",
                files: vec![("a.lock", lock), ("out/old", s("kept"))],
                args: args(&["fetch", "--lock", "a.lock", "--store", &store, "-o", "out/"]),
            },
        ];
        for case in &cases {
            if let Err(e) = lock_both(&exe, &work, case, &cache, &rust_cache) {
                failures.push(format!("{relative}: {e}"));
            }
        }
    }
    let store = packages.join("net-sockets/.cancho-vcs");
    let store_text = store.display().to_string();
    let lock = rust_lock(&work, &rust_cache, &["--store", &store_text, "bind", "listen"]);
    let sig = |c: char| c.to_string().repeat(64);
    let empty_lock = "{\n  \"entries\": {}\n}".to_owned();
    let stale = "{\n  \"entries\": {\n    \"gone\": {\n      \"sig_id\": \"SIG\",\n      \"stage_id\": \"t\",\n      \"source_hash\": \"u\"\n    }\n  }\n}".replace("SIG", &sig('9'));
    let cases = vec![
        // Subsets of the pins.
        Lock {
            name: "resolve, two pins",
            files: vec![("a.lock", lock.clone())],
            args: args(&["resolve", "--lock", "a.lock", &store_text]),
        },
        Lock {
            name: "fetch, two pins",
            files: vec![("a.lock", lock.clone())],
            args: args(&["fetch", "--lock", "a.lock", "--store", &store_text, "-o", "deeper/out"]),
        },
        // Locks and stores that are not there, or not what they should be.
        Lock {
            name: "resolve, an empty lock",
            files: vec![("a.lock", empty_lock.clone())],
            args: args(&["resolve", "--lock", "a.lock", &store_text]),
        },
        Lock {
            name: "fetch, an empty lock",
            files: vec![("a.lock", empty_lock.clone())],
            args: args(&["fetch", "--lock", "a.lock", "--store", &store_text, "-o", "out"]),
        },
        Lock {
            name: "resolve, no lock file",
            files: vec![],
            args: args(&["resolve", "--lock", "absent.lock", &store_text]),
        },
        Lock {
            name: "fetch, no lock file",
            files: vec![],
            args: args(&["fetch", "--lock", "absent.lock", "--store", &store_text, "-o", "out"]),
        },
        Lock {
            name: "resolve, a lock that is not JSON",
            files: vec![("a.lock", s("{"))],
            args: args(&["resolve", "--lock", "a.lock", &store_text]),
        },
        Lock {
            name: "fetch, a lock that is not JSON",
            files: vec![("a.lock", s("{"))],
            args: args(&["fetch", "--lock", "a.lock", "--store", &store_text, "-o", "out"]),
        },
        Lock {
            name: "resolve, a pin that is not published",
            files: vec![("a.lock", stale.clone())],
            args: args(&["resolve", "--lock", "a.lock", &store_text]),
        },
        Lock {
            name: "fetch, a pin that is not published",
            files: vec![("a.lock", stale)],
            args: args(&["fetch", "--lock", "a.lock", "--store", &store_text, "-o", "out"]),
        },
        Lock {
            name: "resolve, a store with nothing in it",
            files: vec![("st/manifest.json", manifest_text(&[]))],
            args: args(&["resolve", "st"]),
        },
        Lock {
            name: "resolve, no store directory",
            files: vec![],
            args: args(&["resolve", "absent"]),
        },
        Lock {
            name: "resolve, a lock with no origin and no store",
            files: vec![("a.lock", lock.clone())],
            args: args(&["resolve", "--lock", "a.lock"]),
        },
        Lock {
            name: "fetch, a lock with no origin and no store",
            files: vec![("a.lock", lock.clone())],
            args: args(&["fetch", "--lock", "a.lock", "-o", "out"]),
        },
        // The command lines.
        Lock { name: "resolve, nothing", files: vec![], args: args(&["resolve"]) },
        Lock { name: "resolve, two stores", files: vec![], args: args(&["resolve", "a", "b"]) },
        Lock { name: "resolve, an option", files: vec![], args: args(&["resolve", "--bogus"]) },
        Lock { name: "resolve, --lock last", files: vec![], args: args(&["resolve", "--lock"]) },
        Lock { name: "resolve, --store", files: vec![], args: args(&["resolve", "--store", "x"]) },
        Lock { name: "fetch, nothing", files: vec![], args: args(&["fetch"]) },
        Lock {
            name: "fetch, no -o",
            files: vec![("a.lock", empty_lock.clone())],
            args: args(&["fetch", "--lock", "a.lock"]),
        },
        Lock { name: "fetch, no --lock", files: vec![], args: args(&["fetch", "-o", "out"]) },
        Lock { name: "fetch, a word", files: vec![], args: args(&["fetch", "word"]) },
        Lock { name: "fetch, an option", files: vec![], args: args(&["fetch", "--bogus"]) },
        Lock { name: "fetch, --lock last", files: vec![], args: args(&["fetch", "--lock"]) },
        Lock { name: "fetch, --store last", files: vec![], args: args(&["fetch", "--store"]) },
        Lock { name: "fetch, -o last", files: vec![], args: args(&["fetch", "-o"]) },
    ];
    for case in &cases {
        if let Err(e) = lock_both(&exe, &work, case, &cache, &rust_cache) {
            failures.push(e);
        }
    }
    // A store changed after it was published: a source that is not what its name says, one that is gone, a pin that moved.
    let made = work.join("tampered");
    let _ = std::fs::remove_dir_all(&made);
    super::store::copy_dir(&store, &made);
    let sources = made.join("sources");
    let first = std::fs::read_dir(&sources).unwrap().next().unwrap().unwrap().path();
    let tampered_lock =
        rust_lock(&work, &rust_cache, &["--store", &made.display().to_string(), "--all"]);
    std::fs::write(&first, "module x;\n").unwrap();
    let made_text = made.display().to_string();
    let broken = vec![
        Lock {
            name: "resolve, a source changed",
            files: vec![],
            args: args(&["resolve", &made_text]),
        },
        Lock {
            name: "fetch, a source changed",
            files: vec![("a.lock", tampered_lock.clone())],
            args: args(&["fetch", "--lock", "a.lock", "--store", &made_text, "-o", "out"]),
        },
    ];
    for case in &broken {
        if let Err(e) = lock_both(&exe, &work, case, &cache, &rust_cache) {
            failures.push(e);
        }
    }
    std::fs::remove_file(&first).unwrap();
    let gone = vec![Lock {
        name: "resolve, a source gone",
        files: vec![],
        args: args(&["resolve", &made_text]),
    }];
    for case in &gone {
        if let Err(e) = lock_both(&exe, &work, case, &cache, &rust_cache) {
            failures.push(e);
        }
    }
    // A lock that says where its store is: a commit of a git repository.
    let repo = super::store::repository(&work, &packages, "repository");
    let origin_lock = rust_lock(
        &work,
        &rust_cache,
        &[
            "--git",
            &repo.url,
            "--rev",
            &repo.rev,
            "--path",
            "packages/net-sockets/.cancho-vcs",
            "--all",
        ],
    );
    let origin_cases = vec![
        Lock {
            name: "resolve, by the origin",
            files: vec![("a.lock", origin_lock.clone())],
            args: args(&["resolve", "--lock", "a.lock"]),
        },
        Lock {
            name: "fetch, by the origin",
            files: vec![("a.lock", origin_lock.clone())],
            args: args(&["fetch", "--lock", "a.lock", "-o", "out"]),
        },
        Lock {
            name: "fetch, by the origin, the store named",
            files: vec![("a.lock", origin_lock)],
            args: args(&["fetch", "--lock", "a.lock", "--store", &store_text, "-o", "out"]),
        },
    ];
    for case in &origin_cases {
        if let Err(e) = lock_both(&exe, &work, case, &cache, &rust_cache) {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "`vcs fetch`/`resolve` differ in {} cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let _ = std::fs::remove_dir_all(&work);
}

/// The `--requires` arguments (`LOCK:STORE`) the store at `relative` of `packages/` was published with: each requirement it records, its lock written to `locks/` of `work` and the store it
/// names found (a path relative to the store, here made absolute).
fn requires_of(work: &Path, packages: &Path, relative: &str) -> Vec<String> {
    let store = packages.join(relative);
    let mut names: Vec<String> = match std::fs::read_dir(store.join("requires")) {
        Ok(read) => read.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort_by_key(|n| n.trim_end_matches(".json").parse::<u32>().unwrap());
    let locks = work.join("locks");
    std::fs::create_dir_all(&locks).unwrap();
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let text = std::fs::read_to_string(store.join("requires").join(name)).unwrap();
        let at = text.find("\"lock\":").expect("a requirement has a lock") + "\"lock\":".len();
        // The lock is the last member: the text from its brace to the closing brace of the file.
        let lock = text[at..text.rfind('}').unwrap()].trim().to_owned();
        let file = locks.join(format!("{}-{i}.lock", relative.replace('/', "_")));
        std::fs::write(&file, lock).unwrap();
        let store_at = text.find("\"store\": \"").unwrap() + "\"store\": \"".len();
        let given = &text[store_at..store_at + text[store_at..].find('"').unwrap()];
        out.push("--requires".to_owned());
        if given.is_empty() {
            out.push(file.display().to_string());
        } else {
            let found = store.join(given);
            // Lexically, as the command does not follow links: `..` made out.
            let mut parts: Vec<String> = Vec::new();
            for part in found.display().to_string().split('/') {
                match part {
                    "" | "." => {}
                    ".." => {
                        parts.pop();
                    }
                    other => parts.push(other.to_owned()),
                }
            }
            out.push(format!("{}:/{}", file.display(), parts.join("/")));
        }
    }
    out
}

/// `vcs publish` by cancho and by Rust. Four stores of `packages/` (the ones `store.rs` samples) are published again from their sources into a store of the test, against the stores they
/// require, without `--std` and then with it; then the source of `net-sockets` is changed in the ways a publish tells apart (a comment, a body, a new function, a new effect row, a second
/// module, a file that does not parse, one that is refused, one that imports `std`), and the command lines the Rust command refuses are tried. The status, the standard output, the first line
/// of the standard error (not for a program that is refused) and every file of the store are compared. (`--dir` and `sync` are not ported yet.)
#[test]
fn vcs_publish_in_cancho_logs_a_file_like_cancho_vcs_publish() {
    let exe = build("cli-vcs-publish", &with_front_end("cli.cho"));
    let work = scratch("selfhost-vcs-publish");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages")
        .canonicalize()
        .expect("packages/ is there");
    let cache = work.join("cache");
    let mut failures = Vec::new();
    let blob = |relative: &str| -> PathBuf {
        let dir = packages.join(relative).join("sources");
        let first = std::fs::read_dir(dir).unwrap().next().unwrap().unwrap().path();
        first
    };
    let s = |t: &str| t.to_owned();
    // The stores of `packages/`, from their sources.
    for relative in [
        "net-sockets/.cancho-vcs",
        "http-response/.cancho-vcs",
        "x509/.cancho-vcs/x509_verify",
        "tls/.cancho-vcs/tls_hello",
    ] {
        let source = blob(relative).display().to_string();
        let mut first = vec![s("publish"), s("--store"), s("{dir}/st")];
        first.extend(requires_of(&work, &packages, relative));
        first.push(source.clone());
        let mut second = first.clone();
        second.insert(1, s("--std"));
        if let Err(e) = steps_both(&exe, &work, relative, &[], &[first, second], &cache, &cache) {
            failures.push(e);
        }
    }
    // The source of `net-sockets` changed.
    let original = std::fs::read_to_string(blob("net-sockets/.cancho-vcs")).unwrap();
    let write = |path: &str, text: &str| vec![s("@write"), s(path), s(text)];
    let publish = |file: &str| vec![s("publish"), s("--store"), s("st"), s(file)];
    let more = |extra: &str| format!("{original}\n{extra}\n");
    let cases: Vec<(&str, Vec<Vec<String>>)> = vec![
        (
            "again, and a comment",
            vec![
                publish("a.cho"),
                publish("a.cho"),
                write("a.cho", &more("// a comment, nothing else")),
                publish("a.cho"),
            ],
        ),
        (
            "a body changed",
            vec![
                publish("a.cho"),
                write(
                    "a.cho",
                    &original.replace("return at + len(src);", "return at + len(src) + 0;"),
                ),
                publish("a.cho"),
            ],
        ),
        (
            "a new function",
            vec![
                publish("a.cho"),
                write("a.cho", &more("pub fn twice(n: int) -> [] int {\n    return n + n;\n}")),
                publish("a.cho"),
                publish("a.cho"),
            ],
        ),
        (
            "a new effect row",
            vec![
                write(
                    "a.cho",
                    &more(
                        "pub fn kept[&h](heap: &!h Heap) -> [heap] int {\n    let area = box_slice(heap, 4, 0);\n    unbox_slice(heap, area);\n    return 0;\n}",
                    ),
                ),
                publish("a.cho"),
            ],
        ),
        (
            "a second module in the store",
            vec![
                publish("a.cho"),
                write(
                    "b.cho",
                    "module lib.other;\n\npub fn same(n: int) -> [] int {\n    return n;\n}\n",
                ),
                publish("b.cho"),
            ],
        ),
        ("a file that does not parse", vec![write("a.cho", &more("fn broken(")), publish("a.cho")]),
        (
            "a file that is refused",
            vec![
                write("a.cho", &more("pub fn bad() -> [] int {\n    return missing_name;\n}")),
                publish("a.cho"),
            ],
        ),
        (
            "a file that imports std",
            vec![
                write(
                    "a.cho",
                    "module lib.s;\n\nimport std.bytes;\n\npub fn same[&a, &b](x: &a [byte], y: &b [byte]) -> [] bool {\n    return bytes.equal(x, y);\n}\n",
                ),
                publish("a.cho"),
                vec![s("publish"), s("--std"), s("--store"), s("st"), s("a.cho")],
                vec![s("publish"), s("--std"), s("--store"), s("st"), s("a.cho")],
            ],
        ),
        (
            "the default store",
            vec![
                write("a.cho", &original),
                vec![s("publish"), s("a.cho")],
                vec![s("publish"), s("a.cho")],
            ],
        ),
        (
            "a store that is a file",
            vec![write("st", "not a directory"), write("a.cho", &original), publish("a.cho")],
        ),
        (
            "a manifest that is not JSON",
            vec![write("a.cho", &original), write("st/manifest.json", "{"), publish("a.cho")],
        ),
        ("no file", vec![vec![s("publish")]]),
        ("a file that is not there", vec![publish("absent.cho")]),
        ("two files", vec![vec![s("publish"), s("a.cho"), s("b.cho")]]),
        ("--dir with a file", vec![vec![s("publish"), s("--dir"), s("d"), s("a.cho")]]),
        (
            "--dir with --requires",
            vec![vec![s("publish"), s("--dir"), s("d"), s("--requires"), s("x")]],
        ),
        (
            "a lock that is not there, no store",
            vec![
                write("a.cho", &original),
                vec![s("publish"), s("--requires"), s("absent.lock"), s("a.cho")],
            ],
        ),
        (
            "a lock that is not there, a store",
            vec![
                write("a.cho", &original),
                vec![s("publish"), s("--requires"), s("absent.lock:st2"), s("a.cho")],
            ],
        ),
        ("an option", vec![vec![s("publish"), s("--bogus")]]),
        ("--store last", vec![vec![s("publish"), s("--store")]]),
        ("--dir last", vec![vec![s("publish"), s("--dir")]]),
        ("--requires last", vec![vec![s("publish"), s("--requires")]]),
    ];
    for (name, steps) in &cases {
        let files = vec![("a.cho", original.clone())];
        if let Err(e) = steps_both(&exe, &work, name, &files, steps, &cache, &cache) {
            failures.push(e);
        }
    }
    // A store that required another, published again with no requirement: the old ones go. And a program whose own row has two labels.
    let net_store = packages.join("net-sockets/.cancho-vcs").display().to_string();
    let net_lock = rust_lock(&work, &cache, &["--store", &net_store, "--all"]);
    let required = format!("n.lock:{net_store}");
    let tiny = "module lib.t;\n\npub fn one(n: int) -> [] int {\n    return n;\n}\n";
    let again = [("a.cho", tiny.to_owned()), ("n.lock", net_lock)];
    let steps = [
        vec![s("publish"), s("--store"), s("st"), s("--requires"), required, s("a.cho")],
        publish("a.cho"),
    ];
    if let Err(e) = steps_both(&exe, &work, "requirements dropped", &again, &steps, &cache, &cache)
    {
        failures.push(e);
    }
    let two = format!(
        "{original}\npub fn two[&h, &c](heap: &!h Heap, clock: &c Clock) -> [heap, clock] int {{\n    let area = box_slice(heap, 4, 0);\n    unbox_slice(heap, area);\n    return clock_ms(clock);\n}}\n"
    );
    let files = [("a.cho", two)];
    if let Err(e) =
        steps_both(&exe, &work, "a row of two labels", &files, &[publish("a.cho")], &cache, &cache)
    {
        failures.push(e);
    }
    assert!(
        failures.is_empty(),
        "`vcs publish` differs in {} cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let _ = std::fs::remove_dir_all(&work);
}
