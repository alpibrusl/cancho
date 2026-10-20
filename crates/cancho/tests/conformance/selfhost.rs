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

mod bootstrap;
mod cases;
mod compile;
mod parity;
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
#[path = "../../../cancho-id/examples/dump_ids.rs"]
mod ids_oracle;
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
    "edition 9;\nfn f() -> [] int { return 1; }",
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
pub(crate) fn with_front_end(root: &'static str) -> Vec<&'static str> {
    vec![
        root,
        "driver.cho",
        "printing.cho",
        "floatprint.cho",
        "formatting.cho",
        "jsonprint.cho",
        "jsonread.cho",
        "ingest.cho",
        "blake3.cho",
        "identity.cho",
        "prune.cho",
        "listing.cho",
        "checker.cho",
        "body.cho",
        "exprs.cho",
        "scope.cho",
        "effects.cho",
        "prims.cho",
        "builtins.cho",
        "signatures.cho",
        "types.cho",
        "emit.cho",
        "foreign.cho",
        "pass1.cho",
        "ast.cho",
        "kinds.cho",
        "rules.cho",
        "lexcore.cho",
        "tables.cho",
        "runtime.cho",
        "consteval.cho",
        "instances.cho",
        "floats.cho",
        "floatcode.cho",
        "fold.cho",
        "threadcode.cho",
        "codes.cho",
        "externcode.cho",
        "netcode.cho",
        "platform.cho",
        "wordcode.cho",
        "cryptocode.cho",
        "ex.cho",
        "lookup.cho",
        "atoms.cho",
        "narrowing.cho",
        "allocs.cho",
        "filecode.cho",
    ]
}

pub(crate) fn build(tag: &str, files: &[&str]) -> PathBuf {
    let dir = scratch(&format!("selfhost-{tag}"));
    let exe = dir.join(tag);
    let root = repo_root().join("examples/selfhost");
    // The first compiler is built by the Rust one, whatever the default is: that is what makes it the first.
    let build = Command::new(BIN)
        .arg("build")
        .args(files.iter().map(|f| root.join(f)))
        .args(["--std".as_ref(), "-o".as_ref(), exe.as_os_str()])
        .env("CANCHO_COMPILER", "rust")
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

/// Programs for the printer that the repository does not hold: every way the tree needs parentheses, `else if` ladders, the shapes a
/// destructuring, a pattern or a declaration's brackets take, escapes, and one-part tuples.
const PRINT_EDGE: &[&str] = &[
    "fn f(a: int, b: int, c: int) -> [] int { return (a + b) * c - (a - (b - c)) / (a % b); }",
    "fn f(a: int, b: int, c: int) -> [] int { return a << b >> c | a ^ b & c; }",
    "fn f(a: bool, b: bool, c: bool) -> [] bool { return a && b || c && !(a || b); }",
    "fn f(a: int) -> [] int { return -(a / 10) + -(-a) + ~a + *a; }",
    "fn f() -> [] int { return -(5) + -5 + -(0x10) - (-9223372036854775808); }",
    "fn f() -> [] float { return -(1.5) + -1.5 + 2.5 - (-0.0); }",
    "fn f(a: int) -> [] int { return ((a)) + (((a + 1))); }",
    "fn f(a: [int]) -> [] int { return a[1] + a[(1 + 2)] + (a)[0] + a[0..2][1..2][0]; }",
    "fn f(t: (int, (int, int))) -> [] int { return t.1.0 + (1).2 + (1.5).0; }",
    "fn f(t: int) -> [] (int,) { return (t,); }",
    "fn f(t: int) -> [] (int, int) { return (t, t); }",
    "fn f(t: int) -> [] () { return (); }",
    "struct P { x: int, y: int }\nfn f(p: P) -> [] int { if (P { x: 1, y: 2 }).x == 1 { return 1; } else if p.x == 2 { return 2; } else { return 3; } }",
    "struct P { x: int }\nfn f() -> [] int { while (P { x: 1 }).x < 3 { return 1; } return 0; }",
    "struct P { x: int }\nfn f() -> [] int { match (P { x: 1 }).x { } return 0; }",
    "struct P { x: int }\nfn f() -> [] int { if (P { x: 1 }).x + 1 == 2 { return 1; } return 0; }",
    "struct P { x: int }\nfn f() -> [] P { return P { }; }",
    "enum E { A, B(int), C(int, [int]) }\nfn f(e: E) -> [] int { match e { E::A => { return 0; } E::B(x) => { return x; } E::C(_, y) => { return y[0]; } } }",
    "enum E[T] { A, B(T) }\nfn f(e: E[int]) -> [] int { match e { E::A => { } E::B(_) => { } } return 0; }",
    "fn f(a: int) -> [] int { if a == 1 { return 1; } else if a == 2 { return 2; } else if a == 3 { return 3; } return 0; }",
    "fn f(a: int) -> [] int { if a == 1 { return 1; } else { if a == 2 { return 2; } } return 0; }",
    "fn f(a: int) -> [] int { if a == 1 { return 1; } else { if a == 2 { return 2; } return 3; } }",
    "fn f(a: int) -> [] int { if a == 1 { } else { } return 0; }",
    "fn f[T, U: val, &r, &q where r <= q](a: &r T, b: &!q U) -> [] int { return 0; }",
    "fn f[&r](a: &r [byte]) -> [io] int { return 0; }",
    "fn f[T: val](a: T) -> [] T { return a; }",
    "fn f[&r, T, &q](a: T) -> [heap, ffi(\"libc\"), io_write] int { return 0; }",
    "fn f(g: fn(int, [byte]) -> [io, fs_read(\"/etc\")] int) -> [] int { return g(1, \"x\"); }",
    "extern fn puts(s: [byte]) -> [ffi(\"libc\")] int;\nextern fn g[&r](x: &r int) -> [] int;",
    "pub struct S[T] { a: T, b: [T], }\npub res struct R { a: int }\nval struct V { a: int }\nres enum Q { A(R) }\npub val enum W { A }",
    "static T: [int] { return alloc_free(); }\npub static U: [byte] { return \"a\\\\b\\n\\t\\0\\\"\"; }",
    "fn f() -> [] int { let (a, b) = g(); let S { x, y } = h(); let q.S { x } = h(); let T { } = h(); return a; }",
    "fn f() -> [] int { var x: int = 1; let y: (int, [byte]) = (1, \"s\"); x = y.0; a[1] = 2; return x; }",
    "fn f(h: &!r Heap) -> [heap] int { borrow mut h as &!r2 in { borrow x as &r3 in { } } region r { let p = alloc[r](1); let q = alloc_slice[r](4, 0); } defer g(); return 0; }",
    "fn f(c: Ffi(\"libc\"), d: Map[Ffi(\"a\"), int], e: Ffi(\"a\", \"b\")) -> [] int { return 0; }",
    "module a.b.c;\nimport std.io;\nimport x.y as z;\nimport p.q as q;\nfn f() -> [] int { return q.g(a.B::C(1), E::D); }",
    "import std.io;\nfn f() -> [] int { return 0; }\n\n\nfn g() -> [] int { return 1; }",
    "fn f() -> [] int { return 1_000 + 0x_ff + 0b1010 + 0o17 + 'a' + '\\n' + 007; }",
    "fn f() -> [] float { return 1e5 + 1E-5 + 1_0.5_0 + 0.1 + 100000000000000000000.0 + 0.000001 + 1e-7 + 12345.678e2; }",
    "fn f() -> [] f32 { return 0.1f32 + 1e10f32 + 16777217.0f32 + 3.4028235e38f32 + 1e-45f32 + 5f32; }",
    "fn f() -> [] int { return a.b.c(1)(2).d[3].e; }",
];

/// `examples/selfhost/printer.cho` against `cancho_syntax::print`: the canonical text of every program in the repository.
#[test]
fn the_printer_in_cancho_agrees_with_the_rust_printer() {
    let exe = build("printer", &with_front_end("printer.cho"));
    let mut different = Vec::new();
    let corpus = sources();
    for (name, text) in &corpus {
        if answer(&exe, text) != ast_oracle::printed(text) {
            different.push(name.clone());
        }
    }
    // What the repository does not hold, and 600 floating-point literals of every size: the shortest digits that read back are the
    // printer's, and they are worked out here with a bignum.
    let mut programs: Vec<String> = PRINT_EDGE.iter().map(|s| (*s).to_owned()).collect();
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut floats = String::new();
    for i in 0..300 {
        let double = f64::from_bits(next() & 0x7fff_ffff_ffff_ffff);
        if double.is_finite() {
            floats.push_str(&format!("fn d{i}() -> [] float {{ return {double:e}; }}\n"));
        }
        let single = f32::from_bits((next() & 0x7fff_ffff) as u32);
        if single.is_finite() {
            floats.push_str(&format!("fn s{i}() -> [] f32 {{ return {single:e}f32; }}\n"));
        }
    }
    programs.push(floats);
    for text in &programs {
        if answer(&exe, text) != ast_oracle::printed(text) {
            different.push(format!("edge case {text:.60?}"));
        }
    }
    assert!(
        different.is_empty(),
        "the printer in cancho and the Rust printer disagree about {} programs:\n{}",
        different.len(),
        different.join("\n")
    );
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// `examples/selfhost/jsonprinter.cho` against `cancho print --output json`: the tree of every program in the repository as data.
#[test]
fn the_json_printer_in_cancho_agrees_with_the_rust_one() {
    let exe = build("jsonprinter", &with_front_end("jsonprinter.cho"));
    let mut different = Vec::new();
    let mut programs = sources();
    programs.extend(PRINT_EDGE.iter().map(|s| (format!("print edge {s:.40?}"), (*s).to_owned())));
    programs.extend(JSON_EDGE.iter().map(|s| (format!("json edge {s:.40?}"), (*s).to_owned())));
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut floats = String::new();
    for i in 0..300 {
        let double = f64::from_bits(next() & 0x7fff_ffff_ffff_ffff);
        if double.is_finite() {
            floats.push_str(&format!("fn d{i}() -> [] float {{ return {double:e}; }}\n"));
        }
        let single = f32::from_bits((next() & 0x7fff_ffff) as u32);
        if single.is_finite() {
            floats.push_str(&format!("fn s{i}() -> [] f32 {{ return {single:e}f32; }}\n"));
        }
        // Subnormals of both sizes, where the bits are most easily got wrong.
        let small = f32::from_bits((next() & 0x007f_ffff) as u32);
        floats.push_str(&format!("fn t{i}() -> [] f32 {{ return {small:e}f32; }}\n"));
        let tiny = f64::from_bits(next() & 0x000f_ffff_ffff_ffff);
        floats.push_str(&format!("fn u{i}() -> [] float {{ return {tiny:e}; }}\n"));
    }
    programs.push(("random floats".to_owned(), floats));
    for (name, text) in &programs {
        if answer(&exe, text) != ast_oracle::printed_json(text) {
            different.push(name.clone());
        }
    }
    assert!(
        different.is_empty(),
        "the JSON printer in cancho and the Rust one disagree about {} of {} programs:\n{}",
        different.len(),
        programs.len(),
        different.join("\n")
    );
    // A literal of more than 120 significant digits has bits this port does not work out: it says so, and writes no JSON.
    let long = format!("fn f() -> [] float {{ return 1{}.0; }}\n", "0".repeat(130));
    let said = answer(&exe, &long);
    assert!(said.starts_with("CANNOT "), "{said:.80}");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// Programs for the identities that the repository has few of: rows that are not sorted or have a label twice, a name declared twice,
/// parameters and bounds and clauses, every kind of pattern and binder, strings with escapes, every literal, names no import reaches.
const IDS_EDGE: &[&str] = &[
    "fn f() -> [io, io_write, io] int { return 0; }",
    "fn g[&i](x: &i Io, y: fn(int) -> [io, io, fs_read] int) -> [ffi(\"b\"), ffi(\"a\"), ffi(\"a\"), io, ffi(\"a\\n\")] int { return 0; }",
    "fn f() -> [] int { return 1; }\nfn f() -> [] int { return 2; }\nfn g() -> [] int { return f(); }",
    "struct S { a: int }\nstruct S { b: bool }\nfn f(s: S) -> [] int { return 0; }",
    "struct A { b: B }\nstruct B { a: A, c: C }\nstruct C { x: int }\nenum E { X(A), Y }\nfn f(a: A, e: E, c: C) -> [] int { return 0; }",
    "fn k[T: val, U, &r, &s where r <= s, s <= r, r <= q](a: &r T, b: &!s U, c: [T], d: (T, U), e: fn(&r T) -> [io] U) -> [io] int { return 0; }",
    "val struct V[T: val] { t: T }\nres struct R { v: V[int], r: &q int }\nenum G[T] { A(T), B }\nres enum H[T: val, U] { C(T, U) }",
    "pub extern fn getpid[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\nextern fn abs(x: int) -> [] int;\nfn f() -> [] int { return abs(getpid(1)); }",
    "static T: [int] { return alloc_free(); }\npub static U: [byte] { return \"a\\\\b\\n\\t\\0\\\"\"; }\nfn f() -> [] int { return len(T) + len(U); }",
    "fn f(p: P, q: q.P) -> [] int { let P { y, x, y2 } = p; let q.P { z, a } = q; return x + y + a + z + y2; }",
    "fn f(t: (int, int)) -> [] int { let (a, b) = t; let (a, c) = (b, a); return a + b + c + t.0 + t.1; }",
    "fn f(e: E) -> [] int { match e { E::A(x, _, y) => { return x + y; } q.E::B(z) => { return z; } E::C => { return 0; } _ => { return 1; } } }",
    "fn f[&r](x: &r int) -> [] int { var y = 1; y = y + *x; borrow x as &s in { region t { let p = alloc[t](1); let q = alloc_slice[s](2, 3); } } borrow mut y as &!u in { } return y; }",
    "fn f(a: int) -> [] int { let a = a + 1; { } if a > 1 { let a = 2; return a; } else { let b = a; return b; } }",
    "fn f(a: int) -> [] int { while a < 3 { defer g(a); } return a; }\nfn g(a: int) -> [] int { return a; }",
    "fn f() -> [] int { return 0 + 1 - 2 * 3 / 4 % 5; }\nfn g(a: bool, b: bool, x: int, y: int) -> [] bool { return a && b || !a && x == y && x != y || x < y && x <= y || x > y && x >= y || x | y == x ^ y && x & y == ~x && x << 1 == y >> 2; }",
    "fn f() -> [] float { return 0.0 + -0.0 + 1.0 + 1e308 + 5e-324 + 0.1 + 123456789.123456789; }\nfn g() -> [] f32 { return 0.0f32 + 1e-45f32 + 1.1754942e-38f32 + 3.4028235e38f32 + 0.1f32; }",
    "fn f() -> [] int { return 9223372036854775807 + 0 + 1_000 + 0x_ff + 0b101 + 'a' + '\\n'; }",
    "fn f() -> [] int { return m.g(m.S { x: 1 }, m.E::A(2), E::B, S { y: 3 }, p.x, p.0, a[1], a[1..2]); }",
    "fn f(s: [byte]) -> [] int { return len(\"é\\u\") + len(s); }",
];

/// `examples/selfhost/ids.cho` against `cancho_id::identify`: the identity of every declaration of the programs of the repository, each alone
/// and each with the standard library after it (where a name is answered by the hash of the declaration it reaches).
#[test]
fn the_identities_in_cancho_agree_with_the_rust_ones() {
    let exe = build("ids", &with_front_end("ids.cho"));
    let mut programs = sources();
    programs.extend(IDS_EDGE.iter().map(|s| (format!("ids edge {s:.40?}"), (*s).to_owned())));
    let mut different = Vec::new();
    let mut compared = 0usize;
    let mut refused = 0usize;
    let alone =
        in_parallel(&programs, |(_, text)| answer(&exe, &stream(std::slice::from_ref(text))));
    for ((name, text), ours) in programs.iter().zip(alone) {
        let theirs = ids_oracle::identities(std::slice::from_ref(text));
        compared += 1;
        refused += usize::from(theirs.starts_with("ERR"));
        if ours != theirs {
            different.push(format!("{name} alone"));
        }
    }
    // Programs of several files, where a name is reached through an import.
    for files in SEVERAL_FILES {
        let files: Vec<String> = files.iter().map(|f| (*f).to_owned()).collect();
        compared += 1;
        if answer(&exe, &stream(&files)) != ids_oracle::identities(&files) {
            different.push(format!("{:?}", files.iter().take(3).collect::<Vec<_>>()));
        }
    }
    // With the library: a sample, as each of these parses half a megabyte.
    let sample: Vec<&(String, String)> = programs
        .iter()
        .enumerate()
        .filter(|(i, (name, _))| i % 8 == 0 || name.starts_with("ids edge"))
        .map(|(_, c)| c)
        .collect();
    let with_std = in_parallel(&sample, |(_, text)| {
        let files = with_library(text);
        (answer(&exe, &stream(&files)), ids_oracle::identities(&files))
    });
    for ((name, _), (ours, theirs)) in sample.iter().zip(with_std) {
        compared += 1;
        if ours != theirs {
            different.push(format!("{name} with the library"));
        }
    }
    assert!(
        different.is_empty(),
        "the identities in cancho and the Rust ones differ for {} of {compared} programs:\n{}",
        different.len(),
        different.join("\n")
    );
    assert!(refused > 5, "only {refused} programs were refused by the parser");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// `examples/selfhost/blake3.cho` against the `blake3` crate: messages of every length around the sizes at which the tree of chunks changes
/// shape (a block is 64 bytes, a chunk 1,024, and the stack of chaining values is joined at every power of two chunks).
#[test]
fn blake3_in_cancho_agrees_with_the_blake3_crate() {
    let exe = build("blake3sum", &["blake3sum.cho", "blake3.cho", "lexcore.cho"]);
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut lengths: Vec<usize> = (0..=130).collect();
    for chunks in [1usize, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 64, 65] {
        for delta in [0usize, 1, 63, 64, 65, 1023] {
            lengths.push(chunks * 1024 - 1024 + delta + 1);
            lengths.push(chunks * 1024 + delta);
        }
    }
    lengths.extend([200_000, 1 << 20]);
    lengths.sort_unstable();
    lengths.dedup();
    let mut different = Vec::new();
    for length in &lengths {
        let message: Vec<u8> = (0..*length)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        let want = format!("{}\n", blake3::hash(&message).to_hex());
        let mut child = Command::new(&exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the compiled port runs");
        child.stdin.take().expect("a piped stdin").write_all(&message).expect("the port reads");
        let got = String::from_utf8_lossy(&child.wait_with_output().expect("it finishes").stdout)
            .into_owned();
        if got != want {
            different.push(*length);
        }
    }
    assert!(
        different.is_empty(),
        "BLAKE3 in cancho and the blake3 crate disagree about messages of {} lengths: {different:?}",
        different.len()
    );
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// JSON for `ingest` that is wrong in the ways its refusals list: not JSON, in each of the reader's ways, and JSON whose unit, items,
/// names, kinds and numbers are not what the builder takes.
const INGEST_EDGE: &[&str] = &[
    "",
    "  ",
    "{",
    "}",
    "[",
    "[1,]",
    "[1 2]",
    "{\"a\":1,}",
    "{\"a\" 1}",
    "{1:2}",
    "{\"a\":1 \"b\":2}",
    "{\"a\":1,\"a\":2}",
    "\"abc",
    "\"\\q\"",
    "\"\\u12\"",
    "\"\\u12G4\"",
    "\"a\\",
    "tru",
    "nul",
    "nullx",
    "truefalse",
    "-",
    "--1",
    "1.5",
    "1e5",
    "99999999999999999999",
    "-9223372036854775808",
    "-9223372036854775809",
    "9223372036854775807",
    "9223372036854775808",
    "007",
    "[1] x",
    "x",
    "{} {}",
    "[]",
    "7",
    "null",
    "\"s\"",
    "{}",
    "{\"items\": []}",
    "{\"items\": 5}",
    "{\"items\": {}}",
    "{\"edition\": 1}",
    "{\"foo\": 1}",
    "{\"module\": 5}",
    "{\"module\": \"\"}",
    "{\"module\": \"a..b\", \"items\": [{\"kind\":\"static\",\"name\":\"x\",\"ty\":{\"name\":\"int\"},\"body\":[]}]}",
    "{\"module\": \"a.b\", \"items\": [{\"kind\":\"static\",\"name\":\"x\",\"ty\":{\"name\":\"int\"},\"body\":[]}]}",
    "{\"module\": \"fn.b\"}",
    "{\"imports\": [{}]}",
    "{\"imports\": [5]}",
    "{\"imports\": {\"path\": \"a\"}}",
    "{\"imports\": [{\"path\": \"std.io\", \"x\": 1}]}",
    "{\"imports\": [{\"path\": \"std.io\", \"as\": \"console\"}]}",
    "{\"imports\": [{\"path\": \"std.io\", \"as\": 3}]}",
    "{\"imports\": [{\"path\": \"std..io\"}]}",
    "{\"imports\": [{\"path\": \"std.io\", \"as\": \"2x\"}]}",
    "{\"items\": [5]}",
    "{\"items\": [{}]}",
    "{\"items\": [{\"kind\": 5}]}",
    "{\"items\": [{\"kind\": \"bogus\"}]}",
    "{\"items\": [{\"kind\": \"fn\"}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\"}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"while\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"public\": 1, \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"int\",\"value\":1}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"int\",\"value\":\"1\"}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"tuple\",\"parts\":[]}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"tuple_field\",\"base\":{\"kind\":\"name\",\"name\":\"a\"},\"index\":-1}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"tuple_field\",\"base\":{\"kind\":\"name\",\"name\":\"a\"},\"index\":4294967297}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"unary\",\"op\":\"?\",\"operand\":{\"kind\":\"int\",\"value\":1}}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"binary\",\"op\":\"**\",\"lhs\":{\"kind\":\"int\",\"value\":1},\"rhs\":{\"kind\":\"int\",\"value\":2}}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"unary\",\"op\":\"-\",\"operand\":{\"kind\":\"int\",\"value\":-5}}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"float\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":18442240474082181120}},{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":9218868437227405312}},{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":9221120237041090560}},{\"kind\":\"return\",\"value\":{\"kind\":\"f32\",\"bits\":2139095040}},{\"kind\":\"return\",\"value\":{\"kind\":\"f32\",\"bits\":2143289344}},{\"kind\":\"return\",\"value\":{\"kind\":\"f32\",\"bits\":4286578688}},{\"kind\":\"return\",\"value\":{\"kind\":\"f32\",\"bits\":1}},{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":9223372036854775808}},{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":0}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"float\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"f32\",\"bits\":4294967295}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"float\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"f32\",\"bits\":4294967296}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"float\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":18446744073709551616}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"float\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":-1}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"float\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"float\",\"bits\":1.5}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"float\"}, \"body\": [{\"kind\":\"return\",\"value\":{\"kind\":\"str\",\"value\":\"a\\u0001\\u00e9\\ud800\\n\\\"\\\\/\"}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"generics\": [\"T\", \"U\"], \"bounds\": [null, \"val\", \"val\", \"res\"], \"regions\": [\"r\"], \"outlives\": [{\"inner\": \"r\", \"outer\": \"r\"}], \"params\": [], \"effects\": [{\"name\": \"io\"}, {\"name\": \"ffi\", \"argument\": \"libc\"}], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"generics\": [], \"regions\": [], \"outlives\": [{\"inner\": \"r\", \"outer\": \"s\"}], \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"bounds\": [\"mut\"], \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"bounds\": [5], \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"effects\": [{\"name\": \"ffi\", \"argument\": 5}], \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"effects\": null, \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [{\"name\": \"a\"}], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [{\"name\": \"a\", \"ty\": 5}], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [{\"name\": \"a\", \"ty\": {\"kind\": 5}}], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [{\"name\": \"a\", \"ty\": {\"kind\": \"zzz\"}}], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [{\"name\": \"a\", \"ty\": {\"kind\": \"ref\", \"unique\": true, \"region\": \"r\", \"inner\": {\"kind\": \"slice\", \"inner\": {\"kind\": \"tuple\", \"parts\": [{\"name\": \"int\", \"qualifier\": \"q\", \"args\": [{\"kind\": \"lit\", \"value\": \"libc\"}, {\"kind\": \"fn\", \"params\": [{\"name\":\"int\"}], \"effects\": [{\"name\":\"io\"}], \"ret\": {\"name\":\"int\"}}]}]}}}}], \"ret\": {\"name\":\"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"extern\", \"name\": \"getpid\", \"regions\": [\"f\"], \"params\": [], \"effects\": [], \"ret\": {\"name\":\"int\"}, \"symbol\": \"getpid\"}]}",
    "{\"items\": [{\"kind\": \"extern\", \"name\": \"getpid\", \"params\": [], \"ret\": {\"name\":\"int\"}}]}",
    "{\"items\": [{\"kind\": \"struct\", \"name\": \"S\", \"fields\": []}]}",
    "{\"items\": [{\"kind\": \"struct\", \"name\": \"S\", \"mode\": \"zzz\", \"fields\": []}]}",
    "{\"items\": [{\"kind\": \"struct\", \"name\": \"S\", \"mode\": 5, \"fields\": []}]}",
    "{\"items\": [{\"kind\": \"struct\", \"name\": \"S\", \"mode\": \"res\", \"generics\": [\"T\"], \"bounds\": [\"val\"], \"fields\": [{\"name\": \"x\", \"ty\": {\"name\": \"T\"}}]}]}",
    "{\"items\": [{\"kind\": \"enum\", \"name\": \"E\", \"variants\": [{\"name\": \"A\"}, {\"name\": \"B\", \"payload\": [{\"name\": \"int\"}]}]}]}",
    "{\"items\": [{\"kind\": \"enum\", \"name\": \"E\", \"variants\": [{\"name\": \"A\", \"payload\": null}]}]}",
    "{\"items\": [{\"kind\": \"enum\", \"name\": \"E\"}]}",
    "{\"items\": [{\"kind\": \"static\", \"name\": \"N\", \"public\": true, \"ty\": {\"name\": \"int\"}, \"body\": []}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"match\",\"scrutinee\":{\"kind\":\"name\",\"name\":\"e\"},\"arms\":[{\"pattern\":{\"kind\":\"wildcard\"},\"body\":[]},{\"pattern\":{\"kind\":\"variant\",\"qualifier\":\"q\",\"name\":\"E\",\"variant\":\"A\",\"bindings\":[null,\"x\"]},\"body\":[]},{\"pattern\":{\"kind\":\"variant\",\"name\":\"E\",\"variant\":\"B\",\"bindings\":[]},\"body\":[]}]}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"match\",\"scrutinee\":{\"kind\":\"name\",\"name\":\"e\"},\"arms\":[{\"pattern\":{\"kind\":\"zzz\"},\"body\":[]}]}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"match\",\"scrutinee\":{\"kind\":\"name\",\"name\":\"e\"},\"arms\":[{\"pattern\":{\"kind\":\"variant\",\"name\":\"E\",\"variant\":\"A\",\"bindings\":[5]},\"body\":[]}]}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"if\",\"cond\":{\"kind\":\"bool\",\"value\":true},\"then\":[],\"else\":[{\"kind\":\"if\",\"cond\":{\"kind\":\"bool\",\"value\":false},\"then\":[],\"else\":null}]},{\"kind\":\"if\",\"cond\":{\"kind\":\"bool\",\"value\":true},\"then\":[],\"else\":[]}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"if\",\"cond\":{\"kind\":\"struct_lit\",\"name\":\"S\",\"fields\":[]},\"then\":[]}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"destructure\",\"qualifier\":\"q\",\"name\":\"S\",\"fields\":[\"a\",\"b\"],\"value\":{\"kind\":\"name\",\"name\":\"s\"}},{\"kind\":\"destructure_tuple\",\"names\":[\"a\"],\"value\":{\"kind\":\"name\",\"name\":\"s\"}},{\"kind\":\"borrow\",\"value\":\"x\",\"unique\":true,\"region\":\"r\",\"body\":[]},{\"kind\":\"region\",\"region\":\"r\",\"body\":[]},{\"kind\":\"defer\",\"value\":{\"kind\":\"call\",\"name\":\"g\",\"args\":[]}},{\"kind\":\"assign\",\"place\":{\"kind\":\"index\",\"base\":{\"kind\":\"name\",\"name\":\"a\"},\"index\":{\"kind\":\"int\",\"value\":1}},\"value\":{\"kind\":\"slice\",\"base\":{\"kind\":\"name\",\"name\":\"a\"},\"start\":{\"kind\":\"int\",\"value\":1},\"end\":{\"kind\":\"int\",\"value\":2}}},{\"kind\":\"let\",\"mutable\":true,\"name\":\"y\",\"ty\":null,\"value\":{\"kind\":\"alloc\",\"region\":\"r\",\"value\":{\"kind\":\"alloc_slice\",\"region\":\"r\",\"count\":{\"kind\":\"int\",\"value\":1},\"fill\":{\"kind\":\"int\",\"value\":0}}}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"destructure\",\"name\":\"S\",\"fields\":[\"a\",5],\"value\":{\"kind\":\"name\",\"name\":\"s\"}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"expr\",\"value\":{\"kind\":\"variant\",\"qualifier\":\"q\",\"name\":\"E\",\"variant\":\"A\",\"args\":[{\"kind\":\"int\",\"value\":1}]}},{\"kind\":\"expr\",\"value\":{\"kind\":\"variant\",\"name\":\"E\",\"variant\":\"A\"}},{\"kind\":\"expr\",\"value\":{\"kind\":\"field\",\"base\":{\"kind\":\"int\",\"value\":1},\"name\":\"f\"}},{\"kind\":\"expr\",\"value\":{\"kind\":\"tuple_field\",\"base\":{\"kind\":\"float\",\"bits\":4607182418800017408},\"index\":0}},{\"kind\":\"expr\",\"value\":{\"kind\":\"tuple\",\"parts\":[{\"kind\":\"int\",\"value\":1}]}}]}]}",
    "{\"items\": [{\"kind\": \"fn\", \"name\": \"f\", \"params\": [], \"ret\": {\"name\":\"int\"}, \"body\": [{\"kind\":\"expr\",\"value\":{\"kind\":\"binary\",\"op\":\"*\",\"lhs\":{\"kind\":\"binary\",\"op\":\"+\",\"lhs\":{\"kind\":\"int\",\"value\":1},\"rhs\":{\"kind\":\"int\",\"value\":2}},\"rhs\":{\"kind\":\"binary\",\"op\":\"-\",\"lhs\":{\"kind\":\"int\",\"value\":3},\"rhs\":{\"kind\":\"binary\",\"op\":\"-\",\"lhs\":{\"kind\":\"int\",\"value\":4},\"rhs\":{\"kind\":\"int\",\"value\":5}}}}}]}]}",
];

/// `value` with one change: a node of the tree (counted from the root) made wrong in the way `how` says.
fn damage(value: &mut cancho_syntax::Json, target: &mut usize, how: u64) -> bool {
    use cancho_syntax::{Json, Span};
    let here = Span::new(0, 0);
    if *target == 0 {
        *target = usize::MAX;
        let whole = how % 20;
        let replacement = match whole {
            0 => Json::Null(here),
            1 => Json::Bool(true, here),
            2 => Json::Int(-1, here),
            3 => Json::Int(7, here),
            4 => Json::Str("x".to_owned(), here),
            5 => Json::Str("bogus".to_owned(), here),
            6 => Json::Str("while".to_owned(), here),
            7 => Json::Str("2fast".to_owned(), here),
            8 => Json::Number("1.5".to_owned(), here),
            9 => Json::Number("18446744073709551616".to_owned(), here),
            10 => Json::Array(Vec::new(), here),
            11 => Json::Object(Vec::new(), here),
            12 => Json::Str(String::new(), here),
            13 => Json::Int(4_294_967_296, here),
            _ => Json::Null(here),
        };
        match (whole, &mut *value) {
            // Take the first child away.
            (14, Json::Array(items, _)) if !items.is_empty() => {
                items.remove(0);
            }
            (14, Json::Object(entries, _)) if !entries.is_empty() => {
                entries.remove(0);
            }
            // Take the last child away.
            (15, Json::Array(items, _)) if !items.is_empty() => {
                items.pop();
            }
            (15, Json::Object(entries, _)) if !entries.is_empty() => {
                entries.pop();
            }
            // Write the first child twice.
            (16, Json::Array(items, _)) if !items.is_empty() => {
                let first = items[0].clone();
                items.insert(0, first);
            }
            (16, Json::Object(entries, _)) if !entries.is_empty() => {
                let first = entries[0].clone();
                entries.insert(0, first);
            }
            // Name the first key `kind`, or the last.
            (17, Json::Object(entries, _)) if !entries.is_empty() => {
                entries[0].0 = "kind".to_owned();
            }
            (18, Json::Object(entries, _)) if !entries.is_empty() => {
                let last = entries.len() - 1;
                entries[last].0 = "zzz".to_owned();
            }
            (19, Json::Array(items, _)) if items.len() > 1 => {
                items.swap(0, 1);
            }
            _ => *value = replacement,
        }
        return true;
    }
    *target -= 1;
    match value {
        Json::Array(items, _) => items.iter_mut().any(|item| damage(item, target, how)),
        Json::Object(entries, _) => entries.iter_mut().any(|(_, item)| damage(item, target, how)),
        _ => false,
    }
}

fn nodes_of(value: &cancho_syntax::Json) -> usize {
    use cancho_syntax::Json;
    match value {
        Json::Array(items, _) => 1 + items.iter().map(nodes_of).sum::<usize>(),
        Json::Object(entries, _) => 1 + entries.iter().map(|(_, v)| nodes_of(v)).sum::<usize>(),
        _ => 1,
    }
}

/// `examples/selfhost/ingester.cho` against `cancho ingest`: the JSON of every program in the repository must give the text `print` gives,
/// and JSON that is wrong in each way the refusals list must be refused with the same rule and the same span.
#[test]
fn the_ingester_in_cancho_agrees_with_the_rust_one() {
    let exe = build("ingester", &with_front_end("ingester.cho"));
    let mut inputs: Vec<(String, String)> = Vec::new();
    let mut programs = sources();
    programs.extend(PRINT_EDGE.iter().map(|s| (format!("print edge {s:.40?}"), (*s).to_owned())));
    programs.extend(JSON_EDGE.iter().map(|s| (format!("json edge {s:.40?}"), (*s).to_owned())));
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for (n, (name, text)) in programs.iter().enumerate() {
        let mut map = cancho_syntax::SourceMap::new();
        let mut ast = cancho_syntax::Ast::new();
        let base = map.add("file", text.clone());
        if cancho_syntax::parse_into(&mut ast, text, base).is_err() {
            continue;
        }
        let json = cancho_syntax::ast_to_json(&ast);
        inputs.push((format!("{name} as json"), cancho_syntax::write_json(&json)));
        // Every program, damaged once or twice in the places that tree has; the larger ones less often, so that the test stays short.
        let times = if text.len() > 3000 { usize::from(n % 4 == 0) } else { 12 };
        for k in 0..times {
            let mut damaged = json.clone();
            let mut target = (next() % nodes_of(&damaged) as u64) as usize;
            damage(&mut damaged, &mut target, next());
            if k % 5 == 4 {
                let mut target = (next() % nodes_of(&damaged) as u64) as usize;
                damage(&mut damaged, &mut target, next());
            }
            inputs.push((format!("{name} damaged {k}"), cancho_syntax::write_json(&damaged)));
        }
    }
    inputs.extend(INGEST_EDGE.iter().map(|s| (format!("ingest edge {s:.50?}"), (*s).to_owned())));
    // Nesting at the limit and past it, in arrays and in objects.
    for depth in [126, 127, 128, 129, 130] {
        inputs.push((
            format!("{depth} arrays"),
            format!("{}{}", "[".repeat(depth), "]".repeat(depth)),
        ));
        inputs.push((
            format!("{depth} objects"),
            format!("{}0{}", "{\"a\":".repeat(depth), "}".repeat(depth)),
        ));
    }
    // Spaces in every place the reader skips them.
    inputs.push((
        "spaced".to_owned(),
        "\n {\t\"items\" :\r[ { \"kind\" : \"static\" , \"name\" : \"x\" , \"ty\" : { \"name\" : \"int\" } , \"body\" : [ ] } ] } \n".to_owned(),
    ));
    let (mut different, mut refused, mut accepted) = (Vec::new(), [0usize; 3], 0usize);
    for (name, json) in &inputs {
        let want = ast_oracle::ingested(json);
        if let Some(rule) = want.strip_prefix("ERR ") {
            match rule.split(' ').next() {
                Some("ingest-json") => refused[0] += 1,
                Some("ingest-node") => refused[1] += 1,
                Some("ingest-arity") => refused[2] += 1,
                _ => {}
            }
        } else {
            accepted += 1;
        }
        if answer(&exe, json) != want {
            different.push(name.clone());
        }
    }
    assert!(
        different.is_empty(),
        "the ingester in cancho and the Rust one disagree about {} of {} inputs:\n{}",
        different.len(),
        inputs.len(),
        different.join("\n")
    );
    // The comparison is only worth something if it covers every rule and the programs that are let through.
    assert!(refused.iter().all(|n| *n >= 20), "refusals by rule (json, node, arity): {refused:?}");
    assert!(accepted > 300, "only {accepted} inputs were accepted");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// Programs for the JSON printer that the repository does not hold: the shapes of the vocabulary that the corpus has few of.
const JSON_EDGE: &[&str] = &[
    "",
    "import std.io;",
    "import std.io as console;\nimport a.b.c as d;\nimport x.y as y;",
    "module m.n;",
    "module m.n;\nimport std.io;\nfn f() -> [] int { return 0; }",
    "module m;\nfn f() -> [] int { return 0; }\nmodule n;\nimport q;\nfn g() -> [] int { return 1; }",
    "fn f[T, U: val, &r, &s where r <= s, s <= r](a: &r T) -> [io, ffi(\"libc\")] U { return a; }",
    "pub extern fn getpid[&f](ffi: &f Ffi(\"libc\"), x: fn(int, [byte]) -> [io] (int, int)) -> [ffi(\"libc\")] int;",
    "pub val struct P[T, U: val] { x: T, y: (U, int), z: [byte], w: &r Q[T, Ffi(\"a\")] }\nres struct R { }\nstruct S { a: int }",
    "pub enum E[T] { A, B(int, T), C(&r [byte]) }\nval enum F { X }\nres enum G { Y(int) }",
    "pub static N: int { return 1; }\nstatic M: [byte] { return \"a\\n\\t\\r\\0\\\\\\\"\\x\"; }",
    "fn f() -> [] int { let (a, b) = g(); let S { x, y } = h(); let q.S { x } = h(); let T { } = h(); return a; }",
    "fn f() -> [] int { var x: int = 1; let y: (int, [byte]) = (1, \"s\"); x = y.0; a[1] = 2; a[1..2] = b; return x; }",
    "fn f() -> [] int { match e { _ => { } E::A => { } q.E::B(a, _) => { } E::C() => { } } return 0; }",
    "fn f(h: &!r Heap) -> [heap] int { borrow mut h as &!r2 in { borrow x as &r3 in { } } region r { let p = alloc[r](1); let q = alloc_slice[r](4, 0); } defer g(); return 0; }",
    "fn f() -> [] int { if a { } if b { x(); } else { y(); } if c { } else if d { } else { } while e { f(); } return -1 + !a * ~b + *c; }",
    "fn f() -> [] bool { return a && b || c == d != e < f <= g > h >= i | j ^ k & l << m >> n + o - p * q / r % s; }",
    "fn f() -> [] int { return q.g(a.B::C(1), E::D, s.T { x: 1, y: 2 }, q.U { }, 9223372036854775807, 0, true, false); }",
    "fn f() -> [] int { return 'a' + '\\n' + '\\'' + \"\\u\".len; }",
    "fn f() -> [] float { return 0.0 + 1.0 + 1e308 + 5e-324 + 2.2250738585072014e-308 + 1.7976931348623157e308 + 123456789.123456789; }",
    "fn f() -> [] f32 { return 0.0f32 + 1f32 + 1e-45f32 + 1.1754942e-38f32 + 1.1754944e-38f32 + 3.4028235e38f32 + 16777216f32; }",
];

/// Programs for the formatter that the repository does not hold: a comment in each place a comment can stand, blank lines, the
/// spellings of literals, the edition marker, `pub extern`, an `import` out of place, and what is empty.
const FORMAT_EDGE: &[&str] = &[
    "// header\n\n// second\nimport std.io as console;   // trailing import\n\n// doc for f\nfn f(a: int) -> [] int {   // after brace\n    // leading\n    let x = 'a' + 0x10 + (a);  // trailing x\n\n\n    let y = 1.50;\n    // dangling at end\n    return x;\n}\n// end of file\n",
    "fn f() -> [] int {\n    // keep\n    return 0; // me\n}\n",
    "fn f() -> [] int {\n    return g(1, // one\n        2);\n}\n",
    "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else if a == 2 {\n        return 2;\n    } else {\n        return 3;\n    }\n}\n",
    "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else {\n        if a == 2 {\n            return 2;\n        }\n    }\n    return 3;\n}\n",
    "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else {\n        if a == 2 {\n            return 2;\n        }\n        return 4;\n    }\n}\n",
    "edition 2;\n\nfn f() -> [] int {\n    return 0;\n}\n",
    "edition 2; // the edition\nfn f() -> [] int { return 0; }\n",
    "fn f() -> [] int {\n\n    let x = 1;\n\n    return x;\n\n}\n",
    "pub extern fn getpid[&f](ffi: &f Ffi(\"libc\")) -> [ffi(\"libc\")] int;\n",
    "fn f() -> [] int {\n    return 0;\n}\n\nimport std.io;\n",
    "fn f() -> [] int {\n    return 0;\n}\nimport a.b;\nimport c.d;\n",
    "",
    "// only a comment   \n\n\n",
    "fn f() -> [] int {   \r\n    return 0; // c  \r\n}\r\n",
    "fn f() -> [] int {\n    let s = \"// not a comment\";\n    return 0;\n}\n",
    "struct P {\n    // the x\n    x: int, // x end\n    y: int,\n}\n\nenum E {\n    A, // first\n    // second\n    B(int),\n}\n\nfn f(e: E) -> [] int {\n    match e {\n        // arm a\n        E::A => {\n            return 0; // zero\n        }\n        E::B(n) => {\n            return n;\n        }\n    }\n}\n",
    "fn f() -> [] int { return 1_000 + 0x_ff + 0b1010 + 'a' + '\\n' + 007 + (1) + ((2)); }",
    "fn f() -> [] float { return 1.50 + 1e5 + 0.1; }",
    "fn f() -> [] int { return g(1, 2,) + h(); } // done",
    "fn f() -> [] int { /* not a comment in this language */ return 0; }",
    "// a\n// b\n\n\n// c\nfn f() -> [] int { return 0; }\n// d\n\n// e",
    "module m;\nimport a;\n// between\nimport b; // after\nfn f() -> [] int { return 0; }\n",
    "fn f() -> [] int {\n    {\n        // in a block\n    }\n    return 0;\n}\n",
    "fn f(\n    a: int, // a\n    b: int, // b\n) -> [] int {\n    return a + b;\n}\n",
];

/// Ways of spoiling the layout of a program that leave it the same program: no indentation, CRLF, white space at the ends of lines,
/// blank lines doubled, lines blanked, everything on one line, and comments added at the ends of lines and between them.
fn spoil(text: &str, how: usize, seed: &mut u64) -> String {
    let mut next = |bound: u64| {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed % bound
    };
    let lines: Vec<&str> = text.split('\n').collect();
    match how {
        0 => lines.iter().map(|l| l.trim_start()).collect::<Vec<_>>().join("\n"),
        1 => lines.join("\r\n"),
        2 => lines
            .iter()
            .map(|l| if l.trim().is_empty() { (*l).to_owned() } else { format!("{l}  ") })
            .collect::<Vec<_>>()
            .join("\n"),
        3 => lines.join("\n\n"),
        4 => {
            let mut out = Vec::new();
            for l in &lines {
                let r = next(100);
                if r < 8 {
                    out.push(format!("    // inserted {}", next(100)));
                }
                if r > 92 && !l.trim().is_empty() && !l.contains("//") {
                    out.push(format!("{l} // end {}", next(100)));
                } else {
                    out.push((*l).to_owned());
                }
            }
            out.join("\n")
        }
        5 => lines
            .iter()
            .filter(|l| !l.trim_start().starts_with("//"))
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .replace(';', ";\n"),
        _ => {
            lines.iter().map(|l| if next(10) == 0 { "" } else { *l }).collect::<Vec<_>>().join("\n")
        }
    }
}

/// `examples/selfhost/formatter.cho` against `cancho_syntax::format`: the repository's programs, the same programs with their layout
/// spoilt two of seven ways each, and `FORMAT_EDGE`. The formatter is given the Rust printer's text, so what is compared is the formatter alone.
#[test]
fn the_formatter_in_cancho_agrees_with_the_rust_formatter() {
    let exe = build("formatter", &with_front_end("formatter.cho"));
    let mut programs: Vec<(String, String)> = sources();
    programs.extend(FORMAT_EDGE.iter().map(|s| (format!("edge case {s:.40?}"), (*s).to_owned())));
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut spoilt = Vec::new();
    for (n, (name, text)) in programs.iter().enumerate() {
        // The programs the repository must refuse are not spoilt: they do not parse, and the printer's text is the one refusal line.
        if ast_oracle::printed(text).starts_with("ERR") {
            continue;
        }
        // Two ways for each program, a different two for the next, so that all seven are used often and the test stays short.
        for how in [n % 7, (n + 3) % 7] {
            spoilt.push((format!("{name} spoilt {how}"), spoil(text, how, &mut seed)));
        }
    }
    programs.extend(spoilt);
    let (mut different, mut formatted, mut refused, mut unchanged) = (Vec::new(), 0, 0, 0);
    for (name, text) in &programs {
        let printed = ast_oracle::printed(text);
        if printed.starts_with("ERR") {
            continue;
        }
        let input = format!("FILE {}\n{text}FILE {}\n{printed}", text.len(), printed.len());
        let want = ast_oracle::formatted(text);
        if want.starts_with("CANNOT") {
            refused += 1;
        } else if want == *text {
            unchanged += 1;
        } else {
            formatted += 1;
        }
        if answer(&exe, &input) != want {
            different.push(name.clone());
        }
    }
    assert!(
        different.is_empty(),
        "the formatter in cancho and the Rust formatter disagree about {} of {} programs:\n{}",
        different.len(),
        programs.len(),
        different.join("\n")
    );
    // The comparison is only worth something if it covers files that change, and files that are refused.
    eprintln!(
        "formatted {formatted}, refused {refused}, unchanged {unchanged}, of {}",
        programs.len()
    );
    assert!(formatted > 400, "only {formatted} programs were changed by the formatter");
    assert!(refused >= 10, "only {refused} programs were refused by the formatter");
    assert!(unchanged > 300, "only {unchanged} programs were already formatted");
    let _ = std::fs::remove_dir_all(exe.parent().expect("a scratch directory"));
}

/// The first line of the input of the compiler in cancho: the system, and the architecture where Linux has two.
pub(crate) fn target_line() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_arch = "aarch64") {
        "linux aarch64"
    } else {
        "linux"
    }
}
