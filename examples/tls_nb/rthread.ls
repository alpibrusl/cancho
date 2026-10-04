edition 5;

module rthread;

import dns;
import net.sockets;
import net.connect;

// `rthread` -- the worker half of a resolver that runs on other threads (`docs/tls-nonblocking.md` section 7).
//
// What the language allows decides the shape, so here is what was tried and what is left:
//
//   * `getaddrinfo` cannot be declared: its answer is a linked list of structs behind a `struct addrinfo **`, and a foreign
//     function's result can be an integer or an opaque handle that nothing can read through. `res_query` can: it takes a name,
//     two integers and a buffer, and fills the buffer with the DNS answer, which `dns.parse` reads. It asks the name servers in
//     `/etc/resolv.conf` over UDP, with libc's own timeout, retry and server rotation, and (unlike `getaddrinfo`) does not read
//     `/etc/hosts`, does not apply a search list and does not go through `nsswitch.conf`.
//   * `res_query`'s name is a C string that is not the last parameter, and a byte slice crosses as a pointer **and** a length, so it
//     cannot be declared with the name as a slice. `basename` takes one string, last, and answers a pointer into it, which is
//     declared here as an `int` and handed to `res_query` as the name. (The `int` is the whole reason this works, and it is the type
//     hole `c_ptr` exists to close: gaps 1, 5 and 6 of section 9 there. `strdup` would also do, but a declaration of `free` is refused by the LLVM
//     backend as an internal error: gap 8.)
//   * A thread's payload is one pointer-width value. The thread needs the authority to call libc **and** a channel to the thread
//     that wants answers; the main thread keeps using the same `Ffi` for OpenSSL, so it cannot give it away. The payload is therefore
//     a shared reference to the `Ffi`, which crosses (checked: `gaps/t3_shared_ffi_payload.ls`), and the channel is made by the
//     worker itself: it dials the main thread's loopback listener on a port both know (`bell_port`) with libc's own `socket` and
//     `connect`, and talks with `read` and `write`. The main thread sees an ordinary `Conn` and watches it on its `Poller`: that
//     is how it learns, without waiting in `join`, that an answer is ready.
//
// The channel's messages. A request is a length byte (1 to 253), the name, and a zero byte. A reply is
//
//     [code + 1000 as two bytes] [count] [smallest TTL as four bytes] [count addresses, four bytes each]
//
// where `code` is `dns.parse`'s answer (the count, or one of its negative codes) or -300 when `res_query` itself failed.

// GNU `basename`: answers a pointer into its argument (the whole of it when it has no `/`), so for a name without one it is the
// address of the caller's own buffer as an integer. Nothing is allocated, so nothing is freed.
extern fn basename[&f, &s](ffi: &f Ffi("libc"), text: &s [byte]) -> [ffi("libc")] int;

// `int res_query(const char *dname, int class, int type, unsigned char *answer, int anslen)`: the buffer's slice supplies the last
// two arguments.
extern fn res_query[&f, &a](ffi: &f Ffi("libc"), dname: int, class: int, kind: int, answer: &!a [byte]) -> [ffi("libc")] c_int;

// The port of the main thread's loopback listener. A constant because the payload cannot carry one and nothing reports the port of an ephemeral listener (gap 9).
pub fn bell_port() -> [] int {
    return 39517;
}

pub fn lookup_failed() -> [] int {
    return 0 - 300;
}

// Read exactly `n` bytes into `buf[at..at + n]`. 0 on success, -1 if the peer closed or the read failed.
fn read_exactly[&f, &b](ffi: &f Ffi("libc"), fd: int, buf: &!b [byte], at: int, n: int) -> [ffi("libc")] int {
    var got = 0;
    while got < n {
        let k = sockets.read(ffi, fd, buf[at + got..at + n]);
        if k <= 0 {
            return 0 - 1;
        }
        got = got + k;
    }
    return 0;
}

fn write_all[&f, &b](ffi: &f Ffi("libc"), fd: int, buf: &b [byte]) -> [ffi("libc")] int {
    var sent = 0;
    while sent < len(buf) {
        let k = sockets.write(ffi, fd, buf[sent..len(buf)]);
        if k <= 0 {
            return 0 - 1;
        }
        sent = sent + k;
    }
    return 0;
}

