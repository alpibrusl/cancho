edition 5;

module tls;

import std.chacha20;
import tls_client;
import tls_record;
import tls_slot;
import x509_verify;

// `tls` -- the engine: many TLS 1.3 client connections in slots, bytes in
// and bytes out, no socket and no capability inside (`docs/tls-pure.md`
// §2.2; `docs/tls-core.md` §10). The caller owns the sockets and the
// poller; the engine owns each slot's state in two boxes, as
// `rtcp.Resolver` owns its lookups (`docs/tls-pure.md` §2.1). Not
// independently reviewed (#209).
//
// A server's chain is verified against the roots `trust` was given, the
// host and the time `start` was given (`docs/x509-verify.md`).
// Revocation is not checked: a revoked certificate that is otherwise
// valid is accepted (`docs/tls-pure.md` §5.4).

pub res struct Engine {
    ints: Box[[int]],
    bytes: Box[[byte]],
    roots: Box[[byte]],
    meta: Box[[int]],
    drbg: Box[[byte]],
    // Tickets for resumption (`docs/tls-resumption.md` §4): each entry's
    // ticket, PSK and host name, and in `tmeta` what the rules of §3 need.
    tickets: Box[[byte]],
    tmeta: Box[[int]],
}

// The roots' room, in `x509_verify.store_load`'s format: this machine's
// 128-root bundle takes 138,350 bytes (`docs/x509-verify.md` §8.2).
fn roots_cap() -> [] int {
    return 1048576;
}

// meta: [0] slots, [1] seeded, [2] bytes of roots, [3] blocks skipped, [4 + s] slot `s` in use,
// [4 + slots + s] when slot `s`'s connection started (Unix milliseconds).
fn m_slots() -> [] int {
    return 0;
}

fn m_seeded() -> [] int {
    return 1;
}

fn m_roots_len() -> [] int {
    return 2;
}

fn m_skipped() -> [] int {
    return 3;
}

fn m_busy(slot: int) -> [] int {
    return 4 + slot;
}

fn m_started[&e](engine: &e Engine, slot: int) -> [] int {
    return 4 + contents(engine.meta)[m_slots()] + slot;
}

pub fn open[&h](heap: &!h Heap, slots: int) -> [heap] Engine {
    return open_with_tickets(heap, slots, slots);
}

// `open`, with room for `tickets` saved tickets (`save`; at least 1, at
// most 65,534). A save when the room is full replaces the oldest.
pub fn open_with_tickets[&h](heap: &!h Heap, slots: int, tickets: int) -> [heap] Engine {
    var n = slots;
    if n < 1 {
        n = 1;
    }
    var t = tickets;
    if t < 1 {
        t = 1;
    }
    if t > 65534 {
        t = 65534;
    }
    var engine = Engine { ints: box_slice(heap, n * tls_client.ints_len(), 0), bytes: box_slice(heap, n * tls_client.bytes_len(), byte_of(0)), roots: box_slice(heap, roots_cap(), byte_of(0)), meta: box_slice(heap, 4 + 2 * n, 0), drbg: box_slice(heap, 32, byte_of(0)), tickets: box_slice(heap, t * entry_bytes(), byte_of(0)), tmeta: box_slice(heap, t_entries() + t * t_fields(), 0) };
    borrow mut engine as &!w in {
        contents(w.meta)[m_slots()] = n;
        contents(w.tmeta)[t_capacity()] = t;
        contents(w.tmeta)[t_max_age()] = 3600;
    }
    return engine;
}

// Frees the engine, after overwriting every slot's secrets and the DRBG
// key (best effort, `docs/tls-core.md` §8).
pub fn close[&h](heap: &!h Heap, engine: Engine) -> [heap] int {
    var e = engine;
    borrow mut e as &!w in {
        var s = 0;
        while s < contents(w.meta)[m_slots()] {
            drop(w, s);
            s = s + 1;
        }
        let k = contents(w.drbg);
        var i = 0;
        while i < 32 {
            k[i] = byte_of(0);
            i = i + 1;
        }
    }
    borrow mut e as &!w in {
        var t = 0;
        while t < contents(w.tmeta)[t_capacity()] {
            wipe(w, t);
            t = t + 1;
        }
    }
    let Engine { ints, bytes, roots, meta, drbg, tickets, tmeta } = e;
    unbox_slice(heap, ints);
    unbox_slice(heap, bytes);
    unbox_slice(heap, roots);
    unbox_slice(heap, meta);
    unbox_slice(heap, drbg);
    unbox_slice(heap, tickets);
    unbox_slice(heap, tmeta);
    return 0;
}

