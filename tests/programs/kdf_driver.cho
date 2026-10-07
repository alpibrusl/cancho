// `docs/hkdf.md` §4: `std.crypto`'s SHA-2 family, `std.hmac` and
// `std.hkdf` driven from standard input, so the CAVP files, RFC 4231,
// RFC 5869, the TLS 1.3 key schedules, Wycheproof and the differential
// all go through one program.
//
// One case per line, fields separated by one space, byte strings in
// lowercase hex (`-` for an empty one), numbers in decimal. `<alg>` is
// 256, 384 or 512; `<hl>` is a digest length, 32 or 48.
//
//     H <alg> <msg>                         one-shot hash
//     U <alg> <piece> <msg>                 streamed, `<piece>` bytes per update
//     A <alg> <count> <byte>                `<count>` copies of one byte, streamed
//     B <alg> <count>                       one-shot over `<count>` bytes of `a` on the heap
//     C <alg> <seed>                        the CAVP Monte Carlo test: 100 checkpoints
//     M <hl> <out> <key> <msg>              hmac.mac into `<out>` bytes
//     E <hl> <out> <salt> <ikm>             hkdf.extract into `<out>` bytes
//     X <hl> <prk> <info> <out>             hkdf.expand
//     L <hl> <secret> <label> <ctx> <out>   hkdf.expand_label
//     D <hl> <out> <secret> <label> <hash>  hkdf.derive_secret
//     K <hl> <salt> <ikm> <info> <out>      extract, then expand: all of HKDF
//
// and one line out per case: `<code> <tag> <output in hex>` (for `C`,
// the checkpoints joined by `,`).
import std.buffer;
import std.crypto;
import std.hkdf;
import std.hmac;
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

// Where field `k` (0 is the operation) of the line at `at` starts.
fn field[&s](s: &s [byte], at: int, k: int) -> [] int {
    var f = at;
    var n = 0;
    while n < k {
        f = field_end(s, f) + 1;
        n = n + 1;
    }
    return f;
}

fn hex_len[&s](s: &s [byte], at: int) -> [] int {
    let end = field_end(s, at);
    if end - at == 1 && int_of(s[at]) == 45 {
        return 0;
    }
    return (end - at) / 2;
}

fn hex_into[&s, &o](s: &s [byte], at: int, o: &!o [byte]) -> [] int {
    var i = 0;
    while i < len(o) {
        o[i] = byte_of(nibble(int_of(s[at + i * 2])) * 16 + nibble(int_of(s[at + i * 2 + 1])));
        i = i + 1;
    }
    return 0;
}

fn decimal[&s](s: &s [byte], at: int) -> [] int {
    let end = field_end(s, at);
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
    return 0;
}

fn report[&i, &d](io: &!i Io, code: int, d: &d [byte]) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    io.write_all(io, hkdf.refusal_tag(code));
    io.space(io);
    print_hex(io, d);
    io.newline(io);
    return 0;
}

fn digest_len(alg: int) -> [] int {
    return alg / 8;
}

fn one_shot[&m, &o](alg: int, msg: &m [byte], out: &!o [byte]) -> [] int {
    if alg == 256 {
        return crypto.sha256(msg, out);
    }
    if alg == 384 {
        return crypto.sha384(msg, out);
    }
    return crypto.sha512(msg, out);
}

fn start[&st](alg: int, st: &!st [int]) -> [] int {
    if alg == 256 {
        return crypto.sha256_init(st);
    }
    if alg == 384 {
        return crypto.sha384_init(st);
    }
    return crypto.sha512_init(st);
}

fn feed[&st, &d](alg: int, st: &!st [int], data: &d [byte]) -> [] int {
    if alg == 256 {
        return crypto.sha256_update(st, data);
    }
    if alg == 384 {
        return crypto.sha384_update(st, data);
    }
    return crypto.sha512_update(st, data);
}

fn finish[&st, &o](alg: int, st: &!st [int], out: &!o [byte]) -> [] int {
    if alg == 256 {
        return crypto.sha256_final(st, out);
    }
    if alg == 384 {
        return crypto.sha384_final(st, out);
    }
    return crypto.sha512_final(st, out);
}

