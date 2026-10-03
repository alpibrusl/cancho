//! `docs/package-system.md` §7: a store in another repository (an origin on a
//! lock, fetched through a cache of git checkouts) and a library of several
//! files published as a directory.
//!
//! Every repository here is a local one made with `git`, so nothing touches a
//! network. The cache is a scratch directory per test (`LEX_SYS_CACHE`), so a
//! test sees exactly the fetches it made.

use super::*;

/// A library of two modules: `libx.base` has a `static` (a table, the thing
/// `vcs publish` could not publish before §7.4) and a function that reads it;
/// `libx.top` imports it.
const BASE: &str = "\
module libx.base;

static table: [int] {
    let t = alloc_slice[static](4, 0);
    t[0] = 10;
    t[1] = 20;
    t[2] = 30;
    t[3] = 40;
    return t;
}

pub fn pick(i: int) -> [] int {
    return table[i];
}
";

const TOP: &str = "\
module libx.top;

import libx.base;

pub fn twice(i: int) -> [] int {
    return base.pick(i) * 2;
}
";

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("a writable scratch directory");
    let path = dir.join(name);
    std::fs::write(&path, text).expect("a writable scratch file");
    path
}

fn vcs(cache: &Path, args: &[&dyn AsRef<std::ffi::OsStr>]) -> std::process::Output {
    let mut cmd = Command::new(BIN);
    cmd.arg("vcs").env("LEX_SYS_CACHE", cache);
    for a in args {
        cmd.arg(a.as_ref());
    }
    cmd.output().expect("the compiler runs")
}

fn ok(output: &std::process::Output) -> String {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository holding `libx` (sources in `src/`, the published stores in
/// `.lex-sys-vcs/`), committed. Returns its path and the commit.
fn library_repo(tag: &str) -> (PathBuf, PathBuf, String) {
    let work = scratch(tag);
    let repo = work.join("libx");
    write(&repo.join("src"), "base.ls", BASE);
    write(&repo.join("src"), "top.ls", TOP);
    git(&repo, &["init", "--quiet"]);
    let cache = work.join("cache");
    ok(&vcs(
        &cache,
        &[&"publish", &"--dir", &repo.join("src"), &"--store", &repo.join(".lex-sys-vcs")],
    ));
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "libx"]);
    let rev = git(&repo, &["rev-parse", "HEAD"]);
    (repo, cache, rev)
}

fn lock_top(cache: &Path, repo: &Path, rev: &str, out: &Path) -> std::process::Output {
    vcs(
        cache,
        &[
            &"lock",
            &"--git",
            &repo,
            &"--rev",
            &rev,
            &"--path",
            &".lex-sys-vcs/libx.top",
            &"-o",
            &out,
            &"--all",
        ],
    )
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    files.sort();
    files
}

// ---- §7.4: publishing a directory -------------------------------------

#[test]
fn a_directory_with_a_static_and_an_import_publishes_in_dependency_order() {
    let work = scratch("vcs-dir-order");
    let src = work.join("src");
    // Module names and file names that sort the wrong way round: `alib.top`
    // comes first alphabetically and imports `zlib.base`.
    let base = BASE.replace("libx.base", "zlib.base");
    let top = TOP.replace("libx.top", "alib.top").replace("libx.base", "zlib.base");
    write(&src, "a_top.ls", &top);
    write(&src, "b_base.ls", &base);
    let store = work.join("stores");
    let stdout = ok(&vcs(&work, &[&"publish", &"--dir", &src, &"--store", &store]));

    let base_at = stdout.find("== zlib.base").expect("the base is published");
    let top_at = stdout.find("== alib.top").expect("the top is published");
    assert!(base_at < top_at, "a module is published after what it imports:\n{stdout}");
    assert!(
        stdout.contains("published table"),
        "a static is published like any declaration: {stdout}"
    );
    assert!(stdout.contains("published twice"), "{stdout}");
    assert!(store.join("alib.top").join("requires").is_dir(), "the import became a requirement");

    // The closure holds together.
    let resolved = ok(&vcs(&work, &[&"resolve", &store.join("alib.top")]));
    assert!(resolved.contains("verified transitively"), "{resolved}");
}

