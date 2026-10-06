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
// `examples/selfhost/diff.sh` runs it against the Rust lexer over a corpus.
//
// The tokeniser itself is `lexcore.ls`, a module, so that `parser.ls` can use it too:
//
//     lex-sys run examples/selfhost/lexer.ls examples/selfhost/lexcore.ls --std < f.ls

import std.io as console;
import std.buffer;
import selfhost.lexcore as lc;

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
        match lc.step(text, pos, after_dot) {
            lc.Step::Skip(next) => {
                pos = next;
            }
            lc.Step::Token(kind, from, to) => {
                if show {
                    emit(io, lc.name(kind), from, to);
                }
                after_dot = lc.is_dot(kind);
                pos = to;
            }
            lc.Step::Fail(rule, from, to) => {
                console.write_all(io, "ERR ");
                emit(io, lc.rule_name(rule), from, to);
                return 1;
            }
        }
    }
    return 1;
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
