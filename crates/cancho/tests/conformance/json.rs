//! `std.json` (`docs/json.md`): the parser and writer against `serde_json`
//! and Rust's own correctly rounded `f64` parse, and the library's own tests.

use super::*;
use std::io::Write;
use std::process::Stdio;

/// Build one of `tests/programs/` against the standard library.
pub(super) fn build_driver(tag: &str, program: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(tag);
    let exe = dir.join("driver");
    let build = Command::new(BIN)
        .args(["build", "--std"])
        .arg(repo_root().join("tests/programs").join(program))
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("the compiler runs");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    (dir, exe)
}

pub(super) fn feed(exe: &Path, input: &[u8]) -> std::process::Output {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the driver runs");
    // Written from a thread: a driver that answers before it has read
    // everything must not deadlock the pipe.
    let mut stdin = child.stdin.take().expect("a piped stdin");
    let bytes = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let out = child.wait_with_output().expect("the driver finishes");
    let _ = writer.join();
    out
}

#[test]
fn the_library_tests_pass_on_both_backends() {
    // `tests/lex/*.cho` are `cancho test` files: the library's own unit
    // tests, written in the language they test.
    let dir = repo_root().join("tests/lex");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/lex exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "cho"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    for file in files {
        for backend in ["llvm", "cranelift"] {
            let out = Command::new(BIN)
                .args(["test", "--std", "--backend", backend])
                .arg(&file)
                .output()
                .expect("the compiler runs");
            let text = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{} on {backend}:\n{text}\n{}",
                file.display(),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(text.contains("test result: ok."), "{}: {text}", file.display());
        }
    }
}

/// A small deterministic generator, so a failure names a reproducible input.
pub(super) struct Lcg(pub(super) u64);
impl Lcg {
    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    pub(super) fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The decimal text of `m / 2^s`, exactly: `m * 5^s` with the point `s`
/// digits from the right. `m` odd makes it the midpoint between two floats
/// when `m` has 54 bits.
fn exact_binary_fraction(m: u128, s: u32) -> String {
    let digits = (m * 5u128.pow(s)).to_string();
    if s == 0 {
        return digits;
    }
    let padded = format!("{digits:0>width$}", width = s as usize + 1);
    let split = padded.len() - s as usize;
    format!("{}.{}", &padded[..split], &padded[split..])
}

fn number_corpus() -> Vec<String> {
    let mut rng = Lcg(0x5eed);
    let mut numbers: Vec<String> = Vec::new();

    // Every exponent, shortest round-trip form.
    for _ in 0..4000 {
        let bits = rng.next() << 11 | rng.below(2048);
        let x = f64::from_bits(bits & 0x7fff_ffff_ffff_ffff);
        if x.is_finite() {
            numbers.push(format!("{x:e}"));
            numbers.push(format!("-{x:e}"));
        }
    }
    // Seventeen significant digits of a value in [0, 1) and in a few decades.
    for _ in 0..2000 {
        let x = rng.next() as f64 / (1u64 << 53) as f64;
        numbers.push(format!("{x:.16e}"));
        numbers.push(format!("{x:.20}"));
        numbers.push(format!("{:.3}", x * 1000.0));
    }
    // Halfway between two adjacent floats, and a hair either side of it:
    // the cases a floating-point shortcut gets wrong.
    for _ in 0..600 {
        let k = (1u128 << 52) + u128::from(rng.next()) % (1u128 << 52);
        let m = 2 * k + 1;
        let s = rng.below(26) as u32;
        let mid = exact_binary_fraction(m, s);
        numbers.push(mid.clone());
        numbers.push(format!("{mid}1"));
        let mut below = mid.clone();
        below.pop();
        numbers.push(format!("{below}49"));
        // And the same above 2^53, as integers.
        let j = 1 + rng.below(10) as u32;
        let big = m << (j - 1);
        numbers.push(big.to_string());
        numbers.push(format!("{big}.0000000000000000000001"));
        numbers.push(format!("{big}e0"));
    }
    // Integers at and past the ends of `int`.
    for _ in 0..800 {
        numbers.push((rng.next() as i64).wrapping_mul(rng.next() as i64 | 1).to_string());
    }
    for text in [
        "0",
        "-0",
        "1",
        "-1",
        "9223372036854775807",
        "-9223372036854775808",
        "9223372036854775808",
        "-9223372036854775809",
        "18446744073709551615",
        "18446744073709551616",
        "9007199254740992",
        "9007199254740993",
        "9007199254740995",
        "123456789012345678901234567890",
        "0.1",
        "0.2",
        "0.3",
        "1e22",
        "1e23",
        "5e-324",
        "2.4703282292062327e-324",
        "2.4703282292062328e-324",
        "2.2250738585072011e-308",
        "2.2250738585072014e-308",
        "1.7976931348623157e308",
        "1.7976931348623158e308",
        "1.7976931348623159e308",
        "1e309",
        "-1e400",
        "1e-400",
        "0e5",
        "-0.0",
        "100e-2",
        "0.000001",
        "1e-7",
        "4.35",
        "1e999999999999",
        "1e-999999999999",
    ] {
        numbers.push(text.to_owned());
    }
    numbers
}

#[test]
fn numbers_are_read_exactly_as_rust_reads_them() {
    // Rust's `str::parse::<f64>` is correctly rounded, which is the
    // property under test: not "close", the same bits. Integers are checked
    // against an `i128` parse, saturated to `i64`; a number that is not an
    // integer is checked for `to_int`'s truncating saturation too.
    let numbers = number_corpus();
    let (dir, exe) = build_driver("json-floats", "json_floats.cho");
    let doc = format!("[{}]", numbers.join(","));
    let out = feed(&exe, doc.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), numbers.len());

