edition 5;

module rtcp;

import std.conns;
import dns;

// `rtcp` -- a resolver on the poller: a DNS A lookup made over TCP, as one more state machine beside the TLS ones.
//
// Nothing here blocks and nothing here is foreign: the query is dialled with `tcp_connect_start` to the name server's address
// (an IP literal), written, and the answer read as the poller reports it, under a deadline the caller keeps. No thread, no
// `Ffi`, no libc. What it gives up against `getaddrinfo` is stated in `docs/tls-nonblocking.md` section 7: no `/etc/hosts`, no
// search list, no retry across several servers (one is given), TCP only (every recursive resolver that matters answers it; a
// query and its answer cost one connection's handshake more than UDP would).
//
// The shape is the one `attempt.ls` has: a `Resolver` owns its connections (a `std.conns` table) and its buffers, up to `slots()`
// lookups are in flight, and each is a slot the caller watches under `token0 + slot`. A lookup is `start`ed, `advance`d when the
// poller reports its token, and `finish`ed. Its answer stays readable (`count_of`, `addr_of`, `ttl_of`) until `finish`.

pub fn slots() -> [] int {
    return 16;
}

fn stride() -> [] int {
    return 8;
}

fn q_max() -> [] int {
    return 300;
}

fn a_max() -> [] int {
    return 1500;
}

// Per slot: [phase, lookup id, bytes sent, bytes received, deadline, query length, query id, answer length]. The answers:
// `dns.addrs_size()` integers per slot (the addresses, then the smallest TTL), and the count in `counts`.
res struct Core {
    at: Box[[int]],
    qbuf: Box[[byte]],
    abuf: Box[[byte]],
    answers: Box[[int]],
    counts: Box[[int]],
}

pub res struct Resolver {
    tab: conns.Table,
    core: Core,
}

fn connecting() -> [] int {
    return 1;
}

fn sending() -> [] int {
    return 2;
}

fn reading() -> [] int {
    return 3;
}

pub fn pending() -> [] int {
    return 0 - 1000;
}

// Answers a lookup ends with besides `dns.parse`'s (an address count of 0 or more, or its negative codes).
pub fn no_connect() -> [] int {
    return 0 - 200;
}

pub fn no_send() -> [] int {
    return 0 - 201;
}

pub fn no_answer() -> [] int {
    return 0 - 202;
}

pub fn timed_out() -> [] int {
    return 0 - 203;
}

pub fn no_slot() -> [] int {
    return 0 - 204;
}

pub fn bad_name() -> [] int {
    return 0 - 205;
}

pub fn open[&h](heap: &!h Heap) -> [heap] Resolver {
    let core = Core { at: box_slice(heap, slots() * stride(), 0), qbuf: box_slice(heap, slots() * q_max(), byte_of(0)), abuf: box_slice(heap, slots() * a_max(), byte_of(0)), answers: box_slice(heap, slots() * dns.addrs_size(), 0), counts: box_slice(heap, slots(), 0) };
    return Resolver { tab: conns.empty(heap, slots()), core: core };
}

// End the resolver: closes every connection still open and frees the buffers.
pub fn close[&h](heap: &!h Heap, rs: Resolver) -> [heap] int {
    let Resolver { tab, core } = rs;
    let Core { at, qbuf, abuf, answers, counts } = core;
    unbox_slice(heap, at);
    unbox_slice(heap, qbuf);
    unbox_slice(heap, abuf);
    unbox_slice(heap, answers);
    unbox_slice(heap, counts);
    conns.drop(heap, tab);
    return 0;
}

pub fn busy[&r](rs: &r Resolver, slot: int) -> [] bool {
    return slot >= 0 && slot < slots() && contents(rs.core.at)[slot * stride()] != 0;
}

pub fn lookup_of[&r](rs: &r Resolver, slot: int) -> [] int {
    return contents(rs.core.at)[slot * stride() + 1];
}

pub fn expired[&r](rs: &r Resolver, slot: int, now: int) -> [] bool {
    return busy(rs, slot) && now >= contents(rs.core.at)[slot * stride() + 4];
}

// How many addresses the finished lookup in `slot` has, the `k`-th of them (packed `a * 2^24 + b * 2^16 + c * 2^8 + d`), and the
// smallest TTL among them in seconds.
pub fn count_of[&r](rs: &r Resolver, slot: int) -> [] int {
    return contents(rs.core.counts)[slot];
}

pub fn addr_of[&r](rs: &r Resolver, slot: int, k: int) -> [] int {
    return contents(rs.core.answers)[slot * dns.addrs_size() + k];
}

pub fn ttl_of[&r](rs: &r Resolver, slot: int) -> [] int {
    return contents(rs.core.answers)[slot * dns.addrs_size() + dns.max_addrs()];
}

