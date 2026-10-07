// `docs/x509.md` §5.4: `packages/x509` on damaged certificates.
//
//     (echo 1000000; cat tests/vectors/x509/corpus.pem) | ./x509_fuzz
//
// The first input line is the number of rounds; the rest a PEM bundle.
// Each round takes one of the bundle's certificates and damages it one
// of six ways: random bytes replaced, bits flipped, bytes set to the
// values DER lengths and tags are made of, a cut at a random length, a
// byte inserted or removed, or the PEM text itself damaged and decoded
// again. Then it is parsed. The claim under test: every round answers a
// code with a tag (0 to -19), nothing traps (a trap ends the process
// with its own status), and the run ends. The answer is one line, the
// count of each code, then `x509 fuzz ok rounds=<n>`, or `FAILED`.
import std.buffer;
import std.io;
import x509;

fn read_stdin[&h, &i](heap: &!h Heap, io: &!i Io, text: buffer.Buffer) -> [heap, io_read] buffer.Buffer {
    var out = text;
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

fn next(x: int) -> [] int {
    return (x * 1103515245 + 12345) % 2147483648;
}

// A value DER is made of: a length's first byte, a tag, or an end.
fn interesting(x: int) -> [] int {
    let k = x % 12;
    if k == 0 {
        return 0x00;
    }
    if k == 1 {
        return 0x7f;
    }
    if k == 2 {
        return 0x80;
    }
    if k == 3 {
        return 0x81;
    }
    if k == 4 {
        return 0x82;
    }
    if k == 5 {
        return 0x84;
    }
    if k == 6 {
        return 0x85;
    }
    if k == 7 {
        return 0xff;
    }
    if k == 8 {
        return 0x1f;
    }
    if k == 9 {
        return 0x30;
    }
    if k == 10 {
        return 0xa3;
    }
    return 0x01;
}

// Base64 and the characters around it.
fn pem_char(x: int) -> [] int {
    let k = x % 70;
    if k < 64 {
        let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        return int_of(alphabet[k]);
    }
    if k == 64 {
        return 61;
    }
    if k == 65 {
        return 10;
    }
    if k == 66 {
        return 32;
    }
    if k == 67 {
        return 45;
    }
    if k == 68 {
        return 0;
    }
    return 0xc3;
}

// One round: the answer code of parsing (or decoding) a damaged copy of
// certificate `c`, and the generator's new state in `st[0]`.
fn round[&s, &d, &o, &w, &p, &v, &n, &t](s: &s [byte], der: &d [byte], offsets: &o [int], c: int, work: &!w [byte], text: &!p [byte], view: &!v [int], info: &!n [int], st: &!t [int]) -> [] int {
    let ds = offsets[3 * c];
    let n = offsets[3 * c + 1] - ds;
    var x = next(st[0]);
    let mode = x / 256 % 6;
    var used = n;
    var code = 0;
    if mode < 5 {
        var j = 0;
        while j < n {
            work[j] = der[ds + j];
            j = j + 1;
        }
        x = next(x);
        let changes = 1 + x / 256 % 4;
        var t = 0;
        while t < changes {
            x = next(x);
            let at = x / 256 % n;
            x = next(x);
            if mode == 0 {
                work[at] = byte_of(x / 256 % 256);
            } else if mode == 1 {
                work[at] = byte_of(int_of(work[at]) ^ 1 << x / 256 % 8);
            } else if mode == 2 {
                work[at] = byte_of(interesting(x / 256));
            } else if mode == 3 {
                used = x / 256 % (n + 1);
            } else if x / 256 % 2 == 0 && used < len(work) {
                // Insert a byte at `at`.
                var k = used;
                while k > at {
                    work[k] = work[k - 1];
                    k = k - 1;
                }
                work[at] = byte_of(x / 65536 % 256);
                used = used + 1;
            } else if used > 0 {
                // Remove the byte at `at`.
                var k = at;
                while k + 1 < used {
                    work[k] = work[k + 1];
                    k = k + 1;
                }
                used = used - 1;
            }
            t = t + 1;
        }
        // The slice is exactly as long as the input, so a read past its
        // end is a bounds trap, not a quiet read of the buffer's tail.
        code = x509.parse(work[0..used], view);
    } else {
        let ps = offsets[3 * c + 2];
        let pe = offsets[3 * c + 5];
        let m = pe - ps;
        var j = 0;
        while j < m {
            text[j] = s[ps + j];
            j = j + 1;
        }
        x = next(x);
        let changes = 1 + x / 256 % 3;
        var t = 0;
        while t < changes {
            x = next(x);
            let at = x / 256 % m;
            x = next(x);
            text[at] = byte_of(pem_char(x / 256));
            t = t + 1;
        }
        code = x509.pem_next(text[0..m], 0, work, info);
        if code == 0 {
            code = x509.parse(work[0..info[0]], view);
        } else if code != 1 && code != -16 && code != -18 {
            code = -100;
        }
    }
    st[0] = x;
    return code;
}

fn fuzz[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {
    var bad = 0;
    var rounds = 0;
    var at = 0;
    while at < len(s) && int_of(s[at]) != 10 {
        rounds = rounds * 10 + int_of(s[at]) - 48;
        at = at + 1;
    }
    region a {
        // Every certificate's DER, and for each: DER start, DER end, PEM
        // start; PEM ends are the next PEM start (or the last END line).
        let der = alloc_slice[a](32768, byte_of(0));
        let offsets = alloc_slice[a](3 * 65 + 3, 0);
        let info = alloc_slice[a](2, 0);
        var count = 0;
        var used = 0;
        var going = true;
        while going && count < 64 {
            let from = at;
            let found = x509.pem_next(s, at, der[used..len(der)], info);
            if found != 0 {
                going = false;
            } else {
                offsets[3 * count] = used;
                offsets[3 * count + 1] = used + info[0];
                offsets[3 * count + 2] = from;
                used = used + info[0];
                at = info[1];
                count = count + 1;
            }
        }
        offsets[3 * count + 2] = at;
        region b {
            let work = alloc_slice[b](x509.max_certificate() + 1024, byte_of(0));
            let text = alloc_slice[b](24576, byte_of(0));
            let view = alloc_slice[b](x509.view_len(), 0);
            let st = alloc_slice[b](1, 42);
            let counts = alloc_slice[b](21, 0);
            var k = 0;
            while k < rounds && count > 0 {
                let code = round(s, der, offsets, k % count, work, text, view, info, st);
                if code == 1 {
                    counts[20] = counts[20] + 1;
                } else if code <= 0 && code >= -19 {
                    counts[0 - code] = counts[0 - code] + 1;
                } else {
                    bad = bad + 1;
                }
                k = k + 1;
            }
            var c = 0;
            while c < 20 {
                if counts[c] > 0 {
                    io.write_all(io, x509.refusal_tag(0 - c));
                    io.write_all(io, "=");
                    io.print_nat(io, counts[c]);
                    io.space(io);
                }
                c = c + 1;
            }
            io.write_all(io, "no-block=");
            io.print_nat(io, counts[20]);
            io.newline(io);
        }
        if count == 0 {
            bad = bad + 1;
        }
    }
    if bad == 0 {
        io.write_all(io, "x509 fuzz ok rounds=");
        io.print_nat(io, rounds);
    } else {
        io.write_all(io, "x509 fuzz FAILED bad=");
        io.print_nat(io, bad);
    }
    io.newline(io);
    return bad;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    var bad = 0;
    borrow text as &b in {
        bad = fuzz(io, buffer.bytes(b));
    }
    buffer.drop(heap, text);
    return bad;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args);
    release(ffi);
    release(fs);
    var bad = 0;
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            bad = run(h, i);
        }
    }
    release(heap);
    release(io);
    if bad > 0 {
        return 1;
    }
    return 0;
}
