// `docs/rsa.md` §4: `std.bigmod` and `std.rsa` driven from standard
// input, so Wycheproof, the NIST files, Python's `pow` and OpenSSL all
// go through one program.
//
// One case per line, byte strings in lowercase hex (`-` for empty):
//
//     M <n> <e> <a>                          bigmod.pow_mod: `<code> <tag> <a^e mod n>`
//     X <n> <e> <a>, Y <n> <e> <a>           the same with `out` one byte too long, or `work` only 10 words
//     P <h> <n> <e> <msg> <sig>              rsa.pkcs1_verify of the hash of msg: `<code> <tag> -`
//     D <h> <n> <e> <digest> <sig>           rsa.pkcs1_verify of a digest as given
//     S <h> <mgf h> <salt> <n> <e> <msg> <sig>   rsa.pss_verify
//     T <rounds> <case>                      the case `rounds` times, answered once (`docs/rsa.md` §5.5)
//
// `h` is a digest length in bytes (32, 48 or 64), as `std.rsa` names
// hashes.
import std.buffer;
import std.bigmod;
import std.crypto;
import std.io;
import std.rsa;

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
    io.write_all(io, rsa.refusal_tag(code));
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
    if h == 64 {
        return crypto.sha512(msg, out);
    }
    return 0;
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
        if op == 77 || op == 88 || op == 89 {
            let n = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, n);
            f = next_field(s, f);
            let e = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, e);
            f = next_field(s, f);
            let a = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, a);
            let out = alloc_slice[r](len(n) + 1, byte_of(0));
            var code = 0;
            if op == 77 {
                code = bigmod.pow_mod(n, e, a, out[0..len(n)], work);
            } else if op == 88 {
                code = bigmod.pow_mod(n, e, a, out, work);
            } else {
                code = bigmod.pow_mod(n, e, a, out[0..len(n)], work[0..10]);
            }
            if code == 0 {
                report(io, code, out[0..len(n)], show);
            } else {
                report(io, code, out[0..0], show);
            }
        } else {
            let h = number(s, f);
            f = next_field(s, f);
            var mgf = h;
            var salt = 0;
            if op == 83 {
                mgf = number(s, f);
                f = next_field(s, f);
                salt = number(s, f);
                f = next_field(s, f);
            }
            let n = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, n);
            f = next_field(s, f);
            let e = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, e);
            f = next_field(s, f);
            let msg = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, msg);
            f = next_field(s, f);
            let sig = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, sig);
            var code = 0;
            if op == 68 {
                code = rsa.pkcs1_verify(h, n, e, msg, sig, work);
            } else {
                let digest = alloc_slice[r](64, byte_of(0));
                var dl = h;
                if dl != 32 && dl != 48 && dl != 64 {
                    dl = 32;
                }
                digest_of(dl, msg, digest);
                if op == 80 {
                    code = rsa.pkcs1_verify(h, n, e, digest[0..dl], sig, work);
                } else {
                    code = rsa.pss_verify(h, mgf, salt, n, e, digest[0..dl], sig, work);
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
            let work = alloc_slice[w](rsa.work_len(), 0);
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
