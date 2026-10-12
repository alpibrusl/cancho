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

/// The `main` the language needs, for a program that defines `run`.
fn with_main(program: &str) -> String {
    format!("{program}\nfn main(world: World) -> [] int {{ release(world); return run(); }}\n")
}

/// The program, which has a `main`, built by the Rust compiler.
fn built_by_rust(index: usize, program: &str) -> Ended {
    let dir = scratch(&format!("selfhost-compile-rust-{index}"));
    let source = dir.join("program.cho");
    let exe = dir.join("program");
    std::fs::write(&source, program).expect("the scratch directory is writable");
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
/// `return`, `if` and `while`, and every operator. The ones that trap are here too: a trap is
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
    "fn run() -> [] int { var i = 0; var s = 0; while i < 10 { s = s + i; i = i + 1; } return s; }",
    "fn run() -> [] int { var i = 0; while i < 100 { i = i + 7; } return i; }",
    "fn run() -> [] int { var i = 5; while i > 100 { i = i + 1; } return i; }",
    "fn run() -> [] int { if true { return 1; } return 2; }",
    "fn run() -> [] int { if false { return 1; } return 2; }",
    "fn run() -> [] int { if 1 < 2 { return 10; } else { return 20; } }",
    "fn run() -> [] int { if 3 < 2 { return 10; } else { return 20; } }",
    "fn run() -> [] int { var x = 0; if 1 < 2 { x = 5; } x = x + 1; return x; }",
    "fn run() -> [] int { var x = 0; if 2 < 1 { x = 5; } x = x + 1; return x; }",
    "fn run() -> [] int { var x = 0; if 2 < 1 { x = 5; } else { x = 8; } return x; }",
    "fn f(n: int) -> [] int { if n < 0 { return 0 - 1; } else if n == 0 { return 0; } else { return 1; } }\nfn run() -> [] int { return f(0 - 5) + f(0) * 10 + f(7) * 100 + 100; }",
    "fn f(n: int) -> [] int { if n < 10 { return 1; } else if n < 100 { return 2; } else if n < 1000 { return 3; } return 4; }\nfn run() -> [] int { return f(5) + f(50) * 10 + f(500) * 100 + f(5000) * 1000 - 4000 - 3000; }",
    "fn fact(n: int) -> [] int { if n <= 1 { return 1; } return n * fact(n - 1); }\nfn run() -> [] int { return fact(5); }",
    "fn fact(n: int) -> [] int { if n <= 1 { return 1; } return n * fact(n - 1); }\nfn run() -> [] int { return fact(20) / 1000000000000000; }",
    "fn fact(n: int) -> [] int { if n <= 1 { return 1; } return n * fact(n - 1); }\nfn run() -> [] int { return fact(21); }",
    "fn fib(n: int) -> [] int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }\nfn run() -> [] int { return fib(15); }",
    "fn fib(n: int) -> [] int { var a = 0; var b = 1; var i = 0; while i < n { let t = a + b; a = b; b = t; i = i + 1; } return a; }\nfn run() -> [] int { return fib(30) % 256; }",
    "fn fib(n: int) -> [] int { var a = 0; var b = 1; var i = 0; while i < n { let t = a + b; a = b; b = t; i = i + 1; } return a; }\nfn run() -> [] int { return fib(93); }",
    "fn gcd(a: int, b: int) -> [] int { var x = a; var y = b; while y != 0 { let t = x % y; x = y; y = t; } return x; }\nfn run() -> [] int { return gcd(1071, 462) + gcd(17, 5) + gcd(100, 75); }",
    "fn pow(b: int, e: int) -> [] int { var r = 1; var i = 0; while i < e { r = r * b; i = i + 1; } return r; }\nfn run() -> [] int { return pow(2, 10) % 256 + pow(3, 4); }",
    "fn pow(b: int, e: int) -> [] int { var r = 1; var i = 0; while i < e { r = r * b; i = i + 1; } return r; }\nfn run() -> [] int { return pow(2, 63); }",
    "fn pow(b: int, e: int) -> [] int { var r = 1; var i = 0; while i < e { r = r * b; i = i + 1; } return r; }\nfn run() -> [] int { return pow(2, 62) / pow(2, 55); }",
    "fn collatz(n: int) -> [] int { var x = n; var steps = 0; while x != 1 { if x % 2 == 0 { x = x / 2; } else { x = 3 * x + 1; } steps = steps + 1; } return steps; }\nfn run() -> [] int { return collatz(27); }",
    "fn digits(n: int) -> [] int { var x = n; var s = 0; while x > 0 { s = s + x % 10; x = x / 10; } return s; }\nfn run() -> [] int { return digits(98765); }",
    "fn prime(n: int) -> [] bool { if n < 2 { return false; } var d = 2; while d * d <= n { if n % d == 0 { return false; } d = d + 1; } return true; }\nfn run() -> [] int { var count = 0; var i = 0; while i < 100 { if prime(i) { count = count + 1; } i = i + 1; } return count; }",
    "fn isqrt(n: int) -> [] int { var r = 0; while (r + 1) * (r + 1) <= n { r = r + 1; } return r; }\nfn run() -> [] int { return isqrt(1000000) % 256 + isqrt(99); }",
    "fn run() -> [] int { var i = 0; var j = 0; var s = 0; while i < 5 { j = 0; while j < 5 { s = s + i * j; j = j + 1; } i = i + 1; } return s; }",
    "fn run() -> [] int { var i = 0; var s = 0; while i < 10 { if i % 2 == 0 { s = s + i; } else { s = s - 1; } i = i + 1; } return s + 100; }",
    "fn first_over(n: int) -> [] int { var i = 0; while true { if i * i > n { return i; } i = i + 1; } return 0; }\nfn run() -> [] int { return first_over(50); }",
    "fn find(n: int) -> [] int { var i = 0; while i < 100 { if i * 3 == n { return i; } i = i + 1; } return 0 - 1; }\nfn run() -> [] int { return find(42) + find(7) + 100; }",
    "fn run() -> [] int { var i = 0; var s = 0; while i < 1000000 { s = s + i % 3; i = i + 1; } return s % 256; }",
    "fn run() -> [] int { var s = 1; var i = 0; while i < 70 { s = s * 2; i = i + 1; } return s; }",
    "fn run() -> [] int { var s = 0; var i = 0; while i < 10 { s = s + 100 / (5 - i); i = i + 1; } return s; }",
    "fn run() -> [] int { let a = true && true; let b = true && false; let c = false || true; let d = false || false; if a && !b && c && !d { return 1; } return 0; }",
    "fn t() -> [] bool { return true; }\nfn f() -> [] bool { return false; }\nfn run() -> [] int { var n = 0; if t() && f() { n = n + 1; } if t() || f() { n = n + 10; } if f() && t() { n = n + 100; } if f() || t() { n = n + 50; } return n; }",
    "fn boom(a: int) -> [] bool { return 1 / a == 1; }\nfn run() -> [] int { if false && boom(0) { return 1; } return 2; }",
    "fn boom(a: int) -> [] bool { return 1 / a == 1; }\nfn run() -> [] int { if true || boom(0) { return 1; } return 2; }",
    "fn boom(a: int) -> [] bool { return 1 / a == 1; }\nfn run() -> [] int { if true && boom(0) { return 1; } return 2; }",
    "fn boom(a: int) -> [] bool { return 1 / a == 1; }\nfn run() -> [] int { if false || boom(0) { return 1; } return 2; }",
    "fn ok(a: int) -> [] bool { return a > 0; }\nfn run() -> [] int { var n = 0; var i = 0 - 3; while i < 4 { if ok(i) && i < 3 || i == 0 - 2 { n = n + 1; } i = i + 1; } return n; }",
    "fn run() -> [] int { var i = 0; var s = 0; while i < 20 && s < 50 { s = s + i; i = i + 1; } return s * 10 + i; }",
    "fn run() -> [] int { let a = 5; let b = a > 3 && a < 10; let c = a > 3 || a < 0; let d = !(a == 5); if b && c && !d { return 7; } else { return 9; } }",
    "fn sign(n: int) -> [] int { if n < 0 { return 0 - 1; } if n > 0 { return 1; } return 0; }\nfn run() -> [] int { return sign(0 - 9) + sign(0) + sign(9) + 10; }",
    "fn max(a: int, b: int) -> [] int { if a > b { return a; } return b; }\nfn min(a: int, b: int) -> [] int { if a < b { return a; } return b; }\nfn run() -> [] int { return max(3, 8) * 10 + min(3, 8); }",
    "fn abs(n: int) -> [] int { if n < 0 { return 0 - n; } return n; }\nfn run() -> [] int { return abs(0 - 5) + abs(5) + abs(0 - 9223372036854775807); }",
    "fn abs(n: int) -> [] int { if n < 0 { return 0 - n; } return n; }\nfn run() -> [] int { return abs(0 - 9223372036854775807 - 1); }",
    "fn run() -> [] int { var x = 10; while x > 0 { x = x - 3; } return x + 100; }",
    "fn count(n: int) -> [] int { if n == 0 { return 0; } return 1 + count(n - 1); }\nfn run() -> [] int { return count(1000); }",
    "fn ack(m: int, n: int) -> [] int { if m == 0 { return n + 1; } if n == 0 { return ack(m - 1, 1); } return ack(m - 1, ack(m, n - 1)); }\nfn run() -> [] int { return ack(2, 3); }",
];

