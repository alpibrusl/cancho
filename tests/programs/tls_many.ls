edition 5;

// `docs/tls-core.md` §10: `conc` TLS 1.3 connections from `packages/tls`
// on ONE thread, driven by the `Poller`, to one server.
//
//     tls_many <ip> <port> <host> <conc> <chunk> [<seed> | resume]  < roots.pem
//
// Standard input is the PEM bundle of roots the engine trusts
// (`docs/x509-verify.md`). Each connection does a handshake, sends `GET / HTTP/1.0`, reads
// until the server's close_notify and closes. Each socket read is at most
// `chunk` bytes, so `chunk` 1 feeds the engine one byte at a time
// (fragmentation) and 65536 hands it everything a read gives
// (coalescing). One line per connection:
//
//     <slot> <code> <tag> <bytes received> <SHA-256 of them>
//
// then `done ok=<n> failed=<n>`. A connection still unfinished after 30
// seconds fails as `timeout`. Certificates are checked against
// `clock_unix_ms`. The request is padded to 2^14 bytes, a full record,
// so every slot's output buffer is used deep.
//
// With `resume`, a second round follows the first: each connection offers a
// ticket the first round saved (`tls.save`, `tls.start_with`;
// `docs/tls-resumption.md`), and each line ends with `resumed` or `full`.
//
// The engine's entropy is 32 bytes of /dev/urandom, or `seed`, 64 hex
// digits, for tests only: with a fixed seed every key is predictable, and
// that is what lets `conformance/tls.rs` replay recorded servers to it.
import std.buffer;
import std.conns;
import std.crypto;
import std.io;
import tls;

fn number[&s](s: &s [byte]) -> [] int {
    var n = 0;
    var i = 0;
    while i < len(s) {
        n = n * 10 + int_of(s[i]) - 48;
        i = i + 1;
    }
    return n;
}

