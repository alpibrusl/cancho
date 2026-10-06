// parser.ls -- the lex-sys parser, written in lex-sys.
//
// Stage 2 of the self-hosting spike (`docs/self-hosting.md`, epic #295): a port of
// `crates/lex-sys-syntax/src/parser/{expr,stmt,items}.rs`, so that a difference in behaviour
// is a finding about the language or about the Rust parser and not about a redesign.
//
// It does not build a tree. It writes the tree the Rust parser would have built as a
// **postfix listing**, one node per line, children before their parent, at the moment the
// node is made (which is the moment the Rust parser pushes it into its arena). A node says
// how many children of each kind it takes off the stack, so the listing needs no brackets:
//
//     E.Int 7 @ 4 5
//     E.Name x @ 8 9
//     E.Binary Add @ 4 9
//
// or, if the Rust parser would have refused the input, one line `ERR rule-tag start end`.
// `crates/lex-sys-syntax/examples/dump_ast.rs` is the oracle, `diff.sh` compares them.
//
//     lex-sys run examples/selfhost/parser.ls examples/selfhost/lexcore.ls --std < f.ls
//
// Not compared: float values (the listing has the literal's span and whether it is an
// `f32`, not its bits; a literal that rounds to infinity is refused), symbol ids and message text. Strings are written as hex.
//
// How it is written, because the language shapes it:
//
// * There are no integer constants, so a token is stored as its `lexcore.code`, an
//   integer, and compared with `look(st, n, lc.Tok::Comma)`.
// * There is no early return from an error, so a refusal is **sticky**: the first one is
//   recorded in the state and every later call does nothing (`kind` answers end of file
//   once the parser has failed, so every loop ends), as `?` would have.
// * Nothing is allocated per node. The state, the tokens and the imports share one
//   boxed slice of integers, `st`: slots 0..16 are the parser's fields, then three slots
//   per token, then the import list.
// * The refusal has to void the listing, so the parser runs twice, silently and then
//   aloud, as the lexer does.

import std.io as console;
import std.buffer;
import selfhost.lexcore as lc;

// A span: what a parse function answers for the node it made.
struct Sp {
    start: int,
    end: int,
}

// A block: how many statements, and where its closing brace ends.
struct Bk {
    count: int,
    end: int,
}

// The `[...]` of a declaration: where it starts (-1 if there is none) and what it held.
struct Dp {
    start: int,
    generics: int,
    regions: int,
    bounds: int,
}

// ------------------------------------------------------------ the state ---
//
//   0 pos           the index of the token the parser is looking at
//   1 no_struct     1 while parsing an `if` or `while` condition
//   2 failed        1 once a refusal has been recorded
//   3 rule          the refusal's rule, 4 its start, 5 its end
//   6 edition       this file's edition
//   7 show          1 on the pass that prints
//   8 tokens        how many tokens there are, the end-of-file token included
//   9 module        0 for the root, 1 for the module the file declares
//  10 module_first  the index of the declared module path's first token
//  11 module_count  and how many segments it has
//  12 imports       how many imports are recorded
//  16 ...           three slots a token: its code, its start, its end
//  then the imports, one slot each: the `import` token's index, doubled, plus the module

fn tokens_at() -> [] int {
    return 16;
}

fn ok[&s](st: &!s [int]) -> [] bool {
    return st[2] == 0;
}

fn eof_code() -> [] int {
    return lc.code(lc.Tok::Eof);
}

// ---------------------------------------------------------------- rules ---

fn r_type_mismatch() -> [] int {
    return 3;
}

fn r_literal_out_of_range() -> [] int {
    return 4;
}

fn r_unknown_edition() -> [] int {
    return 5;
}

fn r_program_shape() -> [] int {
    return 6;
}

fn r_pattern_shape() -> [] int {
    return 7;
}

fn r_foreign_declaration() -> [] int {
    return 8;
}

fn r_region_mismatch() -> [] int {
    return 9;
}

fn r_mode_bound_violated() -> [] int {
    return 10;
}

fn r_unknown_name() -> [] int {
    return 11;
}

fn rule_tag(r: int) -> [] &static [byte] {
    if r < 3 {
        return lc.rule_name(r);
    }
    if r == 3 {
        return "type-mismatch";
    }
    if r == 4 {
        return "literal-out-of-range";
    }
    if r == 5 {
        return "unknown-edition";
    }
    if r == 6 {
        return "program-shape";
    }
    if r == 7 {
        return "pattern-shape";
    }
    if r == 8 {
        return "foreign-declaration";
    }
    if r == 9 {
        return "region-mismatch";
    }
    if r == 10 {
        return "mode-bound-violated";
    }
    return "unknown-name";
}

// The first refusal is the one that counts.
fn fail[&s](st: &!s [int], rule: int, from: int, to: int) -> [] int {
    if st[2] == 0 {
        st[2] = 1;
        st[3] = rule;
        st[4] = from;
        st[5] = to;
    }
    return 0;
}

// ------------------------------------------------------- token plumbing ---

fn tstart[&s](st: &!s [int], i: int) -> [] int {
    return st[16 + 3 * i + 1];
}

fn tend[&s](st: &!s [int], i: int) -> [] int {
    return st[16 + 3 * i + 2];
}

// The code of token `i`, whatever the parser's state.
fn code_at[&s](st: &!s [int], i: int) -> [] int {
    return st[16 + 3 * i];
}

// The kind of the token `n` ahead, saturating at end of file. After a refusal
// everything is end of file, so every loop that waits for a closer ends.
fn kind[&s](st: &!s [int], n: int) -> [] int {
    if st[2] != 0 {
        return eof_code();
    }
    var i = st[0] + n;
    if i > st[8] - 1 {
        i = st[8] - 1;
    }
    return code_at(st, i);
}

fn look[&s](st: &!s [int], n: int, t: lc.Tok) -> [] bool {
    return kind(st, n) == lc.code(t);
}

// The token consumed, by index. End of file is never consumed.
fn bump[&s](st: &!s [int]) -> [] int {
    let i = st[0];
    if kind(st, 0) != eof_code() {
        st[0] = i + 1;
    }
    return i;
}

fn eat[&s](st: &!s [int], t: lc.Tok) -> [] bool {
    if look(st, 0, t) {
        bump(st);
        return true;
    }
    return false;
}

fn expect[&s](st: &!s [int], t: lc.Tok) -> [] int {
    if look(st, 0, t) {
        return bump(st);
    }
    fail(st, r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
    return st[0];
}

fn ident[&s](st: &!s [int]) -> [] int {
    return expect(st, lc.Tok::Ident);
}

// Is the token `n` ahead a name spelled `word`?
fn word[&s, &x](st: &!s [int], text: &x [byte], n: int, spelled: &static [byte]) -> [] bool {
    if !look(st, n, lc.Tok::Ident) {
        return false;
    }
    let i = st[0] + n;
    return lc.spells(text, tstart(st, i), tend(st, i), spelled);
}

// Still inside a list that ends at `closer`.
fn more[&s](st: &!s [int], closer: lc.Tok) -> [] bool {
    return ok(st) && !look(st, 0, closer);
}

// `bracketed`: inside brackets a struct literal is possible again.
fn open_brackets[&s](st: &!s [int]) -> [] int {
    let outer = st[1];
    st[1] = 0;
    return outer;
}

fn close_brackets[&s](st: &!s [int], outer: int) -> [] int {
    st[1] = outer;
    return 0;
}

fn sp(a: int, b: int) -> [] Sp {
    return Sp { start: a, end: b };
}

fn nothing() -> [] Sp {
    return Sp { start: 0, end: 0 };
}

// ---------------------------------------------------------------- output ---

fn w[&i, &s, &r](io: &!i Io, st: &!s [int], str: &r [byte]) -> [io_write] int {
    if st[7] == 1 {
        console.write_all(io, str);
    }
    return 0;
}

fn wn[&i, &s](io: &!i Io, st: &!s [int], n: int) -> [io_write] int {
    if st[7] == 1 {
        console.print_int(io, n);
    }
    return 0;
}

fn wname[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], i: int) -> [io_write] int {
    if st[7] == 1 {
        console.write_all(io, text[tstart(st, i)..tend(st, i)]);
    }
    return 0;
}

