module selfhost.checker;

// checker.ls -- the declarations half of the lex-sys checker, in the order the Rust checker makes
// its checks: imports, type declarations, foreign declarations, statics and signatures
// (`collect_declarations`, `crates/lex-sys-ir/src/lib.rs`). The checks themselves are
// `pass1.ls` and `foreign.ls`; this is the order.

import selfhost.ast;
import selfhost.pass1;
import selfhost.foreign;

// 0 if the declarations are well formed, else the refusal is in the state (`st[2..6]`): a rule.
pub fn check[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    pass1.check_imports(st, text);
    if ast.ok(st) {
        pass1.collect_types(st, text);
    }
    if ast.ok(st) {
        foreign.check_externs(st, text);
    }
    if ast.ok(st) {
        pass1.check_statics(st, text);
    }
    if ast.ok(st) {
        pass1.check_signatures(st, text);
    }
    if ast.ok(st) {
        return 0;
    }
    return 1;
}