pub fn slots[&e](engine: &e Engine) -> [] int {
    return contents(engine.meta)[m_slots()];
}

// The roots of a PEM bundle (the system's, or an operator's file) become
// the trust store, replacing any before. A block that is not a
// certificate this package reads is skipped, never fatal; `skipped`
// says how many. Answers the roots stored, or `x509-chain-too-large`
// when they do not fit. No root is a store that trusts nothing, which
// the caller must not start with (`docs/tls-pure.md` §5.1).
pub fn trust[&e, &p](engine: &!e Engine, pem: &p [byte]) -> [] int {
    // A ticket saved before is not offered after this (`docs/tls-resumption.md` §3, rule 2).
    contents(engine.tmeta)[t_trust()] = contents(engine.tmeta)[t_trust()] + 1;
    var count = 0;
    region r {
        let info = alloc_slice[r](2, 0);
        count = x509_verify.store_load(pem, contents(engine.roots), info);
        if count < 0 {
            contents(engine.meta)[m_roots_len()] = 0;
            count = tls_record.x509_chain_too_large();
        } else {
            contents(engine.meta)[m_roots_len()] = info[0];
        }
        contents(engine.meta)[m_skipped()] = info[1];
    }
    return count;
}

// The blocks the last `trust` skipped.
pub fn skipped[&e](engine: &e Engine) -> [] int {
    return contents(engine.meta)[m_skipped()];
}

// 32 bytes of the caller's entropy key the DRBG (`docs/tls-pure.md` §6).
pub fn seed[&e, &s](engine: &!e Engine, entropy: &s [byte]) -> [] int {
    if len(entropy) != 32 {
        return tls_record.no_entropy();
    }
    let k = contents(engine.drbg);
    var i = 0;
    while i < 32 {
        k[i] = entropy[i];
        i = i + 1;
    }
    contents(engine.meta)[m_seeded()] = 1;
    return 0;
}

// Fast-key-erasure (`docs/tls-pure.md` §6): each ChaCha20 block's first 32
// bytes replace the key, its last 32 are output.
fn draw[&e, &o](engine: &!e Engine, out: &!o [byte]) -> [] int {
    let key = contents(engine.drbg);
    region r {
        let nonce = alloc_slice[r](12, byte_of(0));
        let stream = alloc_slice[r](64, byte_of(0));
        var done = 0;
        while done < len(out) {
            chacha20.block(key, 0, nonce, stream);
            var i = 0;
            while i < 32 {
                key[i] = stream[i];
                i = i + 1;
            }
            i = 0;
            while i < 32 && done < len(out) {
                out[done] = stream[32 + i];
                done = done + 1;
                i = i + 1;
            }
        }
        var i = 0;
        while i < 64 {
            stream[i] = byte_of(0);
            i = i + 1;
        }
    }
    return 0;
}

fn slot_ok[&e](engine: &e Engine, slot: int) -> [] bool {
    return slot >= 0 && slot < contents(engine.meta)[m_slots()];
}

fn ints_of(slot: int) -> [] int {
    return slot * tls_client.ints_len();
}

fn bytes_of(slot: int) -> [] int {
    return slot * tls_client.bytes_len();
}

// Starts a connection in `slot` to the server named `host`. `now_unix_ms`
// (`clock_unix_ms`) is the time the server's certificates are checked
// against. 0, or a refusal.
pub fn start[&e, &h](engine: &!e Engine, slot: int, host: &h [byte], now_unix_ms: int) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] != 0 {
        return tls_record.bad_slot();
    }
    if contents(engine.meta)[m_seeded()] != 1 {
        return tls_record.no_entropy();
    }
    var code = 0;
    region r {
        let random = alloc_slice[r](96, byte_of(0));
        draw(engine, random);
        let i = ints_of(slot);
        let b = bytes_of(slot);
        code = tls_client.start_psk(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()], host, random, now_unix_ms / 1000, host[0..0], host[0..0], 0, 0, 0, contents(engine.tmeta)[t_resume()] == 1);
        contents(engine.meta)[m_started(engine, slot)] = now_unix_ms;
        var k = 0;
        while k < 96 {
            random[k] = byte_of(0);
            k = k + 1;
        }
    }
    contents(engine.meta)[m_busy(slot)] = 1;
    return code;
}

