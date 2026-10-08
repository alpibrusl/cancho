//! `examples/selfhost/` (`docs/self-hosting.md`): the cancho lexer and parser, written
//! in cancho, against the Rust ones they were ported from.
//!
//! Each port reads a source file on standard input and prints what the Rust front end
//! made of it: the lexer one `Kind start end` line per token, the parser one line per
//! syntax node in postfix order, and either of them, for a file the Rust front end
//! refuses, the single line `ERR rule start end`. The oracles are the two examples of
//! `cancho-syntax` (`dump_tokens.rs`, `dump_ast.rs`), included here so that the test and
//! the command-line tools `diff.sh` and `fuzz.py` use cannot disagree about the format.
//!
//! The corpus is every program in the repository, the ones that must be refused included
//! (a refusal is a line to match too), and the edge cases below: what the repository does
//! not contain. `examples/selfhost/fuzz.py` adds mutants, which is where most refusals
//! come from; it is not run here because it takes a minute, and the corpus already holds
//! the shapes it found.

use super::*;

mod cases;
mod compile;
use cases::BODY_EDGE;

#[allow(dead_code)]
#[path = "../../../cancho-syntax/examples/dump_ast.rs"]
mod ast_oracle;
#[allow(dead_code)]
#[path = "../../../cancho-ir/examples/check_bodies.rs"]
mod bodies_oracle;
#[allow(dead_code)]
#[path = "../../../cancho-ir/examples/check_declarations.rs"]
mod declarations_oracle;
#[allow(dead_code)]
#[path = "../../../cancho-syntax/examples/dump_tokens.rs"]
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

