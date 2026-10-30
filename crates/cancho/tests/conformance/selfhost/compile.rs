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

/// What a program did: its exit status, or that it was stopped by a signal, which is how a trap
/// ends, and what it wrote to its standard output.
#[derive(Debug, PartialEq)]
enum Ended {
    Status(i32, String),
    Trapped(String),
}

fn run_it(exe: &Path) -> Ended {
    let out = Command::new(exe).output().expect("the program runs");
    let printed = String::from_utf8_lossy(&out.stdout).into_owned();
    match out.status.code() {
        Some(code) => Ended::Status(code, printed),
        None => Ended::Trapped(printed),
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
    let target = super::target_line();
    let ir = answer(compiler, &format!("TARGET {target}\n{program}"));
    assert!(
        !ir.starts_with("ERR"),
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

/// Generic functions, structs and enums: one copy of a function for each type it is called at.
const GENERICS: &[&str] = &[
    "fn id[T](x: T) -> [] T { return x; }\nfn run() -> [] int { var t = id(7); if id(true) { t = t + 1; } return t; }",
    "struct Pair[A, B] { first: A, second: B }\nfn swap[A, B](p: Pair[A, B]) -> [] Pair[B, A] { let Pair { first, second } = p; return Pair { first: second, second: first }; }\nfn run() -> [] int { let q = swap(Pair { first: 3, second: true }); if q.first { return q.second + 10; } return 0; }",
    "enum Opt[T] { Nothing, Just(T) }\nfn or_else[T: val](o: Opt[T], d: T) -> [] T { match o { Opt::Nothing => { return d; } Opt::Just(v) => { return v; } } }\nfn run() -> [] int { let a: Opt[int] = Opt::Just(5); let b: Opt[int] = Opt::Nothing; return or_else(a, 0) * 10 + or_else(b, 3) + or_else(Opt::Nothing, 100); }",
    "enum Opt[T] { Nothing, Just(T) }\nfn pick[T: val](o: Opt[T], d: T) -> [] T { match o { Opt::Nothing => { return d; } Opt::Just(v) => { return v; } } }\nfn twice[T: val](o: Opt[T], d: T) -> [] T { return pick(o, pick(Opt::Nothing, d)); }\nfn run() -> [] int { return twice(Opt::Just(9), 1) + twice(Opt::Nothing, 4); }",
    "struct Cell[T] { value: T, tag: int }\nfn tagged[T](v: T, t: int) -> [] Cell[T] { return Cell { value: v, tag: t }; }\nfn bump(c: Cell[bool]) -> [] int { if c.value { return c.tag + 1; } return 0; }\nfn run() -> [] int { return bump(tagged(true, 4)); }",
    "enum Opt[T] { Nothing, Just(T) }\nfn run() -> [] int { let a: Opt[int] = Opt::Just(9223372036854775807); match a { Opt::Just(v) => { return v + 1; } Opt::Nothing => { return 0; } } }",
];

/// Floating point: arithmetic, comparisons (NaN, infinity, minus zero), the conversions and the bit casts, `f32`,
/// floats in structs, slices, statics and payloads.
const FLOATS: &[&str] = &[
    "edition 6;\nfn run() -> [] int { let x = 1.5 + 2.25 * 4.0; return truncate(x * 10.0); }",
    "edition 6;\nfn run() -> [] int { let a = float_of(7); let b = float_of(2); return truncate(a / b * 100.0); }",
    "edition 6;\nfn run() -> [] int { let n = 0.0 / 0.0; var k = 0; if is_nan(n) { k = k + 1; } if n == n { k = k + 10; } if n != n { k = k + 100; } return k; }",
    "edition 6;\nfn run() -> [] int { let inf = 1.0 / 0.0; var k = 0; if inf > 1.0e308 { k = k + 1; } if -inf < -1.0e308 { k = k + 2; } if inf >= inf { k = k + 4; } return k; }",
    "edition 6;\nfn run() -> [] int { var k = 0; if -0.0 == 0.0 { k = k + 1; } if bits_of(-0.0) < 0 { k = k + 2; } if bits_of(0.0) == 0 { k = k + 4; } return k; }",
    "edition 6;\nfn run() -> [] int { return truncate(-2.7) + 10; }",
    "edition 6;\nfn run() -> [] int { return truncate(1.0e30); }",
    "edition 6;\nfn run() -> [] int { return truncate(0.0 / 0.0); }",
    "edition 6;\nfn run() -> [] int { return truncate(-9223372036854775808.0); }",
    "edition 6;\nfn run() -> [] int { return truncate(9223372036854774784.0) % 100; }",
    "edition 6;\nfn half(x: float) -> [] float { return x / 2.0; }
fn run() -> [] int { var x = 100.0; var i = 0; while i < 3 { x = half(x); i = i + 1; } return truncate(x * 8.0); }",
    "edition 6;\nstruct V { x: float, y: float }
fn dot(a: V, b: V) -> [] float { return a.x * b.x + a.y * b.y; }
fn run() -> [] int { return truncate(dot(V { x: 1.5, y: 2.0 }, V { x: 4.0, y: 0.25 }) * 4.0); }",
    "edition 6;\nfn run() -> [] int { region a { var t = alloc_slice[a](4, 0.5); t[1] = 2.5; t[2] = t[0] + t[1]; var s = 0.0; var i = 0; while i < len(t) { s = s + t[i]; i = i + 1; } return truncate(s * 10.0); } }",
    "edition 6;\nstatic table: [float] {
    let t = alloc_slice[static](4, 1.0);
    t[1] = 0.5;
    t[2] = t[1] / 4.0;
    return t;
}
fn run() -> [] int { return truncate((table[0] + table[1] + table[2]) * 16.0); }",
    "edition 6;\nfn run() -> [] int { let x = 16777217; let f = f32_of_int(x); return int_of_f32(f); }",
    "edition 6;\nfn run() -> [] int { let f = f32_of(0.1); let d = float_of32(f); if d == 0.1 { return 1; } return truncate(d * 1000.0); }",
    "edition 6;\nfn run() -> [] int { let f = f32_of_bits(1069547520); return int_of_f32(sqrt32(f * 4.0f32) * 10.0f32); }",
    "edition 6;\nfn run() -> [] int { return int_of_f32(1.5f32 + 2.25f32 * 4.0f32); }",
    "edition 7;\nfn run() -> [] int { var k = 0; if bits_of32(f32_of_bits(2143289345)) == 2143289344 { k = k + 1; } if bits_of32(1.0f32) == 1065353216 { k = k + 2; } if bits_of(float_of_bits(9218868437227405313)) == 9221120237041090560 { k = k + 4; } return k; }",
    "edition 6;\nfn run() -> [] int { return truncate(sqrt(2.0) * 1000.0) % 256; }",
    "edition 6;\nfn big() -> [] float { return 1.0e308; }
fn run() -> [] int { let x = big() * 10.0; if x > big() { return 7; } return 3; }",
    "edition 6;\nfn run() -> [] int { var s = 0.0; var i = 1; while i <= 10 { s = s + 1.0 / float_of(i); i = i + 1; } return truncate(s * 1000.0) % 256; }",
    "edition 6;\nfn pick(flag: bool, a: float, b: float) -> [] float { if flag { return a; } return b; }
fn run() -> [] int { return truncate(pick(true, 3.5, 1.0) + pick(false, 3.5, 1.0)); }",
];

/// Threads: `spawn` and `join`, a function held in a name, payloads and results of each kind that fits a pointer.
const THREADS: &[&str] = &[
    "edition 5;\nfn worker(x: int) -> [] int { return x * 2; }\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    let w = worker;\n    let h = spawn(21, w);\n    return join(h);\n}",
    "edition 5;\nfn one(x: int) -> [] int { return x; }\nfn two(x: int) -> [] int { return x + 1; }\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    let f = one;\n    let g = two;\n    let a = spawn(1, f);\n    let b = spawn(5, g);\n    return join(a) * 10 + join(b);\n}",
    "edition 5;\nfn flip(b: bool) -> [] bool { return !b; }\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    let f = flip;\n    let t = spawn(false, f);\n    if join(t) { return 7; }\n    return 1;\n}",
    "edition 5;\nfn up(b: byte) -> [] byte { return byte_of(int_of(b) + 1); }\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    let f = up;\n    let t = spawn(byte_of(65), f);\n    return int_of(join(t));\n}",
    "edition 5;\nstruct Counter { n: int }\nfn work[&r](c: &!r Counter) -> [] int { c.n = c.n + 5; return 0; }\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    var c = Counter { n: 1 };\n    borrow mut c as &!r in {\n        let f = work;\n        let a = spawn(r, f);\n        let x = join(a);\n        let b = spawn(r, f);\n        let y = join(b);\n    }\n    return c.n;\n}",
    "edition 5;\nfn worker(io: Io) -> [] int {\n    borrow mut io as &!i in { putchar(i, 72); putchar(i, 10); }\n    release(io);\n    return 7;\n}\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    let w = worker;\n    let h = spawn(io, w);\n    return join(h) - 7;\n}",
    "edition 5;\nfn worker(x: int) -> [] int { return x - 1; }\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    var i = 0;\n    var total = 0;\n    while i < 4 {\n        let w = worker;\n        total = total + join(spawn(i, w));\n        i = i + 1;\n    }\n    return total + 10;\n}",
    "edition 5;\nfn double(x: int) -> [] int { return x + x; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    let f = double;\n    return f(20) + f(1);\n}",
    "edition 5;\nfn boom(x: int) -> [] int { return x + 9223372036854775807; }\nfn main(world: World) -> [conc] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(io);\n    let f = boom;\n    return join(spawn(2, f));\n}",
];

/// Foreign functions: a whole number, a `c_int` result, a byte slice, and an opaque pointer.
const FOREIGN: &[&str] = &[
    "extern fn labs[&f](ffi: &f Ffi(\"libc\"), n: int) -> [ffi(\"libc\")] int;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io);\n    release(fs);\n    release(heap);\n    release(args);\n    let libc = narrow(ffi, \"libc\");\n    var r = 0;\n    borrow libc as &f in { r = labs(f, 0 - 5) + labs(f, 7); }\n    release(libc);\n    return r;\n}",
    "extern fn access[&f, &p](ffi: &f Ffi(\"libc\"), path: &p [byte], mode: int) -> [ffi(\"libc\")] c_int;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io);\n    release(fs);\n    release(heap);\n    release(args);\n    let libc = narrow(ffi, \"libc\");\n    var r = 0;\n    borrow libc as &f in {\n        region a {\n            let here = alloc_slice[a](2, byte_of(0));\n            here[0] = byte_of('.');\n            let nope = alloc_slice[a](3, byte_of(0));\n            nope[0] = byte_of('/');\n            nope[1] = byte_of('~');\n            r = access(f, here, 0) * 100 + access(f, nope, 0) + 50;\n        }\n    }\n    release(libc);\n    return r;\n}",
    "extern fn write[&f, &s](ffi: &f Ffi(\"libc\"), fd: int, buf: &s [byte], n: int) -> [ffi(\"libc\")] int;\nfn say[&f, &s](libc: &f Ffi(\"libc\"), line: &s [byte]) -> [ffi(\"libc\")] int {\n    return write(libc, 1, line, len(line));\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io);\n    release(fs);\n    release(heap);\n    release(args);\n    let libc = narrow(ffi, \"libc\");\n    var r = 0;\n    borrow libc as &f in { r = say(f, \"straight to fd 1\\n\"); }\n    release(libc);\n    return r;\n}",
    "edition 3;\nextern fn fdopen[&f, &m](ffi: &f Ffi(\"libc\"), fd: int, mode: &m [byte]) -> [ffi(\"libc\")] c_ptr;\nextern fn fclose[&f](ffi: &f Ffi(\"libc\"), stream: c_ptr) -> [ffi(\"libc\")] int;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net } = split(world);\n    release(io);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    let libc = narrow(ffi, \"libc\");\n    var r = 0;\n    borrow libc as &f in {\n        region a {\n            let mode = alloc_slice[a](2, byte_of(0));\n            mode[0] = byte_of('r');\n            let good = fdopen(f, 0, mode);\n            let bad = fdopen(f, 999, mode);\n            if good == null_ptr() { r = r + 1; }\n            if bad == null_ptr() { r = r + 10; }\n            if good != bad { r = r + 100 - 90; }\n            if good != null_ptr() { r = r + fclose(f, good); }\n        }\n    }\n    release(libc);\n    return r;\n}",
];

/// Sockets and the clock: the edition 2 integers (`bind`, `connect`, `listen`, `accept`), a connection made, accepted,
/// written and read inside the process, one refused, and the clocks.
const SOCKETS: &[&str] = &[
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(io);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(clock);\n    var code = 99;\n    borrow net as &n in {\n        match tcp_listen(n, 47123, 4, 0) {\n            Listening::Ok(l) => {\n                var listener = l;\n                match tcp_connect(n, \"127.0.0.1\", 47123) {\n                    Dialed::Ok(c) => {\n                        var client = c;\n                        borrow mut listener as &!lh in {\n                            match tcp_accept(lh) {\n                                Accepted::Ok(a) => {\n                                    var server = a;\n                                    region r {\n                                        let msg = alloc_slice[r](5, byte_of(104));\n                                        var sent = 0;\n                                        borrow mut client as &!ch in {\n                                            match conn_write(ch, msg) {\n                                                Sent::Wrote(k) => { sent = k; }\n                                                Sent::Again => { sent = 0 - 1; }\n                                                Sent::Failed(e) => { sent = 0 - 2; }\n                                            }\n                                        }\n                                        let into = alloc_slice[r](16, byte_of(0));\n                                        var got = 0 - 1;\n                                        borrow mut server as &!sh in {\n                                            match conn_read(sh, into) {\n                                                Received::Data(k) => { got = k; }\n                                                Received::End => { got = 0 - 3; }\n                                                Received::Again => { got = 0 - 4; }\n                                                Received::Failed(e) => { got = 0 - 5; }\n                                            }\n                                        }\n                                        code = sent * 10 + got + int_of(into[0]) - 104;\n                                    }\n                                    conn_close(server);\n                                }\n                                Accepted::Again => { code = 1; }\n                                Accepted::Failed(e) => { code = 2; }\n                            }\n                        }\n                        conn_close(client);\n                    }\n                    Dialed::Failed(e) => { code = 3; }\n                }\n                listener_close(listener);\n            }\n            Listening::Failed(e) => { code = 4; }\n        }\n    }\n    release(net);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(io);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(clock);\n    var code = 99;\n    borrow net as &n in {\n        match tcp_connect(n, \"127.0.0.1\", 1) {\n            Dialed::Ok(c) => { conn_close(c); code = 1; }\n            Dialed::Failed(e) => { if e > 0 { code = 0; } else { code = 2; } }\n        }\n    }\n    release(net);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(io);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    var code = 99;\n    borrow clock as &k in {\n        let a = clock_ms(k);\n        let b = clock_ms(k);\n        let w = clock_unix_ms(k);\n        code = 0;\n        if b < a { code = code + 1; }\n        if w < 1000000000000 { code = code + 2; }\n    }\n    release(clock);\n    return code;\n}",
    "edition 2;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net } = split(world);\n    release(io);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 5;\n    borrow net as &n in {\n        let l = bind(n, 47124);\n        if l >= 0 {\n            code = listen(l, 4) + 2;\n            let c = connect(n, \"127.0.0.1\", 47124);\n            let a = accept(l);\n            if c >= 0 && a >= 0 { code = code + 10; }\n        }\n    }\n    release(net);\n    return code;\n}",
    // A prelude enum matched through a reference (`&!l Listening`), the tag read through it and the payloads bound as references: found by `cancho-gateway`.
    "edition 6;\nfn payload[&l](l: &!l Listening) -> [] int {\n    var n = 0;\n    match l {\n        Listening::Ok(x) => { n = 7; }\n        Listening::Failed(e) => { n = 9; }\n    }\n    return n;\n}\nfn main(world: World) -> [] int {\n    release(world);\n    var listener = Listening::Failed(5);\n    var out = 0;\n    borrow mut listener as &!pw in { out = payload(pw); }\n    match listener {\n        Listening::Ok(x) => { listener_close(x); }\n        Listening::Failed(e) => { out = out + e; }\n    }\n    return out;\n}",
    "edition 6;\nfn failed[&l](l: &!l Listening) -> [] bool {\n    var f = false;\n    match l {\n        Listening::Ok(x) => { }\n        Listening::Failed(e) => { f = true; }\n    }\n    return f;\n}\nfn main(world: World) -> [] int {\n    release(world);\n    var a = Listening::Failed(0);\n    var n = 0;\n    borrow mut a as &!p in { if failed(p) { n = n + 1; } }\n    match a {\n        Listening::Ok(x) => { listener_close(x); }\n        Listening::Failed(e) => { }\n    }\n    return n + 40;\n}",
];

/// Pipes and a child: a program read through a channel, and one written to as well as read from.
const PIPES: &[&str] = &[
    "edition 7;\n\nfn run[&x, &i](exec: &x Exec(\"/bin\"), io: &!i Io) -> [exec(\"/bin\"), pipe_read, io_write] int {\n    var status = 100;\n    match pipe_open() {\n        Piped::Ok(from, out_end) => {\n            var reader = from;\n            match exec_spawn(exec, \"/bin/echo\", \"hello\\0world\\0\", \"\", Stdio::Null, Stdio::Pipe(out_end), Stdio::Null) {\n                Spawned::Ok(c) => {\n                    region a {\n                        let buf = alloc_slice[a](64, byte_of(0));\n                        var going = true;\n                        while going {\n                            borrow mut reader as &!r in {\n                                match pipe_read(r, buf) {\n                                    Received::Data(n) => {\n                                        write_bytes(io, buf[0..n]);\n                                    }\n                                    Received::End => {\n                                        going = false;\n                                    }\n                                    Received::Again => {\n                                    }\n                                    Received::Failed(e) => {\n                                        going = false;\n                                    }\n                                }\n                            }\n                        }\n                    }\n                    match child_wait(c) {\n                        Exited::Code(n) => {\n                            status = n;\n                        }\n                        Exited::Signaled(s) => {\n                            status = 200 + s;\n                        }\n                        Exited::Failed(e) => {\n                            status = 150;\n                        }\n                    }\n                }\n                Spawned::Failed(e) => {\n                    status = e;\n                }\n            }\n            pipe_close(reader);\n        }\n        Piped::Failed(e) => {\n            status = 99;\n        }\n    }\n    return status;\n}\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(signals);\n    let bin = narrow(exec, \"/bin\");\n    var status = 0;\n    var console = io;\n    borrow bin as &b in {\n        borrow mut console as &!i in {\n            status = run(b, i);\n        }\n    }\n    release(bin);\n    release(console);\n    return status;\n}\n",
    "edition 7;\n\nfn run[&x, &i](exec: &x Exec(\"/bin\"), io: &!i Io) -> [exec(\"/bin\"), pipe_read, pipe_write, io_write] int {\n    var status = 100;\n    match pipe_open() {\n        Piped::Ok(from, out_end) => {\n            var reader = from;\n            match pipe_open() {\n                Piped::Ok(to, in_end) => {\n                    var writer = to;\n                    region w {\n                        let hello = alloc_slice[w](3, byte_of(104));\n                        borrow mut writer as &!wr in {\n                            match pipe_write(wr, hello) {\n                                Sent::Wrote(k) => { status = k; }\n                                Sent::Again => { status = 98; }\n                                Sent::Failed(e) => { status = 97; }\n                            }\n                        }\n                    }\n                    pipe_close(writer);\n                    match exec_spawn(exec, \"/bin/cat\", \"\", \"\", Stdio::Pipe(in_end), Stdio::Pipe(out_end), Stdio::Null) {\n                        Spawned::Ok(c) => {\n                            region a {\n                                let buf = alloc_slice[a](64, byte_of(0));\n                                var going = true;\n                                var total = 0;\n                                while going {\n                                    borrow mut reader as &!r in {\n                                        match pipe_read(r, buf) {\n                                            Received::Data(n) => { write_bytes(io, buf[0..n]); total = total + n; }\n                                            Received::End => { going = false; }\n                                            Received::Again => { }\n                                            Received::Failed(e) => { going = false; }\n                                        }\n                                    }\n                                }\n                                status = status * 10 + total;\n                            }\n                            match child_wait(c) {\n                                Exited::Code(n) => { status = status + n; }\n                                Exited::Signaled(s) => { status = 200 + s; }\n                                Exited::Failed(e) => { status = 150; }\n                            }\n                        }\n                        Spawned::Failed(e) => { status = e; }\n                    }\n                }\n                Piped::Failed(e) => { child_end_close(out_end); status = 96; }\n            }\n            pipe_close(reader);\n        }\n        Piped::Failed(e) => { status = 99; }\n    }\n    return status;\n}\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    release(net);\n    release(clock);\n    release(signals);\n    let bin = narrow(exec, \"/bin\");\n    var status = 0;\n    var console = io;\n    borrow bin as &b in {\n        borrow mut console as &!i in {\n            status = run(b, i);\n        }\n    }\n    release(bin);\n    release(console);\n    return status;\n}",
];

/// `defer`: run at every exit of its block, innermost and last-written first, after the value of a `return` is read.
const DEFERS: &[&str] = &[
    "struct C { n: int }\nfn bump[&r](c: &!r C) -> [] int { c.n = c.n * 10 + 1; return 0; }\nfn bump2[&r](c: &!r C) -> [] int { c.n = c.n * 10 + 2; return 0; }\nfn f[&r](c: &!r C, flag: bool) -> [] int {\n    defer bump(c);\n    if flag {\n        defer bump2(c);\n        return 5;\n    }\n    return 6;\n}\nfn run() -> [] int {\n    var c = C { n: 0 };\n    var a = 0;\n    borrow mut c as &!r in {\n        a = f(r, true);\n        a = a + f(r, false);\n    }\n    return c.n % 251 + a;\n}",
    "struct C { n: int }\nfn bump[&r](c: &!r C) -> [] int { c.n = c.n + 1; return 0; }\nfn run() -> [] int {\n    var c = C { n: 0 };\n    borrow mut c as &!r in {\n        var i = 0;\n        while i < 5 {\n            defer bump(r);\n            i = i + 1;\n        }\n        defer bump(r);\n    }\n    return c.n;\n}",
    "res struct T { fd: int }\nfn open(n: int) -> [] T { return T { fd: n }; }\nfn close(t: T) -> [] int { let T { fd } = t; return fd; }\nfn take(flag: bool) -> [] int {\n    let t = open(7);\n    defer close(t);\n    if flag { return 2; }\n    return 4;\n}\nfn run() -> [] int { return take(true) * 10 + take(false); }",
];

/// Words: `load_le64`, `byte_mask64`, the bit counts, `mul_wide`, `add_carry`, `sub_borrow`, `value_barrier`,
/// `copy_within` and `trap`, with the traps their bounds make.
const WORDS: &[&str] = &[
    "edition 9;\nfn run() -> [] int { region a { let t = alloc_slice[a](16, byte_of(0)); t[3] = byte_of(1); t[9] = byte_of(255); return load_le64(t, 0) % 1000 + load_le64(t, 8) % 997; } }",
    "edition 9;\nfn run() -> [] int { region a { let t = alloc_slice[a](8, byte_of(0)); return load_le64(t, 1); } }",
    "edition 9;\nfn run() -> [] int { region a { let t = alloc_slice[a](128, byte_of(5)); t[3] = byte_of(9); t[64] = byte_of(9); t[100] = byte_of(9); let m = byte_mask64(t, 0, byte_of(9)); let n = byte_mask64(t, 64, byte_of(9)); return popcount(m) * 100 + trailing_zeros(m) * 10 + popcount(n) + leading_zeros(n) % 7; } }",
    "edition 9;\nfn run() -> [] int { region a { let t = alloc_slice[a](63, byte_of(5)); return byte_mask64(t, 0, byte_of(5)); } }",
    "edition 9;\nfn run() -> [] int { return trailing_zeros(0) + leading_zeros(0) + popcount(-1) + trailing_zeros(48) + leading_zeros(1); }",
    "edition 9;\nfn run() -> [] int { let (h, l) = mul_wide(-1, -1); let (h2, l2) = mul_wide(1000003, 99991); return h % 100 + l % 100 + h2 + l2 % 1000; }",
    "edition 9;\nfn run() -> [] int { let (s, c) = add_carry(-1, 1, 0); let (s2, c2) = add_carry(-1, -1, 1); let (d, b) = sub_borrow(0, 1, 0); let (d2, b2) = sub_borrow(5, 3, 1); return c * 1000 + c2 * 100 + b * 10 + b2 + s % 7 + s2 % 11 + d % 13 + d2; }",
    "edition 9;\nfn run() -> [] int { return value_barrier(42) + value_barrier((3 ^ 5) - 1 >> 63); }",
    "edition 9;\nfn run() -> [] int { region a { let t = alloc_slice[a](10, byte_of(0)); var i = 0; while i < 10 { t[i] = byte_of(i + 1); i = i + 1; } copy_within(t, 2, 0, 5); copy_within(t, 0, 3, 7); return int_of(t[0]) * 1000 + int_of(t[4]) * 100 + int_of(t[9]); } }",
    "edition 9;\nfn run() -> [] int { region a { let t = alloc_slice[a](10, byte_of(0)); copy_within(t, 6, 0, 5); return 0; } }",
    "edition 9;\nfn run() -> [] int { if 1 < 2 { trap(); } return 3; }",
];

/// A run of bytes held in a struct, an enum and a tuple, and taken out again.
const HELD: &[&str] = &[
    "struct W { text: &static [byte], n: int }\nfn run() -> [] int { let w = W { text: \"hello\", n: 3 }; return len(w.text) * 10 + w.n + int_of(w.text[1]) % 10; }",
    "enum Tok { Word(&static [byte]), Num(int), End }\nfn size(t: Tok) -> [] int { match t { Tok::Word(s) => { return len(s) + int_of(s[0]) % 5; } Tok::Num(n) => { return n; } Tok::End => { return 0 - 1; } } }\nfn run() -> [] int { return size(Tok::Word(\"abcd\")) * 100 + size(Tok::Num(7)) + size(Tok::End); }",
    "fn cut(t: &static [byte], k: int) -> [] (&static [byte], &static [byte]) { return (t[0..k], t[k..len(t)]); }\nfn run() -> [] int { let (a, b) = cut(\"abcdef\", 2); return len(a) * 10 + len(b) + int_of(b[0]) % 7; }",
    "struct W { text: &static [byte], n: int }\nfn width[&r](w: &r W) -> [] int { return len(w.text) + w.n; }\nfn run() -> [] int { var w = W { text: \"abc\", n: 5 }; var out = 0; borrow w as &r in { out = width(r); } return out; }",
    "struct W { text: &static [byte], n: int }\nfn run() -> [] int { let w = W { text: \"abc\", n: 5 }; let W { text, n } = w; return len(text) * 10 + n; }",
];

/// The hardware AES and GHASH builtins: one block, counter mode, the powers of H, the tag and its verdict, and the
/// length check that traps.
const CRYPTO: &[&str] = &[
    "edition 9;\nfn run() -> [] int { if !hw_sha256() { return 0; } region a { let state = alloc_slice[a](8, 0); let block = alloc_slice[a](16, 7); sha256_block_words(state, block); let data = alloc_slice[a](128, byte_of(1)); sha256_blocks(state, data); return state[0] % 251 + state[7] % 17; } }",
    "edition 9;\nfn run() -> [] int { if !hw_sha256() { trap(); } region a { let state = alloc_slice[a](8, 0); let data = alloc_slice[a](100, byte_of(1)); sha256_blocks(state, data); return 0; } }",
    "edition 9;\nfn run() -> [] int { if !hw_aes_gcm() { return 0; } region a { let keys = alloc_slice[a](176, byte_of(7)); let block = alloc_slice[a](16, byte_of(3)); let out = alloc_slice[a](16, byte_of(0)); aes_encrypt_block(keys, 10, block, out); var s = 0; var i = 0; while i < 16 { s = s + int_of(out[i]) * (i + 1); i = i + 1; } return s % 251; } }",
    "edition 9;\nfn run() -> [] int { if !hw_aes_gcm() { return 0; } region a { let keys = alloc_slice[a](240, byte_of(9)); let nonce = alloc_slice[a](12, byte_of(1)); let input = alloc_slice[a](100, byte_of(5)); let out = alloc_slice[a](100, byte_of(0)); aes_ctr32(keys, 14, nonce, 7, input, out); var s = 0; var i = 0; while i < 100 { s = s + int_of(out[i]) * (i + 1); i = i + 1; } return s % 251; } }",
    "edition 9;\nfn run() -> [] int { if !hw_aes_gcm() { return 0; } region a { let keys = alloc_slice[a](192, byte_of(2)); let table = alloc_slice[a](128, byte_of(0)); ghash_powers(keys, 11, table); let nonce = alloc_slice[a](12, byte_of(4)); let aad = alloc_slice[a](20, byte_of(6)); let text = alloc_slice[a](50, byte_of(8)); let tag = alloc_slice[a](16, byte_of(0)); gcm_tag(keys, 11, table, nonce, aad, text, tag); let d = gcm_tag_diff(keys, 11, table, nonce, aad, text, tag); var s = d; var i = 0; while i < 16 { s = s * 3 + int_of(tag[i]); s = s % 100003; i = i + 1; } return s % 251; } }",
    "edition 9;\nfn run() -> [] int { if !hw_aes_gcm() { trap(); } region a { let keys = alloc_slice[a](100, byte_of(7)); let block = alloc_slice[a](16, byte_of(3)); let out = alloc_slice[a](16, byte_of(0)); aes_encrypt_block(keys, 10, block, out); return 0; } }",
];

/// Runs of structs: filled, written and read an element at a time, cut.
const RUNS: &[&str] = &[
    "struct P { a: int, b: bool }\nfn run() -> [] int { region r { let ps = alloc_slice[r](3, P { a: 1, b: true }); ps[1] = P { a: 5, b: false }; var s = 0; var i = 0; while i < len(ps) { s = s + ps[i].a; if ps[i].b { s = s + 100; } i = i + 1; } return s; } }",
    "struct P { a: int, b: bool }\nfn total(ps: &static [P]) -> [] int { var s = 0; var i = 0; while i < len(ps) { s = s + ps[i].a; i = i + 1; } return s; }\nfn run() -> [] int { region r { let ps = alloc_slice[r](4, P { a: 3, b: true }); ps[2] = P { a: 10, b: false }; let part = ps[1..3]; return part[1].a * 10 + len(part); } }",
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

/// Programs that print: a `main` with the whole `World`, split, the parts it does not use released, the
/// `Io` borrowed for the block that writes. What they write to the standard output must be the same.
const OUTPUT: &[&str] = &[
    "fn say[&i](io: &!i Io, c: int) -> [io_write] int { return putchar(io, c); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in {\n        say(r, 72);\n        say(r, 105);\n        say(r, 10);\n    }\n    release(io);\n    return 0;\n}",
    "fn put[&i](io: &!i Io, c: int) -> [io_write] int { return putchar(io, c); }\nfn hello[&i](io: &!i Io) -> [io_write] int {\n    let a = put(io, 72); let b = put(io, 101); let c = put(io, 108); let d = put(io, 108); let e = put(io, 111);\n    let f = put(io, 44); let g = put(io, 32); let h = put(io, 119); let j = put(io, 111); let k = put(io, 114);\n    let l = put(io, 108); let m = put(io, 100); let n = put(io, 33); return put(io, 10);\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in { let x = hello(r); }\n    release(io);\n    return 0;\n}",
    "fn print_nat[&i](io: &!i Io, n: int) -> [io_write] int {\n    if n >= 10 { let r = print_nat(io, n / 10); }\n    return putchar(io, 48 + n % 10);\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in { let a = print_nat(r, 12345); let b = putchar(r, 10); }\n    release(io);\n    return 0;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in {\n        var c = 97;\n        while c < 123 { let x = putchar(r, c); c = c + 1; }\n        let y = putchar(r, 10);\n    }\n    release(io);\n    return 0;\n}",
    "fn print_nat[&i](io: &!i Io, n: int) -> [io_write] int {\n    if n >= 10 { let r = print_nat(io, n / 10); }\n    return putchar(io, 48 + n % 10);\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in {\n        var i = 1;\n        while i <= 15 {\n            if i % 15 == 0 { let a = putchar(r, 70); let b = putchar(r, 66); }\n            else if i % 5 == 0 { let a = putchar(r, 66); }\n            else if i % 3 == 0 { let a = putchar(r, 70); }\n            else { let a = print_nat(r, i); }\n            let n = putchar(r, 10);\n            i = i + 1;\n        }\n    }\n    release(io);\n    return 0;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in { let a = putchar(r, 79); let b = putchar(r, 75); let c = putchar(r, 10); }\n    release(io);\n    return 7;\n}",
    "fn big(n: int) -> [] int { return n + 1; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in { let a = putchar(r, 97); let b = putchar(r, 98); }\n    release(io);\n    return big(9223372036854775807);\n}",
    "fn line[&i](io: &!i Io, c: int, n: int) -> [io_write] int {\n    var k = 0;\n    while k < n { let x = putchar(io, c); k = k + 1; }\n    return putchar(io, 10);\n}\nfn tri[&i](io: &!i Io, n: int) -> [io_write] int {\n    var k = 1;\n    while k <= n { let x = line(io, 42, k); k = k + 1; }\n    return 0;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in { let t = tri(r, 5); }\n    release(io);\n    return 0;\n}",
    "fn print_nat[&i](io: &!i Io, n: int) -> [io_write] int {\n    if n >= 10 { let r = print_nat(io, n / 10); }\n    return putchar(io, 48 + n % 10);\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    var steps = 0;\n    borrow mut io as &!r in {\n        var x = 27;\n        while x != 1 {\n            if x % 2 == 0 { x = x / 2; } else { x = 3 * x + 1; }\n            steps = steps + 1;\n        }\n        let a = print_nat(r, steps);\n        let b = putchar(r, 10);\n    }\n    release(io);\n    return steps % 256;\n}",
    "fn print_int[&i](io: &!i Io, n: int) -> [io_write] int {\n    if n < 0 { let m = putchar(io, 45); return print_nat(io, 0 - n); }\n    return print_nat(io, n);\n}\nfn print_nat[&i](io: &!i Io, n: int) -> [io_write] int {\n    if n >= 10 { let r = print_nat(io, n / 10); }\n    return putchar(io, 48 + n % 10);\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in { let a = print_int(r, 0 - 4096); let b = putchar(r, 10); let c = print_int(r, 0); let d = putchar(r, 10); }\n    release(io);\n    return 0;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!a in { let x = putchar(a, 49); }\n    borrow mut io as &!b in { let y = putchar(b, 50); let z = putchar(b, 10); }\n    release(io);\n    return 0;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    var r = 0;\n    borrow mut io as &!p in { r = putchar(p, 65); let n = putchar(p, 10); }\n    release(io);\n    return r % 256;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    borrow mut io as &!r in {\n        var i = 0;\n        while i < 1000 { let c = putchar(r, 48 + i % 10); if i % 50 == 49 { let n = putchar(r, 10); } i = i + 1; }\n    }\n    release(io);\n    return 0;\n}",
    "fn bump[&i](io: &!i Io, n: int) -> [io_write] int { return putchar(io, 48 + n); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n        release(ffi);\n        release(fs);\n        release(heap);\n        release(args);\n    var n = 3;\n    borrow mut io as &!r in { borrow n as &m in { let a = bump(r, 4); } let b = putchar(r, 10); }\n    release(io);\n    return n;\n}",
];

/// Programs with strings: the literal as a slice of bytes, a slice passed to a function, `len` and
/// indexing (a bound that fails traps), `write_bytes`, bytes and whole numbers converted, a reference to
/// a whole number read and written through.
const STRINGS: &[&str] = &[
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = write_bytes(r, \"hello, world\\n\");\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = write_bytes(r, \"\");\n        let b = write_bytes(r, \"x\\n\");\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = write_bytes(r, \"tab:\\t| quote:\\\" | back:\\\\ | nul-less\\n\");\n    }\n    release(io);\n    return code;\n}",
    "fn put[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) { let c = putchar(io, int_of(s[k])); k = k + 1; }\n    return k;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = put(r, \"abc\");\n        let b = put(r, \"def\\n\");\n    }\n    release(io);\n    return code;\n}",
    "fn rev[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = len(s);\n    while k > 0 { k = k - 1; let c = putchar(io, int_of(s[k])); }\n    return 0;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = rev(r, \"stressed\");\n        let n = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
    "fn same[&a, &b](x: &a [byte], y: &b [byte]) -> [] int {\n    if len(x) != len(y) { return 0; }\n    var k = 0;\n    while k < len(x) { if x[k] != y[k] { return 0; } k = k + 1; }\n    return 1;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = same(\"alpha\", \"alpha\") * 10 + same(\"alpha\", \"alpine\") + same(\"\", \"\") * 100;\n    }\n    release(io);\n    return code;\n}",
    "fn rot[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) {\n        var c = int_of(s[k]);\n        if c >= 97 && c <= 122 { c = 97 + (c - 97 + 13) % 26; }\n        else if c >= 65 && c <= 90 { c = 65 + (c - 65 + 13) % 26; }\n        let o = putchar(io, c);\n        k = k + 1;\n    }\n    return 0;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = rot(r, \"Hello, World!\");\n        let n = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
    "fn nat[&i](io: &!i Io, n: int) -> [io_write] int {\n    if n >= 10 { let a = nat(io, n / 10); }\n    return putchar(io, 48 + n % 10);\n}\nfn words[&s](s: &s [byte]) -> [] int {\n    var k = 0;\n    var n = 0;\n    var inside = false;\n    while k < len(s) {\n        let c = int_of(s[k]);\n        if c == 32 { inside = false; }\n        else if !inside { inside = true; n = n + 1; }\n        k = k + 1;\n    }\n    return n;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = words(\"  the quick  brown fox \");\n        let a = nat(r, code);\n        let n = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
    "fn up[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) {\n        var c = int_of(s[k]);\n        if c >= 97 && c <= 122 { c = c - 32; }\n        let o = putchar(io, c);\n        k = k + 1;\n    }\n    return 0;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = up(r, \"mixed Case 123\");\n        let n = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
    "fn at[&s](s: &s [byte], i: int) -> [] int { return int_of(s[i]); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = write_bytes(r, \"before\\n\");\n        code = at(\"abcde\", 7);\n    }\n    release(io);\n    return code;\n}",
    "fn at[&s](s: &s [byte], i: int) -> [] int { return int_of(s[i]); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = at(\"abcde\", 4);\n    }\n    release(io);\n    return code;\n}",
    "fn at[&s](s: &s [byte], i: int) -> [] int { return int_of(s[i]); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = at(\"abcde\", 0 - 1);\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let s = \"abc\";\n        let t = s;\n        code = len(s) + len(t) * 10;\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        var s = \"x\";\n        s = \"yyyy\";\n        code = len(s);\n        let a = write_bytes(r, s);\n        let n = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
    "fn incr[&q](x: &!q int) -> [] int { *x = *x + 1; return *x; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        var n = 1;\n        borrow mut n as &!m in { code = incr(m); }\n        code = code + n * 10;\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        var n = 41;\n        borrow n as &m in { code = *m + 1; }\n    }\n    release(io);\n    return code;\n}",
    "fn sum[&s](s: &s [byte]) -> [] int {\n    var k = 0;\n    var t = 0;\n    while k < len(s) { t = t + int_of(s[k]); k = k + 1; }\n    return t;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = sum(\"abc\");\n    }\n    release(io);\n    return code;\n}",
    "fn put[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) { let c = putchar(io, int_of(s[k])); k = k + 1; }\n    return k;\n}\nfn nat[&i](io: &!i Io, n: int) -> [io_write] int {\n    if n >= 10 { let a = nat(io, n / 10); }\n    return putchar(io, 48 + n % 10);\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = put(r, \"x = \");\n        let b = nat(r, 4096);\n        let c = put(r, \", y = \");\n        let d = nat(r, 7);\n        let n = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = len(\"\") + len(\"a\") * 2 + len(\"abc\") * 4;\n    }\n    release(io);\n    return code;\n}",
    "fn put[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) { let c = putchar(io, int_of(s[k])); k = k + 1; }\n    return k;\n}\nfn twice[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int { let a = put(io, s); return put(io, s); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = twice(r, \"ab\");\n        let n = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        var b = byte_of(65);\n        let c = putchar(r, int_of(b));\n        code = int_of(byte_of(300));\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = int_of(byte_of(65 + 256 + 1)) + int_of(byte_of(0 - 1));\n    }\n    release(io);\n    return code;\n}",
    "fn pick[&s](s: &s [byte], i: int) -> [] int { if s[i] == s[0] { return 100; } return int_of(s[i]); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let s = \"hey\";\n        code = pick(s, 1) + pick(s, 2);\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        var buf = \"zzz\";\n        var n = 0;\n        while n < len(buf) { let o = putchar(r, int_of(buf[n])); n = n + 1; }\n        let nl = putchar(r, 10);\n    }\n    release(io);\n    return code;\n}",
];

/// Structs of whole numbers, bytes and booleans: a literal, a field read, a copy, a struct passed and
/// returned, a field written through a unique reference, `*r` of one.
const STRUCTS: &[&str] = &[
    "struct P { x: int, y: int }\nfn run() -> [] int { let p = P { x: 3, y: 4 }; return p.x * 10 + p.y; }",
    "struct P { x: int, y: int }\nfn run() -> [] int { let p = P { x: 3, y: 4 }; let q = p; return q.y - p.x; }",
    "struct P { x: int, y: int }\nfn mk(a: int, b: int) -> [] P { return P { x: a, y: b }; }\nfn run() -> [] int { let p = mk(7, 9); return p.y - p.x; }",
    "struct P { x: int, y: int }\nfn sum(p: P) -> [] int { return p.x + p.y; }\nfn run() -> [] int { return sum(P { x: 20, y: 22 }); }",
    "struct P { x: int, y: int }\nfn swap(p: P) -> [] P { return P { x: p.y, y: p.x }; }\nfn run() -> [] int { let q = swap(P { x: 1, y: 2 }); return q.x * 10 + q.y; }",
    "struct P { x: int, y: int }\nfn run() -> [] int { var p = P { x: 1, y: 2 }; p = P { x: p.y, y: p.x + 5 }; return p.x * 10 + p.y; }",
    "struct P { x: int, y: int }\nfn bump[&r](p: &!r P) -> [] int { p.x = p.x + 1; p.y = p.y * 2; return p.x + p.y; }\nfn run() -> [] int { var p = P { x: 1, y: 5 }; var n = 0; borrow mut p as &!q in { n = bump(q); n = n + bump(q); } return n + p.x * 100; }",
    "struct P { x: int, y: int }\nfn get[&r](p: &r P) -> [] int { return p.y; }\nfn run() -> [] int { let p = P { x: 1, y: 42 }; var n = 0; borrow p as &q in { n = get(q); } return n; }",
    "struct P { x: int, y: int }\nfn whole[&r](p: &r P) -> [] int { let c = *p; return c.x + c.y; }\nfn run() -> [] int { let p = P { x: 30, y: 12 }; var n = 0; borrow p as &q in { n = whole(q); } return n; }",
    "struct P { x: int, y: int }\nfn run() -> [] int { var p = P { x: 0, y: 0 }; var i = 0; while i < 10 { p = P { x: p.x + i, y: p.y + 2 }; i = i + 1; } return p.x + p.y; }",
    "struct Flags { a: bool, b: bool, n: int }\nfn run() -> [] int { let f = Flags { a: true, b: false, n: 5 }; if f.a && !f.b { return f.n; } return 0; }",
    "struct B { c: byte, n: int }\nfn run() -> [] int { let b = B { c: byte_of(200), n: 1 }; return int_of(b.c) + b.n; }",
    "struct B { c: byte, n: int }\nfn run() -> [] int { var b = B { c: byte_of(7), n: 1 }; b = B { c: byte_of(int_of(b.c) + 1), n: b.n }; return int_of(b.c); }",
    "struct One { v: int }\nfn inc(o: One) -> [] One { return One { v: o.v + 1 }; }\nfn run() -> [] int { return inc(inc(inc(One { v: 0 }))).v; }",
    "struct P { x: int, y: int }\nfn run() -> [] int { let big = 9223372036854775807; let p = P { x: big, y: 1 }; return p.x + p.y; }",
    "struct P { x: int, y: int }\nfn add1[&r](p: &!r P) -> [] int { p.x = p.x + 1; return 0; }\nfn run() -> [] int { var p = P { x: 9223372036854775807, y: 1 }; var n = 0; borrow mut p as &!q in { n = add1(q); } return n; }",
    "struct P { x: int, y: int }\nfn run() -> [] int { var p = P { x: 1, y: 1 }; var n = 0; borrow mut p as &!q in { q.x = 5; *q = P { x: q.x * 2, y: q.y + 1 }; n = q.x + q.y; } return n + p.x; }",
    "struct A { n: int }\nstruct C { m: int, ok: bool }\nfn run() -> [] int { let a = A { n: 3 }; let c = C { m: a.n * 2, ok: true }; if c.ok { return c.m; } return 0; }",
    "struct P { x: int, y: int }\nfn f(p: P, k: int) -> [] int { if k == 0 { return p.x; } return f(P { x: p.x + p.y, y: p.x }, k - 1); }\nfn run() -> [] int { return f(P { x: 1, y: 0 }, 10); }",
    "struct P { x: int, y: int }\nfn mk() -> [] P { let big = 9223372036854775807; return P { x: big + 1, y: 0 }; }\nfn run() -> [] int { return mk().x; }",
];

/// Enums of whole numbers, bytes and booleans, and `match` on them: by value, behind a reference (the
/// payloads are references, written through), with a `_` arm, nested, in a loop.
const ENUMS: &[&str] = &[
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn area(s: Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return r * r * 3; }\n        Shape::Rect(w, h) => { return w * h; }\n        Shape::Dot => { return 1; }\n    }\n}\nfn run() -> [] int { return area(Shape::Circle(5)); }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn area(s: Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return r * r * 3; }\n        Shape::Rect(w, h) => { return w * h; }\n        Shape::Dot => { return 1; }\n    }\n}\nfn run() -> [] int { return area(Shape::Rect(6, 7)); }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn area(s: Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return r * r * 3; }\n        Shape::Rect(w, h) => { return w * h; }\n        Shape::Dot => { return 1; }\n    }\n}\nfn run() -> [] int { return area(Shape::Dot); }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn area(s: Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return r * r * 3; }\n        Shape::Rect(w, h) => { return w * h; }\n        Shape::Dot => { return 1; }\n    }\n}\nfn run() -> [] int { let a = Shape::Circle(2); let b = a; return area(a) + area(b); }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn area(s: Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return r * r * 3; }\n        Shape::Rect(w, h) => { return w * h; }\n        Shape::Dot => { return 1; }\n    }\n}\nfn mk(k: int) -> [] Shape { if k == 0 { return Shape::Dot; } if k == 1 { return Shape::Circle(k + 4); } return Shape::Rect(k, k); }\nfn run() -> [] int { return area(mk(0)) + area(mk(1)) * 10 + area(mk(3)) * 1000; }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn run() -> [] int {\n    var i = 0;\n    var t = 0;\n    while i < 6 {\n        let s = Shape::Rect(i, i + 1);\n        match s {\n            Shape::Circle(r) => { t = t + r; }\n            Shape::Rect(w, h) => { t = t + w * h; }\n            Shape::Dot => { t = t + 100; }\n        }\n        i = i + 1;\n    }\n    return t;\n}",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn run() -> [] int { let s = Shape::Circle(9); var out = 0; match s { Shape::Circle(r) => { out = r; } _ => { out = 0 - 1; } } return out; }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn run() -> [] int { let s = Shape::Dot; var out = 5; match s { Shape::Circle(r) => { out = r; } _ => { out = 77; } } return out; }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn run() -> [] int { let s = Shape::Rect(2, 3); var out = 5; match s { Shape::Dot => { out = 1; } _ => { out = 2; } } return out; }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn run() -> [] int { let s = Shape::Rect(2, 3); var out = 5; match s { Shape::Rect(_, h) => { out = h; } _ => { out = 2; } } return out; }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn grow[&q](s: &!q Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { *r = *r + 1; }\n        Shape::Rect(w, h) => { *w = *w * 2; *h = 0; }\n        Shape::Dot => { }\n    }\n    return 0;\n}\nfn area(s: Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return r * r * 3; }\n        Shape::Rect(w, h) => { return w * h; }\n        Shape::Dot => { return 1; }\n    }\n}\nfn run() -> [] int { var c = Shape::Circle(2); var n = 0; borrow mut c as &!q in { n = grow(q); n = grow(q); } return area(c) + n; }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn grow[&q](s: &!q Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { *r = *r + 1; }\n        Shape::Rect(w, h) => { *w = *w * 2; *h = 0; }\n        Shape::Dot => { }\n    }\n    return 0;\n}\nfn area(s: Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return r * r * 3; }\n        Shape::Rect(w, h) => { return w * h; }\n        Shape::Dot => { return 1; }\n    }\n}\nfn run() -> [] int { var c = Shape::Rect(4, 9); var n = 0; borrow mut c as &!q in { n = grow(q); } return area(c) + 10; }",
    "enum Shape { Circle(int), Rect(int, int), Dot }\nfn read[&q](s: &q Shape) -> [] int {\n    match s {\n        Shape::Circle(r) => { return *r; }\n        Shape::Rect(w, h) => { return *w + *h; }\n        Shape::Dot => { return 0; }\n    }\n}\nfn run() -> [] int { let s = Shape::Rect(20, 22); var n = 0; borrow s as &q in { n = read(q); } return n; }",
    "enum Tri { Yes, No, Maybe(bool) }\nfn code(t: Tri) -> [] int {\n    match t {\n        Tri::Yes => { return 1; }\n        Tri::No => { return 2; }\n        Tri::Maybe(b) => { if b { return 3; } return 4; }\n    }\n}\nfn run() -> [] int { return code(Tri::Yes) + code(Tri::No) * 10 + code(Tri::Maybe(true)) * 100 + code(Tri::Maybe(false)) * 1000; }",
    "enum Byt { Raw(byte), Num(int) }\nfn amount(b: Byt) -> [] int {\n    match b {\n        Byt::Raw(c) => { return int_of(c); }\n        Byt::Num(n) => { return n; }\n    }\n}\nfn run() -> [] int { return amount(Byt::Raw(byte_of(200))) + amount(Byt::Num(7)); }",
    "enum Op { Add, Sub, Mul }\nfn apply(o: Op, a: int, b: int) -> [] int {\n    match o {\n        Op::Add => { return a + b; }\n        Op::Sub => { return a - b; }\n        Op::Mul => { return a * b; }\n    }\n}\nfn run() -> [] int { return apply(Op::Add, 3, 4) * 100 + apply(Op::Sub, 10, 4) * 10 + apply(Op::Mul, 2, 3); }",
    "enum Op { Add, Sub, Mul }\nfn apply(o: Op, a: int, b: int) -> [] int {\n    match o {\n        Op::Add => { return a + b; }\n        Op::Sub => { return a - b; }\n        Op::Mul => { return a * b; }\n    }\n}\nfn run() -> [] int { return apply(Op::Mul, 9223372036854775807, 2); }",
    "enum Op { Add, Sub, Mul }\nfn next(o: Op) -> [] Op {\n    match o {\n        Op::Add => { return Op::Sub; }\n        Op::Sub => { return Op::Mul; }\n        Op::Mul => { return Op::Add; }\n    }\n}\nfn code(o: Op) -> [] int { match o { Op::Add => { return 1; } Op::Sub => { return 2; } Op::Mul => { return 3; } } }\nfn run() -> [] int { var o = Op::Add; var t = 0; var i = 0; while i < 7 { t = t * 4 + code(o); o = next(o); i = i + 1; } return t % 251; }",
    "enum E { A(int), B(int) }\nfn run() -> [] int { var e = E::A(1); var out = 0; match e { E::A(x) => { match E::B(x + 5) { E::A(y) => { out = y; } E::B(z) => { out = z * 10; } } } E::B(w) => { out = w; } } return out; }",
    "enum E { A(int), B(int) }\nfn run() -> [] int { var e = E::A(1); var n = 0; borrow mut e as &!q in { match q { E::A(x) => { *x = 41; } E::B(x) => { *x = 0; } } } match e { E::A(x) => { n = x + 1; } E::B(x) => { n = x; } } return n; }",
];

