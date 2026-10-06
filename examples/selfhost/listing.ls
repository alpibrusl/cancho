module selfhost.listing;

// listing.ls -- the syntax tree of `ast.ls`, written out as a listing.
//
// Stages 2 and 3a of the self-hosting spike (`docs/self-hosting.md` section 6, epic #295).
// `ast.ls` is the parser, a port of `crates/lex-sys-syntax/src/parser/{expr,stmt,items}.rs`
// that builds the syntax tree in flat tables. This file walks the tree and writes it as a
// **postfix listing**, one node per
// line, children before their parent, each saying how many children it takes off the
// stack, so the listing needs no brackets:
//
//     E.Int 7 @ 4 5
//     E.Name x @ 8 9
//     E.Binary Add @ 4 9
//
// or, if the Rust parser would have refused the input, one line `ERR rule-tag start end`.
// `crates/lex-sys-syntax/examples/dump_ast.rs` writes the same listing from the Rust AST
// and is the oracle; `diff.sh` and `fuzz.py` compare them. Since the listing is produced
// by walking the tree and not while parsing, a match is a statement about the tree.
//
//     lex-sys run examples/selfhost/parser.ls examples/selfhost/driver.ls examples/selfhost/listing.ls \
//         examples/selfhost/ast.ls examples/selfhost/kinds.ls examples/selfhost/lexcore.ls --std < f.ls
//
// Not compared: float values (the listing has the literal's span and whether it is an
// `f32`, not its bits; a literal that rounds to infinity is refused), symbol ids and
// message text. Strings are written as hex.
//
// Things the tree does not keep, because the parser has already checked them and the
// tokens are still there: an effect row, a declaration's `[T, &r where ...]`, a
// destructuring pattern's names and a match pattern. A node holds the index of the first
// token and the walkers below read the rest from the tokens.

import std.io as console;
import selfhost.lexcore as lc;
import selfhost.ast;
import selfhost.kinds;

// ---------------------------------------------------------------- output ---

pub fn w[&i, &r](io: &!i Io, str: &r [byte]) -> [io_write] int {
    return console.write_all(io, str);
}

pub fn wn[&i](io: &!i Io, n: int) -> [io_write] int {
    return console.print_int(io, n);
}

pub fn wname[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], i: int) -> [io_write] int {
    return console.write_all(io, text[ast.tstart(st, i)..ast.tend(st, i)]);
}

// A qualifier, or `-` for none (-1).
pub fn wqual[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], q: int) -> [io_write] int {
    if q < 0 {
        return w(io, "-");
    }
    return wname(io, st, text, q);
}

// ` @ start end` and the end of the line.
pub fn wspan[&i, &s](io: &!i Io, st: &!s [int], id: int) -> [io_write] int {
    w(io, " @ ");
    wn(io, ast.nstart(st, id));
    w(io, " ");
    wn(io, ast.nend(st, id));
    return w(io, "\n");
}

pub fn wbool[&i](io: &!i Io, v: int) -> [io_write] int {
    if v != 0 {
        return w(io, "1");
    }
    return w(io, "0");
}

pub fn whex_digit[&i](io: &!i Io, d: int) -> [io_write] int {
    let digits = "0123456789abcdef";
    return w(io, digits[d..d + 1]);
}

// The decoded bytes of the string literal at token `i`, as hex; `-` for the empty string.
pub fn wstring[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], i: int) -> [io_write] int {
    let to = ast.tend(st, i) - 1;
    var j = ast.tstart(st, i) + 1;
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
        whex_digit(io, c / 16);
        whex_digit(io, c % 16);
        any = true;
        j = j + 1;
    }
    if !any {
        w(io, "-");
    }
    return 0;
}

// A name's text as hex: an `extern fn`'s C symbol is its own name.
pub fn wsymbol[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], i: int) -> [io_write] int {
    var j = ast.tstart(st, i);
    while j < ast.tend(st, i) {
        let c = lc.at(text, j);
        whex_digit(io, c / 16);
        whex_digit(io, c % 16);
        j = j + 1;
    }
    return 0;
}