#[test]
fn publishing_a_directory_twice_writes_the_same_bytes_and_survives_an_edit() {
    let work = scratch("vcs-dir-determinism");
    let src = work.join("src");
    write(&src, "base.ls", BASE);
    write(&src, "top.ls", TOP);
    let (a, b) = (work.join("a"), work.join("b"));
    ok(&vcs(&work, &[&"publish", &"--dir", &src, &"--store", &a]));
    ok(&vcs(&work, &[&"publish", &"--dir", &src, &"--store", &b]));
    let tree = |root: &Path| -> Vec<(PathBuf, Vec<u8>)> {
        let mut all = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in files_in(&d) {
                if e.is_dir() {
                    stack.push(e);
                } else {
                    all.push((
                        e.strip_prefix(root).unwrap().to_path_buf(),
                        std::fs::read(&e).unwrap(),
                    ));
                }
            }
        }
        all.sort();
        all
    };
    assert_eq!(tree(&a), tree(&b), "the same source publishes to the same bytes");

    // An edit to a declaration is refused by a plain `publish` (incremental
    // publish is not built) and must not be by a directory publish.
    write(&src, "base.ls", &BASE.replace("t[0] = 10;", "t[0] = 11;"));
    ok(&vcs(&work, &[&"publish", &"--dir", &src, &"--store", &a]));
    let resolved = ok(&vcs(&work, &[&"resolve", &a.join("libx.top")]));
    assert!(resolved.contains("still resolve"), "{resolved}");
}

#[test]
fn a_directory_refuses_an_import_it_cannot_place_and_a_cycle() {
    let work = scratch("vcs-dir-refusals");
    let src = work.join("src");
    write(&src, "top.ls", TOP); // imports libx.base, which is not here
    let out = vcs(&work, &[&"publish", &"--dir", &src, &"--store", &work.join("s")]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("libx.base") && err.contains("neither `std` nor a module"), "{err}");

    let cyc = work.join("cyc");
    write(&cyc, "p.ls", "module p;\nimport q;\npub fn f() -> [] int { return 1; }\n");
    write(&cyc, "q.ls", "module q;\nimport p;\npub fn g() -> [] int { return 2; }\n");
    let out = vcs(&work, &[&"publish", &"--dir", &cyc, &"--store", &work.join("s2")]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("import each other"), "a cycle is named");
}

#[test]
fn a_program_in_the_directory_is_skipped_not_published() {
    let work = scratch("vcs-dir-program");
    let src = work.join("src");
    write(&src, "base.ls", BASE);
    write(&src, "prog.ls", "fn main(world: World) -> [] int { return 0; }\n");
    let stdout = ok(&vcs(&work, &[&"publish", &"--dir", &src, &"--store", &work.join("s")]));
    assert!(stdout.contains("skipped") && stdout.contains("prog.ls"), "{stdout}");
    assert!(!work.join("s").join("main").exists());
}

// ---- §7.3: an origin on the lock --------------------------------------

#[test]
fn a_lock_with_an_origin_fetches_the_closure_from_a_git_repository_and_it_builds() {
    let (repo, cache, rev) = library_repo("vcs-origin-e2e");
    let lock = cache.parent().unwrap().join("top.lock");
    let stdout = ok(&lock_top(&cache, &repo, &rev, &lock));
    assert!(stdout.contains("locked twice"), "{stdout}");

    let text = std::fs::read_to_string(&lock).unwrap();
    assert!(text.contains("\"origin\"") && text.contains(&rev), "the lock records where: {text}");

    // No `--store`: the origin says where. The closure comes with it: `top`
    // requires `base`, in the same repository.
    let out = cache.parent().unwrap().join("fetched");
    let stdout = ok(&vcs(&cache, &[&"fetch", &"--lock", &lock, &"-o", &out]));
    assert_eq!(
        files_in(&out).len(),
        2,
        "the top and, through its requirement, the base:\n{stdout}"
    );
    assert!(
        cache.join("git").join(&rev).join(".lex-sys-vcs").is_dir()
            && !cache.join("git").join(&rev).join(".git").exists(),
        "the cache holds the tree, not a repository"
    );

    let consumer = write(
        &cache.parent().unwrap().join("app"),
        "app.ls",
        "import libx.top;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args } = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             return top.twice(2);\n\
         }\n",
    );
    let exe = cache.parent().unwrap().join("app").join("app");
    let mut build = Command::new(BIN);
    build.arg("build").arg(&consumer);
    for f in files_in(&out) {
        build.arg(f);
    }
    let build = build.arg("-o").arg(&exe).output().expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    let run = Command::new(&exe).output().expect("the program runs");
    assert_eq!(run.status.code(), Some(60), "twice(2) is pick(2) * 2 = 60");
}