/// Programs with a `main` of their own: an arena's bytes written out, and the heap -- `box_slice`, the
/// run read and written through `contents`, `unbox_slice`, a count that is negative or too large traps.
const OWN_MAIN: &[&str] = &[
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        region a {\n            var buf = alloc_slice[a](5, byte_of(65));\n            var i = 0;\n            while i < len(buf) { buf[i] = byte_of(65 + i); i = i + 1; }\n            let n = write_bytes(r, buf);\n            let c = putchar(r, 10);\n            code = n;\n        }\n    }\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 5, 7);\n            let n = unbox_slice(h, b);\n            code = n;\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 5, 7);\n            borrow mut b as &!q in {\n                var s = contents(q);\n                s[2] = 30;\n                var i = 0;\n                while i < len(s) { code = code + s[i]; i = i + 1; }\n            }\n            let n = unbox_slice(h, b);\n            code = code + n * 100;\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 4, byte_of(66));\n            borrow mut b as &!q in {\n                var s = contents(q);\n                s[1] = byte_of(200);\n                code = int_of(s[0]) + int_of(s[1]);\n            }\n            let n = unbox_slice(h, b);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 3, false);\n            borrow mut b as &!q in {\n                var s = contents(q);\n                s[1] = true;\n                if s[1] && !s[0] { code = 9; }\n            }\n            let n = unbox_slice(h, b);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn total[&s](s: &s [int]) -> [] int {\n    var k = 0;\n    var t = 0;\n    while k < len(s) { t = t + s[k]; k = k + 1; }\n    return t;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 6, 2);\n            borrow b as &q in {\n                code = total(contents(q));\n            }\n            let n = unbox_slice(h, b);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 0, 2);\n            let n = unbox_slice(h, b);\n            code = n + 40;\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 0, 2);\n            borrow mut b as &!q in { var s = contents(q); code = s[0]; }\n            let n = unbox_slice(h, b);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 3, 2);\n            borrow mut b as &!q in { var s = contents(q); code = s[3]; }\n            let n = unbox_slice(h, b);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 0 - 1, 2);\n            let n = unbox_slice(h, b);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let b = box_slice(h, 4611686018427387904, 2);\n            let n = unbox_slice(h, b);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            var i = 0;\n            while i < 100 {\n                let b = box_slice(h, 1000, i);\n                borrow mut b as &!q in { var s = contents(q); code = code + s[999]; }\n                let n = unbox_slice(h, b);\n                i = i + 1;\n            }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            let a = box_slice(h, 3, 1);\n            let b = box_slice(h, 2, 10);\n            borrow mut a as &!p in { borrow mut b as &!q in { var x = contents(p); var y = contents(q); x[0] = y[1] + 5; code = x[0] + x[1]; } }\n            let n = unbox_slice(h, b);\n            let m = unbox_slice(h, a);\n            code = code + n * 100 + m * 1000;\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn fill[&x](h: &!x Heap, n: int, v: int) -> [heap] int {\n    var out = 0;\n    let b = box_slice(h, n, v);\n    borrow mut b as &!q in {\n        var s = contents(q);\n        var i = 0;\n        while i < len(s) { s[i] = s[i] * i; i = i + 1; }\n        i = 0;\n        while i < len(s) { out = out + s[i]; i = i + 1; }\n    }\n    let k = unbox_slice(h, b);\n    return out + k;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            code = fill(h, 10, 3);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn fill[&x](h: &!x Heap, n: int, v: int) -> [heap] int {\n    let b = box_slice(h, n, v);\n    return unbox_slice(h, b);\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n            code = fill(h, 0 - 5, 3);\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let b = box_slice(h, 5, byte_of(72));\n            borrow b as &q in {\n                let s = contents(q);\n                let w = write_bytes(r, s);\n                let c = putchar(r, 10);\n                code = w;\n            }\n            let n = unbox_slice(h, b);\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "static hello: [byte] {\n    return \"hi there\\n\";\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        code = write_bytes(r, hello);\n    }\n    release(io);\n    return code;\n}",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    var code = 0;\n    borrow mut io as &!r in {\n        let a = write_bytes(r, word[7..12]);\n        let c = putchar(r, 32);\n        let b = write_bytes(r, word[0..5]);\n        let d = putchar(r, 10);\n        code = a + b;\n    }\n    release(io);\n    return code;\n}",
    "edition 5;\nres struct Buf { held: Box[[byte]], used: int }\nfn empty[&h](heap: &!h Heap, capacity: int) -> [heap] Buf { return Buf { held: box_slice(heap, capacity, byte_of(0)), used: 0 }; }\nfn drop[&h](heap: &!h Heap, b: Buf) -> [heap] int { let Buf { held, used } = b; unbox_slice(heap, held); return used; }\nfn size[&b](b: &b Buf) -> [] int { return b.used; }\nfn push[&h](heap: &!h Heap, b: Buf, value: byte) -> [heap] Buf {\n    let Buf { held, used } = b;\n    var capacity = 0;\n    borrow held as &r in { capacity = len(contents(r)); }\n    if used + 1 <= capacity {\n        borrow mut held as &!w in { let s = contents(w); s[used] = value; }\n        return Buf { held: held, used: used + 1 };\n    }\n    let bigger = box_slice(heap, capacity * 2 + 1, byte_of(0));\n    borrow mut bigger as &!w in {\n        borrow held as &r in { let from = contents(r); let to = contents(w); let n = copy_into(to, from[0..used]); to[used] = value; }\n    }\n    unbox_slice(heap, held);\n    return Buf { held: bigger, used: used + 1 };\n}\nfn bytes[&b](b: &b Buf) -> [] &b [byte] { return contents(b.held)[0..b.used]; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            var b = empty(h, 2);\n            var i = 0;\n            while i < 10 { b = push(h, b, byte_of(65 + i)); i = i + 1; }\n            borrow b as &q in {\n                let w = write_bytes(r, bytes(q));\n                let c = putchar(r, 10);\n                code = w * 100 + size(q);\n            }\n            let n = drop(h, b);\n            code = code + n;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nres struct Buf { held: Box[[byte]], used: int }\nfn empty[&h](heap: &!h Heap, capacity: int) -> [heap] Buf { return Buf { held: box_slice(heap, capacity, byte_of(0)), used: 0 }; }\nfn drop[&h](heap: &!h Heap, b: Buf) -> [heap] int { let Buf { held, used } = b; unbox_slice(heap, held); return used; }\nfn size[&b](b: &b Buf) -> [] int { return b.used; }\nfn push[&h](heap: &!h Heap, b: Buf, value: byte) -> [heap] Buf {\n    let Buf { held, used } = b;\n    var capacity = 0;\n    borrow held as &r in { capacity = len(contents(r)); }\n    if used + 1 <= capacity {\n        borrow mut held as &!w in { let s = contents(w); s[used] = value; }\n        return Buf { held: held, used: used + 1 };\n    }\n    let bigger = box_slice(heap, capacity * 2 + 1, byte_of(0));\n    borrow mut bigger as &!w in {\n        borrow held as &r in { let from = contents(r); let to = contents(w); let n = copy_into(to, from[0..used]); to[used] = value; }\n    }\n    unbox_slice(heap, held);\n    return Buf { held: bigger, used: used + 1 };\n}\nfn bytes[&b](b: &b Buf) -> [] &b [byte] { return contents(b.held)[0..b.used]; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let b = empty(h, 4);\n            let n = drop(h, b);\n            code = n;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nres struct Buf { held: Box[[byte]], used: int }\nfn empty[&h](heap: &!h Heap, capacity: int) -> [heap] Buf { return Buf { held: box_slice(heap, capacity, byte_of(0)), used: 0 }; }\nfn drop[&h](heap: &!h Heap, b: Buf) -> [heap] int { let Buf { held, used } = b; unbox_slice(heap, held); return used; }\nfn size[&b](b: &b Buf) -> [] int { return b.used; }\nfn push[&h](heap: &!h Heap, b: Buf, value: byte) -> [heap] Buf {\n    let Buf { held, used } = b;\n    var capacity = 0;\n    borrow held as &r in { capacity = len(contents(r)); }\n    if used + 1 <= capacity {\n        borrow mut held as &!w in { let s = contents(w); s[used] = value; }\n        return Buf { held: held, used: used + 1 };\n    }\n    let bigger = box_slice(heap, capacity * 2 + 1, byte_of(0));\n    borrow mut bigger as &!w in {\n        borrow held as &r in { let from = contents(r); let to = contents(w); let n = copy_into(to, from[0..used]); to[used] = value; }\n    }\n    unbox_slice(heap, held);\n    return Buf { held: bigger, used: used + 1 };\n}\nfn bytes[&b](b: &b Buf) -> [] &b [byte] { return contents(b.held)[0..b.used]; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            var b = empty(h, 0);\n            b = push(h, b, byte_of(200));\n            borrow b as &q in { code = int_of(bytes(q)[0]); }\n            let n = drop(h, b);\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            code = wrapping_mul(9223372036854775807, 2) + wrapping_add(9223372036854775807, 1) % 1000 + wrapping_sub(0 - 9223372036854775807 - 1, 1) % 1000;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let t = \"hello\";\n            code = index_of_byte(t, byte_of(108)) * 10 + index_of_byte(t, byte_of(122)) + 5;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            region q {\n                var buf = alloc_slice[q](8, byte_of(0));\n                let n = copy_into(buf, \"abc\");\n                let m = copy_into(buf[3..8], \"defgh\");\n                code = n * 10 + m + int_of(buf[7]);\n            }\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            region q {\n                var buf = alloc_slice[q](2, byte_of(0));\n                let n = copy_into(buf, \"abc\");\n                code = n;\n            }\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let c = getchar(r);\n            code = 0 - c;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 5;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    release(net);\n    release(clock);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            code = write_err(r, \"to stderr\\n\");\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "edition 7;\nfn put[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) { let c = putchar(io, int_of(s[k])); k = k + 1; }\n    return k;\n}\nfn roundtrip[&f, &i](fs: &f Fs(\"/tmp\"), io: &!i Io) -> [fs_read(\"/tmp\"), fs_write(\"/tmp\"), file_read, file_write, io_write] int {\n    var status = 0;\n    match open_write(fs, \"/tmp/cancho_selfhost_t1.txt\") {\n        Opened::Ok(file) => {\n            var w = file;\n            borrow mut w as &!x in {\n                match file_write(x, \"hello, file\\n\") {\n                    Done::Ok(n) => { status = n; }\n                    Done::Failed(e) => { status = 900 + e; }\n                }\n            }\n            file_close(w);\n        }\n        Opened::Failed(e) => { status = 800 + e; }\n    }\n    match open_read(fs, \"/tmp/cancho_selfhost_t1.txt\") {\n        Opened::Ok(file) => {\n            var r = file;\n            region a {\n                let buf = alloc_slice[a](64, byte_of(0));\n                borrow mut r as &!x in {\n                    match file_read(x, buf) {\n                        Read::Got(n) => { put(io, buf[0..n]); status = status + n * 100; }\n                        Read::End => { status = status + 1; }\n                        Read::Failed(e) => { status = status + 2; }\n                    }\n                }\n            }\n            file_close(r);\n        }\n        Opened::Failed(e) => { status = status + 700 + e; }\n    }\n    return status;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(exec);\n    let tmp = narrow(fs, \"/tmp\");\n    var status = 0;\n    borrow tmp as &f in {\n        borrow mut io as &!i in { status = roundtrip(f, i); }\n    }\n    release(tmp);\n    release(io);\n    return status % 256;\n}",
    "edition 7;\nfn roundtrip[&f](fs: &f Fs(\"/tmp\")) -> [fs_read(\"/tmp\"), fs_write(\"/tmp\")] int {\n    var status = 0;\n    match open_read(fs, \"/tmp/cancho_selfhost_does_not_exist\") {\n        Opened::Ok(file) => { status = file_close(file); }\n        Opened::Failed(e) => { status = status + e; }\n    }\n    match open_write(fs, \"/tmp/cancho_selfhost_t2.txt\") {\n        Opened::Ok(file) => { file_close(file); }\n        Opened::Failed(e) => { status = status + 100; }\n    }\n    match open_new(fs, \"/tmp/cancho_selfhost_t2.txt\") {\n        Opened::Ok(file) => { file_close(file); status = status + 1000; }\n        Opened::Failed(e) => { status = status + e * 10; }\n    }\n    match open_append(fs, \"/tmp/cancho_selfhost_t2.txt\") {\n        Opened::Ok(file) => { file_close(file); status = status + 3; }\n        Opened::Failed(e) => { status = status + 500; }\n    }\n    match open_rw(fs, \"/tmp/cancho_selfhost_t2.txt\") {\n        Opened::Ok(file) => { file_close(file); status = status + 4; }\n        Opened::Failed(e) => { status = status + 600; }\n    }\n    return status;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(exec); release(io);\n    let tmp = narrow(fs, \"/tmp\");\n    var status = 0;\n    borrow tmp as &f in { status = roundtrip(f); }\n    release(tmp);\n    return status % 256;\n}",
    "edition 7;\nfn probe[&f](fs: &f Fs(\"/tmp\")) -> [fs_read(\"/tmp\")] int {\n    match open_read(fs, \"/tmp/../etc/passwd\") {\n        Opened::Ok(file) => { return file_close(file); }\n        Opened::Failed(e) => { return 1; }\n    }\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(exec); release(io);\n    let tmp = narrow(fs, \"/tmp\");\n    var status = 0;\n    borrow tmp as &f in { status = probe(f); }\n    release(tmp);\n    return status;\n}",
    "edition 7;\nfn probe[&f](fs: &f Fs(\"/tmp\")) -> [fs_read(\"/tmp\")] int {\n    match open_read(fs, \"/etc/passwd\") {\n        Opened::Ok(file) => { return file_close(file); }\n        Opened::Failed(e) => { return 1; }\n    }\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(exec); release(io);\n    let tmp = narrow(fs, \"/tmp\");\n    var status = 0;\n    borrow tmp as &f in { status = probe(f); }\n    release(tmp);\n    return status;\n}",
    "edition 7;\nfn probe[&f](fs: &f Fs(\"/tmp\")) -> [fs_read(\"/tmp\")] int {\n    match open_read(fs, \"/tmpx/file\") {\n        Opened::Ok(file) => { return file_close(file); }\n        Opened::Failed(e) => { return 1; }\n    }\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(exec); release(io);\n    let tmp = narrow(fs, \"/tmp\");\n    var status = 0;\n    borrow tmp as &f in { status = probe(f); }\n    release(tmp);\n    return status;\n}",
    "edition 7;\nfn status_of(c: Child) -> [] int {\n    match child_wait(c) {\n        Exited::Code(n) => { return n; }\n        Exited::Signaled(s) => { return 100 + s; }\n        Exited::Failed(e) => { return 200; }\n    }\n}\nfn run[&x](exec: &x Exec(\"/usr/bin\")) -> [exec(\"/usr/bin\")] int {\n    var total = 0;\n    match exec_spawn(exec, \"/usr/bin/true\", \"\", \"\", Stdio::Null, Stdio::Null, Stdio::Null) {\n        Spawned::Ok(c) => { total = total + status_of(c); }\n        Spawned::Failed(e) => { total = total + 50; }\n    }\n    match exec_spawn(exec, \"/usr/bin/false\", \"\", \"\", Stdio::Null, Stdio::Null, Stdio::Null) {\n        Spawned::Ok(c) => { total = total + status_of(c) * 2; }\n        Spawned::Failed(e) => { total = total + 50; }\n    }\n    match exec_spawn(exec, \"/usr/bin/nonexistent_program\", \"\", \"\", Stdio::Null, Stdio::Null, Stdio::Null) {\n        Spawned::Ok(c) => { total = total + status_of(c); }\n        Spawned::Failed(e) => { total = total + e * 10; }\n    }\n    match exec_spawn(exec, \"/usr/bin/env\", \"FOO=bar\\0\", \"\", Stdio::Null, Stdio::Null, Stdio::Null) {\n        Spawned::Ok(c) => { total = total + status_of(c); }\n        Spawned::Failed(e) => { total = total + 1000; }\n    }\n    return total;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(fs);\n    let bin = narrow(exec, \"/usr/bin\");\n    var status = 0;\n    borrow bin as &x in { status = run(x); }\n    release(bin);\n    release(io);\n    return status % 256;\n}",
    "edition 7;\nfn run[&x](exec: &x Exec(\"/usr/bin\")) -> [exec(\"/usr/bin\")] int {\n    match exec_spawn(exec, \"/bin/echo\", \"\", \"\", Stdio::Null, Stdio::Null, Stdio::Null) {\n        Spawned::Ok(c) => { match child_wait(c) { Exited::Code(n) => { return n; } Exited::Signaled(s) => { return 100; } Exited::Failed(e) => { return 101; } } }\n        Spawned::Failed(e) => { return 1; }\n    }\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(fs);\n    let bin = narrow(exec, \"/usr/bin\");\n    var status = 0;\n    borrow bin as &x in { status = run(x); }\n    release(bin);\n    release(io);\n    return status;\n}",
    "edition 7;\nfn put[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) { let c = putchar(io, int_of(s[k])); k = k + 1; }\n    return k;\n}\nfn run[&x, &f, &i](exec: &x Exec(\"\"), fs: &f Fs(\"/tmp\"), io: &!i Io) -> [exec(\"\"), fs_read(\"/tmp\"), fs_write(\"/tmp\"), file_read, io_write] int {\n    var status = 0;\n    match open_write(fs, \"/tmp/cancho_selfhost_t3.txt\") {\n        Opened::Ok(out) => {\n            match exec_spawn(exec, \"/bin/echo\", \"from the child\\0\", \"\", Stdio::Null, Stdio::File(out), Stdio::Null) {\n                Spawned::Ok(c) => {\n                    match child_wait(c) {\n                        Exited::Code(n) => { status = n; }\n                        Exited::Signaled(s) => { status = 100; }\n                        Exited::Failed(e) => { status = 101; }\n                    }\n                }\n                Spawned::Failed(e) => { status = 150; }\n            }\n        }\n        Opened::Failed(e) => { status = 160; }\n    }\n    match open_read(fs, \"/tmp/cancho_selfhost_t3.txt\") {\n        Opened::Ok(file) => {\n            var r = file;\n            region a {\n                let buf = alloc_slice[a](64, byte_of(0));\n                borrow mut r as &!w in {\n                    match file_read(w, buf) {\n                        Read::Got(n) => { put(io, buf[0..n]); status = status + n; }\n                        Read::End => { status = status + 1; }\n                        Read::Failed(e) => { status = status + 2; }\n                    }\n                }\n            }\n            file_close(r);\n        }\n        Opened::Failed(e) => { status = status + 170; }\n    }\n    return status;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals);\n    let tmp = narrow(fs, \"/tmp\");\n    var status = 0;\n    borrow exec as &x in {\n        borrow tmp as &f in {\n            borrow mut io as &!i in { status = run(x, f, i); }\n        }\n    }\n    release(tmp);\n    release(exec);\n    release(io);\n    return status % 256;\n}",
    "\nfn put[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {\n    var k = 0;\n    while k < len(s) { let c = putchar(io, int_of(s[k])); k = k + 1; }\n    return k;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi); release(fs); release(heap);\n    var count = 0;\n    borrow args as &a in {\n        borrow mut io as &!i in {\n            count = arg_count(a);\n            let name = arg(a, 0);\n            if len(name) > 0 { let p = putchar(i, 78); }\n            let n = putchar(i, 48 + count);\n            let nl = putchar(i, 10);\n        }\n    }\n    release(args);\n    release(io);\n    return count;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let b = box(h, 41);\n            let n = unbox(h, b);\n            code = n + 1;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "struct Point {\n    x: int,\n    y: int,\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let b = box(h, Point { x: 3, y: 4 });\n            borrow b as &q in { code = contents(q).x * 10 + contents(q).y; }\n            let p = unbox(h, b);\n            code = code + p.x;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "struct Point {\n    x: int,\n    y: int,\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            var b = box(h, Point { x: 3, y: 4 });\n            borrow mut b as &!w in { let p = contents(w); p.x = 30; p.y = p.y + 1; }\n            let p = unbox(h, b);\n            code = p.x + p.y;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let b = box(h, true);\n            let v = unbox(h, b);\n            if v { code = 7; }\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "enum List {\n    Empty,\n    Cons(int, Box[List]),\n}\nfn push[&h](heap: &!h Heap, rest: List, value: int) -> [heap] List {\n    return List::Cons(value, box(heap, rest));\n}\nfn drain[&h](heap: &!h Heap, list: List) -> [heap] int {\n    match list {\n        List::Empty => { return 0; }\n        List::Cons(value, rest) => {\n            let tail = unbox(heap, rest);\n            return value * 2 + drain(heap, tail);\n        }\n    }\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            var list = List::Empty;\n            list = push(h, list, 5);\n            list = push(h, list, 1);\n            list = push(h, list, 4);\n            code = drain(h, list);\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "enum List {\n    Empty,\n    Cons(int, Box[List]),\n}\nfn push[&h](heap: &!h Heap, rest: List, value: int) -> [heap] List {\n    return List::Cons(value, box(heap, rest));\n}\nfn drain[&h](heap: &!h Heap, list: List) -> [heap] int {\n    match list {\n        List::Empty => { return 0; }\n        List::Cons(value, rest) => {\n            let tail = unbox(heap, rest);\n            return value * 2 + drain(heap, tail);\n        }\n    }\n}\nfn length[&l](list: &l List) -> [] int {\n    match list {\n        List::Empty => { return 0; }\n        List::Cons(value, rest) => { return 1 + length(contents(rest)); }\n    }\n}\nfn total[&l](list: &l List) -> [] int {\n    match list {\n        List::Empty => { return 0; }\n        List::Cons(value, rest) => { return *value + total(contents(rest)); }\n    }\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            var list = List::Empty;\n            var i = 0;\n            while i < 10 { list = push(h, list, i); i = i + 1; }\n            borrow list as &q in { code = length(q) * 100 + total(q); }\n            let n = drain(h, list);\n            code = code + n % 3;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let inner = box(h, 9);\n            let outer = box(h, inner);\n            let i2 = unbox(h, outer);\n            code = unbox(h, i2);\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            let b = box(h, 5);\n            borrow b as &q in { code = *contents(q); }\n            let n = unbox(h, b);\n            code = code + n * 10;\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "struct Point {\n    x: int,\n    y: int,\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            region a {\n                var node = alloc[a](Point { x: 6, y: 7 });\n                node.x = node.x + 1;\n                code = node.x * 10 + node.y;\n            }\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "struct Point {\n    x: int,\n    y: int,\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            region a {\n                var i = 0;\n                var t = 0;\n                while i < 8 { let n = alloc[a](Point { x: i, y: i * i }); t = t + n.y; i = i + 1; }\n                code = t;\n            }\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "struct Point {\n    x: int,\n    y: int,\n}\nfn value_of[&r](p: &r Point) -> [] int { return p.x * 10 + p.y; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            region a {\n                let n = alloc[a](Point { x: 6, y: 7 });\n                let r = value_of(n);\n                code = r;\n            }\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "res struct Holder {\n    held: Box[int],\n}\nfn peek[&r](holder: &r Holder) -> [] int {\n    let box_ref = holder.held;\n    return *contents(box_ref);\n}\nfn bump[&r](holder: &!r Holder) -> [] int {\n    let box_ref = holder.held;\n    return *contents(box_ref) + 1;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var code = 0;\n    borrow mut heap as &!h in {\n        borrow mut io as &!r in {\n            var holder = Holder { held: box(h, 41) };\n            borrow holder as &q in { code = peek(q); }\n            borrow mut holder as &!w in { code = code + bump(w); }\n            let Holder { held } = holder;\n            code = code + unbox(h, held);\n        }\n    }\n    release(heap);\n    release(io);\n    return code;\n}",
    "fn hold[&h](heap: &!h Heap, n: int) -> [heap] (Box[int], int) { return (box(heap, n), 1); }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(args);\n    var value = 0;\n    var tag = 0;\n    borrow mut heap as &!h in {\n        let held = hold(h, 41);\n        let (boxed, marker) = held;\n        value = unbox(h, boxed);\n        tag = marker;\n    }\n    release(heap);\n    release(io);\n    return value + tag;\n}",
    "edition 7;\nfn roundtrip[&f](fs: &f Fs(\"/tmp\")) -> [fs_read(\"/tmp\"), fs_write(\"/tmp\")] int {\n    var status = fs_write(fs, \"/tmp/cancho_selfhost_t4.txt\", \"one shot\");\n    region a {\n        let buf = alloc_slice[a](16, byte_of(0));\n        let n = fs_read(fs, \"/tmp/cancho_selfhost_t4.txt\", buf);\n        status = status * 100 + n + int_of(buf[0]);\n    }\n    var missing = 0;\n    region a {\n        let buf = alloc_slice[a](4, byte_of(0));\n        missing = fs_read(fs, \"/tmp/cancho_selfhost_t4_missing\", buf);\n    }\n    return status + missing + 500;\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(exec); release(io);\n    let tmp = narrow(fs, \"/tmp\");\n    var status = 0;\n    borrow tmp as &f in { status = roundtrip(f); }\n    release(tmp);\n    return status % 256;\n}",
    "edition 7;\nfn probe[&a, &b](l: &a Fs(\"/tmp\"), r: &b Fs(\"/var\")) -> [fs_write(\"/tmp\"), fs_write(\"/var\")] int {\n    return fs_write(l, \"/tmp/cancho_selfhost_t5.txt\", \"x\") * 10 + fs_write(r, \"/var/tmp/cancho_selfhost_t5.txt\", \"yy\");\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);\n    release(ffi); release(heap); release(args); release(net); release(clock); release(signals); release(exec); release(io);\n    let (a, b) = narrow(fs, \"/tmp\", \"/var\");\n    var status = 0;\n    borrow a as &x in { borrow b as &y in { status = probe(x, y); } }\n    release(a);\n    release(b);\n    return status;\n}",
];

/// Arenas and `static` tables: `region` and `alloc_slice` of whole numbers, bytes and booleans -- indexed, passed, nested,
/// allocated in a loop, a count that is negative or too large traps.
const ARENAS: &[&str] = &[
    "fn run() -> [] int { var t = 0; region a { let s = alloc_slice[a](5, 7); t = len(s) * 100 + s[4]; } return t; }",
    "fn run() -> [] int { var t = 0; region a { var s = alloc_slice[a](10, 3); var i = 0; while i < len(s) { s[i] = s[i] + i; i = i + 1; } i = 0; while i < len(s) { t = t + s[i]; i = i + 1; } } return t; }",
    "fn run() -> [] int { var t = 0; region a { var s = alloc_slice[a](4, byte_of(65)); s[2] = byte_of(200); t = int_of(s[0]) + int_of(s[2]); } return t; }",
    "fn primes(n: int) -> [] int {\n    var count = 0;\n    region a {\n        var composite = alloc_slice[a](n + 1, false);\n        var i = 2;\n        while i <= n {\n            if !composite[i] {\n                count = count + 1;\n                var j = i * i;\n                while j <= n { composite[j] = true; j = j + i; }\n            }\n            i = i + 1;\n        }\n    }\n    return count;\n}\nfn run() -> [] int { return primes(100) + primes(2) * 1000; }",
    "fn run() -> [] int { var t = 0; region a { var s = alloc_slice[a](3, 1); region b { var u = alloc_slice[b](3, 10); u[1] = s[0] + 5; t = u[1] + u[2]; } t = t + s[2]; } return t; }",
    "fn run() -> [] int { var t = 0; region a { region b { var s = alloc_slice[a](2, 4); s[1] = 9; t = s[0] + s[1]; } } return t; }",
    "fn bump[&r](s: &!r [int]) -> [] int { var i = 0; while i < len(s) { s[i] = s[i] + 1; i = i + 1; } return len(s); }\nfn run() -> [] int { var t = 0; region a { var s = alloc_slice[a](6, 2); let n = bump(s); let m = bump(s); t = n + m + s[5]; } return t; }",
    "fn run() -> [] int { var i = 0; var t = 0; while i < 50 { region a { var s = alloc_slice[a](100, i); t = t + s[99]; } i = i + 1; } return t; }",
    "fn run() -> [] int { var t = 0; region a { let s = alloc_slice[a](0, 1); t = len(s); } return t; }",
    "fn run() -> [] int { var t = 0; region a { let s = alloc_slice[a](0, 1); t = s[0]; } return t; }",
    "fn run() -> [] int { var t = 0; region a { let s = alloc_slice[a](3, 1); t = s[3]; } return t; }",
    "fn run() -> [] int { var t = 0; region a { let s = alloc_slice[a](3, 1); t = s[0 - 1]; } return t; }",
    "fn mk(n: int) -> [] int { var t = 0; region a { let s = alloc_slice[a](n, 1); t = len(s); } return t; }\nfn run() -> [] int { return mk(0 - 1); }",
    "fn mk(n: int) -> [] int { var t = 0; region a { let s = alloc_slice[a](n, 1); t = len(s); } return t; }\nfn run() -> [] int { return mk(4611686018427387904); }",
    "fn mk(n: int) -> [] int { var t = 0; region a { let s = alloc_slice[a](n, byte_of(1)); t = len(s); } return t; }\nfn run() -> [] int { return mk(4611686018427387904) + 1; }",
    "fn find(n: int) -> [] int {\n    region a {\n        var s = alloc_slice[a](n, 0);\n        var i = 0;\n        while i < n { s[i] = i * i; i = i + 1; }\n        if n > 4 { return s[4]; }\n    }\n    return 0 - 1;\n}\nfn run() -> [] int { return find(10) * 100 + find(2) + 1; }",
    "fn run() -> [] int { var t = 0; region a { region b { let x = alloc_slice[a](2, 1); let y = alloc_slice[b](2, 2); t = x[0] + y[0]; } let z = alloc_slice[a](2, 5); t = t + z[1]; } return t; }",
    "fn fib(n: int) -> [] int {\n    var r = 0;\n    region a {\n        var t = alloc_slice[a](n + 2, 0);\n        t[1] = 1;\n        var i = 2;\n        while i <= n { t[i] = t[i - 1] + t[i - 2]; i = i + 1; }\n        r = t[n];\n    }\n    return r;\n}\nfn run() -> [] int { return fib(30) % 251; }",
    "fn fib(n: int) -> [] int {\n    var r = 0;\n    region a {\n        var t = alloc_slice[a](n + 2, 0);\n        t[1] = 1;\n        var i = 2;\n        while i <= n { t[i] = t[i - 1] + t[i - 2]; i = i + 1; }\n        r = t[n];\n    }\n    return r;\n}\nfn run() -> [] int { return fib(100); }",
    "static squares: [int] {\n    let t = alloc_slice[static](10, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return squares[3] + squares[9] + len(squares); }",
    "static squares: [int] {\n    let t = alloc_slice[static](10, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i; i = i + 1; }\n    return t;\n}\nfn at(i: int) -> [] int { return squares[i]; }\nfn run() -> [] int { return at(7) - at(2); }",
    "static squares: [int] {\n    let t = alloc_slice[static](10, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { var t = 0; var i = 0; while i < len(squares) { t = t + squares[i]; i = i + 1; } return t; }",
    "static squares: [int] {\n    let t = alloc_slice[static](10, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return squares[10]; }",
    "static squares: [int] {\n    let t = alloc_slice[static](10, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i; i = i + 1; }\n    return t;\n}\nfn at(i: int) -> [] int { return squares[i]; }\nfn run() -> [] int { return at(0 - 1); }",
    "static flags: [bool] {\n    let t = alloc_slice[static](4, false);\n    t[2] = true;\n    return t;\n}\nfn run() -> [] int { if flags[2] && !flags[1] { return 7; } return 0; }",
    "static word: [byte] {\n    return \"hey\";\n}\nfn run() -> [] int { return int_of(word[1]) + len(word); }",
    "static decode: [int] {\n    let table = alloc_slice[static](256, 0 - 1);\n    let alpha = \"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\";\n    var i = 0;\n    while i < len(alpha) {\n        table[int_of(alpha[i])] = i;\n        i = i + 1;\n    }\n    return table;\n}\nfn value_of(c: int) -> [] int { return decode[c]; }\nfn run() -> [] int { return value_of(65) + value_of(97) * 10 + value_of(57) * 100 + value_of(33) * 1000 + 5000; }",
    "static squares: [int] {\n    let t = alloc_slice[static](10, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i; i = i + 1; }\n    return t;\n}\nstatic cubes: [int] {\n    let t = alloc_slice[static](5, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i * i; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return squares[4] + cubes[4] + len(cubes); }",
    "static none: [int] {\n    let t = alloc_slice[static](0, 0);\n    return t;\n}\nfn run() -> [] int { return len(none) + 3; }",
    "static big: [int] {\n    let t = alloc_slice[static](100000, 1);\n    var i = 1;\n    while i < len(t) { t[i] = t[i - 1] + i % 7; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return big[99999] % 251; }",
    "static squares: [int] {\n    let t = alloc_slice[static](10, 0);\n    var i = 0;\n    while i < len(t) { t[i] = i * i; i = i + 1; }\n    return t;\n}\nfn sum[&r](s: &r [int]) -> [] int {\n    var t = 0;\n    var i = 0;\n    while i < len(s) { t = t + s[i]; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return sum(squares); }",
    "static primes: [int] {\n    let t = alloc_slice[static](8, 0);\n    t[0] = 2; t[1] = 3; t[2] = 5; t[3] = 7; t[4] = 11; t[5] = 13; t[6] = 17; t[7] = 19;\n    return t;\n}\nfn is_prime(n: int) -> [] bool {\n    var i = 0;\n    while i < len(primes) { if primes[i] == n { return true; } i = i + 1; }\n    return false;\n}\nfn run() -> [] int { var c = 0; var n = 0; while n < 20 { if is_prime(n) { c = c + 1; } n = n + 1; } return c; }",
    "static bytes: [byte] {\n    let t = alloc_slice[static](3, byte_of(0));\n    t[0] = byte_of(200);\n    t[2] = byte_of(7);\n    return t;\n}\nfn run() -> [] int { return int_of(bytes[0]) + int_of(bytes[1]) + int_of(bytes[2]); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[0..5]; return len(s) * 100 + int_of(s[4]); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[7..12]; return len(s) * 100 + int_of(s[0]); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[3..3]; return len(s) + 7; }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[0..12]; return len(s); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[2..9]; let t = s[1..4]; return len(t) * 100 + int_of(t[0]); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[0..13]; return len(s); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[5..4]; return len(s); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[0 - 1..3]; return len(s); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[3..0 - 1]; return len(s); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn run() -> [] int { let s = word[2..4]; return int_of(s[2]); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn first[&r](s: &r [byte], n: int) -> [] &r [byte] { return s[0..n]; }\nfn run() -> [] int { let a = first(word, 4); return len(a) * 100 + int_of(a[3]); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn first[&r](s: &r [byte], n: int) -> [] &r [byte] { return s[0..n]; }\nfn run() -> [] int { let a = first(word, 40); return len(a); }",
    "static word: [byte] {\n    return \"hello, world\";\n}\nfn first[&r](s: &r [byte], n: int) -> [] &r [byte] { return s[0..n]; }\nfn tail[&r](s: &r [byte], from: int) -> [] &r [byte] { return s[from..len(s)]; }\nfn run() -> [] int { let a = tail(first(word, 9), 4); return len(a) * 100 + int_of(a[0]); }",
    "fn name() -> [] &static [byte] { return \"abcdef\"; }\nfn run() -> [] int { let s = name(); return len(s) * 100 + int_of(s[5]); }",
    "fn name() -> [] &static [byte] { return \"abcdef\"; }\nfn run() -> [] int { let s = name()[1..4]; return len(s) * 100 + int_of(s[0]); }",
    "fn pick(k: int) -> [] &static [byte] { if k == 0 { return \"zero\"; } if k == 1 { return \"one\"; } return \"many\"; }\nfn run() -> [] int { return len(pick(0)) * 100 + len(pick(1)) * 10 + len(pick(5)); }",
    "static nums: [int] {\n    let t = alloc_slice[static](8, 0);\n    var i = 0;\n    while i < len(t) { t[i] = (i + 1) * 3; i = i + 1; }\n    return t;\n}\nfn pre[&r](s: &r [int], n: int) -> [] &r [int] { return s[0..n]; }\nfn sum[&r](s: &r [int]) -> [] int { var t = 0; var i = 0; while i < len(s) { t = t + s[i]; i = i + 1; } return t; }\nfn run() -> [] int { return sum(pre(nums, 3)) * 100 + sum(pre(nums, 0)) + sum(nums[5..8]); }",
    "static nums: [int] {\n    let t = alloc_slice[static](8, 0);\n    var i = 0;\n    while i < len(t) { t[i] = (i + 1) * 3; i = i + 1; }\n    return t;\n}\nfn total[&r](s: &r [int]) -> [] int { if len(s) == 0 { return 0; } if len(s) == 1 { return s[0]; } let h = len(s) / 2; return total(s[0..h]) + total(s[h..len(s)]); }\nfn run() -> [] int { return total(nums); }",
    "static nums: [int] {\n    let t = alloc_slice[static](8, 0);\n    var i = 0;\n    while i < len(t) { t[i] = (i + 1) * 3; i = i + 1; }\n    return t;\n}\nfn find[&r](s: &r [int], x: int) -> [] int { var lo = 0; var hi = len(s); while lo < hi { let mid = (lo + hi) / 2; if s[mid] < x { lo = mid + 1; } else { hi = mid; } } return lo; }\nfn run() -> [] int { return find(nums, 12) * 10 + find(nums, 100) + find(nums, 0) * 100; }",
    "fn run() -> [] int { var t = 0; region a { var s = alloc_slice[a](10, 1); let h = s[2..6]; var i = 0; while i < len(h) { h[i] = 9; i = i + 1; } i = 0; while i < len(s) { t = t + s[i]; i = i + 1; } } return t; }",
    "fn skip[&r](s: &r [byte]) -> [] &r [byte] { if len(s) == 0 { return s; } return s[1..len(s)]; }\nfn run() -> [] int { var s = \"abcdef\"; var n = 0; while len(s) > 0 { s = skip(s); n = n + 1; } return n; }",
    "struct In { a: int, ok: bool }\nstruct Out { first: In, n: int }\nfn run() -> [] int { let o = Out { first: In { a: 5, ok: true }, n: 50 }; return o.first.a + o.n; }",
    "struct In { a: int, ok: bool }\nstruct Out { first: In, n: int }\nfn mk(k: int) -> [] Out { return Out { first: In { a: k, ok: k > 2 }, n: k * 10 }; }\nfn run() -> [] int { let o = mk(5); let p = mk(1); var r = o.first.a + p.n; if o.first.ok && !p.first.ok { r = r + 100; } return r; }",
    "struct In { a: int, ok: bool }\nstruct Out { first: In, n: int }\nfn run() -> [] int { let o = Out { first: In { a: 5, ok: true }, n: 50 }; let Out { first, n } = o; let In { a, ok } = first; if ok { return a + n; } return 0; }",
    "struct In { a: int, ok: bool }\nstruct Out { first: In, n: int }\nfn swap(o: Out) -> [] Out { return Out { first: In { a: o.n, ok: o.first.ok }, n: o.first.a }; }\nfn run() -> [] int { let o = swap(Out { first: In { a: 1, ok: false }, n: 2 }); return o.first.a * 10 + o.n; }",
    "struct In { a: int, ok: bool }\nstruct Out { first: In, n: int }\nfn read[&r](o: &r Out) -> [] int { return o.first.a + o.n; }\nfn run() -> [] int { let o = Out { first: In { a: 7, ok: true }, n: 8 }; var t = 0; borrow o as &q in { t = read(q); } return t; }",
    "struct In { a: int, ok: bool }\nstruct Out { first: In, n: int }\nfn run() -> [] int { var o = Out { first: In { a: 1, ok: false }, n: 2 }; var t = 0; borrow mut o as &!q in { q.n = 9; q.first = In { a: 4, ok: true }; t = q.n + q.first.a; } return t + o.first.a; }",
    "struct In { a: int, ok: bool }\nstruct Out { first: In, n: int }\nfn run() -> [] int { var o = Out { first: In { a: 1, ok: false }, n: 2 }; o = Out { first: In { a: o.n, ok: true }, n: o.first.a }; return o.first.a * 10 + o.n; }",
    "enum Tok { A, B, C }\nenum Step { Skip(int), Token(Tok, int, int) }\nfn which(s: Step) -> [] int {\n    match s {\n        Step::Skip(n) => { return n; }\n        Step::Token(t, a, b) => {\n            match t {\n                Tok::A => { return a; }\n                Tok::B => { return b; }\n                Tok::C => { return a + b; }\n            }\n        }\n    }\n}\nfn run() -> [] int { return which(Step::Token(Tok::C, 3, 4)) * 100 + which(Step::Skip(2)) * 10 + which(Step::Token(Tok::A, 6, 7)); }",
    "enum Tok { A, B, C }\nenum Step { Skip(int), Token(Tok, int, int) }\nfn which(s: Step) -> [] int {\n    match s {\n        Step::Skip(n) => { return n; }\n        Step::Token(t, a, b) => {\n            match t {\n                Tok::A => { return a; }\n                Tok::B => { return b; }\n                Tok::C => { return a + b; }\n            }\n        }\n    }\n}\nfn run() -> [] int { return which(Step::Token(Tok::B, 6, 7)) + which(Step::Skip(0 - 9)); }",
    "enum Op { Add, Sub }\nstruct Instr { op: Op, a: int, b: int }\nfn exec(i: Instr) -> [] int { match i.op { Op::Add => { return i.a + i.b; } Op::Sub => { return i.a - i.b; } } }\nfn run() -> [] int { return exec(Instr { op: Op::Add, a: 5, b: 6 }) * 100 + exec(Instr { op: Op::Sub, a: 5, b: 6 }) + 200; }",
    "struct Pair { x: int, y: int }\nenum Shape { Dot(Pair), Seg(Pair, Pair) }\nfn len2(s: Shape) -> [] int { match s { Shape::Dot(p) => { return p.x + p.y; } Shape::Seg(a, b) => { return b.x - a.x + b.y - a.y; } } }\nfn run() -> [] int { return len2(Shape::Dot(Pair { x: 3, y: 4 })) * 10 + len2(Shape::Seg(Pair { x: 1, y: 1 }, Pair { x: 4, y: 6 })); }",
    "static crc: [int] {\n    let t = alloc_slice[static](256, 0);\n    var i = 0;\n    while i < 256 {\n        var c = i;\n        var k = 0;\n        while k < 8 {\n            if c & 1 == 1 { c = c >> 1 ^ 0xedb88320; } else { c = c >> 1; }\n            c = c & 0xffffffff;\n            k = k + 1;\n        }\n        t[i] = c;\n        i = i + 1;\n    }\n    return t;\n}\nfn run() -> [] int { var c = 0xffffffff; let s = \"123456789\"; var i = 0; while i < len(s) { c = c >> 8 ^ crc[(c ^ int_of(s[i])) & 0xff]; c = c & 0xffffffff; i = i + 1; } return (c ^ 0xffffffff) % 251; }",
    "static crc: [int] {\n    let t = alloc_slice[static](256, 0);\n    var i = 0;\n    while i < 256 {\n        var c = i;\n        var k = 0;\n        while k < 8 {\n            if c & 1 == 1 { c = c >> 1 ^ 0xedb88320; } else { c = c >> 1; }\n            c = c & 0xffffffff;\n            k = k + 1;\n        }\n        t[i] = c;\n        i = i + 1;\n    }\n    return t;\n}\nstatic after: [int] {\n    let t = alloc_slice[static](len(crc), 0);\n    var i = 0;\n    while i < len(t) { t[i] = crc[i] % 1000; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return (crc[255] % 251 + after[7] + after[100]) % 251; }",
    "fn twice(n: int) -> [] int { return n * 2; }\nstatic viacall: [int] {\n    let t = alloc_slice[static](5, 0);\n    var i = 0;\n    while i < 5 { t[i] = twice(i) + 1; i = i + 1; }\n    return t;\n}\nstatic after_call: [int] {\n    let t = alloc_slice[static](len(viacall), 0);\n    var i = 0;\n    while i < len(t) { t[i] = viacall[i] * 10; i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return viacall[4] + after_call[2]; }",
    "static classes: [int] {\n    let t = alloc_slice[static](128, 0);\n    var c = 0;\n    while c < 128 {\n        var v = 0;\n        let letter = c >= 65 && c <= 90 || c >= 97 && c <= 122;\n        let digit = c >= 48 && c <= 57;\n        if letter || digit { v = v + 1; }\n        if c == 9 || c >= 32 && c != 127 { v = v + 2; }\n        if c == 47 { v = v + 4; } else if c == 63 { v = v + 8; }\n        t[c] = v;\n        c = c + 1;\n    }\n    return t;\n}\nfn run() -> [] int { return classes[65] * 100 + classes[47] * 10 + classes[0] + classes[63]; }",
    "static upper: [byte] {\n    let out = alloc_slice[static](11, byte_of(0));\n    let source = \"hello world\";\n    var i = 0;\n    while i < len(source) {\n        var c = int_of(source[i]);\n        if c >= 97 && c <= 122 { c = c - 32; }\n        out[i] = byte_of(c);\n        i = i + 1;\n    }\n    return out;\n}\nfn run() -> [] int { return int_of(upper[0]) + int_of(upper[5]) + len(upper); }",
    "static primes: [bool] {\n    let t = alloc_slice[static](50, true);\n    t[0] = false;\n    t[1] = false;\n    var i = 2;\n    while i * i < 50 {\n        if t[i] {\n            var j = i * i;\n            while j < 50 { t[j] = false; j = j + i; }\n        }\n        i = i + 1;\n    }\n    return t;\n}\nfn run() -> [] int { var n = 0; var i = 0; while i < len(primes) { if primes[i] { n = n + 1; } i = i + 1; } return n; }",
    "static sizes: [int] {\n    let t = alloc_slice[static](6, 1);\n    var i = 1;\n    while i < len(t) { t[i] = t[i - 1] * 3 + (i % 2) - (i / 3); i = i + 1; }\n    return t;\n}\nfn run() -> [] int { return sizes[5] % 251 + sizes[0 - 0]; }",
    "static neg: [int] {\n    let t = alloc_slice[static](3, 0 - 7);\n    t[1] = -t[0] * 3;\n    t[2] = ~t[1] + (t[0] << 2) - (t[1] >> 1);\n    return t;\n}\nfn run() -> [] int { return (neg[0] + neg[1] + neg[2] + 1000) % 251; }",
    "static window: [int] {\n    let t = alloc_slice[static](10, 0);\n    let part = t[2..6];\n    part[0] = 5;\n    part[3] = 9;\n    return t;\n}\nfn run() -> [] int { return window[2] * 10 + window[5] + window[0]; }",
    "fn swap(pair: (int, bool)) -> [] (bool, int) { let (n, f) = pair; return (f, n); }\nfn run() -> [] int { let r = swap((7, true)); if r.0 { return r.1 * 10; } return 1; }",
    "fn three(a: int) -> [] (int, bool, int) { return (a, a > 3, a * 2); }\nfn run() -> [] int { let (x, ok, y) = three(5); if ok { return x + y; } return 0; }",
    "fn run() -> [] int { let t = (4, 5, 6); let (a, b, c) = t; return a * 10 + c + t.1; }",
    "fn sum[&p](pair: &p (int, int)) -> [] int { return pair.0 + pair.1; }\nfn run() -> [] int { let pair = (40, 2); var total = 0; borrow pair as &q in { total = sum(q); } return total; }",
    "fn first(p: ((int, int), int)) -> [] int { let (inner, k) = p; return inner.0 + inner.1 * 10 + k * 100; }\nfn run() -> [] int { return first(((1, 2), 3)) % 251; }",
    "struct Holder { t: (int, bool) }\nfn run() -> [] int { let h = Holder { t: (9, true) }; let (n, f) = h.t; if f { return n; } return 0; }",
    "enum Msg { Pair((int, int)), Unit }\nfn total(m: Msg) -> [] int { match m { Msg::Pair(p) => { return p.0 * p.1; } Msg::Unit => { return 0 - 1; } } }\nfn run() -> [] int { return total(Msg::Pair((6, 7))); }",
    "fn run() -> [] int { var t = (1, 2); var i = 0; while i < 5 { t = (t.1, t.0 + t.1); i = i + 1; } return t.0 * 100 + t.1; }",
    "fn minmax(a: int, b: int) -> [] (int, int) { if a < b { return (a, b); } return (b, a); }\nfn run() -> [] int { let (lo, hi) = minmax(9, 4); return hi * 10 + lo; }",
    "fn run() -> [] int { let big = 9223372036854775807; let t = (big, 1); return t.0 + t.1; }",
];

/// Decimal literals that are hard to round: halfway cases, the edges of the subnormals and of the largest
/// number, long digit strings, and underscores.
const DOUBLE_LITERALS: &[&str] = &[
    "0.1",
    "0.3",
    "1.0",
    "2.5e-3",
    "1e23",
    "8.5e-5",
    "5e-324",
    "1e-323",
    "2.4703282292062327e-324",
    "2.4703282292062328e-324",
    "2.2250738585072014e-308",
    "2.2250738585072011e-308",
    "1.7976931348623157e308",
    "123456789012345678901234567890e-20",
    "9007199254740993.0",
    "9007199254740992.5",
    "9007199254740993.000000000000000000001",
    "1_000.5",
    "0.000001",
    "4.35",
    "6.02214076e23",
    "299792458.0",
    "3.141592653589793238462643383279",
    "0.30000000000000004",
    "1e22",
    "1e-22",
    "7.0e-10",
    "123456789.123456789e-5",
    "1.5e300",
    "0.0",
    "100000000000000000000000.0",
    "5.551115123125783e-17",
];

/// The same for `f32`, which rounds once, from the decimal.
const SINGLE_LITERALS: &[&str] = &[
    "0.1f32",
    "0.3f32",
    "1.0f32",
    "16777217.0f32",
    "16777219.0f32",
    "1.17549435e-38f32",
    "1.4e-45f32",
    "7.0e-46f32",
    "7.1e-46f32",
    "3.4028235e38f32",
    "3.4028234e38f32",
    "1e-5f32",
    "123456.79f32",
    "1.0000000596046447753906251f32",
    "1_0.5f32",
    "0.0f32",
    "8.5e-5f32",
];

/// Tuples of four to six, made, passed, returned and taken apart.
const QUADS: &[&str] = &[
    r#"fn make(a: int, b: int) -> [] (int, int, int, bool) { return (a, b, a + b, a > b); }
fn total(q: (int, int, int, bool)) -> [] int {
    let (x, y, z, w) = q;
    if w { return x * 1000 + y * 100 + z; }
    return x + y * 2 + z * 3;
}
fn five(p: int) -> [] (int, int, int, int, int) { return (p + 4, 1, p + 2, 3, p + p); }
fn six(p: int, ok: bool) -> [] (int, bool, byte, int, int, int) { return (p, ok, byte_of(p), p * 2, p * 3, p * 4); }
fn run() -> [] int {
    let q = make(3, 2);
    let r = make(1, 5);
    let (a, b, c, d) = (7, 8, 9, 10);
    let f = five(10);
    let (f0, f1, f2, f3, f4) = f;
    let s = six(5, true);
    let (s0, s1, s2, s3, s4, s5) = s;
    var extra = 0;
    if s1 { extra = int_of(s2) + s5; }
    return total(q) + total(r) + q.2 + r.1 + a + b + c + d + f0 + f2 * 10 + f4 * 100 + f.3 + s3 + s4 + extra;
}"#,
];

/// Tuples of more than eight parts, made, passed, returned and taken apart.
const WIDE: &[&str] = &[
    r#"fn make() -> [] (int, bool, int, bool, int, bool, int, bool, int) { return (1, true, 3, true, 5, true, 7, true, 9); }
fn run() -> [] int {
    let t = make();
    let (x0, x1, x2, x3, x4, x5, x6, x7, x8) = t;
    var sum = 0;
    if x1 { sum = sum + 1; }
    if x7 { sum = sum + 2; }
    return x0 + x2 + x8 + t.4 + sum;
}"#,
    r#"fn make() -> [] (int, bool, int, bool, int, bool, int, bool, int, bool, int, bool) { return (1, true, 3, true, 5, true, 7, true, 9, true, 11, true); }
fn run() -> [] int {
    let t = make();
    let (x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11) = t;
    var sum = 0;
    if x1 { sum = sum + 1; }
    if x11 { sum = sum + 2; }
    return x0 + x2 + x10 + t.4 + sum;
}"#,
    r#"fn make() -> [] (int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool) { return (1, true, 3, true, 5, true, 7, true, 9, true, 11, true, 13, true, 15, true); }
fn run() -> [] int {
    let t = make();
    let (x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15) = t;
    var sum = 0;
    if x1 { sum = sum + 1; }
    if x15 { sum = sum + 2; }
    return x0 + x2 + x14 + t.4 + sum;
}"#,
    r#"fn make() -> [] (int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool, int, bool) { return (1, true, 3, true, 5, true, 7, true, 9, true, 11, true, 13, true, 15, true, 17, true, 19, true, 21, true, 23, true, 25, true, 27, true, 29, true, 31, true, 33, true, 35, true, 37, true, 39, true); }
fn run() -> [] int {
    let t = make();
    let (x0, x1, x2, x3, x4, x5, x6, x7, x8, x9, x10, x11, x12, x13, x14, x15, x16, x17, x18, x19, x20, x21, x22, x23, x24, x25, x26, x27, x28, x29, x30, x31, x32, x33, x34, x35, x36, x37, x38, x39) = t;
    var sum = 0;
    if x1 { sum = sum + 1; }
    if x39 { sum = sum + 2; }
    return x0 + x2 + x38 + t.4 + sum;
}"#,
    r#"fn pass(t: (int, int, int, int, int, int, int, int, int, int)) -> [] (int, int, int, int, int, int, int, int, int, int) {
    return (t.9, t.8, t.7, t.6, t.5, t.4, t.3, t.2, t.1, t.0);
}
fn run() -> [] int {
    let r = pass((1, 2, 3, 4, 5, 6, 7, 8, 9, 10));
    let (a, b, c, d, e, f, g, h, i, j) = r;
    return a * 1 + b * 2 + j * 3 + e;
}"#,
];

/// Tuples of more than three parts that hold references to slices, in every place, made, returned and taken apart.
const SLICE_TUPLES: &[&str] = &[
    r#"fn kind(n: int) -> [] (bool, &static [byte], &static [byte], &static [byte]) {
    if n == 1 { return (true, "read-only", "", ""); }
    return (false, "", "x", "yz");
}
fn run() -> [] int {
    let (a, b, c, d) = kind(1);
    let (a2, b2, c2, d2) = kind(2);
    var r = 0;
    if a { r = r + 1; }
    if !a2 { r = r + 2; }
    return r * 100 + len(b) * 10 + len(c) + len(d) + len(b2) + len(c2) * 5 + len(d2) * 7;
}"#,
    r#"fn pick(n: int, s: &static [byte]) -> [] (int, &static [byte], bool, &static [byte], &static [byte]) {
    if n == 1 { return (n, s, true, "xy", "abc"); }
    return (n, "", false, s, "");
}
fn five() -> [] (bool, &static [byte], &static [byte], &static [byte], int) { return (true, "one", "", "three", 5); }
fn run() -> [] int {
    let (a, b, c, d, e) = pick(1, "hello");
    let (a2, b2, c2, d2, e2) = pick(2, "hello");
    let (p, q, s, t, u) = five();
    var r = 0;
    if c { r = r + 1; }
    if p { r = r + 2; }
    return r + a * 100 + a2 * 1000 + len(b) + len(d) * 3 + len(e) * 5 + len(b2) + len(d2) * 7 + len(e2) + len(q) * 11 + len(s) + len(t) * 13 + u;
}"#,
    r#"fn mixed(k: int) -> [] (&static [byte], int, &static [byte], bool, &static [byte], &static [byte]) {
    return ("a", k, "bb", k > 1, "ccc", "dddd");
}
fn run() -> [] int {
    let m = mixed(3);
    let (a, k, b, flag, c, d) = m;
    var r = len(a) + k * 10 + len(b) * 100 + len(c) * 1000 + len(d) * 10000;
    if flag { r = r + 100000; }
    return r + len(m.5) + m.1;
}"#,
];

/// A function that lends a reference to a `res` part of a struct, shared or unique, and the references taken from it.
const LENT: &[&str] = &[
    r#"res struct Inner { a: int, b: int }
res struct Outer { inner: Inner, n: int }
fn inner_of[&m](m: &m Outer) -> [] &m Inner { return m.inner; }
fn sum[&m](i: &m Inner) -> [] int { return i.a * 10 + i.b; }
fn run() -> [] int {
    var o = Outer { inner: Inner { a: 3, b: 4 }, n: 5 };
    var r = 0;
    borrow o as &q in {
        let i = inner_of(q);
        r = sum(i) + i.a;
    }
    let Outer { inner, n } = o;
    let Inner { a, b } = inner;
    return r + n + a + b;
}"#,
    r#"res struct Inner { a: int, b: int }
res struct Outer { inner: Inner, n: int }
fn inner_of[&m](m: &!m Outer) -> [] &!m Inner { return m.inner; }
fn run() -> [] int {
    var o = Outer { inner: Inner { a: 7, b: 8 }, n: 1 };
    borrow mut o as &!q in {
        let i = inner_of(q);
        i.a = i.a + 100;
    }
    let Outer { inner, n } = o;
    let Inner { a, b } = inner;
    return a + b + n;
}"#,
];

/// A variable whose type is what a later statement makes it: `var x = List::Empty;`.
const LATE: &[&str] = &[
    r#"enum List[T] { Empty, Cons(T, int) }
fn length(l: List[int]) -> [] int {
    match l {
        List::Empty => { return 0; }
        List::Cons(v, n) => { return v + n; }
    }
}
fn run() -> [] int {
    var x = List::Empty;
    var total = length(x);
    x = List::Cons(5, 7);
    return total + length(x);
}"#,
    r#"enum Maybe[T] { Nothing, Just(T) }
fn unwrap(m: Maybe[int], fallback: int) -> [] int {
    match m {
        Maybe::Nothing => { return fallback; }
        Maybe::Just(v) => { return v; }
    }
}
fn first_even(limit: int) -> [] int {
    var found = Maybe::Nothing;
    var i = 1;
    while i < limit {
        if i % 2 == 0 && unwrap(found, 0 - 1) < 0 {
            found = Maybe::Just(i * 10);
        }
        i = i + 1;
    }
    return unwrap(found, 0 - 1);
}
fn run() -> [] int { return first_even(9) + first_even(2) * 1000; }"#,
    r#"enum Maybe[T] { Nothing, Just(T) }
fn unwrap(m: Maybe[int], fallback: int) -> [] int {
    match m {
        Maybe::Nothing => { return fallback; }
        Maybe::Just(v) => { return v; }
    }
}
fn run() -> [] int {
    var slot = Maybe::Just(41);
    let before = unwrap(slot, 0);
    slot = Maybe::Nothing;
    let after = unwrap(slot, 7);
    slot = Maybe::Just(before + after);
    return unwrap(slot, 0);
}"#,
];

/// Functions held by their type in a parameter or a name, and called through it.
const FNPTRS: &[&str] = &[r#"struct P { a: int, b: int }
fn double(x: int) -> [] int { return x + x; }
fn add(x: int, y: int) -> [] int { return x + y; }
fn gt(x: int, y: int) -> [] bool { return x > y; }
fn mk(x: int, y: int) -> [] P { return P { a: x, b: y }; }
fn apply(f: fn(int) -> [] int, x: int) -> [] int { return f(x); }
fn twice(f: fn(int) -> [] int, x: int) -> [] int { return apply(f, apply(f, x)); }
fn fold2(f: fn(int, int) -> [] int, a: int, b: int) -> [] int { let g = f; return g(a, b) + f(b, a); }
fn pick(f: fn(int, int) -> [] bool, a: int, b: int) -> [] int { if f(a, b) { return 1; } return 0; }
fn build(f: fn(int, int) -> [] P, a: int) -> [] int { let p = f(a, 2 * a); return p.a * 10 + p.b; }
fn run() -> [] int {
    return twice(double, 3) + fold2(add, 4, 5) * 100 + pick(gt, 5, 2) * 10000 + build(mk, 7) * 100000;
}"#];

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
    for program in WITH_MAIN
        .iter()
        .chain(OUTPUT)
        .chain(STRINGS)
        .chain(OWN_MAIN)
        .chain(THREADS)
        .chain(FOREIGN)
        .chain(SOCKETS)
        .chain(PIPES)
    {
        jobs.push(((*program).to_owned(), (*program).to_owned()));
    }
    for program in STRUCTS
        .iter()
        .chain(ENUMS)
        .chain(DEFERS)
        .chain(WORDS)
        .chain(RUNS)
        .chain(CRYPTO)
        .chain(HELD)
        .chain(GENERICS)
        .chain(FLOATS)
        .chain(ARENAS)
        .chain(FNPTRS)
        .chain(LATE)
        .chain(QUADS)
        .chain(WIDE)
        .chain(SLICE_TUPLES)
        .chain(LENT)
    {
        jobs.push((with_main(program), with_main(program)));
    }
    // A program that makes, measures, renames and removes a file through a directory handle, in `/tmp`; the two
    // builds use names of their own, for they run at the same time.
    let probe = include_str!("dirs_probe.cho");
    jobs.push((probe.replace("NAME", "rust"), probe.replace("NAME", "cancho")));
    // The operations on an open file (a read and a write at an offset, the size, the truncation, the sync, the lock) and
    // on a path (rename, remove), in `/tmp`.
    let files = include_str!("files_probe.cho");
    jobs.push((files.replace("NAME", "rust"), files.replace("NAME", "cancho")));
    // Programs of `tests/accept` that import nothing: what a block defers, run at each of its exits; a struct that
    // holds capabilities; capabilities forked.
    for name in [
        "defer",
        "spawn_heap_in_struct",
        "narrow_many_in_struct",
        "fork_heap_workers",
        "fork_clock_workers",
        "fork_clock_row",
        "spawn_row",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/accept")
            .join(format!("{name}.cho"));
        let program = std::fs::read_to_string(&path).expect("a program of tests/accept");
        jobs.push((program.clone(), program));
    }
    // The poller and a handed-over peer, over a connection made inside the process (each build has a port of its
    // own, for they run at the same time, and below the range the kernel hands out to outgoing connections,
    // which another test's client could be holding); standard output flushed and standard input read.
    let poller = include_str!("poller_probe.cho");
    jobs.push((poller.replace("PORT", "29133"), poller.replace("PORT", "29134")));
    let stdio = include_str!("stdio_probe.cho");
    jobs.push((stdio.to_owned(), stdio.to_owned()));
    for text in DOUBLE_LITERALS {
        let bits = text.replace('_', "").parse::<f64>().expect("a double").to_bits() as i64;
        let program = format!(
            "fn run() -> [] int {{ if bits_of({text}) == {bits} {{ return 0; }} return 1; }}"
        );
        jobs.push((with_main(&program), with_main(&program)));
        let negative = format!(
            "fn run() -> [] int {{ if bits_of(-{text}) == {} {{ return 0; }} return 1; }}",
            (-text.replace('_', "").parse::<f64>().expect("a double")).to_bits() as i64
        );
        jobs.push((with_main(&negative), with_main(&negative)));
    }
    for text in SINGLE_LITERALS {
        let bits = text
            .trim_end_matches("f32")
            .replace('_', "")
            .parse::<f32>()
            .expect("a single")
            .to_bits();
        let program = format!(
            "edition 6;\nfn run() -> [] int {{ if bits_of32({text}) == {bits} {{ return 0; }} return 1; }}"
        );
        jobs.push((with_main(&program), with_main(&program)));
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
        if matches!(theirs, Ended::Trapped(_)) {
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

/// A function the compiler in cancho has no code for stops a build only when the program reaches it, and then
/// the answer says where the compiler stopped, not that a symbol is missing at link time.
#[test]
fn a_function_the_compiler_cannot_write_is_a_refusal_when_the_program_reaches_it() {
    let compiler = build("compile-refusal", &with_front_end("compile.cho"));
    let target = super::target_line();
    let apply = "fn apply(f: fn(int, int, int, int) -> [] int, x: int) -> [] int { return f(x, x, x, x); }\n\
                 fn sum(a: int, b: int, c: int, d: int) -> [] int { return a + b + c + d; }\n";
    let reached = format!(
        "{apply}fn main(world: World) -> [] int {{ release(world); return apply(sum, 1); }}\n"
    );
    let answer_reached = answer(&compiler, &format!("TARGET {target}\n{reached}"));
    let call = reached.find("apply(sum, 1)").expect("the call");
    assert_eq!(
        answer_reached.trim_end(),
        format!("ERR SKIP {call} {}", call + "apply(sum, 1)".len()),
        "a call to a function left out is refused at the call"
    );
    let unreached =
        format!("{apply}fn main(world: World) -> [] int {{ release(world); return 0; }}\n");
    let answer_unreached = answer(&compiler, &format!("TARGET {target}\n{unreached}"));
    assert!(
        !answer_unreached.starts_with("ERR"),
        "a function nobody calls is left out without a word:\n{answer_unreached}"
    );
    let _ = std::fs::remove_dir_all(compiler.parent().expect("a scratch directory"));
}

/// Builds that start together with nothing cached (`cargo test` runs many at once, and so do people with two
/// terminals) each build the compiler in cancho, and none of them may see another's half-written files.
#[test]
fn builds_that_start_together_with_an_empty_cache_all_succeed() {
    let dir = scratch("compile-race");
    let program = dir.join("program.cho");
    std::fs::write(
        &program,
        "fn main(world: World) -> [] int {\n    release(world);\n    return 5;\n}\n",
    )
    .expect("the scratch directory is writable");
    let cache = dir.join("cache");
    let children: Vec<_> = (0..8)
        .map(|i| {
            Command::new(BIN)
                .arg("build")
                .arg(&program)
                .args(["--compiler", "auto", "-o"])
                .arg(dir.join(format!("out{i}")))
                .env("CANCHO_CACHE", &cache)
                .stderr(Stdio::piped())
                .spawn()
                .expect("cancho runs")
        })
        .collect();
    for (i, child) in children.into_iter().enumerate() {
        let ended = child.wait_with_output().expect("cancho ends");
        assert!(ended.status.success(), "build {i}: {}", String::from_utf8_lossy(&ended.stderr));
        assert_eq!(run_it(&dir.join(format!("out{i}"))), Ended::Status(5, String::new()));
    }
    let kept: Vec<_> = std::fs::read_dir(&cache)
        .expect("the cache was made")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert_eq!(kept.len(), 1, "one directory in the cache: {kept:?}");
    let left: Vec<_> = std::fs::read_dir(cache.join(&kept[0]))
        .expect("the compiler's directory")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert_eq!(left, vec![std::ffi::OsString::from("compiler")], "no scratch left behind");
}

/// `cancho build --compiler <exe>` builds with the compiler in cancho: the program, with the standard
/// library it imports, runs as the Rust build does, and a refusal is an exit status 1 that says where.
#[test]
fn build_with_the_compiler_in_cancho_from_the_command_line() {
    let compiler = build("compile-cli", &with_front_end("compiler.cho"));
    let dir = compiler.parent().expect("a scratch directory").to_path_buf();
    let program = dir.join("program.cho");
    std::fs::write(
        &program,
        "import std.io;\nfn say[&i](con: &!i Io) -> [io_write] int {\n    return io.write_all(con, \"from the compiler in cancho\\n\");\n}\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    borrow mut io as &!i in {\n        say(i);\n    }\n    release(io);\n    return 3;\n}\n",
    )
    .expect("the scratch directory is writable");
    let built = |extra: &[&str], out: &str| {
        let exe = dir.join(out);
        let ended = Command::new(BIN)
            .arg("build")
            .arg(&program)
            .args(["--std", "-o"])
            .arg(&exe)
            .args(extra)
            .output()
            .expect("cancho runs");
        (ended, exe)
    };
    let (rust, rust_exe) = built(&[], "by-rust");
    assert!(rust.status.success(), "{}", String::from_utf8_lossy(&rust.stderr));
    let (ours, our_exe) = built(&["--compiler", compiler.to_str().expect("a path")], "by-cancho");
    assert!(ours.status.success(), "{}", String::from_utf8_lossy(&ours.stderr));
    assert_eq!(run_it(&rust_exe), run_it(&our_exe));
    // `--compiler auto` builds the compiler from the repository once, into a cache, and uses it.
    let cache = dir.join("cache");
    let auto = dir.join("by-auto");
    let ended = Command::new(BIN)
        .arg("build")
        .arg(&program)
        .args(["--std", "--compiler", "auto", "-o"])
        .arg(&auto)
        .env("CANCHO_CACHE", &cache)
        .output()
        .expect("cancho runs");
    assert!(ended.status.success(), "{}", String::from_utf8_lossy(&ended.stderr));
    assert_eq!(run_it(&rust_exe), run_it(&auto));
    assert!(
        std::fs::read_dir(&cache).expect("the cache was made").count() == 1,
        "one compiler was cached"
    );
    let refused = dir.join("refused.cho");
    std::fs::write(
        &refused,
        "fn main(world: World) -> [] int {\n    release(world);\n    return true;\n}\n",
    )
    .expect("the scratch directory is writable");
    let ended = Command::new(BIN)
        .arg("build")
        .arg(&refused)
        .args([
            "-o".as_ref(),
            dir.join("refused").as_os_str(),
            "--compiler".as_ref(),
            compiler.as_os_str(),
        ])
        .output()
        .expect("cancho runs");
    assert_eq!(ended.status.code(), Some(1));
    let said = String::from_utf8_lossy(&ended.stderr);
    assert!(
        said.contains("refused.cho:3:")
            && said.contains("error: ")
            && said.contains(cancho_syntax::Rule::TypeMismatch.explanation())
            && said.contains("(rule `type-mismatch`)")
            && said.contains("^^^^"),
        "where, why (the sentence of the rule, from the compiler in cancho), the rule, the line and what of it:\n{said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The rule `program` must be refused by: the one it declares itself (`//~ RULE <tag>`, as the Rust conformance suite reads
/// it), or else the one `cancho check --output json` names.
fn rust_rule(program: &Path) -> Option<String> {
    let text = std::fs::read_to_string(program).expect("a fixture");
    if let Some(declared) = text.lines().find_map(|l| l.strip_prefix("//~ RULE ")) {
        return Some(declared.trim().to_owned());
    }
    let ended = Command::new(BIN)
        .args(["check", "--output", "json"])
        .arg(program)
        .output()
        .expect("cancho runs");
    let json: serde_json::Value = serde_json::from_slice(&ended.stdout).ok()?;
    json["refused"][0]["rule"].as_str().map(str::to_owned)
}

/// The rule the compiler in cancho refuses `program` by (the message names it after the Rust front end's sentence),
/// `"accepted"` if it builds it, or what it said if it stopped otherwise.
fn cancho_rule(compiler: &Path, program: &Path, out: &Path) -> String {
    let ended = Command::new(BIN)
        .arg("build")
        .arg(program)
        .arg("-o")
        .arg(out)
        .arg("--compiler")
        .arg(compiler)
        .output()
        .expect("cancho runs");
    let said = String::from_utf8_lossy(&ended.stderr);
    if let Some(at) = said.find("(rule `") {
        let rest = &said[at + "(rule `".len()..];
        return rest[..rest.find('`').unwrap_or(rest.len())].to_owned();
    }
    if ended.status.success() {
        return "accepted".to_owned();
    }
    format!("stopped: {}", said.lines().next().unwrap_or(""))
}

/// Every program the language refuses (`tests/reject/`) is refused by the compiler in cancho with the same rule as by
/// the Rust front end. Not yet: `reject_gaps.txt` lists the ones that are not (#512), each of which must still differ,
/// so that the list shrinks and the programs that agree go on agreeing.
#[test]
fn refusals_agree_with_the_rust_front_end() {
    let compiler = build("compile-refusals", &with_front_end("compiler.cho"));
    let dir = compiler.parent().expect("a scratch directory").to_path_buf();
    let gaps: std::collections::BTreeSet<&str> = include_str!("reject_gaps.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    let mut fixtures: Vec<PathBuf> =
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/reject"))
            .expect("tests/reject")
            .map(|e| e.expect("an entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "cho"))
            .collect();
    // A fixture whose refusal is a property of a target (`//~ TARGET`) is the Rust compiler's: the compiler in cancho writes
    // code for the hosts only (WebAssembly is #486).
    fixtures.retain(|p| {
        !std::fs::read_to_string(p)
            .expect("a fixture")
            .lines()
            .any(|l| l.starts_with("//~ TARGET "))
    });
    fixtures.sort();
    let wrong = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for (lane, chunk) in fixtures.chunks(fixtures.len().div_ceil(8)).enumerate() {
            let (compiler, dir, gaps, wrong) = (&compiler, &dir, &gaps, &wrong);
            scope.spawn(move || {
                for (i, program) in chunk.iter().enumerate() {
                    let name = program.file_name().and_then(|n| n.to_str()).expect("a name");
                    let theirs = rust_rule(program);
                    let ours = cancho_rule(compiler, program, &dir.join(format!("r{lane}-{i}")));
                    let agree = theirs.as_deref() == Some(ours.as_str());
                    let listed = gaps.contains(name);
                    if agree == listed {
                        let what = if listed {
                            format!("{name}: now refused by the right rule `{ours}`; take it out of reject_gaps.txt")
                        } else {
                            format!("{name}: the Rust front end says {theirs:?}, the compiler in cancho {ours:?}")
                        };
                        wrong.lock().expect("a lock").push(what);
                    }
                }
            });
        }
    });
    let wrong = wrong.into_inner().expect("a lock");
    assert!(wrong.is_empty(), "{} programs:\n{}", wrong.len(), wrong.join("\n"));
}

/// The default build, named by nothing: the compiler written in cancho writes the code. The Rust front end checks first while
/// `reject_gaps.txt` lists a program the compiler in cancho does not refuse as the Rust one does (#512), so the default refuses
/// at least what the Rust compiler does and in its words; when the list is empty the check goes (`RUST_CHECKS_FIRST`) and a
/// refusal is the compiler's own, in the sentence of its rule.
#[test]
fn the_default_build_checks_with_rust_only_while_a_gap_is_listed() {
    let gaps = include_str!("reject_gaps.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .count();
    let dir = scratch("default-refusal");
    let program = dir.join("refused.cho");
    std::fs::write(
        &program,
        "fn main(world: World) -> [] int {\n    release(world);\n    return true;\n}\n",
    )
    .expect("the scratch directory is writable");
    let ended = Command::new(BIN)
        .arg("build")
        .arg(&program)
        .arg("-o")
        .arg(dir.join("refused"))
        .env_remove("CANCHO_COMPILER")
        .env("CANCHO_CACHE", dir.join("cache"))
        .output()
        .expect("cancho runs");
    assert_eq!(ended.status.code(), Some(1));
    let said = String::from_utf8_lossy(&ended.stderr);
    if gaps > 0 {
        assert!(
            said.contains("expected `int`, found `bool`"),
            "while a gap is listed the Rust front end checks:\n{said}"
        );
    } else {
        assert!(
            said.contains("(rule `type-mismatch`)")
                && !said.contains("expected `int`, found `bool`"),
            "with no gap listed the refusal is the compiler in cancho's own; set `RUST_CHECKS_FIRST` to false:\n{said}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// What the compiler in cancho has no code for is built by the Rust compiler when nothing was named, and is a refusal that says
/// so when the compiler in cancho was asked for.
#[test]
fn a_default_build_falls_back_to_the_rust_compiler_for_what_the_other_has_no_code_for() {
    let program = repo_root().join("tests/accept/aliasing_in_a_slice_of_references.cho");
    let dir = scratch("default-fallback");
    let built = |named: Option<&str>| {
        let mut command = Command::new(BIN);
        command
            .arg("build")
            .arg(&program)
            .args(["--std", "-o"])
            .arg(dir.join("out"))
            .env("CANCHO_CACHE", dir.join("cache"))
            .env_remove("CANCHO_COMPILER");
        if let Some(name) = named {
            command.env("CANCHO_COMPILER", name);
        }
        command.output().expect("cancho runs")
    };
    let default = built(None);
    assert!(default.status.success(), "{}", String::from_utf8_lossy(&default.stderr));
    let asked = built(Some("auto"));
    assert_eq!(asked.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&asked.stderr).contains("has no code for this construct yet"),
        "{}",
        String::from_utf8_lossy(&asked.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
