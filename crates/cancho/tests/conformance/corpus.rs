//! The corpus: every accept and reject fixture, every example, and the command line around them.

use super::*;

/// Collect the `//~ <key> <value>` directives from a fixture's header.
fn directives(source: &str, key: &str) -> Vec<String> {
    let prefix = format!("//~ {key} ");
    source
        .lines()
        .filter_map(|line| line.trim().strip_prefix(&prefix).map(|rest| rest.to_owned()))
        .collect()
}

fn directive(source: &str, key: &str) -> Option<String> {
    directives(source, key).into_iter().next()
}

/// `tests/run.cho` (`docs/gate.md` step G3) walks the fixtures: a program in cancho, built here by the `cancho` under test and run from the root of the
/// repository with the kind of fixture to walk. It replaces the three walkers that were here (every accept fixture, every reject fixture, every example),
/// which did what it does in Rust; the gate runs it with no Rust (`scripts/gate.sh`). The compiler the fixtures are built with and the cache they build
/// into are the ones this test run has (`CANCHO_COMPILER`, `CANCHO_CACHE`), since the program has no environment to pass on.
fn walk_fixtures(kind: &str) {
    let root = repo_root();
    let dir = scratch(&format!("run-cho-{kind}"));
    let exe = dir.join("fixtures-run");
    let build = Command::new(BIN)
        .current_dir(&root)
        .args(["build", "tests/run.cho", "tests/support/harness.cho", "--std", "-o"])
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "tests/run.cho should compile:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let cache = std::env::var("CANCHO_CACHE")
        .unwrap_or_else(|_| dir.join("cache").to_string_lossy().into_owned());
    let mut walk = Command::new(&exe);
    walk.current_dir(&root).arg(BIN).arg(kind).args(["--cache", &cache]);
    if let Ok(compiler) = std::env::var("CANCHO_COMPILER") {
        walk.args(["--compiler", &compiler]);
    }
    let out = walk.output().expect("the walker runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the {kind} fixtures:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.starts_with("ok: "), "the walker said {said}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn accepted_programs_build_and_run() {
    walk_fixtures("accept");
}

#[test]
fn refused_programs_are_refused_with_the_stated_reason() {
    walk_fixtures("reject");
}

/// `docs/agent-errors.md` §3.2: every rule in the catalogue is a rule
/// some fixture reaches.
///
/// The harness's own header has said *"adding a rule to the language
/// means adding a fixture here"* since M0. It was kept for 38 rules out
/// of 47 when the catalogue was first written down — and the gap was
/// invisible until then, because "every rule" was a claim about a set
/// nobody had enumerated. **A catalogue is what makes a coverage claim
/// falsifiable**, which is an argument for tagging that has nothing to
/// do with agents.
#[test]
fn every_rule_has_a_fixture() {
    use std::collections::BTreeSet;

    // One rule the single-file harness cannot reach by construction:
    // `pub` is about reaching *another* module, a file declares at most
    // one module, and a fixture here is one file. It is covered by
    // `the_module_rules_are_enforced_across_files`, which is where a
    // second file exists.
    //
    // And one no source file can reach on purpose: `internal` names the
    // compiler's failures, and a fixture that produced one would be a bug
    // report. `docs/internal-errors.md` §5: it is covered by the unit
    // tests in `cancho` and `cancho-codegen`, which break the IR by hand.
    // `not-public` and `internal` are the two the single-file harness
    // cannot reach by construction; `ingest-*` are the three a `.cho`
    // fixture cannot reach at all -- they name the refusals of the JSON
    // ingest path, whose fixtures are `.json` handed to `cancho ingest`,
    // and the ingest tests hold one per rule
    // (`docs/structured-ingest.md` §3).
    const COVERED_ELSEWHERE: [&str; 5] =
        ["not-public", "internal", "ingest-arity", "ingest-json", "ingest-node"];

    let declared: BTreeSet<String> = fixtures("reject")
        .iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(path).ok()?;
            Some(directive(&source, "RULE")?.trim().to_owned())
        })
        .collect();

    let rules = golden("rules");
    let missing: Vec<&str> = rules
        .iter()
        .map(String::as_str)
        .filter(|tag| !declared.contains(*tag) && !COVERED_ELSEWHERE.contains(tag))
        .collect();
    assert!(
        missing.is_empty(),
        "these rules exist and no fixture reaches them: {missing:?}\n\
         Add one under `tests/reject/`, or say here where it is covered instead."
    );

    // The other direction: a fixture declaring a tag the catalogue does
    // not have is a typo that would otherwise pass forever.
    let catalogue: BTreeSet<&str> = rules.iter().map(String::as_str).collect();
    for tag in &declared {
        assert!(catalogue.contains(tag.as_str()), "no rule is called `{tag}`");
    }
}

/// Every example must build, run, and print what its header says (`tests/run.cho examples`): a walker rather than one test per example, so an example added
/// later is covered without anyone remembering to cover it, and an example that stops matching the language fails CI instead of quietly rotting.
#[test]
fn every_example_runs_and_prints_what_it_says() {
    walk_fixtures("examples");
}

