module selfhost.ast;

// ast.ls -- the lex-sys syntax tree, and the parser that builds it, written in lex-sys.
//
// Stage 3a of the self-hosting spike (`docs/self-hosting.md` section 6, epic #295): the
// parser of stage 2 now *builds the tree* instead of printing it, so that the stages after
// it (resolution, the checker) have something to walk. It is still a port of
// `crates/lex-sys-syntax/src/parser/{expr,stmt,items}.rs`, node for node.
//
// The tree is the Rust AST's own design: flat tables and indices, not owned children. Every
// node is one record of 16 integers in one table, and a node refers to another by its
// index. A list (arguments, parameters, statements) is a chain: the parent holds the first
// child and the count, and each child holds the index of the next one. Nothing is
// allocated but the one boxed slice `st`, which also holds the parser's state and the
// tokens, so a program's whole front end is one allocation whose size is known from the
// length of the text.
//
//   slots 0..16        the parser's fields (below)
//   then               three slots per token: its `lexcore.code`, start and end
//   then               one slot per import
//   then               the node table, sixteen slots a node
//
// A node's slots are 0 its kind (a `NK`, as `kcode`), 1 its start, 2 its end, 3 the next
// node of the list it is in (-1 if last), and from 4 whatever its kind keeps; a reference
// to a token is the token's index. `docs`-style layout per kind is written at each
// parse function, which is where it is set.
//
// How it is written, because the language shapes it:
//
// * There are no integer constants, so a token is stored as its `lexcore.code`, an
//   integer, and compared with `look(st, n, lc.Tok::Comma)`, and a node's kind is an
//   enum `NK` compared through `kcode` the same way.
// * There is no early return from an error, so a refusal is **sticky**: the first one is
//   recorded in the state and every later call does nothing (`kind` answers end of file
//   once the parser has failed, so every loop ends, and `mk` makes no node), as `?`
//   would have. Every function has to be total on the state a refusal leaves.

import selfhost.lexcore as lc;
import selfhost.kinds;
import selfhost.rules;

// A list under construction: its first node, its last, and how many.
pub struct Ls {
    head: int,
    tail: int,
    n: int,
}

// The `[...]` of a declaration: where it starts (-1 if there is none) and what it held.
pub struct Dp {
    start: int,
    generics: int,
    regions: int,
    bounds: int,
}

// ------------------------------------------------------------ the state ---
//
// A program is a set of files (`docs/many-files.md`), parsed one after another into the same
// tables, as the Rust `parse_into` does: the text is every file in turn with one byte
// between them, so a span is an offset into the whole, and a file's tokens end with its own
// end-of-file token.
//
//   0 pos           the index of the token the parser is looking at
//   1 no_struct     1 while parsing an `if` or `while` condition
//   2 failed        1 once a refusal has been recorded
//   3 rule          the refusal's rule, 4 its start, 5 its end
//   6 edition       the edition of the file being parsed
//   7 last          the index of that file's end-of-file token
//   8 tokens        how many tokens there are so far, the end-of-file tokens included
//   9 module        the module the file being parsed is in (0 is the root)
//  10 tail          the last item (a node), -1 if there is none
//  11 modules       how many modules there are, the root included
//  12 imports       how many imports are recorded
//  13 nodes         how many nodes the table holds
//  14 base          where the node table starts
//  15 items         the first item (a node), -1 if there is none
//  16 module_base   where the module table starts: two slots a module, the index of its
//                   path's first token (-1 for the root) and how many segments it has
//  17 files         how many files the program has
//  18 file_base     where the file table starts: two slots a file, where it starts in the
//                   text and how long it is
//  19 import_base   where the import list starts: one slot an import, the `import` token's
//                   index times 65536 plus the module it was written in
//  20 binding_base  where the stack of local bindings starts: three slots a binding, the token
//                   of its name, its type and 1 if it is mutable (`body.ls`)
//  21 bindings      how many bindings are live
//  24 ...           three slots a token: its `lexcore.code`, start and end
//  then             the imports, the modules, the files and the nodes

// What `st` must hold for a program of `tokens` tokens in `files` files: every node takes at
// least a token, so the number of tokens bounds the number of nodes.
pub fn size_for(tokens: int, files: int) -> [] int {
    return 24 + 4 * tokens + 2 * (files + 2) + 2 * files + 16 * (tokens + 16) + 3 * (tokens + 16);
}

pub fn layout[&s](st: &!s [int], tokens: int, files: int) -> [] int {
    st[19] = 24 + 3 * tokens;
    st[16] = 24 + 4 * tokens;
    st[18] = st[16] + 2 * (files + 2);
    st[14] = st[18] + 2 * files;
    st[17] = files;
    st[20] = st[14] + 16 * (tokens + 16);
    st[21] = 0;
    st[13] = 0;
    st[15] = 0 - 1;
    st[10] = 0 - 1;
    st[11] = 1;
    st[st[16]] = 0 - 1;
    st[st[16] + 1] = 0;
    st[8] = 0;
    st[12] = 0;
    return 0;
}

// Where file `f` starts in the text, and how long it is.
pub fn set_file[&s](st: &!s [int], f: int, base: int, length: int) -> [] int {
    st[st[18] + 2 * f] = base;
    st[st[18] + 2 * f + 1] = length;
    return 0;
}

// The first token of module `m`'s path and how many segments it has.
pub fn module_first[&s](st: &!s [int], m: int) -> [] int {
    return st[st[16] + 2 * m];
}

pub fn module_len[&s](st: &!s [int], m: int) -> [] int {
    return st[st[16] + 2 * m + 1];
}

// The import recorded as number `n`: its `import` token's index and the module it is in.
pub fn import_keyword[&s](st: &!s [int], n: int) -> [] int {
    return st[st[19] + n] / 65536;
}

pub fn import_module[&s](st: &!s [int], n: int) -> [] int {
    return st[st[19] + n] % 65536;
}

// Do tokens `a` and `b` spell the same?
pub fn same[&s, &x](st: &!s [int], text: &x [byte], a: int, b: int) -> [] bool {
    let from = tstart(st, a);
    let n = tend(st, a) - from;
    if n != tend(st, b) - tstart(st, b) {
        return false;
    }
    let other = tstart(st, b);
    var k = 0;
    while k < n {
        if text[from + k] != text[other + k] {
            return false;
        }
        k = k + 1;
    }
    return true;
}

