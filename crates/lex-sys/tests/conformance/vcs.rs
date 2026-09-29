//! `docs/vcs-publish.md`: `lex-sys vcs publish`/`log` against a real store
//! on disk, through the compiled binary rather than the library directly
//! -- the same reason every other conformance test here shells out to
//! `BIN` instead of calling `lex_sys_ir` in-process.

use super::*;

const SOURCE: &str = "\
fn add(a: int, b: int) -> [] int { return a + b; }
fn sub(a: int, b: int) -> [] int { return a - b; }
";

fn write_source(dir: &Path, text: &str) -> PathBuf {
    let path = dir.join("f.ls");
    std::fs::write(&path, text).expect("a writable scratch file");
    path
}

#[test]
fn publish_logs_every_declaration_once() {
    let dir = scratch("vcs-publish-once");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");

    let output = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(&store)
        .arg(&file)
        .output()
        .expect("the compiler runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("published add"), "{stdout}");
    assert!(stdout.contains("published sub"), "{stdout}");

    assert!(store.join("manifest.json").is_file(), "publish should write a manifest");
    let ops: Vec<_> = std::fs::read_dir(store.join("ops"))
        .expect("publish should create ops/")
        .map(|e| e.expect("a readable entry").path())
        .collect();
    assert_eq!(ops.len(), 2, "one AddFunction per declaration, found {ops:?}");
}

#[test]
fn publishing_the_same_declarations_again_is_a_no_op() {
    let dir = scratch("vcs-publish-idempotent");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");

    let first = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(&store)
        .arg(&file)
        .output()
        .expect("the compiler runs");
    assert!(first.status.success());

    let second = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(&store)
        .arg(&file)
        .output()
        .expect("the compiler runs");
    assert!(second.status.success());
    let stdout = String::from_utf8_lossy(&second.stdout);
    assert!(
        stdout.contains("nothing new"),
        "a second publish of the same source should not re-log anything: {stdout}"
    );

    let ops: Vec<_> = std::fs::read_dir(store.join("ops")).expect("ops/ exists").collect();
    assert_eq!(ops.len(), 2, "no new op files from the second, unchanged publish");
}

#[test]
fn changing_an_already_published_body_is_refused_not_silently_logged() {
    let dir = scratch("vcs-publish-changed");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");

    let first = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(&store)
        .arg(&file)
        .output()
        .expect("the compiler runs");
    assert!(first.status.success());

    // `add`'s body changes; `sub` does not.
    write_source(
        &dir,
        "fn add(a: int, b: int) -> [] int { return a + b + 1; }\n\
         fn sub(a: int, b: int) -> [] int { return a - b; }\n",
    );

    let second = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(&store)
        .arg(&file)
        .output()
        .expect("the compiler runs");
    assert_eq!(
        second.status.code(),
        Some(1),
        "an incremental change is refused, not silently re-logged: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert!(String::from_utf8_lossy(&second.stderr).contains("already published"));

    // Nothing from the refused run was written.
    let ops: Vec<_> = std::fs::read_dir(store.join("ops")).expect("ops/ exists").collect();
    assert_eq!(ops.len(), 2, "a refused publish must not partially write");
}

#[test]
fn log_lists_what_a_store_already_has() {
    let dir = scratch("vcs-log");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");

    let publish = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(&store)
        .arg(&file)
        .output()
        .expect("the compiler runs");
    assert!(publish.status.success());

    let log = Command::new(BIN)
        .args(["vcs", "log", "--store"])
        .arg(&store)
        .output()
        .expect("the compiler runs");
    assert!(log.status.success());
    let stdout = String::from_utf8_lossy(&log.stdout);
    assert!(stdout.contains("add"), "{stdout}");
    assert!(stdout.contains("sub"), "{stdout}");
}

#[test]
fn log_against_an_empty_store_says_so_rather_than_erroring() {
    let dir = scratch("vcs-log-empty");
    let output = Command::new(BIN)
        .args(["vcs", "log", "--store"])
        .arg(dir.join("store"))
        .output()
        .expect("the compiler runs");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("nothing published yet"));
}

#[test]
fn publishing_a_type_error_is_refused_with_its_rule_not_a_panic() {
    let dir = scratch("vcs-publish-refused");
    let file = write_source(&dir, "fn broken(a: int) -> [] int { return a + \"nope\"; }\n");
    let output = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(dir.join("store"))
        .arg(&file)
        .output()
        .expect("the compiler runs");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("expected `int`"));
}
