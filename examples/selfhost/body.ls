module selfhost.body;

// body.ls -- function bodies in the lex-sys checker, written in lex-sys.
//
// Stage 3c of the self-hosting spike (`docs/self-hosting.md` section 6, epic #295): the second
// half of the checker, which `lex_sys_ir::check_bodies` exposes as an oracle that answers
// **per function**. A body's check reads nothing of any other function's body (a call is
// checked against the callee's *signature*), so each function is a case of its own, and the port
// can be right about some functions while it does not yet handle others.
//
// For each function the answer is one of
//
//     OK                        the Rust checker finds nothing wrong with the body
//     ERR rule start end        its first refusal
//     SKIP                      the body uses something this port does not check yet
//
// and `SKIP` is raised *where the Rust checker would have met the construct*, so a refusal found
// earlier in the function still counts and anything after it does not.
//
// What this slice handles: functions with no type or region parameters and only scalar
// parameters and result (`int`, `byte`, `float`, `f32`, `bool`); `let`, `var`, assignment to a
// local, an expression statement, `if`, `while`, `return`; integer, float, `f32` and `bool`
// literals, locals, the unary and binary operators, and calls to such functions. That is what
// type checking, constant folding (a literal operation that can only trap is refused, and that
// folds through nested literals), `unreachable-statement`, `missing-return` and the
// exactness of an empty row come down to when no value is a resource and no reference exists.
// Everything else is `SKIP`.
//
// A type is a scalar tag: 1 `int`, 2 `byte`, 3 `float`, 4 `f32`, 5 `bool`; 0 for none. The
// stack of live bindings is a region of the state (slots 20 and 21).

import selfhost.lexcore as lc;
import selfhost.ast;
import selfhost.kinds;
import selfhost.rules;
import selfhost.pass1;

// What an expression came to: its type, and whether it is a known integer (a literal, or a
// literal operation that folded) and which.
struct Ex {
    ty: int,
    known: bool,
    value: int,
}

fn plain(ty: int) -> [] Ex {
    return Ex { ty: ty, known: false, value: 0 };
}

fn constant(value: int) -> [] Ex {
    return Ex { ty: 1, known: true, value: value };
}

fn nothing() -> [] Ex {
    return Ex { ty: 0, known: false, value: 0 };
}

// Something the port does not check yet: the function's answer is `SKIP`.
fn skip[&s](st: &!s [int], from: int, to: int) -> [] Ex {
    ast.fail(st, rules.r_skip(), from, to);
    return nothing();
}

fn largest() -> [] int {
    return 9223372036854775807;
}

fn smallest() -> [] int {
    return 0 - 9223372036854775807 - 1;
}

// ------------------------------------------------------------- bindings ---

fn bind[&s](st: &!s [int], name: int, ty: int, mutable: bool) -> [] int {
    let at = st[20] + 3 * st[21];
    st[at] = name;
    st[at + 1] = ty;
    st[at + 2] = ast.flag(mutable);
    st[21] = st[21] + 1;
    return 0;
}

// The most recent live binding spelled like token `name`, or -1.
fn find[&s, &x](st: &!s [int], text: &x [byte], name: int) -> [] int {
    var i = st[21] - 1;
    while i >= 0 {
        if ast.same(st, text, st[st[20] + 3 * i], name) {
            return i;
        }
        i = i - 1;
    }
    return 0 - 1;
}

fn binding_type[&s](st: &!s [int], i: int) -> [] int {
    return st[st[20] + 3 * i + 1];
}

fn binding_mutable[&s](st: &!s [int], i: int) -> [] bool {
    return st[st[20] + 3 * i + 2] != 0;
}

// ---------------------------------------------------------------- types ---

// The scalar tag of the resolved type node `ty`, or 0 if it is not a scalar.
fn scalar[&s](st: &!s [int], ty: int) -> [] int {
    if ast.is_kind(st, ty, kinds.NK::TName) && ast.get(st, ty, 8) == 0 {
        return ast.get(st, ty, 10);
    }
    return 0;
}