// The module whose path is the `n` segments from token `first`, added if it is new. A path
// names one module however many files declare it.
pub fn module_named[&s, &x](st: &!s [int], text: &x [byte], first: int, n: int) -> [] int {
    var m = 1;
    while m < st[11] {
        if module_len(st, m) == n {
            var k = 0;
            var equal = true;
            while k < n && equal {
                equal = same(st, text, first + 2 * k, module_first(st, m) + 2 * k);
                k = k + 1;
            }
            if equal {
                return m;
            }
        }
        m = m + 1;
    }
    st[st[16] + 2 * m] = first;
    st[st[16] + 2 * m + 1] = n;
    st[11] = m + 1;
    return m;
}

// ----------------------------------------------------------- the nodes ---

// Slot `slot` of node `id`; 0 for no node (-1), which is what a node that was never made
// answers after a refusal.
pub fn get[&s](st: &!s [int], id: int, slot: int) -> [] int {
    if id < 0 {
        return 0;
    }
    return st[st[14] + 16 * id + slot];
}

pub fn put[&s](st: &!s [int], id: int, slot: int, value: int) -> [] int {
    if id >= 0 {
        st[st[14] + 16 * id + slot] = value;
    }
    return value;
}

pub fn is_kind[&s](st: &!s [int], id: int, k: kinds.NK) -> [] bool {
    return id >= 0 && get(st, id, 0) == kinds.kcode(k);
}

pub fn nstart[&s](st: &!s [int], id: int) -> [] int {
    return get(st, id, 1);
}

pub fn nend[&s](st: &!s [int], id: int) -> [] int {
    return get(st, id, 2);
}

pub fn next[&s](st: &!s [int], id: int) -> [] int {
    return get(st, id, 3);
}

// A new node of kind `k` spanning `from..to`, every other slot -1. After a refusal there
// are no more nodes: the tree is void and the answer is -1.
pub fn mk[&s](st: &!s [int], k: kinds.NK, from: int, to: int) -> [] int {
    if st[2] != 0 {
        return 0 - 1;
    }
    let id = st[13];
    st[13] = id + 1;
    let at = st[14] + 16 * id;
    st[at] = kinds.kcode(k);
    st[at + 1] = from;
    st[at + 2] = to;
    var slot = 3;
    while slot < 16 {
        st[at + slot] = 0 - 1;
        slot = slot + 1;
    }
    return id;
}

pub fn empty_list() -> [] Ls {
    return Ls { head: 0 - 1, tail: 0 - 1, n: 0 };
}

// Append node `id` to the list.
pub fn push[&s](st: &!s [int], list: Ls, id: int) -> [] Ls {
    if id < 0 {
        return list;
    }
    if list.head < 0 {
        return Ls { head: id, tail: id, n: 1 };
    }
    put(st, list.tail, 3, id);
    return Ls { head: list.head, tail: id, n: list.n + 1 };
}

// ------------------------------------------------- rules and token plumbing ---

pub fn tokens_at() -> [] int {
    return 16;
}

pub fn ok[&s](st: &!s [int]) -> [] bool {
    return st[2] == 0;
}

pub fn eof_code() -> [] int {
    return lc.code(lc.Tok::Eof);
}

// The first refusal is the one that counts.
pub fn fail[&s](st: &!s [int], rule: int, from: int, to: int) -> [] int {
    if st[2] == 0 {
        st[2] = 1;
        st[3] = rule;
        st[4] = from;
        st[5] = to;
    }
    return 0;
}

// ------------------------------------------------------- token plumbing ---

pub fn tstart[&s](st: &!s [int], i: int) -> [] int {
    return st[24 + 3 * i + 1];
}

pub fn tend[&s](st: &!s [int], i: int) -> [] int {
    return st[24 + 3 * i + 2];
}

// The code of token `i`, whatever the parser's state.
pub fn code_at[&s](st: &!s [int], i: int) -> [] int {
    return st[24 + 3 * i];
}

// The kind of the token `n` ahead, saturating at end of file. After a refusal
// everything is end of file, so every loop that waits for a closer ends.
pub fn kind[&s](st: &!s [int], n: int) -> [] int {
    if st[2] != 0 {
        return eof_code();
    }
    var i = st[0] + n;
    if i > st[7] {
        i = st[7];
    }
    return code_at(st, i);
}

pub fn look[&s](st: &!s [int], n: int, t: lc.Tok) -> [] bool {
    return kind(st, n) == lc.code(t);
}

// The token consumed, by index. End of file is never consumed.
pub fn bump[&s](st: &!s [int]) -> [] int {
    let i = st[0];
    if kind(st, 0) != eof_code() {
        st[0] = i + 1;
    }
    return i;
}

pub fn eat[&s](st: &!s [int], t: lc.Tok) -> [] bool {
    if look(st, 0, t) {
        bump(st);
        return true;
    }
    return false;
}

pub fn expect[&s](st: &!s [int], t: lc.Tok) -> [] int {
    if look(st, 0, t) {
        return bump(st);
    }
    fail(st, rules.r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
    return st[0];
}

pub fn ident[&s](st: &!s [int]) -> [] int {
    return expect(st, lc.Tok::Ident);
}

// Is the token `n` ahead a name spelled `word`?
pub fn word[&s, &x](st: &!s [int], text: &x [byte], n: int, spelled: &static [byte]) -> [] bool {
    if !look(st, n, lc.Tok::Ident) {
        return false;
    }
    let i = st[0] + n;
    return lc.spells(text, tstart(st, i), tend(st, i), spelled);
}

// Still inside a list that ends at `closer`.
pub fn more[&s](st: &!s [int], closer: lc.Tok) -> [] bool {
    return ok(st) && !look(st, 0, closer);
}

// `bracketed`: inside brackets a struct literal is possible again.
pub fn open_brackets[&s](st: &!s [int]) -> [] int {
    let outer = st[1];
    st[1] = 0;
    return outer;
}

pub fn close_brackets[&s](st: &!s [int], outer: int) -> [] int {
    st[1] = outer;
    return 0;
}

// ------------------------------------------------------------- literals ---

// An integer literal's value, refusing what does not fit in 64 signed bits. The Rust parser
// reads the magnitude as a `u64` and compares it with the limit; here the magnitude is
// accumulated as a *negative* number, which holds one more value than a positive one does,
// and each step is checked before it is taken, because arithmetic here traps on overflow.
pub fn int_value[&s, &x](st: &!s [int], text: &x [byte], i: int, negated: bool) -> [] int {
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
        fail(st, rules.r_literal_out_of_range(), from, to);
        return 0;
    }
    if negated {
        return acc;
    }
    return 0 - acc;
}

// ---------------------------------------------------------------- types ---

