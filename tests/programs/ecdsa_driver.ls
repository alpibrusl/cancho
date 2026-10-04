// `docs/ecdsa.md` §4: `std.ecdsa`, and `std.bigmod`'s registers, driven
// from standard input, so Wycheproof, NIST, OpenSSL and Python all go
// through one program.
//
// One case per line, byte strings in lowercase hex (`-` for empty):
//
//     V <curve> <h> <point> <msg> <sig>   ecdsa.verify_der of the hash of msg: `<code> <tag> -`
//     R <curve> <h> <point> <msg> <sig>   ecdsa.verify_raw
//     W <curve> <h> <point> <msg> <sig>   ecdsa.verify_raw with `work` only 10 words
//     D <curve> <point> <digest> <sig>    ecdsa.verify_der of a digest as given
//     F <n> <op> <a> <b>                  bigmod registers: `<code> <tag> <result>`, op `m` a*b, `a` a+b,
//                                         `s` a-b, `i` a^-1 (n prime), all mod n; `r` a mod n
//                                         by `load_reduced`, for a < 2n
//     T <rounds> <case>                   the case `rounds` times, answered once (`docs/ecdsa.md` §5.4)
//
// `curve` is 256 or 384; `h` a digest length in bytes (32, 48 or 64).
import std.bigmod;
import std.buffer;
import std.crypto;
import std.ecdsa;
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

fn next_field[&s](s: &s [byte], at: int) -> [] int {
    return field_end(s, at) + 1;
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

fn number[&s](s: &s [byte], at: int) -> [] int {
    let e = field_end(s, at);
    var n = 0;
    var p = at;
    while p < e {
        n = n * 10 + int_of(s[p]) - 48;
        p = p + 1;
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
    if len(d) == 0 {
        io.write_all(io, "-");
    }
    return 0;
}

fn report[&i, &d](io: &!i Io, code: int, d: &d [byte], show: bool) -> [io_write] int {
    if !show {
        return 0;
    }
    io.print_int(io, code);
    io.space(io);
    io.write_all(io, ecdsa.refusal_tag(code));
    io.space(io);
    print_hex(io, d);
    io.newline(io);
    return 0;
}

fn digest_of[&m, &o](h: int, msg: &m [byte], out: &!o [byte]) -> [] int {
    if h == 32 {
        return crypto.sha256(msg, out);
    }
    if h == 48 {
        return crypto.sha384(msg, out);
    }
    return crypto.sha512(msg, out);
}

// The register API on its own (`F`).
fn registers[&s, &w, &o](s: &s [byte], at: int, work: &!w [int], out: &!o [byte]) -> [] int {
    var f = at;
    var code = 0;
    region r {
        let n = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, n);
        f = next_field(s, f);
        let op = int_of(s[f]);
        f = next_field(s, f);
        let a = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, a);
        f = next_field(s, f);
        let b = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, b);
        code = bigmod.setup(n, work);
        if code == 0 && op == 114 {
            code = bigmod.load_reduced(a, work, bigmod.reg(2));
            if code == 0 {
                bigmod.store_reg(work, bigmod.reg(2), out[0..len(n)]);
            }
            return code;
        }
        if code == 0 {
            code = bigmod.load_reg(a, work, bigmod.reg(0));
        }
        if code == 0 {
            code = bigmod.load_reg(b, work, bigmod.reg(1));
        }
        if code == 0 {
            let x = bigmod.reg(0);
            let y = bigmod.reg(1);
            let z = bigmod.reg(2);
            if op == 109 {
                bigmod.to_mont(work, x, x);
                bigmod.to_mont(work, y, y);
                bigmod.mul(work, x, y, z);
                bigmod.from_mont(work, z, z);
            } else if op == 97 {
                bigmod.add(work, x, y, z);
            } else if op == 115 {
                bigmod.sub(work, x, y, z);
            } else {
                bigmod.to_mont(work, x, x);
                bigmod.inverse(work, x, z);
                bigmod.from_mont(work, z, z);
            }
            bigmod.store_reg(work, z, out[0..len(n)]);
        }
    }
    return code;
}

fn one[&i, &s, &w](io: &!i Io, s: &s [byte], at: int, work: &!w [int]) -> [io_write] int {
    if int_of(s[at]) == 84 {
        let rounds = number(s, at + 2);
        let rest = next_field(s, at + 2);
        var k = 1;
        while k < rounds {
            case(io, s, rest, work, false);
            k = k + 1;
        }
        return case(io, s, rest, work, true);
    }
    return case(io, s, at, work, true);
}

fn case[&i, &s, &w](io: &!i Io, s: &s [byte], at: int, work: &!w [int], show: bool) -> [io_write] int {
    let op = int_of(s[at]);
    var f = at + 2;
    region r {
        if op == 70 {
            let out = alloc_slice[r](hex_len(s, f), byte_of(0));
            let code = registers(s, f, work, out);
            if code == 0 {
                report(io, code, out, show);
            } else {
                report(io, code, out[0..0], show);
            }
        } else {
            let curve = number(s, f);
            f = next_field(s, f);
            var h = 0;
            if op != 68 {
                h = number(s, f);
                f = next_field(s, f);
            }
            let point = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, point);
            f = next_field(s, f);
            let msg = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, msg);
            f = next_field(s, f);
            let sig = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, sig);
            var code = 0;
            if op == 68 {
                code = ecdsa.verify_der(curve, msg, point, sig, work);
            } else {
                let digest = alloc_slice[r](h, byte_of(0));
                digest_of(h, msg, digest);
                if op == 86 {
                    code = ecdsa.verify_der(curve, digest, point, sig, work);
                } else if op == 87 {
                    code = ecdsa.verify_raw(curve, digest, point, sig, work[0..10]);
                } else {
                    code = ecdsa.verify_raw(curve, digest, point, sig, work);
                }
            }
            report(io, code, sig[0..0], show);
        }
    }
    return 0;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        let s = buffer.bytes(b);
        region w {
            let work = alloc_slice[w](ecdsa.work_len(), 0);
            var at = 0;
            while at < len(s) {
                if int_of(s[at]) != 10 {
                    one(io, s, at, work);
                }
                while at < len(s) && int_of(s[at]) != 10 {
                    at = at + 1;
                }
                at = at + 1;
            }
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
