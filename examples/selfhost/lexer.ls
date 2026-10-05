// lexer.ls -- the lex-sys tokeniser, written in lex-sys.
//
// A feasibility spike for `docs/self-hosting.md`, not a replacement for
// `crates/lex-sys-syntax/src/lexer.rs`: a line-for-line port of that file,
// so that any difference in behaviour is a finding about the language or
// about the Rust lexer, and not about a redesign.
//
// Reads a source file on standard input and writes one line per token,
//
//     Kind start end
//
// where `Kind` is the Rust `TokenKind` variant's name and the two numbers
// are the half-open byte span, or, if the Rust lexer would have refused the
// input, a single line
//
//     ERR rule-tag start end
//
//     lex-sys run examples/selfhost/lexer.ls --std < some_file.ls
//
// `examples/selfhost/diff.sh` runs it against the Rust lexer over a corpus.
//
// Generated parts (the `Tok` enum, `name`, the keyword table and the
// punctuation tables) were produced by a script from the Rust source's
// tables and are kept as plain source on purpose.

import std.io as console;
import std.buffer;

enum Tok {
    Ident,
    Int,
    Float,
    Fn,
    Let,
    Var,
    If,
    Else,
    While,
    Return,
    Defer,
    True,
    False,
    Struct,
    Enum,
    Match,
    Res,
    Val,
    Extern,
    Module,
    Import,
    Pub,
    Borrow,
    Region,
    As,
    In,
    Mut,
    Where,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Semi,
    Colon,
    ColonColon,
    DotDot,
    Arrow,
    FatArrow,
    Underscore,
    Eq,
    EqEq,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    Str,
    AmpAmp,
    Amp,
    PipePipe,
    Pipe,
    Caret,
    Tilde,
    LtLt,
    GtGt,
    Eof,
}

// One lexeme's worth of progress. `Skip` is whitespace or a comment: it moves
// the cursor and produces nothing, as in the Rust lexer.
enum Step {
    Skip(int),
    Token(Tok, int, int),
    Fail(int, int, int),
}

// Rule tags, as the Rust `Rule::tag` spells them.
fn literal_form() -> [] int {
    return 0;
}

fn unknown_escape() -> [] int {
    return 1;
}

fn unexpected_character() -> [] int {
    return 2;
}

fn rule_name(r: int) -> [] &static [byte] {
    if r == 0 {
        return "literal-form";
    }
    if r == 1 {
        return "unknown-escape";
    }
    return "unexpected-character";
}

