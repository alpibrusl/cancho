//! Golden hashes — what the encoder actually emits, written down.
//!
//! # Why this file exists
//!
//! The 61 tests beside the encoder are all *relational*: renaming a local
//! does not change the body, formatting does not reach the hash, an index is
//! not a range. Every one of them compares two hashes to each other. Not one
//! of them says what a hash **is**.
//!
//! That left `docs/canonical-ast.md` §8's central claim unobservable. §8 says
//! tag values are *"stable within a build and not yet frozen across
//! releases"* and that *"no claim is made that a hash from this build matches
//! one from any other build"* — which is honest, and which nothing could have
//! detected either way. A hash that nothing pins can move without anyone
//! noticing, including the person who moved it.
//!
//! So this file pins them. It is **not** a freeze, and a failure here is not
//! a bug report.
//!
//! # What to do when this test fails
//!
//! Read the diff and decide which of two things happened.
//!
//! * **You changed the encoding on purpose** — added a node kind, renumbered
//!   a tag, canonicalised something that used to be written verbatim. Then
//!   the new hashes are correct: update the table below, and say in the
//!   commit message that identities moved and why. That sentence is the
//!   whole point of this file. Anyone reasoning about whether §8 can ever be
//!   emptied needs the *rate* these move at, and the commit log is where
//!   that record accumulates.
//!
//! * **You did not** — you changed the parser, an unrelated pass, a
//!   dependency. Then something reached the hash that should not have, and
//!   this caught it. That is the bug.
//!
//! The one thing not to do is update the table without deciding which.
//!
//! The failure prints the new table one row per line; `cargo fmt` re-wraps it
//! afterwards, so paste and then format.
//!
//! # Why these fixtures
//!
//! One per node family the encoder has a tag for, kept as small as a
//! fixture can be while still reaching the tag. Small matters: when a hash
//! moves, the set of fixtures that moved with it says roughly where.

use cancho_id::identify;
use cancho_syntax::parse;

/// `(fixture name, source, declaration to read)`.
///
/// Source is a single line per fixture on purpose — formatting does not reach
/// the hash (there is a test for that next door), so the shape here is chosen
/// for reading rather than to stand for anything.
const FIXTURES: &[(&str, &str, &str)] = &[
    ("empty-row", "fn f() -> [] int { return 0; }", "f"),
    ("int-literal", "fn f() -> [] int { return 7; }", "f"),
    ("bool-literal", "fn f() -> [] bool { return true; }", "f"),
    ("float-literal", "fn f() -> [] float { return 1.5; }", "f"),
    ("arithmetic", "fn f(a: int, b: int) -> [] int { return a * b - 1; }", "f"),
    ("bitwise", "fn f(a: int) -> [] int { return (a << 2) ^ 0x3f; }", "f"),
    // `!` and `~` rather than `-`: there is no unary minus to reach for
    // here. `0 - a` is a *binary* subtract, which is how this fixture was
    // first written and what the golden table caught -- perturbing the
    // BINARY tag moved it, so it had never covered UNARY at all.
    ("unary-not", "fn f(a: bool) -> [] bool { return !a; }", "f"),
    ("unary-bit-not", "fn f(a: int) -> [] int { return ~a; }", "f"),
    ("unary-deref", "fn f[&r](s: &r int) -> [] int { return *s; }", "f"),
    ("comparison", "fn f(a: int, b: int) -> [] bool { return a <= b; }", "f"),
    ("call", "fn g(n: int) -> [] int { return n; } fn f() -> [] int { return g(1); }", "f"),
    ("recursion", "fn f(n: int) -> [] int { if n < 2 { return 1; } return n * f(n - 1); }", "f"),
    ("let-and-local", "fn f() -> [] int { let x = 1; return x; }", "f"),
    ("var-and-assign", "fn f() -> [] int { var x = 1; x = 2; return x; }", "f"),
    ("while-loop", "fn f() -> [] int { var i = 0; while i < 3 { i = i + 1; } return i; }", "f"),
    ("if-else", "fn f(a: bool) -> [] int { if a { return 1; } else { return 2; } }", "f"),
    ("struct-decl", "struct P { x: int, y: int }", "P"),
    ("struct-decl-res", "res struct F { fd: int }", "F"),
    (
        "struct-literal",
        "struct P { x: int, y: int } fn f() -> [] P { return P { x: 1, y: 2 }; }",
        "f",
    ),
    ("field-read", "struct P { x: int, y: int } fn f(p: P) -> [] int { return p.x; }", "f"),
    (
        "destructure",
        "struct P { x: int, y: int } fn f(p: P) -> [] int { let P { x, y } = p; return x; }",
        "f",
    ),
    ("enum-decl", "enum Shape { Circle(int), Empty }", "Shape"),
    (
        "match-arms",
        "enum S { A(int), B } fn f(s: S) -> [] int { match s { S::A(n) => { return n; } S::B => { return 0; } } }",
        "f",
    ),
    ("tuple", "fn f() -> [] (int, int) { return (1, 2); }", "f"),
    ("tuple-field", "fn f(t: (int, int)) -> [] int { return t.0; }", "f"),
    ("reference", "fn f[&r](s: &r int) -> [] int { return 0; }", "f"),
    ("unique-reference", "fn f[&r](s: &!r int) -> [] int { return 0; }", "f"),
    ("slice-index", "fn f[&r](s: &r [int]) -> [] int { return s[0]; }", "f"),
    ("slice-range", "fn f[&r](s: &r [int]) -> [] int { return len(s[1..2]); }", "f"),
    ("generic", "fn f[T](x: T) -> [] T { return x; }", "f"),
    ("effect-row", "fn f[&i](io: &!i Io) -> [io_write] int { return putchar(io, 65); }", "f"),
    (
        "effect-row-argument",
        "fn f[&f2](x: &f2 Ffi(\"libc\")) -> [ffi(\"libc\")] int { return 0; }",
        "f",
    ),
    ("extern-decl", "extern fn e[&f](x: &f Ffi(\"libc\"), n: int) -> [ffi(\"libc\")] int;", "e"),
    (
        "region",
        "fn f() -> [] int { region a { let s = alloc_slice[a](2, 0); return len(s); } }",
        "f",
    ),
    ("static-item", "static t: [int] { let s = alloc_slice[static](2, 0); return s; }", "t"),
];

