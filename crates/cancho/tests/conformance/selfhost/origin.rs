//! The origin cache in cancho (`examples/selfhost/cliorigin.cho`, `docs/cli-in-cancho.md` section 22, slice 14a), against the Rust code it stands in for
//! (`crates/cancho/src/vcs_origin.rs`), which is reached through `cancho add` (it makes the checkout before it looks at the store) and `$CANCHO_CACHE`.

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

/// Every file beneath `dir` with its text, sorted: what a checkout holds.
fn tree(dir: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).expect("a directory is read") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path.clone());
                found.push((
                    format!("{}/", path.strip_prefix(dir).unwrap().display()),
                    String::new(),
                ));
            } else {
                found.push((
                    path.strip_prefix(dir).unwrap().display().to_string(),
                    std::fs::read_to_string(&path).unwrap_or_default(),
                ));
            }
        }
    }
    found.sort();
    found
}

/// `cancho add` of a dependency on the origin, in a fresh project with `$CANCHO_CACHE` at `cache`: its status, standard output and standard error.
fn rust_add(
    project: &Path,
    cache: &Path,
    url: &str,
    how: &[&str],
    path: &str,
) -> (Option<i32>, String, String) {
    let _ = std::fs::remove_dir_all(project);
    std::fs::create_dir_all(project).expect("the project directory is made");
    std::fs::write(project.join("cancho.toml"), "[package]\nname = \"p\"\n")
        .expect("the project file is written");
    let done = Command::new(BIN)
        .current_dir(project)
        .env("CANCHO_CACHE", cache)
        .args(["add", "dep", url])
        .args(how)
        .args(["--path", path, "--ignore-compiler-rev"])
        .output()
        .expect("cancho add runs");
    (
        done.status.code(),
        String::from_utf8_lossy(&done.stdout).into_owned(),
        String::from_utf8_lossy(&done.stderr).into_owned(),
    )
}

fn mine(tool: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let done = Command::new(tool).args(args).output().expect("origincheck runs");
    (
        done.status.code(),
        String::from_utf8_lossy(&done.stdout).trim().to_owned(),
        String::from_utf8_lossy(&done.stderr).trim().to_owned(),
    )
}

