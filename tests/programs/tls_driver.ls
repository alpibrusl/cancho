edition 5;

// `docs/tls-core.md` §6: `packages/tls` driven from standard input, one
// line at a time, each answered and flushed before the next is read, so
// a harness can shuttle bytes between this client and a real server.
// Byte strings are lowercase hex (`-` for empty).
//
//     S <suite> <key> <iv> <seq> <type> <plaintext>   tls_record.seal: `<code> <tag> <record>`
//     O <suite> <key> <iv> <seq> <record>             tls_record.open: `<code> <tag> <type> <content>`
//     T <suite> <key> <iv> <seq> <type> <plaintext>   tls_record.seal12 (TLS 1.2), answered as `S`
//     U <suite> <key> <iv> <seq> <record>             tls_record.open12 (TLS 1.2), answered as `O`
//     P <hash length> <secret> <label> <seed> <n>     tls_record.prf: `<code> <tag> <n bytes>`
//
// (`suite` in hex: 1301, 1302 or 1303.)
//     C <host> <random> <roots> <now>         tls_client.start (random: 96 bytes; roots: a PEM bundle, the trust
//                                             store; now: seconds since 1970, for the certificates' validity)
//     F <bytes>                               tls_client.feed, then everything `take` and `recv` give
//     W <plaintext>                           tls_client.send
//     Q                                       tls_client.finish
//
// `C`, `F`, `W` and `Q` answer `<code> <tag> <event> <bytes for the socket> <application data received>`.
import std.buffer;
import std.io;
import tls_client;
import tls_record;
import x509_verify;

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

fn tag_line[&i](io: &!i Io, code: int) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    if code > 0 || code == tls_client.would_block() {
        io.write_all(io, "ok");
    } else {
        io.write_all(io, tls_record.refusal_tag(code));
    }
    return 0;
}

fn record_op[&i, &s](io: &!i Io, s: &s [byte], at: int) -> [io_write] int {
    let op = int_of(s[at]);
    var f = at + 2;
    let suite = nibble(int_of(s[f])) * 4096 + nibble(int_of(s[f + 1])) * 256 + nibble(int_of(s[f + 2])) * 16 + nibble(int_of(s[f + 3]));
    f = next_field(s, f);
    region r {
        let key = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, key);
        f = next_field(s, f);
        let iv = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, iv);
        f = next_field(s, f);
        let seq = number(s, f);
        f = next_field(s, f);
        if op == 83 || op == 84 {
            let kind = number(s, f);
            f = next_field(s, f);
            let text = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, text);
            let out = alloc_slice[r](len(text) + 29, byte_of(0));
            var n = 0;
            if op == 83 {
                n = tls_record.seal(suite, key, iv, seq, kind, text, out);
            } else {
                n = tls_record.seal12(suite, key, iv, seq, kind, text, out);
            }
            if n > 0 {
                tag_line(io, 0);
                io.space(io);
                print_hex(io, out[0..n]);
            } else {
                tag_line(io, n);
                io.write_all(io, " -");
            }
        } else {
            let rec = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, rec);
            var room = len(rec) - 21;
            if room < 0 {
                room = 0;
            }
            let out = alloc_slice[r](room, byte_of(0));
            let info = alloc_slice[r](2, 0);
            var code = 0;
            if op == 79 {
                code = tls_record.open(suite, key, iv, seq, rec, out, info);
            } else {
                code = tls_record.open12(suite, key, iv, seq, rec, out, info);
            }
            tag_line(io, code);
            if code == 0 {
                io.space(io);
                io.print_int(io, info[0]);
                io.space(io);
                print_hex(io, out[0..info[1]]);
            } else {
                io.write_all(io, " -");
            }
        }
    }
    io.newline(io);
    return 0;
}

// `P <hash length> <secret> <label> <seed> <n>`: the TLS 1.2 PRF.
fn prf_op[&i, &s](io: &!i Io, s: &s [byte], at: int) -> [io_write] int {
    var f = at + 2;
    let h = number(s, f);
    f = next_field(s, f);
    region r {
        let secret = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, secret);
        f = next_field(s, f);
        let label = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, label);
        f = next_field(s, f);
        let seed = alloc_slice[r](hex_len(s, f), byte_of(0));
        hex_into(s, f, seed);
        f = next_field(s, f);
        let out = alloc_slice[r](number(s, f), byte_of(0));
        tag_line(io, tls_record.prf(h, secret, label, seed, out));
        io.space(io);
        print_hex(io, out);
    }
    io.newline(io);
    return 0;
}

// Everything `take` has, then everything `recv` has.
fn drain[&i, &n, &b, &o](io: &!i Io, ints: &!n [int], bytes: &!b [byte], out: &!o [byte]) -> [io_write] int {
    // The event once the output is taken: what the connection waits for
    // next.
    io.space(io);
    io.print_int(io, tls_client.event_after_take(ints));
    io.space(io);
    var any = false;
    var n = tls_client.take(ints, bytes, out);
    while n > 0 {
        print_hex(io, out[0..n]);
        any = true;
        n = tls_client.take(ints, bytes, out);
    }
    if !any {
        io.write_all(io, "-");
    }
    io.space(io);
    any = false;
    n = tls_client.recv(ints, bytes, out);
    while n > 0 {
        print_hex(io, out[0..n]);
        any = true;
        n = tls_client.recv(ints, bytes, out);
    }
    if !any {
        io.write_all(io, "-");
    }
    io.newline(io);
    return 0;
}