/// The hashes as of the commit that last touched them.
///
/// `(fixture name, sig, body)`. A type declaration has one identity, so its
/// two columns are the same hash; so does an `extern`, for the reason the
/// encoder gives — there is no body a caller could fail to notice.
const GOLDEN: &[(&str, &str, &str)] = &[
    (
        "empty-row",
        "1256033bac54eba7fee1a619018f2647e72379fd178c8f7cc958f74fb41a8ee3",
        "d0aa4d0beecc0bb1f31b368ff97ddfef1ceb6158bffecb108260c1eb264da1da",
    ),
    (
        "int-literal",
        "1256033bac54eba7fee1a619018f2647e72379fd178c8f7cc958f74fb41a8ee3",
        "d035023cc0492f27c2cf11d5bdea65d55a68865b9c035cb174fa7f55e2adfb09",
    ),
    (
        "bool-literal",
        "af93b122140a4853822e08c02c24f69d20e2239401ef4bfc0be4f5a14e0b9f68",
        "e42a4bf7f638558e31f5dde0f813883ffde7b77f32085ef7ed66ad2b1640ceb3",
    ),
    (
        "float-literal",
        "564dd353fa33f77c6b10e98cba4b82c5bf232b9e0ceffe3564f5fca656444fc8",
        "c3ac95e3c215f74e4006da4b39566792e71b704573ae2742cec5dd83a757b8e4",
    ),
    (
        "arithmetic",
        "c5e38ed91492e3078e8d7e85a77c385480a24650e3255b24fd28a602b77474b8",
        "54c98d15655c6099331560a492a0fee46e29922f27113cce2e6476bef25fc50c",
    ),
    (
        "bitwise",
        "805d09c1834d1782d5dd867099c800ae8caf4a925df03bd6216e845215466bb3",
        "be960f39daee8eefb1d0f4b927c962f4ad1be5e1d458ce7c561c8bf453a5b4cc",
    ),
    (
        "unary-not",
        "3e887039b6cdf8cc6cd8f6b9e9e89e36d6f2301c98840afaea3ed4fe21c749ff",
        "740b25c27f31e37684fa3bfe4c940335e981c4a35fb425a1ef85e6f1a1bebefb",
    ),
    (
        "unary-bit-not",
        "805d09c1834d1782d5dd867099c800ae8caf4a925df03bd6216e845215466bb3",
        "ccb2d8d2357886649e5f97e40452361da51bfa18d56068bb2266d6b8fc1014ec",
    ),
    (
        "unary-deref",
        "5cece7af3b661477bf6ca6f15ebe4ac7161bde13e52b26937db76c2c69404f82",
        "6ae4cf9d2500c29dc601a09346c77df94f4b585e309b8751bf3af0d496546ace",
    ),
    (
        "comparison",
        "b1cb9ea2390a93432116fc6bd183d06d32ae4d2fd2da68bd7f650c9952dc9562",
        "ed3b3fe8f951d6dbc5e0202d646374637de21a527fd7a673a89bea2f59aba0b0",
    ),
    (
        "call",
        "1256033bac54eba7fee1a619018f2647e72379fd178c8f7cc958f74fb41a8ee3",
        "cc6d0fc194d3700bdb273cecf278bc913c0f9c24482bdcc0fe55f98219ec6eac",
    ),
    (
        "recursion",
        "805d09c1834d1782d5dd867099c800ae8caf4a925df03bd6216e845215466bb3",
        "b3a801202e4487c9c1e8f3cf806faf603eb53d9a77ba188d87492a835740afc1",
    ),
    (
        "let-and-local",
        "1256033bac54eba7fee1a619018f2647e72379fd178c8f7cc958f74fb41a8ee3",
        "1d1d1ba3aff384be7243f9dd7032fe72654b885ecd0728f0018c54ab3d695580",
    ),
    (
        "var-and-assign",
        "1256033bac54eba7fee1a619018f2647e72379fd178c8f7cc958f74fb41a8ee3",
        "30d14edb520b271c2849f78a79ca8c883a4ea74fa51ac3c98d9f65009e4e8382",
    ),
    (
        "while-loop",
        "1256033bac54eba7fee1a619018f2647e72379fd178c8f7cc958f74fb41a8ee3",
        "5d75f432c865c945485908522fcc546801269a3d179bf55b0cd9f3fc1988cf70",
    ),
    (
        "if-else",
        "8c4db275e617cbfa6af41335b1b324038389497946e773ba45947b19765a210e",
        "1133a7c137d6315d7b4b685f24b2118512ac5973f7ec3c2d842ea4ea09c9aac3",
    ),
    (
        "struct-decl",
        "e11eee6bac21a5a49315ff9a19005ec1110514c4f6dfb34c75a63d8c37a08bba",
        "e11eee6bac21a5a49315ff9a19005ec1110514c4f6dfb34c75a63d8c37a08bba",
    ),
    (
        "struct-decl-res",
        "14ee82f0af5819504be3b57aa6c95b78e92e4501d244ed89ea2627b137494ad4",
        "14ee82f0af5819504be3b57aa6c95b78e92e4501d244ed89ea2627b137494ad4",
    ),
    (
        "struct-literal",
        "af070a452c2a34fff5b631b5e1c3b4cdc0b7f2399653595a03bbbd0d173533df",
        "ce7356de2b56f3b44cfd2eef23b4f6e5319ec5d50dfa492f5804fd7315e1594c",
    ),
    (
        "field-read",
        "721f0425a4c2fc2f49643e8c20666f149f8a44e8531d940c4d6589f7fde5b2c6",
        "5f3625a33b2e60cec2383b13f95810a4c55e4ecc93135bfdac7bea5a5fedcd23",
    ),
    (
        "destructure",
        "721f0425a4c2fc2f49643e8c20666f149f8a44e8531d940c4d6589f7fde5b2c6",
        "519b85c7af72e8abd4c049b314b982bad0c009419bbb910baffe3bfc336f9b6e",
    ),
    (
        "enum-decl",
        "2ed02eb8649654a70196e1a7c082adcfb6fb2b1a6d132c47a2841dddab8db516",
        "2ed02eb8649654a70196e1a7c082adcfb6fb2b1a6d132c47a2841dddab8db516",
    ),
    (
        "match-arms",
        "c408b605ae716a30b2efeb6064d3bdb20845ca819ed47c2a90e9615480559d42",
        "af2033e1744f46fceab69b3e9832d775481eb522c8edc8a6f27f03e563d88487",
    ),
    (
        "tuple",
        "7b28a40a8fa79ee83521994dcfdaaa098f0e8266b41d83959c028ca71d0de21e",
        "ba9240db134cd3d682b1873f29d537374b2baca352f5fcde77e15d0f9215f352",
    ),
    (
        "tuple-field",
        "f34f08d58e7308126cdfa60e9279ddd0db5e1ddc59daaa14c0c9fbb303a30d0e",
        "800e6b246e511748951f5bcde5ecbd209dc48794a711295f5c5a6133e9d92836",
    ),
    (
        "reference",
        "5cece7af3b661477bf6ca6f15ebe4ac7161bde13e52b26937db76c2c69404f82",
        "d0aa4d0beecc0bb1f31b368ff97ddfef1ceb6158bffecb108260c1eb264da1da",
    ),
    (
        "unique-reference",
        "ac1fc21fc858e4aadea0ad01c837067d66ff21ad7ae0c3067a945834ab9c295a",
        "d0aa4d0beecc0bb1f31b368ff97ddfef1ceb6158bffecb108260c1eb264da1da",
    ),
    (
        "slice-index",
        "943e84de1fbb448c6de509cd93dac59b53557b2e23271c16a786b6bf2d6f583f",
        "ea5e0270bac92246e005461bf8a56e69f98a1047d6506ff2f685c7da17a27d54",
    ),
    (
        "slice-range",
        "943e84de1fbb448c6de509cd93dac59b53557b2e23271c16a786b6bf2d6f583f",
        "299efbd9a213317d9a07a0a8104771c81108f56ba1c61ffae6bfc315605ac700",
    ),
    (
        "generic",
        "5dafe4ced0a131214c57d4178876b82450c2eda75dd552f72fea310db2de523a",
        "c721c66a48e2666914ea059e5641626db60f349a0c9dc5e23b056b4f8c915049",
    ),
    (
        "effect-row",
        "abebd853d6b74e20b6a500ec16da5c0e7e031c8df44f2c153b40bd000d9dda63",
        "aa29f53ce4ad100feace478cea02fcf96d6792c88e490a261967def9f7d05e2b",
    ),
    (
        "effect-row-argument",
        "2bfdc94bf49275cacb3291eef592e4a887743c1d9e1accbf549570dda4df9f68",
        "d0aa4d0beecc0bb1f31b368ff97ddfef1ceb6158bffecb108260c1eb264da1da",
    ),
    (
        "extern-decl",
        "8375dd99689e873f2f1ae7e6716c06b54f23590b841a6a700d846a5df335e569",
        "8375dd99689e873f2f1ae7e6716c06b54f23590b841a6a700d846a5df335e569",
    ),
    (
        "region",
        "1256033bac54eba7fee1a619018f2647e72379fd178c8f7cc958f74fb41a8ee3",
        "a0f187ec697ea634ba264f5fffb5248b04e211b9315ff821674bc4698880f930",
    ),
    (
        "static-item",
        "2b56a426d7383f96abbffef7d88ff507188d98a53c61c043fa27044421877ff3",
        "d29824f10a3518ddb72a4e9610ceb82dd1ab763de9d759215d63f4c4d695a80d",
    ),
];

