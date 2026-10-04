edition 5;

// `resolve_demo` -- three ways to turn a name into addresses while a poller loop keeps running, and what each costs the loop.
//
//     resolve_demo <mode> <ns-ip> <ns-port> <count> <name>...
//
//   mode  blocking   `tcp_connect_start(name)`: the resolver inside the builtin, `getaddrinfo` (what `lexsys-hooks` has today)
//         tcp        `rtcp.ls`: DNS over TCP to <ns-ip>:<ns-port>, a state machine on the same poller
//         thread     `rthread.ls`: four worker threads, `res_query` in each, answers through loopback connections
//
// `<count>` is how many of the names after it to look up (each name is given once and looked up once; repeat a name to repeat it).
// The loop is a `Poller` wait of 1 ms and a tick counter. After 50 ms of ticking, every lookup is started at once. What is printed:
// each lookup's answer and how long it took, and the loop's longest gap between two iterations: **a loop that waits for a lookup
// shows it as a gap as long as the lookup.** `<ns-ip>`/`<ns-port>` are used by `tcp` only; `blocking` and `thread` read
// `/etc/resolv.conf`, as libc does.

import std.io;
import nat;
import std.conns;
import dns;
import rtcp;
import rthread;

// One lookup's record in the results array: [code, count, ttl, address 0..7].
fn rec() -> [] int {
    return 12;
}

// Print one lookup's record: code, addresses, TTL, how long it took.
fn show[&i, &t, &r](io: &!i Io, n: int, name: &t [byte], rsl: &r [int], elapsed: int) -> [io_write] int {
    let o = n * rec();
    io.write_all(io, "lookup ");
    io.print_nat(io, n);
    io.write_all(io, " ");
    io.write_all(io, name);
    io.write_all(io, " code=");
    io.print_int(io, rsl[o]);
    io.write_all(io, " addrs=");
    var k = 0;
    region a {
        let text = alloc_slice[a](16, byte_of(0));
        while k < rsl[o + 1] {
            if k > 0 {
                io.write_all(io, ",");
            }
            let l = dns.put_dotted(text, 0, rsl[o + 3 + k]);
            io.write_all(io, text[0..l]);
            k = k + 1;
        }
    }
    io.write_all(io, " ttl=");
    io.print_int(io, rsl[o + 2]);
    io.write_all(io, " ms=");
    io.print_int(io, elapsed);
    io.newline(io);
    return 0;
}

fn summary[&i](io: &!i Io, iters: int, maxgap: int, total: int) -> [io_write] int {
    io.write_all(io, "loop iterations=");
    io.print_nat(io, iters);
    io.write_all(io, " max_gap_ms=");
    io.print_nat(io, maxgap);
    io.write_all(io, " total_ms=");
    io.print_nat(io, total);
    io.newline(io);
    return 0;
}

// ---- blocking: what hooks does today -------------------------------------------------------------------

fn run_blocking[&n, &k, &i, &g](net: &n Net(""), clock: &k Clock, io: &!i Io, a: &g Args, first: int, count: int) -> [args, net_out(""), poll, clock, io_write] int {
    match poller_new() {
        Polling::Failed(e) => {
            return 4;
        }
        Polling::Ok(pl) => {
            var poll = pl;
            let t0 = clock_ms(clock);
            var last = t0;
            var maxgap = 0;
            var iters = 0;
            var started = 0;
            let start_at = t0 + 50;
            region r {
                let rsl = alloc_slice[r](32 * rec(), 0);
                let t_start = alloc_slice[r](32, 0);
                let t_done = alloc_slice[r](32, 0);
                let events = alloc_slice[r](16, 0);
                while started < count || iters < 100 {
                    let now = clock_ms(clock);
                    if now - last > maxgap {
                        maxgap = now - last;
                    }
                    last = now;
                    iters = iters + 1;
                    if now >= start_at && started < count {
                        // The whole loop stops here for as long as the name takes to resolve.
                        t_start[started] = clock_ms(clock);
                        match tcp_connect_start(net, arg(a, first + started), 9) {
                            Dialed::Ok(c) => {
                                conn_close(c);
                                rsl[started * rec()] = 0;
                            }
                            Dialed::Failed(e) => {
                                rsl[started * rec()] = e;
                            }
                        }
                        t_done[started] = clock_ms(clock);
                        started = started + 1;
                    }
                    borrow mut poll as &!pw in {
                        poller_wait(pw, events, 1);
                    }
                }
                var j = 0;
                while j < count {
                    show(io, j, arg(a, first + j), rsl, t_done[j] - t_start[j]);
                    j = j + 1;
                }
            }
            summary(io, iters, maxgap, clock_ms(clock) - t0);
            poller_close(poll);
            return 0;
        }
    }
}