#[test]
fn a_second_fetch_is_served_from_the_cache_without_git() {
    let (repo, cache, rev) = library_repo("vcs-origin-offline");
    let work = cache.parent().unwrap().to_path_buf();
    let lock = work.join("top.lock");
    ok(&lock_top(&cache, &repo, &rev, &lock));

    // Delete the repository and take `git` off the path: only the cache is left.
    std::fs::remove_dir_all(&repo).unwrap();
    let out = work.join("fetched");
    let output = Command::new(BIN)
        .args(["vcs", "fetch", "--lock"])
        .arg(&lock)
        .arg("-o")
        .arg(&out)
        .env("LEX_SYS_CACHE", &cache)
        .env("PATH", work.join("no-such-dir"))
        .output()
        .expect("the compiler runs");
    ok(&output);
    assert_eq!(files_in(&out).len(), 2);
}

#[test]
fn a_cached_checkout_that_was_changed_is_refused_and_nothing_is_written() {
    let (repo, cache, rev) = library_repo("vcs-origin-tamper");
    let work = cache.parent().unwrap().to_path_buf();
    let lock = work.join("top.lock");
    ok(&lock_top(&cache, &repo, &rev, &lock));

    let checkout = cache.join("git").join(&rev);
    let blob = files_in(&checkout.join(".lex-sys-vcs/libx.top/sources"))
        .into_iter()
        .find(|p| p.extension().is_some_and(|e| e == "ls"))
        .expect("the top's source blob");
    let text = std::fs::read_to_string(&blob).unwrap();
    std::fs::write(&blob, text.replace("* 2", "* 3")).unwrap();

    let out = work.join("fetched");
    let output = vcs(&cache, &[&"fetch", &"--lock", &lock, &"-o", &out]);
    assert!(!output.status.success(), "a changed source must not be accepted");
    assert!(files_in(&out).is_empty(), "nothing is written when a pin fails");
}

#[test]
fn a_lock_holds_a_commit_and_never_a_name() {
    let (repo, cache, rev) = library_repo("vcs-origin-pins");
    let work = cache.parent().unwrap().to_path_buf();
    let lock = work.join("top.lock");

    let by_name =
        vcs(&cache, &[&"lock", &"--git", &repo, &"--rev", &"master", &"-o", &lock, &"--all"]);
    assert!(!by_name.status.success(), "a branch name is not a pin");
    assert!(String::from_utf8_lossy(&by_name.stderr).contains("full commit hash"));

    // `--ref` is looked up once, and the hash is what is written.
    let branch = git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let by_ref = vcs(
        &cache,
        &[
            &"lock",
            &"--git",
            &repo,
            &"--ref",
            &branch,
            &"--path",
            &".lex-sys-vcs/libx.top",
            &"-o",
            &lock,
            &"--all",
        ],
    );
    ok(&by_ref);
    let text = std::fs::read_to_string(&lock).unwrap();
    assert!(text.contains(&rev) && !text.contains(&format!("\"rev\": \"{branch}\"")), "{text}");

    // A path that leaves the repository is not a store.
    for bad in ["../elsewhere", "/etc", "a/../../b", ""] {
        let out = vcs(
            &cache,
            &[
                &"lock",
                &"--git",
                &repo,
                &"--rev",
                &rev,
                &"--path",
                &bad,
                &"-o",
                &work.join("p.lock"),
                &"--all",
            ],
        );
        assert!(!out.status.success(), "`--path {bad}` should be refused");
    }

    // A hand-edited lock that holds a name is refused where it is read.
    std::fs::write(&lock, text.replace(&rev, &branch)).unwrap();
    let read = vcs(&cache, &[&"fetch", &"--lock", &lock, &"-o", &work.join("o")]);
    assert!(!read.status.success());
    assert!(String::from_utf8_lossy(&read.stderr).contains("bad origin"), "{:?}", read);
}