#[test]
fn the_encoder_emits_what_it_emitted_before() {
    let mut actual: Vec<(String, String, String)> = Vec::new();
    for (name, source, decl) in FIXTURES {
        let ast = parse(source).unwrap_or_else(|e| panic!("`{name}` should parse: {e:?}"));
        let ids = identify(&ast);
        let (sig, body) = match ids.function(decl) {
            Some(f) => (f.sig.to_hex(), f.body.to_hex()),
            None => {
                let t = ids
                    .type_decl(decl)
                    .unwrap_or_else(|| panic!("`{name}` should declare `{decl}`"));
                (t.id.to_hex(), t.id.to_hex())
            }
        };
        actual.push(((*name).to_owned(), sig, body));
    }

    if GOLDEN.is_empty() {
        panic!("GOLDEN is empty; paste this in:\n\n{}", render(&actual));
    }

    // Matched by fixture *name*, never by position. Adding a fixture in the
    // middle would otherwise report every row below it as changed, which is
    // the one thing this test must not do: the list of what moved is the
    // signal, and a list that cries wolf on an insertion would be ignored
    // within two slices.
    let mut moved: Vec<String> = Vec::new();
    let mut added: Vec<&str> = Vec::new();
    for (name, sig, body) in &actual {
        match GOLDEN.iter().find(|(n, _, _)| n == name) {
            Some((_, s, b)) if s == sig && b == body => {}
            Some(_) => moved.push(name.clone()),
            None => added.push(name),
        }
    }
    let dropped: Vec<&str> = GOLDEN
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| !actual.iter().any(|(a, _, _)| a == n))
        .collect();

    if !moved.is_empty() || !added.is_empty() || !dropped.is_empty() {
        let mut what = Vec::new();
        if !moved.is_empty() {
            what.push(format!("{} moved: {}", moved.len(), moved.join(", ")));
        }
        if !added.is_empty() {
            what.push(format!("{} new: {}", added.len(), added.join(", ")));
        }
        if !dropped.is_empty() {
            what.push(format!("{} gone: {}", dropped.len(), dropped.join(", ")));
        }
        panic!(
            "canonical identities are not what GOLDEN records -- {}\n\n\
             A *moved* hash is not automatically a bug: `docs/canonical-ast.md` §8\n\
             says these are not frozen. Decide which happened, then act:\n\
             \x20 * you changed the encoding on purpose -> update GOLDEN below, and say\n\
             \x20   in the commit message that identities moved and why. That record is\n\
             \x20   what this test is for.\n\
             \x20 * you did not -> something reached the hash that should not have, and\n\
             \x20   this is the bug.\n\n\
             A *new* or *gone* row is only a fixture being added or removed.\n\n\
             The new table:\n\n{}",
            what.join("; "),
            render(&actual)
        );
    }
}

