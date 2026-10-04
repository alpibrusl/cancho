// `docs/x509-verify.md` §6: `packages/x509`'s verifier driven from
// standard input, one line at a time.
//
//     S <PEM bundle in hex>
//         the store becomes that bundle's roots; answers
//         `<roots> <skipped> <bytes used>`, or `<code> <tag>`
//     V <now> <max intermediates> <host in hex, or -> <cert hex> ...
//         the chain, leaf first, against the store; answers `<code> <tag>`
import std.buffer;
import std.io;
import x509_verify;

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

fn field_end[&s](s: &s [byte], at: int, e: int) -> [] int {
    var k = at;
    while k < e && int_of(s[k]) != 32 {
        k = k + 1;
    }
    return k;
}

fn number[&s](s: &s [byte], at: int, e: int) -> [] int {
    var n = 0;
    var k = at;
    var sign = 1;
    if k < e && int_of(s[k]) == 45 {
        sign = -1;
        k = k + 1;
    }
    while k < e {
        n = n * 10 + int_of(s[k]) - 48;
        k = k + 1;
    }
    return sign * n;
}

// Hex `s[at..e]` into `out` from `to`; the bytes written.
fn unhex[&s, &o](s: &s [byte], at: int, e: int, out: &!o [byte], to: int) -> [] int {
    var k = 0;
    while at + 2 * k + 1 < e {
        out[to + k] = byte_of(nibble(int_of(s[at + 2 * k])) * 16 + nibble(int_of(s[at + 2 * k + 1])));
        k = k + 1;
    }
    return k;
}

fn tag_line[&i](io: &!i Io, code: int) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    io.write_all(io, x509_verify.refusal_tag(code));
    io.newline(io);
    return 0;
}

// One `V` line, `s[at..e]` after the `V `.
fn verify_line[&h, &i, &s, &t](heap: &!h Heap, io: &!i Io, s: &s [byte], at: int, e: int, store: &t [byte]) -> [heap, io_write] int {
    var p = at;
    var f = field_end(s, p, e);
    let now = number(s, p, f);
    p = f + 1;
    f = field_end(s, p, e);
    let max = number(s, p, f);
    p = f + 1;
    f = field_end(s, p, e);
    let host_at = p;
    let host_end = f;
    p = f + 1;
    // Count the certificates.
    var n = 0;
    var q = p;
    while q < e {
        n = n + 1;
        q = field_end(s, q, e) + 1;
    }
    let certs = box_slice(heap, (e - p) / 2 + 1, byte_of(0));
    let ranges = box_slice(heap, 2 * n, 0);
    let host = box_slice(heap, (host_end - host_at) / 2 + 1, byte_of(0));
    var hn = 0;
    var cb = certs;
    var rb = ranges;
    var hb = host;
    borrow mut hb as &!hw in {
        if !(host_end - host_at == 1 && int_of(s[host_at]) == 45) {
            hn = unhex(s, host_at, host_end, contents(hw), 0);
        }
    }
    borrow mut cb as &!cw in {
        borrow mut rb as &!rw in {
            let c = contents(cw);
            let r = contents(rw);
            var used = 0;
            var k = 0;
            q = p;
            while k < n {
                let fe = field_end(s, q, e);
                r[2 * k] = used;
                used = used + unhex(s, q, fe, c, used);
                r[2 * k + 1] = used;
                q = fe + 1;
                k = k + 1;
            }
        }
    }
    var code = 0;
    borrow cb as &cr in {
        borrow rb as &rr in {
            borrow hb as &hr in {
                code = x509_verify.verify(store, contents(cr), contents(rr), contents(hr)[0..hn], now, max);
            }
        }
    }
    tag_line(io, code);
    unbox_slice(heap, cb);
    unbox_slice(heap, rb);
    unbox_slice(heap, hb);
    return 0;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    var store = box_slice(heap, 4194304, byte_of(0));
    var store_used = 0;
    borrow text as &b in {
        let s = buffer.bytes(b);
        var at = 0;
        while at < len(s) {
            var e = at;
            while e < len(s) && int_of(s[e]) != 10 {
                e = e + 1;
            }
            if e - at >= 2 && int_of(s[at]) == 83 {
                let pem = box_slice(heap, (e - at) / 2 + 1, byte_of(0));
                var pb = pem;
                var pn = 0;
                borrow mut pb as &!pw in {
                    pn = unhex(s, at + 2, e, contents(pw), 0);
                }
                region r {
                    let info = alloc_slice[r](2, 0);
                    var roots = 0;
                    borrow pb as &pr in {
                        borrow mut store as &!sw in {
                            roots = x509_verify.store_load(contents(pr)[0..pn], contents(sw), info);
                        }
                    }
                    if roots < 0 {
                        store_used = 0;
                        tag_line(io, roots);
                    } else {
                        store_used = info[0];
                        io.print_int(io, roots);
                        io.space(io);
                        io.print_int(io, info[1]);
                        io.space(io);
                        io.print_int(io, info[0]);
                        io.newline(io);
                    }
                }
                unbox_slice(heap, pb);
            } else if e - at >= 2 && int_of(s[at]) == 86 {
                borrow store as &sr in {
                    verify_line(heap, io, s, at + 2, e, contents(sr)[0..store_used]);
                }
            }
            at = e + 1;
        }
    }
    unbox_slice(heap, store);
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