// `expect_type`: the same scalar, or `type-mismatch` at `span`.
fn expect[&s](st: &!s [int], expected: int, found: int, from: int, to: int) -> [] int {
    if expected != found {
        ast.fail(st, rules.r_type_mismatch(), from, to);
    }
    return 0;
}

// ------------------------------------------------------ integer folding ---
//
// A literal operation that can only trap is refused, and folds through nested literals; a
// result that does not trap is itself a literal. Arithmetic here traps on overflow too, so each
// case is decided before it is taken.

fn add_fits(a: int, b: int) -> [] bool {
    if b > 0 {
        return a <= largest() - b;
    }
    return a >= smallest() - b;
}

fn sub_fits(a: int, b: int) -> [] bool {
    if b < 0 {
        return a <= largest() + b;
    }
    return a >= smallest() + b;
}

fn mul_fits(a: int, b: int) -> [] bool {
    if a == 0 || b == 0 {
        return true;
    }
    if a == 0 - 1 {
        return b != smallest();
    }
    if b == 0 - 1 {
        return a != smallest();
    }
    if a > 0 {
        if b > 0 {
            return a <= largest() / b;
        }
        return b >= smallest() / a;
    }
    if b > 0 {
        return a >= smallest() / b;
    }
    return a >= largest() / b;
}

// 2 to the power `n`, as the 64 bits it is: `n == 63` is the sign bit.
fn bit(n: int) -> [] int {
    var v = 1;
    var k = 0;
    while k < n {
        v = wrapping_mul(v, 2);
        k = k + 1;
    }
    return v;
}

// An integer operation over two known integers: the result, or a refusal (`trapped` is the
// answer's second half).
struct Folded {
    trapped: bool,
    value: int,
}

fn folded(value: int) -> [] Folded {
    return Folded { trapped: false, value: value };
}

fn trapped() -> [] Folded {
    return Folded { trapped: true, value: 0 };
}

// `op` is an index of `ast.op_name`.
fn fold_int(op: int, a: int, b: int) -> [] Folded {
    if op == 13 {
        if add_fits(a, b) {
            return folded(a + b);
        }
        return trapped();
    }
    if op == 14 {
        if sub_fits(a, b) {
            return folded(a - b);
        }
        return trapped();
    }
    if op == 15 {
        if mul_fits(a, b) {
            return folded(a * b);
        }
        return trapped();
    }
    if op == 16 {
        if b == 0 || a == smallest() && b == 0 - 1 {
            return trapped();
        }
        return folded(a / b);
    }
    if op == 17 {
        if b == 0 {
            return trapped();
        }
        if b == 0 - 1 {
            return folded(0);
        }
        return folded(a % b);
    }
    if op == 11 || op == 12 {
        if b < 0 || b > 63 {
            return trapped();
        }
        if op == 11 {
            return folded(wrapping_mul(a, bit(b)));
        }
        return folded(a >> b);
    }
    if op == 10 {
        return folded(a & b);
    }
    if op == 8 {
        return folded(a | b);
    }
    if op == 9 {
        return folded(a ^ b);
    }
    return folded(0);
}

// ---------------------------------------------------------- expressions ---