// A module's path, `a.b`, or `-` for the root.
pub fn wmodule[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], m: int) -> [io_write] int {
    if m == 0 {
        return w(io, "-");
    }
    var at = st[10];
    wname(io, st, text, at);
    var n = 1;
    while n < st[11] {
        w(io, ".");
        wname(io, st, text, at + 2);
        at = at + 2;
        n = n + 1;
    }
    return 0;
}

// ` mod=a.b ed=N`, what an item line ends with.
pub fn wplace[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], id: int) -> [io_write] int {
    w(io, " mod=");
    wmodule(io, st, text, ast.get(st, id, 13));
    w(io, " ed=");
    return wn(io, ast.get(st, id, 14));
}

pub fn wmode[&i](io: &!i Io, mode: int) -> [io_write] int {
    if mode == 1 {
        return w(io, "val");
    }
    if mode == 2 {
        return w(io, "res");
    }
    return w(io, "-");
}

// ------------------------------------------------- what the tokens still say ---

// The line `H.Effects [io_write,ffi:6c696263]`, from the row whose `[` is token `open`.
pub fn row_line[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], open: int) -> [io_write] int {
    w(io, "H.Effects [");
    var k = open + 1;
    var first = true;
    while ast.code_at(st, k) != lc.code(lc.Tok::RBracket) {
        if !first {
            w(io, ",");
        }
        first = false;
        wname(io, st, text, k);
        k = k + 1;
        if ast.code_at(st, k) == lc.code(lc.Tok::LParen) {
            w(io, ":");
            wstring(io, st, text, k + 1);
            k = k + 3;
        }
        if ast.code_at(st, k) == lc.code(lc.Tok::Comma) {
            k = k + 1;
        }
    }
    return w(io, "]\n");
}

// The line `H.Names [a,b]`, from the pattern whose opening bracket is token `open`.
pub fn names_line[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], open: int, closer: lc.Tok) -> [io_write] int {
    w(io, "H.Names [");
    var k = open + 1;
    var first = true;
    while ast.code_at(st, k) != lc.code(closer) {
        if !first {
            w(io, ",");
        }
        first = false;
        wname(io, st, text, k);
        k = k + 1;
        if ast.code_at(st, k) == lc.code(lc.Tok::Comma) {
            k = k + 1;
        }
    }
    return w(io, "]\n");
}

// The line `A.Pat _` or `A.Pat q Enum Variant [a,_]`, from the pattern starting at token `k`.
pub fn pattern_line[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], first: int) -> [io_write] int {
    if ast.code_at(st, first) == lc.code(lc.Tok::Underscore) {
        return w(io, "A.Pat _\n");
    }
    var k = first;
    var qualifier = 0 - 1;
    if ast.code_at(st, k + 1) == lc.code(lc.Tok::Dot) {
        qualifier = k;
        k = k + 2;
    }
    w(io, "A.Pat ");
    wqual(io, st, text, qualifier);
    w(io, " ");
    wname(io, st, text, k);
    w(io, " ");
    wname(io, st, text, k + 2);
    w(io, " [");
    k = k + 3;
    if ast.code_at(st, k) == lc.code(lc.Tok::LParen) {
        k = k + 1;
        var firstb = true;
        while ast.code_at(st, k) != lc.code(lc.Tok::RParen) {
            if !firstb {
                w(io, ",");
            }
            firstb = false;
            if ast.code_at(st, k) == lc.code(lc.Tok::Underscore) {
                w(io, "_");
            } else {
                wname(io, st, text, k);
            }
            k = k + 1;
            if ast.code_at(st, k) == lc.code(lc.Tok::Comma) {
                k = k + 1;
            }
        }
    }
    return w(io, "]\n");
}

