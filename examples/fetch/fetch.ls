// `fetch` -- an HTTP client, and the first program here that connects.
//
//     fetch <address> <port> <path>
//
// Sends `GET <path>` to an IPv4 address and writes the response body to
// standard output. A 2xx status exits 0; any other status still writes
// the body and exits 1, with the status line on standard error. It is
// `curl -s` with one method, one protocol version and no names.
//
// `docs/net.md` §5 counted the programs that ask for each half of the
// network and found inbound 1, outbound 0 -- and said the next step was
// not to build `Net` but to write this, because a program that connects
// is the only way to find out what `connect` needs that the design had
// not thought of. `docs/connect.md` is what it found. Three things, each
// visible below where it happens:
//
// - **No names.** The first argument is `127.0.0.1`, never `localhost`.
//   `getaddrinfo` answers a pointer, and a foreign result is a scalar
//   (`reach.md` §3.1), so this program cannot resolve a host at all.
// - **The destination is data.** The address comes from `argv`, so no
//   row written at compile time can name it.
// - **The address is portable by accident.** `struct sockaddr_in` is
//   not the same bytes on Linux and on macOS. The Linux bytes work on
//   both only because macOS reads family 0 as `AF_INET` for
//   compatibility (`docs/connect.md` §3).
//
// Like `examples/serve/`, it is `extern fn` declarations against libc
// through `Ffi("libc")`, and its authority report says so and no more.
//
// `socket`/`read`/`write`/`close`/`put` used to be declared here, same
// as `examples/serve/`'s own copies were before #142 -- and `connect`
// too, byte-for-byte the same as `examples/report/`'s,
// `examples/vsock/`'s and `examples/agent_guest/`'s. This is the first
// program to need more than one real package at once: `net.sockets`
// (`packages/net-sockets/`) for the first four, `net.connect`
// (`packages/net-connect/`) for the fifth, because no inbound program
// here needs `connect` and no outbound one needs `bind`/`listen`/
// `accept` (`docs/net.md` §1). Two locks, two fetches, one build --
// `examples/README.md`'s own section on this file has the commands.
//
// `octets_of`/`port_of`/`connect_to` and `send_all`/`status_of` used to
// live here too, byte-for-byte the same as `examples/report/report.ls`'s
// and `examples/agent_guest/agent_guest.ls`'s own copies. The first
// three moved into `net.connect` itself (generic to any outbound
// program); `send_all`/`status_of` are `packages/http-response/
// response.ls`, the fifth real package and the HTTP-specific,
// client-side mirror of `http.request` (`docs/package-system.md` §6).

import std.bytes;
import std.io;
import net.sockets;
import net.connect;
import http.response;

// ---------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------

// Send the request, then read until the server closes: the header block
// into `head`, and every byte after the blank line straight to standard
// output. HTTP/1.0 with `Connection: close`, so the end of the body is the
// end of the stream and there is no length to trust.
//
// Answers the status, or -1 for a response that never finished its
// header block or did not start with a status line.
fn exchange[&f, &i, &h, &p](libc: &f Ffi("libc"), io: &!i Io, fd: int, host: &h [byte],
    path: &p [byte]) -> [ffi("libc"), io_write] int {
    region scratch {
        let request = alloc_slice[scratch](len(path) + len(host) + 64, byte_of(0));
        var at = sockets.put(request, 0, "GET ");
        at = sockets.put(request, at, path);
        at = sockets.put(request, at, " HTTP/1.0\r\nHost: ");
        at = sockets.put(request, at, host);
        at = sockets.put(request, at, "\r\nConnection: close\r\n\r\n");
        if !response.send_all(libc, fd, request[0..at]) {
            return 0 - 1;
        }

        let head = alloc_slice[scratch](4096, byte_of(0));
        let chunk = alloc_slice[scratch](4096, byte_of(0));
        var held = 0;
        var status = 0 - 1;
        var body = false;
        var going = true;
        while going {
            let got = sockets.read(libc, fd, chunk);
            if got <= 0 {
                going = false;
            } else if body {
                io.write_all(io, chunk[0..got]);
            } else {
                // Still in the header block. It may end anywhere in this
                // chunk, including across the boundary with the last one,
                // so the search runs over everything held so far -- and
                // only header bytes are held: the same read usually
                // carries the start of the body too, and those go
                // straight out from `chunk`. (The first version copied
                // the whole read into `head` and gave up when it did not
                // fit, which failed on any response whose first body
                // bytes arrived with its blank line.)
                let before = held;
                var take = got;
                if take > len(head) - held {
                    take = len(head) - held;
                }
                sockets.put(head, held, chunk[0..take]);
                held = held + take;
                let end = bytes.find(head[0..held], "\r\n\r\n");
                if end >= 0 {
                    status = response.status_of(head[0..held]);
                    body = true;
                    // Where the body starts, as an offset into this read.
                    // The terminator was not in `head[0..before]` or the
                    // last read would have found it, so this is past 0.
                    let from = end + 4 - before;
                    if from < got {
                        io.write_all(io, chunk[from..got]);
                    }
                } else if held == len(head) {
                    return 0 - 1;
                }
            }
        }
        if !body {
            return 0 - 1;
        }
        return status;
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    // A client: no files and no heap, and nothing downstream can get
    // either back.
    release(fs);
    release(heap);

    let libc = narrow(ffi, "libc");
    var status = 2;
    borrow mut io as &!i in {
        borrow libc as &f in {
            borrow args as &g in {
                if arg_count(g) != 4 {
                    io.error_all(i, "usage: fetch <address> <port> <path>\n");
                } else {
                    region scratch {
                        let octets = alloc_slice[scratch](4, byte_of(0));
                        let port = connect.port_of(arg(g, 2));
                        if !connect.octets_of(arg(g, 1), octets) {
                            io.error_all(i, "fetch: the address must be four decimal octets; there is no name resolution\n");
                        } else if port < 0 {
                            io.error_all(i, "fetch: the port must be 1..65535\n");
                        } else {
                            let fd = connect.connect_to(f, octets, port);
                            if fd < 0 {
                                io.error_all(i, "fetch: could not connect\n");
                                status = 3;
                            } else {
                                let code = exchange(f, i, fd, arg(g, 1), arg(g, 3));
                                sockets.close(f, fd);
                                if code < 0 {
                                    io.error_all(i, "fetch: the response was not HTTP\n");
                                    status = 4;
                                } else if code >= 200 && code < 300 {
                                    status = 0;
                                } else {
                                    io.error_all(i, "fetch: the server answered with a non-2xx status\n");
                                    status = 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    release(libc);
    release(args);
    release(io);
    return status;
}
