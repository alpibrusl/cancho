//! The store checks in cancho (`examples/selfhost/clistore.cho`, `docs/cli-in-cancho.md` section 22, slice 14b), against `cancho install`, which does the same for a
//! project's dependency: the origin cache makes the checkout, `lock_of_all` pins everything the manifest holds, and `fetch_verified` re-checks the closure and writes
//! the sources.

use super::*;

fn git(dir: &Path, args: &[&str]) -> String {
    let done = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=a@b", "-c", "user.name=n", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(done.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&done.stderr));
    String::from_utf8_lossy(&done.stdout).trim().to_owned()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory is made");
    for entry in std::fs::read_dir(from).expect("a directory is read") {
        let path = entry.expect("an entry").path();
        let target = to.join(path.file_name().expect("a name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("a file is copied");
        }
    }
}

/// Every directory beneath `dir` (and `dir`) that holds a `manifest.json`, as paths relative to `dir`.
pub(super) fn stores_in(dir: &Path, at: &Path, found: &mut Vec<String>) {
    if at.join("manifest.json").is_file() {
        found.push(at.strip_prefix(dir).unwrap().display().to_string());
    }
    let mut kids: Vec<PathBuf> = std::fs::read_dir(at)
        .expect("a directory is read")
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.is_dir()
                && p.file_name().is_some_and(|n| n != "sources" && n != "ops" && n != "requires")
        })
        .collect();
    kids.sort();
    for kid in kids {
        stores_in(dir, &kid, found);
    }
}

/// The files of a directory with their bytes, sorted by name.
fn files_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read {
            let path = entry.unwrap().path();
            found.push((
                path.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&path).unwrap(),
            ));
        }
    }
    found.sort();
    found
}

/// What a fetch gave: the status, what it said on the standard error, and the files it wrote.
type Fetch = (Option<i32>, String, Vec<(String, Vec<u8>)>);

struct World {
    url: String,
    rev: String,
}

/// A repository whose commit holds the directory `tree` as `packages` (a store that requires another by a relative path finds it there).
fn repository(work: &Path, tree: &Path, name: &str) -> World {
    let repo = work.join(name);
    let _ = std::fs::remove_dir_all(&repo);
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet", "."]);
    copy_dir(tree, &repo.join("packages"));
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "stores"]);
    let rev = git(&repo, &["rev-parse", "HEAD"]);
    World { url: format!("file://{}", repo.display()), rev }
}

/// `cancho install` of the dependency `path` of `world`: its status and the files it wrote to `build/deps`.
fn rust_install(work: &Path, world: &World, path: &str) -> Fetch {
    let project = work.join("project");
    let _ = std::fs::remove_dir_all(&project);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("cancho.toml"),
        format!(
            "[package]\nname = \"p\"\n\n[dependencies.dep]\ngit = \"{}\"\nrev = \"{}\"\npath = \"{}\"\n",
            world.url, world.rev, path
        ),
    )
    .unwrap();
    let done = Command::new(BIN)
        .current_dir(&project)
        .env("CANCHO_CACHE", work.join("rust-cache"))
        .args(["install", "--ignore-compiler-rev"])
        .output()
        .expect("cancho install runs");
    (
        done.status.code(),
        String::from_utf8_lossy(&done.stderr).into_owned(),
        files_in(&project.join("build/deps")),
    )
}

/// The same in cancho: the checkout by `origincheck`, the fetch by `storecheck`.
fn mine_fetch(tools: (&Path, &Path), work: &Path, world: &World, path: &str) -> Fetch {
    let cache = work.join("mine-cache");
    let out = work.join("mine-out");
    let _ = std::fs::remove_dir_all(&out);
    let ensured = Command::new(tools.0)
        .args(["ensure", cache.to_str().unwrap(), &world.url, &world.rev, path])
        .output()
        .expect("origincheck runs");
    assert_eq!(ensured.status.code(), Some(0), "{ensured:?}");
    let store = String::from_utf8_lossy(&ensured.stdout).trim().to_owned();
    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std").canonicalize().unwrap();
    let done = Command::new(tools.1)
        .args([
            "fetch",
            cache.to_str().unwrap(),
            std_dir.to_str().unwrap(),
            &store,
            out.to_str().unwrap(),
        ])
        .output()
        .expect("storecheck runs");
    (done.status.code(), String::from_utf8_lossy(&done.stderr).into_owned(), files_in(&out))
}

