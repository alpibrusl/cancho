edition 5;

module tls;

import std.chacha20;
import tls_client;
import tls_record;
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
}

// The roots' room, in `x509_verify.store_load`'s format: this machine's
// 128-root bundle takes 138,350 bytes (`docs/x509-verify.md` §8.2).
fn roots_cap() -> [] int {
    return 1048576;
}

// meta: [0] slots, [1] seeded, [2] bytes of roots, [3] blocks skipped, [4 + s] slot `s` in use.
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

pub fn open[&h](heap: &!h Heap, slots: int) -> [heap] Engine {
    var n = slots;
    if n < 1 {
        n = 1;
    }
    var engine = Engine { ints: box_slice(heap, n * tls_client.ints_len(), 0), bytes: box_slice(heap, n * tls_client.bytes_len(), byte_of(0)), roots: box_slice(heap, roots_cap(), byte_of(0)), meta: box_slice(heap, 4 + n, 0), drbg: box_slice(heap, 32, byte_of(0)) };
    borrow mut engine as &!w in {
        contents(w.meta)[m_slots()] = n;
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
    let Engine { ints, bytes, roots, meta, drbg } = e;
    unbox_slice(heap, ints);
    unbox_slice(heap, bytes);
    unbox_slice(heap, roots);
    unbox_slice(heap, meta);
    unbox_slice(heap, drbg);
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
        code = tls_client.start(contents(engine.ints)[i..i + tls_client.ints_len()], contents(engine.bytes)[b..b + tls_client.bytes_len()], host, random, now_unix_ms / 1000);
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
