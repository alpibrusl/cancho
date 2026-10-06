module selfhost.driver;

// driver.ls -- reading a source file, running the front end over it, and printing the answer.
//
// The two programs of `examples/selfhost` that read a file on standard input, `parser.ls` and
// `check.ls`, differ only in what they print once it is parsed, so the reading, the allocation
// and the refusal of the parser are written once, here, and each program's `main` says which
// answer it wants:
//
//     mode 0   the syntax tree as a listing (`listing.ls`)
//     mode 1   the checker's answer (`pass1.ls`): `OK`, `SKIP`, or the first refusal
//
// Either way a refusal of the parser is the one line `ERR rule-tag start end`.

import std.io as console;
import std.buffer;
import selfhost.lexcore as lc;
import selfhost.ast;
import selfhost.listing;
import selfhost.pass1;

fn refusal[&i, &s](io: &!i Io, st: &!s [int]) -> [io_write] int {
    console.write_all(io, "ERR ");
    console.write_all(io, ast.rule_tag(st[3]));
    console.space(io);
    console.print_nat(io, st[4]);
    console.space(io);
    console.print_nat(io, st[5]);
    console.newline(io);
    return 1;
}

fn answer[&i, &s, &x](io: &!i Io, st: &!s [int], text: &x [byte], mode: int) -> [io_write] int {
    ast.layout(st, len(text));
    if ast.parse(st, text) != 0 {
        return refusal(io, st);
    }
    if mode == 0 {
        listing.dump(io, st, text);
        return 0;
    }
    if pass1.check(st, text) == 0 {
        console.write_all(io, "OK");
        console.newline(io);
        return 0;
    }
    if st[3] == ast.r_skip() {
        console.write_all(io, "SKIP");
        console.newline(io);
        return 0;
    }
    return refusal(io, st);
}

// Read standard input, parse it, and print the answer for `mode`. The exit status is 1 for a
// refusal.
pub fn drive(world: World, mode: int) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(args);
    var status = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            let text = lc.slurp(h, i);
            borrow text as &t in {
                let state = box_slice(h, ast.size_for(buffer.size(t)), 0);
                borrow mut state as &!b in {
                    status = answer(i, contents(b), buffer.bytes(t), mode);
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
