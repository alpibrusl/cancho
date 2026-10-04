edition 5;

// `tls_nb` -- many TLS clients on ONE thread, driven by the `Poller`
// (`docs/tls-nonblocking.md`). Each connection is a state machine: connecting, handshake,
// sending a request, reading the response, and again for `reqs` requests, then `close_notify`.
//
//     tls_nb <ip> <port> <host> <total> <conc> <reqs> <verify> <cafile|-> <default-paths> <body> <resp-len> <hold-ms> <release-buffers> <verbose> [key=value ...]
//
// The socket is dialled at `<ip>` (`tcp_connect_start`, an IP literal: no resolver is involved);
// `<host>` is only what goes into the SNI extension and what the certificate has to name. That
// is the pinned-address design of sections 3.8 and 7 of the document, run for real. With `ns=<ip>` the first argument is
// a NAME instead: it is looked up over DNS on the same poller, every address in the answer is judged (`pin.ls`), and the
// connection is made to the address that was judged.
//
//   total   connections to make in all
//   conc    at most this many are in flight (the hooks service uses 64)
//   reqs    requests sent over each connection before it is closed (0: handshake only)
//   verify  1: SSL_VERIFY_PEER with the host name checked; 0: no verification at all
//   cafile  a PEM file of trusted roots, or `-`
//   default-paths 1: also trust the system store (`SSL_CTX_set_default_verify_paths`)
//   body    bytes of body in each POST (`Content-Length`)
//   resp-len exactly how many bytes the server answers with (the test servers are fixed-size)
//   hold-ms with `total <= conc`: once every handshake is done, print `HELD` and wait this long
//           (a script reads the process's memory then) before sending any request
//   release-buffers 1: `SSL_MODE_RELEASE_BUFFERS`
//   verbose 1: one line for each connection
//
// and, after those, `key=value` options:
//
//   io=1           OpenSSL on the descriptor (`SSL_set_fd`) instead of over memory BIOs (the default, 0)
//   deadline=<ms>  one connection's budget for lookup, connect, handshake and every request (default 15000); past it the
//                  connection ends with stage 102 (no answer in time)
//   resume=1       keep the session of the first connection that gets a response and resume from it on every later one
//   ns=<ip> ns-port=<n>   look the first argument up as a name at this name server (DNS over TCP)
//   allow-private=1       with `ns`: addresses in the ranges `pin.ls` refuses are allowed
//   sigpipe=ignore        make writes to a closed peer an error for the whole process (the `io=1` transport needs it)
//
// Outcomes are `stage detail status`: stage 0 is success; 1 connect (detail errno); 2 handshake (detail an X509_V_ERR_* number
// when verification failed, otherwise the first OpenSSL error code, or -1 when the peer closed); 3 write; 4 read; 5 setup;
// 6 name lookup (detail a `dns.ls`/`rtcp.ls` code: -103 NXDOMAIN, -200 cannot reach the name server, -203 timeout);
// 7 destination refused (detail the packed address); 102 deadline.

import std.io;
import nat;
import std.bytes;
import std.conns;
import tls;
import dns;
import rtcp;
import pin;

fn d_stride() -> [] int {
    return 16;
}

// Phases in dd[slot * 16]: 0 free, 1 connecting, 2 handshake, 3 sending, 4 reading, 6 held.
// dd: [phase, deadline, sent, got, reqs_done, started_ms, hs_ms, conn_index, status]
// cfg: [total, conc, reqs, verify, resp_len, hold, deadline_ms, req_len, verbose, port]
// rs (per connection): [stage, detail, status, hs_ms]
// st: [started, finished, held, ok, reused, bytes_in]

fn put_decimal[&b](out: &!b [byte], at: int, n: int) -> [] int {
    var digits = 1;
    var m = n;
    while m >= 10 {
        m = m / 10;
        digits = digits + 1;
    }
    var i = digits - 1;
    var v = n;
    while i >= 0 {
        out[at + i] = byte_of('0' + v % 10);
        v = v / 10;
        i = i - 1;
    }
    return at + digits;
}

fn put_text[&b, &t](out: &!b [byte], at: int, text: &t [byte]) -> [] int {
    // The same loop `net.sockets.put` and three other files have; the one here ends on the index it reached, so that it is the loop of a
    // program that builds with only `std` (this directory takes no package for `Ffi("libc")`: `docs/tls-nonblocking.md` section 6).
    var i = 0;
    while i < len(text) {
        out[at + i] = text[i];
        i = i + 1;
    }
    return at + i;
}