fn expr[&s, &x](st: &!s [int], text: &x [byte], id: int) -> [] Ex {
    let from = ast.nstart(st, id);
    let to = ast.nend(st, id);
    if ast.is_kind(st, id, kinds.NK::EInt) {
        return constant(ast.get(st, id, 4));
    }
    if ast.is_kind(st, id, kinds.NK::EFloat) {
        if ast.get(st, id, 4) == 0 {
            return plain(3);
        }
        // The `f32` suffix is edition 6's, and refused here and not in the parser, which does not
        // know the edition.
        if st[23] < 6 {
            ast.fail(st, rules.r_literal_form(), from, to);
            return nothing();
        }
        return plain(4);
    }
    if ast.is_kind(st, id, kinds.NK::EBool) {
        return plain(5);
    }
    if ast.is_kind(st, id, kinds.NK::EName) {
        return name_expr(st, text, id);
    }
    if ast.is_kind(st, id, kinds.NK::EUnary) {
        return unary_expr(st, text, id);
    }
    if ast.is_kind(st, id, kinds.NK::EBinary) {
        return binary_expr(st, text, id);
    }
    if ast.is_kind(st, id, kinds.NK::ECall) {
        return call_expr(st, text, id);
    }
    return skip(st, from, to);
}

fn name_expr[&s, &x](st: &!s [int], text: &x [byte], id: int) -> [] Ex {
    let from = ast.nstart(st, id);
    let to = ast.nend(st, id);
    let name = ast.get(st, id, 4);
    let i = find(st, text, name);
    if i >= 0 {
        return plain(binding_type(st, i));
    }
    // A name that is not a local may still be a `static`, a function used as a value, or a
    // builtin: those are values of types this slice does not check.
    if names_something(st, text, name) {
        return skip(st, from, to);
    }
    ast.fail(st, rules.r_unknown_name(), from, to);
    return nothing();
}

// Is `name` a static of the current function's module, a function of any module, or a builtin?
fn names_something[&s, &x](st: &!s [int], text: &x [byte], name: int) -> [] bool {
    var it = st[15];
    while it >= 0 {
        if ast.is_kind(st, it, kinds.NK::IStatic) && ast.same(st, text, ast.get(st, it, 4), name) && ast.get(st, it, 13) == st[22] {
            return true;
        }
        if ast.is_kind(st, it, kinds.NK::IFn) && ast.same(st, text, ast.get(st, it, 4), name) {
            return true;
        }
        it = ast.next(st, it);
    }
    return pass1.builtin_named(st, text, name);
}

fn unary_expr[&s, &x](st: &!s [int], text: &x [byte], id: int) -> [] Ex {
    let from = ast.nstart(st, id);
    let to = ast.nend(st, id);
    let op = ast.get(st, id, 4);
    let operand_id = ast.get(st, id, 5);
    if op == 3 {
        return skip(st, from, to);
    }
    let operand = expr(st, text, operand_id);
    if !ast.ok(st) {
        return nothing();
    }
    let of = ast.nstart(st, operand_id);
    let ot = ast.nend(st, operand_id);
    if op == 0 {
        if operand.ty != 1 && operand.ty != 3 && operand.ty != 4 {
            ast.fail(st, rules.r_operator_type_mismatch(), of, ot);
            return nothing();
        }
        if operand.known && operand.ty == 1 {
            if operand.value == smallest() {
                ast.fail(st, rules.r_constant_traps(), from, to);
                return nothing();
            }
            return constant(0 - operand.value);
        }
        return plain(operand.ty);
    }
    if op == 1 {
        expect(st, 5, operand.ty, of, ot);
        return plain(5);
    }
    expect(st, 1, operand.ty, of, ot);
    if operand.known {
        return constant(~operand.value);
    }
    return plain(1);
}

