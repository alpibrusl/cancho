//! The oracle for `examples/selfhost/check.cho` (`docs/self-hosting.md` section 6, stage 3b).
//!
//! Reads a source file on standard input and writes `OK` if the Rust parser accepts it and
//! `cancho_ir::check_declarations` finds its declarations well formed, else one line
//! `ERR rule-tag start end` for the first refusal of either.
//!
//!     cargo run -q -p cancho-ir --example check_declarations < some_file.cho
//!
//! With `--files` the input is a stream of files (`FILE <length>` and then that many bytes
//! each), parsed as one program: the user's files and then the library.

use std::io::Read;

/// The answer for a program of several files, parsed one after another into one AST as the
/// compiler does (`parse_into`, each file at its own base offset in a `SourceMap`). The
/// conformance test (`crates/cancho/tests/conformance/selfhost.rs`) includes this file and
/// calls this, so the test and the command-line tool cannot disagree.
pub fn listing_files(files: &[String]) -> String {
    let mut map = cancho_syntax::SourceMap::new();
    let mut ast = cancho_syntax::Ast::new();
    let bases: Vec<u32> = files.iter().map(|text| map.add("file", text.clone())).collect();
    for (text, base) in files.iter().zip(bases) {
        if let Err(d) = cancho_syntax::parse_into(&mut ast, text, base) {
            return format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end);
        }
    }
    match cancho_ir::check_declarations(&ast) {
        Ok(()) => "OK\n".to_owned(),
        Err(d) => format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end),
    }
}

/// The answer for `source` alone.
pub fn listing(source: &str) -> String {
    listing_files(&[source.to_owned()])
}

/// Split a stream of files, each a line `FILE <length>` and then that many bytes, into the
/// files. The programs of `examples/selfhost` read their input the same way.
pub fn read_stream(bytes: &[u8]) -> Vec<String> {
    let mut files = Vec::new();
    let mut at = 0;
    while bytes[at..].starts_with(b"FILE ") {
        let line_end = at + bytes[at..].iter().position(|b| *b == b'\n').expect("a header line");
        let length: usize = std::str::from_utf8(&bytes[at + 5..line_end])
            .expect("a header")
            .parse()
            .expect("a length");
        let text = &bytes[line_end + 1..line_end + 1 + length];
        files.push(String::from_utf8_lossy(text).into_owned());
        at = line_end + 1 + length;
    }
    files
}

#[allow(dead_code)]
fn main() {
    let mut source = Vec::new();
    std::io::stdin().read_to_end(&mut source).expect("stdin");
    if std::env::args().any(|a| a == "--files") {
        print!("{}", listing_files(&read_stream(&source)));
    } else {
        print!("{}", listing(&String::from_utf8_lossy(&source)));
    }
}