/// Programs with a `main` of their own, which the cancho compiler calls as the C `main` does.
const WITH_MAIN: &[&str] = &[
    "fn finish(w: World) -> [] int { release(w); return 3; }\nfn main(world: World) -> [] int { return finish(world); }",
    "fn finish(w: World, n: int) -> [] int { release(w); return n * 2; }\nfn main(world: World) -> [] int { return finish(world, 21); }",
    "fn pass(w: World) -> [] int { return finish(w, 5); }\nfn finish(w: World, n: int) -> [] int { release(w); return n + 1; }\nfn main(world: World) -> [] int { return pass(world); }",
    "fn main(world: World) -> [] int { var i = 0; var s = 0; while i < 10 { s = s + i; i = i + 1; } release(world); return s; }",
    "fn big(n: int) -> [] int { return n + 1; }\nfn main(world: World) -> [] int { release(world); return big(9223372036854775807); }",
    "fn work(w: World, n: int) -> [] int { release(w); if n > 3 { return n * 10; } return n; }\nfn main(world: World) -> [] int { return work(world, 5) % 256; }",
    "fn sq(n: int) -> [] int { return n * n; }\nfn main(world: World) -> [] int { release(world); return sq(sq(3)) - sq(8); }",
    "fn main(world: World) -> [] int { let r = release(world); return r + 4; }",
];