fn binary_expr[&s, &x](st: &!s [int], text: &x [byte], id: int) -> [] Ex {
    let from = ast.nstart(st, id);
    let to = ast.nend(st, id);
    let op = ast.get(st, id, 4);
    let lhs_id = ast.get(st, id, 5);
    let rhs_id = ast.get(st, id, 6);
    let lf = ast.nstart(st, lhs_id);
    let lt = ast.nend(st, lhs_id);
    let l = expr(st, text, lhs_id);
    if !ast.ok(st) {
        return nothing();
    }
    let r = expr(st, text, rhs_id);
    if !ast.ok(st) {
        return nothing();
    }
    // Both sides agree first, then the operator says what it accepts.
    expect(st, l.ty, r.ty, ast.nstart(st, rhs_id), ast.nend(st, rhs_id));
    if !ast.ok(st) {
        return nothing();
    }
    let operand = l.ty;
    let arithmetic = operand == 1 || operand == 3 || operand == 4;
    if op >= 13 && op <= 16 {
        if !arithmetic {
            ast.fail(st, rules.r_operator_type_mismatch(), lf, lt);
            return nothing();
        }
    }
    if op == 17 || op >= 8 && op <= 12 {
        expect(st, 1, operand, lf, lt);
    }
    if op == 0 || op == 1 {
        expect(st, 5, operand, lf, lt);
    }
    if op >= 4 && op <= 7 {
        if !arithmetic {
            ast.fail(st, rules.r_operator_type_mismatch(), lf, lt);
            return nothing();
        }
    }
    if !ast.ok(st) {
        return nothing();
    }
    // A comparison and a short-circuit answer a `bool`; the rest answer their operand.
    var result = operand;
    if op <= 7 {
        result = 5;
    }
    if l.known && r.known && operand == 1 {
        let f = fold_int(op, l.value, r.value);
        if f.trapped {
            ast.fail(st, rules.r_constant_traps(), from, to);
            return nothing();
        }
        if result == 1 {
            return constant(f.value);
        }
    }
    return plain(result);
}

fn call_expr[&s, &x](st: &!s [int], text: &x [byte], id: int) -> [] Ex {
    let from = ast.nstart(st, id);
    let to = ast.nend(st, id);
    let qualifier = ast.get(st, id, 4);
    let callee = ast.get(st, id, 5);
    let nargs = ast.get(st, id, 7);
    // Only an unqualified name can be a local.
    if qualifier < 0 {
        let local = find(st, text, callee);
        if local >= 0 {
            ast.fail(st, rules.r_not_a_function(), from, to);
            return nothing();
        }
    }
    let unit_of = st[22];
    let target = pass1.resolve_module(st, text, unit_of, qualifier);
    if target < 0 {
        ast.fail(st, rules.r_module_not_imported(), from, to);
        return nothing();
    }
    // A builtin the file's edition can name comes first, then a foreign function: not checked.
    if pass1.builtin_name(st, text, callee, st[23]) {
        return skip(st, from, to);
    }
    var it = st[15];
    while it >= 0 {
        if ast.is_kind(st, it, kinds.NK::IExtern) && ast.get(st, it, 13) == target && ast.same(st, text, ast.get(st, it, 4), callee) {
            return skip(st, from, to);
        }
        it = ast.next(st, it);
    }
    var found = 0 - 1;
    it = st[15];
    while it >= 0 && found < 0 {
        if ast.is_kind(st, it, kinds.NK::IFn) && ast.get(st, it, 13) == target && ast.same(st, text, ast.get(st, it, 4), callee) {
            found = it;
        }
        it = ast.next(st, it);
    }
    if found < 0 {
        ast.fail(st, rules.r_not_a_function(), from, to);
        return nothing();
    }
    if target != unit_of && ast.get(st, found, 5) == 0 {
        ast.fail(st, rules.r_not_public(), from, to);
        return nothing();
    }
    // A callee that is generic, takes regions, declares effects, or is not scalar throughout.
    if !plain_function(st, found) {
        return skip(st, from, to);
    }
    if ast.get(st, found, 8) != nargs {
        ast.fail(st, rules.r_arity_mismatch(), from, to);
        return nothing();
    }
    var param = ast.get(st, found, 7);
    var arg = ast.get(st, id, 6);
    while param >= 0 && ast.ok(st) {
        let a = expr(st, text, arg);
        if ast.ok(st) {
            expect(st, scalar(st, ast.get(st, param, 5)), a.ty, ast.nstart(st, arg), ast.nend(st, arg));
        }
        param = ast.next(st, param);
        arg = ast.next(st, arg);
    }
    if !ast.ok(st) {
        return nothing();
    }
    return plain(scalar(st, ast.get(st, found, 10)));
}

