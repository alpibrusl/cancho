edition 7;

// `docs/crypto-builtins.md` §7: the hardware AES and GHASH builtins,
// driven from standard input. One case per line, byte strings in
// lowercase hex:
//
//     H                                  hw_aes_gcm(): 1 or 0
//     A <rounds> <round keys> <block>    aes_encrypt_block: the block out
//     G <h> <y> <data>                   ghash_update: y after
//
// and one line out per case. `A` and `G` are only sent when `H` said 1.
import std.buffer;
import std.io;

fn read_stdin[&h, &i](heap: &!h Heap, io: &!i Io, text: buffer.Buffer) -> [heap, io_read] buffer.Buffer {
    var out = text;
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

fn nibble(c: int) -> [] int {
    if c >= 97 {
        return c - 87;
    }
    return c - 48;
}

fn field_end[&s](s: &s [byte], at: int) -> [] int {
    var e = at;
    while e < len(s) && int_of(s[e]) != 32 && int_of(s[e]) != 10 {
        e = e + 1;
    }
    return e;
}

fn hex_into[&s, &o](s: &s [byte], at: int, o: &!o [byte]) -> [] int {
    var i = 0;
    while i < len(o) {
        o[i] = byte_of(nibble(int_of(s[at + i * 2])) * 16 + nibble(int_of(s[at + i * 2 + 1])));
        i = i + 1;
    }
    return 0;
}

fn decimal[&s](s: &s [byte], at: int, end: int) -> [] int {
    var n = 0;
    var i = at;
    while i < end {
        n = n * 10 + int_of(s[i]) - 48;
        i = i + 1;
    }
    return n;
}

fn print_hex[&i, &d](io: &!i Io, d: &d [byte]) -> [io_write] int {
    let digits = "0123456789abcdef";
    var n = 0;
    while n < len(d) {
        let b = int_of(d[n]);
        io.write_all(io, digits[b >> 4..(b >> 4) + 1]);
        io.write_all(io, digits[b & 15..(b & 15) + 1]);
        n = n + 1;
    }
    io.newline(io);
    return 0;
}

// One case, the line starting at `at`; answers where the next line starts.
fn one[&i, &s](io: &!i Io, s: &s [byte], at: int) -> [io_write] int {
    let op = int_of(s[at]);
    if op == 72 {
        if hw_aes_gcm() {
            io.write_all(io, "1");
        } else {
            io.write_all(io, "0");
        }
        io.newline(io);
        return at + 2;
    }
    let f1 = at + 2;
    let e1 = field_end(s, f1);
    let f2 = e1 + 1;
    let e2 = field_end(s, f2);
    let f3 = e2 + 1;
    let e3 = field_end(s, f3);
    region r {
        if op == 65 {
            let rounds = decimal(s, f1, e1);
            let keys = alloc_slice[r]((e2 - f2) / 2, byte_of(0));
            hex_into(s, f2, keys);
            let block = alloc_slice[r]((e3 - f3) / 2, byte_of(0));
            hex_into(s, f3, block);
            let out = alloc_slice[r](16, byte_of(0));
            aes_encrypt_block(keys, rounds, block, out);
            print_hex(io, out);
        } else {
            let h = alloc_slice[r]((e1 - f1) / 2, byte_of(0));
            hex_into(s, f1, h);
            let y = alloc_slice[r]((e2 - f2) / 2, byte_of(0));
            hex_into(s, f2, y);
            let data = alloc_slice[r]((e3 - f3) / 2, byte_of(0));
            hex_into(s, f3, data);
            ghash_update(h, y, data);
            print_hex(io, y);
        }
    }
    return e3 + 1;
}

fn cases[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {
    var at = 0;
    while at < len(s) {
        at = one(io, s, at);
    }
    return 0;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        cases(io, buffer.bytes(b));
    }
    buffer.drop(heap, text);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(fs);
    release(args);
    release(net);
    release(clock);
    release(signals);
    release(exec);
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i);
        }
    }
    release(heap);
    release(io);
    return 0;
}
