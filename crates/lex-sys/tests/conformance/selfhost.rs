//! `examples/selfhost/` (`docs/self-hosting.md`): the lex-sys lexer and parser, written
//! in lex-sys, against the Rust ones they were ported from.
//!
//! Each port reads a source file on standard input and prints what the Rust front end
//! made of it: the lexer one `Kind start end` line per token, the parser one line per
//! syntax node in postfix order, and either of them, for a file the Rust front end
//! refuses, the single line `ERR rule start end`. The oracles are the two examples of
//! `lex-sys-syntax` (`dump_tokens.rs`, `dump_ast.rs`), included here so that the test and
//! the command-line tools `diff.sh` and `fuzz.py` use cannot disagree about the format.
//!
//! The corpus is every program in the repository, the ones that must be refused included
//! (a refusal is a line to match too), and the edge cases below: what the repository does
//! not contain. `examples/selfhost/fuzz.py` adds mutants, which is where most refusals
//! come from; it is not run here because it takes a minute, and the corpus already holds
//! the shapes it found.

use super::*;

#[allow(dead_code)]
#[path = "../../../lex-sys-syntax/examples/dump_ast.rs"]
mod ast_oracle;
#[allow(dead_code)]
#[path = "../../../lex-sys-ir/examples/check_declarations.rs"]
mod declarations_oracle;
#[allow(dead_code)]
#[path = "../../../lex-sys-syntax/examples/dump_tokens.rs"]
mod token_oracle;

/// Programs that are not in the repository: the limits of integers and floats, escapes, and
/// the order of `module`, `import` and items.
const EDGE: &[&str] = &[
    "",
    "// nothing\n",
    "fn f() -> [] int { return 9223372036854775807 + 0x7fff_ffff_ffff_ffff + 0_0_7; }",
    "fn f() -> [] int { return -9223372036854775808 - -0x8000000000000000; }",
    "fn f() -> [] int { return 9223372036854775808; }",
    "fn f() -> [] int { return 0x8000000000000000; }",
    "fn f() -> [] int { return -9223372036854775809; }",
    "fn f() -> [] int { return 99999999999999999999999999999999; }",
    "fn f() -> [] int { return 'a' + '\\n' + '\\'' + -'x'; }",
    "fn f() -> [] int { return t.0.1.4294967295; }",
    "fn f() -> [] int { return t.4294967296; }",
    "fn f() -> [] float { return 1.7976931348623157e308; }",
    "fn f() -> [] float { return 1.7976931348623159e308; }",
    "fn f() -> [] float { return 1.797693134862315807937289714053034150799341327100378269361737789804449682927647509466490179775872070963302871479068578727958880e308; }",
    "fn f() -> [] float { return 1.7976931348623158079372897140530341507993413271003782693617377898044496829276475094664901797758720709633028714790685787279588e308; }",
    "fn f() -> [] float { return 1e309 + 0e99999999999999999999 + 1e-400; }",
    "fn f() -> [] float { return 1e99999999999999999999999; }",
    "fn f() -> [] f32 { return 3.4028234663852886e38f32; }",
    "fn f() -> [] f32 { return 3.4028235677973366e38f32; }",
    "fn f() -> [] f32 { return 3.4028235677973365e38f32 + 1e39f32; }",
    "fn f() -> [] f32 { return -1e39f32; }",
    "fn f() -> [] f32 { return 2f32; }",
    "fn f() -> [] int { g(\"a\\nb\\tc\\rd\\0e\\\\f\\\"g\", \"\", \"é日\"); }",
    "fn f() -> [] int { g(\"a\\qb\"); }",
    "fn f() -> [io_write, ffi(\"libc\"), fs_read(\"x\\n\")] int { return 0; }",
    "edition 7;\nfn f() -> [] int { return 1; }",
    "edition 8;\nfn f() -> [] int { return 1; }",
    "edition 99999999999999999999;",
    "edition );\nfn f() -> [] int { return 1; }",
    "module a.b.c;\nimport std.io;\nimport std.buffer as b;\nfn f() -> [] int { return 1; }",
    "module a;\nmodule b;",
    "fn f() -> [] int { return 1; }\nmodule a;",
    "module m.n;\nimport a;\nfn f() -> [] int { return 1; }\nimport b.c;\nimport e.f.g as h;",
    "pub static t: [int] { return 1; }\nstatic u: int { return 2; }",
    "pub res struct S[T] { a: T, }\nval enum E[T: val] { A(T), B, }\nres enum F { X }",
    "struct S[T: res] { a: T }",
    "val struct S[T: val] { a: T }",
    "struct S[&r] { a: int }",
    "extern fn g[T](x: T) -> [] int;",
    "extern fn g[&r](x: &r [byte], n: int) -> [ffi(\"libc\")] int;",
    "fn f[T, &r, U: val, &q where r <= q, q <= s](x: T) -> [] int { return 1; }",
    "fn f(a: fn(int, &r [byte]) -> [io_write] (int, bool), b: Ffi(\"libc\"), c: io.Buffer[int, (int, int)]) -> [] int { return 1; }",
    "fn f() -> [] int { if x == P { a: 1 } { return 1; } return 0; }",
    "fn f() -> [] int { if x == (P { a: 1 }) { return 1; } while a[P { a: 1 }.a] < 1 { } return 0; }",
    "fn f() -> [] int { return m.x; }",
    "fn f() -> [] int { return m.g(1) + m.E::V(2) + m.E::W + E::C(); }",
    "fn f() -> [] int { let (a, (b, c)) = t; }",
    "fn f() -> [] int { var P { a } = t; }",
    "fn f() -> [] int { borrow mut x as &r in { } }",
    "fn f() -> [] int { match x { E::A => { } E::B(a, _) => { return 1; }, _ => { } } return 0; }",
    "fn f() -> [] int { match x { 1 => { } } }",
    "fn f() -> [] int { while true {",
    "fn f() -> [] int { return a || b && c == d != e < f | g ^ h & i << j + k * -l; }",
    "fn f() -> [] int { return 1 $ 2; }",
    "fn f() -> [] int { return g(1, 2",
];