// The request: a POST of `body` bytes of `x`. Answers its length.
fn build_request[&b, &h](out: &!b [byte], host: &h [byte], body: int) -> [] int {
    var at = put_text(out, 0, "POST /hook HTTP/1.1\r\nHost: ");
    at = put_text(out, at, host);
    at = put_text(out, at, "\r\nContent-Type: application/json\r\nContent-Length: ");
    at = put_decimal(out, at, body);
    at = put_text(out, at, "\r\nConnection: keep-alive\r\n\r\n");
    var i = 0;
    while i < body {
        out[at + i] = byte_of('x');
        i = i + 1;
    }
    return at + body;
}

// HTTP status from `HTTP/1.x NNN`, or -1.
fn status_of[&h](head: &h [byte], n: int) -> [] int {
    if n < 12 || int_of(head[0]) != 'H' || int_of(head[1]) != 'T' || int_of(head[2]) != 'T' || int_of(head[3]) != 'P' || int_of(head[4]) != '/' || int_of(head[8]) != ' ' {
        return 0 - 1;
    }
    var code = 0;
    var i = 9;
    while i < 12 {
        let c = int_of(head[i]);
        if c < '0' || c > '9' {
            return 0 - 1;
        }
        code = code * 10 + (c - '0');
        i = i + 1;
    }
    return code;
}

// End the connection in `slot`: record how it ended, free everything of it.
fn finish[&f, &t, &a, &d, &o, &r, &s](ffi: &f Ffi("tls"), tab: &!t conns.Table, tt: &!a [int], dd: &!d [int], out: &!o [byte], rs: &!r [int], st: &!s [int], slot: int, stage: int, detail: int, now: int) -> [ffi("tls"), conn_write] int {
    let b = slot * d_stride();
    let idx = dd[b + 7];
    if stage == 0 {
        tls.shutdown(ffi, tab, tt, out, slot);
        st[3] = st[3] + 1;
    }
    rs[idx * 5] = stage;
    rs[idx * 5 + 1] = detail;
    rs[idx * 5 + 2] = dd[b + 8];
    rs[idx * 5 + 3] = dd[b + 6];
    rs[idx * 5 + 4] = dd[b + 9];
    tls.drop(ffi, tt, slot);
    conns.close(tab, slot);
    dd[b] = 0;
    st[1] = st[1] + 1;
    return 0;
}

