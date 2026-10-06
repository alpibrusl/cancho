#!/usr/bin/env python3
"""Hammer the parser port (`parser.ls`) against the Rust parser it was ported from.

    fuzz.py ORACLE PORT [--count N] [--seed S] [--keep DIR] [files...]

ORACLE is `dump_ast` (`crates/lex-sys-syntax/examples/dump_ast.rs`), PORT is `parser.ls`
built; both read a source file on standard input and must answer with the same bytes:
the same listing, or the same `ERR rule start end`. The corpus is the files named (by
default every `.ls` file under the repository), a set of hand-written edge cases for what
the repository does not contain -- the limits of integers and floats, escapes, the order
of module, import and item -- and `--count` mutants of the files: a byte range deleted,
a token duplicated, dropped or replaced by another one, the file cut short. The mutants
are where the refusals are: each one is a program broken in a place the Rust parser has
to name, and the port has to name the same one.
"""
import argparse, collections, concurrent.futures, glob, os, random, re, subprocess, sys, tempfile

TOKEN = re.compile(rb"[A-Za-z_][A-Za-z_0-9]*|\d[\w.]*|'(?:\\.|[^'\\])'|\"(?:\\.|[^\"\\])*\"|::|->|=>|\.\.|<=|>=|==|!=|&&|\|\||<<|>>|\S")

POOL = [b"fn", b"let", b"var", b"if", b"else", b"while", b"return", b"match", b"struct", b"enum",
        b"res", b"val", b"extern", b"pub", b"borrow", b"mut", b"region", b"in", b"as", b"where",
        b"(", b")", b"{", b"}", b"[", b"]", b",", b".", b";", b":", b"::", b"..", b"->", b"=>",
        b"_", b"=", b"==", b"!=", b"<", b"<=", b">", b">=", b"+", b"-", b"*", b"/", b"%", b"!",
        b"&", b"&&", b"|", b"||", b"^", b"~", b"<<", b">>", b"x", b"T", b"1", b"0x", b"1.5",
        b"\"s\"", b"'a'", b"module", b"import", b"edition", b"static", b"alloc", b"alloc_slice",
        b"module a.b;", b"import std.io as io;", b"edition 3;", b"[T: val]", b"[&r]", b"&r", b"&!r",
        b"1_000", b"0xff", b"2f32", b"1.5f32", b"9223372036854775808", b"1e999"]