/// The table, ready to paste back into `GOLDEN`.
fn render(rows: &[(String, String, String)]) -> String {
    let mut out = String::from("const GOLDEN: &[(&str, &str, &str)] = &[\n");
    for (name, sig, body) in rows {
        out.push_str(&format!("    (\"{name}\", \"{sig}\", \"{body}\"),\n"));
    }
    out.push_str("];\n");
    out
}

/// Every fixture reaches a declaration, and no two fixtures are the same
/// program wearing different names.
///
/// Without this a fixture could quietly stop testing anything — a typo that
/// made two sources identical would still produce a stable table, and the
/// table would go on passing while covering one node kind less.
///
/// It compares the **pair**, not the body alone. Writing it the other way
/// fails immediately and correctly: `empty-row`, `reference` and
/// `unique-reference` have three different signatures over one body, because
/// all three bodies are `return 0;`. That is the whole point of hashing a
/// signature apart from a body, so the sameness is the feature.
#[test]
fn the_fixtures_are_distinct() {
    let mut seen: Vec<((String, String), &str)> = Vec::new();
    for (name, source, decl) in FIXTURES {
        let ast = parse(source).unwrap_or_else(|e| panic!("`{name}` should parse: {e:?}"));
        let ids = identify(&ast);
        let pair = match ids.function(decl) {
            Some(f) => (f.sig.to_hex(), f.body.to_hex()),
            None => {
                let t = ids
                    .type_decl(decl)
                    .unwrap_or_else(|| panic!("`{name}` should declare `{decl}`"));
                (t.id.to_hex(), t.id.to_hex())
            }
        };
        if let Some((_, other)) = seen.iter().find(|(p, _)| *p == pair) {
            panic!(
                "`{name}` and `{other}` hash the same; one of them is not testing what it names"
            );
        }
        seen.push((pair, name));
    }
}