fn name(t: Tok) -> [] &static [byte] {
    match t {
        Tok::Ident => {
            return "Ident";
        }
        Tok::Int => {
            return "Int";
        }
        Tok::Float => {
            return "Float";
        }
        Tok::Fn => {
            return "Fn";
        }
        Tok::Let => {
            return "Let";
        }
        Tok::Var => {
            return "Var";
        }
        Tok::If => {
            return "If";
        }
        Tok::Else => {
            return "Else";
        }
        Tok::While => {
            return "While";
        }
        Tok::Return => {
            return "Return";
        }
        Tok::Defer => {
            return "Defer";
        }
        Tok::True => {
            return "True";
        }
        Tok::False => {
            return "False";
        }
        Tok::Struct => {
            return "Struct";
        }
        Tok::Enum => {
            return "Enum";
        }
        Tok::Match => {
            return "Match";
        }
        Tok::Res => {
            return "Res";
        }
        Tok::Val => {
            return "Val";
        }
        Tok::Extern => {
            return "Extern";
        }
        Tok::Module => {
            return "Module";
        }
        Tok::Import => {
            return "Import";
        }
        Tok::Pub => {
            return "Pub";
        }
        Tok::Borrow => {
            return "Borrow";
        }
        Tok::Region => {
            return "Region";
        }
        Tok::As => {
            return "As";
        }
        Tok::In => {
            return "In";
        }
        Tok::Mut => {
            return "Mut";
        }
        Tok::Where => {
            return "Where";
        }
        Tok::LParen => {
            return "LParen";
        }
        Tok::RParen => {
            return "RParen";
        }
        Tok::LBrace => {
            return "LBrace";
        }
        Tok::RBrace => {
            return "RBrace";
        }
        Tok::LBracket => {
            return "LBracket";
        }
        Tok::RBracket => {
            return "RBracket";
        }
        Tok::Comma => {
            return "Comma";
        }
        Tok::Dot => {
            return "Dot";
        }
        Tok::Semi => {
            return "Semi";
        }
        Tok::Colon => {
            return "Colon";
        }
        Tok::ColonColon => {
            return "ColonColon";
        }
        Tok::DotDot => {
            return "DotDot";
        }
        Tok::Arrow => {
            return "Arrow";
        }
        Tok::FatArrow => {
            return "FatArrow";
        }
        Tok::Underscore => {
            return "Underscore";
        }
        Tok::Eq => {
            return "Eq";
        }
        Tok::EqEq => {
            return "EqEq";
        }
        Tok::BangEq => {
            return "BangEq";
        }
        Tok::Lt => {
            return "Lt";
        }
        Tok::LtEq => {
            return "LtEq";
        }
        Tok::Gt => {
            return "Gt";
        }
        Tok::GtEq => {
            return "GtEq";
        }
        Tok::Plus => {
            return "Plus";
        }
        Tok::Minus => {
            return "Minus";
        }
        Tok::Star => {
            return "Star";
        }
        Tok::Slash => {
            return "Slash";
        }
        Tok::Percent => {
            return "Percent";
        }
        Tok::Bang => {
            return "Bang";
        }
        Tok::Str => {
            return "Str";
        }
        Tok::AmpAmp => {
            return "AmpAmp";
        }
        Tok::Amp => {
            return "Amp";
        }
        Tok::PipePipe => {
            return "PipePipe";
        }
        Tok::Pipe => {
            return "Pipe";
        }
        Tok::Caret => {
            return "Caret";
        }
        Tok::Tilde => {
            return "Tilde";
        }
        Tok::LtLt => {
            return "LtLt";
        }
        Tok::GtGt => {
            return "GtGt";
        }
        Tok::Eof => {
            return "Eof";
        }
    }
}

// ---------------------------------------------------------- characters ---

// The byte at `i`, or -1 past the end. Everything below reads through this,
// which is what lets a lookahead be written without a bounds check.
fn at[&r](text: &r [byte], i: int) -> [] int {
    if i < 0 {
        return 0 - 1;
    }
    if i >= len(text) {
        return 0 - 1;
    }
    return int_of(text[i]);
}

fn is_digit(c: int) -> [] bool {
    return c >= '0' && c <= '9';
}

fn is_hex(c: int) -> [] bool {
    return is_digit(c) || c >= 'a' && c <= 'f' || c >= 'A' && c <= 'F';
}

fn is_ident_start(c: int) -> [] bool {
    return c == '_' || c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z';
}

fn is_ident_continue(c: int) -> [] bool {
    return is_ident_start(c) || is_digit(c);
}

// `u8::is_ascii_whitespace`: space, \t, \n, form feed, \r. Not vertical tab.
fn is_space(c: int) -> [] bool {
    return c == ' ' || c == '\t' || c == '\n' || c == 12 || c == '\r';
}

// The end of the (possibly multi-byte) character that starts at `i`.
fn char_end[&r](text: &r [byte], i: int) -> [] int {
    var end = i + 1;
    while end < len(text) && at(text, end) >= 128 && at(text, end) < 192 {
        end = end + 1;
    }
    return end;
}

// Is `c` one of the six escapes? `quote` is the delimiter of the literal.
fn escape_ok(c: int, quote: int) -> [] bool {
    return c == 'n' || c == 'r' || c == 't' || c == '\\' || c == quote || c == '0';
}

// Does `text[from..to]` spell `word`?
fn spells[&r, &w](text: &r [byte], from: int, to: int, word: &w [byte]) -> [] bool {
    if to - from != len(word) {
        return false;
    }
    var k = 0;
    while k < len(word) {
        if text[from + k] != word[k] {
            return false;
        }
        k = k + 1;
    }
    return true;
}