// Move the connection in `slot` along. Answers 0 while it waits, 1 when it has ended (finished and freed).
fn advance[&f, &t, &p, &a, &d, &o, &n, &q, &c, &rq, &r, &s, &h, &i](ffi: &f Ffi("tls"), tab: &!t conns.Table, poller: &!p Poller, tt: &!a [int], dd: &!d [int], out: &!o [byte], net: &!n [byte], plain: &!q [byte], cfg: &c [int], req: &rq [byte], rs: &!r [int], st: &!s [int], host: &h [byte], info: &!i [byte], slot: int, now: int) -> [ffi("tls"), conn_read, conn_write, poll] int {
    let b = slot * d_stride();
    let verify = cfg[3] == 1;
    var progress = true;
    while progress {
        progress = false;
        if dd[b] == 1 {
            let status = conns.connect_status(tab, slot);
            if status != 0 {
                finish(ffi, tab, tt, dd, out, rs, st, slot, tls.stage_connect(), status, now);
                return 1;
            }
            var fd = 0 - 1;
            if cfg[11] == 1 {
                fd = tls.fd_of(tab, slot);
            }
            if tls.open(ffi, dd[b + 10], tt, slot, host, verify, fd, st[6]) != 0 {
                finish(ffi, tab, tt, dd, out, rs, st, slot, tls.stage_setup(), tls.detail_of(tt, slot), now);
                return 1;
            }
            dd[b] = 2;
            progress = true;
        } else if dd[b] == 2 {
            let hs = tls.handshake(ffi, tab, poller, tt, out, net, slot, slot);
            if hs == tls.pending() {
                return 0;
            }
            if hs == tls.failed() {
                finish(ffi, tab, tt, dd, out, rs, st, slot, tls.stage_of(tt, slot), tls.detail_of(tt, slot), now);
                return 1;
            }
            dd[b + 6] = now - dd[b + 5];
            if tls.reused(ffi, tt, slot) {
                st[4] = st[4] + 1;
            }
            if int_of(info[0]) == 0 {
                let version = tls.describe(ffi, tt, slot, info[1..len(info)]);
                info[0] = byte_of(1);
                dd[b + 12] = version;
            }
            if cfg[5] > 0 {
                dd[b] = 6;
                st[2] = st[2] + 1;
                return 0;
            }
            if cfg[2] == 0 {
                // reqs = 0: a handshake and a close_notify, nothing sent.
                finish(ffi, tab, tt, dd, out, rs, st, slot, 0, 0, now);
                return 1;
            }
            dd[b] = 3;
            dd[b + 2] = 0;
            progress = true;
        } else if dd[b] == 3 {
            if cfg[2] == 0 {
                finish(ffi, tab, tt, dd, out, rs, st, slot, 0, 0, now);
                return 1;
            }
            let k = tls.write(ffi, tab, poller, tt, out, slot, slot, req[dd[b + 2]..cfg[7]]);
            if k == 0 - 1 {
                return 0;
            }
            if k < 0 {
                finish(ffi, tab, tt, dd, out, rs, st, slot, tls.stage_of(tt, slot), tls.detail_of(tt, slot), now);
                return 1;
            }
            dd[b + 2] = dd[b + 2] + k;
            if dd[b + 2] >= cfg[7] {
                dd[b] = 4;
                dd[b + 3] = 0;
            }
            progress = true;
        } else if dd[b] == 4 {
            let k = tls.read(ffi, tab, poller, tt, out, net, slot, slot, plain[0..len(plain)]);
            if k == 0 - 1 {
                return 0;
            }
            if k == 0 - 2 {
                finish(ffi, tab, tt, dd, out, rs, st, slot, tls.stage_of(tt, slot), tls.detail_of(tt, slot), now);
                return 1;
            }
            if k <= 0 {
                // close_notify (0) or a bare EOF (-3) before the whole response.
                finish(ffi, tab, tt, dd, out, rs, st, slot, tls.stage_read(), 0 - 1, now);
                return 1;
            }
            if dd[b + 3] == 0 {
                dd[b + 8] = status_of(plain, k);
            }
            dd[b + 3] = dd[b + 3] + k;
            st[5] = st[5] + k;
            if dd[b + 3] >= cfg[4] {
                dd[b + 4] = dd[b + 4] + 1;
                if cfg[12] == 1 && st[6] == 0 {
                    // The first complete response: the session ticket is in by now. Later connections resume from it.
                    st[6] = tls.save_session(ffi, tt, slot);
                }
                if dd[b + 4] >= cfg[2] || dd[b + 8] < 100 {
                    finish(ffi, tab, tt, dd, out, rs, st, slot, 0, 0, now);
                    return 1;
                }
                dd[b] = 3;
                dd[b + 2] = 0;
            }
            progress = true;
        } else if dd[b] == 6 {
            // Held: read what the server sends unasked (TLS 1.3 session tickets) so a level-triggered poller does not spin.
            let k = tls.read(ffi, tab, poller, tt, out, net, slot, slot, plain[0..len(plain)]);
            if k == 0 - 2 || k == 0 - 3 || k == 0 {
                finish(ffi, tab, tt, dd, out, rs, st, slot, tls.stage_read(), 0 - 1, now);
                return 1;
            }
        }
    }
    return 0;
}

// Start connection number `idx`: dial `ip:port` (an IP literal), watch for writable, and set the slot up. `pinned` is the packed
// address when it came from a lookup (reported with the outcome), else 0.
fn begin[&h, &n, &q, &t, &p, &d, &c](heap: &!h Heap, net: &n Net(""), tab: conns.Table, poller: &!p Poller, ip: &q [byte], dd: &!d [int], cfg: &c [int], idx: int, pinned: int, ctx: int, now: int) -> [heap, net_out(""), poll] (conns.Table, int) {
    match tcp_connect_start(net, ip, cfg[9]) {
        Dialed::Failed(err) => {
            return (tab, 0 - 1 - err);
        }
        Dialed::Ok(c) => {
            let (grown, slot) = conns.put(heap, tab, c);
            var table = grown;
            if slot < 0 || slot >= cfg[1] {
                if slot >= 0 {
                    borrow mut table as &!ct in {
                        conns.close(ct, slot);
                    }
                }
                return (table, 0 - 1);
            }
            var watched = 0 - 1;
            borrow mut table as &!ct in {
                watched = conns.watch(ct, poller, slot, slot, 2);
                if watched != 0 {
                    conns.close(ct, slot);
                }
            }
            if watched != 0 {
                return (table, 0 - 1);
            }
            let b = slot * d_stride();
            dd[b] = 1;
            dd[b + 1] = now + cfg[6];
            dd[b + 2] = 0;
            dd[b + 3] = 0;
            dd[b + 4] = 0;
            dd[b + 5] = now;
            dd[b + 6] = 0;
            dd[b + 7] = idx;
            dd[b + 8] = 0 - 1;
            dd[b + 9] = pinned;
            dd[b + 10] = ctx;
            return (table, slot);
        }
    }
}

