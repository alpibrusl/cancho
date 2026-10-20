//! The oracle for `examples/selfhost/ids.cho` (`docs/self-hosting.md`): the identity of every
//! declaration of a program of several files, as `cancho ids` writes them.
//!
//! The conformance test (`crates/cancho/tests/conformance/selfhost.rs`) includes this file and calls
//! `identities`. Run alone it reads one file on standard input.
//!
//!     cargo run -q -p cancho-id --example dump_ids < some_file.cho

use std::fmt::Write;
use std::io::Read;

use cancho_syntax::{Ast, SourceMap, parse_into};

/// The lines of `cancho ids` for the files parsed in turn into one program (each at its own
/// base offset, as the compiler does), or the one refusal line of the parser. The program is not
/// checked first, which `cancho ids` also does.
pub fn identities(files: &[String]) -> String {
    let mut map = SourceMap::new();
    let mut ast = Ast::new();
    let bases: Vec<u32> = files.iter().map(|text| map.add("file", text.clone())).collect();
    for (text, base) in files.iter().zip(bases) {
        if let Err(d) = parse_into(&mut ast, text, base) {
            return format!("ERR {} {} {}\n", d.rule.tag(), d.span.start, d.span.end);
        }
    }
    let identities = cancho_id::identify(&ast);
    let mut out = String::new();
    for decl in &identities.types {
        let _ = writeln!(out, "type {:<24} {}", decl.name, decl.id);
    }
    for func in &identities.functions {
        let _ = writeln!(out, "sig  {:<24} {}", func.name, func.sig);
        let _ = writeln!(out, "body {:<24} {}", func.name, func.body);
    }
    out
}

#[allow(dead_code)]
fn main() {
    let mut source = String::new();
    std::io::stdin().read_to_string(&mut source).expect("standard input is text");
    print!("{}", identities(&[source]));
}
