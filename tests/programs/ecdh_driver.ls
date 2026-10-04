// `docs/ecdh.md` §4: `std.ecdh` driven from standard input, so
// Wycheproof, NIST CAVP and the OpenSSL differential all go through one
// program.
//
// One case per line, fields separated by one space, byte strings in
// lowercase hex and `-` for an empty one:
//
//     K <curve> <scalar>          ecdh.public_key
//     S <curve> <scalar> <peer>   ecdh.shared
//
// `curve` is 256 or 384, in decimal. One line out per case:
// `<code> <tag> <output in hex>`.
import std.buffer;
import std.ecdh;
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

fn report[&i, &d](io: &!i Io, code: int, d: &d [byte]) -> [io_write] int {
    io.print_int(io, code);
    io.space(io);
    io.write_all(io, ecdh.refusal_tag(code));
    io.space(io);
    if code == 0 {
        print_hex(io, d);
    }
    io.newline(io);
    return 0;
}

// One case, the line `s[at..]`; answers where the next line starts.
fn one[&i, &s, &w](io: &!i Io, s: &s [byte], at: int, work: &!w [int]) -> [io_write] int {
    let op = int_of(s[at]);
    let f1 = at + 2;
    let e1 = field_end(s, f1);
    let f2 = e1 + 1;
    let e2 = field_end(s, f2);
    var next = e2;
    let curve = decimal(s, f1, e1);
    var size = curve / 8;
    if size < 1 {
        size = 1;
    }
    region r {
        let scalar = alloc_slice[r](hex_len(s, f2, e2), byte_of(0));
        hex_into(s, f2, scalar);
        if op == 75 {
            let out = alloc_slice[r](1 + 2 * size, byte_of(0));
            report(io, ecdh.public_key(curve, scalar, out, work), out);
        } else {
            let f3 = e2 + 1;
            let e3 = field_end(s, f3);
            next = e3;
            let peer = alloc_slice[r](hex_len(s, f3, e3), byte_of(0));
            hex_into(s, f3, peer);
            let out = alloc_slice[r](size, byte_of(0));
            report(io, ecdh.shared(curve, scalar, peer, out, work), out);
        }
    }
    return next + 1;
}

fn cases[&i, &s, &w](io: &!i Io, s: &s [byte], work: &!w [int]) -> [io_write] int {
    var at = 0;
    while at < len(s) {
        at = one(io, s, at, work);
    }
    return 0;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    // On the heap: the work is larger than a region's 64 KiB.
    let work = box_slice(heap, ecdh.work_len(), 0);
    borrow mut work as &!x in {
        borrow text as &b in {
            cases(io, buffer.bytes(b), contents(x));
        }
    }
    unbox_slice(heap, work);
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