/// Every store in `packages/` fetched by `cancho install` and by `storecheck fetch`: the same status, and, where it worked, the same files.
#[test]
fn the_stores_of_packages_are_fetched_in_cancho_as_cancho_install_fetches_them() {
    let origin = build("origincheck-store", &with_front_end("origincheck.cho"));
    let store = build("storecheck", &with_front_end("storecheck.cho"));
    let work = scratch("selfhost-store");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let world = repository(&work, &packages, "repository");
    let mut roots: Vec<PathBuf> = std::fs::read_dir(&packages)
        .unwrap()
        .map(|e| e.unwrap().path().join(".cancho-vcs"))
        .filter(|p| p.is_dir())
        .collect();
    roots.sort();
    let (mut agreed, mut different) = (Vec::new(), Vec::new());
    for root in roots {
        let package = root.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned();
        let mut paths = Vec::new();
        stores_in(&root, &root, &mut paths);
        for inner in paths {
            // The whole of `packages/` was compared once (26 stores, 800 s in the debug build); CI compares these four: no dependency, one dependency, a dependency chain that
            // imports `std`, and a store of four sources.
            let sampled = [
                "net-sockets/.cancho-vcs",
                "http-response/.cancho-vcs",
                "x509/.cancho-vcs/x509_verify",
                "tls/.cancho-vcs/tls_hello",
            ];
            let wanted = if inner.is_empty() {
                format!("{package}/.cancho-vcs")
            } else {
                format!("{package}/.cancho-vcs/{inner}")
            };
            if !sampled.contains(&wanted.as_str()) {
                continue;
            }
            let path = if inner.is_empty() {
                format!("packages/{package}/.cancho-vcs")
            } else {
                format!("packages/{package}/.cancho-vcs/{inner}")
            };
            let name = path.clone();
            let theirs = rust_install(&work, &world, &path);
            let ours = mine_fetch((&origin, &store), &work, &world, &path);
            let theirs_ok = theirs.0 == Some(0);
            let ours_ok = ours.0 == Some(0);
            if theirs_ok != ours_ok || (theirs_ok && theirs.2 != ours.2) {
                different.push(format!(
                    "{name}:\n  theirs {:?} {}\n  mine   {:?} {}",
                    theirs.0,
                    theirs.2.len(),
                    ours.0,
                    ours.1
                ));
            } else {
                agreed.push(format!(
                    "{name}: {}",
                    if theirs_ok {
                        format!("{} file(s)", ours.2.len())
                    } else {
                        format!(
                            "both refuse ({:?}: {})",
                            ours.0,
                            theirs.1.lines().next().unwrap_or("")
                        )
                    }
                ));
            }
        }
    }
    eprintln!("agreed on: {}", agreed.join("; "));
    assert!(
        different.is_empty(),
        "`cancho install` and `storecheck fetch` disagree about:\n{}",
        different.join("\n")
    );
    assert_eq!(agreed.len(), 4, "the stores compared: {agreed:?}");
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(origin.parent().unwrap());
    let _ = std::fs::remove_dir_all(store.parent().unwrap());
}