/// A `static`'s `sig` carries its name, as a function's does: two statics of
/// one type are two identities. `vcs publish` keys its manifest by `sig`, and
/// when the type was the whole of it the second `[int]` table of a module was
/// refused as "already published at a different body".
#[test]
fn two_statics_of_one_type_have_two_signatures() {
    let ast = parse(
        "static p: [int] { let h = alloc_slice[static](2, 0); return h; }\n\
         static q: [int] { let h = alloc_slice[static](2, 0); return h; }",
    )
    .expect("parses");
    let ids = identify(&ast);
    let (p, q) = (ids.function("p").expect("p"), ids.function("q").expect("q"));
    assert_ne!(p.sig, q.sig, "the name is part of a static's signature");
    // The body is the same text, and stays the same: a name is the
    // declaration's, not the body's.
    assert_eq!(p.body, q.body);
}

/// A static's `sig` cannot equal a function's or an extern's of the same
/// name: the encodings start with different tags.
#[test]
fn a_static_signature_is_never_a_function_signature() {
    let s = identify(
        &parse("static t: [int] { let h = alloc_slice[static](2, 0); return h; }").unwrap(),
    );
    let f = identify(&parse("fn t() -> [] int { return 0; }").unwrap());
    assert_ne!(s.function("t").unwrap().sig, f.function("t").unwrap().sig);
}
