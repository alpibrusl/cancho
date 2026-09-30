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

// ---------------------------------------------------------------------
// `docs/package-system.md` §4.6 -- a package that depends on a package.
// Synthetic, name-only modules throughout: the mechanism itself is what
// these prove, not any real package's own shape.
// ---------------------------------------------------------------------

fn write_module(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("a writable scratch file");
    path
}

fn publish_with_requires(
    store: &Path,
    file: &Path,
    requires: &[(&Path, &Path)],
) -> std::process::Output {
    let mut cmd = Command::new(BIN);
    cmd.args(["vcs", "publish", "--store"]).arg(store);
    for (lock_file, dep_store) in requires {
        cmd.arg("--requires");
        cmd.arg(format!("{}:{}", lock_file.display(), dep_store.display()));
    }
    cmd.arg(file);
    cmd.output().expect("the compiler runs")
}

#[test]
fn publishing_an_unresolved_import_is_refused_with_the_missing_module_named() {
    // The regression this whole section guards: without `--requires`,
    // a package that imports another package still refuses exactly the
    // way it did before this feature existed.
    let dir = scratch("vcs-requires-missing");
    let file = write_module(
        &dir,
        "dependent.ls",
        "module closure.dependent;\n\
         import closure.leaf;\n\
         pub fn dependent_fn() -> [] int { return leaf.leaf_fn() + 1; }\n",
    );
    let output = publish_with_requires(&dir.join("store"), &file, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no module `closure.leaf`"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn publish_with_requires_resolves_a_real_dependency_and_records_it() {
    let dir = scratch("vcs-requires-publish");
    let leaf = write_module(
        &dir,
        "leaf.ls",
        "module closure.leaf;\npub fn leaf_fn() -> [] int { return 1; }\n",
    );
    let leaf_store = dir.join("leaf-store");
    publish(&leaf_store, &leaf);

    let leaf_lock = dir.join("leaf.lock");
    assert!(lock(&leaf_store, &leaf_lock, &["leaf_fn"]).status.success());

    let dependent = write_module(
        &dir,
        "dependent.ls",
        "module closure.dependent;\n\
         import closure.leaf;\n\
         pub fn dependent_fn() -> [] int { return leaf.leaf_fn() + 1; }\n",
    );
    let dependent_store = dir.join("dependent-store");
    let output = publish_with_requires(&dependent_store, &dependent, &[(&leaf_lock, &leaf_store)]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("published dependent_fn"));

    let requires_dir = dependent_store.join("requires");
    let files: Vec<_> = std::fs::read_dir(&requires_dir)
        .expect("publish with --requires should write requires/")
        .collect();
    assert_eq!(files.len(), 1, "one file per distinct dependency store");
    let recorded = std::fs::read_to_string(requires_dir.join("0.json")).expect("a readable file");
    assert!(recorded.contains("leaf_fn"), "{recorded}");
}

#[test]
fn resolve_walks_a_transitively_required_store() {
    let dir = scratch("vcs-requires-resolve");
    let leaf = write_module(
        &dir,
        "leaf.ls",
        "module closure.leaf;\npub fn leaf_fn() -> [] int { return 1; }\n",
    );
    let leaf_store = dir.join("leaf-store");
    publish(&leaf_store, &leaf);
    let leaf_lock = dir.join("leaf.lock");
    assert!(lock(&leaf_store, &leaf_lock, &["leaf_fn"]).status.success());

    let dependent = write_module(
        &dir,
        "dependent.ls",
        "module closure.dependent;\n\
         import closure.leaf;\n\
         pub fn dependent_fn() -> [] int { return leaf.leaf_fn() + 1; }\n",
    );
    let dependent_store = dir.join("dependent-store");
    assert!(
        publish_with_requires(&dependent_store, &dependent, &[(&leaf_lock, &leaf_store)])
            .status
            .success()
    );

    let output = resolve(&dependent_store);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 declaration(s)"), "{stdout}");
    assert!(stdout.contains("verified transitively"), "{stdout}");
}

#[test]
fn fetch_materializes_the_whole_closure_and_a_consumer_builds_against_it() {
    let dir = scratch("vcs-requires-fetch-e2e");
    let leaf = write_module(
        &dir,
        "leaf.ls",
        "module closure.leaf;\npub fn leaf_fn() -> [] int { return 41; }\n",
    );
    let leaf_store = dir.join("leaf-store");
    publish(&leaf_store, &leaf);
    let leaf_lock = dir.join("leaf.lock");
    assert!(lock(&leaf_store, &leaf_lock, &["leaf_fn"]).status.success());

    let dependent = write_module(
        &dir,
        "dependent.ls",
        "module closure.dependent;\n\
         import closure.leaf;\n\
         pub fn dependent_fn() -> [] int { return leaf.leaf_fn() + 1; }\n",
    );
    let dependent_store = dir.join("dependent-store");
    assert!(
        publish_with_requires(&dependent_store, &dependent, &[(&leaf_lock, &leaf_store)])
            .status
            .success()
    );

    let consumer_lock = dir.join("consumer.lock");
    assert!(lock(&dependent_store, &consumer_lock, &["dependent_fn"]).status.success());
    let fetch_dir = dir.join("fetched");
    let output = fetch(&consumer_lock, &dependent_store, &fetch_dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let fetched_files: Vec<_> = std::fs::read_dir(&fetch_dir)
        .expect("the fetch dir exists")
        .map(|e| e.expect("a readable entry").path())
        .collect();
    assert_eq!(
        fetched_files.len(),
        2,
        "one fetch should materialize both `dependent`'s own file and `leaf`'s, found \
         {fetched_files:?}"
    );

    let consumer = write_module(
        &dir,
        "main.ls",
        "import closure.dependent;\n\
         fn main(world: World) -> [] int {\n\
             let Split { io, ffi, fs, heap, args } = split(world);\n\
             release(io); release(ffi); release(fs); release(heap); release(args);\n\
             return dependent.dependent_fn();\n\
         }\n",
    );
    let exe = dir.join("consumer");
    let mut build = Command::new(BIN);
    build.arg("build").arg(&consumer);
    for f in &fetched_files {
        build.arg(f);
    }
    build.arg("-o").arg(&exe);
    let build = build.output().expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    let run = Command::new(&exe).output().expect("the compiled program runs");
    assert_eq!(run.status.code(), Some(42), "leaf_fn() (41) + 1, through two fetched files");
}

#[test]
fn resolve_refuses_a_dependency_cycle() {
    let dir = scratch("vcs-requires-cycle");
    let a = write_module(&dir, "a.ls", "module cycle.a;\npub fn a_fn() -> [] int { return 1; }\n");
    let b = write_module(&dir, "b.ls", "module cycle.b;\npub fn b_fn() -> [] int { return 2; }\n");
    let a_store = dir.join("a-store");
    let b_store = dir.join("b-store");
    publish(&a_store, &a);
    publish(&b_store, &b);

    let a_needs_b = dir.join("a-needs-b.lock");
    let b_needs_a = dir.join("b-needs-a.lock");
    assert!(lock(&b_store, &a_needs_b, &["b_fn"]).status.success());
    assert!(lock(&a_store, &b_needs_a, &["a_fn"]).status.success());

    // A real second publish of each can't create the cycle (`requires/`
    // is fixed at publish time, and neither store depends on the other
    // yet) -- so the cycle is wired directly, the same shape a real one
    // would leave on disk, to check the walk itself rather than how one
    // gets created.
    std::fs::create_dir_all(a_store.join("requires")).expect("a writable store");
    std::fs::create_dir_all(b_store.join("requires")).expect("a writable store");
    let a_requires = format!(
        "{{\"store\": {:?}, \"lock\": {}}}",
        b_store.display(),
        std::fs::read_to_string(&a_needs_b).expect("a readable lock")
    );
    let b_requires = format!(
        "{{\"store\": {:?}, \"lock\": {}}}",
        a_store.display(),
        std::fs::read_to_string(&b_needs_a).expect("a readable lock")
    );
    std::fs::write(a_store.join("requires/0.json"), a_requires).expect("a writable file");
    std::fs::write(b_store.join("requires/0.json"), b_requires).expect("a writable file");

    let output = resolve(&a_store);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("dependency cycle"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn resolve_refuses_a_diamond_conflict() {
    let dir = scratch("vcs-requires-diamond");
    let leaf = write_module(
        &dir,
        "leaf.ls",
        "module diamond.leaf;\n\
         pub fn leaf_fn() -> [] int { return 1; }\n\
         pub fn leaf_fn2() -> [] int { return 2; }\n",
    );
    let leaf_store = dir.join("leaf-store");
    publish(&leaf_store, &leaf);

    let mid1_lock = dir.join("mid1-leaf.lock");
    assert!(lock(&leaf_store, &mid1_lock, &["leaf_fn"]).status.success());
    let mid1 = write_module(
        &dir,
        "mid1.ls",
        "module diamond.mid1;\npub fn mid1_fn() -> [] int { return 10; }\n",
    );
    let mid1_store = dir.join("mid1-store");
    assert!(
        publish_with_requires(&mid1_store, &mid1, &[(&mid1_lock, &leaf_store)]).status.success()
    );

    // `mid2` also requires `leaf`, from the *same* store, but its own
    // lock (hand-edited, the same way the cycle test above wires its
    // scenario directly) disagrees with `mid1`'s about what `leaf_fn`
    // pins -- claiming `leaf_fn2`'s own sig_id under the name
    // `leaf_fn`, so the two paths reach the same `(store, name)` pair
    // with two different heads.
    let mid2_lock_path = dir.join("mid2-leaf.lock");
    assert!(lock(&leaf_store, &mid2_lock_path, &["leaf_fn2"]).status.success());
    let mid2_lock_text = std::fs::read_to_string(&mid2_lock_path).expect("a readable lock");
    let mid2_lock_text = mid2_lock_text.replace("\"leaf_fn2\"", "\"leaf_fn\"");
    std::fs::write(&mid2_lock_path, &mid2_lock_text).expect("a writable lock");
    let mid2 = write_module(
        &dir,
        "mid2.ls",
        "module diamond.mid2;\npub fn mid2_fn() -> [] int { return 20; }\n",
    );
    let mid2_store = dir.join("mid2-store");
    assert!(
        publish_with_requires(&mid2_store, &mid2, &[(&mid2_lock_path, &leaf_store)])
            .status
            .success()
    );

    let top_mid1_lock = dir.join("top-mid1.lock");
    let top_mid2_lock = dir.join("top-mid2.lock");
    assert!(lock(&mid1_store, &top_mid1_lock, &["mid1_fn"]).status.success());
    assert!(lock(&mid2_store, &top_mid2_lock, &["mid2_fn"]).status.success());
    let top = write_module(
        &dir,
        "top.ls",
        "module diamond.top;\npub fn top_fn() -> [] int { return 100; }\n",
    );
    let output = publish_with_requires(
        &dir.join("top-store"),
        &top,
        &[(&top_mid1_lock, &mid1_store), (&top_mid2_lock, &mid2_store)],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("diamond dependency"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn two_paths_agreeing_on_the_same_pin_is_not_a_diamond() {
    let dir = scratch("vcs-requires-agree");
    let leaf = write_module(
        &dir,
        "leaf.ls",
        "module agree.leaf;\npub fn leaf_fn() -> [] int { return 1; }\n",
    );
    let leaf_store = dir.join("leaf-store");
    publish(&leaf_store, &leaf);

    let mid1_lock = dir.join("mid1-leaf.lock");
    let mid2_lock = dir.join("mid2-leaf.lock");
    assert!(lock(&leaf_store, &mid1_lock, &["leaf_fn"]).status.success());
    assert!(lock(&leaf_store, &mid2_lock, &["leaf_fn"]).status.success());

    let mid1 = write_module(
        &dir,
        "mid1.ls",
        "module agree.mid1;\npub fn mid1_fn() -> [] int { return 10; }\n",
    );
    let mid2 = write_module(
        &dir,
        "mid2.ls",
        "module agree.mid2;\npub fn mid2_fn() -> [] int { return 20; }\n",
    );
    let mid1_store = dir.join("mid1-store");
    let mid2_store = dir.join("mid2-store");
    assert!(
        publish_with_requires(&mid1_store, &mid1, &[(&mid1_lock, &leaf_store)]).status.success()
    );
    assert!(
        publish_with_requires(&mid2_store, &mid2, &[(&mid2_lock, &leaf_store)]).status.success()
    );

    let top_mid1_lock = dir.join("top-mid1.lock");
    let top_mid2_lock = dir.join("top-mid2.lock");
    assert!(lock(&mid1_store, &top_mid1_lock, &["mid1_fn"]).status.success());
    assert!(lock(&mid2_store, &top_mid2_lock, &["mid2_fn"]).status.success());
    let top = write_module(
        &dir,
        "top.ls",
        "module agree.top;\npub fn top_fn() -> [] int { return 100; }\n",
    );
    let output = publish_with_requires(
        &dir.join("top-store"),
        &top,
        &[(&top_mid1_lock, &mid1_store), (&top_mid2_lock, &mid2_store)],
    );
    assert!(
        output.status.success(),
        "two paths pinning the same store's same name to the same head should compose, but: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
