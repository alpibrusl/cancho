// check.ls -- the lex-sys checker so far, written in lex-sys: the program.
//
// Stage 3b of the self-hosting spike (`docs/self-hosting.md` section 6, epic #295). Reads a
// source file on standard input, parses it with `ast.ls` and runs `pass1.ls` over the tree, and
// writes one line:
//
//     OK                        the declarations are well formed
//     ERR rule-tag start end    the first refusal, from the parser or from the checker
//     SKIP                      the declarations use something whose checks are not ported yet
//
// The oracle is `lex_sys_syntax::parse` followed by `lex_sys_ir::check_declarations`, which is
// the half of the Rust checker this is a port of; `tests/conformance/selfhost.rs` compares them
// over every program in the repository.
//
//     lex-sys run examples/selfhost/check.ls examples/selfhost/driver.ls examples/selfhost/listing.ls \
//         examples/selfhost/pass1.ls examples/selfhost/ast.ls examples/selfhost/kinds.ls \
//         examples/selfhost/lexcore.ls examples/selfhost/tables.ls --std < f.ls

import selfhost.driver;

fn main(world: World) -> [] int {
    return driver.drive(world, 1);
}