F64_MAX = "1.7976931348623157e308"
EDGE = {
    "ints": "fn f() -> [] int { return 9223372036854775807 + 0x7fffffffffffffff + 0x7fff_ffff_ffff_ffff + 0_0_7; }",
    "int_min": "fn f() -> [] int { return -9223372036854775808; }",
    "int_min_hex": "fn f() -> [] int { return -0x8000000000000000; }",
    "int_over": "fn f() -> [] int { return 9223372036854775808; }",
    "int_over_hex": "fn f() -> [] int { return 0x8000000000000000; }",
    "int_over_u64": "fn f() -> [] int { return 18446744073709551616; }",
    "int_over_neg": "fn f() -> [] int { return -9223372036854775809; }",
    "int_far": "fn f() -> [] int { return 99999999999999999999999999999999; }",
    "int_hex_far": "fn f() -> [] int { return 0xffffffffffffffffffff; }",
    "chars": "fn f() -> [] int { return 'a' + '\\n' + '\\0' + '\\'' + '\\\\' + -'x'; }",
    "tuple_field": "fn f() -> [] int { return t.0.1.4294967295; }",
    "tuple_field_over": "fn f() -> [] int { return t.4294967296; }",
    "tuple_field_hex": "fn f() -> [] int { return t.0x1; }",
    "f64_max": f"fn f() -> [] float {{ return {F64_MAX}; }}",
    "f64_over": "fn f() -> [] float { return 1.7976931348623159e308; }",
    "f64_tie": "fn f() -> [] float { return 1.797693134862315807937289714053034150799341327100378269361737789804449682927647509466490179775872070963302871479068578727958880e308; }",
    "f64_below_tie": "fn f() -> [] float { return 1.7976931348623158079372897140530341507993413271003782693617377898044496829276475094664901797758720709633028714790685787279588e308; }",
    "f64_e309": "fn f() -> [] float { return 1e309; }",
    "f64_e308": "fn f() -> [] float { return 9e308; }",
    "f64_zeros": "fn f() -> [] float { return 0.000000e999 + 0e99999999999999999999; }",
    "f64_underflow": "fn f() -> [] float { return 1e-400 + 0.0000001e-99999999999999999999; }",
    "f64_leading": "fn f() -> [] float { return 000.00001797693134862315807937289714053034150799341327100378269361737789804449682927647509466490179775872070963302871479068578727958e313; }",
    "f64_underscores": "fn f() -> [] float { return 1_7.9_7e3_0_7; }",
    "f64_huge_exponent": "fn f() -> [] float { return 1e99999999999999999999999; }",
    "f32_max": "fn f() -> [] f32 { return 3.4028234663852886e38f32; }",
    "f32_over": "fn f() -> [] f32 { return 3.4028235677973366e38f32; }",
    "f32_below": "fn f() -> [] f32 { return 3.4028235677973365e38f32; }",
    "f32_e39": "fn f() -> [] f32 { return 1e39f32; }",
    "f32_neg_over": "fn f() -> [] f32 { return -1e39f32; }",
    "f32_zero": "fn f() -> [] f32 { return 0e99f32; }",
    "f32_int": "fn f() -> [] f32 { return 2f32; }",
    "strings": 'fn f() -> [] int { g("a\\nb\\tc\\rd\\0e\\\\f\\"g", "", "é日"); }',
    "string_escape_bad": 'fn f() -> [] int { g("a\\qb"); }',
    "string_open": 'fn f() -> [] int { g("abc); }',
    "effects": 'fn f() -> [io_write, ffi("libc"), fs_read("x\\n")] int { return 0; }',
    "edition_ok": "edition 7;\nfn f() -> [] int { return 1; }",
    "edition_8": "edition 8;\nfn f() -> [] int { return 1; }",
    "edition_0": "edition 0;\nfn f() -> [] int { return 1; }",
    "edition_big": "edition 99999999999999999999;\nfn f() -> [] int { return 1; }",
    "edition_word": "edition x;\nfn f() -> [] int { return 1; }",
    "edition_twice": "edition 2;\nedition 3;",
    "module_ok": "module a.b.c;\nimport std.io;\nimport std.buffer as b;\nfn f() -> [] int { return 1; }",
    "module_twice": "module a;\nmodule b;",
    "module_late": "fn f() -> [] int { return 1; }\nmodule a;",
    "module_after_import": "import x.y;\nmodule a;",
    "imports_mixed": "import a.b;\nfn f() -> [] int { return 1; }\nimport c as d;\nimport e.f.g as h;",
    "module_imports": "module m.n;\nimport a;\nfn f() -> [] int { return 1; }\nimport b.c;",
    "empty": "",
    "blank": "\n\n  // nothing\n",
    "only_pub": "pub",
    "pub_static": "pub static t: [int] { return 1; }\nstatic u: int { return 2; }",
    "res_struct": "pub res struct S[T] { a: T, }\nval enum E[T: val] { A(T), B, }\nres enum F { X }",
    "val_bound": "val struct S[T: val] { a: T }",
    "res_bound": "struct S[T: res] { a: T }",
    "bound_other": "struct S[T: int] { a: T }",
    "struct_region": "struct S[&r] { a: int }",
    "enum_region": "enum E[&r] { A }",
    "extern_generic": "extern fn g[T](x: T) -> [] int;",
    "extern_ok": "extern fn g[&r](x: &r [byte], n: int) -> [ffi(\"libc\")] int;",
    "where_clause": "fn f[&a, &b where a <= b, b <= c](x: int) -> [] int { return 1; }",
    "where_bad": "fn f[&a where a < b](x: int) -> [] int { return 1; }",
    "generics_mixed": "fn f[T, &r, U: val, &q where r <= q](x: T) -> [] int { return 1; }",
    "types": "fn f(a: fn(int, &r [byte]) -> [io_write] (int, bool), b: Ffi(\"libc\"), c: io.Buffer[int, (int, int)], d: &!r [(int, int)]) -> [] int { return 1; }",
    "type_lit_args": "fn f(a: Ffi(\"libc\")) -> [] int { return 1; }",
    "struct_lit": "fn f() -> [] int { let p = P { x: 1, y: Q { z: 2 }, }; let q = m.P { x: 1 }; return 0; }",
    "struct_lit_cond": "fn f() -> [] int { if x == P { a: 1 } { return 1; } return 0; }",
    "struct_lit_cond_paren": "fn f() -> [] int { if x == (P { a: 1 }) { return 1; } while a[P { a: 1 }.a] < 1 { } return 0; }",
    "qualified_value": "fn f() -> [] int { return m.x; }",
    "qualified_call": "fn f() -> [] int { return m.g(1) + m.E::V(2) + m.E::W; }",
    "variant": "fn f() -> [] int { return E::A + E::B(1, 2) + E::C(); }",
    "index_slice": "fn f() -> [] int { return a[1] + a[1..2] + a[b[0]..c[1..2][0]]; }",
    "alloc": "fn f() -> [] int { let a = alloc[r](P { x: 1 }); let b = alloc_slice[r](10, 0); return 0; }",
    "alloc_bad": "fn f() -> [] int { let a = alloc(1); }",
    "tuples": "fn f() -> [] int { let t = (1, 2, (3, 4),); let (a, b) = t; return (1); }",
    "tuple_nested_pattern": "fn f() -> [] int { let (a, (b, c)) = t; }",
    "var_destructure": "fn f() -> [] int { var (a, b) = t; }",
    "var_destructure_struct": "fn f() -> [] int { var P { a } = t; }",
    "destructure": "fn f() -> [] int { let P { a, b, } = x; let m.P { c } = y; return 0; }",
    "borrow_ok": "fn f() -> [] int { borrow x as &r in { } borrow mut y as &!s in { } region a { } return 0; }",
    "borrow_mismatch": "fn f() -> [] int { borrow mut x as &r in { } }",
    "borrow_mismatch2": "fn f() -> [] int { borrow x as &!r in { } }",
    "match": "fn f() -> [] int { match x { E::A => { } E::B(a, _) => { return 1; }, m.E::C => { }, _ => { } } return 0; }",
    "match_bad": "fn f() -> [] int { match x { 1 => { } } }",
    "match_eof": "fn f() -> [] int { match x { E::A => { }",
    "block_eof": "fn f() -> [] int { while true {",
    "else_if": "fn f() -> [] int { if a { } else if b { } else if c { return 1; } else { } return 0; }",
    "if_no_else": "fn f() -> [] int { if a { } return 0; }",
    "ops": "fn f() -> [] int { return a || b && c == d != e < f <= g > h >= i | j ^ k & l << m >> n + o - p * q / r % s; }",
    "unary": "fn f() -> [] int { return !a + ~b + -c + *d - -1 - -1.5 - - - x; }",
    "unary_neg_lit": "fn f() -> [] int { return -1 - -0x10 - -'a' - -1.5f32; }",
    "assign": "fn f() -> [] int { x = 1; r.f = 2; a[0] = 3; *p = 4; t.0 = 5; return 0; }",
    "defer": "fn f() -> [] int { defer g(); return 0; }",
    "deep_parens": "fn f() -> [] int { return " + "(" * 400 + "1" + ")" * 400 + "; }",
    "deep_unary": "fn f() -> [] int { return " + "-" * 1000 + "x; }",
    "deep_blocks": "fn f() -> [] int { " + "if a { " * 300 + "}" * 300 + " return 0; }",
    "deep_types": "fn f(x: " + "[" * 300 + "int" + "]" * 300 + ") -> [] int { return 0; }",
    "many_args": "fn f() -> [] int { return g(" + ",".join(str(i) for i in range(2000)) + "); }",
    "long_name": "fn " + "a" * 5000 + "() -> [] int { return 0; }",
    "bad_start": "return 1;",
    "bad_item": "let x = 1;",
    "res_nothing": "res fn f() -> [] int { return 0; }",
    "lexer_error": "fn f() -> [] int { return 1 $ 2; }",
    "lexer_error_late": "fn f() -> [] int { return 0; }\n" * 50 + "fn g() -> [] int { return 1x; }",
    "non_utf8": b"fn f() -> [] int { return 0; } // \xff\xfe\n",
    "no_semi": "fn f() -> [] int { return 1 }",
    "no_arrow": "fn f() { return 1; }",
    "no_row": "fn f() -> int { return 1; }",
    "eof_in_params": "fn f(a: int,",
    "eof_in_generics": "fn f[T,",
    "eof_in_row": "fn f() -> [io_write,",
    "eof_in_args": "fn f() -> [] int { return g(1, 2",
    "eof_in_struct": "struct S { a: int,",
    "eof_in_enum": "enum E { A(int,",
    "comma_trailing": "fn f(a: int,) -> [io_write,] int { return g(1,); }\nstruct S { a: int, }\nenum E { A(int,), }",
}