// ---- tcp: the resolver on the poller ------------------------------------------------------------------

fn run_tcp[&h, &n, &k, &i, &g, &q](heap: &!h Heap, net: &n Net(""), clock: &k Clock, io: &!i Io, a: &g Args, ns_ip: &q [byte], ns_port: int, first: int, count: int) -> [args, heap, net_out(""), conn_read, conn_write, poll, clock, io_write] int {
    match poller_new() {
        Polling::Failed(e) => {
            return 4;
        }
        Polling::Ok(pl) => {
            var poll = pl;
            var rs = rtcp.open(heap);
            let t0 = clock_ms(clock);
            var last = t0;
            var maxgap = 0;
            var iters = 0;
            var started = 0;
            var finished = 0;
            let start_at = t0 + 50;
            region r {
                let rsl = alloc_slice[r](32 * rec(), 0);
                let t_start = alloc_slice[r](32, 0);
                let t_done = alloc_slice[r](32, 0);
                let events = alloc_slice[r](128, 0);
                while finished < count {
                    let now = clock_ms(clock);
                    if now - last > maxgap {
                        maxgap = now - last;
                    }
                    last = now;
                    iters = iters + 1;
                    while now >= start_at && started < count && started - finished < rtcp.slots() {
                        t_start[started] = now;
                        var slot = 0 - 1;
                        var code = 0;
                        borrow mut poll as &!pw in {
                            let (grown, s, c) = rtcp.start(heap, rs, net, pw, ns_ip, ns_port, arg(a, first + started), 100, started, 4000 + started, now + 5000);
                            rs = grown;
                            slot = s;
                            code = c;
                        }
                        if slot < 0 {
                            rsl[started * rec()] = code;
                            t_done[started] = now;
                            finished = finished + 1;
                        }
                        started = started + 1;
                    }
                    var ready = 0;
                    borrow mut poll as &!pw in {
                        ready = poller_wait(pw, events, 1);
                    }
                    var j = 0;
                    while j < ready {
                        let slot = events[2 * j] - 100;
                        var live = false;
                        borrow rs as &rr in {
                            live = rtcp.busy(rr, slot);
                        }
                        if live {
                            var code = rtcp.pending();
                            borrow mut rs as &!rw in {
                                borrow mut poll as &!pw in {
                                    code = rtcp.advance(rw, pw, slot, 100);
                                }
                            }
                            if code != rtcp.pending() {
                                borrow rs as &rr in {
                                    let id = rtcp.lookup_of(rr, slot);
                                    rsl[id * rec()] = code;
                                    var m = 0;
                                    if code > 0 {
                                        rsl[id * rec() + 1] = code;
                                        rsl[id * rec() + 2] = rtcp.ttl_of(rr, slot);
                                        while m < code {
                                            rsl[id * rec() + 3 + m] = rtcp.addr_of(rr, slot, m);
                                            m = m + 1;
                                        }
                                    }
                                    t_done[id] = clock_ms(clock);
                                }
                                borrow mut rs as &!rw in {
                                    rtcp.finish(rw, slot);
                                }
                                finished = finished + 1;
                            }
                        }
                        j = j + 1;
                    }
                    var s = 0;
                    while s < rtcp.slots() {
                        var late = false;
                        var id = 0;
                        borrow rs as &rr in {
                            late = rtcp.expired(rr, s, now);
                            id = rtcp.lookup_of(rr, s);
                        }
                        if late {
                            rsl[id * rec()] = rtcp.timed_out();
                            t_done[id] = now;
                            borrow mut rs as &!rw in {
                                rtcp.finish(rw, s);
                            }
                            finished = finished + 1;
                        }
                        s = s + 1;
                    }
                }
                var j = 0;
                while j < count {
                    show(io, j, arg(a, first + j), rsl, t_done[j] - t_start[j]);
                    j = j + 1;
                }
            }
            summary(io, iters, maxgap, clock_ms(clock) - t0);
            rtcp.close(heap, rs);
            poller_close(poll);
            return 0;
        }
    }
}