fn keyword[&r](text: &r [byte], from: int, to: int) -> [] Tok {
    if spells(text, from, to, "fn") {
        return Tok::Fn;
    }
    if spells(text, from, to, "let") {
        return Tok::Let;
    }
    if spells(text, from, to, "var") {
        return Tok::Var;
    }
    if spells(text, from, to, "if") {
        return Tok::If;
    }
    if spells(text, from, to, "else") {
        return Tok::Else;
    }
    if spells(text, from, to, "while") {
        return Tok::While;
    }
    if spells(text, from, to, "return") {
        return Tok::Return;
    }
    if spells(text, from, to, "defer") {
        return Tok::Defer;
    }
    if spells(text, from, to, "true") {
        return Tok::True;
    }
    if spells(text, from, to, "false") {
        return Tok::False;
    }
    if spells(text, from, to, "struct") {
        return Tok::Struct;
    }
    if spells(text, from, to, "enum") {
        return Tok::Enum;
    }
    if spells(text, from, to, "match") {
        return Tok::Match;
    }
    if spells(text, from, to, "res") {
        return Tok::Res;
    }
    if spells(text, from, to, "extern") {
        return Tok::Extern;
    }
    if spells(text, from, to, "module") {
        return Tok::Module;
    }
    if spells(text, from, to, "import") {
        return Tok::Import;
    }
    if spells(text, from, to, "pub") {
        return Tok::Pub;
    }
    if spells(text, from, to, "val") {
        return Tok::Val;
    }
    if spells(text, from, to, "borrow") {
        return Tok::Borrow;
    }
    if spells(text, from, to, "region") {
        return Tok::Region;
    }
    if spells(text, from, to, "as") {
        return Tok::As;
    }
    if spells(text, from, to, "in") {
        return Tok::In;
    }
    if spells(text, from, to, "mut") {
        return Tok::Mut;
    }
    if spells(text, from, to, "where") {
        return Tok::Where;
    }
    if spells(text, from, to, "_") {
        return Tok::Underscore;
    }
    return Tok::Ident;
}

// ------------------------------------------------------------ literals ---

fn string_lit[&r](text: &r [byte], start: int) -> [] Step {
    let n = len(text);
    var i = start + 1;
    while i < n && at(text, i) != '"' {
        if at(text, i) == '\n' {
            return Step::Fail(literal_form(), start, i);
        }
        if at(text, i) == '\\' {
            // A backslash as the very last byte leaves the loop with `i`
            // still short of the end: see the report on this port.
            if i + 1 >= n {
                return Step::Token(Tok::Str, start, i + 1);
            }
            if !escape_ok(at(text, i + 1), '"') {
                return Step::Fail(unknown_escape(), i, char_end(text, i + 1));
            }
            i = i + 2;
        } else {
            i = i + 1;
        }
    }
    if i == n {
        return Step::Fail(literal_form(), start, n);
    }
    return Step::Token(Tok::Str, start, i + 1);
}

fn char_lit[&r](text: &r [byte], start: int) -> [] Step {
    let n = len(text);
    var j = start + 1;
    let c = at(text, j);
    if c < 0 {
        return Step::Fail(literal_form(), start, n);
    }
    if c == '\'' {
        return Step::Fail(literal_form(), start, j + 1);
    }
    if c == '\n' {
        return Step::Fail(literal_form(), start, j);
    }
    if c == '\\' {
        let esc = at(text, j + 1);
        if esc < 0 {
            return Step::Fail(literal_form(), start, n);
        }
        if !escape_ok(esc, '\'') {
            return Step::Fail(unknown_escape(), j, char_end(text, j + 1));
        }
        j = j + 2;
    } else {
        if c >= 128 {
            return Step::Fail(literal_form(), start, char_end(text, j));
        }
        j = j + 1;
    }
    if at(text, j) == '\'' {
        return Step::Token(Tok::Int, start, j + 1);
    }
    var scan = j;
    while scan < n && at(text, scan) != '\n' && at(text, scan) != '\'' {
        scan = scan + 1;
    }
    if at(text, scan) == '\'' {
        return Step::Fail(literal_form(), start, scan + 1);
    }
    return Step::Fail(literal_form(), start, scan);
}