#[test]
fn moving_the_branch_changes_nothing_a_lock_already_pins() {
    let (repo, cache, rev) = library_repo("vcs-origin-moved");
    let work = cache.parent().unwrap().to_path_buf();
    let lock = work.join("top.lock");
    ok(&lock_top(&cache, &repo, &rev, &lock));

    // The library moves on: an edit, republished and committed.
    write(&repo.join("src"), "base.ls", &BASE.replace("t[2] = 30;", "t[2] = 99;"));
    ok(&vcs(
        &cache,
        &[&"publish", &"--dir", &repo.join("src"), &"--store", &repo.join(".lex-sys-vcs")],
    ));
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "moved"]);

    let out = work.join("fetched");
    ok(&vcs(&cache, &[&"fetch", &"--lock", &lock, &"-o", &out]));
    let all: String = files_in(&out).iter().map(|f| std::fs::read_to_string(f).unwrap()).collect();
    assert!(
        all.contains("t[2] = 30;") && !all.contains("t[2] = 99;"),
        "the pinned commit, not the head"
    );
}

#[test]
fn an_explicit_store_wins_over_the_origin() {
    let (repo, cache, rev) = library_repo("vcs-origin-override");
    let work = cache.parent().unwrap().to_path_buf();
    let lock = work.join("top.lock");
    ok(&lock_top(&cache, &repo, &rev, &lock));
    let out = work.join("fetched");
    // Same store, named: works with no cache and no git.
    let output = Command::new(BIN)
        .args(["vcs", "fetch", "--lock"])
        .arg(&lock)
        .arg("--store")
        .arg(repo.join(".lex-sys-vcs/libx.top"))
        .arg("-o")
        .arg(&out)
        .env("LEX_SYS_CACHE", work.join("empty-cache"))
        .env("PATH", work.join("no-such-dir"))
        .output()
        .expect("the compiler runs");
    ok(&output);
    assert_eq!(files_in(&out).len(), 2);
}

#[test]
fn a_package_in_another_repository_requires_through_the_origin() {
    // `libx` is one repository; `liby` is another and requires `libx.top`
    // by a lock that has an origin, with no store path that would mean
    // anything inside `liby`.
    let (repo, cache, rev) = library_repo("vcs-origin-closure");
    let work = cache.parent().unwrap().to_path_buf();
    let libx_lock = work.join("libx.lock");
    ok(&lock_top(&cache, &repo, &rev, &libx_lock));

    let liby = work.join("liby");
    write(
        &liby,
        "y.ls",
        "module liby;\nimport libx.top;\npub fn thrice(i: int) -> [] int { return top.twice(i) + top.twice(i) / 2; }\n",
    );
    let ystore = liby.join(".lex-sys-vcs");
    // `--requires <lock>` alone: the lock says where the store is.
    ok(&vcs(
        &cache,
        &[&"publish", &"--store", &ystore, &"--requires", &libx_lock, &liby.join("y.ls")],
    ));
    let recorded = std::fs::read_to_string(ystore.join("requires/0.json")).unwrap();
    assert!(recorded.contains("\"origin\"") && recorded.contains(&rev));
    assert!(
        recorded.contains("\"store\": \"\""),
        "no machine-specific path is recorded: {recorded}"
    );

    git(&liby, &["init", "--quiet"]);
    git(&liby, &["add", "-A"]);
    git(&liby, &["commit", "--quiet", "-m", "liby"]);
    let yrev = git(&liby, &["rev-parse", "HEAD"]);

    // A consumer on a machine with an empty cache, and the library repo gone
    // from its original path: everything comes from the two origins.
    let fresh = work.join("fresh-cache");
    let ylock = work.join("liby.lock");
    ok(&vcs(&fresh, &[&"lock", &"--git", &liby, &"--rev", &yrev, &"-o", &ylock, &"--all"]));
    let out = work.join("fetched");
    ok(&vcs(&fresh, &[&"fetch", &"--lock", &ylock, &"-o", &out]));
    assert_eq!(files_in(&out).len(), 3, "liby, and libx's top and base from the other repository");
}