// Bytes the socket gave: how many were taken, or the connection's failure.
pub fn feed[&e, &d](engine: &!e Engine, slot: int, data: &d [byte]) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return tls_record.bad_slot();
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    let used = contents(engine.meta)[m_roots_len()];
    return tls_client.feed(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()], data, contents(engine.roots)[0..used]);
}

// Bytes for the socket.
pub fn take[&e, &o](engine: &!e Engine, slot: int, out: &!o [byte]) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return 0;
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    return tls_client.take(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()], out);
}

pub fn send[&e, &p](engine: &!e Engine, slot: int, plaintext: &p [byte]) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return tls_record.bad_slot();
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    return tls_client.send(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()], plaintext);
}

// Application data: how many bytes, 0 after the peer's close_notify,
// `would_block()` when nothing is waiting, or the failure.
pub fn recv[&e, &o](engine: &!e Engine, slot: int, into: &!o [byte]) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return tls_record.bad_slot();
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    return tls_client.recv(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()], into);
}

pub fn would_block() -> [] int {
    return tls_client.would_block();
}

// The socket ended (`tls_client.peer_eof`).
pub fn eof[&e](engine: &!e Engine, slot: int) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return tls_record.bad_slot();
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    return tls_client.peer_eof(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()]);
}

pub fn finish[&e](engine: &!e Engine, slot: int) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return tls_record.bad_slot();
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    return tls_client.finish(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()]);
}

pub fn event_want_read() -> [] int {
    return tls_client.event_want_read();
}

pub fn event_want_write() -> [] int {
    return tls_client.event_want_write();
}

pub fn event_established() -> [] int {
    return tls_client.event_established();
}

pub fn event_closed() -> [] int {
    return tls_client.event_closed();
}

pub fn event_failed() -> [] int {
    return tls_client.event_failed();
}

pub fn event[&e](engine: &e Engine, slot: int) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return tls_client.event_failed();
    }
    let i = ints_of(slot);
    return tls_client.event(contents(engine.ints)[i..i + tls_client.ints_len()]);
}

pub fn failure[&e](engine: &e Engine, slot: int) -> [] int {
    if !slot_ok(engine, slot) {
        return tls_record.bad_slot();
    }
    let i = ints_of(slot);
    return tls_client.failure(contents(engine.ints)[i..i + tls_client.ints_len()]);
}

pub fn refusal_tag(code: int) -> [] &static [byte] {
    return tls_record.refusal_tag(code);
}

// Frees `slot`, overwriting its secrets (best effort, `docs/tls-core.md` §8).
pub fn drop[&e](engine: &!e Engine, slot: int) -> [] int {
    if !slot_ok(engine, slot) {
        return tls_record.bad_slot();
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    tls_client.drop(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()]);
    contents(engine.meta)[m_busy(slot)] = 0;
    return 0;
}

// ---- Resumption (`docs/tls-resumption.md`) ----

// A ticket entry's bytes: the ticket, its PSK, the host name it is for.
fn entry_bytes() -> [] int {
    return tls_slot.ticket_cap() + 48 + 256;
}

fn e_psk() -> [] int {
    return tls_slot.ticket_cap();
}

fn e_host() -> [] int {
    return tls_slot.ticket_cap() + 48;
}

// `tmeta`: [0] entries, [1] trust generation, [2] the longest a
// verification is relied on, in seconds, [3] the next entry a full table
// replaces, [4] 1 if `set_resumption` turned resumption on; then each
// entry's fields.
fn t_capacity() -> [] int {
    return 0;
}

fn t_trust() -> [] int {
    return 1;
}

fn t_max_age() -> [] int {
    return 2;
}

fn t_hand() -> [] int {
    return 3;
}

fn t_resume() -> [] int {
    return 4;
}

fn t_entries() -> [] int {
    return 5;
}

// An entry's fields: generation, in use, ticket length, hash length, host
// length, when received (Unix milliseconds: when its connection started,
// so an age is never short), lifetime (seconds), ticket_age_add,
// verified at, the leaf's notAfter, the trust generation it was saved under.
fn t_fields() -> [] int {
    return 11;
}

fn tf(e: int, field: int) -> [] int {
    return t_entries() + e * t_fields() + field;
}

// What a handle names: an entry in use under the same generation, or -1.
fn entry_of[&e](engine: &e Engine, handle: int) -> [] int {
    let e = handle % 65536 - 1;
    if handle <= 0 || e < 0 || e >= contents(engine.tmeta)[t_capacity()] {
        return 0 - 1;
    }
    if contents(engine.tmeta)[tf(e, 1)] != 1 || contents(engine.tmeta)[tf(e, 0)] != handle / 65536 {
        return 0 - 1;
    }
    return e;
}