fn digits[&r](text: &r [byte], from: int) -> [] int {
    var j = from;
    while is_digit(at(text, j)) || at(text, j) == '_' {
        j = j + 1;
    }
    return j;
}

fn number[&r](text: &r [byte], start: int, after_dot: bool) -> [] Step {
    let hex = at(text, start) == '0' && at(text, start + 1) == 'x';
    var j = start;
    if hex {
        j = start + 2;
        if !is_hex(at(text, j)) {
            return Step::Fail(literal_form(), start, j);
        }
        while is_hex(at(text, j)) || at(text, j) == '_' {
            j = j + 1;
        }
    } else {
        j = digits(text, start);
    }
    var float = false;
    if !hex && !after_dot {
        if at(text, j) == '.' && is_digit(at(text, j + 1)) {
            float = true;
            j = digits(text, j + 1);
        }
        if at(text, j) == 'e' || at(text, j) == 'E' {
            var after = j + 1;
            if at(text, after) == '+' || at(text, after) == '-' {
                after = after + 1;
            }
            if is_digit(at(text, after)) {
                float = true;
                j = digits(text, after);
            }
        }
    }
    if is_ident_continue(at(text, j)) {
        return Step::Fail(unexpected_character(), j, j + 1);
    }
    if float {
        return Step::Token(Tok::Float, start, j);
    }
    return Step::Token(Tok::Int, start, j);
}

// --------------------------------------------------------- punctuation ---

fn punct[&r](text: &r [byte], i: int) -> [] Step {
    let b = at(text, i);
    let nx = at(text, i + 1);
    if b == '-' && nx == '>' {
        return Step::Token(Tok::Arrow, i, i + 2);
    }
    if b == '&' && nx == '&' {
        return Step::Token(Tok::AmpAmp, i, i + 2);
    }
    if b == '|' && nx == '|' {
        return Step::Token(Tok::PipePipe, i, i + 2);
    }
    if b == '=' && nx == '=' {
        return Step::Token(Tok::EqEq, i, i + 2);
    }
    if b == '=' && nx == '>' {
        return Step::Token(Tok::FatArrow, i, i + 2);
    }
    if b == ':' && nx == ':' {
        return Step::Token(Tok::ColonColon, i, i + 2);
    }
    if b == '.' && nx == '.' {
        return Step::Token(Tok::DotDot, i, i + 2);
    }
    if b == '!' && nx == '=' {
        return Step::Token(Tok::BangEq, i, i + 2);
    }
    if b == '<' && nx == '=' {
        return Step::Token(Tok::LtEq, i, i + 2);
    }
    if b == '>' && nx == '=' {
        return Step::Token(Tok::GtEq, i, i + 2);
    }
    if b == '<' && nx == '<' {
        return Step::Token(Tok::LtLt, i, i + 2);
    }
    if b == '>' && nx == '>' {
        return Step::Token(Tok::GtGt, i, i + 2);
    }
    if b == '(' {
        return Step::Token(Tok::LParen, i, i + 1);
    }
    if b == ')' {
        return Step::Token(Tok::RParen, i, i + 1);
    }
    if b == '{' {
        return Step::Token(Tok::LBrace, i, i + 1);
    }
    if b == '}' {
        return Step::Token(Tok::RBrace, i, i + 1);
    }
    if b == '[' {
        return Step::Token(Tok::LBracket, i, i + 1);
    }
    if b == ']' {
        return Step::Token(Tok::RBracket, i, i + 1);
    }
    if b == ',' {
        return Step::Token(Tok::Comma, i, i + 1);
    }
    if b == '.' {
        return Step::Token(Tok::Dot, i, i + 1);
    }
    if b == ';' {
        return Step::Token(Tok::Semi, i, i + 1);
    }
    if b == ':' {
        return Step::Token(Tok::Colon, i, i + 1);
    }
    if b == '=' {
        return Step::Token(Tok::Eq, i, i + 1);
    }
    if b == '<' {
        return Step::Token(Tok::Lt, i, i + 1);
    }
    if b == '>' {
        return Step::Token(Tok::Gt, i, i + 1);
    }
    if b == '+' {
        return Step::Token(Tok::Plus, i, i + 1);
    }
    if b == '-' {
        return Step::Token(Tok::Minus, i, i + 1);
    }
    if b == '*' {
        return Step::Token(Tok::Star, i, i + 1);
    }
    if b == '/' {
        return Step::Token(Tok::Slash, i, i + 1);
    }
    if b == '%' {
        return Step::Token(Tok::Percent, i, i + 1);
    }
    if b == '!' {
        return Step::Token(Tok::Bang, i, i + 1);
    }
    if b == '|' {
        return Step::Token(Tok::Pipe, i, i + 1);
    }
    if b == '^' {
        return Step::Token(Tok::Caret, i, i + 1);
    }
    if b == '~' {
        return Step::Token(Tok::Tilde, i, i + 1);
    }
    if b == '&' {
        return Step::Token(Tok::Amp, i, i + 1);
    }
    return Step::Fail(unexpected_character(), i, char_end(text, i));
}