/// A store that does not hold: the same refusal from both. Each case changes a copy of `packages/net-sockets` (no dependency, one source) and fetches it.
#[test]
fn a_store_that_does_not_hold_is_refused_in_cancho_as_cancho_install_refuses_it() {
    let origin = build("origincheck-bad-store", &with_front_end("origincheck.cho"));
    let store = build("storecheck-bad", &with_front_end("storecheck.cho"));
    let work = scratch("selfhost-bad-store");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let base = work.join("base");
    copy_dir(&packages.join("net-sockets"), &base.join("net-sockets"));
    let inside = base.join("net-sockets/.cancho-vcs");
    let blob = std::fs::read_dir(inside.join("sources")).unwrap().next().unwrap().unwrap().path();
    let text = std::fs::read_to_string(&blob).unwrap();
    type Change = Box<dyn Fn(&Path)>;
    let cases: Vec<(&str, Change)> = vec![
        ("as it is", Box::new(|_| {})),
        (
            "a blob that is not what its name says",
            Box::new({
                let name = blob.file_name().unwrap().to_owned();
                let text = text.clone();
                move |at| {
                    std::fs::write(at.join("sources").join(&name), format!("{text}\n// changed\n"))
                        .unwrap()
                }
            }),
        ),
        (
            "a blob that is not there",
            Box::new({
                let name = blob.file_name().unwrap().to_owned();
                move |at| std::fs::remove_file(at.join("sources").join(&name)).unwrap()
            }),
        ),
        (
            "a body that is not the one the manifest says",
            Box::new(|at| {
                let manifest = std::fs::read_to_string(at.join("manifest.json")).unwrap();
                let mut value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
                let entry = value["entries"].as_object_mut().unwrap().values_mut().next().unwrap();
                entry["stage_id"] = serde_json::Value::String("0".repeat(64));
                std::fs::write(at.join("manifest.json"), value.to_string()).unwrap();
            }),
        ),
        (
            "a signature that its source does not produce",
            Box::new(|at| {
                let manifest = std::fs::read_to_string(at.join("manifest.json")).unwrap();
                let mut value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
                let entries = value["entries"].as_object_mut().unwrap();
                let key = entries.keys().next().unwrap().clone();
                let entry = entries.remove(&key).unwrap();
                entries.insert("f".repeat(64), entry);
                std::fs::write(at.join("manifest.json"), value.to_string()).unwrap();
            }),
        ),
        (
            "a name published twice, the signature that sorts last being no longer produced",
            Box::new(|at| {
                let manifest = std::fs::read_to_string(at.join("manifest.json")).unwrap();
                let mut value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
                let entries = value["entries"].as_object_mut().unwrap();
                let copy = entries.values().next().unwrap().clone();
                entries.insert("f".repeat(64), copy);
                std::fs::write(at.join("manifest.json"), value.to_string()).unwrap();
            }),
        ),
        (
            "a manifest that is not JSON",
            Box::new(|at| std::fs::write(at.join("manifest.json"), "{").unwrap()),
        ),
        (
            "a manifest with nothing in it",
            Box::new(|at| std::fs::write(at.join("manifest.json"), "{\"entries\": {}}").unwrap()),
        ),
        (
            "a store that requires itself",
            Box::new(|at| {
                let manifest = std::fs::read_to_string(at.join("manifest.json")).unwrap();
                let value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
                let (sig, entry) = value["entries"].as_object().unwrap().iter().next().unwrap();
                let requirement = serde_json::json!({
                    "store": ".",
                    "lock": { "entries": { entry["name"].as_str().unwrap(): {
                        "sig_id": sig, "stage_id": entry["stage_id"], "source_hash": entry["source_hash"]
                    } } }
                });
                std::fs::create_dir_all(at.join("requires")).unwrap();
                std::fs::write(at.join("requires/0.json"), requirement.to_string()).unwrap();
            }),
        ),
    ];
    let (mut agreed, mut different) = (Vec::new(), Vec::new());
    for (name, change) in cases {
        let tree = work.join("tree");
        let _ = std::fs::remove_dir_all(&tree);
        copy_dir(&base, &tree);
        change(&tree.join("net-sockets/.cancho-vcs"));
        let world = repository(&work, &tree, "repository");
        let path = "packages/net-sockets/.cancho-vcs";
        let theirs = rust_install(&work, &world, path);
        let ours = mine_fetch((&origin, &store), &work, &world, path);
        let (theirs_ok, ours_ok) = (theirs.0 == Some(0), ours.0 == Some(0));
        if theirs_ok != ours_ok || (theirs_ok && theirs.2 != ours.2) {
            different.push(format!(
                "{name}:\n  theirs {:?} {}\n  mine   {:?} {}",
                theirs.0, theirs.1, ours.0, ours.1
            ));
        } else {
            agreed.push(format!("{name}: {}", if ours_ok { "fetched" } else { "refused" }));
        }
        let _ = std::fs::remove_dir_all(work.join("mine-cache"));
        let _ = std::fs::remove_dir_all(work.join("rust-cache"));
    }
    eprintln!("agreed on: {}", agreed.join("; "));
    assert!(
        different.is_empty(),
        "`cancho install` and `storecheck fetch` disagree about:\n{}",
        different.join("\n")
    );
    assert_eq!(
        agreed.iter().filter(|a| a.ends_with("fetched")).count(),
        1,
        "only the unchanged store is fetched: {agreed:?}"
    );
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(origin.parent().unwrap());
    let _ = std::fs::remove_dir_all(store.parent().unwrap());
}