fn state_words(alg: int) -> [] int {
    if alg == 256 {
        return crypto.sha256_state_len();
    }
    return crypto.sha512_state_len();
}

// `msg` fed `piece` bytes at a time (the last piece shorter).
fn streamed[&m, &o](alg: int, piece: int, msg: &m [byte], out: &!o [byte]) -> [] int {
    region r {
        let st = alloc_slice[r](state_words(alg), 0);
        start(alg, st);
        var at = 0;
        while at < len(msg) {
            var end = at + piece;
            if end > len(msg) {
                end = len(msg);
            }
            feed(alg, st, msg[at..end]);
            at = end;
        }
        finish(alg, st, out);
    }
    return 0;
}

fn repeated[&o](alg: int, count: int, b: int, out: &!o [byte]) -> [] int {
    region r {
        let st = alloc_slice[r](state_words(alg), 0);
        let chunk = alloc_slice[r](1000, byte_of(b));
        start(alg, st);
        var left = count;
        while left > 0 {
            var n = 1000;
            if left < n {
                n = left;
            }
            feed(alg, st, chunk[0..n]);
            left = left - n;
        }
        finish(alg, st, out);
    }
    return 0;
}

// FIPS 180-4's Monte Carlo test as CAVP runs it: 100 checkpoints, each
// 1,000 hashes of the previous three digests concatenated.
fn monte[&i, &s](io: &!i Io, alg: int, seed: &s [byte]) -> [io_write] int {
    let d = digest_len(alg);
    io.write_all(io, "0 ok ");
    region r {
        let md = alloc_slice[r](3 * d, byte_of(0));
        let next = alloc_slice[r](d, byte_of(0));
        let current = alloc_slice[r](d, byte_of(0));
        var k = 0;
        while k < d {
            current[k] = seed[k];
            k = k + 1;
        }
        var j = 0;
        while j < 100 {
            k = 0;
            while k < d {
                md[k] = current[k];
                md[d + k] = current[k];
                md[2 * d + k] = current[k];
                k = k + 1;
            }
            var i = 3;
            while i < 1003 {
                one_shot(alg, md, next);
                k = 0;
                while k < 2 * d {
                    md[k] = md[d + k];
                    k = k + 1;
                }
                k = 0;
                while k < d {
                    md[2 * d + k] = next[k];
                    k = k + 1;
                }
                i = i + 1;
            }
            k = 0;
            while k < d {
                current[k] = next[k];
                k = k + 1;
            }
            if j > 0 {
                io.write_all(io, ",");
            }
            print_hex(io, current);
            j = j + 1;
        }
    }
    io.newline(io);
    return 0;
}

