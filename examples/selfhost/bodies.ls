// bodies.ls -- the checker's answer for each function body: the program.
//
// Stage 3c of the self-hosting spike (`docs/self-hosting.md` section 6, epic #295). Reads a program
// on standard input, checks its declarations, and then writes one line for each function,
// `fn <start> <end>` and `OK`, the first refusal of its body, or `SKIP` for a body that uses something
// `body.ls` does not check yet. The oracle is `lex_sys_ir::check_bodies`.

import selfhost.driver;

fn main(world: World) -> [] int {
    return driver.drive(world, 2, false);
}