// Start a lookup of `name` at the server `ns_ip:ns_port`, watching its connection for writable under token `token0 + slot`.
// Answers the resolver and `(slot, code)`: `slot` is where it lives (code `pending()`), or -1 with the reason in `code`
// (`no_slot()`, `bad_name()`, `no_connect()`), in which case nothing is left to clean up. `query_id` is the 16-bit DNS id: the
// caller picks it, it must not repeat while a lookup to the same server is open, and it should not be guessable.
pub fn start[&h, &n, &p, &q, &r](heap: &!h Heap, rs: Resolver, net: &n Net(""), poller: &!p Poller, ns_ip: &q [byte], ns_port: int, name: &r [byte], token0: int, lookup: int, query_id: int, deadline: int) -> [heap, net_out(""), poll] (Resolver, int, int) {
    let Resolver { tab, core } = rs;
    var cr = core;
    var held = 0;
    borrow tab as &tt in {
        held = conns.live(tt);
    }
    if held >= slots() {
        return (Resolver { tab: tab, core: cr }, 0 - 1, no_slot());
    }
    match tcp_connect_start(net, ns_ip, ns_port) {
        Dialed::Failed(err) => {
            return (Resolver { tab: tab, core: cr }, 0 - 1, no_connect());
        }
        Dialed::Ok(c) => {
            let (grown, slot) = conns.put(heap, tab, c);
            var table = grown;
            if slot < 0 || slot >= slots() {
                if slot >= 0 {
                    borrow mut table as &!ct in {
                        conns.close(ct, slot);
                    }
                }
                return (Resolver { tab: table, core: cr }, 0 - 1, no_connect());
            }
            // TCP DNS: a two-byte length, then the message.
            let base = slot * q_max();
            var qlen = 0 - 1;
            borrow mut cr as &!cw in {
                qlen = dns.build_query(name, query_id, contents(cw.qbuf), base + 2);
                if qlen >= 0 && qlen + 2 <= q_max() {
                    contents(cw.qbuf)[base] = byte_of(qlen / 256 % 256);
                    contents(cw.qbuf)[base + 1] = byte_of(qlen % 256);
                }
            }
            if qlen < 0 || qlen + 2 > q_max() {
                borrow mut table as &!ct in {
                    conns.close(ct, slot);
                }
                return (Resolver { tab: table, core: cr }, 0 - 1, bad_name());
            }
            var watched = 0 - 1;
            borrow mut table as &!ct in {
                watched = conns.watch(ct, poller, slot, token0 + slot, 2);
                if watched != 0 {
                    conns.close(ct, slot);
                }
            }
            if watched != 0 {
                return (Resolver { tab: table, core: cr }, 0 - 1, no_connect());
            }
            let b = slot * stride();
            borrow mut cr as &!cw in {
                let at = contents(cw.at);
                at[b] = connecting();
                at[b + 1] = lookup;
                at[b + 2] = 0;
                at[b + 3] = 0;
                at[b + 4] = deadline;
                at[b + 5] = qlen + 2;
                at[b + 6] = query_id;
                at[b + 7] = 0;
                contents(cw.counts)[slot] = 0;
            }
            return (Resolver { tab: table, core: cr }, slot, pending());
        }
    }
}

// Copy the parse answer of `slot` out of the scratch array into the slot's own.
fn keep[&a, &s, &c](scratch: &a [int], answers: &!s [int], counts: &!c [int], slot: int, code: int) -> [] int {
    var k = 0;
    while k < dns.addrs_size() {
        answers[slot * dns.addrs_size() + k] = scratch[k];
        k = k + 1;
    }
    if code > 0 {
        counts[slot] = code;
    } else {
        counts[slot] = 0;
    }
    return code;
}

// Move the lookup in `slot` along after the poller reported its connection. Answers `pending()` while it waits, otherwise how it
// ended: `dns.parse`'s answer (the number of addresses, readable with `count_of`/`addr_of`/`ttl_of`; or one of `dns`'s negative
// codes), or `no_connect()`, `no_send()`, `no_answer()`. The caller then calls `finish`.
pub fn advance[&r, &p](rs: &!r Resolver, poller: &!p Poller, slot: int, token0: int) -> [conn_read, conn_write, poll] int {
    let b = slot * stride();
    let at = contents(rs.core.at);
    var progress = true;
    while progress {
        progress = false;
        if at[b] == connecting() {
            if conns.connect_status(rs.tab, slot) != 0 {
                return no_connect();
            }
            at[b] = sending();
            progress = true;
        } else if at[b] == sending() {
            let base = slot * q_max();
            match conns.write(rs.tab, slot, contents(rs.core.qbuf)[base + at[b + 2]..base + at[b + 5]]) {
                Sent::Wrote(k) => {
                    at[b + 2] = at[b + 2] + k;
                    if at[b + 2] >= at[b + 5] {
                        at[b] = reading();
                        if conns.rewatch(rs.tab, poller, slot, token0 + slot, 1) != 0 {
                            return no_send();
                        }
                    }
                    progress = true;
                }
                Sent::Again => {
                    return pending();
                }
                Sent::Failed(err) => {
                    return no_send();
                }
            }
        } else if at[b] == reading() {
            let base = slot * a_max();
            let abuf = contents(rs.core.abuf);
            // The answer is a two-byte length then that many bytes; read into the slot's buffer from where we stopped.
            match conns.read(rs.tab, slot, abuf[base + at[b + 3]..base + a_max()]) {
                Received::Data(k) => {
                    at[b + 3] = at[b + 3] + k;
                    if at[b + 3] >= 2 {
                        let want = int_of(abuf[base]) * 256 + int_of(abuf[base + 1]);
                        if want + 2 > a_max() || want < 12 {
                            return dns.malformed();
                        }
                        if at[b + 3] >= want + 2 {
                            at[b + 7] = want;
                            var code = 0;
                            region scratch {
                                let tmp = alloc_slice[scratch](dns.addrs_size(), 0);
                                code = dns.parse(abuf[base + 2..base + 2 + want], want, at[b + 6], tmp);
                                keep(tmp, contents(rs.core.answers), contents(rs.core.counts), slot, code);
                            }
                            return code;
                        }
                    }
                    progress = true;
                }
                Received::End => {
                    return no_answer();
                }
                Received::Again => {
                    return pending();
                }
                Received::Failed(err) => {
                    return no_answer();
                }
            }
        }
    }
    return pending();
}

// End the lookup in `slot`: close its connection and free the slot.
pub fn finish[&r](rs: &!r Resolver, slot: int) -> [] int {
    conns.close(rs.tab, slot);
    contents(rs.core.at)[slot * stride()] = 0;
    return 0;
}
