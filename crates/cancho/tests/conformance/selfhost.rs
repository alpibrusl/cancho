//! `examples/selfhost/` (`docs/self-hosting.md`): the compiler written in cancho, against the Rust one it was ported from.
//!
//! The ports of the lexer, the parser, the printer, the JSON printer, the identities, BLAKE3, the ingester, the formatter and the checkers were compared here with the Rust
//! front end's crates over the programs of the repository. Those tests are `tests/commands/front_end_syntax.sh`, `front_end_names.sh`, `front_end_checks.sh`, `parity.sh` and
//! `regen.sh` now, replaying the answers the Rust crates gave once (`tests/vectors/frontend/`, `docs/gate.md` G4 wave 5). What stays here are the command-line tests.

use super::*;

mod authority;
mod bootstrap;
mod cli;
mod compile;
mod messages;
mod origin;
mod store;
mod vcs;

#[allow(dead_code)]
#[path = "../../../cancho-syntax/examples/dump_ast.rs"]
mod ast_oracle;

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

fn sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["std", "examples", "packages", "tests/accept", "tests/reject", "tests/programs"] {
        super::walk_ls_files(&root.join(dir), &mut files);
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

/// What the Rust command line says it was built from: the crate version, the commit and the host triple of `cancho --version`.
pub(crate) fn rust_stamp() -> (String, String, String) {
    let said = Command::new(BIN).arg("--version").output().expect("the compiler runs");
    let line = String::from_utf8_lossy(&said.stdout).trim().to_owned();
    let inner = line.strip_prefix("cancho ").unwrap_or_else(|| panic!("a version line: {line}"));
    let (version, rest) =
        inner.split_once(" (rev ").unwrap_or_else(|| panic!("a version line: {line}"));
    let (rev, rest) =
        rest.split_once(", host ").unwrap_or_else(|| panic!("a version line: {line}"));
    (version.to_owned(), rev.to_owned(), rest.trim_end_matches(')').to_owned())
}

/// `version.cho` stamped (`scripts/stamp-version.sh`) with `rev` and `host`, written into a scratch directory of its own; the path.
pub(crate) fn stamped_version(tag: &str, rev: &str, host: &str) -> PathBuf {
    let dir = scratch(&format!("selfhost-stamp-{tag}"));
    let out = dir.join("version.cho");
    let done = Command::new("sh")
        .arg(repo_root().join("scripts/stamp-version.sh"))
        .args([rev, host])
        .env("VERSION_CHO", &out)
        .output()
        .expect("the stamp script runs");
    assert!(
        done.status.success(),
        "stamping {rev} {host}: {}",
        String::from_utf8_lossy(&done.stderr)
    );
    out
}

/// A program built from `root`'s front end with `version.cho` replaced by the stamped copy at `version`.
pub(crate) fn build_stamped(tag: &str, root: &'static str, version: &Path) -> PathBuf {
    let files: Vec<String> = with_front_end(root)
        .into_iter()
        .map(|f| if f == "version.cho" { version.display().to_string() } else { f.to_owned() })
        .collect();
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    build(tag, &refs)
}

/// The files of a front-end program: the root, which says what to print, then the modules of the
/// command line in cancho, which are `examples/selfhost/cancho.files` less `cli.cho` (the one
/// root that list names itself). The list is read, not repeated here: `cancho.files` is the only
/// list of what the front end is, and `the_file_list_of_the_standalone_build_is_the_import_closure_of_cli`
/// keeps it equal to the import closure.
pub(crate) fn with_front_end(root: &'static str) -> Vec<&'static str> {
    let list = repo_root().join("examples/selfhost/cancho.files");
    let text = std::fs::read_to_string(&list).expect("examples/selfhost/cancho.files is read");
    // Leaked once per call: the callers hold `&'static str`, and a test process is short.
    let text: &'static str = Box::leak(text.into_boxed_str());
    std::iter::once(root)
        .chain(text.lines().filter(|f| !f.is_empty() && *f != "cli.cho" && *f != root))
        .collect()
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