def fnmain(body):
    return f"fn main(world: World) -> [] int {{ {body} return 0; }}"


CHECK_EDGE = {
    "ok": fnmain(""),
    "dup_struct": "struct A { a: int }\nstruct A { b: int }\n" + fnmain(""),
    "dup_struct_enum": "struct A { a: int }\nenum A { X }\n" + fnmain(""),
    "dup_in_module": "module m;\nstruct A { a: int }\nstruct A { b: int }",
    "redeclare_int": "struct int { a: int }",
    "redeclare_bool": "enum bool { A }",
    "redeclare_prelude": "struct World { a: int }",
    "redeclare_prelude_late": "edition 1;\nstruct Conn { a: int }\n" + fnmain(""),
    "redeclare_prelude_ed5": "edition 5;\nstruct Conn { a: int }\n" + fnmain(""),
    "redeclare_f32_ed6": "edition 6;\nstruct f32 { a: int }",
    "redeclare_f32_ed5": "edition 5;\nstruct f32 { a: int }",
    "generic_int": "struct S[int] { a: int }",
    "generic_dup": "struct S[T, T] { a: T }",
    "generic_ok": "struct S[T, U] { a: T, b: U }\n" + fnmain(""),
    "field_dup": "struct S { a: int, a: bool }",
    "field_unknown": "struct S { a: Nope }",
    "field_arity": "struct S { a: Box }",
    "field_arity2": "struct P[T] { a: T }\nstruct S { a: P[int, int] }",
    "field_arity0": "struct S { a: int[int] }",
    "field_param_args": "struct S[T] { a: T[int] }",
    "field_unsized": "struct S { a: [int] }",
    "field_ref_unscoped": "struct S { a: &r int }",
    "field_ref_static": "struct S { a: &static [byte] }",
    "field_ref_static_unique": "struct S { a: &!static [byte] }",
    "field_tuple_one": "struct S { a: (int) }",
    "field_tuple_empty": "struct S { a: () }",
    "field_tuple_trailing": "struct S { a: (int,) }",
    "field_fn": "struct S { a: fn(int) -> [] int }",
    "field_fn_bad": "struct S { a: fn(Nope) -> [] int }",
    "field_qualified": "struct S { a: x.T }",
    "field_qualified_import": "module m;\nimport m as x;\nstruct T { a: int }\nstruct S { a: x.T }",
    "field_qualified_private": "module m;\nimport m as x;\nstruct T { a: int }\nstruct S { a: x.T }",
    "field_box_slice": "struct S { a: Box[[int]] }",
    "field_vec_slice": "struct P[T] { a: T }\nstruct S { a: P[[int]] }",
    "enum_empty": "enum E { }",
    "enum_dup_variant": "enum E { A, A }",
    "enum_payload_bad": "enum E { A(Nope) }",
    "enum_payload_arity": "enum E { A(Box, int) }",
    "infinite_self": "struct S { a: S }",
    "infinite_enum": "enum E { A(E) }",
    "infinite_mutual": "struct A { b: B }\nstruct B { a: A }",
    "infinite_box": "struct S { a: Box[S] }\n" + fnmain(""),
    "infinite_tuple": "struct S { a: (int, S) }",
    "infinite_generic": "struct W[T] { a: T }\nstruct S { a: W[S] }",
    "infinite_ref": "struct S[&r] { a: int }",
    "val_decl": "val struct S { a: int }",
    "res_decl": "res struct S { a: int }",
    "bound_val": "struct S[T: val] { a: T }\nstruct U { a: S[int] }",
    "bound_val_res": "struct S[T: val] { a: T }\nstruct U { a: S[Box[int]] }",
    "extern_decl": "extern fn g(x: int) -> [] int;",
    "static_ok": "static t: [int] { return 1; }\n" + fnmain(""),
    "static_dup": "static t: [int] { return 1; }\nstatic t: [int] { return 2; }",
    "static_fn_clash": "static t: [int] { return 1; }\nfn t() -> [] int { return 1; }",
    "static_fn_clash_before": "fn t() -> [] int { return 1; }\nstatic t: [int] { return 1; }",
    "static_scalar": "static t: int { return 1; }",
    "static_f32": "static t: [f32] { return 1; }",
    "static_struct": "static t: [S] { return 1; }\nstruct S { a: int }",
    "static_unknown": "static t: [Nope] { return 1; }",
    "static_ref": "static t: &static [int] { return 1; }",
    "static_bool_float_byte": "static a: [bool] { return 1; }\nstatic b: [float] { return 1; }\nstatic c: [byte] { return 1; }",
    "fn_dup": "fn f() -> [] int { return 1; }\nfn f() -> [] int { return 2; }",
    "fn_dup_module": "module m;\nfn f() -> [] int { return 1; }\nfn f() -> [] int { return 2; }",
    "fn_builtin": "fn getchar() -> [] int { return 1; }",
    "fn_builtin_edition": "edition 1;\nfn connect() -> [] int { return 1; }",
    "fn_builtin_edition2": "edition 2;\nfn connect() -> [] int { return 1; }",
    "fn_generic_int": "fn f[int]() -> [] int { return 1; }",
    "fn_generic_dup": "fn f[T, T]() -> [] int { return 1; }",
    "fn_region_dup": "fn f[&r, &r]() -> [] int { return 1; }",
    "fn_region_generic": "fn f[T, &T]() -> [] int { return 1; }",
    "fn_region_static": "fn f[&static]() -> [] int { return 1; }",
    "fn_where_ok": "fn f[&a, &b where a <= b](x: &a int, y: &b int) -> [] int { return 1; }\n" + fnmain(""),
    "fn_where_missing": "fn f[&a where a <= b]() -> [] int { return 1; }",
    "fn_where_missing_inner": "fn f[&b where a <= b]() -> [] int { return 1; }",
    "fn_where_generic": "fn f[T, &a where a <= T]() -> [] int { return 1; }",
    "fn_param_dup": "fn f(a: int, a: int) -> [] int { return 1; }",
    "fn_param_dup_late": "fn f(a: int, b: Nope, a: int) -> [] int { return 1; }",
    "fn_param_unknown": "fn f(a: Nope) -> [] int { return 1; }",
    "fn_ret_unknown": "fn f() -> [] Nope { return 1; }",
    "fn_param_unscoped_region": "fn f(a: &r int) -> [] int { return 1; }",
    "fn_param_scoped_region": "fn f[&r](a: &r int) -> [] int { return 1; }\n" + fnmain(""),
    "fn_param_unsized": "fn f(a: [int]) -> [] int { return 1; }",
    "fn_param_slice_ref": "fn f[&r](a: &r [int], b: &!r [int]) -> [] int { return 1; }\n" + fnmain(""),
    "fn_param_generic_args": "fn f[T](a: T[int]) -> [] int { return 1; }",
    "fn_pub_private": "struct S { a: int }\npub fn f(a: S) -> [] int { return 1; }",
    "fn_pub_private_ret": "struct S { a: int }\npub fn f() -> [] S { return S { a: 1 }; }",
    "fn_pub_private_nested": "struct S { a: int }\npub fn f(a: Box[S]) -> [] int { return 1; }",
    "fn_pub_private_ref": "struct S { a: int }\npub fn f[&r](a: &r S) -> [] int { return 1; }",
    "fn_pub_private_tuple": "struct S { a: int }\npub fn f(a: (int, S)) -> [] int { return 1; }",
    "fn_pub_private_fn": "struct S { a: int }\npub fn f(a: fn(S) -> [] int) -> [] int { return 1; }",
    "fn_pub_public": "pub struct S { a: int }\npub fn f(a: S) -> [] int { return 1; }\n" + fnmain(""),
    "fn_private_private": "struct S { a: int }\nfn f(a: S) -> [] int { return 1; }\n" + fnmain(""),
    "import_unknown": "import nothing.here;\n" + fnmain(""),
    "import_self": "module a.b;\nimport a.b;\n" + fnmain(""),
    "import_self_alias": "module a.b;\nimport a.b as x;\nstruct T { a: int }\nstruct S { a: x.T }",
    "import_dup_alias": "module a.b;\nimport a.b;\nimport a.b as b;",
    "import_dup_alias2": "module a.b;\nimport a.b as x;\nimport a.b as x;",
    "import_root_self": "import a;",
    "import_late": "fn f() -> [] int { return 1; }\nimport zzz;",
    "std_import": "import std.io;\n" + fnmain(""),
    "prelude_names": "fn f(a: World, b: Io, c: Heap, d: Box[int], e: Split, f: Args, g: Fs) -> [] int { return 1; }",
    "prelude_ed2": "edition 2;\nfn f(a: Net) -> [] int { return 1; }",
    "prelude_ed1": "edition 1;\nfn f(a: Net) -> [] int { return 1; }",
    "prelude_ed7": "edition 7;\nfn f(a: Exec, b: Child) -> [] int { return 1; }",
    "prelude_split_ed": "edition 2;\nfn f(a: Split) -> [] int { return 1; }",
    "ffi_type": "fn f(a: &r Ffi(\"libc\")) -> [] int { return 1; }",
    "type_order_late": "struct A { b: B }\nstruct B { a: int }\n" + fnmain(""),
    "first_error_wins": "struct A { a: Nope }\nstruct A { b: int }",
    "first_error_wins2": "struct A { b: int }\nstruct A { b: int }\nstruct C { a: Nope }",
    "member_before_infinite": "struct A { a: A, b: Nope }",
    "float_types": "struct S { a: float, b: byte, c: bool, d: int }\n" + fnmain(""),
    "f32_ed5": "edition 5;\nstruct S { a: f32 }",
    "f32_ed6": "edition 6;\nstruct S { a: f32 }\n" + fnmain(""),
    "byte_redeclared": "struct byte { a: int }\nstruct S { a: byte }\n" + fnmain(""),
}


