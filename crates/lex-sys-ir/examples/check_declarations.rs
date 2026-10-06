//! The oracle for `examples/selfhost/check.ls` (`docs/self-hosting.md` section 6, stage 3b).
//!
//! Reads a source file on standard input and writes `OK` if the Rust parser accepts it and
//! `lex_sys_ir::check_declarations` finds its declarations well formed, else one line
//! `ERR rule-tag start end` for the first refusal of either.
//!
//!     cargo run -q -p lex-sys-ir --example check_declarations < some_file.ls

use std::io::Read;

/// The answer for `source`. The conformance test (`crates/lex-sys/tests/conformance/selfhost.rs`)
/// includes this file and calls this, so the test and the command-line tool cannot disagree.
pub fn listing(source: &str) -> String {
    let ast = match lex_sys_syntax::parse(source) {
        Ok(ast) => ast,
        Err(d) => return format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end),
    };
    match lex_sys_ir::check_declarations(&ast) {
        Ok(()) => "OK\n".to_owned(),
        Err(d) => format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end),
    }
}

#[allow(dead_code)]
fn main() {
    let mut source = Vec::new();
    std::io::stdin().read_to_end(&mut source).expect("stdin");
    print!("{}", listing(&String::from_utf8_lossy(&source)));
}
