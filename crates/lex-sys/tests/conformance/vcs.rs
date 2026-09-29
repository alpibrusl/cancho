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

// ---------------------------------------------------------------------
// `lex-sys vcs resolve` (`docs/package-system.md` §4.2/§6) -- a real
// second store, never trusted from its own lock alone.
// ---------------------------------------------------------------------

fn publish(store: &Path, file: &Path) {
    let output = Command::new(BIN)
        .args(["vcs", "publish", "--store"])
        .arg(store)
        .arg(file)
        .output()
        .expect("the compiler runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn resolve(store: &Path) -> std::process::Output {
    Command::new(BIN).args(["vcs", "resolve"]).arg(store).output().expect("the compiler runs")
}

#[test]
fn resolve_confirms_a_clean_store_matches_exactly() {
    let dir = scratch("vcs-resolve-clean");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    let output = resolve(&store);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2 declaration(s)"), "{stdout}");
    assert!(stdout.contains("still resolve exactly as published"), "{stdout}");
}

#[test]
fn resolve_against_an_empty_store_says_so_rather_than_erroring() {
    let dir = scratch("vcs-resolve-empty");
    let output = resolve(&dir.join("store"));
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("nothing published"));
}

#[test]
fn resolve_refuses_a_pin_that_no_longer_matches_its_own_source() {
    let dir = scratch("vcs-resolve-drifted");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    // Hand-corrupt one pin's `stage_id` without touching the source blob
    // it points at -- exactly what a stale or hand-edited lock looks like,
    // the case `docs/package-system.md` §4.2 says a resolver must never
    // trust silently.
    let manifest_path = store.join("manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).expect("a readable manifest");
    let real_stage_id = "b".repeat(64);
    assert!(
        !manifest.contains(&real_stage_id),
        "the sentinel stage_id must not collide with a real one"
    );
    // `sub`'s own stage_id, from `SOURCE` -- swapped for a well-formed but
    // wrong one.
    let (_, after_sub) = manifest.split_once("\"name\": \"sub\"").expect("sub is in the manifest");
    let start = after_sub.find("\"stage_id\": \"").expect("sub has a stage_id") + 13;
    let real = &after_sub[start..start + 64];
    let corrupted = manifest.replacen(real, &real_stage_id, 1);
    std::fs::write(&manifest_path, corrupted).expect("a writable manifest");

    let output = resolve(&store);
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sub"), "{stderr}");
    assert!(stderr.contains("no longer matches"), "{stderr}");
}

#[test]
fn resolve_refuses_source_that_no_longer_typechecks() {
    let dir = scratch("vcs-resolve-broken");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    // A source blob that is self-consistent (its name really is its own
    // hash, so `Blobs::get` accepts it) but does not type-check under
    // today's compiler -- what `docs/hash-stability.md`'s 71% figure
    // means in practice, constructed directly rather than waited for.
    let broken = "fn add(a: int, b: int) -> [] int { return a + unknown_name; }\n\
                  fn sub(a: int, b: int) -> [] int { return a - b; }\n";
    let blobs = lex_sys_vcs::Blobs::open(&store).expect("the store's sources/ dir opens");
    let broken_hash = blobs.put(broken).expect("a writable blob store");

    let manifest_path = store.join("manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).expect("a readable manifest");
    let (_, after_add) = manifest.split_once("\"name\": \"add\"").expect("add is in the manifest");
    let start = after_add.find("\"source_hash\": \"").expect("add has a source_hash") + 16;
    let real_hash = &after_add[start..start + 64];
    let corrupted = manifest.replacen(real_hash, &broken_hash, 1);
    std::fs::write(&manifest_path, corrupted).expect("a writable manifest");

    let output = resolve(&store);
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no longer type-checks"), "{stderr}");
    assert!(stderr.contains("unknown-name"), "{stderr}");
}

#[test]
fn resolve_refuses_a_pin_whose_source_blob_is_missing() {
    let dir = scratch("vcs-resolve-missing-blob");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    for entry in std::fs::read_dir(store.join("sources")).expect("sources/ exists") {
        std::fs::remove_file(entry.expect("a readable entry").path()).expect("a removable blob");
    }

    let output = resolve(&store);
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stdout));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no source blob recorded"));
}

// ---------------------------------------------------------------------
// `lex-sys vcs lock` and `vcs resolve --lock` (`docs/package-system.md`
// §4.5) -- a name chosen once, resolved by hash forever after.
// ---------------------------------------------------------------------

fn lock(store: &Path, out: &Path, names: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(["vcs", "lock", "--store"])
        .arg(store)
        .arg("-o")
        .arg(out)
        .args(names)
        .output()
        .expect("the compiler runs")
}

fn resolve_locked(lock_file: &Path, store: &Path) -> std::process::Output {
    Command::new(BIN)
        .args(["vcs", "resolve", "--lock"])
        .arg(lock_file)
        .arg(store)
        .output()
        .expect("the compiler runs")
}

#[test]
fn lock_pins_a_name_to_its_current_hashes() {
    let dir = scratch("vcs-lock-basic");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    let lock_file = dir.join("lex-sys.lock");
    let output = lock(&store, &lock_file, &["add"]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("locked add"));

    let contents = std::fs::read_to_string(&lock_file).expect("a readable lock file");
    assert!(contents.contains("\"add\""), "{contents}");
    assert!(!contents.contains("\"sub\""), "only the locked name should appear: {contents}");
}