def build_corpus(paths):
    files = []
    for p in paths:
        with open(p, "rb") as f:
            files.append((p, f.read()))
    return files


def mutate(data: bytes, rng: random.Random) -> bytes:
    tokens = [m.span() for m in TOKEN.finditer(data)]
    if not tokens:
        return data
    kind = rng.randrange(7)
    a, b = tokens[rng.randrange(len(tokens))]
    if kind == 0:  # drop a token
        return data[:a] + data[b:]
    if kind == 1:  # duplicate a token
        return data[:b] + b" " + data[a:b] + data[b:]
    if kind == 2:  # replace a token
        return data[:a] + POOL[rng.randrange(len(POOL))] + data[b:]
    if kind == 3:  # insert a token
        return data[:a] + POOL[rng.randrange(len(POOL))] + b" " + data[a:]
    if kind == 4:  # cut the file short
        return data[:a]
    if kind == 5:  # delete a byte range
        c = min(len(data), a + 1 + rng.randrange(40))
        return data[:a] + data[c:]
    c, d = tokens[rng.randrange(len(tokens))]  # swap two tokens
    (a, b), (c, d) = sorted([(a, b), (c, d)])
    if b > c:
        return data
    return data[:a] + data[c:d] + data[b:c] + data[a:b] + data[d:]