// A function with no type or region parameters, an empty row, and only scalar parameters and
// result: the functions this slice checks, and the callees it can call.
fn plain_function[&s](st: &!s [int], fn_item: int) -> [] bool {
    if ast.get(st, fn_item, 6) >= 0 {
        return false;
    }
    if pass1.is_code(st, ast.get(st, fn_item, 9) + 1, lc.Tok::RBracket) == false {
        return false;
    }
    var param = ast.get(st, fn_item, 7);
    while param >= 0 {
        if scalar(st, ast.get(st, param, 5)) == 0 {
            return false;
        }
        param = ast.next(st, param);
    }
    return scalar(st, ast.get(st, fn_item, 10)) != 0;
}

// ----------------------------------------------------------- statements ---

// Does a block whose first statement is `head`, of `n`, end in a statement that returns on every
// path (`terminates`)? A `while` never counts.
fn terminates[&s](st: &!s [int], head: int, n: int) -> [] bool {
    if n == 0 {
        return false;
    }
    var last = head;
    var i = 1;
    while i < n {
        last = ast.next(st, last);
        i = i + 1;
    }
    if ast.is_kind(st, last, kinds.NK::SReturn) {
        return true;
    }
    if ast.is_kind(st, last, kinds.NK::SIf) {
        if ast.get(st, last, 8) <= 0 {
            return false;
        }
        return terminates(st, ast.get(st, last, 5), ast.get(st, last, 6)) && terminates(st, ast.get(st, last, 7), ast.get(st, last, 8));
    }
    return false;
}

fn block[&s, &x](st: &!s [int], text: &x [byte], head: int, n: int, ret: int) -> [] int {
    let mark = st[21];
    var at = head;
    var i = 0;
    while i < n && ast.ok(st) {
        if i > 0 && terminates(st, head, i) {
            ast.fail(st, rules.r_unreachable_statement(), ast.nstart(st, at), ast.nend(st, at));
        } else {
            stmt(st, text, at, ret);
        }
        at = ast.next(st, at);
        i = i + 1;
    }
    st[21] = mark;
    return 0;
}

fn condition[&s, &x](st: &!s [int], text: &x [byte], id: int) -> [] int {
    let found = expr(st, text, id);
    if ast.ok(st) {
        expect(st, 5, found.ty, ast.nstart(st, id), ast.nend(st, id));
    }
    return 0;
}