fn digit(c: int) -> [] int {
    if c >= 97 {
        return c - 87;
    }
    return c - 48;
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

// Per slot, in `st`: [phase, request sent, bytes received, result, pending start, pending end], then the SHA-256 state.
fn stride() -> [] int {
    return 8 + crypto.sha256_state_len();
}

fn pend_cap() -> [] int {
    return 65536;
}

fn connecting() -> [] int {
    return 1;
}

fn running() -> [] int {
    return 2;
}

fn finished() -> [] int {
    return 3;
}

// The result a slot ended with: 0, a `tls` refusal, or these.
fn r_timeout() -> [] int {
    return 0 - 1000;
}

fn r_socket() -> [] int {
    return 0 - 1001;
}

fn tag_of(code: int) -> [] &static [byte] {
    if code == r_timeout() {
        return "timeout";
    }
    if code == r_socket() {
        return "socket";
    }
    return tls.refusal_tag(code);
}

fn end_slot[&t, &s](tab: &!t conns.Table, st: &!s [int], slot: int, result: int) -> [] int {
    let b = slot * stride();
    st[b] = finished();
    st[b + 3] = result;
    conns.close(tab, slot);
    return 1;
}

// Moves what the engine has for the socket into the slot's pending
// buffer, and writes as much of it as the socket takes.
fn flush[&e, &t, &s, &p](engine: &!e tls.Engine, tab: &!t conns.Table, st: &!s [int], pend: &!p [byte], slot: int) -> [conn_write] int {
    let b = slot * stride();
    let base = slot * pend_cap();
    if st[b + 4] == st[b + 5] {
        st[b + 4] = 0;
        st[b + 5] = tls.take(engine, slot, pend[base..base + pend_cap()]);
    }
    while st[b + 4] < st[b + 5] {
        match conns.write(tab, slot, pend[base + st[b + 4]..base + st[b + 5]]) {
            Sent::Wrote(k) => {
                st[b + 4] = st[b + 4] + k;
                if st[b + 4] == st[b + 5] {
                    st[b + 4] = 0;
                    st[b + 5] = tls.take(engine, slot, pend[base..base + pend_cap()]);
                }
            }
            Sent::Again => {
                return 0;
            }
            Sent::Failed(e) => {
                return 0 - 1;
            }
        }
    }
    return 0;
}

// Everything the engine has received, hashed and counted.
fn drain[&e, &s, &a](engine: &!e tls.Engine, st: &!s [int], app: &!a [byte], slot: int) -> [] int {
    let b = slot * stride();
    var n = tls.recv(engine, slot, app);
    while n > 0 {
        st[b + 2] = st[b + 2] + n;
        crypto.sha256_update(st[b + 8..b + stride()], app[0..n]);
        n = tls.recv(engine, slot, app);
    }
    return n;
}

// One slot after the poller reported it: answers 1 when it finished.
fn advance[&e, &t, &p, &s, &q, &a, &i, &r, &h](engine: &!e tls.Engine, tab: &!t conns.Table, poller: &!p Poller, st: &!s [int], pend: &!q [byte], app: &!a [byte], input: &!i [byte], req: &r [byte], host: &h [byte], slot: int, chunk: int, now: int, handle: int) -> [conn_read, conn_write, poll] int {
    let b = slot * stride();
    if st[b] == connecting() {
        if conns.connect_status(tab, slot) != 0 {
            return end_slot(tab, st, slot, r_socket());
        }
        let code = tls.start_with(engine, slot, host, now, handle);
        if code != 0 {
            return end_slot(tab, st, slot, code);
        }
        crypto.sha256_init(st[b + 8..b + stride()]);
        st[b] = running();
    }
    // One socket read a wakeup: the poller is level-triggered, so a slot
    // with more waiting is reported again, after the others had their
    // turn. With `chunk` 1, the 64 connections advance a byte at a time,
    // interleaved.
    var did_read = false;
    var going = true;
    while going {
        going = false;
        if flush(engine, tab, st, pend, slot) != 0 {
            return end_slot(tab, st, slot, r_socket());
        }
        let ev = tls.event(engine, slot);
        if ev == tls.event_established() && st[b + 1] == 0 {
            tls.send(engine, slot, req);
            st[b + 1] = 1;
            going = true;
        } else if ev == tls.event_failed() && st[b + 4] == st[b + 5] {
            return end_slot(tab, st, slot, tls.failure(engine, slot));
        } else if ev != tls.event_want_write() && st[b + 4] == st[b + 5] && !did_read {
            did_read = true;
            match conns.read(tab, slot, input[0..chunk]) {
                Received::Data(k) => {
                    var used = 0;
                    while used < k {
                        let c = tls.feed(engine, slot, input[used..k]);
                        if c < 0 {
                            used = k;
                        } else {
                            used = used + c;
                        }
                        let left = drain(engine, st, app, slot);
                        if left == 0 {
                            // close_notify: answer it, and the connection is done.
                            tls.finish(engine, slot);
                            flush(engine, tab, st, pend, slot);
                            return end_slot(tab, st, slot, 0);
                        }
                        flush(engine, tab, st, pend, slot);
                    }
                    going = true;
                }
                Received::End => {
                    let code = tls.eof(engine, slot);
                    drain(engine, st, app, slot);
                    if code == 0 {
                        return end_slot(tab, st, slot, 0);
                    }
                    return end_slot(tab, st, slot, code);
                }
                Received::Again => {
                    going = false;
                }
                Received::Failed(e) => {
                    return end_slot(tab, st, slot, r_socket());
                }
            }
        }
    }
    // Watch for writable only while bytes wait for the socket.
    var events = 1;
    if st[b + 4] < st[b + 5] {
        events = 3;
    }
    conns.rewatch(tab, poller, slot, slot, events);
    return 0;
}

fn report[&i, &e, &s](io: &!i Io, engine: &e tls.Engine, st: &!s [int], conc: int, show_resumed: bool) -> [io_write] int {
    var ok = 0;
    var failed = 0;
    region r {
        let digest = alloc_slice[r](32, byte_of(0));
        var slot = 0;
        while slot < conc {
            let b = slot * stride();
            let code = st[b + 3];
            io.print_int(io, slot);
            io.space(io);
            io.print_int(io, code);
            io.space(io);
            io.write_all(io, tag_of(code));
            io.space(io);
            io.print_int(io, st[b + 2]);
            io.space(io);
            crypto.sha256_final(st[b + 8..b + stride()], digest);
            print_hex(io, digest);
            if show_resumed {
                if tls.resumed(engine, slot) {
                    io.write_all(io, " resumed");
                } else {
                    io.write_all(io, " full");
                }
            }
            io.newline(io);
            if code == 0 {
                ok = ok + 1;
            } else {
                failed = failed + 1;
            }
            slot = slot + 1;
        }
    }
    io.write_all(io, "done ok=");
    io.print_int(io, ok);
    io.write_all(io, " failed=");
    io.print_int(io, failed);
    io.newline(io);
    return failed;
}

fn read_stdin[&h, &i](heap: &!h Heap, io: &!i Io, text: buffer.Buffer) -> [heap, io_read] buffer.Buffer {
    var out = text;
    var c = getchar(io);
    while c >= 0 {
        out = buffer.push(heap, out, byte_of(c));
        c = getchar(io);
    }
    return out;
}

fn drive[&h, &n, &k, &i, &q, &g, &e, &u](heap: &!h Heap, net: &n Net(""), clock: &k Clock, io: &!i Io, ip: &q [byte], host: &g [byte], engine: &!e tls.Engine, entropy: &u [byte], port: int, conc: int, chunk: int, rounds: int) -> [heap, net_out(""), conn_read, conn_write, poll, clock, io_write] int {
    tls.seed(engine, entropy);
    match poller_new() {
        Polling::Failed(e) => {
            return 4;
        }
        Polling::Ok(pl) => {
            var poll = pl;
            var table = conns.empty(heap, conc);
            let stb = box_slice(heap, stride() * conc, 0);
            let pendb = box_slice(heap, pend_cap() * conc, byte_of(0));
            let inb = box_slice(heap, 65536, byte_of(0));
            let appb = box_slice(heap, 16640, byte_of(0));
            var failed = 0;
            borrow mut stb as &!sw in {
                borrow mut pendb as &!pw in {
                    borrow mut inb as &!iw in {
                        borrow mut appb as &!aw in {
                            let st = contents(sw);
                            region a {
                                // The request is exactly 2^14 bytes, one full record: its
                                // headers padded with `X-Pad`.
                                let req = alloc_slice[a](16384, byte_of(112));
                                let head = "GET / HTTP/1.0\r\nHost: ";
                                var at = 0;
                                while at < len(head) {
                                    req[at] = head[at];
                                    at = at + 1;
                                }
                                var j = 0;
                                while j < len(host) {
                                    req[at] = host[j];
                                    at = at + 1;
                                    j = j + 1;
                                }
                                let pad = "\r\nX-Pad: ";
                                j = 0;
                                while j < len(pad) {
                                    req[at] = pad[j];
                                    at = at + 1;
                                    j = j + 1;
                                }
                                at = 16384;
                                req[at - 4] = byte_of(13);
                                req[at - 3] = byte_of(10);
                                req[at - 2] = byte_of(13);
                                req[at - 1] = byte_of(10);
                                let handles = alloc_slice[a](conc, 0);
                                let events = alloc_slice[a](2 * conc + 2, 0);
                                var round = 0;
                                while round < rounds {
                                if round > 0 {
                                    // Each connection's ticket is kept, its slot freed, and the
                                    // next round starts from nothing but the tickets.
                                    var k = 0;
                                    while k < conc {
                                        handles[k] = tls.save(engine, k);
                                        tls.drop(engine, k);
                                        k = k + 1;
                                    }
                                    k = 0;
                                    while k < stride() * conc {
                                        st[k] = 0;
                                        k = k + 1;
                                    }
                                    io.write_all(io, "round 2\n");
                                }
                                // Dial every connection; each is watched for writable under its slot.
                                var s = 0;
                                while s < conc {
                                    match tcp_connect_start(net, ip, port) {
                                        Dialed::Failed(err) => {
                                            st[s * stride()] = finished();
                                            st[s * stride() + 3] = r_socket();
                                        }
                                        Dialed::Ok(c) => {
                                            let (grown, slot) = conns.put(heap, table, c);
                                            table = grown;
                                            borrow mut table as &!tw in {
                                                borrow mut poll as &!pr in {
                                                    conns.watch(tw, pr, slot, slot, 2);
                                                }
                                            }
                                            st[slot * stride()] = connecting();
                                        }
                                    }
                                    s = s + 1;
                                }
                                let deadline = clock_ms(clock) + 30000;
                                var live = 0;
                                s = 0;
                                while s < conc {
                                    if st[s * stride()] != finished() {
                                        live = live + 1;
                                    }
                                    s = s + 1;
                                }
                                while live > 0 && clock_ms(clock) < deadline {
                                    var ready = 0;
                                    borrow mut poll as &!pr in {
                                        ready = poller_wait(pr, events, 200);
                                    }
                                    let now = clock_unix_ms(clock);
                                    var r = 0;
                                    while r < ready {
                                        let slot = events[2 * r];
                                        if slot >= 0 && slot < conc && st[slot * stride()] != finished() {
                                            var done = 0;
                                            borrow mut table as &!tw in {
                                                borrow mut poll as &!pr in {
                                                    done = advance(engine, tw, pr, st, contents(pw), contents(aw), contents(iw), req[0..at], host, slot, chunk, now, handles[slot]);
                                                }
                                            }
                                            live = live - done;
                                        }
                                        r = r + 1;
                                    }
                                }
                                s = 0;
                                while s < conc {
                                    if st[s * stride()] != finished() {
                                        borrow mut table as &!tw in {
                                            end_slot(tw, st, s, r_timeout());
                                        }
                                    }
                                    s = s + 1;
                                }
                                failed = failed + report(io, engine, st, conc, rounds > 1);
                                round = round + 1;
                                }
                            }
                        }
                    }
                }
            }
            unbox_slice(heap, stb);
            unbox_slice(heap, pendb);
            unbox_slice(heap, inb);
            unbox_slice(heap, appb);
            conns.drop(heap, table);
            poller_close(poll);
            if failed > 0 {
                return 1;
            }
            return 0;
        }
    }
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(ffi);
    let urandom = narrow(fs, "/dev/urandom");
    var status = 2;
    borrow mut io as &!i in {
        borrow args as &g in {
            if arg_count(g) < 6 {
                io.error_all(i, "usage: tls_many <ip> <port> <host> <conc> <chunk> < roots.pem\n");
            } else {
                let port = number(arg(g, 2));
                let conc = number(arg(g, 4));
                let chunk = number(arg(g, 5));
                if conc < 1 || conc > 256 || chunk < 1 || chunk > 65536 || len(arg(g, 3)) > 255 {
                    io.error_all(i, "tls_many: bad numbers, or a host over 255 bytes\n");
                } else {
                    borrow mut heap as &!h in {
                        var pem = buffer.empty(h, 65536);
                        pem = read_stdin(h, i, pem);
                        var engine = tls.open(h, conc);
                        region r {
                            let entropy = alloc_slice[r](32, byte_of(0));
                            var rounds = 1;
                            if arg_count(g) >= 7 && len(arg(g, 6)) == 6 {
                                rounds = 2;
                            }
                            if arg_count(g) >= 7 && len(arg(g, 6)) == 64 {
                                let seed = arg(g, 6);
                                var k = 0;
                                while k < 32 {
                                    entropy[k] = byte_of(digit(int_of(seed[2 * k])) * 16 + digit(int_of(seed[2 * k + 1])) & 255);
                                    k = k + 1;
                                }
                            } else {
                                borrow urandom as &u in {
                                    fs_read(u, "/dev/urandom", entropy);
                                }
                            }
                            borrow mut engine as &!ew in {
                                var roots = 0;
                                borrow pem as &pb in {
                                    roots = tls.trust(ew, buffer.bytes(pb));
                                }
                                if roots < 1 {
                                    io.error_all(i, "tls_many: no root certificate on standard input\n");
                                } else {
                                    borrow net as &nn in {
                                        borrow clock as &cc in {
                                            status = drive(h, nn, cc, i, arg(g, 1), arg(g, 3), ew, entropy, port, conc, chunk, rounds);
                                        }
                                    }
                                }
                            }
                        }
                        tls.close(h, engine);
                        buffer.drop(h, pem);
                    }
                }
            }
        }
    }
    release(urandom);
    release(net);
    release(clock);
    release(args);
    release(io);
    release(heap);
    return status;
}