// The line `H.Generics [T:val,U] [r] [a<=b]`, from the `[...]` of a declaration whose `[`
// is token `start` (-1 if it has none). `[T, &r, U: val, &q where r <= q]` interleaves two
// lists the Rust AST keeps apart, so this walks the tokens three times.
pub fn generics_line[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], start: int) -> [io_write] int {
    w(io, "H.Generics [");
    if start >= 0 {
        var k = start + 1;
        var first = true;
        while ast.code_at(st, k) != lc.code(lc.Tok::RBracket) && ast.code_at(st, k) != lc.code(lc.Tok::Where) {
            if ast.code_at(st, k) == lc.code(lc.Tok::Amp) {
                k = k + 2;
            } else {
                if !first {
                    w(io, ",");
                }
                first = false;
                wname(io, st, text, k);
                k = k + 1;
                if ast.code_at(st, k) == lc.code(lc.Tok::Colon) {
                    w(io, ":val");
                    k = k + 2;
                }
            }
            if ast.code_at(st, k) == lc.code(lc.Tok::Comma) {
                k = k + 1;
            }
        }
    }
    w(io, "] [");
    if start >= 0 {
        var k = start + 1;
        var first = true;
        while ast.code_at(st, k) != lc.code(lc.Tok::RBracket) && ast.code_at(st, k) != lc.code(lc.Tok::Where) {
            if ast.code_at(st, k) == lc.code(lc.Tok::Amp) {
                if !first {
                    w(io, ",");
                }
                first = false;
                wname(io, st, text, k + 1);
                k = k + 2;
            } else {
                k = k + 1;
                if ast.code_at(st, k) == lc.code(lc.Tok::Colon) {
                    k = k + 2;
                }
            }
            if ast.code_at(st, k) == lc.code(lc.Tok::Comma) {
                k = k + 1;
            }
        }
    }
    w(io, "] [");
    if start >= 0 {
        var k = start + 1;
        while ast.code_at(st, k) != lc.code(lc.Tok::RBracket) && ast.code_at(st, k) != lc.code(lc.Tok::Where) {
            k = k + 1;
        }
        if ast.code_at(st, k) == lc.code(lc.Tok::Where) {
            k = k + 1;
            var first = true;
            var go = true;
            while go {
                if !first {
                    w(io, ",");
                }
                first = false;
                wname(io, st, text, k);
                w(io, "<=");
                wname(io, st, text, k + 2);
                k = k + 3;
                if ast.code_at(st, k) == lc.code(lc.Tok::Comma) {
                    k = k + 1;
                } else {
                    go = false;
                }
            }
        }
    }
    return w(io, "]\n");
}

// ------------------------------------------------------------ the walkers ---

// `n` nodes of a list that starts at `head`, each by `walk` of its kind. The walkers are
// mutually recursive and the language has no function values, so each list has its own loop.
pub fn dump_types[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], head: int, n: int) -> [io_write] int {
    var at = head;
    var left = n;
    while left > 0 {
        dump_type(io, st, text, at);
        at = ast.next(st, at);
        left = left - 1;
    }
    return 0;
}

pub fn dump_exprs[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], head: int, n: int) -> [io_write] int {
    var at = head;
    var left = n;
    while left > 0 {
        dump_expr(io, st, text, at);
        at = ast.next(st, at);
        left = left - 1;
    }
    return 0;
}

pub fn dump_stmts[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], head: int, n: int) -> [io_write] int {
    var at = head;
    var left = n;
    while left > 0 {
        dump_stmt(io, st, text, at);
        at = ast.next(st, at);
        left = left - 1;
    }
    return 0;
}

pub fn dump_type[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], id: int) -> [io_write] int {
    if ast.is_kind(st, id, kinds.NK::TName) {
        dump_types(io, st, text, ast.get(st, id, 6), ast.get(st, id, 7));
        w(io, "T.Name ");
        wqual(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 5));
        w(io, " ");
        wn(io, ast.get(st, id, 7));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::TRef) {
        dump_type(io, st, text, ast.get(st, id, 6));
        w(io, "T.Ref ");
        wbool(io, ast.get(st, id, 4));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 5));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::TSlice) {
        dump_type(io, st, text, ast.get(st, id, 6));
        w(io, "T.Slice");
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::TTuple) {
        dump_types(io, st, text, ast.get(st, id, 6), ast.get(st, id, 7));
        w(io, "T.Tuple ");
        wn(io, ast.get(st, id, 7));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::TLit) {
        w(io, "T.Lit ");
        wstring(io, st, text, ast.get(st, id, 4));
        return wspan(io, st, id);
    }
    // A function type.
    dump_types(io, st, text, ast.get(st, id, 5), ast.get(st, id, 6));
    row_line(io, st, text, ast.get(st, id, 4));
    dump_type(io, st, text, ast.get(st, id, 7));
    w(io, "T.Fn ");
    wn(io, ast.get(st, id, 6));
    return wspan(io, st, id);
}

