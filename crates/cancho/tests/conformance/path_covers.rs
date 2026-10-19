//! `exec_covers` and `fs_covers` (`docs/processes.md` §4.13, #535): asking whether a path is inside a bound by the
//! rule the operations enforce, over real processes and paths, on every code generator.
//!
//! The driver is `tests/programs/path_covers.cho`. It is built for each bound a test wants by replacing the text
//! `"/bin"` (the bound of its capabilities and of their rows) with the bound, and with each of the three ways this
//! repository can build it: Cranelift, LLVM through the Rust compiler, and LLVM through the compiler in cancho.
//!
//! What makes these tests more than a table: for every path, the answer of `exec_covers` is held against what the
//! operation really does. A path the answer calls outside must kill the driver at `exec_spawn` (a signal, no output);
//! one it calls covered must reach the kernel and come back as a value (`spawned code` or `failed errno`). So the
//! predicate and the trap cannot drift without a path in these tables showing it, and the same holds for
//! `fs_covers` against `open_read`.

use super::*;
use std::os::unix::process::ExitStatusExt;

const CONFIGS: [(&str, &[&str]); 3] = [
    ("cranelift", &["--backend", "cranelift"]),
    ("llvm", &["--backend", "llvm", "--compiler", "rust"]),
    ("compiler in cancho", &["--backend", "llvm", "--compiler", "auto"]),
];

/// The driver for `bound`, once per configuration. `tag` names the scratch directory (tests run in parallel).
fn drivers(tag: &str, bound: &str) -> Vec<(&'static str, PathBuf)> {
    let dir = scratch(tag);
    let source = std::fs::read_to_string(repo_root().join("tests/programs/path_covers.cho"))
        .expect("the driver is in the repository")
        .replace("\"/bin\"", &format!("\"{bound}\""))
        // `split` hands out `Exec("")` and `Fs("")`: the empty bound is the capability as it is, and narrowing to
        // it is refused as a no-op.
        .replace("narrow(exec, \"\")", "exec")
        .replace("narrow(fs, \"\")", "fs");
    let file = dir.join("driver.cho");
    std::fs::write(&file, source).expect("a writable scratch directory");
    CONFIGS
        .iter()
        .map(|(name, flags)| {
            let exe = dir.join(name.replace(' ', "-"));
            let build = Command::new(BIN)
                .arg("build")
                .arg(&file)
                .arg("--std")
                .args(*flags)
                .arg("-o")
                .arg(&exe)
                .output()
                .expect("the compiler runs");
            assert!(
                build.status.success(),
                "`{name}` should build the driver for `{bound}`:\n{}",
                String::from_utf8_lossy(&build.stderr)
            );
            (*name, exe)
        })
        .collect()
}

fn run(exe: &Path, mode: &str, path: &str) -> std::process::Output {
    Command::new(exe).args([mode, path]).output().expect("the driver runs")
}

/// What `exec_covers` or `fs_covers` printed.
fn asked(config: &str, exe: &Path, mode: &str, path: &str) -> bool {
    let out = run(exe, mode, path);
    assert_eq!(out.status.code(), Some(0), "`{config}` `{mode}` {path:?}: {out:?}");
    match String::from_utf8_lossy(&out.stdout).trim() {
        "covered" => true,
        "outside" => false,
        other => panic!("`{config}` `{mode}` {path:?} printed {other:?}"),
    }
}

/// The real operation: `Some(line)` when it came back as a value, `None` when the trap killed the driver (a signal
/// and nothing printed).
fn done(config: &str, exe: &Path, mode: &str, path: &str) -> Option<String> {
    let out = run(exe, mode, path);
    if out.status.code() == Some(0) {
        return Some(String::from_utf8_lossy(&out.stdout).trim().to_owned());
    }
    assert!(
        out.status.signal().is_some() && out.stdout.is_empty(),
        "`{config}` `{mode}` {path:?} neither returned nor trapped: {out:?}"
    );
    None
}

/// Every path of `cases` with the answer it should get, on every configuration: asked of both predicates, and held
/// against the operation each stands for.
fn check(tag: &str, bound: &str, cases: &[(&str, bool)]) {
    // The compile-time rule (`narrow`, the effect rows) is the same rule for what it can see in a literal: no `..`,
    // no length cap, no NUL. A change to one of the two that the other does not follow is a path in a table below.
    for (path, covered) in cases {
        if !path.contains("..") && path.len() < 4096 {
            assert_eq!(
                cancho_ir::extends_path(bound, path),
                *covered,
                "`extends_path` on bound `{bound}` path {path:?}"
            );
        }
    }
    for (config, exe) in drivers(tag, bound) {
        for (path, covered) in cases {
            for mode in ["c", "f"] {
                assert_eq!(
                    asked(config, &exe, mode, path),
                    *covered,
                    "`{config}` `{mode}` bound `{bound}` path {path:?}"
                );
            }
            for mode in ["e", "o"] {
                let result = done(config, &exe, mode, path);
                assert_eq!(
                    result.is_some(),
                    *covered,
                    "`{config}` `{mode}` bound `{bound}` path {path:?}: the operation {} but the predicate said {}",
                    if result.is_some() { "returned" } else { "trapped" },
                    if *covered { "covered" } else { "outside" },
                );
            }
        }
    }
}