// `[io_write, ffi("libc")]`: parsed and not kept; the node remembers where it starts, and
// a reader walks the tokens (`row_*`), which are already checked.
pub fn effect_row[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let open = expect(st, lc.Tok::LBracket);
    var go = true;
    while go && more(st, lc.Tok::RBracket) {
        ident(st);
        if eat(st, lc.Tok::LParen) {
            expect(st, lc.Tok::Str);
            expect(st, lc.Tok::RParen);
        }
        go = eat(st, lc.Tok::Comma);
    }
    expect(st, lc.Tok::RBracket);
    return open;
}

pub fn type_list[&s, &x](st: &!s [int], text: &x [byte], closer: lc.Tok) -> [] Ls {
    var list = empty_list();
    var go = true;
    while go && more(st, closer) {
        list = push(st, list, type_expr(st, text));
        go = eat(st, lc.Tok::Comma);
    }
    return list;
}

// A type. TName: 4 qualifier token or -1, 5 name token, 6 first argument, 7 how many.
// TRef: 4 unique, 5 region token, 6 inner. TSlice: 6 inner. TTuple: 6 first, 7 how many.
// TLit: 4 the string's token. TFn: 4 the effect row's `[`, 5 first parameter, 6 how many,
// 7 the result.
pub fn type_expr[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let t = st[0];
    let begin = tstart(st, t);
    if eat(st, lc.Tok::Amp) {
        let unique = eat(st, lc.Tok::Bang);
        let reg = ident(st);
        let inner = type_expr(st, text);
        let id = mk(st, kinds.NK::TRef, begin, nend(st, inner));
        put(st, id, 4, flag(unique));
        put(st, id, 5, reg);
        put(st, id, 6, inner);
        return id;
    }
    if eat(st, lc.Tok::Fn) {
        expect(st, lc.Tok::LParen);
        let params = type_list(st, text, lc.Tok::RParen);
        expect(st, lc.Tok::RParen);
        expect(st, lc.Tok::Arrow);
        let row = effect_row(st, text);
        let ret = type_expr(st, text);
        let id = mk(st, kinds.NK::TFn, begin, nend(st, ret));
        put(st, id, 4, row);
        put(st, id, 5, params.head);
        put(st, id, 6, params.n);
        put(st, id, 7, ret);
        return id;
    }
    if eat(st, lc.Tok::LParen) {
        let parts = type_list(st, text, lc.Tok::RParen);
        let close = expect(st, lc.Tok::RParen);
        let id = mk(st, kinds.NK::TTuple, begin, tend(st, close));
        put(st, id, 6, parts.head);
        put(st, id, 7, parts.n);
        return id;
    }
    if eat(st, lc.Tok::LBracket) {
        let inner = type_expr(st, text);
        let close = expect(st, lc.Tok::RBracket);
        let id = mk(st, kinds.NK::TSlice, begin, tend(st, close));
        put(st, id, 6, inner);
        return id;
    }
    let first = ident(st);
    var qualifier = 0 - 1;
    var name = first;
    if eat(st, lc.Tok::Dot) {
        qualifier = first;
        name = ident(st);
    }
    if eat(st, lc.Tok::LParen) {
        let lit_tok = expect(st, lc.Tok::Str);
        let close = expect(st, lc.Tok::RParen);
        let lit = mk(st, kinds.NK::TLit, begin, tend(st, close));
        put(st, lit, 4, lit_tok);
        let id = mk(st, kinds.NK::TName, begin, tend(st, close));
        put(st, id, 4, qualifier);
        put(st, id, 5, name);
        put(st, id, 6, lit);
        put(st, id, 7, 1);
        return id;
    }
    var args = empty_list();
    var end = tend(st, t);
    if eat(st, lc.Tok::LBracket) {
        args = type_list(st, text, lc.Tok::RBracket);
        let close = expect(st, lc.Tok::RBracket);
        end = tend(st, close);
    }
    let id = mk(st, kinds.NK::TName, begin, end);
    put(st, id, 4, qualifier);
    put(st, id, 5, name);
    put(st, id, 6, args.head);
    put(st, id, 7, args.n);
    return id;
}

pub fn flag(b: bool) -> [] int {
    var v = 0;
    if b {
        v = 1;
    }
    return v;
}

// ----------------------------------------------------------- operators ---