// A qualifier, or `-` for none (-1).
fn wqual[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], q: int) -> [io_write] int {
    if q < 0 {
        return w(io, st, "-");
    }
    return wname(io, st, text, q);
}

// ` @ start end` and the end of the line.
fn wspan[&i, &s](io: &!i Io, st: &!s [int], from: int, to: int) -> [io_write] int {
    w(io, st, " @ ");
    wn(io, st, from);
    w(io, st, " ");
    wn(io, st, to);
    return w(io, st, "\n");
}

fn wbool[&i, &s](io: &!i Io, st: &!s [int], b: bool) -> [io_write] int {
    if b {
        return w(io, st, "1");
    }
    return w(io, st, "0");
}

fn whex_digit[&i, &s](io: &!i Io, st: &!s [int], d: int) -> [io_write] int {
    let digits = "0123456789abcdef";
    return w(io, st, digits[d..d + 1]);
}

// The decoded bytes of a string literal, as hex; `-` for the empty string.
fn wstring[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], i: int) -> [io_write] int {
    if st[7] != 1 {
        return 0;
    }
    let to = tend(st, i) - 1;
    var j = tstart(st, i) + 1;
    var any = false;
    while j < to {
        var c = lc.at(text, j);
        if c == '\\' {
            j = j + 1;
            let e = lc.at(text, j);
            c = e;
            if e == 'n' {
                c = 10;
            }
            if e == 'r' {
                c = 13;
            }
            if e == 't' {
                c = 9;
            }
            if e == '0' {
                c = 0;
            }
        }
        whex_digit(io, st, c / 16);
        whex_digit(io, st, c % 16);
        any = true;
        j = j + 1;
    }
    if !any {
        w(io, st, "-");
    }
    return 0;
}

// `mod=a.b ed=N`, what an item line ends with.
fn wplace[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    w(io, st, " mod=");
    wmodule(io, st, text, st[9]);
    w(io, st, " ed=");
    wn(io, st, st[6]);
    return 0;
}

// A module's path, `a.b`, or `-` for the root.
fn wmodule[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], m: int) -> [io_write] int {
    if m == 0 {
        return w(io, st, "-");
    }
    var at = st[10];
    wname(io, st, text, at);
    var n = 1;
    while n < st[11] {
        w(io, st, ".");
        wname(io, st, text, at + 2);
        at = at + 2;
        n = n + 1;
    }
    return 0;
}

// ------------------------------------------------------------- literals ---

// An integer literal's value, refusing what does not fit in 64 signed bits. The Rust parser
// reads the magnitude as a `u64` and compares it with the limit; here the magnitude is
// accumulated as a *negative* number, which holds one more value than a positive one does,
// and each step is checked before it is taken, because arithmetic here traps on overflow.
fn int_value[&s, &x](st: &!s [int], text: &x [byte], i: int, negated: bool) -> [] int {
    let from = tstart(st, i);
    let to = tend(st, i);
    if lc.at(text, from) == '\'' {
        var value = lc.at(text, from + 1);
        if value == '\\' {
            let e = lc.at(text, from + 2);
            value = e;
            if e == 'n' {
                value = 10;
            }
            if e == 'r' {
                value = 13;
            }
            if e == 't' {
                value = 9;
            }
            if e == '0' {
                value = 0;
            }
        }
        if negated {
            return 0 - value;
        }
        return value;
    }
    let hex = lc.at(text, from) == '0' && lc.at(text, from + 1) == 'x';
    var base = 10;
    var j = from;
    if hex {
        base = 16;
        j = from + 2;
    }
    let largest = 9223372036854775807;
    var bound = 0 - largest;
    if negated {
        bound = bound - 1;
    }
    var acc = 0;
    var bad = false;
    while j < to && !bad {
        let c = lc.at(text, j);
        if c != '_' {
            var d = c - '0';
            if c >= 'a' {
                d = c - 'a' + 10;
            } else if c >= 'A' {
                d = c - 'A' + 10;
            }
            if acc < (bound + d) / base {
                bad = true;
            } else {
                acc = acc * base - d;
            }
        }
        j = j + 1;
    }
    if bad {
        fail(st, r_literal_out_of_range(), from, to);
        return 0;
    }
    if negated {
        return acc;
    }
    return 0 - acc;
}

// ---------------------------------------------------------------- types ---

// `[io_write, ffi("libc")]`, written as the line `H.Effects [io_write,ffi:6c696263]`.
fn effect_row[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    expect(st, lc.Tok::LBracket);
    w(io, st, "H.Effects [");
    var first = true;
    var go = true;
    while go && more(st, lc.Tok::RBracket) {
        if !first {
            w(io, st, ",");
        }
        first = false;
        let name = ident(st);
        wname(io, st, text, name);
        if eat(st, lc.Tok::LParen) {
            let lit = expect(st, lc.Tok::Str);
            w(io, st, ":");
            wstring(io, st, text, lit);
            expect(st, lc.Tok::RParen);
        }
        go = eat(st, lc.Tok::Comma);
    }
    expect(st, lc.Tok::RBracket);
    return w(io, st, "]\n");
}

fn type_list[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], closer: lc.Tok) -> [io_write] int {
    var n = 0;
    var go = true;
    while go && more(st, closer) {
        type_expr(io, st, text);
        n = n + 1;
        go = eat(st, lc.Tok::Comma);
    }
    return n;
}