/// Programs for the declarations half of the checker (`examples/selfhost/check.cho`): each
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
    "extern fn g[&f, &r, &i, &h](x: int) -> [] int;",
    "extern fn getpid[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn getchar[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "edition 1;\nextern fn connect[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\nextern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "module a;\nextern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\nmodule b;\nextern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&r, &r](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&static](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), x: Nope) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] Nope;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), x: float) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), x: byte) -> [ffi(\"libc\")] int;",
    "struct S { a: int }\nextern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), x: S) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r](ffi: &f Ffi(\"libc\"), x: &r int) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r](ffi: &f Ffi(\"libc\"), x: &r [int]) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r](ffi: &f Ffi(\"libc\"), x: &r [byte]) -> [ffi(\"libc\")] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r](ffi: &f Ffi(\"libc\"), x: &!r [byte]) -> [ffi(\"libc\")] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), p: c_ptr) -> [ffi(\"libc\")] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), p: c_int) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] c_int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] c_ptr;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] bool;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] float;",
    "struct S { a: int }\nextern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] S;",
    "extern fn g[&f, &r](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] &r [byte];",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] c_ptr[int];",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"\")) -> [ffi(\"\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"lib c\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc,libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\"), io_write] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libm\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\"), ffi(\"libc\")] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc,libm\")) -> [ffi(\"libc,libm\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libm,libc\")) -> [ffi(\"libc,libm\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc,libm\")) -> [ffi(\"libm,libc\")] int;",
    "extern fn g[&f, &r, &i, &h](x: int) -> [] int;",
    "extern fn g[&f, &g](a: &f Ffi(\"libc\"), b: &g Ffi(\"libm\")) -> [ffi(\"libc\"), ffi(\"libm\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), io: &!i Io) -> [ffi(\"libc\"), io_read, io_write, err_write] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), io: &!i Io) -> [ffi(\"libc\"), io_write] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), h: &!h Heap) -> [ffi(\"libc\"), heap] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), a: &r Args) -> [ffi(\"libc\"), args] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), fs: &r Fs(\"/tmp\")) -> [ffi(\"libc\"), fs_read(\"/tmp\"), fs_write(\"/tmp\"), file_read, file_write, dir_read, dir_write] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), fs: &r Fs(\"/tmp\")) -> [ffi(\"libc\"), fs_read(\"/tmp\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), fs: &r Fs(\"/tmp\")) -> [ffi(\"libc\"), fs_read(\"/var\"), fs_write(\"/tmp\"), file_read, file_write, dir_read, dir_write] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), fs: &r Fs(\"a\\\\b\")) -> [ffi(\"libc\"), fs_read(\"a\\\\b\"), fs_write(\"a\\\\b\"), file_read, file_write, dir_read, dir_write] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), w: &r World) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), f: &r File) -> [ffi(\"libc\"), file_read, file_write] int;\nfn main(world: World) -> [] int {  return 0; }",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\"), fs: &r Fs) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\nfn g() -> [] int { return 1; }",
    "fn g() -> [] int { return 1; }\nextern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    "extern fn g[&f, &r, &i, &h](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\nstatic g: [int] { return 1; }",
    "extern fn g[&f, &r, &i, &h](x: Nope) -> [] int;\nstruct S { a: Nope }",
    "struct S { a: Nope }\nextern fn g[&f, &r, &i, &h](x: Nope) -> [] int;",
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

/// The files of `parser.cho` and `check.cho`: the root, which says what to print, and the modules
/// the two share.
fn with_front_end(root: &'static str) -> Vec<&'static str> {
    vec![
        root,
        "driver.cho",
        "listing.cho",
        "checker.cho",
        "body.cho",
        "exprs.cho",
        "scope.cho",
        "prims.cho",
        "builtins.cho",
        "types.cho",
        "emit.cho",
        "foreign.cho",
        "pass1.cho",
        "ast.cho",
        "kinds.cho",
        "rules.cho",
        "lexcore.cho",
        "tables.cho",
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

/// A program of several files as the ports read it: each a line `FILE <length>` and then that many
/// bytes (`examples/selfhost/driver.cho`).
fn stream(files: &[String]) -> String {
    files.iter().map(|f| format!("FILE {}\n{f}", f.len())).collect()
}

/// The program's file and then the standard library, as `cancho check --std` parses them. The
/// library is every file of `std/` in name order; the compiler's own order differs, which cannot
/// change an answer about the program's file because the library has no refusal of its own and
/// the program's items come first.
fn with_library(program: &str) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(repo_root().join("std"))
        .expect("std/")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "cho"))
        .collect();
    names.sort();
    let mut files = vec![program.to_owned()];
    files.extend(names.iter().map(|p| std::fs::read_to_string(p).expect("a library file")));
    files
}

/// Run `f` over `items` on four threads: each answer takes a process and the library is half a
/// megabyte.
fn in_parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let chunk = items.len().div_ceil(4).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| scope.spawn(|| part.iter().map(&f).collect::<Vec<R>>()))
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a worker finished")).collect()
    })
}

/// The checker port against `check_declarations`: every answer must be the oracle's, byte for
/// byte.
///
/// Twice: each program alone, which is how it stops at its first `import std...`, and with the
/// library, which is what `cancho check --std` does and what makes the program's own
/// declarations meet the library's. The second is a sample (every fourth program and every edge
/// case), because each of its runs parses half a megabyte.
#[test]
fn the_checker_in_cancho_agrees_with_the_rust_declarations_check() {
    let alone = build("check", &with_front_end("check.cho"));
    let with_std = build("check_with_std", &with_front_end("check_files.cho"));
    let mut corpus = sources();
    corpus.extend(CHECK_EDGE.iter().map(|s| (format!("check case {s:.40?}"), (*s).to_owned())));
    let sample: Vec<&(String, String)> = corpus
        .iter()
        .enumerate()
        .filter(|(i, (name, _))| i % 4 == 0 || name.starts_with("check case"))
        .map(|(_, c)| c)
        .collect();

    let mut compared = 0;
    let mut refusals = 0;
    let mut different = Vec::new();
    let mut judge = |what: &str, name: &str, ours: String, theirs: String| {
        compared += 1;
        refusals += usize::from(theirs.starts_with("ERR"));
        if ours != theirs {
            different.push(format!("{what} {name}: port {ours:?}, rust {theirs:?}"));
        }
    };
    let alone_answers = in_parallel(&corpus, |(_, text)| answer(&alone, text));
    for ((name, text), ours) in corpus.iter().zip(alone_answers) {
        judge("alone", name, ours, declarations_oracle::listing(text));
    }
    let library_answers = in_parallel(&sample, |(_, text)| {
        let files = with_library(text);
        (answer(&with_std, &stream(&files)), declarations_oracle::listing_files(&files))
    });
    for ((name, _), (ours, theirs)) in sample.iter().zip(library_answers) {
        judge("with std", name, ours, theirs);
    }
    assert!(
        different.is_empty(),
        "`check.cho` and `check_declarations` disagree about {} of {compared} programs:\n{}",
        different.len(),
        different.join("\n")
    );
    assert!(compared > 700 && refusals > 200, "compared {compared}, of which {refusals} refusals");
    let _ = std::fs::remove_dir_all(alone.parent().expect("a scratch directory"));
    let _ = std::fs::remove_dir_all(with_std.parent().expect("a scratch directory"));
}

