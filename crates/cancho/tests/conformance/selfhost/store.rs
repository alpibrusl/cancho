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
fn stores_in(dir: &Path, at: &Path, found: &mut Vec<String>) {
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

struct World {
    repo: PathBuf,
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
    World { url: format!("file://{}", repo.display()), repo, rev }
}

/// `cancho install` of the dependency `path` of `world`: its status and the files it wrote to `build/deps`.
fn rust_install(
    work: &Path,
    world: &World,
    path: &str,
) -> (Option<i32>, String, Vec<(String, Vec<u8>)>) {
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
fn mine_fetch(
    tools: (&Path, &Path),
    work: &Path,
    world: &World,
    path: &str,
) -> (Option<i32>, String, Vec<(String, Vec<u8>)>) {
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