// The operator `level` of the precedence table has at the current token, as an index into
// `op_name`, or -1.
pub fn op_at[&s](st: &!s [int], level: int) -> [] int {
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

pub fn op_name(op: int) -> [] &static [byte] {
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

// The smallest magnitude that rounds to infinity as a `float`, 2^1024 - 2^970, in decimal:
// halfway between the largest finite `float` and 2^1024, which rounds up.
pub fn float_limit() -> [] &static [byte] {
    return "179769313486231580793728971405303415079934132710037826936173778980444968292764750946649017977587207096330286416692887910946555547851940402630657488671505820681908902000708383676273854845817711531764475730270069855571366959622842914819860834936475292719074168444365510704342711559699508093042880177904174497792";
}

// The same for `f32`: 2^128 - 2^103.
pub fn f32_limit() -> [] &static [byte] {
    return "340282356779733661637539395458142568448";
}

// Would the float literal `tok` round to infinity? The Rust parser asks `str::parse`
// and refuses a literal that is not finite; this compares the literal's digits with the
// decimal digits of the smallest value that is, which is exact, and needs no conversion.
// A literal with no non-zero digit never overflows.
pub fn float_overflows[&s, &x](st: &!s [int], text: &x [byte], tok: int, single: bool) -> [] bool {
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

// ---------------------------------------------------------- expressions ---
//
// EInt: 4 value. EFloat: 4 1 for an `f32`. EBool: 4 value. EStr: 4 the token. EName: 4 the
// token. EStructLit: 4 qualifier, 5 name, 6 first FieldInit, 7 how many. FieldInit: 4 the
// field's token, 5 its value. EField: 4 base, 5 name. ETuple: 4 first, 5 how many.
// ETupleField: 4 base, 5 index. EVariant: 4 qualifier, 5 enum, 6 variant, 7 first argument,
// 8 how many. EUnary: 4 operator (0 Neg, 1 Not, 2 BitNot, 3 Deref), 5 operand. EBinary: 4
// operator (`op_name`), 5 left, 6 right. ECall: 4 qualifier, 5 callee, 6 first argument,
// 7 how many. EIndex: 4 base, 5 index. ESlice: 4 base, 5 start, 6 end. EAlloc: 4 region
// token, 5 value. EAllocSlice: 4 region token, 5 count, 6 fill.

pub fn expr[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    return binary_level(st, text, 0);
}

pub fn binary_level[&s, &x](st: &!s [int], text: &x [byte], level: int) -> [] int {
    if level == 10 {
        return unary(st, text);
    }
    var lhs = binary_level(st, text, level + 1);
    var go = true;
    while go && ok(st) {
        let op = op_at(st, level);
        if op < 0 {
            go = false;
        } else {
            bump(st);
            let rhs = binary_level(st, text, level + 1);
            let id = mk(st, kinds.NK::EBinary, nstart(st, lhs), nend(st, rhs));
            put(st, id, 4, op);
            put(st, id, 5, lhs);
            put(st, id, 6, rhs);
            lhs = id;
        }
    }
    return lhs;
}

pub fn unary_node[&s](st: &!s [int], op: int, from: int, operand: int) -> [] int {
    let id = mk(st, kinds.NK::EUnary, from, nend(st, operand));
    put(st, id, 4, op);
    put(st, id, 5, operand);
    return id;
}

pub fn unary[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    if look(st, 0, lc.Tok::Bang) {
        let op = bump(st);
        return unary_node(st, 1, tstart(st, op), unary(st, text));
    }
    if look(st, 0, lc.Tok::Star) {
        let op = bump(st);
        return unary_node(st, 3, tstart(st, op), unary(st, text));
    }
    if look(st, 0, lc.Tok::Tilde) {
        let op = bump(st);
        return unary_node(st, 2, tstart(st, op), unary(st, text));
    }
    if look(st, 0, lc.Tok::Minus) {
        let minus = bump(st);
        if look(st, 0, lc.Tok::Int) {
            let tok = bump(st);
            let value = int_value(st, text, tok, true);
            let id = mk(st, kinds.NK::EInt, tstart(st, minus), tend(st, tok));
            put(st, id, 4, value);
            return id;
        }
        if look(st, 0, lc.Tok::Float) {
            let tok = bump(st);
            return float_node(st, text, tok, tstart(st, minus));
        }
        return unary_node(st, 0, tstart(st, minus), unary(st, text));
    }
    return postfix(st, text);
}

// A float literal, refused if it is infinite. Only its span and width are kept.
pub fn float_node[&s, &x](st: &!s [int], text: &x [byte], tok: int, from: int) -> [] int {
    var single = false;
    if lc.spells(text, tend(st, tok) - 3, tend(st, tok), "f32") {
        single = true;
        if float_overflows(st, text, tok, true) {
            fail(st, rules.r_literal_out_of_range(), tstart(st, tok), tend(st, tok));
        }
    } else if float_overflows(st, text, tok, false) {
        fail(st, 0, tstart(st, tok), tend(st, tok));
    }
    let id = mk(st, kinds.NK::EFloat, from, tend(st, tok));
    put(st, id, 4, flag(single));
    return id;
}

pub fn postfix[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    var base = primary(st, text);
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
                let id = mk(st, kinds.NK::ETupleField, nstart(st, base), tend(st, tok));
                put(st, id, 4, base);
                put(st, id, 5, value);
                base = id;
            } else {
                let name = ident(st);
                let id = mk(st, kinds.NK::EField, nstart(st, base), tend(st, tok));
                put(st, id, 4, base);
                put(st, id, 5, name);
                base = id;
            }
        } else if look(st, 0, lc.Tok::LBracket) {
            bump(st);
            let outer = open_brackets(st);
            let first = expr(st, text);
            var second = 0 - 1;
            if eat(st, lc.Tok::DotDot) {
                second = expr(st, text);
            }
            close_brackets(st, outer);
            let close = expect(st, lc.Tok::RBracket);
            if second >= 0 {
                let id = mk(st, kinds.NK::ESlice, nstart(st, base), tend(st, close));
                put(st, id, 4, base);
                put(st, id, 5, first);
                put(st, id, 6, second);
                base = id;
            } else {
                let id = mk(st, kinds.NK::EIndex, nstart(st, base), tend(st, close));
                put(st, id, 4, base);
                put(st, id, 5, first);
                base = id;
            }
        } else {
            go = false;
        }
    }
    return base;
}

// Arguments up to `)`, with brackets open.
pub fn args[&s, &x](st: &!s [int], text: &x [byte]) -> [] Ls {
    let outer = open_brackets(st);
    var list = empty_list();
    var go = true;
    while go && more(st, lc.Tok::RParen) {
        list = push(st, list, expr(st, text));
        go = eat(st, lc.Tok::Comma);
    }
    close_brackets(st, outer);
    return list;
}

pub fn primary[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let t = st[0];
    let begin = tstart(st, t);
    if look(st, 0, lc.Tok::Int) {
        bump(st);
        let value = int_value(st, text, t, false);
        let id = mk(st, kinds.NK::EInt, begin, tend(st, t));
        put(st, id, 4, value);
        return id;
    }
    if look(st, 0, lc.Tok::Float) {
        bump(st);
        return float_node(st, text, t, begin);
    }
    if look(st, 0, lc.Tok::Str) {
        bump(st);
        let id = mk(st, kinds.NK::EStr, begin, tend(st, t));
        put(st, id, 4, t);
        return id;
    }
    if look(st, 0, lc.Tok::True) || look(st, 0, lc.Tok::False) {
        bump(st);
        let id = mk(st, kinds.NK::EBool, begin, tend(st, t));
        put(st, id, 4, flag(lc.code(lc.Tok::True) == code_at(st, t)));
        return id;
    }
    if word(st, text, 0, "alloc_slice") {
        bump(st);
        expect(st, lc.Tok::LBracket);
        let reg = ident(st);
        expect(st, lc.Tok::RBracket);
        expect(st, lc.Tok::LParen);
        let outer = open_brackets(st);
        let count = expr(st, text);
        expect(st, lc.Tok::Comma);
        let fill = expr(st, text);
        close_brackets(st, outer);
        let close = expect(st, lc.Tok::RParen);
        let id = mk(st, kinds.NK::EAllocSlice, begin, tend(st, close));
        put(st, id, 4, reg);
        put(st, id, 5, count);
        put(st, id, 6, fill);
        return id;
    }
    if word(st, text, 0, "alloc") {
        bump(st);
        expect(st, lc.Tok::LBracket);
        let reg = ident(st);
        expect(st, lc.Tok::RBracket);
        expect(st, lc.Tok::LParen);
        let outer = open_brackets(st);
        let value = expr(st, text);
        close_brackets(st, outer);
        let close = expect(st, lc.Tok::RParen);
        let id = mk(st, kinds.NK::EAlloc, begin, tend(st, close));
        put(st, id, 4, reg);
        put(st, id, 5, value);
        return id;
    }
    if look(st, 0, lc.Tok::Ident) {
        return name_expr(st, text);
    }
    if look(st, 0, lc.Tok::LParen) {
        bump(st);
        let outer = open_brackets(st);
        let first = expr(st, text);
        close_brackets(st, outer);
        if !eat(st, lc.Tok::Comma) {
            expect(st, lc.Tok::RParen);
            return first;
        }
        var parts = push(st, empty_list(), first);
        var go = true;
        while go && more(st, lc.Tok::RParen) {
            let outer2 = open_brackets(st);
            parts = push(st, parts, expr(st, text));
            close_brackets(st, outer2);
            go = eat(st, lc.Tok::Comma);
        }
        let close = expect(st, lc.Tok::RParen);
        let id = mk(st, kinds.NK::ETuple, begin, tend(st, close));
        put(st, id, 4, parts.head);
        put(st, id, 5, parts.n);
        return id;
    }
    fail(st, rules.r_type_mismatch(), begin, tend(st, t));
    return 0 - 1;
}