fn type_expr[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let t = st[0];
    let begin = tstart(st, t);
    if eat(st, lc.Tok::Amp) {
        let unique = eat(st, lc.Tok::Bang);
        let reg = ident(st);
        let inner = type_expr(io, st, text);
        w(io, st, "T.Ref ");
        wbool(io, st, unique);
        w(io, st, " ");
        wname(io, st, text, reg);
        wspan(io, st, begin, inner.end);
        return sp(begin, inner.end);
    }
    if eat(st, lc.Tok::Fn) {
        expect(st, lc.Tok::LParen);
        let n = type_list(io, st, text, lc.Tok::RParen);
        expect(st, lc.Tok::RParen);
        expect(st, lc.Tok::Arrow);
        effect_row(io, st, text);
        let ret = type_expr(io, st, text);
        w(io, st, "T.Fn ");
        wn(io, st, n);
        wspan(io, st, begin, ret.end);
        return sp(begin, ret.end);
    }
    if eat(st, lc.Tok::LParen) {
        let n = type_list(io, st, text, lc.Tok::RParen);
        let close = expect(st, lc.Tok::RParen);
        w(io, st, "T.Tuple ");
        wn(io, st, n);
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    if eat(st, lc.Tok::LBracket) {
        type_expr(io, st, text);
        let close = expect(st, lc.Tok::RBracket);
        w(io, st, "T.Slice");
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    let first = ident(st);
    var qualifier = 0 - 1;
    var name = first;
    if eat(st, lc.Tok::Dot) {
        qualifier = first;
        name = ident(st);
    }
    var nargs = 0;
    var end = tend(st, t);
    if eat(st, lc.Tok::LParen) {
        let lit = expect(st, lc.Tok::Str);
        let close = expect(st, lc.Tok::RParen);
        w(io, st, "T.Lit ");
        wstring(io, st, text, lit);
        wspan(io, st, begin, tend(st, close));
        w(io, st, "T.Name ");
        wqual(io, st, text, qualifier);
        w(io, st, " ");
        wname(io, st, text, name);
        w(io, st, " 1");
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    if eat(st, lc.Tok::LBracket) {
        nargs = type_list(io, st, text, lc.Tok::RBracket);
        let close = expect(st, lc.Tok::RBracket);
        end = tend(st, close);
    }
    w(io, st, "T.Name ");
    wqual(io, st, text, qualifier);
    w(io, st, " ");
    wname(io, st, text, name);
    w(io, st, " ");
    wn(io, st, nargs);
    wspan(io, st, begin, end);
    return sp(begin, end);
}

// ---------------------------------------------------------- expressions ---

// The operator `level` of the precedence table has at the current token, as an index into
// `op_name`, or -1.
fn op_at[&s](st: &!s [int], level: int) -> [] int {
    if level == 0 {
        if look(st, 0, lc.Tok::PipePipe) {
            return 0;
        }
    }
    if level == 1 {
        if look(st, 0, lc.Tok::AmpAmp) {
            return 1;
        }
    }
    if level == 2 {
        if look(st, 0, lc.Tok::EqEq) {
            return 2;
        }
        if look(st, 0, lc.Tok::BangEq) {
            return 3;
        }
    }
    if level == 3 {
        if look(st, 0, lc.Tok::Lt) {
            return 4;
        }
        if look(st, 0, lc.Tok::LtEq) {
            return 5;
        }
        if look(st, 0, lc.Tok::Gt) {
            return 6;
        }
        if look(st, 0, lc.Tok::GtEq) {
            return 7;
        }
    }
    if level == 4 {
        if look(st, 0, lc.Tok::Pipe) {
            return 8;
        }
    }
    if level == 5 {
        if look(st, 0, lc.Tok::Caret) {
            return 9;
        }
    }
    if level == 6 {
        if look(st, 0, lc.Tok::Amp) {
            return 10;
        }
    }
    if level == 7 {
        if look(st, 0, lc.Tok::LtLt) {
            return 11;
        }
        if look(st, 0, lc.Tok::GtGt) {
            return 12;
        }
    }
    if level == 8 {
        if look(st, 0, lc.Tok::Plus) {
            return 13;
        }
        if look(st, 0, lc.Tok::Minus) {
            return 14;
        }
    }
    if level == 9 {
        if look(st, 0, lc.Tok::Star) {
            return 15;
        }
        if look(st, 0, lc.Tok::Slash) {
            return 16;
        }
        if look(st, 0, lc.Tok::Percent) {
            return 17;
        }
    }
    return 0 - 1;
}

fn op_name(op: int) -> [] &static [byte] {
    if op == 0 {
        return "Or";
    }
    if op == 1 {
        return "And";
    }
    if op == 2 {
        return "Eq";
    }
    if op == 3 {
        return "Ne";
    }
    if op == 4 {
        return "Lt";
    }
    if op == 5 {
        return "Le";
    }
    if op == 6 {
        return "Gt";
    }
    if op == 7 {
        return "Ge";
    }
    if op == 8 {
        return "BitOr";
    }
    if op == 9 {
        return "BitXor";
    }
    if op == 10 {
        return "BitAnd";
    }
    if op == 11 {
        return "Shl";
    }
    if op == 12 {
        return "Shr";
    }
    if op == 13 {
        return "Add";
    }
    if op == 14 {
        return "Sub";
    }
    if op == 15 {
        return "Mul";
    }
    if op == 16 {
        return "Div";
    }
    return "Rem";
}

fn expr[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    return binary_level(io, st, text, 0);
}

fn binary_level[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], level: int) -> [io_write] Sp {
    if level == 10 {
        return unary(io, st, text);
    }
    var lhs = binary_level(io, st, text, level + 1);
    var go = true;
    while go && ok(st) {
        let op = op_at(st, level);
        if op < 0 {
            go = false;
        } else {
            bump(st);
            let rhs = binary_level(io, st, text, level + 1);
            w(io, st, "E.Binary ");
            w(io, st, op_name(op));
            wspan(io, st, lhs.start, rhs.end);
            lhs = sp(lhs.start, rhs.end);
        }
    }
    return lhs;
}

fn unary_line[&i, &s](io: &!i Io, st: &!s [int], name: &static [byte], from: int, to: int) -> [io_write] int {
    w(io, st, "E.Unary ");
    w(io, st, name);
    return wspan(io, st, from, to);
}

fn unary[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    if look(st, 0, lc.Tok::Bang) {
        let op = bump(st);
        let operand = unary(io, st, text);
        unary_line(io, st, "Not", tstart(st, op), operand.end);
        return sp(tstart(st, op), operand.end);
    }
    if look(st, 0, lc.Tok::Star) {
        let op = bump(st);
        let operand = unary(io, st, text);
        unary_line(io, st, "Deref", tstart(st, op), operand.end);
        return sp(tstart(st, op), operand.end);
    }
    if look(st, 0, lc.Tok::Tilde) {
        let op = bump(st);
        let operand = unary(io, st, text);
        unary_line(io, st, "BitNot", tstart(st, op), operand.end);
        return sp(tstart(st, op), operand.end);
    }
    if look(st, 0, lc.Tok::Minus) {
        let minus = bump(st);
        if look(st, 0, lc.Tok::Int) {
            let tok = bump(st);
            let value = int_value(st, text, tok, true);
            w(io, st, "E.Int ");
            wn(io, st, value);
            wspan(io, st, tstart(st, minus), tend(st, tok));
            return sp(tstart(st, minus), tend(st, tok));
        }
        if look(st, 0, lc.Tok::Float) {
            let tok = bump(st);
            float_literal(io, st, text, tok);
            wspan(io, st, tstart(st, minus), tend(st, tok));
            return sp(tstart(st, minus), tend(st, tok));
        }
        let operand = unary(io, st, text);
        unary_line(io, st, "Neg", tstart(st, minus), operand.end);
        return sp(tstart(st, minus), operand.end);
    }
    return postfix(io, st, text);
}

// The smallest magnitude that rounds to infinity as a `float`, 2^1024 - 2^970, in decimal:
// halfway between the largest finite `float` and 2^1024, which rounds up.
fn float_limit() -> [] &static [byte] {
    return "179769313486231580793728971405303415079934132710037826936173778980444968292764750946649017977587207096330286416692887910946555547851940402630657488671505820681908902000708383676273854845817711531764475730270069855571366959622842914819860834936475292719074168444365510704342711559699508093042880177904174497792";
}

// The same for `f32`: 2^128 - 2^103.
fn f32_limit() -> [] &static [byte] {
    return "340282356779733661637539395458142568448";
}

// Would the float literal `tok` round to infinity? The Rust parser asks `str::parse`
// and refuses a literal that is not finite; this compares the literal's digits with the
// decimal digits of the smallest value that is, which is exact, and needs no conversion.
// A literal with no non-zero digit never overflows.
fn float_overflows[&s, &x](st: &!s [int], text: &x [byte], tok: int, single: bool) -> [] bool {
    var limit = float_limit();
    var to = tend(st, tok);
    if single {
        limit = f32_limit();
        to = to - 3;
    }
    var j = tstart(st, tok);
    var int_digits = 0;
    var seen_dot = false;
    var started = false;
    var leading_zeros = 0;
    var k = 0;
    var order = 0;
    while j < to && (lc.is_digit(lc.at(text, j)) || lc.at(text, j) == '_' || lc.at(text, j) == '.') {
        let c = lc.at(text, j);
        if c == '.' {
            seen_dot = true;
        } else if c != '_' {
            if !seen_dot {
                int_digits = int_digits + 1;
            }
            if !started && c == '0' {
                leading_zeros = leading_zeros + 1;
            } else {
                started = true;
                if k < len(limit) && order == 0 {
                    let l = int_of(limit[k]);
                    if c > l {
                        order = 1;
                    }
                    if c < l {
                        order = 0 - 1;
                    }
                }
                k = k + 1;
            }
        }
        j = j + 1;
    }
    if !started {
        return false;
    }
    // Fewer digits than the limit: the missing ones are zeros.
    while k < len(limit) && order == 0 {
        if int_of(limit[k]) != '0' {
            order = 0 - 1;
        }
        k = k + 1;
    }
    var exponent = 0;
    var negative = false;
    if lc.at(text, j) == 'e' || lc.at(text, j) == 'E' {
        j = j + 1;
        if lc.at(text, j) == '-' {
            negative = true;
            j = j + 1;
        } else if lc.at(text, j) == '+' {
            j = j + 1;
        }
        while j < to {
            let c = lc.at(text, j);
            if c != '_' && exponent < 1000000 {
                exponent = exponent * 10 + (c - '0');
            }
            j = j + 1;
        }
    }
    if negative {
        exponent = 0 - exponent;
    }
    let magnitude = int_digits - leading_zeros + exponent;
    if magnitude > len(limit) {
        return true;
    }
    return magnitude == len(limit) && order >= 0;
}

// `E.Float` or `E.F32`, and not yet the line's end; refuses a literal that is infinite. Only
// the span of a float is compared.
fn float_literal[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], tok: int) -> [io_write] int {
    if lc.spells(text, tend(st, tok) - 3, tend(st, tok), "f32") {
        if float_overflows(st, text, tok, true) {
            fail(st, r_literal_out_of_range(), tstart(st, tok), tend(st, tok));
        }
        return w(io, st, "E.F32");
    }
    if float_overflows(st, text, tok, false) {
        fail(st, 0, tstart(st, tok), tend(st, tok));
    }
    return w(io, st, "E.Float");
}

fn postfix[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    var base = primary(io, st, text);
    var go = true;
    while go && ok(st) {
        if look(st, 0, lc.Tok::Dot) {
            bump(st);
            let tok = st[0];
            if look(st, 0, lc.Tok::Int) {
                bump(st);
                let value = int_value(st, text, tok, false);
                if value > 4294967295 || value < 0 {
                    fail(st, 0, tstart(st, tok), tend(st, tok));
                }
                w(io, st, "E.TupleField ");
                wn(io, st, value);
                wspan(io, st, base.start, tend(st, tok));
                base = sp(base.start, tend(st, tok));
            } else {
                let name = ident(st);
                w(io, st, "E.Field ");
                wname(io, st, text, name);
                wspan(io, st, base.start, tend(st, tok));
                base = sp(base.start, tend(st, tok));
            }
        } else if look(st, 0, lc.Tok::LBracket) {
            bump(st);
            let outer = open_brackets(st);
            expr(io, st, text);
            var range = false;
            if eat(st, lc.Tok::DotDot) {
                expr(io, st, text);
                range = true;
            }
            close_brackets(st, outer);
            let close = expect(st, lc.Tok::RBracket);
            if range {
                w(io, st, "E.Slice");
            } else {
                w(io, st, "E.Index");
            }
            wspan(io, st, base.start, tend(st, close));
            base = sp(base.start, tend(st, close));
        } else {
            go = false;
        }
    }
    return base;
}

// Arguments up to `)`, with brackets open: how many.
fn args[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    let outer = open_brackets(st);
    var n = 0;
    var go = true;
    while go && more(st, lc.Tok::RParen) {
        expr(io, st, text);
        n = n + 1;
        go = eat(st, lc.Tok::Comma);
    }
    close_brackets(st, outer);
    return n;
}

fn primary[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let t = st[0];
    let begin = tstart(st, t);
    if look(st, 0, lc.Tok::Int) {
        bump(st);
        let value = int_value(st, text, t, false);
        w(io, st, "E.Int ");
        wn(io, st, value);
        wspan(io, st, begin, tend(st, t));
        return sp(begin, tend(st, t));
    }
    if look(st, 0, lc.Tok::Float) {
        bump(st);
        float_literal(io, st, text, t);
        wspan(io, st, begin, tend(st, t));
        return sp(begin, tend(st, t));
    }
    if look(st, 0, lc.Tok::Str) {
        bump(st);
        w(io, st, "E.Str ");
        wstring(io, st, text, t);
        wspan(io, st, begin, tend(st, t));
        return sp(begin, tend(st, t));
    }
    if look(st, 0, lc.Tok::True) || look(st, 0, lc.Tok::False) {
        bump(st);
        w(io, st, "E.Bool ");
        wbool(io, st, lc.code(lc.Tok::True) == code_at(st, t));
        wspan(io, st, begin, tend(st, t));
        return sp(begin, tend(st, t));
    }
    if word(st, text, 0, "alloc_slice") {
        bump(st);
        expect(st, lc.Tok::LBracket);
        let reg = ident(st);
        expect(st, lc.Tok::RBracket);
        expect(st, lc.Tok::LParen);
        let outer = open_brackets(st);
        expr(io, st, text);
        expect(st, lc.Tok::Comma);
        expr(io, st, text);
        close_brackets(st, outer);
        let close = expect(st, lc.Tok::RParen);
        w(io, st, "E.AllocSlice ");
        wname(io, st, text, reg);
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    if word(st, text, 0, "alloc") {
        bump(st);
        expect(st, lc.Tok::LBracket);
        let reg = ident(st);
        expect(st, lc.Tok::RBracket);
        expect(st, lc.Tok::LParen);
        let outer = open_brackets(st);
        expr(io, st, text);
        close_brackets(st, outer);
        let close = expect(st, lc.Tok::RParen);
        w(io, st, "E.Alloc ");
        wname(io, st, text, reg);
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    if look(st, 0, lc.Tok::Ident) {
        return name_expr(io, st, text);
    }
    if look(st, 0, lc.Tok::LParen) {
        bump(st);
        let outer = open_brackets(st);
        let first = expr(io, st, text);
        close_brackets(st, outer);
        if !eat(st, lc.Tok::Comma) {
            expect(st, lc.Tok::RParen);
            return first;
        }
        var n = 1;
        var go = true;
        while go && more(st, lc.Tok::RParen) {
            let outer2 = open_brackets(st);
            expr(io, st, text);
            close_brackets(st, outer2);
            n = n + 1;
            go = eat(st, lc.Tok::Comma);
        }
        let close = expect(st, lc.Tok::RParen);
        w(io, st, "E.Tuple ");
        wn(io, st, n);
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    fail(st, r_type_mismatch(), begin, tend(st, t));
    return nothing();
}

// A name, a call, a variant or a struct literal, each possibly qualified.
fn name_expr[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let t = st[0];
    let begin = tstart(st, t);
    var qualifier = 0 - 1;
    let dotted = look(st, 1, lc.Tok::Dot) && look(st, 2, lc.Tok::Ident);
    if dotted && (look(st, 3, lc.Tok::LParen) || look(st, 3, lc.Tok::ColonColon)) {
        qualifier = ident(st);
        expect(st, lc.Tok::Dot);
    } else if dotted && look(st, 3, lc.Tok::LBrace) && st[1] == 0 {
        qualifier = ident(st);
        expect(st, lc.Tok::Dot);
    }
    let name = ident(st);
    if look(st, 0, lc.Tok::ColonColon) {
        bump(st);
        let variant = ident(st);
        var n = 0;
        var end = tend(st, variant);
        if eat(st, lc.Tok::LParen) {
            n = args(io, st, text);
            let close = expect(st, lc.Tok::RParen);
            end = tend(st, close);
        }
        w(io, st, "E.Variant ");
        wqual(io, st, text, qualifier);
        w(io, st, " ");
        wname(io, st, text, name);
        w(io, st, " ");
        wname(io, st, text, variant);
        w(io, st, " ");
        wn(io, st, n);
        wspan(io, st, begin, end);
        return sp(begin, end);
    }
    if look(st, 0, lc.Tok::LParen) {
        bump(st);
        let n = args(io, st, text);
        let close = expect(st, lc.Tok::RParen);
        w(io, st, "E.Call ");
        wqual(io, st, text, qualifier);
        w(io, st, " ");
        wname(io, st, text, name);
        w(io, st, " ");
        wn(io, st, n);
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    if look(st, 0, lc.Tok::LBrace) && st[1] == 0 {
        bump(st);
        let outer = open_brackets(st);
        var n = 0;
        var go = true;
        while go && more(st, lc.Tok::RBrace) {
            let field = ident(st);
            expect(st, lc.Tok::Colon);
            w(io, st, "F.Name ");
            wname(io, st, text, field);
            w(io, st, "\n");
            expr(io, st, text);
            n = n + 1;
            go = eat(st, lc.Tok::Comma);
        }
        close_brackets(st, outer);
        let close = expect(st, lc.Tok::RBrace);
        w(io, st, "E.StructLit ");
        wqual(io, st, text, qualifier);
        w(io, st, " ");
        wname(io, st, text, name);
        w(io, st, " ");
        wn(io, st, n);
        wspan(io, st, begin, tend(st, close));
        return sp(begin, tend(st, close));
    }
    if qualifier >= 0 {
        fail(st, r_unknown_name(), tstart(st, st[0]), tend(st, st[0]));
        return nothing();
    }
    w(io, st, "E.Name ");
    wname(io, st, text, name);
    wspan(io, st, begin, tend(st, t));
    return sp(begin, tend(st, t));
}

// ----------------------------------------------------------- statements ---

fn block[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Bk {
    expect(st, lc.Tok::LBrace);
    var n = 0;
    while more(st, lc.Tok::RBrace) {
        if look(st, 0, lc.Tok::Eof) {
            fail(st, r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
        } else {
            stmt(io, st, text);
            n = n + 1;
        }
    }
    let close = expect(st, lc.Tok::RBrace);
    return Bk { count: n, end: tend(st, close) };
}

fn stmt[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    if look(st, 0, lc.Tok::Let) || look(st, 0, lc.Tok::Var) {
        return let_stmt(io, st, text);
    }
    if look(st, 0, lc.Tok::Return) {
        let kw = bump(st);
        let value = expr(io, st, text);
        let end = expect(st, lc.Tok::Semi);
        w(io, st, "S.Return");
        wspan(io, st, tstart(st, kw), tend(st, end));
        return sp(tstart(st, kw), tend(st, end));
    }
    if look(st, 0, lc.Tok::Defer) {
        let kw = bump(st);
        let value = expr(io, st, text);
        let end = expect(st, lc.Tok::Semi);
        w(io, st, "S.Defer");
        wspan(io, st, tstart(st, kw), tend(st, end));
        return sp(tstart(st, kw), tend(st, end));
    }
    if look(st, 0, lc.Tok::If) {
        return if_stmt(io, st, text);
    }
    if look(st, 0, lc.Tok::While) {
        let kw = bump(st);
        condition(io, st, text);
        let body = block(io, st, text);
        w(io, st, "S.While ");
        wn(io, st, body.count);
        wspan(io, st, tstart(st, kw), body.end);
        return sp(tstart(st, kw), body.end);
    }
    if look(st, 0, lc.Tok::Match) {
        return match_stmt(io, st, text);
    }
    if look(st, 0, lc.Tok::Borrow) {
        return borrow_stmt(io, st, text);
    }
    if look(st, 0, lc.Tok::Region) {
        let kw = bump(st);
        let reg = ident(st);
        let body = block(io, st, text);
        w(io, st, "S.Region ");
        wname(io, st, text, reg);
        w(io, st, " ");
        wn(io, st, body.count);
        wspan(io, st, tstart(st, kw), body.end);
        return sp(tstart(st, kw), body.end);
    }
    let first = expr(io, st, text);
    if eat(st, lc.Tok::Eq) {
        expr(io, st, text);
        let end = expect(st, lc.Tok::Semi);
        w(io, st, "S.Assign");
        wspan(io, st, first.start, tend(st, end));
        return sp(first.start, tend(st, end));
    }
    let end = expect(st, lc.Tok::Semi);
    w(io, st, "S.Expr");
    wspan(io, st, first.start, tend(st, end));
    return sp(first.start, tend(st, end));
}

fn pattern_shape[&s](st: &!s [int]) -> [] int {
    return fail(st, r_pattern_shape(), tstart(st, st[0]), tend(st, st[0]));
}

fn let_stmt[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let kw = bump(st);
    let mutable = code_at(st, kw) == lc.code(lc.Tok::Var);

    if look(st, 0, lc.Tok::LParen) {
        if mutable {
            pattern_shape(st);
        }
        bump(st);
        w(io, st, "H.Names [");
        var first = true;
        var go = true;
        while go && more(st, lc.Tok::RParen) {
            if look(st, 0, lc.Tok::LParen) {
                pattern_shape(st);
            }
            if !first {
                w(io, st, ",");
            }
            first = false;
            let name = ident(st);
            wname(io, st, text, name);
            go = eat(st, lc.Tok::Comma);
        }
        w(io, st, "]\n");
        expect(st, lc.Tok::RParen);
        expect(st, lc.Tok::Eq);
        expr(io, st, text);
        let end = expect(st, lc.Tok::Semi);
        w(io, st, "S.DestructureTuple");
        wspan(io, st, tstart(st, kw), tend(st, end));
        return sp(tstart(st, kw), tend(st, end));
    }

    let first = ident(st);
    var pattern_qualifier = 0 - 1;
    var name = first;
    if look(st, 0, lc.Tok::Dot) && look(st, 1, lc.Tok::Ident) && look(st, 2, lc.Tok::LBrace) {
        bump(st);
        pattern_qualifier = first;
        name = ident(st);
    }

    if look(st, 0, lc.Tok::LBrace) {
        if mutable {
            pattern_shape(st);
        }
        bump(st);
        w(io, st, "H.Names [");
        var firstf = true;
        var go = true;
        while go && more(st, lc.Tok::RBrace) {
            if !firstf {
                w(io, st, ",");
            }
            firstf = false;
            let field = ident(st);
            wname(io, st, text, field);
            go = eat(st, lc.Tok::Comma);
        }
        w(io, st, "]\n");
        expect(st, lc.Tok::RBrace);
        expect(st, lc.Tok::Eq);
        expr(io, st, text);
        let end = expect(st, lc.Tok::Semi);
        w(io, st, "S.Destructure ");
        wqual(io, st, text, pattern_qualifier);
        w(io, st, " ");
        wname(io, st, text, name);
        wspan(io, st, tstart(st, kw), tend(st, end));
        return sp(tstart(st, kw), tend(st, end));
    }

    var typed = false;
    if eat(st, lc.Tok::Colon) {
        type_expr(io, st, text);
        typed = true;
    }
    expect(st, lc.Tok::Eq);
    expr(io, st, text);
    let end = expect(st, lc.Tok::Semi);
    w(io, st, "S.Let ");
    wbool(io, st, mutable);
    w(io, st, " ");
    wname(io, st, text, name);
    w(io, st, " ");
    wbool(io, st, typed);
    wspan(io, st, tstart(st, kw), tend(st, end));
    return sp(tstart(st, kw), tend(st, end));
}

// The condition of an `if`, `while` or `match`: no struct literal at its top level.
fn condition[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let outer = st[1];
    st[1] = 1;
    let cond = expr(io, st, text);
    st[1] = outer;
    return cond;
}

fn if_stmt[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let kw = bump(st);
    condition(io, st, text);
    let then = block(io, st, text);
    var end = then.end;
    var has_else = false;
    var else_count = 0;
    if eat(st, lc.Tok::Else) {
        has_else = true;
        if look(st, 0, lc.Tok::If) {
            let nested = if_stmt(io, st, text);
            end = nested.end;
            else_count = 1;
        } else {
            let other = block(io, st, text);
            end = other.end;
            else_count = other.count;
        }
    }
    w(io, st, "S.If ");
    wn(io, st, then.count);
    w(io, st, " ");
    if has_else {
        wn(io, st, else_count);
    } else {
        w(io, st, "-");
    }
    wspan(io, st, tstart(st, kw), end);
    return sp(tstart(st, kw), end);
}

// `_`, or `q.Enum::Variant(a, _)`, written as the line `A.Pat ...`.
fn pattern[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    if eat(st, lc.Tok::Underscore) {
        return w(io, st, "A.Pat _\n");
    }
    var qualifier = 0 - 1;
    if look(st, 1, lc.Tok::Dot) {
        qualifier = ident(st);
        expect(st, lc.Tok::Dot);
    }
    let enum_name = ident(st);
    expect(st, lc.Tok::ColonColon);
    let variant = ident(st);
    w(io, st, "A.Pat ");
    wqual(io, st, text, qualifier);
    w(io, st, " ");
    wname(io, st, text, enum_name);
    w(io, st, " ");
    wname(io, st, text, variant);
    w(io, st, " [");
    if eat(st, lc.Tok::LParen) {
        var first = true;
        var go = true;
        while go && more(st, lc.Tok::RParen) {
            if !first {
                w(io, st, ",");
            }
            first = false;
            if eat(st, lc.Tok::Underscore) {
                w(io, st, "_");
            } else {
                let b = ident(st);
                wname(io, st, text, b);
            }
            go = eat(st, lc.Tok::Comma);
        }
        expect(st, lc.Tok::RParen);
    }
    return w(io, st, "]\n");
}

fn match_stmt[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let kw = bump(st);
    condition(io, st, text);
    expect(st, lc.Tok::LBrace);
    var arms = 0;
    while more(st, lc.Tok::RBrace) {
        if look(st, 0, lc.Tok::Eof) {
            fail(st, r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
        } else {
            pattern(io, st, text);
            expect(st, lc.Tok::FatArrow);
            let body = block(io, st, text);
            w(io, st, "A.Arm ");
            wn(io, st, body.count);
            w(io, st, "\n");
            arms = arms + 1;
            eat(st, lc.Tok::Comma);
        }
    }
    let close = expect(st, lc.Tok::RBrace);
    w(io, st, "S.Match ");
    wn(io, st, arms);
    wspan(io, st, tstart(st, kw), tend(st, close));
    return sp(tstart(st, kw), tend(st, close));
}

fn borrow_stmt[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] Sp {
    let kw = bump(st);
    let unique = eat(st, lc.Tok::Mut);
    let value = ident(st);
    expect(st, lc.Tok::As);
    expect(st, lc.Tok::Amp);
    let bang = eat(st, lc.Tok::Bang);
    if bang != unique {
        pattern_shape(st);
    }
    let reg = ident(st);
    expect(st, lc.Tok::In);
    let body = block(io, st, text);
    w(io, st, "S.Borrow ");
    wname(io, st, text, value);
    w(io, st, " ");
    wbool(io, st, unique);
    w(io, st, " ");
    wname(io, st, text, reg);
    w(io, st, " ");
    wn(io, st, body.count);
    wspan(io, st, tstart(st, kw), body.end);
    return sp(tstart(st, kw), body.end);
}

// ---------------------------------------------------------------- items ---

// `[T, U: val, &r where r <= q]`. Nothing is written while it is parsed: the three lists it
// holds are interleaved in the source, so `print_params` walks the tokens again, three times.
fn declaration_params[&s](st: &!s [int]) -> [] Dp {
    if !look(st, 0, lc.Tok::LBracket) {
        return Dp { start: 0 - 1, generics: 0, regions: 0, bounds: 0 };
    }
    let start = st[0];
    bump(st);
    var generics = 0;
    var regions = 0;
    var bounds = 0;
    var go = true;
    while go && ok(st) && !look(st, 0, lc.Tok::RBracket) && !look(st, 0, lc.Tok::Where) {
        if eat(st, lc.Tok::Amp) {
            ident(st);
            regions = regions + 1;
        } else {
            ident(st);
            generics = generics + 1;
            if eat(st, lc.Tok::Colon) {
                if look(st, 0, lc.Tok::Val) {
                    bump(st);
                    bounds = bounds + 1;
                } else if look(st, 0, lc.Tok::Res) {
                    fail(st, r_mode_bound_violated(), tstart(st, st[0]), tend(st, st[0]));
                } else {
                    fail(st, r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
                }
            }
        }
        go = eat(st, lc.Tok::Comma);
    }
    if eat(st, lc.Tok::Where) {
        var more_pairs = true;
        while more_pairs && ok(st) {
            ident(st);
            expect(st, lc.Tok::LtEq);
            ident(st);
            more_pairs = eat(st, lc.Tok::Comma);
        }
    }
    expect(st, lc.Tok::RBracket);
    return Dp { start: start, generics: generics, regions: regions, bounds: bounds };
}

// The line `H.Generics [T:val,U] [r] [a<=b]`, from the tokens of an already parsed `[...]`.
fn print_params[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], dp: Dp) -> [io_write] int {
    if st[7] != 1 {
        return 0;
    }
    w(io, st, "H.Generics [");
    if dp.start >= 0 {
        var k = dp.start + 1;
        var first = true;
        while code_at(st, k) != lc.code(lc.Tok::RBracket) && code_at(st, k) != lc.code(lc.Tok::Where) {
            if code_at(st, k) == lc.code(lc.Tok::Amp) {
                k = k + 2;
            } else {
                if !first {
                    w(io, st, ",");
                }
                first = false;
                wname(io, st, text, k);
                k = k + 1;
                if code_at(st, k) == lc.code(lc.Tok::Colon) {
                    w(io, st, ":val");
                    k = k + 2;
                }
            }
            if code_at(st, k) == lc.code(lc.Tok::Comma) {
                k = k + 1;
            }
        }
    }
    w(io, st, "] [");
    if dp.start >= 0 {
        var k = dp.start + 1;
        var first = true;
        while code_at(st, k) != lc.code(lc.Tok::RBracket) && code_at(st, k) != lc.code(lc.Tok::Where) {
            if code_at(st, k) == lc.code(lc.Tok::Amp) {
                if !first {
                    w(io, st, ",");
                }
                first = false;
                wname(io, st, text, k + 1);
                k = k + 2;
            } else {
                k = k + 1;
                if code_at(st, k) == lc.code(lc.Tok::Colon) {
                    k = k + 2;
                }
            }
            if code_at(st, k) == lc.code(lc.Tok::Comma) {
                k = k + 1;
            }
        }
    }
    w(io, st, "] [");
    if dp.start >= 0 {
        var k = dp.start + 1;
        while code_at(st, k) != lc.code(lc.Tok::RBracket) && code_at(st, k) != lc.code(lc.Tok::Where) {
            k = k + 1;
        }
        if code_at(st, k) == lc.code(lc.Tok::Where) {
            k = k + 1;
            var first = true;
            var go = true;
            while go {
                if !first {
                    w(io, st, ",");
                }
                first = false;
                wname(io, st, text, k);
                w(io, st, "<=");
                wname(io, st, text, k + 2);
                k = k + 3;
                if code_at(st, k) == lc.code(lc.Tok::Comma) {
                    k = k + 1;
                } else {
                    go = false;
                }
            }
        }
    }
    return w(io, st, "]\n");
}

// `(a: T, b: U)`: each name as `H.Param` and then its type. How many.
fn param_list[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    expect(st, lc.Tok::LParen);
    var n = 0;
    var go = true;
    while go && more(st, lc.Tok::RParen) {
        let name = ident(st);
        expect(st, lc.Tok::Colon);
        w(io, st, "H.Param ");
        wname(io, st, text, name);
        w(io, st, "\n");
        type_expr(io, st, text);
        n = n + 1;
        go = eat(st, lc.Tok::Comma);
    }
    expect(st, lc.Tok::RParen);
    return n;
}

fn module_path[&s](st: &!s [int]) -> [] int {
    var n = 1;
    ident(st);
    while eat(st, lc.Tok::Dot) {
        ident(st);
        n = n + 1;
    }
    return n;
}

fn fn_decl[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], public: bool) -> [io_write] int {
    let start = expect(st, lc.Tok::Fn);
    let name = ident(st);
    let dp = declaration_params(st);
    print_params(io, st, text, dp);
    param_list(io, st, text);
    expect(st, lc.Tok::Arrow);
    effect_row(io, st, text);
    type_expr(io, st, text);
    let body = block(io, st, text);
    w(io, st, "I.Fn ");
    wname(io, st, text, name);
    w(io, st, " ");
    wbool(io, st, public);
    w(io, st, " ");
    wn(io, st, body.count);
    wplace(io, st, text);
    return wspan(io, st, tstart(st, start), body.end);
}

fn static_decl[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], public: bool) -> [io_write] int {
    let start = bump(st);
    let name = ident(st);
    expect(st, lc.Tok::Colon);
    type_expr(io, st, text);
    let body = block(io, st, text);
    w(io, st, "I.Static ");
    wname(io, st, text, name);
    w(io, st, " ");
    wbool(io, st, public);
    w(io, st, " ");
    wn(io, st, body.count);
    wplace(io, st, text);
    return wspan(io, st, tstart(st, start), body.end);
}

fn extern_decl[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    let start = expect(st, lc.Tok::Extern);
    expect(st, lc.Tok::Fn);
    let name = ident(st);
    let dp = declaration_params(st);
    if dp.generics > 0 {
        fail(st, r_foreign_declaration(), tstart(st, st[0]), tend(st, st[0]));
    }
    print_params(io, st, text, dp);
    param_list(io, st, text);
    expect(st, lc.Tok::Arrow);
    effect_row(io, st, text);
    type_expr(io, st, text);
    let end = expect(st, lc.Tok::Semi);
    w(io, st, "I.Extern ");
    wname(io, st, text, name);
    w(io, st, " ");
    wsymbol(io, st, text, name);
    wplace(io, st, text);
    return wspan(io, st, tstart(st, start), tend(st, end));
}

// The text of a name as hex: an `extern fn`'s C symbol is its own name.
fn wsymbol[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], i: int) -> [io_write] int {
    var j = tstart(st, i);
    while j < tend(st, i) {
        let c = lc.at(text, j);
        whex_digit(io, st, c / 16);
        whex_digit(io, st, c % 16);
        j = j + 1;
    }
    return 0;
}

// The mode a type declaration was written with: 0 none, 1 `val`, 2 `res`.
fn wmode[&i, &s](io: &!i Io, st: &!s [int], mode: int) -> [io_write] int {
    if mode == 1 {
        return w(io, st, "val");
    }
    if mode == 2 {
        return w(io, st, "res");
    }
    return w(io, st, "-");
}

// What `generic_params` refuses after `declaration_params`.
fn generic_params[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], mode: int) -> [io_write] Dp {
    let dp = declaration_params(st);
    if dp.regions > 0 {
        fail(st, r_region_mismatch(), tstart(st, st[0]), tend(st, st[0]));
    }
    if mode == 1 && dp.bounds > 0 {
        fail(st, r_mode_bound_violated(), tstart(st, st[0]), tend(st, st[0]));
    }
    print_params(io, st, text, dp);
    return dp;
}

fn struct_decl[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], mode: int, mode_tok: int, public: bool) -> [io_write] int {
    var start = tstart(st, st[0]);
    if mode_tok >= 0 {
        start = tstart(st, mode_tok);
    }
    expect(st, lc.Tok::Struct);
    let name = ident(st);
    generic_params(io, st, text, mode);
    expect(st, lc.Tok::LBrace);
    var go = true;
    while go && more(st, lc.Tok::RBrace) {
        let field = ident(st);
        expect(st, lc.Tok::Colon);
        w(io, st, "H.Field ");
        wname(io, st, text, field);
        w(io, st, "\n");
        type_expr(io, st, text);
        go = eat(st, lc.Tok::Comma);
    }
    let end = expect(st, lc.Tok::RBrace);
    w(io, st, "I.Struct ");
    wname(io, st, text, name);
    w(io, st, " ");
    wbool(io, st, public);
    w(io, st, " ");
    wmode(io, st, mode);
    wplace(io, st, text);
    return wspan(io, st, start, tend(st, end));
}

fn enum_decl[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], mode: int, mode_tok: int, public: bool) -> [io_write] int {
    var start = tstart(st, st[0]);
    if mode_tok >= 0 {
        start = tstart(st, mode_tok);
    }
    expect(st, lc.Tok::Enum);
    let name = ident(st);
    generic_params(io, st, text, mode);
    expect(st, lc.Tok::LBrace);
    var go = true;
    while go && more(st, lc.Tok::RBrace) {
        let variant = ident(st);
        var payload = 0;
        if eat(st, lc.Tok::LParen) {
            payload = type_list(io, st, text, lc.Tok::RParen);
            expect(st, lc.Tok::RParen);
        }
        w(io, st, "V.Variant ");
        wname(io, st, text, variant);
        w(io, st, " ");
        wn(io, st, payload);
        w(io, st, "\n");
        go = eat(st, lc.Tok::Comma);
    }
    let end = expect(st, lc.Tok::RBrace);
    w(io, st, "I.Enum ");
    wname(io, st, text, name);
    w(io, st, " ");
    wbool(io, st, public);
    w(io, st, " ");
    wmode(io, st, mode);
    wplace(io, st, text);
    return wspan(io, st, start, tend(st, end));
}

// A whole file.
fn unit[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    if word(st, text, 0, "edition") {
        let keyword = bump(st);
        let tok = st[0];
        if !look(st, 0, lc.Tok::Int) {
            fail(st, r_unknown_edition(), tstart(st, tok), tend(st, tok));
        }
        bump(st);
        var value = 0;
        if ok(st) {
            value = int_value(st, text, tok, false);
        }
        expect(st, lc.Tok::Semi);
        if ok(st) && (value < 1 || value > 7) {
            fail(st, r_unknown_edition(), tstart(st, keyword), tend(st, tok));
        }
        st[6] = value;
    }

    var declared_module = false;
    var seen_item = false;
    var imports = 0;
    while ok(st) && !look(st, 0, lc.Tok::Eof) {
        if look(st, 0, lc.Tok::Module) {
            let keyword = bump(st);
            if declared_module {
                fail(st, r_program_shape(), tstart(st, keyword), tend(st, keyword));
            }
            if seen_item {
                fail(st, r_program_shape(), tstart(st, keyword), tend(st, keyword));
            }
            let first = st[0];
            let n = module_path(st);
            expect(st, lc.Tok::Semi);
            st[9] = 1;
            st[10] = first;
            st[11] = n;
            declared_module = true;
        } else if look(st, 0, lc.Tok::Import) {
            let keyword = bump(st);
            module_path(st);
            if eat(st, lc.Tok::As) {
                ident(st);
            }
            expect(st, lc.Tok::Semi);
            st[16 + 3 * st[8] + st[12]] = keyword * 2 + st[9];
            st[12] = st[12] + 1;
            seen_item = true;
        } else {
            seen_item = true;
            let public = eat(st, lc.Tok::Pub);
            if word(st, text, 0, "static") {
                static_decl(io, st, text, public);
            } else if look(st, 0, lc.Tok::Fn) {
                fn_decl(io, st, text, public);
            } else if look(st, 0, lc.Tok::Extern) {
                extern_decl(io, st, text);
            } else if look(st, 0, lc.Tok::Struct) {
                struct_decl(io, st, text, 0, 0 - 1, public);
            } else if look(st, 0, lc.Tok::Enum) {
                enum_decl(io, st, text, 0, 0 - 1, public);
            } else if look(st, 0, lc.Tok::Res) || look(st, 0, lc.Tok::Val) {
                let keyword = bump(st);
                var mode = 1;
                if code_at(st, keyword) == lc.code(lc.Tok::Res) {
                    mode = 2;
                }
                if look(st, 0, lc.Tok::Struct) {
                    struct_decl(io, st, text, mode, keyword, public);
                } else if look(st, 0, lc.Tok::Enum) {
                    enum_decl(io, st, text, mode, keyword, public);
                } else {
                    fail(st, r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
                }
            } else {
                fail(st, r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
            }
        }
    }
    return 0;
}

// The imports, which the Rust AST keeps apart from the items, module by module.
fn print_imports[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    var m = 0;
    while m < 2 {
        var n = 0;
        while n < st[12] {
            let record = st[16 + 3 * st[8] + n];
            if record % 2 == m {
                let keyword = record / 2;
                w(io, st, "M.Import ");
                wmodule(io, st, text, m);
                w(io, st, " ");
                var k = keyword + 1;
                var last = k;
                wname(io, st, text, k);
                while code_at(st, k + 1) == lc.code(lc.Tok::Dot) {
                    w(io, st, ".");
                    wname(io, st, text, k + 2);
                    k = k + 2;
                    last = k;
                }
                k = k + 1;
                w(io, st, " ");
                if code_at(st, k) == lc.code(lc.Tok::As) {
                    wname(io, st, text, k + 1);
                    k = k + 2;
                } else {
                    wname(io, st, text, last);
                }
                wspan(io, st, tstart(st, keyword), tend(st, k));
            }
            n = n + 1;
        }
        m = m + 1;
    }
    return 0;
}

// ----------------------------------------------------------------- main ---

// Turn `text` into tokens in `st`, or record the lexer's refusal.
fn tokenize[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    var pos = 0;
    var after_dot = false;
    var n = 0;
    var done = false;
    while !done {
        if pos >= len(text) {
            st[16 + 3 * n] = eof_code();
            st[16 + 3 * n + 1] = len(text);
            st[16 + 3 * n + 2] = len(text);
            n = n + 1;
            done = true;
        } else {
            match lc.step(text, pos, after_dot) {
                lc.Step::Skip(next) => {
                    pos = next;
                }
                lc.Step::Token(k, from, to) => {
                    st[16 + 3 * n] = lc.code(k);
                    st[16 + 3 * n + 1] = from;
                    st[16 + 3 * n + 2] = to;
                    n = n + 1;
                    after_dot = lc.is_dot(k);
                    pos = to;
                }
                lc.Step::Fail(rule, from, to) => {
                    fail(st, rule, from, to);
                    done = true;
                }
            }
        }
    }
    st[8] = n;
    return 0;
}

fn reset[&s](st: &!s [int], show: int) -> [] int {
    st[0] = 0;
    st[1] = 0;
    st[6] = 1;
    st[7] = show;
    st[9] = 0;
    st[12] = 0;
    return 0;
}

// Parse `text`: the refusal on one line, or the listing.
fn run[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    reset(st, 0);
    tokenize(st, text);
    if ok(st) {
        unit(io, st, text);
    }
    if !ok(st) {
        console.write_all(io, "ERR ");
        console.write_all(io, rule_tag(st[3]));
        console.space(io);
        console.print_nat(io, st[4]);
        console.space(io);
        console.print_nat(io, st[5]);
        console.newline(io);
        return 1;
    }
    reset(st, 1);
    unit(io, st, text);
    print_imports(io, st, text);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(args);
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            let text = lc.slurp(h, i);
            borrow text as &t in {
                let cap = buffer.size(t) + 1;
                let state = box_slice(h, 16 + 4 * cap, 0);
                borrow mut state as &!b in {
                    status = run(i, contents(b), buffer.bytes(t));
                }
                unbox_slice(h, state);
            }
            buffer.drop(h, text);
        }
    }
    release(heap);
    release(io);
    return status;
}
