//! `docs/package-system.md` §7: a published package may import `std`.
//!
//! Through the compiled binary against stores on disk, like `vcs.rs`: the
//! claims here are about what `publish`, `resolve` and `fetch` do with the
//! library, and what a consumer then builds.

use super::*;

/// Its own `abs` next to `std.math`'s: the one name both define, so a
/// publish that attributed by name alone would either skip `pkg.abs` or
/// republish `std.math.abs` under the package's store.
const PKG: &str = "\
module pkg;
import std.math;
pub fn abs(n: int) -> [] int { return math.abs(n) + 100; }
pub fn clamp3(n: int) -> [] int { return math.min(n, 3); }
";

const CONSUMER: &str = "\
import pkg;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return pkg.abs(0 - 5) + pkg.clamp3(9);
}
";

fn vcs(args: &[&str], paths: &[&Path]) -> std::process::Output {
    let mut cmd = Command::new(BIN);
    cmd.arg("vcs").args(args);
    for p in paths {
        cmd.arg(p);
    }
    cmd.output().expect("the compiler runs")
}

fn stderr(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// Publish `PKG` with `--std` into `<dir>/store`.
fn publish_pkg(dir: &Path) -> PathBuf {
    let file = dir.join("pkg.cho");
    std::fs::write(&file, PKG).expect("a writable scratch file");
    let store = dir.join("store");
    let out = vcs(&["publish", "--std", "--store"], &[&store, &file]);
    assert!(out.status.success(), "{}", stderr(&out));
    store
}

#[test]
fn importing_std_without_the_flag_is_refused_and_says_what_to_pass() {
    let dir = scratch("vcs-std-refused");
    let file = dir.join("pkg.cho");
    std::fs::write(&file, PKG).expect("a writable scratch file");
    let store = dir.join("store");
    let out = vcs(&["publish", "--store"], &[&store, &file]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(stderr(&out).contains("--std"), "the refusal should name the flag: {}", stderr(&out));
    assert!(!store.join("manifest.json").exists(), "a refused publish must write nothing");
}

#[test]
fn publish_with_std_logs_only_the_packages_own_functions() {
    let dir = scratch("vcs-std-own-only");
    let store = publish_pkg(&dir);

    let ops: Vec<_> = std::fs::read_dir(store.join("ops")).expect("ops/ exists").collect();
    assert_eq!(ops.len(), 2, "pkg.abs and pkg.clamp3, none of std's own");

    let log = vcs(&["log", "--store"], &[&store]);
    let text = stdout(&log);
    assert!(text.contains("abs"), "{text}");
    assert!(text.contains("clamp3"), "{text}");
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.lines().all(|l| l.ends_with("std")), "each row should be marked std: {text}");
}

#[test]
fn a_std_package_resolves_locks_fetches_and_a_consumer_runs_it() {
    let dir = scratch("vcs-std-e2e");
    let store = publish_pkg(&dir);

    let resolved = vcs(&["resolve"], &[&store]);
    assert!(resolved.status.success(), "{}", stderr(&resolved));

    let lock_file = dir.join("cancho.lock");
    let locked = Command::new(BIN)
        .args(["vcs", "lock", "--store"])
        .arg(&store)
        .arg("-o")
        .arg(&lock_file)
        .args(["abs", "clamp3"])
        .output()
        .expect("the compiler runs");
    assert!(locked.status.success(), "{}", stderr(&locked));

    let fetch_dir = dir.join("fetched");
    let fetched = Command::new(BIN)
        .args(["vcs", "fetch", "--lock"])
        .arg(&lock_file)
        .arg("--store")
        .arg(&store)
        .arg("-o")
        .arg(&fetch_dir)
        .output()
        .expect("the compiler runs");
    assert!(fetched.status.success(), "{}", stderr(&fetched));
    assert!(stdout(&fetched).contains("--std"), "fetch should say the consumer needs it");

    let files: Vec<PathBuf> = std::fs::read_dir(&fetch_dir)
        .expect("the fetch dir exists")
        .map(|e| e.expect("a readable entry").path())
        .collect();
    assert_eq!(files.len(), 1, "std is the compiler's, never fetched into the directory");

    let consumer = dir.join("consumer.cho");
    std::fs::write(&consumer, CONSUMER).expect("a writable scratch file");
    let exe = dir.join("consumer");
    let build = Command::new(BIN)
        .arg("build")
        .arg("--std")
        .arg(&consumer)
        .arg(&files[0])
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", stderr(&build));
    let run = Command::new(&exe).output().expect("the compiled program runs");
    // pkg.abs(-5) = 5 + 100; pkg.clamp3(9) = 3.
    assert_eq!(run.status.code(), Some(108));

    // And without `--std` the same consumer is refused, rather than
    // building against something that is not there.
    let bare = Command::new(BIN)
        .arg("build")
        .arg(&consumer)
        .arg(&files[0])
        .arg("-o")
        .arg(dir.join("bare"))
        .output()
        .expect("the compiler runs");
    assert!(!bare.status.success());
}

#[test]
fn resolve_detects_std_from_the_source_not_from_the_manifest_flag() {
    let dir = scratch("vcs-std-flag-forged");
    let store = publish_pkg(&dir);

    // Claim the package is std-free. A check satisfiable by editing a flag
    // would now fail to find `std.math` and refuse -- or worse, pass for
    // the wrong reason. It must keep verifying against the library.
    let path = store.join("manifest.json");
    let manifest = std::fs::read_to_string(&path).expect("a readable manifest");
    assert!(manifest.contains("\"uses_std\": true"), "{manifest}");
    std::fs::write(&path, manifest.replace("\"uses_std\": true", "\"uses_std\": false"))
        .expect("a writable manifest");

    let resolved = vcs(&["resolve"], &[&store]);
    assert!(resolved.status.success(), "{}", stderr(&resolved));
}

#[test]
fn a_manifest_written_before_the_flag_existed_still_loads() {
    let dir = scratch("vcs-std-old-manifest");
    let file = dir.join("f.cho");
    std::fs::write(&file, "fn add(a: int, b: int) -> [] int { return a + b; }\n")
        .expect("a writable scratch file");
    let store = dir.join("store");
    assert!(vcs(&["publish", "--store"], &[&store, &file]).status.success());

    let path = store.join("manifest.json");
    let manifest = std::fs::read_to_string(&path).expect("a readable manifest");
    let stripped: String =
        manifest.lines().filter(|l| !l.contains("uses_std")).collect::<Vec<_>>().join("\n");
    assert!(!stripped.contains("uses_std"));
    // The line before `uses_std` ends in a comma; drop it so the JSON is valid.
    let fixed = stripped.replace(",\n    }", "\n    }");
    std::fs::write(&path, fixed).expect("a writable manifest");

    let log = vcs(&["log", "--store"], &[&store]);
    assert!(log.status.success(), "{}{}", stdout(&log), stderr(&log));
    assert!(stdout(&log).contains("add"));
}

#[test]
fn a_package_requiring_a_std_package_must_be_published_with_std_too() {
    let dir = scratch("vcs-std-transitive");
    let dep_store = publish_pkg(&dir);
    let lock_file = dir.join("dep.lock");
    let locked = Command::new(BIN)
        .args(["vcs", "lock", "--store"])
        .arg(&dep_store)
        .arg("-o")
        .arg(&lock_file)
        .arg("clamp3")
        .output()
        .expect("the compiler runs");
    assert!(locked.status.success(), "{}", stderr(&locked));

    // This file never mentions `std`; its dependency does.
    let top = dir.join("top.cho");
    std::fs::write(
        &top,
        "module top;\nimport pkg;\npub fn capped(n: int) -> [] int { return pkg.clamp3(n); }\n",
    )
    .expect("a writable scratch file");
    let top_store = dir.join("top-store");
    let requires = format!("{}:{}", lock_file.display(), dep_store.display());

    let without = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(&top_store)
        .arg("--requires")
        .arg(&requires)
        .arg(&top)
        .output()
        .expect("the compiler runs");
    assert_eq!(without.status.code(), Some(1), "{}", stdout(&without));
    assert!(stderr(&without).contains("--std"), "{}", stderr(&without));

    let with = Command::new(BIN)
        .args(["vcs", "publish", "--std", "--store"])
        .arg(&top_store)
        .arg("--requires")
        .arg(&requires)
        .arg(&top)
        .output()
        .expect("the compiler runs");
    assert!(with.status.success(), "{}", stderr(&with));
    assert!(stdout(&with).contains("published capped"));

    // Resolving the dependent walks its `requires/` into a blob that
    // imports `std` -- the closure brings the library in on its own.
    let resolved = vcs(&["resolve"], &[&top_store]);
    assert!(resolved.status.success(), "{}", stderr(&resolved));
}