// Record that connection `idx` ended before it had a slot: where, why, and the address it was pinned to (0 if none).
fn note_failure[&r](rs: &!r [int], idx: int, stage: int, detail: int, pinned: int) -> [] int {
    rs[idx * 5] = stage;
    rs[idx * 5 + 1] = detail;
    rs[idx * 5 + 2] = 0 - 1;
    rs[idx * 5 + 3] = 0;
    rs[idx * 5 + 4] = pinned;
    return 0;
}

fn print_kv[&i, &t](io: &!i Io, key: &t [byte], n: int) -> [io_write] int {
    io.write_all(io, key);
    io.write_all(io, "=");
    io.print_int(io, n);
    io.write_all(io, " ");
    return 0;
}

// The main loop. `READY` (after setup) and `HELD` (see `hold-ms`) go to standard error, which is unbuffered; standard output is
// fully buffered when it is not a terminal and arrives at exit. Answers 0 when every connection ended (whether it worked or not).
fn drive[&f, &h, &n, &k, &i, &q, &g, &c, &x](ffi: &f Ffi("tls"), heap: &!h Heap, net: &n Net(""), clock: &k Clock, io: &!i Io, ip: &q [byte], host: &g [byte], ns: &x [byte], cfg: &c [int], ctx: int) -> [ffi("tls"), heap, net_out(""), conn_read, conn_write, poll, clock, io_write, err_write] int {
    let total = cfg[0];
    let conc = cfg[1];
    match poller_new() {
        Polling::Failed(e) => {
            return 4;
        }
        Polling::Ok(pl) => {
            var poll = pl;
            var table = conns.empty(heap, conc);
            var rz = rtcp.open(heap);
            let ttb = box_slice(heap, tls.stride() * conc, 0);
            let ddb = box_slice(heap, d_stride() * conc, 0);
            let rsb = box_slice(heap, 5 * total, 0);
            let outb = box_slice(heap, tls.out_max() * conc, byte_of(0));
            let netb = box_slice(heap, tls.net_max(), byte_of(0));
            let plb = box_slice(heap, 8192, byte_of(0));
            let rqb = box_slice(heap, cfg[7] + 1, byte_of(0));
            borrow mut ttb as &!tw in {
                borrow mut ddb as &!dw in {
                    borrow mut rsb as &!rw in {
                        borrow mut outb as &!ow in {
                            borrow mut netb as &!nw in {
                                borrow mut plb as &!pw in {
                                    borrow mut rqb as &!qw in {
                                        let tt = contents(tw);
                                        let dd = contents(dw);
                                        let rs = contents(rw);
                                        let out = contents(ow);
                                        let nb = contents(nw);
                                        let plain = contents(pw);
                                        let req = contents(qw);
                                        region a {
                                            let events = alloc_slice[a](128, 0);
                                            let st = alloc_slice[a](8, 0);
                                            let dtext = alloc_slice[a](16, byte_of(0));
                                            let info = alloc_slice[a](256, byte_of(0));
                                            let rl = build_request(req, host, cfg_body(cfg));
                                            if cfg[11] == 1 && cfg[16] == 1 {
                                                // The direct transport writes sockets itself: a closed peer must not kill the process.
                                                tls.ignore_sigpipe(ffi);
                                            }
                                            io.error_all(io, "READY\n");
                                            let t0 = clock_ms(clock);
                                            var held_done = false;
                                            var hold_until = 0;
                                            var spawn_fail = 0;
                                            while st[1] < total && spawn_fail < 100 {
                                                var now = clock_ms(clock);
                                                // Start what there is room for (at most 16 a turn, as the service does).
                                                var started_now = 0;
                                                while st[0] < total && started_now < 16 && live_slots(dd, conc) + st[7] < conc && (cfg[13] != 1 || st[7] < rtcp.slots()) {
                                                    if cfg[13] == 1 {
                                                        // Resolve first: the lookup's id is the connection's index; the dial waits for the answer.
                                                        var rslot = 0 - 1;
                                                        var rcode = 0;
                                                        borrow mut poll as &!pr in {
                                                            let (grown, sl, cd) = rtcp.start(heap, rz, net, pr, ns, cfg[14], ip, 5000, st[0], 20000 + st[0] % 40000, now + cfg[6]);
                                                            rz = grown;
                                                            rslot = sl;
                                                            rcode = cd;
                                                        }
                                                        if rslot < 0 {
                                                            note_failure(rs, st[0], 6, rcode, 0);
                                                            st[1] = st[1] + 1;
                                                        } else {
                                                            st[7] = st[7] + 1;
                                                        }
                                                        st[0] = st[0] + 1;
                                                    } else {
                                                        var slot = 0 - 1;
                                                        borrow mut poll as &!pr in {
                                                            let (grown, s) = begin(heap, net, table, pr, ip, dd, cfg, st[0], 0, ctx, now);
                                                            table = grown;
                                                            slot = s;
                                                        }
                                                        if slot < 0 {
                                                            // The dial failed at once: that connection ends here, with the errno.
                                                            note_failure(rs, st[0], tls.stage_connect(), 0 - 1 - slot, 0);
                                                            st[1] = st[1] + 1;
                                                        }
                                                        st[0] = st[0] + 1;
                                                    }
                                                    started_now = started_now + 1;
                                                }
                                                var timeout = 200;
                                                if st[2] > 0 && !held_done {
                                                    timeout = 50;
                                                }
                                                var ready = 0;
                                                borrow mut poll as &!pr in {
                                                    ready = poller_wait(pr, events, timeout);
                                                }
                                                now = clock_ms(clock);
                                                var j = 0;
                                                while j < ready {
                                                    let slot = events[2 * j];
                                                    if slot >= 5000 {
                                                        // A lookup moved: when it ends, pin the address and dial it.
                                                        var rcode = rtcp.pending();
                                                        var live = false;
                                                        borrow rz as &rr in {
                                                            live = rtcp.busy(rr, slot - 5000);
                                                        }
                                                        if live {
                                                            borrow mut rz as &!rw in {
                                                                borrow mut poll as &!pr in {
                                                                    rcode = rtcp.advance(rw, pr, slot - 5000, 5000);
                                                                }
                                                            }
                                                        }
                                                        if live && rcode != rtcp.pending() {
                                                            var idx = 0;
                                                            var pick = 0 - 1;
                                                            var addr = 0;
                                                            borrow rz as &rr in {
                                                                idx = rtcp.lookup_of(rr, slot - 5000);
                                                            }
                                                            // Copy the answer out of the resolver into one small array and judge it.
                                                            region ra {
                                                                let got = alloc_slice[ra](dns.addrs_size(), 0);
                                                                borrow rz as &rr in {
                                                                    var m = 0;
                                                                    while m < rcode && m < dns.max_addrs() {
                                                                        got[m] = rtcp.addr_of(rr, slot - 5000, m);
                                                                        m = m + 1;
                                                                    }
                                                                }
                                                                if rcode > 0 {
                                                                    pick = pin.choose(got, rcode, cfg[15] == 1);
                                                                    if pick >= 0 {
                                                                        addr = got[pick];
                                                                    } else {
                                                                        addr = got[pin.first_refused(got, rcode)];
                                                                    }
                                                                }
                                                            }
                                                            borrow mut rz as &!rw in {
                                                                rtcp.finish(rw, slot - 5000);
                                                            }
                                                            st[7] = st[7] - 1;
                                                            if rcode <= 0 {
                                                                note_failure(rs, idx, 6, rcode, 0);
                                                                st[1] = st[1] + 1;
                                                            } else if pick < 0 {
                                                                // Refused before any connection: the address is in a range the service does not deliver to.
                                                                note_failure(rs, idx, 7, addr, addr);
                                                                st[1] = st[1] + 1;
                                                            } else {
                                                                let dl = dns.put_dotted(dtext, 0, addr);
                                                                var dslot = 0 - 1;
                                                                borrow mut poll as &!pr in {
                                                                    let (grown, sl) = begin(heap, net, table, pr, dtext[0..dl], dd, cfg, idx, addr, ctx, now);
                                                                    table = grown;
                                                                    dslot = sl;
                                                                }
                                                                if dslot < 0 {
                                                                    note_failure(rs, idx, tls.stage_connect(), 0 - 1 - dslot, addr);
                                                                    st[1] = st[1] + 1;
                                                                }
                                                            }
                                                        }
                                                    } else if slot >= 0 && slot < conc && dd[slot * d_stride()] != 0 {
                                                        borrow mut table as &!tab in {
                                                            borrow mut poll as &!pr in {
                                                                advance(ffi, tab, pr, tt, dd, out, nb, plain, cfg, req, rs, st, host, info, slot, now);
                                                            }
                                                        }
                                                    }
                                                    j = j + 1;
                                                }
                                                // Deadlines.
                                                var s = 0;
                                                while s < conc {
                                                    if dd[s * d_stride()] != 0 && now >= dd[s * d_stride() + 1] {
                                                        borrow mut table as &!tab in {
                                                            finish(ffi, tab, tt, dd, out, rs, st, s, tls.stage_handshake() + 100, 0, now);
                                                        }
                                                    }
                                                    s = s + 1;
                                                }
                                                var rsl = 0;
                                                while cfg[13] == 1 && rsl < rtcp.slots() {
                                                    var late = false;
                                                    var rid = 0;
                                                    borrow rz as &rr in {
                                                        late = rtcp.expired(rr, rsl, now);
                                                        rid = rtcp.lookup_of(rr, rsl);
                                                    }
                                                    if late {
                                                        note_failure(rs, rid, 6, rtcp.timed_out(), 0);
                                                        borrow mut rz as &!rw in {
                                                            rtcp.finish(rw, rsl);
                                                        }
                                                        st[7] = st[7] - 1;
                                                        st[1] = st[1] + 1;
                                                    }
                                                    rsl = rsl + 1;
                                                }
                                                // Hold: every handshake done -> say so, wait, then go on.
                                                if cfg[5] > 0 && !held_done && st[2] + st[1] >= total {
                                                    io.error_all(io, "HELD\n");
                                                    held_done = true;
                                                    hold_until = clock_ms(clock) + cfg[5];
                                                }
                                                if held_done && hold_until > 0 && clock_ms(clock) >= hold_until {
                                                    hold_until = 0;
                                                    var s2 = 0;
                                                    while s2 < conc {
                                                        if dd[s2 * d_stride()] == 6 {
                                                            dd[s2 * d_stride()] = 3;
                                                            dd[s2 * d_stride() + 2] = 0;
                                                            borrow mut table as &!tab in {
                                                                borrow mut poll as &!pr in {
                                                                    advance(ffi, tab, pr, tt, dd, out, nb, plain, cfg, req, rs, st, host, info, s2, now);
                                                                }
                                                            }
                                                        }
                                                        s2 = s2 + 1;
                                                    }
                                                }
                                            }
                                            let t1 = clock_ms(clock);
                                            report(io, rs, st, cfg, info, t1 - t0, dd);
                                            tls.free_session(ffi, st[6]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            unbox_slice(heap, ttb);
            unbox_slice(heap, ddb);
            unbox_slice(heap, rsb);
            unbox_slice(heap, outb);
            unbox_slice(heap, netb);
            unbox_slice(heap, plb);
            unbox_slice(heap, rqb);
            conns.drop(heap, table);
            rtcp.close(heap, rz);
            poller_close(poll);
            return 0;
        }
    }
}

// The value of the option `key=value` among the arguments after the fourteen positional ones, or `default`.
fn option_nat[&g, &k](a: &g Args, key: &k [byte], default: int) -> [args] int {
    var i = 15;
    while i < arg_count(a) {
        let t = arg(a, i);
        if len(t) > len(key) + 1 && int_of(t[len(key)]) == '=' && bytes.starts_with(t, key) {
            return nat.parse(t[len(key) + 1..len(t)]);
        }
        i = i + 1;
    }
    return default;
}

// The text of the option `key=value`, or an empty text.
fn option_text[&g, &k](a: &g Args, key: &k [byte]) -> [args] &g [byte] {
    var i = 15;
    while i < arg_count(a) {
        let t = arg(a, i);
        if len(t) > len(key) + 1 && int_of(t[len(key)]) == '=' && bytes.starts_with(t, key) {
            return t[len(key) + 1..len(t)];
        }
        i = i + 1;
    }
    return arg(a, 0)[0..0];
}

fn cfg_body[&c](cfg: &c [int]) -> [] int {
    return cfg[10];
}

fn live_slots[&d](dd: &d [int], conc: int) -> [] int {
    var n = 0;
    var s = 0;
    while s < conc {
        if dd[s * d_stride()] != 0 {
            n = n + 1;
        }
        s = s + 1;
    }
    return n;
}

fn report[&i, &r, &s, &c, &f, &d](io: &!i Io, rs: &r [int], st: &s [int], cfg: &c [int], info: &f [byte], wall_ms: int, dd: &d [int]) -> [io_write] int {
    let total = cfg[0];
    // Distinct (stage, detail, status) outcomes with counts.
    region a {
        let keys = alloc_slice[a](3 * 64, 0);
        let dtext = alloc_slice[a](16, byte_of(0));
        let counts = alloc_slice[a](64, 0);
        var nk = 0;
        var hs_sum = 0;
        var hs_max = 0;
        var hs_n = 0;
        var c = 0;
        while c < total {
            let stage = rs[c * 5];
            let detail = rs[c * 5 + 1];
            let status = rs[c * 5 + 2];
            if stage == 0 {
                hs_sum = hs_sum + rs[c * 5 + 3];
                hs_n = hs_n + 1;
                if rs[c * 5 + 3] > hs_max {
                    hs_max = rs[c * 5 + 3];
                }
            }
            var k = 0;
            var found = 0 - 1;
            while k < nk {
                if keys[3 * k] == stage && keys[3 * k + 1] == detail && keys[3 * k + 2] == status {
                    found = k;
                }
                k = k + 1;
            }
            if found < 0 && nk < 64 {
                keys[3 * nk] = stage;
                keys[3 * nk + 1] = detail;
                keys[3 * nk + 2] = status;
                found = nk;
                nk = nk + 1;
            }
            if found >= 0 {
                counts[found] = counts[found] + 1;
            }
            if cfg[8] == 1 {
                io.write_all(io, "conn ");
                io.print_int(io, c);
                io.write_all(io, " stage=");
                io.print_int(io, stage);
                io.write_all(io, " detail=");
                io.print_int(io, detail);
                io.write_all(io, " status=");
                io.print_int(io, status);
                io.write_all(io, " hs_ms=");
                io.print_int(io, rs[c * 5 + 3]);
                if rs[c * 5 + 4] != 0 {
                    // The address the connection was pinned to (or, for stage 7, the one that was refused).
                    let dl = dns.put_dotted(dtext, 0, rs[c * 5 + 4]);
                    io.write_all(io, " pinned=");
                    io.write_all(io, dtext[0..dl]);
                }
                io.newline(io);
            }
            c = c + 1;
        }
        var k = 0;
        while k < nk {
            io.write_all(io, "outcome stage=");
            io.print_int(io, keys[3 * k]);
            io.write_all(io, " detail=");
            io.print_int(io, keys[3 * k + 1]);
            io.write_all(io, " status=");
            io.print_int(io, keys[3 * k + 2]);
            io.write_all(io, " count=");
            io.print_int(io, counts[k]);
            io.newline(io);
            k = k + 1;
        }
        io.write_all(io, "summary ");
        print_kv(io, "total", total);
        print_kv(io, "ok", st[3]);
        print_kv(io, "resumed", st[4]);
        print_kv(io, "bytes_in", st[5]);
        print_kv(io, "wall_ms", wall_ms);
        var avg = 0;
        if hs_n > 0 {
            avg = hs_sum / hs_n;
        }
        print_kv(io, "hs_ms_avg", avg);
        print_kv(io, "hs_ms_max", hs_max);
        io.newline(io);
        // The session description, first line of it.
        io.write_all(io, "session ");
        var e = 1;
        while e < len(info) && int_of(info[e]) != 0 && int_of(info[e]) != '\n' {
            e = e + 1;
        }
        io.write_all(io, info[1..e]);
        io.newline(io);
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(fs);
    let ssl = narrow(ffi, "tls");
    var status = 2;
    borrow mut io as &!i in {
        borrow ssl as &f in {
            borrow args as &g in {
                if arg_count(g) < 15 {
                    io.error_all(i, "usage: tls_nb <ip|name> <port> <host> <total> <conc> <reqs> <verify> <cafile|-> <default-paths> <body> <resp-len> <hold-ms> <release-buffers> <verbose> [key=value ...]\n  keys: io=0|1  deadline=<ms>  resume=0|1  ns=<ip>  ns-port=<n>  allow-private=0|1  sigpipe=ignore|default\n");
                } else {
                    let port = nat.parse(arg(g, 2));
                    let total = nat.parse(arg(g, 4));
                    let conc = nat.parse(arg(g, 5));
                    let reqs = nat.parse(arg(g, 6));
                    let verify = nat.parse(arg(g, 7));
                    let defpaths = nat.parse(arg(g, 9));
                    let body = nat.parse(arg(g, 10));
                    let resp = nat.parse(arg(g, 11));
                    let hold = nat.parse(arg(g, 12));
                    let relbuf = nat.parse(arg(g, 13));
                    let verbose = nat.parse(arg(g, 14));
                    let iomode = option_nat(g, "io", 0);
                    let deadline = option_nat(g, "deadline", 15000);
                    let resume = option_nat(g, "resume", 0);
                    // With `ns=<ip>` the first argument is a NAME, looked up first (DNS over TCP, on the poller).
                    let nsip = option_text(g, "ns");
                    let nsport = option_nat(g, "ns-port", 53);
                    let allow_private = option_nat(g, "allow-private", 0);
                    let sigpipe = option_text(g, "sigpipe");
                    var resolve = 0;
                    if len(nsip) > 0 {
                        resolve = 1;
                    }
                    var ignore_sigpipe = 0;
                    if len(sigpipe) == 6 && int_of(sigpipe[0]) == 'i' {
                        ignore_sigpipe = 1;
                    }
                    if port < 1 || port > 65535 || total < 1 || conc < 1 || conc > 4096 || reqs < 0 || verify < 0 || body < 0 || body > 60000 || resp < 1 || hold < 0 || relbuf < 0 || verbose < 0 || defpaths < 0 || iomode < 0 || iomode > 1 || deadline < 1 || resume < 0 || resume > 1 || nsport < 1 || nsport > 65535 || allow_private < 0 || allow_private > 1 {
                        io.error_all(i, "tls_nb: bad number in the arguments\n");
                    } else {
                        // The CA file as a C string.
                        let cafile = arg(g, 8);
                        var cpath = 0;
                        if len(cafile) == 1 && int_of(cafile[0]) == '-' {
                            cpath = 0;
                        } else {
                            cpath = len(cafile);
                        }
                        borrow mut heap as &!h in {
                            let cz = box_slice(h, cpath + 1, byte_of(0));
                            borrow mut cz as &!zw in {
                                let zs = contents(zw);
                                var j = 0;
                                while j < cpath {
                                    zs[j] = cafile[j];
                                    j = j + 1;
                                }
                                let ctx = tls.context(f, verify == 1, defpaths == 1, zs[0..cpath], relbuf == 1);
                                if ctx == 0 {
                                    io.error_all(i, "tls_nb: could not make the TLS context\n");
                                    status = 3;
                                } else {
                                    // cfg: [total, conc, reqs, verify, resp_len, hold, deadline_ms, req_len, verbose, port, body, io, resume, resolve, ns-port, allow-private, ignore-sigpipe]
                                    region a {
                                        let cfg = alloc_slice[a](20, 0);
                                        cfg[0] = total;
                                        cfg[1] = conc;
                                        cfg[2] = reqs;
                                        cfg[3] = verify;
                                        cfg[4] = resp;
                                        cfg[5] = hold;
                                        cfg[6] = deadline;
                                        cfg[7] = 200 + len(arg(g, 3)) + body;
                                        cfg[8] = verbose;
                                        cfg[9] = port;
                                        cfg[10] = body;
                                        cfg[11] = iomode;
                                        cfg[12] = resume;
                                        cfg[13] = resolve;
                                        cfg[14] = nsport;
                                        cfg[15] = allow_private;
                                        cfg[16] = ignore_sigpipe;
                                        borrow net as &nn in {
                                            borrow clock as &cc in {
                                                status = drive(f, h, nn, cc, i, arg(g, 1), arg(g, 3), nsip, cfg, ctx);
                                            }
                                        }
                                    }
                                    tls.SSL_CTX_free(f, ctx);
                                }
                            }
                            unbox_slice(h, cz);
                        }
                    }
                }
            }
        }
    }
    release(ssl);
    release(net);
    release(clock);
    release(args);
    release(io);
    release(heap);
    return status;
}