// Overwrites entry `e` and frees it. Its generation stays, so a handle to
// it is refused from now on.
fn wipe[&e](engine: &!e Engine, e: int) -> [] int {
    let at = e * entry_bytes();
    tls_slot.zero(contents(engine.tickets)[at..at + entry_bytes()]);
    var f = 1;
    while f < t_fields() {
        contents(engine.tmeta)[tf(e, f)] = 0;
        f = f + 1;
    }
    return 0;
}

// Whether this engine's connections say they can resume
// (`psk_key_exchange_modes`): off unless turned on. A server may withhold
// tickets from a client that does not say it (RFC 8446 §4.2.9), so a
// caller that will `save` turns it on; one that never will leaves it off,
// and claims nothing it does not do.
pub fn set_resumption[&e](engine: &!e Engine, on: bool) -> [] int {
    contents(engine.tmeta)[t_resume()] = 0;
    if on {
        contents(engine.tmeta)[t_resume()] = 1;
    }
    return 0;
}

// The longest, in seconds, a full handshake's verification is relied on
// by the resumptions that follow it (`docs/tls-resumption.md` §3, rule 4;
// 3,600 unless set).
pub fn set_ticket_max_age[&e](engine: &!e Engine, seconds: int) -> [] int {
    contents(engine.tmeta)[t_max_age()] = seconds;
    return 0;
}

// Keeps the newest ticket the connection in `slot` received, for a later
// `start_with`: a handle (more than 0), or 0 when there is none to keep
// (no ticket came, the connection failed, or it was TLS 1.2). A ticket is
// saved once: a second `save` of the same connection answers 0.
pub fn save[&e](engine: &!e Engine, slot: int) -> [] int {
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] == 0 {
        return 0;
    }
    let i = ints_of(slot);
    let b = bytes_of(slot);
    let state = contents(engine.ints)[i + tls_slot.i_state()];
    let n = contents(engine.ints)[i + tls_slot.i_ticket_len()];
    let flags = contents(engine.ints)[i + tls_slot.i_flags()];
    if n == 0 || state == tls_slot.state_failed() || flags & tls_slot.f_tls12() != 0 {
        return 0;
    }
    let cap = contents(engine.tmeta)[t_capacity()];
    var e = 0;
    while e < cap && contents(engine.tmeta)[tf(e, 1)] == 1 {
        e = e + 1;
    }
    if e == cap {
        e = contents(engine.tmeta)[t_hand()];
        contents(engine.tmeta)[t_hand()] = (e + 1) % cap;
        wipe(engine, e);
    }
    let h = contents(engine.ints)[i + tls_slot.i_ticket_hash()];
    let host_len = contents(engine.ints)[i + tls_slot.i_host_len()];
    let at = e * entry_bytes();
    var k = 0;
    while k < n {
        contents(engine.tickets)[at + k] = contents(engine.bytes)[b + tls_slot.b_ticket() + k];
        k = k + 1;
    }
    k = 0;
    while k < h {
        contents(engine.tickets)[at + e_psk() + k] = contents(engine.bytes)[b + tls_slot.k_ticket_psk() + k];
        k = k + 1;
    }
    k = 0;
    while k < host_len {
        contents(engine.tickets)[at + e_host() + k] = contents(engine.bytes)[b + tls_slot.b_ticket_host() + k];
        k = k + 1;
    }
    let gen = contents(engine.tmeta)[tf(e, 0)] + 1;
    contents(engine.tmeta)[tf(e, 0)] = gen;
    contents(engine.tmeta)[tf(e, 1)] = 1;
    contents(engine.tmeta)[tf(e, 2)] = n;
    contents(engine.tmeta)[tf(e, 3)] = h;
    contents(engine.tmeta)[tf(e, 4)] = host_len;
    contents(engine.tmeta)[tf(e, 5)] = contents(engine.meta)[m_started(engine, slot)];
    contents(engine.tmeta)[tf(e, 6)] = contents(engine.ints)[i + tls_slot.i_ticket_lifetime()];
    contents(engine.tmeta)[tf(e, 7)] = contents(engine.ints)[i + tls_slot.i_ticket_age_add()];
    contents(engine.tmeta)[tf(e, 8)] = contents(engine.ints)[i + tls_slot.i_verified_at()];
    contents(engine.tmeta)[tf(e, 9)] = contents(engine.ints)[i + tls_slot.i_not_after()];
    contents(engine.tmeta)[tf(e, 10)] = contents(engine.tmeta)[t_trust()];
    // Saved once: the slot's copy goes.
    contents(engine.ints)[i + tls_slot.i_ticket_len()] = 0;
    tls_slot.zero(contents(engine.bytes)[b + tls_slot.b_ticket()..b + tls_slot.b_ticket_host() + 256]);
    return gen * 65536 + e + 1;
}