// A name, a call, a variant or a struct literal, each possibly qualified.
pub fn name_expr[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
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
        var list = empty_list();
        var end = tend(st, variant);
        if eat(st, lc.Tok::LParen) {
            list = args(st, text);
            let close = expect(st, lc.Tok::RParen);
            end = tend(st, close);
        }
        let id = mk(st, kinds.NK::EVariant, begin, end);
        put(st, id, 4, qualifier);
        put(st, id, 5, name);
        put(st, id, 6, variant);
        put(st, id, 7, list.head);
        put(st, id, 8, list.n);
        return id;
    }
    if look(st, 0, lc.Tok::LParen) {
        bump(st);
        let list = args(st, text);
        let close = expect(st, lc.Tok::RParen);
        let id = mk(st, kinds.NK::ECall, begin, tend(st, close));
        put(st, id, 4, qualifier);
        put(st, id, 5, name);
        put(st, id, 6, list.head);
        put(st, id, 7, list.n);
        return id;
    }
    if look(st, 0, lc.Tok::LBrace) && st[1] == 0 {
        bump(st);
        let outer = open_brackets(st);
        var fields = empty_list();
        var go = true;
        while go && more(st, lc.Tok::RBrace) {
            let field = ident(st);
            expect(st, lc.Tok::Colon);
            let value = expr(st, text);
            let init = mk(st, kinds.NK::FieldInit, tstart(st, field), nend(st, value));
            put(st, init, 4, field);
            put(st, init, 5, value);
            fields = push(st, fields, init);
            go = eat(st, lc.Tok::Comma);
        }
        close_brackets(st, outer);
        let close = expect(st, lc.Tok::RBrace);
        let id = mk(st, kinds.NK::EStructLit, begin, tend(st, close));
        put(st, id, 4, qualifier);
        put(st, id, 5, name);
        put(st, id, 6, fields.head);
        put(st, id, 7, fields.n);
        return id;
    }
    if qualifier >= 0 {
        fail(st, rules.r_unknown_name(), tstart(st, st[0]), tend(st, st[0]));
        return 0 - 1;
    }
    let id = mk(st, kinds.NK::EName, begin, tend(st, t));
    put(st, id, 4, name);
    return id;
}

// ----------------------------------------------------------- statements ---
//
// A block is a list of statements and a count, kept by its owner; `Bk` answers both and
// where the closing brace ends. SLet: 4 mutable, 5 name token, 6 type or -1, 7 value.
// SAssign: 4 place, 5 value. SDestructure: 4 qualifier, 5 struct name, 6 the `{`, 7 value.
// SDestructureTuple: 4 the `(`, 5 value. SBorrow: 4 the value's token, 5 unique, 6 region
// token, 7 first statement, 8 how many. SRegion: 4 region token, 5 first, 6 how many.
// SExpr: 4 expression. SIf: 4 condition, 5 first statement of `then`, 6 how many, 7 first
// of `else`, 8 how many (-1 if there is no `else`). SWhile: 4 condition, 5 first, 6 how
// many. SMatch: 4 scrutinee, 5 first arm, 6 how many. Arm: 4 the pattern's first token,
// 5 first statement, 6 how many. SReturn, SDefer: 4 expression.

pub struct Bk {
    list: Ls,
    end: int,
}