/// `ensure` and `resolve_ref` in cancho against the Rust code, on local repositories: the checkout is the same files (and has no `.git` and no
/// half-made neighbour), a second ask changes nothing, and each refusal says what the Rust code says.
#[test]
fn the_origin_cache_in_cancho_makes_the_checkout_the_rust_code_makes() {
    let tool = build("origincheck", &with_front_end("origincheck.cho"));
    let work = scratch("selfhost-origin");
    let _ = std::fs::remove_dir_all(&work);
    let repo = work.join("repo");
    std::fs::create_dir_all(repo.join(".cancho-vcs/inner")).expect("the repository is made");
    std::fs::create_dir_all(repo.join("other")).expect("the repository is made");
    git(&repo, &["init", "--quiet", "."]);
    std::fs::write(repo.join(".cancho-vcs/manifest.json"), "{}\n").unwrap();
    std::fs::write(repo.join(".cancho-vcs/inner/a.txt"), "inner\n").unwrap();
    std::fs::write(repo.join("other/b.txt"), "other\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "one"]);
    let first = git(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join("other/b.txt"), "changed\n").unwrap();
    git(&repo, &["commit", "--quiet", "-am", "two"]);
    let second = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["tag", "v1", &first]);
    let url = format!("file://{}", repo.display());
    let (theirs_cache, my_cache) = (work.join("theirs"), work.join("mine"));
    let project = work.join("project");

    let mut agreed = Vec::new();
    for rev in [&first, &second] {
        let other = rust_add(&project, &theirs_cache, &url, &["--rev", rev], ".cancho-vcs");
        let ours = mine(&tool, &["ensure", my_cache.to_str().unwrap(), &url, rev, ".cancho-vcs"]);
        assert_eq!(ours.0, Some(0), "{ours:?}");
        let (a, b) =
            (tree(&theirs_cache.join("git").join(rev)), tree(&my_cache.join("git").join(rev)));
        assert_eq!(a, b, "the checkout of {rev}");
        assert!(a.iter().all(|(name, _)| !name.starts_with(".git")), "no .git: {a:?}");
        assert!(
            ours.1.ends_with(&format!("git/{rev}/.cancho-vcs")),
            "the store directory: {}",
            ours.1
        );
        let _ = other;
        agreed.push(format!("checkout of {}", &rev[..7]));
    }
    // Only the commits asked for are there, and nothing half made.
    let leftovers = |cache: &Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(cache.join("git"))
            .expect("the cache is there")
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(leftovers(&theirs_cache), leftovers(&my_cache));
    // A second ask changes nothing (the cache is not asked again).
    let again = mine(
        &tool,
        &["ensure", my_cache.to_str().unwrap(), "file:///nowhere", &first, ".cancho-vcs"],
    );
    assert_eq!(again.0, Some(0), "a cached commit asks nothing of the repository: {again:?}");
    agreed.push("a second ask".to_owned());

    // The refusals.
    let cases: Vec<(&str, Vec<String>, String, &str)> = vec![
        ("no such path", vec!["--rev".into(), first.clone()], first.clone(), "nothere"),
        ("not a hash", vec!["--rev".into(), "abc".into()], "abc".into(), ".cancho-vcs"),
        ("no such commit", vec!["--rev".into(), "0".repeat(40)], "0".repeat(40), ".cancho-vcs"),
        ("an escaping path", vec!["--rev".into(), first.clone()], first.clone(), "../x"),
        ("an absolute path", vec!["--rev".into(), first.clone()], first.clone(), "/x"),
    ];
    for (name, how, rev, path) in cases {
        let _ = std::fs::remove_dir_all(&theirs_cache);
        let _ = std::fs::remove_dir_all(&my_cache);
        let how: Vec<&str> = how.iter().map(String::as_str).collect();
        let other = rust_add(&project, &theirs_cache, &url, &how, path);
        let ours = mine(&tool, &["ensure", my_cache.to_str().unwrap(), &url, &rev, path]);
        assert_ne!(ours.0, Some(0), "{name}: {ours:?}");
        let said = ours.2.lines().next().unwrap_or("").to_owned();
        assert!(
            !said.is_empty() && other.2.contains(&said),
            "{name}:\n theirs {other:?}\n mine {ours:?}"
        );
        // Nothing half made is left in the cache, as in the Rust code.
        if my_cache.join("git").is_dir() || theirs_cache.join("git").is_dir() {
            assert_eq!(leftovers(&theirs_cache), leftovers(&my_cache), "{name}: the cache");
        }
        agreed.push(format!("{name}: {:?}", ours.0));
    }

    // A ref is resolved to the hash it names now, and one that is not there is refused with the same words.
    for name in ["HEAD", "v1"] {
        let other = rust_add(&project, &theirs_cache, &url, &["--ref", name], ".cancho-vcs");
        let ours = mine(&tool, &["ref", &url, name]);
        assert!(other.1.contains(&format!("{name} is {}", ours.1)), "{name}: {other:?} {ours:?}");
        agreed.push(format!("ref {name}"));
    }
    let other = rust_add(&project, &theirs_cache, &url, &["--ref", "nope"], ".cancho-vcs");
    let ours = mine(&tool, &["ref", &url, "nope"]);
    assert_eq!(ours.0, Some(3));
    assert!(other.2.contains(&ours.2), "{other:?} {ours:?}");
    for bad in ["-x", "ext::sh"] {
        let ours = mine(&tool, &["ref", bad, "HEAD"]);
        assert_eq!(ours.0, Some(3), "{bad}: {ours:?}");
        assert_eq!(ours.2, format!("not a repository location: `{bad}`"));
    }
    let ours = mine(&tool, &["ref", &url, "-x"]);
    assert_eq!((ours.0, ours.2.as_str()), (Some(3), "not a ref name: `-x`"));

    // With no `--cache`, the root is `.cache/cancho` in the home directory.
    if let Ok(home) = std::env::var("HOME") {
        assert_eq!(mine(&tool, &["root"]).1, format!("{home}/.cache/cancho"));
    }
    eprintln!("agreed on: {}", agreed.join("; "));
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(tool.parent().expect("a scratch directory"));
}
