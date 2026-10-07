//! The oracle for `examples/selfhost/lexer.cho`: the Rust lexer's answer in the port's
//! format. Reads a source file on standard input and writes one `Kind start end` line
//! per token, or, if the lexer refuses it, one line `ERR rule-tag start end`.
//!
//!     cargo run -q -p cancho-syntax --example dump_tokens < some_file.cho
//!
//! `examples/selfhost/diff.sh` compares it with the port over a corpus.

use std::io::Read;

use cancho_syntax::lexer::tokenize;

/// The tokens of `source`, or the one refusal line. The conformance test
/// (`crates/cancho/tests/conformance/selfhost.rs`) includes this file and calls this.
pub fn listing(source: &str) -> String {
    match tokenize(source) {
        Ok(tokens) => tokens
            .iter()
            .map(|t| format!("{:?} {} {}\n", t.kind, t.span.start, t.span.end))
            .collect(),
        Err(d) => format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end),
    }
}

#[allow(dead_code)]
fn main() {
    let mut source = Vec::new();
    std::io::stdin().read_to_end(&mut source).expect("stdin");
    print!("{}", listing(&String::from_utf8_lossy(&source)));
}