pub fn block[&s, &x](st: &!s [int], text: &x [byte]) -> [] Bk {
    expect(st, lc.Tok::LBrace);
    var list = empty_list();
    while more(st, lc.Tok::RBrace) {
        if look(st, 0, lc.Tok::Eof) {
            fail(st, rules.r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
        } else {
            list = push(st, list, stmt(st, text));
        }
    }
    let close = expect(st, lc.Tok::RBrace);
    return Bk { list: list, end: tend(st, close) };
}

// `kw expr ;`, for `return` and `defer`.
pub fn keyword_stmt[&s, &x](st: &!s [int], text: &x [byte], k: kinds.NK) -> [] int {
    let kw = bump(st);
    let value = expr(st, text);
    let end = expect(st, lc.Tok::Semi);
    let id = mk(st, k, tstart(st, kw), tend(st, end));
    put(st, id, 4, value);
    return id;
}

pub fn stmt[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    if look(st, 0, lc.Tok::Let) || look(st, 0, lc.Tok::Var) {
        return let_stmt(st, text);
    }
    if look(st, 0, lc.Tok::Return) {
        return keyword_stmt(st, text, kinds.NK::SReturn);
    }
    if look(st, 0, lc.Tok::Defer) {
        return keyword_stmt(st, text, kinds.NK::SDefer);
    }
    if look(st, 0, lc.Tok::If) {
        return if_stmt(st, text);
    }
    if look(st, 0, lc.Tok::While) {
        let kw = bump(st);
        let cond = condition(st, text);
        let body = block(st, text);
        let id = mk(st, kinds.NK::SWhile, tstart(st, kw), body.end);
        put(st, id, 4, cond);
        put(st, id, 5, body.list.head);
        put(st, id, 6, body.list.n);
        return id;
    }
    if look(st, 0, lc.Tok::Match) {
        return match_stmt(st, text);
    }
    if look(st, 0, lc.Tok::Borrow) {
        return borrow_stmt(st, text);
    }
    if look(st, 0, lc.Tok::Region) {
        let kw = bump(st);
        let reg = ident(st);
        let body = block(st, text);
        let id = mk(st, kinds.NK::SRegion, tstart(st, kw), body.end);
        put(st, id, 4, reg);
        put(st, id, 5, body.list.head);
        put(st, id, 6, body.list.n);
        return id;
    }
    let first = expr(st, text);
    if eat(st, lc.Tok::Eq) {
        let value = expr(st, text);
        let end = expect(st, lc.Tok::Semi);
        let id = mk(st, kinds.NK::SAssign, nstart(st, first), tend(st, end));
        put(st, id, 4, first);
        put(st, id, 5, value);
        return id;
    }
    let end = expect(st, lc.Tok::Semi);
    let id = mk(st, kinds.NK::SExpr, nstart(st, first), tend(st, end));
    put(st, id, 4, first);
    return id;
}

pub fn pattern_shape[&s](st: &!s [int]) -> [] int {
    return fail(st, rules.r_pattern_shape(), tstart(st, st[0]), tend(st, st[0]));
}

pub fn let_stmt[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let kw = bump(st);
    let mutable = code_at(st, kw) == lc.code(lc.Tok::Var);

    if look(st, 0, lc.Tok::LParen) {
        if mutable {
            pattern_shape(st);
        }
        let open = bump(st);
        var go = true;
        while go && more(st, lc.Tok::RParen) {
            if look(st, 0, lc.Tok::LParen) {
                pattern_shape(st);
            }
            ident(st);
            go = eat(st, lc.Tok::Comma);
        }
        expect(st, lc.Tok::RParen);
        expect(st, lc.Tok::Eq);
        let value = expr(st, text);
        let end = expect(st, lc.Tok::Semi);
        let id = mk(st, kinds.NK::SDestructureTuple, tstart(st, kw), tend(st, end));
        put(st, id, 4, open);
        put(st, id, 5, value);
        return id;
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
        let open = bump(st);
        var go = true;
        while go && more(st, lc.Tok::RBrace) {
            ident(st);
            go = eat(st, lc.Tok::Comma);
        }
        expect(st, lc.Tok::RBrace);
        expect(st, lc.Tok::Eq);
        let value = expr(st, text);
        let end = expect(st, lc.Tok::Semi);
        let id = mk(st, kinds.NK::SDestructure, tstart(st, kw), tend(st, end));
        put(st, id, 4, pattern_qualifier);
        put(st, id, 5, name);
        put(st, id, 6, open);
        put(st, id, 7, value);
        return id;
    }

    var ty = 0 - 1;
    if eat(st, lc.Tok::Colon) {
        ty = type_expr(st, text);
    }
    expect(st, lc.Tok::Eq);
    let value = expr(st, text);
    let end = expect(st, lc.Tok::Semi);
    let id = mk(st, kinds.NK::SLet, tstart(st, kw), tend(st, end));
    put(st, id, 4, flag(mutable));
    put(st, id, 5, name);
    put(st, id, 6, ty);
    put(st, id, 7, value);
    return id;
}

// The condition of an `if`, `while` or `match`: no struct literal at its top level.
pub fn condition[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let outer = st[1];
    st[1] = 1;
    let cond = expr(st, text);
    st[1] = outer;
    return cond;
}

pub fn if_stmt[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let kw = bump(st);
    let cond = condition(st, text);
    let then = block(st, text);
    var end = then.end;
    var else_head = 0 - 1;
    var else_count = 0 - 1;
    if eat(st, lc.Tok::Else) {
        if look(st, 0, lc.Tok::If) {
            let nested = if_stmt(st, text);
            end = nend(st, nested);
            else_head = nested;
            else_count = 1;
        } else {
            let other = block(st, text);
            end = other.end;
            else_head = other.list.head;
            else_count = other.list.n;
        }
    }
    let id = mk(st, kinds.NK::SIf, tstart(st, kw), end);
    put(st, id, 4, cond);
    put(st, id, 5, then.list.head);
    put(st, id, 6, then.list.n);
    put(st, id, 7, else_head);
    put(st, id, 8, else_count);
    return id;
}

// A pattern, `_` or `q.Enum::Variant(a, _)`: parsed and not kept, the node remembers where
// it starts and a reader walks the tokens. Answers that token.
pub fn pattern[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let first = st[0];
    if eat(st, lc.Tok::Underscore) {
        return first;
    }
    if look(st, 1, lc.Tok::Dot) {
        ident(st);
        expect(st, lc.Tok::Dot);
    }
    ident(st);
    expect(st, lc.Tok::ColonColon);
    ident(st);
    if eat(st, lc.Tok::LParen) {
        var go = true;
        while go && more(st, lc.Tok::RParen) {
            if !eat(st, lc.Tok::Underscore) {
                ident(st);
            }
            go = eat(st, lc.Tok::Comma);
        }
        expect(st, lc.Tok::RParen);
    }
    return first;
}

pub fn match_stmt[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let kw = bump(st);
    let scrutinee = condition(st, text);
    expect(st, lc.Tok::LBrace);
    var arms = empty_list();
    while more(st, lc.Tok::RBrace) {
        if look(st, 0, lc.Tok::Eof) {
            fail(st, rules.r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
        } else {
            let first = pattern(st, text);
            expect(st, lc.Tok::FatArrow);
            let body = block(st, text);
            let arm = mk(st, kinds.NK::Arm, tstart(st, first), body.end);
            put(st, arm, 4, first);
            put(st, arm, 5, body.list.head);
            put(st, arm, 6, body.list.n);
            arms = push(st, arms, arm);
            eat(st, lc.Tok::Comma);
        }
    }
    let close = expect(st, lc.Tok::RBrace);
    let id = mk(st, kinds.NK::SMatch, tstart(st, kw), tend(st, close));
    put(st, id, 4, scrutinee);
    put(st, id, 5, arms.head);
    put(st, id, 6, arms.n);
    return id;
}

pub fn borrow_stmt[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
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
    let body = block(st, text);
    let id = mk(st, kinds.NK::SBorrow, tstart(st, kw), body.end);
    put(st, id, 4, value);
    put(st, id, 5, flag(unique));
    put(st, id, 6, reg);
    put(st, id, 7, body.list.head);
    put(st, id, 8, body.list.n);
    return id;
}

// `[T, U: val, &r where r <= q]`. Nothing is written while it is parsed: the three lists it
// holds are interleaved in the source, so `print_params` walks the tokens again, three times.
pub fn declaration_params[&s](st: &!s [int]) -> [] Dp {
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
                    fail(st, rules.r_mode_bound_violated(), tstart(st, st[0]), tend(st, st[0]));
                } else {
                    fail(st, rules.r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
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

pub fn module_path[&s](st: &!s [int]) -> [] int {
    var n = 1;
    ident(st);
    while eat(st, lc.Tok::Dot) {
        ident(st);
        n = n + 1;
    }
    return n;
}

// ---------------------------------------------------------------- items ---
//
// Param, Field: 4 the name's token, 5 the type. Variant: 4 the name's token, 5 first payload
// type, 6 how many. Every item: 13 its module (0 root, 1 declared), 14 its edition.
// IFn: 4 name token, 5 `pub`, 6 the `[` of its parameters or -1, 7 first Param, 8 how many,
// 9 the effect row's `[`, 10 result type, 11 first statement, 12 how many. IExtern: 4 name
// token (which is also its C symbol), 6, 7, 8, 9, 10 as an IFn. IStruct: 4 name token, 5
// `pub`, 6 mode (0 none, 1 `val`, 2 `res`), 7 the `[` or -1, 8 first Field. IEnum: the same,
// 8 first Variant. IStatic: 4 name token, 5 `pub`, 6 type, 7 first statement, 8 how many.

// `(a: T, b: U)`: the Params.
pub fn param_list[&s, &x](st: &!s [int], text: &x [byte]) -> [] Ls {
    expect(st, lc.Tok::LParen);
    var list = empty_list();
    var go = true;
    while go && more(st, lc.Tok::RParen) {
        let name = ident(st);
        expect(st, lc.Tok::Colon);
        let ty = type_expr(st, text);
        let id = mk(st, kinds.NK::Param, tstart(st, name), nend(st, ty));
        put(st, id, 4, name);
        put(st, id, 5, ty);
        list = push(st, list, id);
        go = eat(st, lc.Tok::Comma);
    }
    expect(st, lc.Tok::RParen);
    return list;
}

pub fn place[&s](st: &!s [int], id: int) -> [] int {
    put(st, id, 13, st[9]);
    put(st, id, 14, st[6]);
    return id;
}

pub fn fn_decl[&s, &x](st: &!s [int], text: &x [byte], public: bool) -> [] int {
    let start = expect(st, lc.Tok::Fn);
    let name = ident(st);
    let dp = declaration_params(st);
    let params = param_list(st, text);
    expect(st, lc.Tok::Arrow);
    let row = effect_row(st, text);
    let ret = type_expr(st, text);
    let body = block(st, text);
    let id = mk(st, kinds.NK::IFn, tstart(st, start), body.end);
    put(st, id, 4, name);
    put(st, id, 5, flag(public));
    put(st, id, 6, dp.start);
    put(st, id, 7, params.head);
    put(st, id, 8, params.n);
    put(st, id, 9, row);
    put(st, id, 10, ret);
    put(st, id, 11, body.list.head);
    put(st, id, 12, body.list.n);
    return place(st, id);
}

pub fn static_decl[&s, &x](st: &!s [int], text: &x [byte], public: bool) -> [] int {
    let start = bump(st);
    let name = ident(st);
    expect(st, lc.Tok::Colon);
    let ty = type_expr(st, text);
    let body = block(st, text);
    let id = mk(st, kinds.NK::IStatic, tstart(st, start), body.end);
    put(st, id, 4, name);
    put(st, id, 5, flag(public));
    put(st, id, 6, ty);
    put(st, id, 7, body.list.head);
    put(st, id, 8, body.list.n);
    return place(st, id);
}

pub fn extern_decl[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    let start = expect(st, lc.Tok::Extern);
    expect(st, lc.Tok::Fn);
    let name = ident(st);
    let dp = declaration_params(st);
    if dp.generics > 0 {
        fail(st, rules.r_foreign_declaration(), tstart(st, st[0]), tend(st, st[0]));
    }
    let params = param_list(st, text);
    expect(st, lc.Tok::Arrow);
    let row = effect_row(st, text);
    let ret = type_expr(st, text);
    let end = expect(st, lc.Tok::Semi);
    let id = mk(st, kinds.NK::IExtern, tstart(st, start), tend(st, end));
    put(st, id, 4, name);
    put(st, id, 6, dp.start);
    put(st, id, 7, params.head);
    put(st, id, 8, params.n);
    put(st, id, 9, row);
    put(st, id, 10, ret);
    return place(st, id);
}

// What `generic_params` refuses after `declaration_params`.
pub fn generic_params[&s](st: &!s [int], mode: int) -> [] Dp {
    let dp = declaration_params(st);
    if dp.regions > 0 {
        fail(st, rules.r_region_mismatch(), tstart(st, st[0]), tend(st, st[0]));
    }
    if mode == 1 && dp.bounds > 0 {
        fail(st, rules.r_mode_bound_violated(), tstart(st, st[0]), tend(st, st[0]));
    }
    return dp;
}

pub fn struct_decl[&s, &x](st: &!s [int], text: &x [byte], mode: int, mode_tok: int, public: bool) -> [] int {
    var start = tstart(st, st[0]);
    if mode_tok >= 0 {
        start = tstart(st, mode_tok);
    }
    expect(st, lc.Tok::Struct);
    let name = ident(st);
    let dp = generic_params(st, mode);
    expect(st, lc.Tok::LBrace);
    var fields = empty_list();
    var go = true;
    while go && more(st, lc.Tok::RBrace) {
        let field = ident(st);
        expect(st, lc.Tok::Colon);
        let ty = type_expr(st, text);
        let f = mk(st, kinds.NK::Field, tstart(st, field), nend(st, ty));
        put(st, f, 4, field);
        put(st, f, 5, ty);
        fields = push(st, fields, f);
        go = eat(st, lc.Tok::Comma);
    }
    let end = expect(st, lc.Tok::RBrace);
    let id = mk(st, kinds.NK::IStruct, start, tend(st, end));
    put(st, id, 4, name);
    put(st, id, 5, flag(public));
    put(st, id, 6, mode);
    put(st, id, 7, dp.start);
    put(st, id, 8, fields.head);
    return place(st, id);
}

pub fn enum_decl[&s, &x](st: &!s [int], text: &x [byte], mode: int, mode_tok: int, public: bool) -> [] int {
    var start = tstart(st, st[0]);
    if mode_tok >= 0 {
        start = tstart(st, mode_tok);
    }
    expect(st, lc.Tok::Enum);
    let name = ident(st);
    let dp = generic_params(st, mode);
    expect(st, lc.Tok::LBrace);
    var variants = empty_list();
    var go = true;
    while go && more(st, lc.Tok::RBrace) {
        let variant = ident(st);
        var payload = empty_list();
        if eat(st, lc.Tok::LParen) {
            payload = type_list(st, text, lc.Tok::RParen);
            expect(st, lc.Tok::RParen);
        }
        let v = mk(st, kinds.NK::Variant, tstart(st, variant), tend(st, variant));
        put(st, v, 4, variant);
        put(st, v, 5, payload.head);
        put(st, v, 6, payload.n);
        variants = push(st, variants, v);
        go = eat(st, lc.Tok::Comma);
    }
    let end = expect(st, lc.Tok::RBrace);
    let id = mk(st, kinds.NK::IEnum, start, tend(st, end));
    put(st, id, 4, name);
    put(st, id, 5, flag(public));
    put(st, id, 6, mode);
    put(st, id, 7, dp.start);
    put(st, id, 8, variants.head);
    return place(st, id);
}

// A whole file: its items, chained from slot 15 of the state.
pub fn unit[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    if word(st, text, 0, "edition") {
        let keyword = bump(st);
        let tok = st[0];
        if !look(st, 0, lc.Tok::Int) {
            fail(st, rules.r_unknown_edition(), tstart(st, tok), tend(st, tok));
        }
        bump(st);
        var value = 0;
        if ok(st) {
            value = int_value(st, text, tok, false);
        }
        expect(st, lc.Tok::Semi);
        if ok(st) && (value < 1 || value > 7) {
            fail(st, rules.r_unknown_edition(), tstart(st, keyword), tend(st, tok));
        }
        st[6] = value;
    }

    var items = Ls { head: st[15], tail: st[10], n: 0 };
    var declared_module = false;
    var seen_item = false;
    while ok(st) && !look(st, 0, lc.Tok::Eof) {
        if look(st, 0, lc.Tok::Module) {
            let keyword = bump(st);
            if declared_module {
                fail(st, rules.r_program_shape(), tstart(st, keyword), tend(st, keyword));
            }
            if seen_item {
                fail(st, rules.r_program_shape(), tstart(st, keyword), tend(st, keyword));
            }
            let first = st[0];
            let n = module_path(st);
            expect(st, lc.Tok::Semi);
            if ok(st) {
                st[9] = module_named(st, text, first, n);
            }
            declared_module = true;
        } else if look(st, 0, lc.Tok::Import) {
            let keyword = bump(st);
            module_path(st);
            if eat(st, lc.Tok::As) {
                ident(st);
            }
            expect(st, lc.Tok::Semi);
            st[st[19] + st[12]] = keyword * 65536 + st[9];
            st[12] = st[12] + 1;
            seen_item = true;
        } else {
            seen_item = true;
            let public = eat(st, lc.Tok::Pub);
            var item = 0 - 1;
            if word(st, text, 0, "static") {
                item = static_decl(st, text, public);
            } else if look(st, 0, lc.Tok::Fn) {
                item = fn_decl(st, text, public);
            } else if look(st, 0, lc.Tok::Extern) {
                item = extern_decl(st, text);
            } else if look(st, 0, lc.Tok::Struct) {
                item = struct_decl(st, text, 0, 0 - 1, public);
            } else if look(st, 0, lc.Tok::Enum) {
                item = enum_decl(st, text, 0, 0 - 1, public);
            } else if look(st, 0, lc.Tok::Res) || look(st, 0, lc.Tok::Val) {
                let keyword = bump(st);
                var mode = 1;
                if code_at(st, keyword) == lc.code(lc.Tok::Res) {
                    mode = 2;
                }
                if look(st, 0, lc.Tok::Struct) {
                    item = struct_decl(st, text, mode, keyword, public);
                } else if look(st, 0, lc.Tok::Enum) {
                    item = enum_decl(st, text, mode, keyword, public);
                } else {
                    fail(st, rules.r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
                }
            } else {
                fail(st, rules.r_type_mismatch(), tstart(st, st[0]), tend(st, st[0]));
            }
            items = push(st, items, item);
        }
    }
    st[15] = items.head;
    st[10] = items.tail;
    return items.n;
}

// Parse every file of the program, one after another, into the same tables. 0, or 1 if the
// program was refused (see `st[2]`).
pub fn parse[&s, &x](st: &!s [int], text: &x [byte]) -> [] int {
    var f = 0;
    while f < st[17] && ok(st) {
        let base = st[st[18] + 2 * f];
        let first = st[8];
        tokenize(st, text[base..base + st[st[18] + 2 * f + 1]], base);
        if ok(st) {
            st[0] = first;
            st[7] = st[8] - 1;
            st[6] = 1;
            st[9] = 0;
            unit(st, text);
        }
        f = f + 1;
    }
    if ok(st) {
        return 0;
    }
    return 1;
}

// ------------------------------------------------------------ tokenizing ---

// Turn the file `text`, which starts at offset `base` of the program, into tokens at the end
// of the table, or record the lexer's refusal. Spans are offsets into the whole program.
pub fn tokenize[&s, &x](st: &!s [int], text: &x [byte], base: int) -> [] int {
    var pos = 0;
    var after_dot = false;
    var n = st[8];
    var done = false;
    while !done {
        if pos >= len(text) {
            st[24 + 3 * n] = eof_code();
            st[24 + 3 * n + 1] = base + len(text);
            st[24 + 3 * n + 2] = base + len(text);
            n = n + 1;
            done = true;
        } else {
            match lc.step(text, pos, after_dot) {
                lc.Step::Skip(next) => {
                    pos = next;
                }
                lc.Step::Token(k, from, to) => {
                    st[24 + 3 * n] = lc.code(k);
                    st[24 + 3 * n + 1] = base + from;
                    st[24 + 3 * n + 2] = base + to;
                    n = n + 1;
                    after_dot = lc.is_dot(k);
                    pos = to;
                }
                lc.Step::Fail(rule, from, to) => {
                    fail(st, rule, base + from, base + to);
                    done = true;
                }
            }
        }
    }
    st[8] = n;
    return 0;
}

// How many tokens the file `text` has, its end-of-file token included. A file the lexer refuses
// has as many as it lexed before the refusal, which is all the parser will ask for.
pub fn count_tokens[&x](text: &x [byte]) -> [] int {
    var pos = 0;
    var after_dot = false;
    var n = 0;
    var done = false;
    while !done {
        if pos >= len(text) {
            n = n + 1;
            done = true;
        } else {
            match lc.step(text, pos, after_dot) {
                lc.Step::Skip(next) => {
                    pos = next;
                }
                lc.Step::Token(k, from, to) => {
                    n = n + 1;
                    after_dot = lc.is_dot(k);
                    pos = to;
                }
                lc.Step::Fail(rule, from, to) => {
                    done = true;
                }
            }
        }
    }
    return n;
}