// ---- thread: four workers, res_query, answers through the poller ---------------------------------------

// A function value cannot be written `rthread.worker` (gap 9), so the module's function is called through one declared here.
fn worker[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    return rthread.worker(ffi);
}

fn run_thread[&f, &h, &n, &k, &i, &g](ffi: &f Ffi("libc"), heap: &!h Heap, net: &n Net(""), clock: &k Clock, io: &!i Io, a: &g Args, first: int, count: int) -> [args, conc, ffi("libc"), heap, net_in(""), conn_accept, conn_read, conn_write, poll, clock, io_write] int {
    match poller_new() {
        Polling::Failed(e) => {
            return 4;
        }
        Polling::Ok(pl) => {
            var poll = pl;
            var status = 0;
            match tcp_listen(net, rthread.bell_port(), 16, 0) {
                Listening::Failed(e) => {
                    io.write_all(io, "cannot listen on the bell port\n");
                    poller_close(poll);
                    status = 5;
                }
                Listening::Ok(l) => {
                    var listener = l;
                    borrow mut listener as &!lh in {
                        listener_nonblocking(lh);
                        borrow mut poll as &!pw in {
                            poller_add_listener(pw, lh, 0);
                        }
                        // Four threads, each with a function value of its own (docs/parallelism.md section 3.4).
                        let f0 = worker;
                        let f1 = worker;
                        let f2 = worker;
                        let f3 = worker;
                        let t0 = spawn(ffi, f0);
                        let t1 = spawn(ffi, f1);
                        let t2 = spawn(ffi, f2);
                        let t3 = spawn(ffi, f3);
                        status = serve_thread(heap, clock, io, a, first, count, lh, poll);
                        let r0 = join(t0);
                        let r1 = join(t1);
                        let r2 = join(t2);
                        let r3 = join(t3);
                        if r0 + r1 + r2 + r3 != 0 {
                            status = 6;
                        }
                    }
                    listener_close(listener);
                }
            }
            return status;
        }
    }
}