#[test]
fn the_compiler_in_cancho_builds_programs_that_do_what_the_rust_ones_do() {
    let compiler = build("compile", &with_front_end("compile.cho"));
    // Each program defining `run` is built three ways: by Rust with the `main` the language needs, by
    // the compiler in cancho from the program alone (it calls `run` itself), and by the compiler in
    // cancho from the program with that `main`, which is the way real programs are written.
    let mut jobs: Vec<(String, String)> = Vec::new();
    for program in PROGRAMS {
        jobs.push(((*program).to_owned(), (*program).to_owned()));
        jobs.push((with_main(program), with_main(program)));
    }
    for program in WITH_MAIN {
        jobs.push(((*program).to_owned(), (*program).to_owned()));
    }
    let numbered: Vec<(usize, &(String, String))> = jobs.iter().enumerate().collect();
    let results = in_parallel(&numbered, |(index, (rust_text, cancho_text))| {
        let rust = if cancho_text.contains("fn main") {
            built_by_rust(*index, rust_text)
        } else {
            built_by_rust(*index, &with_main(rust_text))
        };
        (cancho_text.clone(), rust, built_by_cancho(&compiler, *index, cancho_text))
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
    eprintln!("{} programs built both ways, {trapped} of them trap", results.len());
    assert!(trapped >= 40, "{trapped} of the programs trap; the test is about them too");
    let _ = std::fs::remove_dir_all(compiler.parent().expect("a scratch directory"));
}