/// Programs for the declarations half of the checker (`examples/selfhost/check.ls`): each
/// breaks, or keeps, one rule of `collect_declarations`, in the order that function checks them.
const CHECK_EDGE: &[&str] = &[
    "fn main(world: World) -> [] int {  return 0; }",
    "struct A { a: int }\nstruct A { b: int }\nfn main(world: World) -> [] int {  return 0; }",
    "struct A { a: int }\nenum A { X }\nfn main(world: World) -> [] int {  return 0; }",
    "module m;\nstruct A { a: int }\nstruct A { b: int }",
    "struct int { a: int }",
    "enum bool { A }",
    "struct World { a: int }",
    "edition 1;\nstruct Conn { a: int }\nfn main(world: World) -> [] int {  return 0; }",
    "edition 5;\nstruct Conn { a: int }\nfn main(world: World) -> [] int {  return 0; }",
    "edition 6;\nstruct f32 { a: int }",
    "edition 5;\nstruct f32 { a: int }",
    "struct S[int] { a: int }",
    "struct S[T, T] { a: T }",
    "struct S[T, U] { a: T, b: U }\nfn main(world: World) -> [] int {  return 0; }",
    "struct S { a: int, a: bool }",
    "struct S { a: Nope }",
    "struct S { a: Box }",
    "struct P[T] { a: T }\nstruct S { a: P[int, int] }",
    "struct S { a: int[int] }",
    "struct S[T] { a: T[int] }",
    "struct S { a: [int] }",
    "struct S { a: &r int }",
    "struct S { a: &static [byte] }",
    "struct S { a: &!static [byte] }",
    "struct S { a: (int) }",
    "struct S { a: () }",
    "struct S { a: (int,) }",
    "struct S { a: fn(int) -> [] int }",
    "struct S { a: fn(Nope) -> [] int }",
    "struct S { a: x.T }",
    "module m;\nimport m as x;\nstruct T { a: int }\nstruct S { a: x.T }",
    "module m;\nimport m as x;\nstruct T { a: int }\nstruct S { a: x.T }",
    "struct S { a: Box[[int]] }",
    "struct P[T] { a: T }\nstruct S { a: P[[int]] }",
    "enum E { }",
    "enum E { A, A }",
    "enum E { A(Nope) }",
    "enum E { A(Box, int) }",
    "struct S { a: S }",
    "enum E { A(E) }",
    "struct A { b: B }\nstruct B { a: A }",
    "struct S { a: Box[S] }\nfn main(world: World) -> [] int {  return 0; }",
    "struct S { a: (int, S) }",
    "struct W[T] { a: T }\nstruct S { a: W[S] }",
    "struct S[&r] { a: int }",
    "val struct S { a: int }",
    "res struct S { a: int }",
    "struct S[T: val] { a: T }\nstruct U { a: S[int] }",
    "struct S[T: val] { a: T }\nstruct U { a: S[Box[int]] }",
    "extern fn g(x: int) -> [] int;",
    "static t: [int] { return 1; }\nfn main(world: World) -> [] int {  return 0; }",
    "static t: [int] { return 1; }\nstatic t: [int] { return 2; }",
    "static t: [int] { return 1; }\nfn t() -> [] int { return 1; }",
    "fn t() -> [] int { return 1; }\nstatic t: [int] { return 1; }",
    "static t: int { return 1; }",
    "static t: [f32] { return 1; }",
    "static t: [S] { return 1; }\nstruct S { a: int }",
    "static t: [Nope] { return 1; }",
    "static t: &static [int] { return 1; }",
    "static a: [bool] { return 1; }\nstatic b: [float] { return 1; }\nstatic c: [byte] { return 1; }",
    "fn f() -> [] int { return 1; }\nfn f() -> [] int { return 2; }",
    "module m;\nfn f() -> [] int { return 1; }\nfn f() -> [] int { return 2; }",
    "fn getchar() -> [] int { return 1; }",
    "edition 1;\nfn connect() -> [] int { return 1; }",
    "edition 2;\nfn connect() -> [] int { return 1; }",
    "fn f[int]() -> [] int { return 1; }",
    "fn f[T, T]() -> [] int { return 1; }",
    "fn f[&r, &r]() -> [] int { return 1; }",
    "fn f[T, &T]() -> [] int { return 1; }",
    "fn f[&static]() -> [] int { return 1; }",
    "fn f[&a, &b where a <= b](x: &a int, y: &b int) -> [] int { return 1; }\nfn main(world: World) -> [] int {  return 0; }",
    "fn f[&a where a <= b]() -> [] int { return 1; }",
    "fn f[&b where a <= b]() -> [] int { return 1; }",
    "fn f[T, &a where a <= T]() -> [] int { return 1; }",
    "fn f(a: int, a: int) -> [] int { return 1; }",
    "fn f(a: int, b: Nope, a: int) -> [] int { return 1; }",
    "fn f(a: Nope) -> [] int { return 1; }",
    "fn f() -> [] Nope { return 1; }",
    "fn f(a: &r int) -> [] int { return 1; }",
    "fn f[&r](a: &r int) -> [] int { return 1; }\nfn main(world: World) -> [] int {  return 0; }",
    "fn f(a: [int]) -> [] int { return 1; }",
    "fn f[&r](a: &r [int], b: &!r [int]) -> [] int { return 1; }\nfn main(world: World) -> [] int {  return 0; }",
    "fn f[T](a: T[int]) -> [] int { return 1; }",
    "struct S { a: int }\npub fn f(a: S) -> [] int { return 1; }",
    "struct S { a: int }\npub fn f() -> [] S { return S { a: 1 }; }",
    "struct S { a: int }\npub fn f(a: Box[S]) -> [] int { return 1; }",
    "struct S { a: int }\npub fn f[&r](a: &r S) -> [] int { return 1; }",
    "struct S { a: int }\npub fn f(a: (int, S)) -> [] int { return 1; }",
    "struct S { a: int }\npub fn f(a: fn(S) -> [] int) -> [] int { return 1; }",
    "pub struct S { a: int }\npub fn f(a: S) -> [] int { return 1; }\nfn main(world: World) -> [] int {  return 0; }",
    "struct S { a: int }\nfn f(a: S) -> [] int { return 1; }\nfn main(world: World) -> [] int {  return 0; }",
    "import nothing.here;\nfn main(world: World) -> [] int {  return 0; }",
    "module a.b;\nimport a.b;\nfn main(world: World) -> [] int {  return 0; }",
    "module a.b;\nimport a.b as x;\nstruct T { a: int }\nstruct S { a: x.T }",
    "module a.b;\nimport a.b;\nimport a.b as b;",
    "module a.b;\nimport a.b as x;\nimport a.b as x;",
    "import a;",
    "fn f() -> [] int { return 1; }\nimport zzz;",
    "import std.io;\nfn main(world: World) -> [] int {  return 0; }",
    "fn f(a: World, b: Io, c: Heap, d: Box[int], e: Split, f: Args, g: Fs) -> [] int { return 1; }",
    "edition 2;\nfn f(a: Net) -> [] int { return 1; }",
    "edition 1;\nfn f(a: Net) -> [] int { return 1; }",
    "edition 7;\nfn f(a: Exec, b: Child) -> [] int { return 1; }",
    "edition 2;\nfn f(a: Split) -> [] int { return 1; }",
    "fn f(a: &r Ffi(\"libc\")) -> [] int { return 1; }",
    "struct A { b: B }\nstruct B { a: int }\nfn main(world: World) -> [] int {  return 0; }",
    "struct A { a: Nope }\nstruct A { b: int }",
    "struct A { b: int }\nstruct A { b: int }\nstruct C { a: Nope }",
    "struct A { a: A, b: Nope }",
    "struct S { a: float, b: byte, c: bool, d: int }\nfn main(world: World) -> [] int {  return 0; }",
    "edition 5;\nstruct S { a: f32 }",
    "edition 6;\nstruct S { a: f32 }\nfn main(world: World) -> [] int {  return 0; }",
    "struct byte { a: int }\nstruct S { a: byte }\nfn main(world: World) -> [] int {  return 0; }",
];