pub fn dump_expr[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], id: int) -> [io_write] int {
    if ast.is_kind(st, id, kinds.NK::EInt) {
        w(io, "E.Int ");
        wn(io, ast.get(st, id, 4));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EFloat) {
        if ast.get(st, id, 4) != 0 {
            w(io, "E.F32");
        } else {
            w(io, "E.Float");
        }
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EBool) {
        w(io, "E.Bool ");
        wbool(io, ast.get(st, id, 4));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EStr) {
        w(io, "E.Str ");
        wstring(io, st, text, ast.get(st, id, 4));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EName) {
        w(io, "E.Name ");
        wname(io, st, text, ast.get(st, id, 4));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EStructLit) {
        var at = ast.get(st, id, 6);
        var left = ast.get(st, id, 7);
        while left > 0 {
            w(io, "F.Name ");
            wname(io, st, text, ast.get(st, at, 4));
            w(io, "\n");
            dump_expr(io, st, text, ast.get(st, at, 5));
            at = ast.next(st, at);
            left = left - 1;
        }
        w(io, "E.StructLit ");
        wqual(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 5));
        w(io, " ");
        wn(io, ast.get(st, id, 7));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EField) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        w(io, "E.Field ");
        wname(io, st, text, ast.get(st, id, 5));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::ETuple) {
        dump_exprs(io, st, text, ast.get(st, id, 4), ast.get(st, id, 5));
        w(io, "E.Tuple ");
        wn(io, ast.get(st, id, 5));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::ETupleField) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        w(io, "E.TupleField ");
        wn(io, ast.get(st, id, 5));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EVariant) {
        dump_exprs(io, st, text, ast.get(st, id, 7), ast.get(st, id, 8));
        w(io, "E.Variant ");
        wqual(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 5));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 6));
        w(io, " ");
        wn(io, ast.get(st, id, 8));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EUnary) {
        dump_expr(io, st, text, ast.get(st, id, 5));
        w(io, "E.Unary ");
        w(io, unary_name(ast.get(st, id, 4)));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EBinary) {
        dump_expr(io, st, text, ast.get(st, id, 5));
        dump_expr(io, st, text, ast.get(st, id, 6));
        w(io, "E.Binary ");
        w(io, ast.op_name(ast.get(st, id, 4)));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::ECall) {
        dump_exprs(io, st, text, ast.get(st, id, 6), ast.get(st, id, 7));
        w(io, "E.Call ");
        wqual(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 5));
        w(io, " ");
        wn(io, ast.get(st, id, 7));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EIndex) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        dump_expr(io, st, text, ast.get(st, id, 5));
        w(io, "E.Index");
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::ESlice) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        dump_expr(io, st, text, ast.get(st, id, 5));
        dump_expr(io, st, text, ast.get(st, id, 6));
        w(io, "E.Slice");
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::EAlloc) {
        dump_expr(io, st, text, ast.get(st, id, 5));
        w(io, "E.Alloc ");
        wname(io, st, text, ast.get(st, id, 4));
        return wspan(io, st, id);
    }
    // `alloc_slice`.
    dump_expr(io, st, text, ast.get(st, id, 5));
    dump_expr(io, st, text, ast.get(st, id, 6));
    w(io, "E.AllocSlice ");
    wname(io, st, text, ast.get(st, id, 4));
    return wspan(io, st, id);
}

pub fn unary_name(op: int) -> [] &static [byte] {
    if op == 0 {
        return "Neg";
    }
    if op == 1 {
        return "Not";
    }
    if op == 2 {
        return "BitNot";
    }
    return "Deref";
}

