edition 5;
module fuzz_common;

// What the fuzzing harnesses share (docs/tls-assurance.md §3.3): reading
// one input, and the fixed values in `fuzz_fixture` turned into what the
// client takes.
import fuzz_fixture;
import std.io;
import tls_client;
import x509_verify;

// Standard input into `into`, at most its length; how many bytes were read.
// An input longer than `into` is cut there, which each harness's buffer is
// sized so a legal input never is.
pub fn read_all[&i, &o](io: &!i Io, into: &!o [byte]) -> [io_read] int {
    var n = 0;
    var c = getchar(io);
    while c >= 0 && n < len(into) {
        into[n] = byte_of(c);
        n = n + 1;
        c = getchar(io);
    }
    return n;
}

fn nibble(c: int) -> [] int {
    if c >= 97 {
        return c - 87;
    }
    return c - 48;
}

// The hex `h` into `out`; how many bytes that was.
pub fn from_hex[&h, &o](h: &h [byte], out: &!o [byte]) -> [] int {
    var i = 0;
    while i < len(out) && 2 * i + 1 < len(h) {
        out[i] = byte_of(nibble(int_of(h[2 * i])) * 16 + nibble(int_of(h[2 * i + 1])));
        i = i + 1;
    }
    return i;
}

// The client's 96 bytes of randomness.
pub fn load_random[&o](out: &!o [byte]) -> [] int {
    return from_hex(fuzz_fixture.random_hex(), out);
}

// The trust store, loaded into `store`; its length in bytes.
pub fn load_store[&s](store: &!s [byte]) -> [] int {
    var n = 0;
    region r {
        let info = alloc_slice[r](2, 0);
        if x509_verify.store_load(fuzz_fixture.roots(), store, info) >= 0 {
            n = info[0];
        }
    }
    return n;
}

// Everything `take` and `recv` have, taken as a caller would; how many
// bytes that was.
pub fn drain[&n, &b, &o](ints: &!n [int], bytes: &!b [byte], out: &!o [byte]) -> [] int {
    var got = 0;
    var n = tls_client.take(ints, bytes, out);
    while n > 0 {
        got = got + n;
        n = tls_client.take(ints, bytes, out);
    }
    n = tls_client.recv(ints, bytes, out);
    while n > 0 {
        got = got + n;
        n = tls_client.recv(ints, bytes, out);
    }
    return got;
}

// `data`, fed until it is all consumed or the client stops taking it.
pub fn feed_all[&n, &b, &d, &o, &p](ints: &!n [int], bytes: &!b [byte], data: &d [byte], out: &!o [byte], store: &p [byte]) -> [] int {
    var at = 0;
    var going = true;
    while going {
        let c = tls_client.feed(ints, bytes, data[at..len(data)], store);
        let drained = drain(ints, bytes, out);
        if c < 0 {
            return c;
        }
        at = at + c;
        if at == len(data) || c == 0 && drained == 0 {
            going = false;
        }
    }
    return at;
}
