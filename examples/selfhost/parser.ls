// parser.ls -- the lex-sys parser, written in lex-sys: the program.
//
// Stages 2 and 3a of the self-hosting spike (`docs/self-hosting.md` section 6, epic #295).
// Reads a source file on standard input and writes the syntax tree `ast.ls` builds as the
// listing `crates/lex-sys-syntax/examples/dump_ast.rs` writes from the Rust AST, or the one line
// `ERR rule-tag start end` if the Rust parser would have refused it.
//
//     lex-sys run examples/selfhost/parser.ls examples/selfhost/driver.ls examples/selfhost/listing.ls \
//         examples/selfhost/pass1.ls examples/selfhost/ast.ls examples/selfhost/kinds.ls \
//         examples/selfhost/lexcore.ls examples/selfhost/tables.ls --std < f.ls

import selfhost.driver;

fn main(world: World) -> [] int {
    return driver.drive(world, 0, false);
}
