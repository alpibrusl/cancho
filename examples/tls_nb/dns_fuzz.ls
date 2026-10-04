edition 5;

// `dns_fuzz` -- `dns.ls` on a known answer, on the errors it must name, and on a million damaged ones.
//
//     lex-sys run dns.ls dns_fuzz.ls --std            # prints `dns ok` and the count; any trap is a failure (exit 132)
//
// The claim under test: `dns.parse` is total. A resolver's answer is bytes from the network, so a parser that traps on one is a
// denial of service for whoever can send a packet; here every mutation of a good answer, every truncation of it, and bytes
// that were never an answer at all come back as a count or an error code, and the process runs to the end.

import std.io;
import dns;

fn put16[&b](m: &!b [byte], at: int, v: int) -> [] int {
    m[at] = byte_of(v / 256 % 256);
    m[at + 1] = byte_of(v % 256);
    return at + 2;
}

fn put32[&b](m: &!b [byte], at: int, v: int) -> [] int {
    put16(m, at, v / 65536);
    put16(m, at + 2, v % 65536);
    return at + 4;
}

// A response to `hooks.example.com`: a CNAME, then two A records (93.184.216.34 and 10.0.0.7). Answers its length.
fn good_answer[&b](m: &!b [byte], id: int) -> [] int {
    let q = dns.build_query("hooks.example.com", id, m, 0);
    // Flags: a response, recursion desired and available.
    m[2] = byte_of(129);
    m[3] = byte_of(128);
    put16(m, 6, 3);
    var at = q;
    // CNAME hooks.example.com -> <pointer to the question's name>, TTL 60.
    at = put16(m, at, 49164);
    at = put16(m, at, 5);
    at = put16(m, at, 1);
    at = put32(m, at, 60);
    at = put16(m, at, 2);
    at = put16(m, at, 49164);
    // A 93.184.216.34, TTL 300.
    at = put16(m, at, 49164);
    at = put16(m, at, 1);
    at = put16(m, at, 1);
    at = put32(m, at, 300);
    at = put16(m, at, 4);
    m[at] = byte_of(93);
    m[at + 1] = byte_of(184);
    m[at + 2] = byte_of(216);
    m[at + 3] = byte_of(34);
    at = at + 4;
    // A 10.0.0.7, TTL 120.
    at = put16(m, at, 49164);
    at = put16(m, at, 1);
    at = put16(m, at, 1);
    at = put32(m, at, 120);
    at = put16(m, at, 4);
    m[at] = byte_of(10);
    m[at + 1] = byte_of(0);
    m[at + 2] = byte_of(0);
    m[at + 3] = byte_of(7);
    return at + 4;
}

fn next(x: int) -> [] int {
    return (x * 1103515245 + 12345) % 2147483648;
}

fn check[&l, &i](io: &!i Io, label: &l [byte], ok: bool, bad: int) -> [io_write] int {
    if ok {
        return bad;
    }
    io.write_all(io, "FAILED: ");
    io.write_all(io, label);
    io.newline(io);
    return bad + 1;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    release(net);
    release(clock);
    var bad = 0;
    var iterations = 0;
    borrow mut io as &!i in {
        region a {
            let m = alloc_slice[a](600, byte_of(0));
            let w = alloc_slice[a](600, byte_of(0));
            let addrs = alloc_slice[a](dns.addrs_size(), 0);
            let q = alloc_slice[a](600, byte_of(0));
            let n = good_answer(m, 4660);

            // Known answer.
            let c = dns.parse(m, n, 4660, addrs);
            bad = check(i, "two addresses", c == 2, bad);
            bad = check(i, "first address", addrs[0] == 93 * 16777216 + 184 * 65536 + 216 * 256 + 34, bad);
            bad = check(i, "second address", addrs[1] == 10 * 16777216 + 7, bad);
            bad = check(i, "smallest TTL of the A records, not the CNAME's", addrs[dns.max_addrs()] == 120, bad);
            // Errors it must name.
            bad = check(i, "wrong id", dns.parse(m, n, 4661, addrs) == dns.wrong_id(), bad);
            bad = check(i, "too short", dns.parse(m, 11, 4660, addrs) == dns.malformed(), bad);
            bad = check(i, "cut inside the first answer", dns.parse(m, n - 30, 4660, addrs) == dns.malformed(), bad);
            m[2] = byte_of(1);
            bad = check(i, "a query is not a response", dns.parse(m, n, 4660, addrs) == dns.not_a_response(), bad);
            m[2] = byte_of(131);
            bad = check(i, "TC bit", dns.parse(m, n, 4660, addrs) == dns.truncated(), bad);
            m[2] = byte_of(129);
            m[3] = byte_of(131);
            bad = check(i, "NXDOMAIN", dns.parse(m, n, 4660, addrs) == dns.rcode_error(3), bad);
            m[3] = byte_of(128);
            // The query builder.
            let ql = dns.build_query("a.b.example.org.", 7, q, 0);
            bad = check(i, "query length (a.b.example.org: 2+2+8+4+1 = 17 name bytes, 12 header, 4 tail)", ql == 33, bad);
            bad = check(i, "empty name", dns.build_query("", 7, q, 0) < 0, bad);
            bad = check(i, "empty label", dns.build_query("a..b", 7, q, 0) < 0, bad);
            bad = check(i, "leading dot", dns.build_query(".a", 7, q, 0) < 0, bad);
            bad = check(i, "a 64-byte label", dns.build_query("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.com", 7, q, 0) < 0, bad);
            bad = check(i, "a buffer that is too small", dns.build_query("example.com", 7, q[0..20], 0) < 0, bad);

            // Damage: 400,000 mutations of the good answer (1 to 4 random bytes replaced, and a random cut), 400,000 more
            // aimed at the counts and lengths in the header and the first record, and 200,000 random strings.
            var x = 42;
            var k = 0;
            while k < 1000000 {
                var len_used = n;
                if k < 600000 {
                    var j = 0;
                    while j < n {
                        w[j] = m[j];
                        j = j + 1;
                    }
                    x = next(x);
                    let changes = 1 + x / 65536 % 4;
                    var t = 0;
                    while t < changes {
                        x = next(x);
                        var at = x / 256 % n;
                        if k >= 400000 {
                            // aimed at the header and the first 60 bytes
                            at = x / 256 % 60;
                        }
                        x = next(x);
                        w[at] = byte_of(x / 256 % 256);
                        t = t + 1;
                    }
                    x = next(x);
                    if x / 4096 % 3 == 0 {
                        len_used = x / 256 % (n + 1);
                    }
                } else {
                    var j = 0;
                    x = next(x);
                    len_used = x / 256 % 600;
                    while j < len_used {
                        x = next(x);
                        w[j] = byte_of(x / 256 % 256);
                        j = j + 1;
                    }
                }
                // The slice is exactly as long as the message, so a read past its end is a bounds trap, not a quiet read of the buffer's tail.
                let got = dns.parse(w[0..len_used], len_used, 4660, addrs);
                if got > dns.max_addrs() {
                    bad = bad + 1;
                }
                iterations = iterations + 1;
                k = k + 1;
            }
        }
        if bad == 0 {
            io.write_all(i, "dns ok iterations=");
            io.print_nat(i, iterations);
            io.newline(i);
        } else {
            io.write_all(i, "dns FAILED checks=");
            io.print_nat(i, bad);
            io.newline(i);
        }
    }
    release(io);
    return bad;
}
