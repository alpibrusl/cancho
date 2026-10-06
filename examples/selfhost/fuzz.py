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
import argparse, concurrent.futures, glob, os, random, re, subprocess, sys, tempfile

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


def run(cmd, data):
    p = subprocess.run(cmd, input=data, capture_output=True, timeout=60)
    return p.returncode, p.stdout, p.stderr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("oracle")
    ap.add_argument("port")
    ap.add_argument("files", nargs="*")
    ap.add_argument("--count", type=int, default=3000)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--keep", help="write each differing input into this directory")
    args = ap.parse_args()

    paths = args.files or sorted(glob.glob("**/*.ls", recursive=True))
    paths = [p for p in paths if "/target/" not in p and not p.startswith("target/")]
    corpus = build_corpus(paths)
    rng = random.Random(args.seed)
    cases = [(f"edge:{k}", v if isinstance(v, bytes) else v.encode()) for k, v in EDGE.items()]
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
        a, b = run([args.oracle], data), run([args.port], data)
        return name, data, a, b

    same = refused = bad = skipped = 0
    shown = 0
    if args.keep:
        os.makedirs(args.keep, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for n, (name, data, a, b) in enumerate(pool.map(one, cases)):
            if a is None:
                skipped += 1
                continue
            if a[1] == b[1]:
                same += 1
                refused += a[1].startswith(b"ERR ")
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
    print(f"cases: {len(cases)}  identical: {same} (of which refusals: {refused})  different: {bad}  "
          f"not comparable (invalid UTF-8): {skipped}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
