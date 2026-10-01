//! `lex-sys test` (`docs/testing.md` §3): discovery, one process per test,
//! and the exit status.

use super::*;

/// Run `lex-sys test` over the named files, written into a fresh scratch
/// directory. Returns the process output and the directory.
fn run_tests(tag: &str, files: &[(&str, &str)], flags: &[&str]) -> (std::process::Output, PathBuf) {
    let dir = scratch(tag);
    let mut paths = Vec::new();
    for (name, text) in files {
        let path = dir.join(name);
        std::fs::write(&path, text).expect("a writable fixture");
        paths.push(path);
    }
    let out =
        Command::new(BIN).arg("test").args(flags).args(&paths).output().expect("the compiler runs");
    (out, dir)
}

fn stdout(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

const PASSING: &str = "\
import std.test;

fn add(a: int, b: int) -> [] int { return a + b; }

fn test_add() -> [] int { return test.assert_eq(add(2, 3), 5); }

fn test_add_is_not_sub() -> [] int { return test.assert_ne(add(2, 3), 6); }
";

#[test]
fn passing_tests_exit_zero_and_are_each_named() {
    let (out, dir) = run_tests("test-pass", &[("pass.ls", PASSING)], &["--std"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}{}", stderr(&out));
    assert!(text.contains("running 2 tests"), "{text}");
    assert!(text.contains("test test_add ... ok"), "{text}");
    assert!(text.contains("test test_add_is_not_sub ... ok"), "{text}");
    assert!(text.contains("test result: ok. 2 passed; 0 failed"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failing_test_and_a_trapping_test_fail_without_stopping_the_run() {
    // A trap kills the process it happens in, so the test after it only
    // runs because each test has a process of its own.
    let source = "\
import std.test;

fn test_first_traps() -> [] int { return test.assert_eq(1, 2); }

fn test_answers_seven() -> [] int { return 7; }

fn test_still_runs() -> [] int { return 0; }
";
    for backend in ["llvm", "cranelift"] {
        let (out, dir) = run_tests(
            &format!("test-fail-{backend}"),
            &[("fail.ls", source)],
            &["--std", "--backend", backend],
        );
        let text = stdout(&out);
        assert_eq!(out.status.code(), Some(4), "{backend}: {text}{}", stderr(&out));
        assert!(text.contains("test test_first_traps ... FAILED"), "{backend}: {text}");
        assert!(text.contains("trapped: killed by signal"), "{backend}: {text}");
        assert!(text.contains("test test_answers_seven ... FAILED"), "{backend}: {text}");
        assert!(text.contains("returned 7"), "{backend}: {text}");
        assert!(text.contains("test test_still_runs ... ok"), "{backend}: {text}");
        assert!(text.contains("test result: FAILED. 1 passed; 2 failed"), "{backend}: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_test_that_answers_a_multiple_of_256_is_not_read_as_a_pass() {
    // An exit status is one byte; 256 would read back as 0.
    let source = "fn test_answers_256() -> [] int { return 256; }\n";
    let (out, dir) = run_tests("test-256", &[("wrap.ls", source)], &[]);
    assert_eq!(out.status.code(), Some(4), "{}{}", stdout(&out), stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_test_can_ask_for_the_heap_and_the_console() {
    let source = "\
import std.test;
import std.vec;
import std.io as console;

fn test_heap_and_io[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_write] int {
    var v = vec.empty(heap, 2, 0);
    v = vec.push(heap, v, 4);
    console.print_int(io, 99);
    var n = 0;
    borrow v as &r in {
        n = vec.get(r, 0);
    }
    vec.drop(heap, v);
    return test.assert_eq(n, 4);
}

fn test_heap_only[&h](heap: &!h Heap) -> [heap] int {
    var v = vec.empty(heap, 2, 0);
    v = vec.push(heap, v, 1);
    vec.drop(heap, v);
    return 0;
}
";
    let (out, dir) = run_tests("test-caps", &[("caps.ls", source)], &["--std"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}{}", stderr(&out));
    assert!(text.contains("test result: ok. 2 passed"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_test_in_another_module_is_found_and_named_by_its_module() {
    let module = "\
module suite.arith;
import std.test;

pub fn test_twice() -> [] int { return test.assert_eq(2 * 3, 6); }

fn test_private_helper_is_not_a_test_here() -> [] int { return 0; }
";
    // `test_private_helper...` is not `pub`, which the runner refuses
    // rather than skips: a test that silently did not run is a pass.
    let (out, dir) = run_tests("test-module-private", &[("arith.ls", module)], &["--std"]);
    assert_eq!(out.status.code(), Some(2), "{}{}", stdout(&out), stderr(&out));
    assert!(stderr(&out).contains("not `pub`"), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);

    let module = module.replace("fn test_private_helper_is_not_a_test_here", "pub fn test_thrice");
    let root = "import std.test;\n\nfn test_root() -> [] int { return test.assert(true); }\n";
    let (out, dir) =
        run_tests("test-module", &[("arith.ls", &module), ("root.ls", root)], &["--std"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}{}", stderr(&out));
    assert!(text.contains("test suite.arith.test_twice ... ok"), "{text}");
    assert!(text.contains("test suite.arith.test_thrice ... ok"), "{text}");
    assert!(text.contains("test test_root ... ok"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn finding_no_tests_is_a_usage_error_not_a_pass() {
    let source = "fn helper() -> [] int { return 0; }\n";
    let (out, dir) = run_tests("test-none", &[("none.ls", source)], &[]);
    assert_eq!(out.status.code(), Some(2), "{}{}", stdout(&out), stderr(&out));
    assert!(stderr(&out).contains("no `test_*` functions found"), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_declares_main_is_refused() {
    let source = "\
fn test_ok() -> [] int { return 0; }

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return 0;
}
";
    let (out, dir) = run_tests("test-main", &[("main.ls", source)], &[]);
    assert_eq!(out.status.code(), Some(2), "{}{}", stdout(&out), stderr(&out));
    assert!(stderr(&out).contains("supplies its own `main`"), "{}", stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_test_with_the_wrong_shape_is_refused_not_skipped() {
    for (tag, source, why) in [
        ("generic", "fn test_g[T: val](x: T) -> [] int { return 0; }\n", "type parameter"),
        ("returns", "fn test_r() -> [] bool { return true; }\n", "does not return `int`"),
        ("param", "fn test_p(x: int) -> [] int { return 0; }\n", "not a unique `Heap` or `Io`"),
        (
            "twice",
            "fn test_t[&a, &b](x: &!a Heap, y: &!b Heap) -> [x, y] int { return 0; }\n",
            "same capability twice",
        ),
    ] {
        let (out, dir) = run_tests(&format!("test-shape-{tag}"), &[("shape.ls", source)], &[]);
        assert_eq!(out.status.code(), Some(2), "{tag}: {}{}", stdout(&out), stderr(&out));
        assert!(stderr(&out).contains(why), "{tag}: {}", stderr(&out));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_program_the_checker_refuses_is_refused_not_failed() {
    // Exit 1 is "the program was refused"; 4 is reserved for a test that
    // ran and failed, so a caller can tell a broken build from a red test.
    let source = "fn test_bad() -> [] int { return undefined_name; }\n";
    let (out, dir) = run_tests("test-refused", &[("bad.ls", source)], &[]);
    assert_eq!(out.status.code(), Some(1), "{}{}", stdout(&out), stderr(&out));
    let _ = std::fs::remove_dir_all(&dir);
}