    let mut wrong = Vec::new();
    for (number, line) in numbers.iter().zip(&lines) {
        let (float_bits, int_value) = line.split_once(' ').expect("two answers a line");
        let want = number.parse::<f64>().expect("Rust reads every number written here");
        let got = f64::from_bits(float_bits.parse::<i64>().unwrap() as u64);
        let is_integer = !number.contains(['.', 'e', 'E']);
        let want_int: i64 = if is_integer {
            number.parse::<i128>().unwrap().clamp(i64::MIN as i128, i64::MAX as i128) as i64
        } else {
            want as i64
        };
        if got.to_bits() != want.to_bits()
            && !(got == 0.0 && want == 0.0 && got.is_sign_negative() == want.is_sign_negative())
        {
            wrong.push(format!("{number}: read {got:e}, Rust reads {want:e}"));
        } else if int_value.parse::<i64>().unwrap() != want_int {
            wrong.push(format!("{number}: to_int {int_value}, want {want_int}"));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        wrong.is_empty(),
        "{} of {} numbers read wrongly, first few:\n{}",
        wrong.len(),
        numbers.len(),
        wrong.iter().take(15).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// Documents that are valid, and exercise every production.
const SEEDS: &[&str] = &[
    r#"{"name":"Ada","age":36,"pi":3.14159,"ok":true,"none":null,"tags":["a","b"],"nest":{"x":[1,2,{"y":-1e3}]}}"#,
    r#"[1,-2,3.5,-0.25,1e10,1E-5,0,-0,true,false,null,"s","",{},[]]"#,
    r#"{"esc":"quote\" back\\ slash\/ nl\n cr\r tab\t bs\b ff\f","uni":"é日😀","raw":"é日😀"}"#,
    r#"  { "spaced" : [ 1 , 2 , 3 ] , "k" : "v" }  "#,
    r#"{"a":{"b":{"c":{"d":{"e":[[[[[1]]]]]}}}}}"#,
    r#"[0.1,0.2,0.3,123456789.123456789,2.5e-8,1.5e22,100,200,300]"#,
    "\"just a string\"",
    "-12.5e-3",
    "true",
    r#"{"":"empty key","a b":1,"\u0000":2}"#,
];

fn mutate(seed: &[u8], rng: &mut Lcg) -> Vec<u8> {
    const POOL: &[u8] =
        b"{}[]\",:\\-+.eE0123456789tfnul \t\n\r\x00\x1f\x7f\xc3\xa9\xff\xed\xa0\x80uabx/'";
    let mut doc = seed.to_vec();
    for _ in 0..1 + rng.below(2) {
        if doc.is_empty() {
            break;
        }
        let at = rng.below(doc.len() as u64) as usize;
        match rng.below(5) {
            0 => doc[at] = POOL[rng.below(POOL.len() as u64) as usize],
            1 => {
                doc.remove(at);
            }
            2 => doc.insert(at, POOL[rng.below(POOL.len() as u64) as usize]),
            3 => doc.truncate(at),
            _ => {
                let to = rng.below(doc.len() as u64) as usize;
                doc.swap(at, to);
            }
        }
    }
    doc
}

/// `serde_json`'s answer: `Ok(Some(value))` if it accepts the bytes, `Ok(None)`
/// if it refuses them, and `Err(())` if it has no opinion worth comparing: a
/// number past the `f64` range, which `serde_json` refuses and this library
/// accepts as an infinity (`to_float`'s documented answer) -- grammar is the
/// same, policy is not.
fn reference(doc: &[u8]) -> Result<Option<serde_json::Value>, ()> {
    match serde_json::from_slice::<serde_json::Value>(doc) {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.to_string().contains("out of range") => Err(()),
        Err(_) => Ok(None),
    }
}

/// Equal as JSON values, with numbers compared as the floats they are --
/// `serde_json` does not round a long decimal correctly unless asked to, and
/// the library under test does, so a one-ulp difference is not a disagreement
/// about what the document says.
fn same(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value::*;
    match (a, b) {
        (Number(x), Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            x == y || (x - y).abs() <= x.abs().max(y.abs()) * 4e-16
        }
        (Array(x), Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q)),
        (Object(x), Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

#[test]
fn the_parser_accepts_what_serde_json_accepts_and_writes_it_back_whole() {
    let (dir, exe) = build_driver("json-roundtrip", "json_roundtrip.cho");
    let mut rng = Lcg(42);
    let mut accepted = 0;
    let mut refused = 0;
    let mut failures = Vec::new();

    let mut docs: Vec<Vec<u8>> = SEEDS.iter().map(|s| s.as_bytes().to_vec()).collect();
    for _ in 0..1500 {
        let seed = SEEDS[rng.below(SEEDS.len() as u64) as usize];
        docs.push(mutate(seed.as_bytes(), &mut rng));
    }
    for doc in &docs {
        let out = feed(&exe, doc);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let ours_ok = out.status.code() == Some(0) && !text.starts_with("E ");
        let Ok(theirs) = reference(doc) else { continue };
        if out.status.code() != Some(0) {
            failures.push(format!(
                "{:?}: the driver died ({:?})",
                String::from_utf8_lossy(doc),
                out.status
            ));
            continue;
        }
        match (ours_ok, theirs) {
            (true, Some(want)) => {
                accepted += 1;
                match serde_json::from_str::<serde_json::Value>(text.trim_end()) {
                    Ok(got) if same(&got, &want) => {}
                    Ok(got) => failures.push(format!(
                        "{:?}: written back as {got}, serde_json reads the input as {want}",
                        String::from_utf8_lossy(doc)
                    )),
                    Err(e) => failures.push(format!(
                        "{:?}: wrote {text:?}, which is not JSON ({e})",
                        String::from_utf8_lossy(doc)
                    )),
                }
            }
            (false, None) => refused += 1,
            (true, None) => failures.push(format!(
                "{:?}: accepted; serde_json refuses it",
                String::from_utf8_lossy(doc)
            )),
            (false, Some(_)) => failures.push(format!(
                "{:?}: refused ({}); serde_json accepts it",
                String::from_utf8_lossy(doc),
                text.trim_end()
            )),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!("{} documents: {accepted} accepted by both, {refused} refused by both", docs.len());
    assert!(
        accepted > 200 && refused > 200,
        "the corpus should exercise both: {accepted}/{refused}"
    );
    assert!(
        failures.is_empty(),
        "{} disagreements, first few:\n{}",
        failures.len(),
        failures.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn a_misused_writer_traps_instead_of_writing_bad_json() {
    // Each of these is a bug in the caller, and the alternative to stopping
    // is a service that sends `{"a":}`.
    for (tag, body) in [
        ("keyless", "w = json.begin_object(heap, w); w = json.put_int(heap, w, 1);"),
        ("key-outside", "w = json.put_key(heap, w, \"k\");"),
        ("key-in-array", "w = json.begin_array(heap, w); w = json.put_key(heap, w, \"k\");"),
        (
            "two-keys",
            "w = json.begin_object(heap, w); w = json.put_key(heap, w, \"a\"); w = json.put_key(heap, w, \"b\");",
        ),
        ("unbalanced-end", "w = json.end_array(heap, w);"),
        ("mismatched-end", "w = json.begin_array(heap, w); w = json.end_object(heap, w);"),
        (
            "end-after-key",
            "w = json.begin_object(heap, w); w = json.put_key(heap, w, \"a\"); w = json.end_object(heap, w);",
        ),
        (
            "unfinished",
            "w = json.begin_array(heap, w); let kept = json.finish(w); buffer.drop(heap, kept); w = json.writer(heap, 4);",
        ),
        // `put_fragment` checks what it splices: a fragment that is not exactly one
        // value would make the whole document invalid from a distance.
        ("fragment-empty", "w = json.put_fragment(heap, w, \"\");"),
        ("fragment-whitespace-only", "w = json.put_fragment(heap, w, \"  \");"),
        ("fragment-two-values", "w = json.put_fragment(heap, w, \"1 2\");"),
        ("fragment-trailing-comma", "w = json.put_fragment(heap, w, \"[1,]\");"),
        ("fragment-unterminated", "w = json.put_fragment(heap, w, \"{\\\"a\\\":\");"),
        (
            "fragment-keyless-in-object",
            "w = json.begin_object(heap, w); w = json.put_fragment(heap, w, \"1\");",
        ),
    ] {
        let dir = scratch(&format!("json-misuse-{tag}"));
        let source = dir.join("misuse.cho");
        std::fs::write(
            &source,
            format!(
                "import std.json;\nimport std.buffer;\n\n\
                 fn run[&r](heap: &!r Heap) -> [heap] int {{\n\
                     var w = json.writer(heap, 4);\n\
                     {body}\n\
                     json.drop(heap, w);\n\
                     return 0;\n\
                 }}\n\n\
                 fn main(world: World) -> [] int {{\n\
                     let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
                     release(args); release(fs); release(ffi); release(io);\n\
                     var status = 0;\n\
                     borrow mut heap as &!h in {{ status = run(h); }}\n\
                     release(heap);\n\
                     return status;\n\
                 }}\n"
            ),
        )
        .expect("a writable fixture");
        let exe = dir.join("misuse");
        let build = Command::new(BIN)
            .args(["build", "--std"])
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{tag}: {}", String::from_utf8_lossy(&build.stderr));
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), None, "{tag} should be killed by a signal, not exit");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Documents of the kinds the JSON test suites (`nst/JSONTestSuite`, `y_`, `n_` and `i_` files)
/// are made of, written out: the corpus below mutates the seeds, and this is what mutation is
/// unlikely to reach -- depth at the limit and past it, bytes a string may not hold, a BOM,
/// numbers at the edge of the grammar, truncations, and trailing bytes.
fn edge_documents() -> Vec<Vec<u8>> {
    let mut docs: Vec<Vec<u8>> = [
        "",
        " ",
        "\n",
        "[",
        "]",
        "{",
        "}",
        "[,]",
        "[1,]",
        "[,1]",
        "[1 2]",
        "[1,,2]",
        "{,}",
        "{\"a\"}",
        "{\"a\":}",
        "{\"a\":1,}",
        "{a:1}",
        "{'a':1}",
        "{\"a\" 1}",
        "{\"a\":1 \"b\":2}",
        "{1:2}",
        "[\"a\"",
        "[\"a\\\"]",
        "\"\\",
        "\"\\u\"",
        "\"\\u12\"",
        "\"\\u123g\"",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"\\ud800\\u0041\"",
        "\"\\ud83d\\ude00\"",
        "\"\\x41\"",
        "\"\\'\"",
        "\"\t\"",
        "\"\n\"",
        "\"\u{7f}\"",
        "\"\0\"",
        "01",
        "-01",
        "+1",
        "1.",
        ".5",
        "-.5",
        "1e",
        "1e+",
        "1E-",
        "0e0",
        "-",
        "--1",
        "1.e1",
        "0x1",
        "NaN",
        "Infinity",
        "-Infinity",
        "nul",
        "nulll",
        "tru",
        "truee",
        "False",
        "TRUE",
        "[1]x",
        "[1] [2]",
        "{} {}",
        "1 2",
        "null null",
        "// c\n1",
        "/* c */ 1",
        "[1, // c\n2]",
        "1e400",
        "-1e400",
        "1e-400",
        "123456789012345678901234567890",
        "-0",
        "0.0e0",
        "\u{feff}1",
        "\u{feff}[]",
        "[]\u{feff}",
        "\u{a0}1",
        "1\u{a0}",
        "\u{2028}1",
        "[\"\u{2028}\u{2029}\"]",
        "\"é日😀\"",
        "[\"\u{10ffff}\"]",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    // Bytes that are not text: truncated, overlong and surrogate UTF-8, in and out of a string.
    for bytes in [
        &b"\"\xc3\""[..],
        b"\"\xc0\xaf\"",
        b"\"\xe0\x80\xaf\"",
        b"\"\xed\xa0\x80\"",
        b"\"\xf4\x90\x80\x80\"",
        b"\"\xf8\x88\x80\x80\x80\"",
        b"\"\x80\"",
        b"\"\xff\"",
        b"\xff",
        b"\xc3\xa9",
        b"[\xc3\xa9]",
        b"{\"\xc3\":1}",
        b"[1,\xffnull]",
        b"\"\xef\xbf\xbe\"",
        b"\"\xf0\x9f\x98\x80\"",
    ] {
        docs.push(bytes.to_vec());
    }
    // Nesting at, just under and just past the limit of 128, both kinds, closed and not.
    for depth in [1usize, 2, 127, 128, 129, 130, 1000] {
        docs.push([vec![b'['; depth], vec![b']'; depth]].concat());
        docs.push([vec![b'['; depth], vec![b']'; depth.saturating_sub(1)]].concat());
        docs.push(
            ["{\"a\":".repeat(depth).into_bytes(), b"1".to_vec(), vec![b'}'; depth]].concat(),
        );
    }
    docs.push(vec![b'['; 100_000]);
    // A document the 4-wide short tape cannot hold, an object with many keys, a long string.
    docs.push(format!("[{}]", "1,".repeat(5000) + "1").into_bytes());
    docs.push(
        format!("{{{}}}", (0..300).map(|k| format!("\"k{k}\":{k}")).collect::<Vec<_>>().join(","))
            .into_bytes(),
    );
    docs.push(format!("\"{}\"", "x\\u00e9".repeat(3000)).into_bytes());
    docs
}

/// Every document of `edge_documents`, every seed, every prefix of every seed (the truncation the
/// suites call `n_structure_*` and `n_*_incomplete`), every seed with any one byte of the pool in
/// any one position, and forty thousand seeds with one or two random damages.
fn parse_with_corpus() -> Vec<Vec<u8>> {
    const POOL: &[u8] = b"{}[]\",:\\-+.eE09tfnul \t\n\r\x00\x1f\x7f\xc3\xa9\xff\xed\xa0\x80uabx/'";
    let mut docs = edge_documents();
    for seed in SEEDS {
        let seed = seed.as_bytes();
        docs.push(seed.to_vec());
        for end in 0..seed.len() {
            docs.push(seed[..end].to_vec());
        }
    }
    for seed in SEEDS.iter().filter(|s| s.len() < 200) {
        let seed = seed.as_bytes();
        for at in 0..seed.len() {
            for &b in POOL {
                let mut doc = seed.to_vec();
                doc[at] = b;
                docs.push(doc);
            }
        }
    }
    let mut rng = Lcg(7);
    for _ in 0..40_000 {
        let seed = SEEDS[rng.below(SEEDS.len() as u64) as usize];
        docs.push(mutate(seed.as_bytes(), &mut rng));
    }
    docs
}

#[test]
fn parse_with_answers_what_parse_answers_and_writes_the_same_tape() {
    // `parse_with` takes its two ints of state from the caller instead of making a region
    // (`docs/json.md` §3.1); the claim is that nothing else differs. The driver runs both on every
    // document -- into a tape of the right size and into one too short, with a state slice that
    // still holds the previous document's garbage -- and compares the answers and every int of the
    // tapes. A parser that agrees on ~59,000 damaged documents and on the edge cases agrees.
    let (dir, exe) = build_driver("json-with-diff", "json_with_diff.cho");
    let docs = parse_with_corpus();
    let mut framed = Vec::new();
    for doc in &docs {
        framed.extend_from_slice(format!("{}\n", doc.len()).as_bytes());
        framed.extend_from_slice(doc);
    }
    let out = feed(&exe, &framed);
    let _ = std::fs::remove_dir_all(&dir);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{text}{}", String::from_utf8_lossy(&out.stderr));
    let fields: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(fields[0], "ok", "{text}");
    let (total, accepted, refused): (usize, usize, usize) =
        (fields[1].parse().unwrap(), fields[2].parse().unwrap(), fields[3].parse().unwrap());
    eprintln!("parse_with: {total} documents, {accepted} accepted, {refused} refused");
    assert_eq!(total, docs.len(), "the driver read every document");
    assert!(accepted > 1000 && refused > 10_000, "the corpus should exercise both: {text}");
}

fn nesting(doc: &[u8]) -> usize {
    doc.iter().filter(|b| matches!(b, b'[' | b'{')).count()
}

#[test]
fn parse_with_agrees_with_serde_json_on_what_is_a_document() {
    // The test above ties `parse_with` to `parse`. This one pins the corpus to an outside oracle,
    // so that "they agree" cannot mean "both wrong the same way": `serde_json` classifies every
    // edge document and the library agrees. Deep nesting is left out: the two parsers' depth
    // limits are policy, and `std.json`'s (128, `docs/json.md`) is pinned by its own tests.
    let (dir, exe) = build_driver("json-with-oracle", "json_roundtrip.cho");
    let mut checked = 0;
    for doc in edge_documents().iter().filter(|d| d.len() < 2000 && nesting(d) < 100) {
        let Ok(theirs) = reference(doc) else { continue };
        let out = feed(&exe, doc);
        let text = String::from_utf8_lossy(&out.stdout);
        let ours = out.status.code() == Some(0) && !text.starts_with("E ");
        assert_eq!(
            ours,
            theirs.is_some(),
            "{:?}: serde_json {}, std.json {}",
            String::from_utf8_lossy(doc),
            if theirs.is_some() { "accepts" } else { "refuses" },
            if ours { "accepts" } else { "refuses" }
        );
        checked += 1;
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(checked > 100, "{checked}");
}
