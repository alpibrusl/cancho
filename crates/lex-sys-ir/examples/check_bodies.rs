//! The oracle for `examples/selfhost/bodies.ls` (`docs/self-hosting.md` section 6, stage 3c).
//!
//! Reads a program on standard input and writes the answer of the Rust checker: the first refusal
//! of the parser or of the declarations as one line `ERR rule-tag start end`, or, if the
//! declarations are well formed, one line for each function, `fn <start> <end>` and then `OK` or
//! `ERR rule-tag start end` for what `lex_sys_ir::check_bodies` made of its body.
//!
//!     cargo run -q -p lex-sys-ir --example check_bodies < some_file.ls
//!
//! With `--files` the input is a stream of files (`FILE <length>` and then that many bytes each),
//! parsed as one program: the user's files and then the library.

use std::io::Read;

/// Split a stream of files, each a line `FILE <length>` and then that many bytes, into the files.
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

/// The answer for a program of several files, parsed as the compiler does (`parse_into`, each
/// file at its own base offset in a `SourceMap`). The conformance test includes this file.
pub fn listing_files(files: &[String]) -> String {
    let mut map = lex_sys_syntax::SourceMap::new();
    let mut ast = lex_sys_syntax::Ast::new();
    let bases: Vec<u32> = files.iter().map(|text| map.add("file", text.clone())).collect();
    for (text, base) in files.iter().zip(bases) {
        if let Err(d) = lex_sys_syntax::parse_into(&mut ast, text, base) {
            return format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end);
        }
    }
    match lex_sys_ir::check_bodies(&ast) {
        Err(d) => format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end),
        Ok(bodies) => bodies
            .iter()
            .map(|b| match &b.refusal {
                None => format!("fn {} {} OK\n", b.span.start, b.span.end),
                Some(d) => format!(
                    "fn {} {} ERR {} {} {}\n",
                    b.span.start,
                    b.span.end,
                    d.rule.tag(),
                    d.span.start,
                    d.span.end
                ),
            })
            .collect(),
    }
}

/// The answer for `source` alone.
pub fn listing(source: &str) -> String {
    listing_files(&[source.to_owned()])
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