#[test]
fn locking_an_unpublished_name_is_refused() {
    let dir = scratch("vcs-lock-unknown");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    let output = lock(&store, &dir.join("lex-sys.lock"), &["nonexistent"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not published"));
}

#[test]
fn resolve_with_lock_scopes_to_just_the_locked_names() {
    let dir = scratch("vcs-resolve-lock-scoped");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    let lock_file = dir.join("lex-sys.lock");
    assert!(lock(&store, &lock_file, &["add"]).status.success());

    let output = resolve_locked(&lock_file, &store);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 declaration(s)"), "only `add` was locked: {stdout}");
}

#[test]
fn resolve_with_lock_against_a_store_that_never_had_it_is_refused() {
    let dir = scratch("vcs-resolve-lock-elsewhere");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);
    let lock_file = dir.join("lex-sys.lock");
    assert!(lock(&store, &lock_file, &["add"]).status.success());

    // A second, unrelated store that never published `add` at all.
    let other_file = write_source(&dir, "fn nine() -> [] int { return 9; }\n");
    let other_store = dir.join("other-store");
    publish(&other_store, &other_file);

    let output = resolve_locked(&lock_file, &other_store);
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("add"), "{stderr}");
    assert!(stderr.contains("no longer published"), "{stderr}");
}

#[test]
fn resolve_with_an_unwritten_lock_file_says_so_rather_than_erroring() {
    let dir = scratch("vcs-resolve-lock-empty");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    let output = resolve_locked(&dir.join("never-written.lock"), &store);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("nothing locked"));
}

// ---------------------------------------------------------------------
// `lex-sys vcs fetch` (`docs/package-system.md` §6) -- real bytes on
// disk, so a locked dependency is buildable without the compiler's own
// module resolution changing at all.
// ---------------------------------------------------------------------

fn fetch(lock_file: &Path, store: &Path, out: &Path) -> std::process::Output {
    Command::new(BIN)
        .args(["vcs", "fetch", "--lock"])
        .arg(lock_file)
        .arg("--store")
        .arg(store)
        .arg("-o")
        .arg(out)
        .output()
        .expect("the compiler runs")
}

#[test]
fn fetch_materializes_a_verified_file_a_real_program_can_import() {
    let dir = scratch("vcs-fetch-e2e");
    let dep_file = write_source(&dir, "module dep;\npub fn seven() -> [] int { return 7; }\n");
    let store = dir.join("store");
    publish(&store, &dep_file);

    let lock_file = dir.join("lex-sys.lock");
    assert!(lock(&store, &lock_file, &["seven"]).status.success());

    let fetch_dir = dir.join("fetched");
    let output = fetch(&lock_file, &store, &fetch_dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("fetched"));

    let fetched_files: Vec<PathBuf> = std::fs::read_dir(&fetch_dir)
        .expect("the fetch dir exists")
        .map(|e| e.expect("a readable entry").path())
        .collect();
    assert_eq!(fetched_files.len(), 1, "one distinct source file was locked");

    // `docs/modules.md` §4.2's own claim, proved rather than assumed: an
    // `import` is a rule for resolving a name against whatever files are
    // on the command line, not an instruction to go and read something
    // -- so nothing in the compiler needs to change for a fetched
    // dependency to be `import`-able, only the file needs to exist.
    let consumer = write_source(
        &dir,
        "import dep;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args } = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             return dep.seven();\n\
         }\n",
    );
    let exe = dir.join("consumer");
    let build = Command::new(BIN)
        .args([
            "build".as_ref(),
            consumer.as_os_str(),
            fetched_files[0].as_os_str(),
            "-o".as_ref(),
            exe.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    let run = Command::new(&exe).output().expect("the compiled program runs");
    assert_eq!(run.status.code(), Some(7), "main should return dep.seven()'s own 7");
}

#[test]
fn fetch_deduplicates_a_file_shared_by_two_locked_names() {
    let dir = scratch("vcs-fetch-dedup");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    let lock_file = dir.join("lex-sys.lock");
    assert!(lock(&store, &lock_file, &["add", "sub"]).status.success());

    let fetch_dir = dir.join("fetched");
    let output = fetch(&lock_file, &store, &fetch_dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let fetched_files: Vec<_> =
        std::fs::read_dir(&fetch_dir).expect("the fetch dir exists").collect();
    assert_eq!(
        fetched_files.len(),
        1,
        "add and sub came from the same file; fetch should write it once"
    );
}

#[test]
fn fetch_writes_nothing_when_a_pin_no_longer_matches() {
    let dir = scratch("vcs-fetch-drifted");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);
    let lock_file = dir.join("lex-sys.lock");
    assert!(lock(&store, &lock_file, &["add"]).status.success());

    let manifest_path = store.join("manifest.json");
    let manifest = std::fs::read_to_string(&manifest_path).expect("a readable manifest");
    let bogus_stage_id = "b".repeat(64);
    assert!(!manifest.contains(&bogus_stage_id));
    let (_, after_add) = manifest.split_once("\"name\": \"add\"").expect("add is in the manifest");
    let start = after_add.find("\"stage_id\": \"").expect("add has a stage_id") + 13;
    let real = &after_add[start..start + 64];
    let corrupted = manifest.replacen(real, &bogus_stage_id, 1);
    std::fs::write(&manifest_path, corrupted).expect("a writable manifest");

    let fetch_dir = dir.join("fetched");
    let output = fetch(&lock_file, &store, &fetch_dir);
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stdout));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no longer matches"));
    assert!(!fetch_dir.exists(), "a refused fetch must not partially write");
}

#[test]
fn fetch_with_an_unwritten_lock_file_says_so_rather_than_erroring() {
    let dir = scratch("vcs-fetch-empty-lock");
    let file = write_source(&dir, SOURCE);
    let store = dir.join("store");
    publish(&store, &file);

    let output = fetch(&dir.join("never-written.lock"), &store, &dir.join("fetched"));
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("nothing locked"));
}