fn stmt[&s, &x](st: &!s [int], text: &x [byte], id: int, ret: int) -> [] int {
    let from = ast.nstart(st, id);
    let to = ast.nend(st, id);
    if ast.is_kind(st, id, kinds.NK::SLet) {
        let value = ast.get(st, id, 7);
        let found = expr(st, text, value);
        if !ast.ok(st) {
            return 0;
        }
        var declared = found.ty;
        let written = ast.get(st, id, 6);
        if written >= 0 {
            pass1.resolve(st, text, written, st[22], st[23], 0 - 1, true, false);
            if !ast.ok(st) {
                return 0;
            }
            declared = scalar(st, written);
            if declared == 0 {
                skip(st, from, to);
                return 0;
            }
            expect(st, declared, found.ty, ast.nstart(st, value), ast.nend(st, value));
        }
        if ast.ok(st) {
            bind(st, ast.get(st, id, 5), declared, ast.get(st, id, 4) != 0);
        }
        return 0;
    }
    if ast.is_kind(st, id, kinds.NK::SAssign) {
        let value = ast.get(st, id, 5);
        let found = expr(st, text, value);
        if !ast.ok(st) {
            return 0;
        }
        let place = ast.get(st, id, 4);
        if !ast.is_kind(st, place, kinds.NK::EName) {
            skip(st, from, to);
            return 0;
        }
        let i = find(st, text, ast.get(st, place, 4));
        if i < 0 {
            ast.fail(st, rules.r_unknown_name(), from, to);
            return 0;
        }
        if !binding_mutable(st, i) {
            ast.fail(st, rules.r_assign_to_immutable(), from, to);
            return 0;
        }
        expect(st, binding_type(st, i), found.ty, ast.nstart(st, value), ast.nend(st, value));
        return 0;
    }
    if ast.is_kind(st, id, kinds.NK::SExpr) {
        expr(st, text, ast.get(st, id, 4));
        return 0;
    }
    if ast.is_kind(st, id, kinds.NK::SIf) {
        condition(st, text, ast.get(st, id, 4));
        if ast.ok(st) {
            block(st, text, ast.get(st, id, 5), ast.get(st, id, 6), ret);
        }
        if ast.ok(st) && ast.get(st, id, 8) >= 0 {
            block(st, text, ast.get(st, id, 7), ast.get(st, id, 8), ret);
        }
        return 0;
    }
    if ast.is_kind(st, id, kinds.NK::SWhile) {
        condition(st, text, ast.get(st, id, 4));
        if ast.ok(st) {
            block(st, text, ast.get(st, id, 5), ast.get(st, id, 6), ret);
        }
        return 0;
    }
    if ast.is_kind(st, id, kinds.NK::SReturn) {
        let value = ast.get(st, id, 4);
        let found = expr(st, text, value);
        if ast.ok(st) {
            expect(st, ret, found.ty, ast.nstart(st, value), ast.nend(st, value));
        }
        return 0;
    }
    skip(st, from, to);
    return 0;
}

// ------------------------------------------------------------ functions ---

// The answer for the function `fn_item`: 0 `OK`, 1 a refusal (in `st[3..6]`), 2 `SKIP`. The
// state's failure is cleared first and left as the answer.
pub fn check_function[&s, &x](st: &!s [int], text: &x [byte], fn_item: int) -> [] int {
    st[2] = 0;
    st[21] = 0;
    st[22] = ast.get(st, fn_item, 13);
    st[23] = ast.get(st, fn_item, 14);
    let from = ast.nstart(st, fn_item);
    let to = ast.nend(st, fn_item);
    if ast.get(st, fn_item, 6) >= 0 || !all_scalar(st, fn_item) {
        ast.fail(st, rules.r_skip(), from, to);
        return 2;
    }
    var param = ast.get(st, fn_item, 7);
    while param >= 0 {
        bind(st, ast.get(st, param, 4), scalar(st, ast.get(st, param, 5)), false);
        param = ast.next(st, param);
    }
    let ret = scalar(st, ast.get(st, fn_item, 10));
    block(st, text, ast.get(st, fn_item, 11), ast.get(st, fn_item, 12), ret);
    if ast.ok(st) {
        // The row is exact: nothing here performs anything, so a declared label is decoration.
        if !pass1.is_code(st, ast.get(st, fn_item, 9) + 1, lc.Tok::RBracket) {
            ast.fail(st, rules.r_effect_declared_not_performed(), from, to);
        }
    }
    if ast.ok(st) && !terminates(st, ast.get(st, fn_item, 11), ast.get(st, fn_item, 12)) {
        ast.fail(st, rules.r_missing_return(), from, to);
    }
    if ast.ok(st) {
        return 0;
    }
    if st[3] == rules.r_skip() {
        return 2;
    }
    return 1;
}

fn all_scalar[&s](st: &!s [int], fn_item: int) -> [] bool {
    var param = ast.get(st, fn_item, 7);
    while param >= 0 {
        if scalar(st, ast.get(st, param, 5)) == 0 {
            return false;
        }
        param = ast.next(st, param);
    }
    return scalar(st, ast.get(st, fn_item, 10)) != 0;
}