STD = None


def with_library(data: bytes) -> bytes:
    """The case's file and then every file of `std/`, as `FILE <length>` records."""
    global STD
    if STD is None:
        STD = [open(f, "rb").read() for f in sorted(glob.glob("std/*.ls"))]
    out = b""
    for part in [data] + STD:
        out += b"FILE %d\n" % len(part) + part
    return out


def run(cmd, data):
    p = subprocess.run(cmd, input=data, capture_output=True, timeout=120)
    return p.returncode, p.stdout, p.stderr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("oracle")
    ap.add_argument("port")
    ap.add_argument("files", nargs="*")
    ap.add_argument("--count", type=int, default=3000)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--keep", help="write each differing input into this directory")
    ap.add_argument("--std", action="store_true",
                    help="with --checker: parse each case with the standard library, as `check --std` does; the "
                    "oracle and the port are given a stream of files (see `driver.ls`), and the port is `check_files`")
    ap.add_argument("--checker", action="store_true",
                    help="compare check.ls with check_declarations: the pass-1 edge cases, and a SKIP answer is not compared")
    args = ap.parse_args()

    paths = args.files or sorted(glob.glob("**/*.ls", recursive=True))
    paths = [p for p in paths if "/target/" not in p and not p.startswith("target/")]
    corpus = build_corpus(paths)
    if args.checker and not args.std:
        # A program that imports `std` stops at its first import (the port reads one file, and `std`
        # is a set of others), so its mutants never reach the checks this is for.
        corpus = [(p, d) for p, d in corpus if b"import std" not in d]
    rng = random.Random(args.seed)
    edge = CHECK_EDGE if args.checker else EDGE
    cases = [(f"edge:{k}", v if isinstance(v, bytes) else v.encode()) for k, v in edge.items()]
    cases += [(f"file:{p}", d) for p, d in corpus]
    for _ in range(args.count):
        p, d = corpus[rng.randrange(len(corpus))]
        cases.append((f"mutant:{p}", mutate(d, rng)))

    def one(case):
        name, data = case
        try:
            data.decode()
        except UnicodeDecodeError:
            # `parse` takes a `&str`, so the oracle lossily decodes the bytes it is given and
            # reports spans in the decoded text; the port works on the bytes. The two cannot
            # be compared, and they are the only cases that differ.
            return name, data, None, None
        if args.std:
            payload = with_library(data)
            a, b = run([args.oracle, "--files"], payload), run([args.port], payload)
        else:
            a, b = run([args.oracle], data), run([args.port], data)
        return name, data, a, b

    same = refused = bad = skipped = skipped_port = 0
    by_rule = collections.Counter()
    shown = 0
    if args.keep:
        os.makedirs(args.keep, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for n, (name, data, a, b) in enumerate(pool.map(one, cases)):
            if a is None:
                skipped += 1
                continue
            if args.checker and b[1].strip() == b"SKIP":
                skipped_port += 1
                continue
            if a[1] == b[1]:
                same += 1
                refused += a[1].startswith(b"ERR ")
                by_rule[a[1].split()[1].decode() if a[1].startswith(b"ERR ") else "OK"] += 1
                continue
            bad += 1
            if args.keep:
                with open(os.path.join(args.keep, f"{n}.ls"), "wb") as f:
                    f.write(data)
            if shown < 6:
                shown += 1
                la, lb = a[1].splitlines(), b[1].splitlines()
                at = next((i for i, (x, y) in enumerate(zip(la, lb)) if x != y), min(len(la), len(lb)))
                print(f"\nDIFFERENT {name} ({len(data)} bytes) at output line {at}\n"
                      f"  oracle: {la[at] if at < len(la) else '<end>'!r}\n"
                      f"  port:   {lb[at] if at < len(lb) else '<end>'!r}\n  stderr: {b[2][:200]!r}")
    if args.checker:
        print("identical by answer:", dict(by_rule.most_common()))
    print(f"cases: {len(cases)}  identical: {same} (of which refusals: {refused})  different: {bad}  "
          f"not comparable (invalid UTF-8): {skipped}  not ported (SKIP): {skipped_port}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
