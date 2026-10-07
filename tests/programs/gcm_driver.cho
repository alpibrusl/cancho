// `docs/tls-parity.md` §3.1: `std.aes` and `std.gcm` driven from
// standard input, so FIPS 197, NIST CAVP, Wycheproof and the pyca
// differential all go through one program.
//
// One case per line, fields separated by one space, byte strings in
// lowercase hex and `-` for an empty one:
//
//     E <key> <block>                     aes.expand, aes.encrypt_block
//     C <key> <iv> <counter> <input>      aes.ctr32
//     S <key> <nonce> <aad> <plaintext>   gcm.seal
//     O <key> <nonce> <aad> <sealed>      gcm.open
//     U <key> <nonce> <aad> <plaintext>   gcm.seal_with a context never prepared
//     s <key> <nonce> <aad> <plaintext>   gcm.seal_software (the software path whatever the CPU)
//     o <key> <nonce> <aad> <sealed>      gcm.open_software
//
// and one line out per case: `<code> <tag> <output in hex>`. `open`'s
// output starts filled with `0xaa`, so a refusal that wrote anything
// shows.
import std.aes;
import std.buffer;
import std.gcm;
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

// The end of the field that starts at `at`.
fn field_end[&s](s: &s [byte], at: int) -> [] int {
    var e = at;
    while e < len(s) && int_of(s[e]) != 32 && int_of(s[e]) != 10 {
        e = e + 1;
    }
    return e;
}

// How many bytes the hex field `s[at..end]` decodes to.
fn hex_len[&s](s: &s [byte], at: int, end: int) -> [] int {
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
    return 0;
}

fn report[&i, &d](io: &!i Io, code: int, tag: &static [byte], d: &d [byte]) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    io.write_all(io, tag);
    io.space(io);
    print_hex(io, d);
    io.newline(io);
    return 0;
}

fn block[&k, &b, &o](key: &k [byte], input: &b [byte], out: &!o [byte]) -> [] int {
    var code = 0;
    region r {
        let skey = alloc_slice[r](aes.skey_len(), 0);
        let nr = aes.expand(key, skey);
        if nr < 0 {
            code = nr;
        } else {
            code = aes.encrypt_block(nr, skey, input, out);
        }
    }
    return code;
}

fn ctr[&k, &v, &b, &o](key: &k [byte], iv: &v [byte], counter: int, input: &b [byte], out: &!o [byte]) -> [] int {
    var code = 0;
    region r {
        let skey = alloc_slice[r](aes.skey_len(), 0);
        let nr = aes.expand(key, skey);
        if nr < 0 {
            code = nr;
        } else {
            code = aes.ctr32(nr, skey, iv, counter, input, out);
        }
    }
    return code;
}

// One case, the line `s[at..]`; answers where the next line starts.
fn one[&i, &s](io: &!i Io, s: &s [byte], at: int) -> [io_write] int {
    let op = int_of(s[at]);
    let f1 = at + 2;
    let e1 = field_end(s, f1);
    let f2 = e1 + 1;
    let e2 = field_end(s, f2);
    var next = e2;
    region r {
        let key = alloc_slice[r](hex_len(s, f1, e1), byte_of(0));
        hex_into(s, f1, key);
        let second = alloc_slice[r](hex_len(s, f2, e2), byte_of(0));
        hex_into(s, f2, second);
        if op == 69 {
            let out = alloc_slice[r](16, byte_of(0));
            let code = block(key, second, out);
            report(io, code, aes.refusal_tag(code), out);
        } else {
            let f3 = e2 + 1;
            let e3 = field_end(s, f3);
            let f4 = e3 + 1;
            let e4 = field_end(s, f4);
            next = e4;
            let data = alloc_slice[r](hex_len(s, f4, e4), byte_of(0));
            hex_into(s, f4, data);
            if op == 67 {
                let out = alloc_slice[r](len(data), byte_of(0));
                let code = ctr(key, second, decimal(s, f3, e3), data, out);
                report(io, code, aes.refusal_tag(code), out);
            } else {
                let aad = alloc_slice[r](hex_len(s, f3, e3), byte_of(0));
                hex_into(s, f3, aad);
                if op == 83 {
                    let out = alloc_slice[r](len(data) + 16, byte_of(0));
                    let code = gcm.seal(key, second, aad, data, out);
                    report(io, code, gcm.refusal_tag(code), out);
                } else if op == 85 {
                    let out = alloc_slice[r](len(data) + 16, byte_of(0));
                    let ctx = alloc_slice[r](gcm.context_len(), 0);
                    let hw = alloc_slice[r](gcm.hw_len(), byte_of(0));
                    let code = gcm.seal_with(ctx, hw, second, aad, data, out);
                    report(io, code, gcm.refusal_tag(code), out);
                } else if op == 115 {
                    let out = alloc_slice[r](len(data) + 16, byte_of(0));
                    let ctx = alloc_slice[r](gcm.context_len(), 0);
                    let hw = alloc_slice[r](gcm.hw_len(), byte_of(0));
                    var code = gcm.prepare(key, ctx, hw);
                    if code == 0 {
                        code = gcm.seal_software(ctx, second, aad, data, out);
                    }
                    report(io, code, gcm.refusal_tag(code), out);
                } else {
                    var room = len(data) - 16;
                    if room < 0 {
                        room = 0;
                    }
                    let out = alloc_slice[r](room, byte_of(0xaa));
                    var code = 0;
                    if op == 111 {
                        let ctx = alloc_slice[r](gcm.context_len(), 0);
                        let hw = alloc_slice[r](gcm.hw_len(), byte_of(0));
                        code = gcm.prepare(key, ctx, hw);
                        if code == 0 {
                            code = gcm.open_software(ctx, second, aad, data, out);
                        }
                    } else {
                        code = gcm.open(key, second, aad, data, out);
                    }
                    report(io, code, gcm.refusal_tag(code), out);
                }
            }
        }
    }
    return next + 1;
}

fn cases[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {
    var at = 0;
    var n = 0;
    while at < len(s) {
        at = one(io, s, at);
        n = n + 1;
    }
    return n;
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