// The main thread's loop of the thread mode. `poll0` is moved in and closed here; every worker connection is closed on the way out,
// which is what tells the workers to end.
fn serve_thread[&h, &k, &i, &g, &l](heap: &!h Heap, clock: &k Clock, io: &!i Io, a: &g Args, first: int, count: int, listener: &!l Listener, poll0: Poller) -> [args, heap, conn_accept, conn_read, conn_write, clock, io_write] int {
    var poll = poll0;
    var table = conns.empty(heap, 4);
    var accepted = 0;
    let t0 = clock_ms(clock);
    var last = t0;
    var maxgap = 0;
    var iters = 0;
    var started = 0;
    var finished = 0;
    let start_at = t0 + 50;
    region r {
        let rsl = alloc_slice[r](32 * rec(), 0);
        let t_start = alloc_slice[r](32, 0);
        let t_done = alloc_slice[r](32, 0);
        let events = alloc_slice[r](128, 0);
        // ws: per worker [busy, lookup id, reply bytes held, -]; rb: 64 bytes of reply per worker
        let ws = alloc_slice[r](4 * 4, 0);
        let rb = alloc_slice[r](4 * 64, byte_of(0));
        let wreq = alloc_slice[r](260, byte_of(0));
        let deadline = t0 + 20000;
        while finished < count && clock_ms(clock) < deadline {
            let now = clock_ms(clock);
            if now - last > maxgap {
                maxgap = now - last;
            }
            last = now;
            iters = iters + 1;
            // Hand a lookup to every idle worker.
            if accepted == 4 && now >= start_at {
                var w = 0;
                while w < 4 && started < count {
                    if ws[4 * w] == 0 {
                        let name = arg(a, first + started);
                        wreq[0] = byte_of(len(name));
                        var c = 0;
                        while c < len(name) {
                            wreq[1 + c] = name[c];
                            c = c + 1;
                        }
                        wreq[1 + len(name)] = byte_of(0);
                        t_start[started] = clock_ms(clock);
                        var sent = Sent::Failed(9);
                        borrow mut table as &!tw in {
                            sent = conns.write(tw, w, wreq[0..len(name) + 2]);
                        }
                        ws[4 * w] = 1;
                        ws[4 * w + 1] = started;
                        ws[4 * w + 2] = 0;
                        started = started + 1;
                        match sent {
                            Sent::Wrote(n) => {
                            }
                            Sent::Again => {
                            }
                            Sent::Failed(e) => {
                                rsl[(started - 1) * rec()] = 0 - 302;
                                t_done[started - 1] = clock_ms(clock);
                                ws[4 * w] = 0;
                                finished = finished + 1;
                            }
                        }
                    }
                    w = w + 1;
                }
            }
            var ready = 0;
            borrow mut poll as &!pw in {
                ready = poller_wait(pw, events, 1);
            }
            var j = 0;
            while j < ready {
                let token = events[2 * j];
                if token == 0 {
                    // A worker has dialled in: its connection becomes the next slot, and the worker's number.
                    match tcp_accept(listener) {
                        Accepted::Ok(c) => {
                            var conn = c;
                            borrow mut conn as &!ch in {
                                conn_nonblocking(ch);
                            }
                            let (grown, slot) = conns.put(heap, table, conn);
                            table = grown;
                            if slot >= 0 && slot < 4 {
                                borrow mut table as &!tw in {
                                    borrow mut poll as &!pw in {
                                        conns.watch(tw, pw, slot, 1 + slot, 1);
                                    }
                                }
                                accepted = accepted + 1;
                            }
                        }
                        Accepted::Again => {
                        }
                        Accepted::Failed(e) => {
                        }
                    }
                } else if token >= 1 && token <= 4 {
                    let w = token - 1;
                    var got = 0 - 1;
                    var dead = false;
                    borrow mut table as &!tw in {
                        match conns.read(tw, w, rb[w * 64 + ws[4 * w + 2]..w * 64 + 64]) {
                            Received::Data(n) => {
                                got = n;
                            }
                            Received::Again => {
                            }
                            Received::End => {
                                dead = true;
                            }
                            Received::Failed(e) => {
                                dead = true;
                            }
                        }
                    }
                    if got > 0 {
                        ws[4 * w + 2] = ws[4 * w + 2] + got;
                        let need = rthread.reply_length(rb, w * 64, ws[4 * w + 2]);
                        if need > 0 && ws[4 * w + 2] >= need {
                            let id = ws[4 * w + 1];
                            rthread.read_reply(rb, w * 64, rsl, id * rec());
                            t_done[id] = clock_ms(clock);
                            ws[4 * w] = 0;
                            ws[4 * w + 2] = 0;
                            finished = finished + 1;
                        }
                    }
                    if dead && ws[4 * w] == 1 {
                        let id = ws[4 * w + 1];
                        rsl[id * rec()] = 0 - 301;
                        t_done[id] = clock_ms(clock);
                        ws[4 * w] = 0;
                        finished = finished + 1;
                    }
                }
                j = j + 1;
            }
        }
        var j = 0;
        while j < count {
            show(io, j, arg(a, first + j), rsl, t_done[j] - t_start[j]);
            j = j + 1;
        }
    }
    summary(io, iters, maxgap, clock_ms(clock) - t0);
    conns.drop(heap, table);
    poller_close(poll);
    return 0;
}

fn main(world: World) -> [conc] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(fs);
    let tlsffi = narrow(ffi, "libc");
    var status = 2;
    borrow mut io as &!i in {
        borrow args as &g in {
            if arg_count(g) < 6 {
                io.error_all(i, "usage: resolve_demo <blocking|tcp|thread> <ns-ip> <ns-port> <count> <name>...\n");
            } else {
                let count = nat.parse(arg(g, 4));
                let port = nat.parse(arg(g, 3));
                if count < 1 || count > 32 || arg_count(g) < 5 + count || port < 1 || port > 65535 {
                    io.error_all(i, "resolve_demo: bad count or port\n");
                } else {
                    let mode = arg(g, 1);
                    borrow mut heap as &!h in {
                        borrow net as &nn in {
                            borrow clock as &c in {
                                borrow tlsffi as &f in {
                                    if len(mode) == 3 && int_of(mode[0]) == 't' && int_of(mode[1]) == 'c' {
                                        status = run_tcp(h, nn, c, i, g, arg(g, 2), port, 5, count);
                                    } else if len(mode) == 6 {
                                        status = run_thread(f, h, nn, c, i, g, 5, count);
                                    } else {
                                        status = run_blocking(nn, c, i, g, 5, count);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    release(tlsffi);
    release(net);
    release(clock);
    release(args);
    release(io);
    release(heap);
    return status;
}
