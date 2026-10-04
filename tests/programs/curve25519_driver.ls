// `docs/x25519.md` §4: `std.x25519` and `std.ed25519` driven from
// standard input, so the RFC 7748 vectors, Wycheproof and the OpenSSL
// differential all go through one program.
//
// One case per line, byte strings in lowercase hex (`-` for empty):
//
//     S <scalar> <u>          x25519.scalarmult: `<code> <tag> <result>`
//     I <n>                   RFC 7748 §5.2's iteration, from k = u = 9, n
//                             times: `0 ok <k after n>`
//     P <seed>                ed25519.public_key_from_seed: `0 ok <key>`
//     E <seed> <msg>          ed25519.sign: `0 ok <signature>`
//     V <key> <msg> <sig>     ed25519.verify: `<1 or 0> ok -`
import std.buffer;
import std.ed25519;
import std.io;
import std.x25519;

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

fn hex_len[&s](s: &s [byte], at: int) -> [] int {
    if field_end(s, at) - at == 1 && int_of(s[at]) == 45 {
        return 0;
    }
    return (field_end(s, at) - at) / 2;
}

fn hex_into[&s, &o](s: &s [byte], at: int, o: &!o [byte]) -> [] int {
    let n = hex_len(s, at);
    var i = 0;
    while i < len(o) && i < n {
        o[i] = byte_of(nibble(int_of(s[at + i * 2])) * 16 + nibble(int_of(s[at + i * 2 + 1])));
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
    return 0;
}

fn report[&i, &d](io: &!i Io, code: int, d: &d [byte]) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    io.write_all(io, x25519.refusal_tag(code));
    io.space(io);
    print_hex(io, d);
    io.newline(io);
    return 0;
}

fn one[&i, &s](io: &!i Io, s: &s [byte], at: int) -> [io_write] int {
    let op = int_of(s[at]);
    let f1 = at + 2;
    region r {
        if op == 83 {
            let k = alloc_slice[r](32, byte_of(0));
            let kn = hex_into(s, f1, k);
            let f2 = field_end(s, f1) + 1;
            let u = alloc_slice[r](32, byte_of(0));
            let un = hex_into(s, f2, u);
            let out = alloc_slice[r](32, byte_of(0));
            if kn != 32 || un != 32 {
                report(io, x25519.scalarmult(k[0..kn], u[0..un], out), out);
            } else {
                report(io, x25519.scalarmult(k, u, out), out);
            }
        } else if op == 80 || op == 69 || op == 86 {
            let a = alloc_slice[r](hex_len(s, f1), byte_of(0));
            hex_into(s, f1, a);
            if op == 80 {
                let pk = alloc_slice[r](32, byte_of(0));
                ed25519.public_key_from_seed(a, pk);
                report(io, 0, pk);
            } else {
                let f2 = field_end(s, f1) + 1;
                let msg = alloc_slice[r](hex_len(s, f2), byte_of(0));
                hex_into(s, f2, msg);
                if op == 69 {
                    let sig = alloc_slice[r](64, byte_of(0));
                    ed25519.sign(a, msg, sig);
                    report(io, 0, sig);
                } else {
                    // The signature as long as the input says: Wycheproof's
                    // cases include truncated and over-long ones.
                    let f3 = field_end(s, f2) + 1;
                    let sig = alloc_slice[r](hex_len(s, f3), byte_of(0));
                    hex_into(s, f3, sig);
                    report(io, ed25519.verify(a, msg, sig), sig[0..0]);
                }
            }
        } else {
            let e = field_end(s, f1);
            var n = 0;
            var p = f1;
            while p < e {
                n = n * 10 + int_of(s[p]) - 48;
                p = p + 1;
            }
            let k = alloc_slice[r](32, byte_of(0));
            let u = alloc_slice[r](32, byte_of(0));
            let next = alloc_slice[r](32, byte_of(0));
            k[0] = byte_of(9);
            u[0] = byte_of(9);
            var it = 0;
            while it < n {
                x25519.scalarmult(k, u, next);
                var j = 0;
                while j < 32 {
                    u[j] = k[j];
                    k[j] = next[j];
                    j = j + 1;
                }
                it = it + 1;
            }
            report(io, 0, k);
        }
    }
    var e = at;
    while e < len(s) && int_of(s[e]) != 10 {
        e = e + 1;
    }
    return e + 1;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        let s = buffer.bytes(b);
        var at = 0;
        while at < len(s) {
            at = one(io, s, at);
        }
    }
    buffer.drop(heap, text);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(ffi);
    release(fs);
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i);
        }
    }
    release(heap);
    release(io);
    return 0;
}