fn client_op[&i, &s, &n, &b, &o, &p, &e](io: &!i Io, s: &s [byte], at: int, ints: &!n [int], bytes: &!b [byte], out: &!o [byte], store: &!p [byte], store_len: int, held: &!e [byte]) -> [io_write] int {
    let op = int_of(s[at]);
    var f = at + 2;
    var code = 0;
    if op == 67 {
        region r {
            let host = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, host);
            f = next_field(s, f);
            let random = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, random);
            // The roots were loaded by `run`; the time is the fourth field.
            f = next_field(s, next_field(s, f));
            code = tls_client.start(ints, bytes, host, random, number(s, f));
        }
    } else if op == 70 {
        let data = out[0..hex_len(s, f)];
        hex_into(s, f, data);
        var consumed = 0;
        var going = true;
        // The input moves to `held` (on the heap, as it may be over a
        // region's 64 KiB: #208 found a 64 KiB Certificate trapped here),
        // since `out` is where the client's answer goes.
        var k = 0;
        while k < len(data) {
            held[k] = data[k];
            k = k + 1;
        }
        let input = held[0..len(data)];
        while going {
            let c = tls_client.feed(ints, bytes, input[consumed..len(input)], store[0..store_len]);
            if c < 0 {
                code = c;
                going = false;
            } else {
                consumed = consumed + c;
                code = consumed;
                if consumed == len(input) {
                    going = false;
                } else if c == 0 {
                    // Output or received data must be taken first: the
                    // harness reads both from this line, so the rest of
                    // the input is reported as not consumed.
                    going = false;
                }
            }
        }
    } else if op == 87 {
        region r {
            let text = alloc_slice[r](hex_len(s, f), byte_of(0));
            hex_into(s, f, text);
            code = tls_client.send(ints, bytes, text);
        }
    } else {
        code = tls_client.finish(ints, bytes);
    }
    tag_line(io, code);
    drain(io, ints, bytes, out);
    return 0;
}

// One line of standard input into `line`; its length, or -1 at the end.
fn read_line[&i, &l](io: &!i Io, line: &!l [byte]) -> [io_read] int {
    var n = 0;
    var c = getchar(io);
    if c < 0 {
        return -1;
    }
    while c >= 0 && c != 10 {
        if n < len(line) {
            line[n] = byte_of(c);
        }
        n = n + 1;
        c = getchar(io);
    }
    return n;
}

fn flush[&i](io: &!i Io) -> [io_write] int {
    match flush_out(io) {
        Done::Ok(n) => {
            return 0;
        }
        Done::Failed(e) => {
            return e;
        }
    }
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var line = box_slice(heap, 262144, byte_of(0));
    var slot_bytes = box_slice(heap, tls_client.bytes_len(), byte_of(0));
    var slot_ints = box_slice(heap, tls_client.ints_len(), 0);
    var out = box_slice(heap, 131072, byte_of(0));
    var store = box_slice(heap, 131072, byte_of(0));
    var held = box_slice(heap, 131072, byte_of(0));
    var store_len = 0;
    borrow mut line as &!lw in {
        borrow mut slot_bytes as &!bw in {
            borrow mut slot_ints as &!iw in {
                borrow mut out as &!ow in {
                    borrow mut store as &!pw in {
                        borrow mut held as &!ew in {
                            let l = contents(lw);
                            var n = read_line(io, l);
                            while n >= 0 {
                                if n > 0 && n <= len(l) {
                                    let s = l[0..n];
                                    let op = int_of(s[0]);
                                    if op == 83 || op == 79 || op == 84 || op == 85 {
                                        record_op(io, s, 0);
                                    } else if op == 80 {
                                        prf_op(io, s, 0);
                                    } else {
                                        if op == 67 {
                                            // The roots, a PEM bundle, are the third field: read
                                            // into `out` and loaded into the store.
                                            let o = contents(ow);
                                            let f = next_field(s, next_field(s, 2));
                                            let pn = hex_into(s, f, o);
                                            region q {
                                                let info = alloc_slice[q](2, 0);
                                                let roots = x509_verify.store_load(o[0..pn], contents(pw), info);
                                                store_len = info[0];
                                                if roots < 0 {
                                                    store_len = 0;
                                                }
                                            }
                                        }
                                        client_op(io, s, 0, contents(iw), contents(bw), contents(ow), contents(pw), store_len, contents(ew));
                                    }
                                }
                                flush(io);
                                n = read_line(io, l);
                            }
                        }
                    }
                }
            }
        }
    }
    unbox_slice(heap, line);
    unbox_slice(heap, slot_bytes);
    unbox_slice(heap, slot_ints);
    unbox_slice(heap, out);
    unbox_slice(heap, store);
    unbox_slice(heap, held);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(args);
    release(ffi);
    release(fs);
    release(net);
    release(clock);
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i);
        }
    }
    release(heap);
    release(io);
    return 0;
}