/// A project that depends on `net-sockets` (a store inside a git repository), with a program and a set of tests that import it.
fn project_with_dependency(dir: &Path, world: &World, path: &str) {
    let _ = std::fs::remove_dir_all(dir);
    for sub in ["src", "tests"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    std::fs::write(
        dir.join("cancho.toml"),
        format!(
            "[package]\nname = \"p\"\n\n[dependencies.socks]\ngit = \"{}\"\nrev = \"{}\"\npath = \"{path}\"\n\n\
             [[bin]]\nname = \"a\"\nsources = [\"src\"]\n\n[[test]]\nname = \"t\"\nsources = [\"tests\"]\n",
            world.url, world.rev
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.cho"),
        "edition 7;\nimport net.sockets;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi);\n    release(net);\n    release(signals);\n    release(clock);\n    release(exec);\n    release(fs);\n    release(args);\n    release(heap);\n    release(io);\n    return 0;\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("tests/t.cho"),
        "edition 7;\nimport net.sockets;\nfn test_one() -> [] int { return 0; }\n",
    )
    .unwrap();
}

/// `install`, `build` and `test` of a project with a dependency, by `cancho` and by `cli.cho`: the same status, the same words on the standard output and (where it is a
/// refusal) error, and the same files in `build/deps`.
#[test]
fn the_command_line_in_cancho_installs_dependencies_like_cancho() {
    let cli = build("cli-dependencies", &with_front_end("cli.cho"));
    let work = scratch("selfhost-dependencies");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let tree = work.join("tree");
    copy_dir(&packages.join("net-sockets"), &tree.join("net-sockets"));
    let good = repository(&work, &tree, "good");
    let empty_tree = work.join("empty-tree");
    copy_dir(&tree, &empty_tree);
    std::fs::write(empty_tree.join("net-sockets/.cancho-vcs/manifest.json"), "{\"entries\": {}}")
        .unwrap();
    let empty = repository(&work, &empty_tree, "empty");
    let path = "packages/net-sockets/.cancho-vcs";
    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std").canonicalize().unwrap();
    let project = work.join("project");
    let (mut agreed, mut different) = (Vec::new(), Vec::new());
    let cases: Vec<(&str, &World, &str, &str)> = vec![
        ("install", &good, path, "install"),
        ("install over a file an older install left", &good, path, "install"),
        ("build", &good, path, "build"),
        ("test", &good, path, "test"),
        ("a directory that is not in the commit", &good, "packages/nowhere", "install"),
        ("a store with nothing in it", &empty, path, "install"),
    ];
    for (name, world, at, command) in cases {
        let mut seen = Vec::new();
        for mine in [false, true] {
            project_with_dependency(&project, world, at);
            if name.contains("older install") {
                std::fs::create_dir_all(project.join("build/deps")).unwrap();
                std::fs::write(project.join("build/deps/old.cho"), "// an old pin\n").unwrap();
            }
            let mut run = Command::new(if mine { cli.as_path() } else { Path::new(BIN) });
            run.current_dir(&project).arg(command).stdin(Stdio::null());
            if mine {
                run.args([
                    "--cache",
                    work.join("mine-cache").to_str().unwrap(),
                    "--std-dir",
                    std_dir.to_str().unwrap(),
                ]);
            } else {
                run.env("CANCHO_CACHE", work.join("rust-cache")).arg("--ignore-compiler-rev");
            }
            let done = run.output().expect("the command runs");
            let said = |bytes: &[u8]| {
                String::from_utf8_lossy(bytes)
                    .lines()
                    .filter(|l| {
                        !l.starts_with("building the compiler in cancho")
                            && !l.starts_with("ld: warning:")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    .replace(&project.display().to_string(), "DIR")
                    .replace(&work.display().to_string(), "WORK")
            };
            if mine && done.status.code() == Some(0) {
                assert!(
                    work.join("mine-cache/git").is_dir(),
                    "{name}: `--cache` is where the checkout went"
                );
            }
            seen.push((
                done.status.code(),
                said(&done.stdout),
                said(&done.stderr),
                files_in(&project.join("build/deps")),
                project.join("build/a").is_file(),
            ));
        }
        // What git said is git's words and differs by the environment it was given; the refusals are compared by their first line.
        let (theirs, mine) = (&seen[0], &seen[1]);
        let same = theirs.0 == mine.0
            && theirs.1 == mine.1
            && theirs.3 == mine.3
            && theirs.4 == mine.4
            && theirs.2.lines().next() == mine.2.lines().next();
        if same {
            agreed.push(format!("{name}: {:?}", mine.0));
        } else {
            different.push(format!("{name}:\n  theirs {theirs:?}\n  mine   {mine:?}"));
        }
        let _ = std::fs::remove_dir_all(work.join("mine-cache"));
        let _ = std::fs::remove_dir_all(work.join("rust-cache"));
    }
    eprintln!("agreed on: {}", agreed.join("; "));
    assert!(
        different.is_empty(),
        "`cancho` and `cli.cho` disagree about a dependency in:\n{}",
        different.join("\n")
    );
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(cli.parent().unwrap());
}

/// `add` by `cancho` and by `cli.cho`, each in a project of its own that has a `cancho.toml` with a comment in it: the same status, the same standard output, the same first line
/// of the standard error, the same `cancho.toml` afterwards (the file is put back if the install fails) and the same files in `build/deps`.
#[test]
fn the_command_line_in_cancho_adds_dependencies_like_cancho() {
    let cli = build("cli-add", &with_front_end("cli.cho"));
    let work = scratch("selfhost-add");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).unwrap();
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let tree = work.join("tree");
    copy_dir(&packages.join("net-sockets"), &tree.join("net-sockets"));
    // Two stores one level down, for the message that says where they are.
    copy_dir(&tree.join("net-sockets/.cancho-vcs"), &tree.join("stores/a"));
    copy_dir(&tree.join("net-sockets/.cancho-vcs"), &tree.join("stores/b"));
    let good = repository(&work, &tree, "good");
    git(&work.join("good"), &["tag", "v1"]);
    let empty_tree = work.join("empty-tree");
    copy_dir(&tree, &empty_tree);
    std::fs::write(empty_tree.join("net-sockets/.cancho-vcs/manifest.json"), "{\"entries\": {}}")
        .unwrap();
    let empty = repository(&work, &empty_tree, "empty");
    let url = good.url.clone();
    let (rev, path) = (good.rev.clone(), "packages/net-sockets/.cancho-vcs".to_owned());
    let empty_url = empty.url.clone();
    let empty_rev = empty.rev.clone();
    let std_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std").canonicalize().unwrap();
    let project = work.join("project");
    let to_strings =
        |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<String>>();
    // (name, the words after `add`, whether the project file is there, whether to add `socks` first)
    let cases: Vec<(&str, Vec<String>, bool, bool)> = vec![
        (
            "a revision",
            vec![
                "socks".into(),
                url.clone(),
                "--rev".into(),
                rev.clone(),
                "--path".into(),
                path.clone(),
            ],
            true,
            false,
        ),
        (
            "a tag",
            vec![
                "socks".into(),
                url.clone(),
                "--ref".into(),
                "v1".into(),
                "--path".into(),
                path.clone(),
            ],
            true,
            false,
        ),
        ("the head", vec!["socks".into(), url.clone(), "--path".into(), path.clone()], true, false),
        (
            "a name taken",
            vec![
                "socks".into(),
                url.clone(),
                "--rev".into(),
                rev.clone(),
                "--path".into(),
                path.clone(),
            ],
            true,
            true,
        ),
        (
            "a directory that is not in the commit",
            vec!["socks".into(), url.clone(), "--path".into(), "nowhere".into()],
            true,
            false,
        ),
        (
            "a directory that is not a store",
            vec!["socks".into(), url.clone(), "--path".into(), "packages/stores".into()],
            true,
            false,
        ),
        (
            "a revision that is not a hash",
            vec!["socks".into(), url.clone(), "--rev".into(), "abc".into()],
            true,
            false,
        ),
        (
            "a ref that is not there",
            vec![
                "socks".into(),
                url.clone(),
                "--ref".into(),
                "nope".into(),
                "--path".into(),
                path.clone(),
            ],
            true,
            false,
        ),
        (
            "a store with nothing in it",
            vec![
                "socks".into(),
                empty_url.clone(),
                "--rev".into(),
                empty_rev.clone(),
                "--path".into(),
                path.clone(),
            ],
            true,
            false,
        ),
        (
            "a url with a quote in it",
            vec!["socks".into(), "file:///x\"y".into(), "--rev".into(), rev.clone()],
            true,
            false,
        ),
        (
            "a path with a backslash in it",
            vec![
                "socks".into(),
                url.clone(),
                "--rev".into(),
                rev.clone(),
                "--path".into(),
                "a\\b".into(),
            ],
            true,
            false,
        ),
        (
            "--rev and --ref",
            vec![
                "socks".into(),
                url.clone(),
                "--rev".into(),
                rev.clone(),
                "--ref".into(),
                "v1".into(),
            ],
            true,
            false,
        ),
        (
            "a name that is not one",
            to_strings(&["a b"]).into_iter().chain([url.clone()]).collect(),
            true,
            false,
        ),
        ("no url", to_strings(&["socks"]), true, false),
        (
            "an option that is not one",
            vec!["socks".into(), url.clone(), "--nope".into()],
            true,
            false,
        ),
        (
            "no project",
            vec![
                "socks".into(),
                url.clone(),
                "--rev".into(),
                rev.clone(),
                "--path".into(),
                path.clone(),
            ],
            false,
            false,
        ),
    ];
    let (mut agreed, mut different) = (Vec::new(), Vec::new());
    for (name, words, has_project, add_first) in cases {
        let mut seen = Vec::new();
        for mine in [false, true] {
            let _ = std::fs::remove_dir_all(&project);
            std::fs::create_dir_all(&project).unwrap();
            if has_project {
                std::fs::write(project.join("cancho.toml"), "# a comment\n[package]\nname = \"p\"")
                    .unwrap();
            }
            let run = |extra: &[String]| {
                let mut command = Command::new(if mine { cli.as_path() } else { Path::new(BIN) });
                command.current_dir(&project).arg("add").args(&words).stdin(Stdio::null());
                if mine {
                    command.args([
                        "--cache",
                        work.join("mine-cache").to_str().unwrap(),
                        "--std-dir",
                        std_dir.to_str().unwrap(),
                    ]);
                } else {
                    command
                        .env("CANCHO_CACHE", work.join("rust-cache"))
                        .arg("--ignore-compiler-rev");
                }
                command.args(extra).output().expect("the command runs")
            };
            if add_first {
                let first = run(&[]);
                assert_eq!(first.status.code(), Some(0), "{name}: {first:?}");
            }
            let done = run(&[]);
            let said = |bytes: &[u8]| {
                String::from_utf8_lossy(bytes)
                    .lines()
                    .filter(|l| {
                        !l.starts_with("building the compiler in cancho")
                            && !l.starts_with("ld: warning:")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    .replace(&project.display().to_string(), "DIR")
                    .replace(&work.display().to_string(), "WORK")
            };
            seen.push((
                done.status.code(),
                said(&done.stdout),
                said(&done.stderr),
                std::fs::read_to_string(project.join("cancho.toml")).unwrap_or_default(),
                files_in(&project.join("build/deps")),
            ));
        }
        let (theirs, mine) = (&seen[0], &seen[1]);
        // The `installed` line of the first add is in both; a refusal is compared by its first line (the usage text after it is Rust's).
        let same = theirs.0 == mine.0
            && theirs.1 == mine.1
            && theirs.3 == mine.3
            && theirs.4 == mine.4
            && theirs.2.lines().next() == mine.2.lines().next();
        if same {
            agreed.push(format!("{name}: {:?}", mine.0));
        } else {
            different.push(format!("{name}:\n  theirs {theirs:?}\n  mine   {mine:?}"));
        }
        let _ = std::fs::remove_dir_all(work.join("mine-cache"));
        let _ = std::fs::remove_dir_all(work.join("rust-cache"));
    }
    eprintln!("agreed on: {}", agreed.join("; "));
    assert!(
        different.is_empty(),
        "`cancho add` and `cli.cho add` disagree about:\n{}",
        different.join("\n")
    );
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(cli.parent().unwrap());
}