// ---------------------------------------------------------------- step ---

// Advance past one thing: whitespace, a comment, or one token.
fn step[&r](text: &r [byte], i: int, after_dot: bool) -> [] Step {
    let b = at(text, i);
    if is_space(b) {
        return Step::Skip(i + 1);
    }
    if b == '/' && at(text, i + 1) == '/' {
        var j = i;
        while j < len(text) && at(text, j) != '\n' {
            j = j + 1;
        }
        return Step::Skip(j);
    }
    if b == '"' {
        return string_lit(text, i);
    }
    if b == '\'' {
        return char_lit(text, i);
    }
    if is_digit(b) {
        return number(text, i, after_dot);
    }
    if is_ident_start(b) {
        var j = i;
        while is_ident_continue(at(text, j)) {
            j = j + 1;
        }
        return Step::Token(keyword(text, i, j), i, j);
    }
    return punct(text, i);
}

fn is_dot(t: Tok) -> [] bool {
    match t {
        Tok::Dot => {
            return true;
        }
        _ => {
            return false;
        }
    }
}

// ---------------------------------------------------------------- main ---

fn emit[&i](io: &!i Io, label: &static [byte], from: int, to: int) -> [io_write] int {
    console.write_all(io, label);
    console.space(io);
    console.print_nat(io, from);
    console.space(io);
    console.print_nat(io, to);
    return console.newline(io);
}

// Tokenise all of `text`. A refusal is always printed; the tokens only when
// `show` is set. 0 on success, 1 on a refusal.
//
// `main` runs this twice, silently and then aloud, because the Rust lexer
// answers with the tokens or the refusal and never both, and a streaming
// port would print the tokens before the error that voids them.
fn lex_all[&s, &i](io: &!i Io, text: &s [byte], show: bool) -> [io_write] int {
    var pos = 0;
    var after_dot = false;
    while true {
        if pos >= len(text) {
            if show {
                emit(io, "Eof", len(text), len(text));
            }
            return 0;
        }
        match step(text, pos, after_dot) {
            Step::Skip(next) => {
                pos = next;
            }
            Step::Token(kind, from, to) => {
                if show {
                    emit(io, name(kind), from, to);
                }
                after_dot = is_dot(kind);
                pos = to;
            }
            Step::Fail(rule, from, to) => {
                console.write_all(io, "ERR ");
                emit(io, rule_name(rule), from, to);
                return 1;
            }
        }
    }
    return 1;
}

fn slurp[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read] buffer.Buffer {
    var out = buffer.empty(heap, 4096);
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(args);
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            let text = slurp(h, i);
            borrow text as &t in {
                status = lex_all(i, buffer.bytes(t), false);
                if status == 0 {
                    status = lex_all(i, buffer.bytes(t), true);
                }
            }
            buffer.drop(h, text);
        }
    }
    release(heap);
    release(io);
    return status;
}