// One case, the line `s[at..]`; answers where the next line starts.
fn one[&h, &i, &s](heap: &!h Heap, io: &!i Io, s: &s [byte], at: int) -> [heap, io_write] int {
    let op = int_of(s[at]);
    let f1 = field(s, at, 1);
    let n1 = decimal(s, f1);
    region r {
        if op == 72 || op == 85 {
            var mf = field(s, at, 2);
            var piece = 0;
            if op == 85 {
                piece = decimal(s, mf);
                mf = field(s, at, 3);
            }
            // On the heap: a message can be larger than an arena.
            let msg = box_slice(heap, hex_len(s, mf), byte_of(0));
            let out = alloc_slice[r](digest_len(n1), byte_of(0));
            var code = 0;
            borrow mut msg as &!mw in {
                hex_into(s, mf, contents(mw));
            }
            borrow msg as &m in {
                if op == 72 {
                    code = one_shot(n1, contents(m), out);
                } else {
                    code = streamed(n1, piece, contents(m), out);
                }
            }
            unbox_slice(heap, msg);
            report(io, code, out);
        } else if op == 65 {
            let out = alloc_slice[r](digest_len(n1), byte_of(0));
            let bf = field(s, at, 3);
            let b = nibble(int_of(s[bf])) * 16 + nibble(int_of(s[bf + 1]));
            report(io, repeated(n1, decimal(s, field(s, at, 2)), b, out), out);
        } else if op == 66 {
            let out = alloc_slice[r](digest_len(n1), byte_of(0));
            let big = box_slice(heap, decimal(s, field(s, at, 2)), byte_of(97));
            var code = 0;
            borrow big as &bg in {
                code = one_shot(n1, contents(bg), out);
            }
            unbox_slice(heap, big);
            report(io, code, out);
        } else if op == 67 {
            let sf = field(s, at, 2);
            let seed = alloc_slice[r](hex_len(s, sf), byte_of(0));
            hex_into(s, sf, seed);
            monte(io, n1, seed);
        } else if op == 77 || op == 69 {
            let out = alloc_slice[r](decimal(s, field(s, at, 2)), byte_of(0));
            let kf = field(s, at, 3);
            let key = alloc_slice[r](hex_len(s, kf), byte_of(0));
            hex_into(s, kf, key);
            let mf = field(s, at, 4);
            let msg = alloc_slice[r](hex_len(s, mf), byte_of(0));
            hex_into(s, mf, msg);
            if op == 77 {
                report(io, hmac.mac(n1, key, msg, out), out);
            } else {
                report(io, hkdf.extract(n1, key, msg, out), out);
            }
        } else if op == 88 {
            let pf = field(s, at, 2);
            let prk = alloc_slice[r](hex_len(s, pf), byte_of(0));
            hex_into(s, pf, prk);
            let inf = field(s, at, 3);
            let info = alloc_slice[r](hex_len(s, inf), byte_of(0));
            hex_into(s, inf, info);
            let out = alloc_slice[r](decimal(s, field(s, at, 4)), byte_of(0));
            report(io, hkdf.expand(n1, prk, info, out), out);
        } else if op == 76 {
            let sf = field(s, at, 2);
            let secret = alloc_slice[r](hex_len(s, sf), byte_of(0));
            hex_into(s, sf, secret);
            let lf = field(s, at, 3);
            let label = alloc_slice[r](hex_len(s, lf), byte_of(0));
            hex_into(s, lf, label);
            let cf = field(s, at, 4);
            let ctx = alloc_slice[r](hex_len(s, cf), byte_of(0));
            hex_into(s, cf, ctx);
            let out = alloc_slice[r](decimal(s, field(s, at, 5)), byte_of(0));
            report(io, hkdf.expand_label(n1, secret, label, ctx, out), out);
        } else if op == 75 {
            let sf = field(s, at, 2);
            let salt = alloc_slice[r](hex_len(s, sf), byte_of(0));
            hex_into(s, sf, salt);
            let kf = field(s, at, 3);
            let ikm = alloc_slice[r](hex_len(s, kf), byte_of(0));
            hex_into(s, kf, ikm);
            let inf = field(s, at, 4);
            let info = alloc_slice[r](hex_len(s, inf), byte_of(0));
            hex_into(s, inf, info);
            let out = alloc_slice[r](decimal(s, field(s, at, 5)), byte_of(0));
            let prk = alloc_slice[r](n1, byte_of(0));
            var code = hkdf.extract(n1, salt, ikm, prk);
            if code == 0 {
                code = hkdf.expand(n1, prk, info, out);
            }
            report(io, code, out);
        } else {
            let out = alloc_slice[r](decimal(s, field(s, at, 2)), byte_of(0));
            let sf = field(s, at, 3);
            let secret = alloc_slice[r](hex_len(s, sf), byte_of(0));
            hex_into(s, sf, secret);
            let lf = field(s, at, 4);
            let label = alloc_slice[r](hex_len(s, lf), byte_of(0));
            hex_into(s, lf, label);
            let hf = field(s, at, 5);
            let th = alloc_slice[r](hex_len(s, hf), byte_of(0));
            hex_into(s, hf, th);
            report(io, hkdf.derive_secret(n1, secret, label, th, out), out);
        }
    }
    var e = at;
    while e < len(s) && int_of(s[e]) != 10 {
        e = e + 1;
    }
    return e + 1;
}

fn cases[&h, &i, &s](heap: &!h Heap, io: &!i Io, s: &s [byte]) -> [heap, io_write] int {
    var at = 0;
    var n = 0;
    while at < len(s) {
        at = one(heap, io, s, at);
        n = n + 1;
    }
    return n;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        cases(heap, io, buffer.bytes(b));
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