fn sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["std", "examples", "packages", "tests/accept", "tests/reject", "tests/programs"] {
        super::duplication::walk_ls_files(&root.join(dir), &mut files);
    }
    files.sort();
    let mut out = Vec::new();
    for path in files {
        // The ports read bytes; the oracles take a `&str`, so a file that is not UTF-8 has no
        // oracle answer worth comparing (its spans would be those of the lossily decoded text).
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.push((path.display().to_string(), text));
        }
    }
    out.extend(EDGE.iter().map(|s| (format!("edge case {s:.40?}"), (*s).to_owned())));
    assert!(out.len() > 500, "the corpus walk found only {} files", out.len());
    out
}

/// The files of `parser.ls` and `check.ls`: the root, which says what to print, and the modules
/// the two share.
fn with_front_end(root: &'static str) -> Vec<&'static str> {
    vec![
        root,
        "driver.ls",
        "listing.ls",
        "pass1.ls",
        "ast.ls",
        "kinds.ls",
        "lexcore.ls",
        "tables.ls",
    ]
}

fn build(tag: &str, files: &[&str]) -> PathBuf {
    let dir = scratch(&format!("selfhost-{tag}"));
    let exe = dir.join(tag);
    let root = repo_root().join("examples/selfhost");
    let build = Command::new(BIN)
        .arg("build")
        .args(files.iter().map(|f| root.join(f)))
        .args(["--std".as_ref(), "-o".as_ref(), exe.as_os_str()])
        .output()
        .expect("the compiler runs");
    assert!(
        build.status.success(),
        "`{files:?}` should compile, but the compiler said:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    exe
}

/// Run a port with the whole of `text` on its standard input. Both ports read to the end of
/// the input before they print anything, so writing it all first cannot deadlock.
fn answer(exe: &Path, text: &str) -> String {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the compiled port runs");
    child
        .stdin
        .take()
        .expect("a piped stdin")
        .write_all(text.as_bytes())
        .expect("the port reads its input");
    let output = child.wait_with_output().expect("the port finishes");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn agree(tag: &str, files: &[&str], oracle: fn(&str) -> String) {
    let exe = build(tag, files);
    let mut different = Vec::new();
    let corpus = sources();
    for (name, text) in &corpus {
        if answer(&exe, text) != oracle(text) {
            different.push(name.clone());
        }
    }
    assert!(
        different.is_empty(),
        "`{files:?}` and the Rust front end disagree about {} of {} programs:\n{}",
        different.len(),
        corpus.len(),
        different.join("\n")
    );
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// The checker port against `check_declarations`. A port answer of `SKIP` says the declarations
/// use something whose checks are not ported, and is not compared; everything else must be
/// the oracle's answer, byte for byte, and enough of the corpus must be compared that the test
/// cannot pass by skipping.
#[test]
fn the_checker_in_lex_sys_agrees_with_the_rust_declarations_check() {
    let exe = build("check", &with_front_end("check.ls"));
    let mut corpus = sources();
    corpus.extend(CHECK_EDGE.iter().map(|s| (format!("check case {s:.40?}"), (*s).to_owned())));
    let (mut compared, mut skipped, mut refusals) = (0, 0, 0);
    let mut different = Vec::new();
    for (name, text) in &corpus {
        let ours = answer(&exe, text);
        if ours == "SKIP\n" {
            skipped += 1;
            continue;
        }
        compared += 1;
        let theirs = declarations_oracle::listing(text);
        refusals += usize::from(theirs.starts_with("ERR"));
        if ours != theirs {
            different.push(format!("{name}: port {ours:?}, rust {theirs:?}"));
        }
    }
    assert!(
        different.is_empty(),
        "`check.ls` and `check_declarations` disagree about {} of {compared} programs:\n{}",
        different.len(),
        different.join("\n")
    );
    assert!(compared > 500 && refusals > 100, "compared {compared}, of which {refusals} refusals");
    assert!(skipped * 10 < compared, "{skipped} programs skipped of {}", corpus.len());
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

#[test]
fn the_lexer_in_lex_sys_agrees_with_the_rust_lexer() {
    agree("lexer", &["lexer.ls", "lexcore.ls"], token_oracle::listing);
}

#[test]
fn the_parser_in_lex_sys_agrees_with_the_rust_parser() {
    agree("parser", &with_front_end("parser.ls"), ast_oracle::listing);
}
