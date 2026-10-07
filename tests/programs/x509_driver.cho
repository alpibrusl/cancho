// `docs/x509.md` §5: `packages/x509` driven from standard input.
//
// If the input starts with `-----BEGIN`, it is a PEM bundle, and every
// certificate in it gives one line:
//
//     <code> <tag> v=<version> serial=<hex> issuer=<hex> subject=<hex>
//     nb=<seconds> na=<seconds> key=<alg>/<curve>/<key bytes> ca=<-1|0|1>
//     pathlen=<n> ku=<n> eku=<n> san=<hex> sig=<alg> lenient=<n> ext=<n>
//
// (one line; the fields are what `scripts/x509_check.py` compares with
// pyca/cryptography). Otherwise every input line is one certificate's
// DER in hex, answered with the same line.
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

fn print_hex[&i, &d](io: &!i Io, d: &d [byte], s: int, e: int) -> [io_write] int {
    let digits = "0123456789abcdef";
    var n = s;
    while n < e {
        let b = int_of(d[n]);
        io.write_all(io, digits[b >> 4..(b >> 4) + 1]);
        io.write_all(io, digits[b & 15..(b & 15) + 1]);
        n = n + 1;
    }
    return 0;
}

fn field[&i](io: &!i Io, name: &static [byte], v: int) -> [io_write] int {
    io.space(io);
    io.write_all(io, name);
    io.print_int(io, v);
    return 0;
}

fn describe[&i, &d, &v](io: &!i Io, code: int, der: &d [byte], view: &v [int]) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    io.write_all(io, x509.refusal_tag(code));
    if code == 0 {
        field(io, "v=", view[x509.version()]);
        io.write_all(io, " serial=");
        print_hex(io, der, view[x509.serial_start()], view[x509.serial_end()]);
        io.write_all(io, " issuer=");
        print_hex(io, der, view[x509.issuer_start()], view[x509.issuer_end()]);
        io.write_all(io, " subject=");
        print_hex(io, der, view[x509.subject_start()], view[x509.subject_end()]);
        field(io, "nb=", view[x509.not_before()]);
        field(io, "na=", view[x509.not_after()]);
        field(io, "key=", view[x509.key_algorithm()]);
        io.write_all(io, "/");
        io.print_int(io, view[x509.key_curve()]);
        io.write_all(io, "/");
        io.print_int(io, view[x509.key_end()] - view[x509.key_start()]);
        field(io, "ca=", view[x509.is_ca()]);
        field(io, "pathlen=", view[x509.path_len()]);
        field(io, "ku=", view[x509.key_usage()]);
        field(io, "eku=", view[x509.ext_key_usage()]);
        io.write_all(io, " san=");
        print_hex(io, der, view[x509.san_start()], view[x509.san_end()]);
        field(io, "sig=", view[x509.signature_algorithm()]);
        field(io, "lenient=", view[x509.leniency()]);
        field(io, "ext=", view[x509.extension_count()]);
    }
    io.newline(io);
    return 0;
}

fn nibble(c: int) -> [] int {
    if c >= 97 {
        return c - 87;
    }
    return c - 48;
}

fn bundle[&i, &s](io: &!i Io, s: &s [byte]) -> [io_write] int {
    var count = 0;
    region r {
        let der = alloc_slice[r](x509.max_certificate() + 1024, byte_of(0));
        let info = alloc_slice[r](2, 0);
        let view = alloc_slice[r](x509.view_len(), 0);
        var at = 0;
        var going = true;
        while going {
            let found = x509.pem_next(s, at, der, info);
            if found == 1 {
                going = false;
            } else if found < 0 {
                // The refusal's line; past its END line if it had one.
                describe(io, found, der, view);
                at = info[1];
                going = at < len(s);
                count = count + 1;
            } else {
                let n = info[0];
                describe(io, x509.parse(der[0..n], view), der[0..n], view);
                at = info[1];
                count = count + 1;
            }
        }
    }
    return count;
}

fn lines[&h, &i, &s](heap: &!h Heap, io: &!i Io, s: &s [byte]) -> [heap, io_write] int {
    var at = 0;
    while at < len(s) {
        var e = at;
        while e < len(s) && int_of(s[e]) != 10 {
            e = e + 1;
        }
        // On the heap: a line may name more than an arena holds.
        let der = box_slice(heap, (e - at) / 2, byte_of(0));
        borrow mut der as &!dw in {
            let d = contents(dw);
            var k = 0;
            while k < len(d) {
                d[k] = byte_of(nibble(int_of(s[at + 2 * k])) * 16 + nibble(int_of(s[at + 2 * k + 1])));
                k = k + 1;
            }
        }
        region r {
            let view = alloc_slice[r](x509.view_len(), 0);
            borrow der as &db in {
                let d = contents(db);
                describe(io, x509.parse(d, view), d, view);
            }
        }
        unbox_slice(heap, der);
        at = e + 1;
    }
    return 0;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    borrow text as &b in {
        let s = buffer.bytes(b);
        if len(s) > 10 && int_of(s[0]) == '-' {
            bundle(io, s);
        } else {
            lines(heap, io, s);
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