// The worker: connect to the main thread's listener, then answer requests until it closes the connection.
pub fn worker[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    var fd = 0 - 1;
    region dial {
        let octets = alloc_slice[dial](4, byte_of(0));
        octets[0] = byte_of(127);
        octets[3] = byte_of(1);
        fd = connect.connect_to(ffi, octets, bell_port());
    }
    if fd < 0 {
        return 1;
    }
    var status = 0;
    region scratch {
        if true {
            let req = alloc_slice[scratch](260, byte_of(0));
            let ans = alloc_slice[scratch](1500, byte_of(0));
            let addrs = alloc_slice[scratch](dns.addrs_size(), 0);
            let reply = alloc_slice[scratch](48, byte_of(0));
            var alive = true;
            while alive {
                if read_exactly(ffi, fd, req, 0, 1) != 0 {
                    alive = false;
                } else {
                    let l = int_of(req[0]);
                    // The name and its terminating zero.
                    if l < 1 || l > 253 || read_exactly(ffi, fd, req, 1, l + 1) != 0 {
                        alive = false;
                    } else {
                        let name = basename(ffi, req[1..l + 2]);
                        var code = lookup_failed();
                        var count = 0;
                        if name != 0 {
                            let n = res_query(ffi, name, 1, 1, ans);
                            if n > 0 {
                                // libc chose the query id; the answer carries it.
                                code = dns.parse(ans[0..n], n, int_of(ans[0]) * 256 + int_of(ans[1]), addrs);
                                if code > 0 {
                                    count = code;
                                }
                            }
                        }
                        reply[0] = byte_of((code + 1000) / 256);
                        reply[1] = byte_of((code + 1000) % 256);
                        reply[2] = byte_of(count);
                        let ttl = addrs[dns.max_addrs()];
                        reply[3] = byte_of(ttl / 16777216 % 256);
                        reply[4] = byte_of(ttl / 65536 % 256);
                        reply[5] = byte_of(ttl / 256 % 256);
                        reply[6] = byte_of(ttl % 256);
                        var k = 0;
                        while k < count {
                            reply[7 + 4 * k] = byte_of(addrs[k] / 16777216 % 256);
                            reply[8 + 4 * k] = byte_of(addrs[k] / 65536 % 256);
                            reply[9 + 4 * k] = byte_of(addrs[k] / 256 % 256);
                            reply[10 + 4 * k] = byte_of(addrs[k] % 256);
                            k = k + 1;
                        }
                        if write_all(ffi, fd, reply[0..7 + 4 * count]) != 0 {
                            alive = false;
                        }
                    }
                }
            }
        }
    }
    sockets.close(ffi, fd);
    return status;
}

// How many bytes of reply the `n` bytes in `buf` (from `at`) make: -1 if it is not yet known, else the whole length.
pub fn reply_length[&b](buf: &b [byte], at: int, n: int) -> [] int {
    if n < 3 {
        return 0 - 1;
    }
    return 7 + 4 * int_of(buf[at + 2]);
}

// Read a complete reply held in `buf[at..]` into `out` (the demo's per-lookup record: code, count, ttl, addresses).
pub fn read_reply[&b, &o](buf: &b [byte], at: int, out: &!o [int], o: int) -> [] int {
    out[o] = int_of(buf[at]) * 256 + int_of(buf[at + 1]) - 1000;
    let count = int_of(buf[at + 2]);
    out[o + 1] = count;
    out[o + 2] = int_of(buf[at + 3]) * 16777216 + int_of(buf[at + 4]) * 65536 + int_of(buf[at + 5]) * 256 + int_of(buf[at + 6]);
    var k = 0;
    while k < count && k < 8 {
        out[o + 3 + k] = int_of(buf[at + 7 + 4 * k]) * 16777216 + int_of(buf[at + 8 + 4 * k]) * 65536 + int_of(buf[at + 9 + 4 * k]) * 256 + int_of(buf[at + 10 + 4 * k]);
        k = k + 1;
    }
    return 0;
}