/// §4.13: a directory bound. The prefix extends at a `/` or not at all (`/bin2`, `/binx`), the directory itself and
/// a trailing slash are inside, a `..` anywhere is outside (the byte pair, so `true..x` as well), and a relative
/// path is outside every bound but `""`.
#[test]
fn a_directory_bound() {
    check(
        "path-covers-dir",
        "/bin",
        &[
            ("/bin/true", true),
            ("/bin", true),
            ("/bin/", true),
            ("/bin/nonexistent", true),
            ("/bin/a.b", true),
            ("/bin/.hidden", true),
            ("/bin//true", true),
            ("/bin2/true", false),
            ("/binx", false),
            ("/bi", false),
            ("/", false),
            ("", false),
            ("true", false),
            ("bin/true", false),
            ("//bin/true", false),
            ("/usr/bin/true", false),
            ("/BIN/true", false),
            ("/bin/../bin/true", false),
            ("/bin/..", false),
            ("/../bin/true", false),
            ("/bin/true..x", false),
        ],
    );
}

/// The exact program is inside its own bound, and one byte more or less is not.
#[test]
fn a_bound_that_is_one_program() {
    check(
        "path-covers-one",
        "/bin/true",
        &[
            ("/bin/true", true),
            ("/bin/true/", true),
            ("/bin/true/x", true),
            ("/bin/truex", false),
            ("/bin/tru", false),
            ("/bin/false", false),
            ("/bin", false),
            ("/bin/true/../false", false),
            ("/bin/true..", false),
        ],
    );
}

/// A bound that ends in `/` already ends at a boundary: what continues is inside, and the directory without its
/// slash is *shorter than the prefix*, so outside.
#[test]
fn a_bound_that_ends_in_a_slash() {
    check(
        "path-covers-slash",
        "/bin/",
        &[
            ("/bin/true", true),
            ("/bin/", true),
            ("/bin//true", true),
            ("/bin", false),
            ("/bin2/true", false),
            ("/bin/../bin/true", false),
            ("/usr/bin/true", false),
        ],
    );
}

/// The empty bound contains every path but the ones the rule refuses outright, `..` and a path too long to copy.
#[test]
fn the_empty_bound() {
    check(
        "path-covers-empty",
        "",
        &[
            ("/bin/true", true),
            ("true", true),
            ("x/y", true),
            ("/etc/passwd", true),
            ("", true),
            ("/bin/../bin/true", false),
            ("..", false),
            ("a/../b", false),
        ],
    );
}

/// The longest path the operations copy is 4095 bytes: at 4096 they trap, and so the predicate says outside.
#[test]
fn the_longest_path() {
    let inside = format!("/bin/{}", "a".repeat(4095 - 5));
    let long = format!("/bin/{}", "a".repeat(4096 - 5));
    assert_eq!((inside.len(), long.len()), (4095, 4096));
    check("path-covers-long", "/bin", &[(&inside, true), (&long, false)]);
    let inside = "a".repeat(4095);
    let long = "a".repeat(4096);
    check("path-covers-long-empty", "", &[(&inside, true), (&long, false)]);
}

/// A NUL after the bound ends the C string at the bound, so the path is the bound itself and is inside: the Rust
/// code generators have always taken it, and the compiler in cancho trapped on it until #535 found it with this
/// question (`docs/processes.md` §4.13). A literal, since a command line has no NUL in it.
#[test]
fn a_nul_after_the_bound() {
    for (config, exe) in drivers("path-covers-nul", "/bin") {
        let out = run(&exe, "z", "");
        assert_eq!(out.status.code(), Some(0), "`{config}`: {out:?}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "covered\ncovered\n", "`{config}`");
    }
}

/// The supervisor of #535: it asks first, so a path outside the bound is a line it writes and not the end of the
/// supervisor, while the same path handed straight to `exec_spawn` ends it.
#[test]
fn a_supervisor_that_asks_first_survives() {
    for (config, exe) in drivers("path-covers-supervisor", "/bin") {
        for path in ["/bin2/true", "/bin/../bin/true", "true", "/usr/bin/true"] {
            let out = run(&exe, "s", path);
            assert_eq!(out.status.code(), Some(0), "`{config}` {path:?}: {out:?}");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                "refused\njournaled\n",
                "`{config}` {path:?}"
            );
            assert!(
                done(config, &exe, "e", path).is_none(),
                "`{config}` {path:?}: without asking it traps"
            );
        }
        let out = run(&exe, "s", "/bin/true");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "spawned code 0\njournaled\n",
            "`{config}`"
        );
    }
}

/// §4.13: asking adds nothing to the report. `tests/accept/path_covers_asks_first.cho` asks of an `Exec` and an
/// `Fs` in functions whose rows are empty, and its program-wide report has `io_write` and no `exec` or `fs_*` label.
#[test]
fn asking_does_not_widen_the_authority_report() {
    let out = Command::new(BIN)
        .arg("authority")
        .arg("--std")
        .arg(repo_root().join("tests/accept/path_covers_asks_first.cho"))
        .output()
        .expect("the compiler runs");
    assert!(out.status.success(), "{out:?}");
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(report.contains("io_write"), "{report}");
    for label in ["exec", "fs_read", "fs_write"] {
        assert!(
            !report.contains(label),
            "`{label}` is in the report of a program that only asks:\n{report}"
        );
    }
}