pub fn dump_stmt[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], id: int) -> [io_write] int {
    if ast.is_kind(st, id, kinds.NK::SLet) {
        if ast.get(st, id, 6) >= 0 {
            dump_type(io, st, text, ast.get(st, id, 6));
        }
        dump_expr(io, st, text, ast.get(st, id, 7));
        w(io, "S.Let ");
        wbool(io, ast.get(st, id, 4));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 5));
        w(io, " ");
        wbool(io, flag_of(ast.get(st, id, 6) >= 0));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SAssign) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        dump_expr(io, st, text, ast.get(st, id, 5));
        w(io, "S.Assign");
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SDestructure) {
        names_line(io, st, text, ast.get(st, id, 6), lc.Tok::RBrace);
        dump_expr(io, st, text, ast.get(st, id, 7));
        w(io, "S.Destructure ");
        wqual(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 5));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SDestructureTuple) {
        names_line(io, st, text, ast.get(st, id, 4), lc.Tok::RParen);
        dump_expr(io, st, text, ast.get(st, id, 5));
        w(io, "S.DestructureTuple");
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SBorrow) {
        dump_stmts(io, st, text, ast.get(st, id, 7), ast.get(st, id, 8));
        w(io, "S.Borrow ");
        wname(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wbool(io, ast.get(st, id, 5));
        w(io, " ");
        wname(io, st, text, ast.get(st, id, 6));
        w(io, " ");
        wn(io, ast.get(st, id, 8));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SRegion) {
        dump_stmts(io, st, text, ast.get(st, id, 5), ast.get(st, id, 6));
        w(io, "S.Region ");
        wname(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wn(io, ast.get(st, id, 6));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SExpr) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        w(io, "S.Expr");
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SIf) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        dump_stmts(io, st, text, ast.get(st, id, 5), ast.get(st, id, 6));
        if ast.get(st, id, 8) >= 0 {
            dump_stmts(io, st, text, ast.get(st, id, 7), ast.get(st, id, 8));
        }
        w(io, "S.If ");
        wn(io, ast.get(st, id, 6));
        w(io, " ");
        if ast.get(st, id, 8) >= 0 {
            wn(io, ast.get(st, id, 8));
        } else {
            w(io, "-");
        }
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SWhile) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        dump_stmts(io, st, text, ast.get(st, id, 5), ast.get(st, id, 6));
        w(io, "S.While ");
        wn(io, ast.get(st, id, 6));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SMatch) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        var arm = ast.get(st, id, 5);
        var left = ast.get(st, id, 6);
        while left > 0 {
            pattern_line(io, st, text, ast.get(st, arm, 4));
            dump_stmts(io, st, text, ast.get(st, arm, 5), ast.get(st, arm, 6));
            w(io, "A.Arm ");
            wn(io, ast.get(st, arm, 6));
            w(io, "\n");
            arm = ast.next(st, arm);
            left = left - 1;
        }
        w(io, "S.Match ");
        wn(io, ast.get(st, id, 6));
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::SReturn) {
        dump_expr(io, st, text, ast.get(st, id, 4));
        w(io, "S.Return");
        return wspan(io, st, id);
    }
    dump_expr(io, st, text, ast.get(st, id, 4));
    w(io, "S.Defer");
    return wspan(io, st, id);
}

pub fn flag_of(b: bool) -> [] int {
    return ast.flag(b);
}

// `H.Param name` and the type, for each of the `n` Params from `head`.
pub fn dump_params[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], head: int, n: int) -> [io_write] int {
    var at = head;
    var left = n;
    while left > 0 {
        w(io, "H.Param ");
        wname(io, st, text, ast.get(st, at, 4));
        w(io, "\n");
        dump_type(io, st, text, ast.get(st, at, 5));
        at = ast.next(st, at);
        left = left - 1;
    }
    return 0;
}