#[test]
fn run_builds_and_executes_in_one_step() {
    let source = repo_root().join("examples").join("hello.cho");
    let output = Command::new(BIN).arg("run").arg(&source).output().expect("the compiler runs");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "Hello, world!\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn the_multi_file_example_builds_and_runs() {
    // `examples/wordfreq` is the capstone: more than one file, arguments,
    // file IO, the heap, matching through references, and slices, each
    // doing real work rather than being demonstrated.
    //
    // The single-file example harness cannot reach it -- it filters on the
    // `.cho` extension and a directory has none -- so it is run here.
    let root = repo_root().join("examples").join("wordfreq");
    let dir = scratch("wordfreq");
    let exe = dir.join("wordfreq");
    let build = Command::new(BIN)
        .arg("build")
        .arg(root.join("main.cho"))
        .arg(root.join("text.cho"))
        .arg(root.join("counts.cho"))
        .arg("--std")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    // With no arguments it counts its own sample. The list is built by
    // prepending, so the order is reverse first-seen.
    let run = Command::new(&exe).output().expect("the compiled program runs");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "dog 1\nlazy 1\nover 1\njumps 1\nfox 2\nbrown 1\nquick 1\nthe 3\n"
    );
    assert_eq!(run.status.code(), Some(0), "eight distinct words");

    // Given a path it counts that file instead.
    let doc = dir.join("doc.txt");
    std::fs::write(&doc, "alpha beta alpha\ngamma beta alpha\n").expect("a writable fixture");
    let counted = Command::new(&exe).arg(&doc).output().expect("the compiled program runs");
    assert_eq!(String::from_utf8_lossy(&counted.stdout), "gamma 1\nbeta 2\nalpha 3\n");

    let _ = std::fs::remove_dir_all(&dir);
}

/// `examples/modular/` — two modules and a root, with a qualifier, an
/// `as`, a private helper and a module importing another.
#[test]
fn the_modular_example_builds_and_runs() {
    let root = repo_root().join("examples").join("modular");
    let dir = scratch("modular-example");
    let exe = dir.join("modular");
    let build = Command::new(BIN)
        .arg("build")
        .arg(root.join("main.cho"))
        .arg(root.join("counts.cho"))
        .arg(root.join("text.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    let run = Command::new(&exe).output().expect("the compiled program runs");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "seen 3, total 60\n60\n");
    assert_eq!(run.status.code(), Some(0));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_growable_buffer_example_builds_and_runs() {
    // `examples/buffer/` is the library `docs/boxed-slices.md` §4
    // describes: growing is allocate-copy-end, written down rather than
    // built in, so the doubling policy belongs to the program.
    let root = repo_root().join("examples").join("buffer");
    let dir = scratch("buffer-example");
    let exe = dir.join("buffer");
    let build = Command::new(BIN)
        .arg("build")
        .arg(root.join("main.cho"))
        .arg(root.join("buffer.cho"))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    let run = Command::new(&exe).output().expect("the compiled program runs");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "counting: 1 4 9 16 25 36 49 64\n");
    assert_eq!(run.status.code(), Some(0), "thirty-one bytes built from a one-byte buffer");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_slab_example_builds_and_runs() {
    // `examples/slab/` is §9's `Gen` hatch, built as `docs/sharing.md` §3
    // describes it. The line that matters is the last one: a handle whose
    // slot was removed comes back `Missing` -- a value, not a dangling
    // pointer -- and the program decides what to do about it.
    //
    // The three `rc_*.cho` reject fixtures are the other half of the same
    // claim: `Gen` is a library, and `Rc` is not one that can be written.
    let root = repo_root().join("examples").join("slab");
    let dir = scratch("slab-example");
    let exe = dir.join("slab");
    let build = Command::new(BIN)
        .arg("build")
        .arg(root.join("main.cho"))
        .arg(root.join("slab.cho"))
        .arg("--std")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    let run = Command::new(&exe).output().expect("the compiled program runs");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "live handle:  7\nafter remove: missing\nnew handle:   9\nold handle:   missing\n"
    );
    assert_eq!(run.status.code(), Some(0), "one slot live at the end, so `drop_slab` said 1");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn emitting_a_bare_object_file_works() {
    let dir = scratch("emit-obj");
    let object = dir.join("hello.o");
    let source = repo_root().join("examples").join("hello.cho");

    let build = Command::new(BIN)
        .args([
            "build".as_ref(),
            source.as_os_str(),
            "--emit".as_ref(),
            "obj".as_ref(),
            "-o".as_ref(),
            object.as_os_str(),
        ])
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    assert!(std::fs::metadata(&object).expect("an object file").len() > 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_wrong_command_line_is_a_usage_error_not_a_refusal() {
    let output = Command::new(BIN).arg("frobnicate").output().expect("the compiler runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}

/// A catalogue the front end keeps (`tests/rules.golden`: the tag of every refusal rule; `tests/builtins.golden`: the name of every builtin), one a line, in
/// the order the compiler lists them: recorded once from the Rust crates (`rules_and_builtins_are_the_goldens`, in `selfhost/parity.rs`, holds the two together
/// until the crates go), so that the tests that need the list read a file and not a Rust crate (`docs/removing-rust.md` phase 1b).
pub(super) fn golden(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(repo_root().join(format!("tests/{name}.golden")))
        .unwrap_or_else(|e| panic!("tests/{name}.golden is read: {e}"));
    text.lines().map(str::to_owned).collect()
}
