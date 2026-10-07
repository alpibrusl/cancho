//! `examples/selfhost/compile.cho` (`docs/self-hosting.md` section 6, stage 4): the compiler written in
//! cancho against the Rust one, by what the programs it builds do.
//!
//! The compiler in cancho reads a program on standard input and writes LLVM IR, which `clang` turns
//! into an executable, the way the Rust backend does. Its IR is not compared with the Rust
//! backend's (they are not the same text and need not be); each program is built both ways, run,
//! and must end the same way: the same exit status, or a trap in both.
//!
//! Each program defines `run`, which the cancho compiler calls from a C `main` and answers as the
//! exit status; the Rust build adds the `main(world)` the language requires, which releases the
//! `World` and returns `run()`. A program the cancho compiler cannot write yet is a failure here: the
//! list below is exactly what it can, and grows with it.

use super::*;

/// What a program did: its exit status, or that it was stopped by a signal, which is how a trap ends.
#[derive(Debug, PartialEq)]
enum Ended {
    Status(i32),
    Trapped,
}

fn run_it(exe: &Path) -> Ended {
    let out = Command::new(exe).output().expect("the program runs");
    match out.status.code() {
        Some(code) => Ended::Status(code),
        None => Ended::Trapped,
    }
}

/// The program built by the Rust compiler, with the `main` the language needs.
fn built_by_rust(index: usize, program: &str) -> Ended {
    let dir = scratch(&format!("selfhost-compile-rust-{index}"));
    let source = dir.join("program.cho");
    let exe = dir.join("program");
    let text = format!("{program}\nfn main(world: World) -> [] int {{ release(world); return run(); }}\n");
    std::fs::write(&source, text).expect("the scratch directory is writable");
    let build = Command::new(BIN)
        .arg("build")
        .arg(&source)
        .args(["-o".as_ref(), exe.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "the Rust compiler should build\n{program}\nbut said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let ended = run_it(&exe);
    let _ = std::fs::remove_dir_all(&dir);
    ended
}

/// The program built by the compiler in cancho, then `clang`.
fn built_by_cancho(compiler: &Path, index: usize, program: &str) -> Ended {
    let ir = answer(compiler, program);
    assert!(
        ir.starts_with("declare"),
        "the compiler in cancho should write code for\n{program}\nbut answered:\n{ir}"
    );
    let dir = scratch(&format!("selfhost-compile-cancho-{index}"));
    let source = dir.join("program.ll");
    let exe = dir.join("program");
    std::fs::write(&source, &ir).expect("the scratch directory is writable");
    let cc = std::env::var("CLANG").unwrap_or_else(|_| "clang".to_owned());
    let build = Command::new(cc)
        .args(["-O1", "-Wno-override-module"])
        .arg(&source)
        .args(["-o".as_ref(), exe.as_os_str()])
        .output()
        .expect("clang runs");
    assert!(
        build.status.success(),
        "clang should build what the compiler in cancho wrote for\n{program}\nbut said:\n{}\n{ir}",
        String::from_utf8_lossy(&build.stderr)
    );
    let ended = run_it(&exe);
    let _ = std::fs::remove_dir_all(&dir);
    ended
}

/// Programs the compiler in cancho can write: whole numbers and booleans, calls, `let`, assignment,
/// `return`, and the operators but `&&` and `||`. The ones that trap are here too: a trap is
/// something both must do.
const PROGRAMS: &[&str] = &[
    "fn run() -> [] int { return 0; }",
    "fn run() -> [] int { return 7; }",
    "fn run() -> [] int { return 255; }",
    "fn run() -> [] int { return 256; }",
    "fn run() -> [] int { return 1 + 2 * 3; }",
    "fn run() -> [] int { return 10 - 3 - 2; }",
    "fn run() -> [] int { return (4 + 5) * 6; }",
    "fn run() -> [] int { return 100 / 7; }",
    "fn run() -> [] int { return 100 % 7; }",
    "fn run() -> [] int { return 0 - 5; }",
    "fn run() -> [] int { return -5 + 10; }",
    "fn run() -> [] int { let x = 3; return x; }",
    "fn run() -> [] int { let x = 3; let y = x * x; return y + x; }",
    "fn run() -> [] int { var x = 1; x = x + 1; x = x * 5; return x; }",
    "fn run() -> [] int { let x = 5; let x = x + 1; return x; }",
    "fn add(a: int, b: int) -> [] int { return a + b; }\nfn run() -> [] int { return add(40, 2); }",
    "fn add(a: int, b: int) -> [] int { return a + b; }\nfn run() -> [] int { return add(add(1, 2), add(3, 4)); }",
    "fn twice(a: int) -> [] int { return a * 2; }\nfn run() -> [] int { let x = twice(21); return twice(x) - 100; }",
    "fn sub(a: int, b: int) -> [] int { return a - b; }\nfn run() -> [] int { return sub(1, 2) + 100; }",
    "fn id(a: int) -> [] int { return a; }\nfn run() -> [] int { var x = id(3); x = id(x) + id(4); return x; }",
    "fn first(a: int, b: int, c: int, d: int) -> [] int { return a; }\nfn run() -> [] int { return first(9, 8, 7, 6); }",
    "fn last(a: int, b: int, c: int, d: int) -> [] int { return d; }\nfn run() -> [] int { return last(9, 8, 7, 6); }",
    "fn f(a: int) -> [] int { let b = a + 1; let c = b + 1; return c; }\nfn run() -> [] int { return f(f(0)); }",
    "fn g(a: int) -> [] int { var b = a; b = b + b; return b; }\nfn run() -> [] int { return g(g(g(1))); }",
    "fn run() -> [] int { let big = 9223372036854775807; return big - big + 3; }",
    "fn run() -> [] int { let big = 9223372036854775807; let small = 0 - big - 1; return small + big + 1; }",
    "fn run() -> [] int { let x = 9223372036854775807; return x + 1; }",
    "fn run() -> [] int { let x = 9223372036854775807; let y = x + x; return 1; }",
    "fn run() -> [] int { let x = 9223372036854775807; return x * 2; }",
    "fn run() -> [] int { let x = 4611686018427387904; return x * 2; }",
    "fn run() -> [] int { let x = 3037000500; return x * x; }",
    "fn run() -> [] int { let x = 3037000499; let y = x * x; return 1; }",
    "fn run() -> [] int { let x = 0 - 9223372036854775807 - 1; return x - 1; }",
    "fn run() -> [] int { let x = 0 - 9223372036854775807 - 1; return 0 - x; }",
    "fn run() -> [] int { let x = 0 - 9223372036854775807 - 1; return x * x; }",
    "fn f(a: int, b: int) -> [] int { return a + b; }\nfn run() -> [] int { return f(9223372036854775807, 1); }",
    "fn f(a: int, b: int) -> [] int { return a - b; }\nfn run() -> [] int { return f(0 - 9223372036854775807 - 1, 1); }",
    "fn f(a: int, b: int) -> [] int { return a * b; }\nfn run() -> [] int { return f(3037000500, 3037000500); }",
    "fn f(a: int) -> [] int { return 0 - a; }\nfn run() -> [] int { return f(0 - 9223372036854775807 - 1); }",
    "fn f(a: int) -> [] int { return 0 - a; }\nfn run() -> [] int { return f(5) + 100; }",
    "fn f(a: int, b: int) -> [] int { return a / b; }\nfn run() -> [] int { return f(7, 0); }",
    "fn f(a: int, b: int) -> [] int { return a % b; }\nfn run() -> [] int { return f(7, 0); }",
    "fn f(a: int, b: int) -> [] int { return a / b; }\nfn run() -> [] int { return f(0 - 9223372036854775807 - 1, 0 - 1); }",
    "fn f(a: int, b: int) -> [] int { return a % b; }\nfn run() -> [] int { return f(0 - 9223372036854775807 - 1, 0 - 1); }",
    "fn f(a: int, b: int) -> [] int { return a / b; }\nfn run() -> [] int { return f(7, 2) + f(0 - 7, 2) + f(7, 0 - 2) + f(0 - 7, 0 - 2) + 100; }",
    "fn f(a: int, b: int) -> [] int { return a % b; }\nfn run() -> [] int { return f(7, 3) + f(0 - 7, 3) + f(7, 0 - 3) + f(0 - 7, 0 - 3) + 100; }",
    "fn f(a: int, b: int) -> [] int { return a / b; }\nfn run() -> [] int { return f(1, 1) + f(0, 5) + f(0 - 1, 1); }",
    "fn f(a: int, b: int) -> [] int { return a << b; }\nfn run() -> [] int { return f(1, 5); }",
    "fn f(a: int, b: int) -> [] int { return a << b; }\nfn run() -> [] int { return f(1, 63) + 7; }",
    "fn f(a: int, b: int) -> [] int { return a << b; }\nfn run() -> [] int { return f(1, 64); }",
    "fn f(a: int, b: int) -> [] int { return a << b; }\nfn run() -> [] int { return f(1, 0 - 1); }",
    "fn f(a: int, b: int) -> [] int { return a >> b; }\nfn run() -> [] int { return f(1024, 3); }",
    "fn f(a: int, b: int) -> [] int { return a >> b; }\nfn run() -> [] int { return f(0 - 1024, 3) + 200; }",
    "fn f(a: int, b: int) -> [] int { return a >> b; }\nfn run() -> [] int { return f(5, 64); }",
    "fn f(a: int, b: int) -> [] int { return a >> b; }\nfn run() -> [] int { return f(0 - 5, 63) + 10; }",
    "fn f(a: int, b: int) -> [] int { return a & b; }\nfn run() -> [] int { return f(12, 10); }",
    "fn f(a: int, b: int) -> [] int { return a | b; }\nfn run() -> [] int { return f(12, 10); }",
    "fn f(a: int, b: int) -> [] int { return a ^ b; }\nfn run() -> [] int { return f(12, 10); }",
    "fn f(a: int) -> [] int { return ~a; }\nfn run() -> [] int { return f(5) + 100; }",
    "fn f(a: int) -> [] int { return ~a; }\nfn run() -> [] int { return f(0 - 1); }",
    "fn f(a: int, b: int) -> [] int { return (a & b) | (a ^ b) + (a << 1); }\nfn run() -> [] int { return f(5, 3); }",
    "fn f(a: int, b: bool) -> [] int { return a; }\nfn run() -> [] int { return f(5, true) + f(6, false); }",
    "fn f(a: int, b: bool) -> [] int { let c = b; return a; }\nfn run() -> [] int { return f(5, true); }",
    "fn f(a: int) -> [] bool { return a < 3; }\nfn run() -> [] int { let b = f(2); let c = f(5); return 4; }",
    "fn lt(a: int, b: int) -> [] bool { return a < b; }\nfn le(a: int, b: int) -> [] bool { return a <= b; }\nfn run() -> [] int { let x = lt(1, 2); let y = le(2, 2); return 9; }",
    "fn gt(a: int, b: int) -> [] bool { return a > b; }\nfn ge(a: int, b: int) -> [] bool { return a >= b; }\nfn run() -> [] int { let x = gt(1, 2); let y = ge(2, 2); return 9; }",
    "fn eq(a: int, b: int) -> [] bool { return a == b; }\nfn ne(a: int, b: int) -> [] bool { return a != b; }\nfn run() -> [] int { let x = eq(1, 2); let y = ne(2, 2); return 9; }",
    "fn same(a: bool, b: bool) -> [] bool { return a == b; }\nfn run() -> [] int { let x = same(true, false); return 3; }",
    "fn neg(a: bool) -> [] bool { return !a; }\nfn run() -> [] int { let x = neg(true); let y = !x; return 3; }",
    "fn t() -> [] bool { return true; }\nfn f() -> [] bool { return false; }\nfn run() -> [] int { let a = t(); let b = f(); let c = !a; return 11; }",
    "fn run() -> [] int { var a = 1; var b = 2; var c = 3; a = b + c; b = a + c; c = a + b; return c; }",
    "fn run() -> [] int { var a = 1; a = a + 1; a = a + 1; a = a + 1; a = a + 1; a = a + 1; return a; }",
    "fn f(x: int) -> [] int { return x + 1; }\nfn g(x: int) -> [] int { return f(x) * 2; }\nfn h(x: int) -> [] int { return g(x) - f(x); }\nfn run() -> [] int { return h(10); }",
    "fn a(x: int) -> [] int { return x + 1; }\nfn run() -> [] int { return a(a(a(a(a(a(a(a(0)))))))); }",
];

#[test]
fn the_compiler_in_cancho_builds_programs_that_do_what_the_rust_ones_do() {
    let compiler = build("compile", &with_front_end("compile.cho"));
    let numbered: Vec<(usize, &str)> = PROGRAMS.iter().copied().enumerate().collect();
    let results = in_parallel(&numbered, |(index, program)| {
        (*program, built_by_rust(*index, program), built_by_cancho(&compiler, *index, program))
    });
    let mut different = Vec::new();
    let mut trapped = 0;
    for (program, theirs, ours) in &results {
        if theirs == &Ended::Trapped {
            trapped += 1;
        }
        if theirs != ours {
            different.push(format!("{program}\n  Rust: {theirs:?}\n  cancho: {ours:?}"));
        }
    }
    assert!(
        different.is_empty(),
        "the compiler in cancho and the Rust compiler build different programs from {} of {}:\n{}",
        different.len(),
        results.len(),
        different.join("\n")
    );
    assert!(trapped >= 15, "{trapped} of the programs trap; the test is about them too");
    let _ = std::fs::remove_dir_all(compiler.parent().expect("a scratch directory"));
}
