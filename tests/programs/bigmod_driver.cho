// `docs/ecdh.md` §2: `std.bigmod.pow_mod` driven from standard input, to
// check its constant-time reduction against Python's `pow`, at moduli
// whose length is a multiple of 30 bits, where a sum or a Montgomery
// product reaches the carry limb.
//
// One case per line: `<n> <e> <a>`, big-endian lowercase hex (`-` for
// an empty one), and one line out: `<code> <a^e mod n in hex>`.
import std.bigmod;
import std.buffer;
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

// One case, the line `s[at..]`; answers where the next line starts.
fn one[&i, &s, &w](io: &!i Io, s: &s [byte], at: int, work: &!w [int]) -> [io_write] int {
    let e1 = field_end(s, at);
    let f2 = e1 + 1;
    let e2 = field_end(s, f2);
    let f3 = e2 + 1;
    let e3 = field_end(s, f3);
    region r {
        let n = alloc_slice[r](hex_len(s, at, e1), byte_of(0));
        hex_into(s, at, n);
        let e = alloc_slice[r](hex_len(s, f2, e2), byte_of(0));
        hex_into(s, f2, e);
        let a = alloc_slice[r](hex_len(s, f3, e3), byte_of(0));
        hex_into(s, f3, a);
        let out = alloc_slice[r](len(n), byte_of(0));
        io.print_int(io, bigmod.pow_mod(n, e, a, out, work));
        io.space(io);
        print_hex(io, out);
        io.newline(io);
    }
    return e3 + 1;
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io) -> [heap, io_read, io_write] int {
    var text = buffer.empty(heap, 4096);
    text = read_stdin(heap, io, text);
    let work = box_slice(heap, bigmod.work_len(), 0);
    borrow mut work as &!x in {
        borrow text as &b in {
            let s = buffer.bytes(b);
            var at = 0;
            while at < len(s) {
                at = one(io, s, at, contents(x));
            }
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