pub fn dump_item[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], id: int) -> [io_write] int {
    if ast.is_kind(st, id, kinds.NK::IFn) {
        generics_line(io, st, text, ast.get(st, id, 6));
        dump_params(io, st, text, ast.get(st, id, 7), ast.get(st, id, 8));
        row_line(io, st, text, ast.get(st, id, 9));
        dump_type(io, st, text, ast.get(st, id, 10));
        dump_stmts(io, st, text, ast.get(st, id, 11), ast.get(st, id, 12));
        w(io, "I.Fn ");
        wname(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wbool(io, ast.get(st, id, 5));
        w(io, " ");
        wn(io, ast.get(st, id, 12));
        wplace(io, st, text, id);
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::IExtern) {
        generics_line(io, st, text, ast.get(st, id, 6));
        dump_params(io, st, text, ast.get(st, id, 7), ast.get(st, id, 8));
        row_line(io, st, text, ast.get(st, id, 9));
        dump_type(io, st, text, ast.get(st, id, 10));
        w(io, "I.Extern ");
        wname(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wsymbol(io, st, text, ast.get(st, id, 4));
        wplace(io, st, text, id);
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::IStruct) {
        generics_line(io, st, text, ast.get(st, id, 7));
        var at = ast.get(st, id, 8);
        while at >= 0 {
            w(io, "H.Field ");
            wname(io, st, text, ast.get(st, at, 4));
            w(io, "\n");
            dump_type(io, st, text, ast.get(st, at, 5));
            at = ast.next(st, at);
        }
        w(io, "I.Struct ");
        wname(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wbool(io, ast.get(st, id, 5));
        w(io, " ");
        wmode(io, ast.get(st, id, 6));
        wplace(io, st, text, id);
        return wspan(io, st, id);
    }
    if ast.is_kind(st, id, kinds.NK::IEnum) {
        generics_line(io, st, text, ast.get(st, id, 7));
        var at = ast.get(st, id, 8);
        while at >= 0 {
            dump_types(io, st, text, ast.get(st, at, 5), ast.get(st, at, 6));
            w(io, "V.Variant ");
            wname(io, st, text, ast.get(st, at, 4));
            w(io, " ");
            wn(io, ast.get(st, at, 6));
            w(io, "\n");
            at = ast.next(st, at);
        }
        w(io, "I.Enum ");
        wname(io, st, text, ast.get(st, id, 4));
        w(io, " ");
        wbool(io, ast.get(st, id, 5));
        w(io, " ");
        wmode(io, ast.get(st, id, 6));
        wplace(io, st, text, id);
        return wspan(io, st, id);
    }
    // A static.
    dump_type(io, st, text, ast.get(st, id, 6));
    dump_stmts(io, st, text, ast.get(st, id, 7), ast.get(st, id, 8));
    w(io, "I.Static ");
    wname(io, st, text, ast.get(st, id, 4));
    w(io, " ");
    wbool(io, ast.get(st, id, 5));
    w(io, " ");
    wn(io, ast.get(st, id, 8));
    wplace(io, st, text, id);
    return wspan(io, st, id);
}

// The imports, which the Rust AST keeps apart from the items, module by module.
pub fn dump_imports[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    var m = 0;
    while m < 2 {
        var n = 0;
        while n < st[12] {
            let record = st[16 + 3 * st[8] + n];
            if record % 2 == m {
                let keyword = record / 2;
                w(io, "M.Import ");
                wmodule(io, st, text, m);
                w(io, " ");
                var k = keyword + 1;
                var last = k;
                wname(io, st, text, k);
                while ast.code_at(st, k + 1) == lc.code(lc.Tok::Dot) {
                    w(io, ".");
                    wname(io, st, text, k + 2);
                    k = k + 2;
                    last = k;
                }
                k = k + 1;
                w(io, " ");
                if ast.code_at(st, k) == lc.code(lc.Tok::As) {
                    wname(io, st, text, k + 1);
                    k = k + 2;
                } else {
                    wname(io, st, text, last);
                }
                w(io, " @ ");
                wn(io, ast.tstart(st, keyword));
                w(io, " ");
                wn(io, ast.tend(st, k));
                w(io, "\n");
            }
            n = n + 1;
        }
        m = m + 1;
    }
    return 0;
}

// The whole tree of a parsed file: its items, then its imports.
pub fn dump[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte]) -> [io_write] int {
    var item = st[15];
    while item >= 0 {
        dump_item(io, st, text, item);
        item = ast.next(st, item);
    }
    return dump_imports(io, st, text);
}