#[test]
fn a_fetch_that_fails_leaves_no_half_made_checkout() {
    let (repo, cache, _rev) = library_repo("vcs-origin-failed");
    let nowhere = "0123456789abcdef0123456789abcdef01234567";
    let out = lock_top(&cache, &repo, nowhere, &cache.parent().unwrap().join("x.lock"));
    assert!(!out.status.success(), "a commit the repository does not have cannot be fetched");
    assert!(
        !cache.join("git").join(nowhere).exists(),
        "a directory in the cache is a complete checkout or it is not there"
    );
    let leftovers: Vec<_> = files_in(&cache.join("git"))
        .into_iter()
        .filter(|p| p.to_string_lossy().contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty(), "no temporary directory is left behind: {leftovers:?}");
}

#[test]
fn a_lock_pins_one_store_and_will_not_be_extended_with_another() {
    let (repo, cache, rev) = library_repo("vcs-origin-one-store");
    let work = cache.parent().unwrap().to_path_buf();
    let lock = work.join("top.lock");
    ok(&lock_top(&cache, &repo, &rev, &lock));

    // The same library, one commit later: a different origin.
    write(&repo.join("src"), "base.ls", &BASE.replace("t[3] = 40;", "t[3] = 41;"));
    ok(&vcs(
        &cache,
        &[&"publish", &"--dir", &repo.join("src"), &"--store", &repo.join(".lex-sys-vcs")],
    ));
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "later"]);
    let later = git(&repo, &["rev-parse", "HEAD"]);
    let before = std::fs::read_to_string(&lock).unwrap();
    let out = lock_top(&cache, &repo, &later, &lock);
    assert!(!out.status.success(), "a lock that pins one commit is not quietly moved to another");
    assert!(String::from_utf8_lossy(&out.stderr).contains("already pins a different store"));
    assert_eq!(std::fs::read_to_string(&lock).unwrap(), before, "and is left as it was");
}

#[test]
fn a_hook_in_the_template_directory_does_not_run() {
    let (repo, cache, rev) = library_repo("vcs-origin-hooks");
    let work = cache.parent().unwrap().to_path_buf();
    let template = work.join("template");
    let marker = work.join("hook-ran");
    let hooks = template.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let output = Command::new(BIN)
        .args(["vcs", "lock", "--git"])
        .arg(&repo)
        .args(["--rev", &rev, "--path", ".lex-sys-vcs/libx.top", "-o"])
        .arg(work.join("h.lock"))
        .arg("--all")
        .env("LEX_SYS_CACHE", &cache)
        .env("GIT_TEMPLATE_DIR", &template)
        .output()
        .expect("the compiler runs");
    ok(&output);
    assert!(!marker.exists(), "git ran a hook from the dependency's checkout");
}

#[test]
fn only_honest_transports_are_used() {
    let work = scratch("vcs-origin-transports");
    let cache = work.join("cache");
    let marker = work.join("ran");
    // A listener standing in for a cleartext server: a connection to it is the failure.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    for url in
        [format!("ext::sh -c 'touch {}'", marker.display()), format!("http://127.0.0.1:{port}/r")]
    {
        let out = vcs(
            &cache,
            &[
                &"lock",
                &"--git",
                &url,
                &"--rev",
                &"0123456789abcdef0123456789abcdef01234567",
                &"-o",
                &work.join("t.lock"),
                &"--all",
            ],
        );
        assert!(!out.status.success(), "`{url}` should be refused");
    }
    assert!(!marker.exists(), "an `ext::` url ran its command");
    assert!(listener.accept().is_err(), "git connected over cleartext http");
}