/// Programs of several files, which the corpus has none of: modules declared in more than one
/// file, an edition per file, imports across files, and refusals in a later file.
const SEVERAL_FILES: &[&[&str]] = &[
    &["fn main(world: World) -> [] int { return 0; }", "fn helper() -> [] int { return 1; }"],
    &[
        "module a;\nimport a;\nfn f() -> [] int { return 1; }",
        "module a;\nfn g() -> [] int { return 2; }",
    ],
    &["module a;\nstruct T { x: int }", "module a;\nstruct T { y: int }"],
    &["module a;\nstruct T { x: int }", "module b;\nstruct T { y: int }"],
    &["module a;\npub struct T { x: int }", "module b;\nimport a;\nstruct U { t: a.T }"],
    &["module a;\nstruct T { x: int }", "module b;\nimport a;\nstruct U { t: a.T }"],
    &["module a.b;\nstruct T { x: int }", "module a.b;\nimport a.b as q;\nstruct U { t: q.T }"],
    &["edition 2;\nstruct S { n: Net }", "struct S { n: Net }"],
    &["struct S { n: Net }", "edition 2;\nstruct S { n: Net }"],
    &["edition 9;\n", "fn f() -> [] int { return 1; }"],
    &["fn f() -> [] int { return 1; }", "edition 9;\n"],
    &["fn f() -> [] int { return 1x; }", "fn g() -> [] int { return 1; }"],
    &["fn f() -> [] int { return 1; }", "fn g() -> [] int { return 1x; }"],
    &["fn f() -> [] int { return 1; }", "fn f() -> [] int { return 2; }"],
    &["module m;\nfn f() -> [] int { return 1; }", "module m;\nfn f() -> [] int { return 2; }"],
    &["import zzz;", "module zzz;"],
    &["module zzz;", "import zzz;"],
    &["", "", ""],
    &["module m;", "", "struct S { a: Nope }"],
    &["struct S { a: T }", "struct T { a: S }"],
    &[
        "module a;\nextern fn g[&f](f: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
        "module b;\nextern fn g[&f](f: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    ],
    &["extern fn g[&f](f: &f Ffi(\"libc\")) -> [] int;", "fn f() -> [] int { return 1; }"],
    &[
        "module a;\nfn g() -> [] int { return 1; }",
        "module b;\nimport a;\nfn f() -> [] int { return a.g(); }",
    ],
    &[
        "module a;\npub fn g() -> [] int { return 1; }",
        "module b;\nimport a;\nfn f() -> [] int { return a.g(); }",
    ],
    &[
        "module a;\npub fn g(x: int) -> [] int { return x; }",
        "module b;\nimport a;\nfn f() -> [] int { return a.g(true); }",
    ],
    &[
        "module a;\npub fn g[&r](x: &r int) -> [] int { return *x; }",
        "module b;\nimport a;\nfn f[&q](p: &q int) -> [] int { return a.g(p); }",
    ],
    &[
        "module a;\nfn g[&r](x: &r int) -> [] int { return *x; }",
        "module b;\nimport a;\nfn f[&q](p: &q int) -> [] int { return a.g(p); }",
    ],
    &["fn f() -> [] int { return g(); }", "fn g() -> [] int { return 1; }"],
    &["fn f() -> [] int { return g(); }", "module m;\nfn g() -> [] int { return 1; }"],
    &[
        "fn g() -> [] int { return 1; }",
        "extern fn g[&f](f: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;",
    ],
    &[
        "module a;\npub enum E { A, B(int) }",
        "module b;\nimport a;\nfn f() -> [] int { let e = a.E::B(1); match e { a.E::A => { return 1; } a.E::B(x) => { return x; } } }",
    ],
    &[
        "module a;\nenum E { A, B(int) }",
        "module b;\nimport a;\nfn f() -> [] int { let e = a.E::A; return 1; }",
    ],
    &[
        "module a;\npub enum E { A, B(int) }\npub enum F { A }",
        "module b;\nimport a;\nfn f(e: a.E) -> [] int { match e { a.F::A => { return 1; } _ => { return 2; } } }",
    ],
    &[
        "module a;\npub enum E { A, B(int) }",
        "module b;\nimport a;\nfn f(e: a.E) -> [] int { match e { E::A => { return 1; } _ => { return 2; } } }",
    ],
    &[
        "module a;\npub enum E { A, B(int) }",
        "module b;\nimport a;\nfn f(e: a.E) -> [] int { match e { z.E::A => { return 1; } _ => { return 2; } } }",
    ],
    &[
        "module a;\npub fn id[T: val](x: T) -> [] T { return x; }",
        "module b;\nimport a;\nfn f() -> [] int { return a.id(3); }",
    ],
    &[
        "module a;\nfn id[T: val](x: T) -> [] T { return x; }",
        "module b;\nimport a;\nfn f() -> [] int { return a.id(3); }",
    ],
    &[
        "module a;\npub fn id[T: val](x: T) -> [] T { return x; }",
        "module b;\nimport a;\nfn f() -> [] bool { return a.id(3); }",
    ],
    &[
        "module a;\npub fn make[T: val]() -> [] T { return make(); }",
        "module b;\nimport a;\nfn f() -> [] int { return a.make(); }",
    ],
    &[
        "module a;\npub fn id[&q](x: &q int) -> [] &q int { return x; }",
        "module b;\nimport a;\nfn f() -> [] int { var v = 1; borrow v as &p in { return *a.id(p); } return 0; }",
    ],
    &[
        "module a;\npub fn id[&q](x: &q int) -> [] &q int { return x; }",
        "module b;\nimport a;\nfn f(o: &r int) -> [] &r int { var v = 1; borrow v as &p in { return a.id(p); } return o; }",
    ],
];

/// Do two answers of the body checker agree? Line by line, where the port may answer `SKIP` for a
/// function whose body it does not check yet: how many lines were compared and how many skipped.
fn bodies_agree(ours: &str, theirs: &str) -> Result<(usize, usize), String> {
    let (ours, theirs): (Vec<&str>, Vec<&str>) = (ours.lines().collect(), theirs.lines().collect());
    if ours.len() != theirs.len() {
        return Err(format!("{} lines, rust {}", ours.len(), theirs.len()));
    }
    let (mut compared, mut skipped) = (0, 0);
    for (a, b) in ours.iter().zip(&theirs) {
        if a.ends_with(" SKIP") {
            skipped += 1;
        } else if a == b {
            compared += 1;
        } else {
            return Err(format!("port {a:?}, rust {b:?}"));
        }
    }
    Ok((compared, skipped))
}

/// The body checker port against `check_bodies`, function by function: a function the port
/// answers `SKIP` for is not compared, and enough must be that the test cannot pass by skipping.
/// Alone, and with the library (a sample, and the library's own functions with each).
#[test]
fn the_body_checker_in_cancho_agrees_with_the_rust_one_per_function() {
    let alone = build("bodies", &with_front_end("bodies.cho"));
    let with_std = build("bodies_with_std", &with_front_end("bodies_files.cho"));
    let mut corpus = sources();
    corpus.extend(BODY_EDGE.iter().map(|s| (format!("body case {s:.40?}"), (*s).to_owned())));
    let sample: Vec<&(String, String)> = corpus
        .iter()
        .enumerate()
        .filter(|(i, (name, _))| i % 8 == 0 || name.starts_with("body case"))
        .map(|(_, c)| c)
        .collect();
    let (mut compared, mut skipped) = (0, 0);
    let mut different = Vec::new();
    let mut judge =
        |what: &str, name: &str, ours: String, theirs: String| match bodies_agree(&ours, &theirs) {
            Ok((c, s)) => {
                compared += c;
                skipped += s;
            }
            Err(why) => different.push(format!("{what} {name}: {why}")),
        };
    let alone_answers = in_parallel(&corpus, |(_, text)| answer(&alone, text));
    for ((name, text), ours) in corpus.iter().zip(alone_answers) {
        judge("alone", name, ours, bodies_oracle::listing(text));
    }
    let library_answers = in_parallel(&sample, |(_, text)| {
        let files = with_library(text);
        (answer(&with_std, &stream(&files)), bodies_oracle::listing_files(&files))
    });
    for ((name, _), (ours, theirs)) in sample.iter().zip(library_answers) {
        judge("with std", name, ours, theirs);
    }
    assert!(
        different.is_empty(),
        "`bodies.cho` and `check_bodies` disagree about {} programs:\n{}",
        different.len(),
        different.join("\n")
    );
    assert!(compared > 20_000, "compared {compared} lines, skipped {skipped}");
    let _ = std::fs::remove_dir_all(alone.parent().expect("a scratch directory"));
    let _ = std::fs::remove_dir_all(with_std.parent().expect("a scratch directory"));
}

#[test]
fn the_ports_read_several_files_as_the_compiler_does() {
    let parser = build("several_parser", &with_front_end("parser_files.cho"));
    let checker = build("several_check", &with_front_end("check_files.cho"));
    let bodies = build("several_bodies", &with_front_end("bodies_files.cho"));
    let mut cases: Vec<Vec<String>> =
        SEVERAL_FILES.iter().map(|case| case.iter().map(|f| (*f).to_owned()).collect()).collect();
    // The library alone, and a program with it: the tree of half a megabyte.
    cases.push(with_library("fn main(world: World) -> [] int { return 0; }"));
    for files in &cases {
        let input = stream(files);
        assert_eq!(
            answer(&parser, &input),
            ast_oracle::listing_files(files),
            "the listing of {:?}",
            files.iter().take(3).collect::<Vec<_>>()
        );
        assert_eq!(
            answer(&checker, &input),
            declarations_oracle::listing_files(files),
            "the check of {:?}",
            files.iter().take(3).collect::<Vec<_>>()
        );
        let theirs = bodies_oracle::listing_files(files);
        assert!(
            bodies_agree(&answer(&bodies, &input), &theirs).is_ok(),
            "the bodies of {:?}",
            files.iter().take(3).collect::<Vec<_>>()
        );
    }
    let _ = std::fs::remove_dir_all(parser.parent().expect("a scratch directory"));
    let _ = std::fs::remove_dir_all(checker.parent().expect("a scratch directory"));
    let _ = std::fs::remove_dir_all(bodies.parent().expect("a scratch directory"));
}

#[test]
fn the_lexer_in_cancho_agrees_with_the_rust_lexer() {
    agree("lexer", &["lexer.cho", "lexcore.cho"], token_oracle::listing);
}

#[test]
fn the_parser_in_cancho_agrees_with_the_rust_parser() {
    agree("parser", &with_front_end("parser.cho"), ast_oracle::listing);
}