// Forgets a saved ticket, overwriting its secret. A handle already used,
// forgotten or replaced is ignored.
pub fn forget[&e](engine: &!e Engine, handle: int) -> [] int {
    let e = entry_of(engine, handle);
    if e >= 0 {
        wipe(engine, e);
    }
    return 0;
}

// Whether entry `e` may be offered for `host` at `now_ms`
// (`docs/tls-resumption.md` §3, rules 1 to 4).
fn may_offer[&e, &h](engine: &e Engine, e: int, host: &h [byte], now_ms: int) -> [] bool {
    let at = e * entry_bytes();
    if contents(engine.tmeta)[tf(e, 4)] != len(host) || contents(engine.tmeta)[tf(e, 10)] != contents(engine.tmeta)[t_trust()] {
        return false;
    }
    var same = true;
    var k = 0;
    while k < len(host) {
        if contents(engine.tickets)[at + e_host() + k] != host[k] {
            same = false;
        }
        k = k + 1;
    }
    let received = contents(engine.tmeta)[tf(e, 5)];
    let now_s = now_ms / 1000;
    return same && now_s <= contents(engine.tmeta)[tf(e, 9)] && now_s < contents(engine.tmeta)[tf(e, 8)] + contents(engine.tmeta)[t_max_age()] && now_ms < received + contents(engine.tmeta)[tf(e, 6)] * 1000 && now_ms >= received;
}

// `start`, offering the ticket `handle` names if the rules of
// `docs/tls-resumption.md` §3 allow it for `host` now; otherwise a full
// handshake, as `start`. The ticket is used once: after this, `handle`
// names nothing. `resumed` says, once established, whether the server
// took it; a server that does not is a full handshake, verified.
pub fn start_with[&e, &h](engine: &!e Engine, slot: int, host: &h [byte], now_unix_ms: int, handle: int) -> [] int {
    let e = entry_of(engine, handle);
    if e < 0 {
        return start(engine, slot, host, now_unix_ms);
    }
    if !may_offer(engine, e, host, now_unix_ms) {
        wipe(engine, e);
        return start(engine, slot, host, now_unix_ms);
    }
    if !slot_ok(engine, slot) || contents(engine.meta)[m_busy(slot)] != 0 {
        return tls_record.bad_slot();
    }
    if contents(engine.meta)[m_seeded()] != 1 {
        return tls_record.no_entropy();
    }
    let at = e * entry_bytes();
    let n = contents(engine.tmeta)[tf(e, 2)];
    let h = contents(engine.tmeta)[tf(e, 3)];
    // RFC 8446 §4.2.11.1: milliseconds since the ticket was received,
    // plus ticket_age_add.
    let age = now_unix_ms - contents(engine.tmeta)[tf(e, 5)] + contents(engine.tmeta)[tf(e, 7)];
    let verified_at = contents(engine.tmeta)[tf(e, 8)];
    let not_after = contents(engine.tmeta)[tf(e, 9)];
    var code = 0;
    region r {
        let random = alloc_slice[r](96, byte_of(0));
        draw(engine, random);
        let i = ints_of(slot);
        let b = bytes_of(slot);
        code = tls_client.start_psk(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()], host, random, now_unix_ms / 1000, contents(engine.tickets)[at..at + n], contents(engine.tickets)[at + e_psk()..at + e_psk() + h], age, verified_at, not_after, true);
        tls_slot.zero(random);
    }
    wipe(engine, e);
    contents(engine.meta)[m_started(engine, slot)] = now_unix_ms;
    contents(engine.meta)[m_busy(slot)] = 1;
    return code;
}

// Whether the connection in `slot` resumed a session.
pub fn resumed[&e](engine: &e Engine, slot: int) -> [] bool {
    if !slot_ok(engine, slot) {
        return false;
    }
    let i = ints_of(slot);
    return tls_client.resumed(contents(engine.ints)[i..i + tls_client.ints_len()]);
}
