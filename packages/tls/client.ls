module tls_client;
import std.crypto;
import std.ecdh;
import std.ecdsa;
import std.ed25519;
import std.hkdf;
import std.hmac;
import std.rsa;
import std.x25519;
import tls_message;
import tls_record;
import x509;
import x509_verify;

// `tls_client` -- one TLS 1.3 client connection: the state machine, the
// transcript and key schedule, certificate verification and signature checks,
// alerts, and application data (`docs/tls-core.md` §3 to §5). A
// connection is two caller-owned slices, `ints` (`ints_len()` words) and
// `bytes` (`bytes_len()` bytes), so an engine can keep many in two boxes
// (`docs/tls-pure.md` §2.1). Nothing is allocated from a size the peer
// names. Not independently reviewed (#209).

// ---- States (`docs/tls-core.md` §4) ----

pub fn state_idle() -> [] int {
    return 0;
}

pub fn state_wait_server_hello() -> [] int {
    return 1;
}

pub fn state_wait_extensions() -> [] int {
    return 2;
}

pub fn state_wait_certificate() -> [] int {
    return 3;
}

pub fn state_wait_verify() -> [] int {
    return 4;
}

pub fn state_wait_finished() -> [] int {
    return 5;
}

pub fn state_connected() -> [] int {
    return 6;
}

pub fn state_closed() -> [] int {
    return 7;
}

pub fn state_failed() -> [] int {
    return 8;
}

// `event`'s answers (`docs/tls-pure.md` §2.2).
pub fn event_want_read() -> [] int {
    return 1;
}

pub fn event_want_write() -> [] int {
    return 2;
}

pub fn event_established() -> [] int {
    return 3;
}

pub fn event_closed() -> [] int {
    return 4;
}

pub fn event_failed() -> [] int {
    return 5;
}

// ---- The int slice ----

fn i_state() -> [] int {
    return 0;
}

fn i_failure() -> [] int {
    return 1;
}

fn i_read_seq() -> [] int {
    return 2;
}

fn i_write_seq() -> [] int {
    return 3;
}

fn i_in_fill() -> [] int {
    return 4;
}

fn i_hs_fill() -> [] int {
    return 5;
}

fn i_out_start() -> [] int {
    return 6;
}

fn i_out_end() -> [] int {
    return 7;
}

fn i_recv_start() -> [] int {
    return 8;
}

fn i_recv_end() -> [] int {
    return 9;
}

fn i_flags() -> [] int {
    return 10;
}

fn i_key_updates() -> [] int {
    return 11;
}

fn i_warnings() -> [] int {
    return 12;
}

fn i_alert() -> [] int {
    return 13;
}

fn i_leaf_len() -> [] int {
    return 14;
}

fn i_host_len() -> [] int {
    return 15;
}

fn i_context_len() -> [] int {
    return 16;
}

// The time `start` was given, in seconds since 1970, for the
// certificates' validity.
fn i_now() -> [] int {
    return 17;
}

// The negotiated suite (0 until ServerHello or a HelloRetryRequest
// names one), and the group of the share the client sent last.
fn i_suite() -> [] int {
    return 18;
}

fn i_group() -> [] int {
    return 19;
}

// Two running transcripts, SHA-256 and SHA-384, both fed until the suite
// is known (RFC 8446 §4.4.1 lets a client keep both).
fn i_transcript() -> [] int {
    return 20;
}

fn i_transcript384() -> [] int {
    return i_transcript() + crypto.sha256_state_len();
}

// `std.ecdh`'s work, for a P-256 or P-384 share a HelloRetryRequest asks
// for: more than a region holds, so it is part of the slot.
fn i_ecdh_work() -> [] int {
    return i_transcript384() + crypto.sha512_state_len();
}

pub fn ints_len() -> [] int {
    return i_ecdh_work() + ecdh.work_len();
}

// Flags.
fn f_ccs_seen() -> [] int {
    return 1;
}

fn f_cert_requested() -> [] int {
    return 2;
}

fn f_close_received() -> [] int {
    return 4;
}

fn f_close_sent() -> [] int {
    return 8;
}

fn f_read_protected() -> [] int {
    return 16;
}

fn f_write_protected() -> [] int {
    return 32;
}

// A HelloRetryRequest came: a second one is refused.
fn f_retried() -> [] int {
    return 64;
}

// ---- The byte slice ----

// One incoming record, header included.
fn b_in() -> [] int {
    return 0;
}

fn in_cap() -> [] int {
    return 5 + tls_record.max_ciphertext();
}

// Handshake reassembly (`docs/tls-pure.md` §7.1: a message is at most 64 KiB).
fn b_hs() -> [] int {
    return b_in() + in_cap();
}

fn hs_cap() -> [] int {
    return 65536;
}

// Bytes waiting for `take`: room for three full records.
fn b_out() -> [] int {
    return b_hs() + hs_cap();
}

fn out_cap() -> [] int {
    return 3 * (tls_record.max_plaintext() + 22);
}

// One opened record's plaintext.
fn b_plain() -> [] int {
    return b_out() + out_cap();
}

fn plain_cap() -> [] int {
    return tls_record.max_ciphertext();
}

// Application data waiting for `recv`.
fn b_recv() -> [] int {
    return b_plain() + plain_cap();
}

fn recv_cap() -> [] int {
    return tls_record.max_ciphertext();
}

// The server's leaf certificate, kept from Certificate to CertificateVerify.
fn b_leaf() -> [] int {
    return b_recv() + recv_cap();
}

fn leaf_cap() -> [] int {
    return 16384;
}

fn b_keys() -> [] int {
    return b_leaf() + leaf_cap();
}

fn k_x25519() -> [] int {
    return b_keys();
}

// The P-256 or P-384 scalar, when a HelloRetryRequest asks for one.
fn k_ecdh() -> [] int {
    return b_keys() + 32;
}

// Keys of up to 32 bytes; secrets of up to 48 (SHA-384's).
fn k_read_key() -> [] int {
    return b_keys() + 80;
}

fn k_read_iv() -> [] int {
    return b_keys() + 112;
}

fn k_write_key() -> [] int {
    return b_keys() + 124;
}

fn k_write_iv() -> [] int {
    return b_keys() + 156;
}

fn k_client_hs() -> [] int {
    return b_keys() + 168;
}

fn k_server_hs() -> [] int {
    return b_keys() + 216;
}

fn k_client_ap() -> [] int {
    return b_keys() + 264;
}

fn k_server_ap() -> [] int {
    return b_keys() + 312;
}

fn k_master() -> [] int {
    return b_keys() + 360;
}

fn k_session_id() -> [] int {
    return b_keys() + 408;
}

fn k_random() -> [] int {
    return b_keys() + 440;
}

fn k_host() -> [] int {
    return b_keys() + 472;
}

// A CertificateRequest's context, echoed in the client's Certificate.
fn k_context() -> [] int {
    return b_keys() + 728;
}

fn keys_len() -> [] int {
    return 984;
}

pub fn bytes_len() -> [] int {
    return b_keys() + keys_len();
}

// ---- Helpers ----

fn copy_bytes[&s, &o](src: &s [byte], out: &!o [byte]) -> [] int {
    var i = 0;
    while i < len(src) {
        out[i] = src[i];
        i = i + 1;
    }
    return len(src);
}

fn zero[&o](out: &!o [byte]) -> [] int {
    var i = 0;
    while i < len(out) {
        out[i] = byte_of(0);
        i = i + 1;
    }
    return 0;
}

fn has[&i](ints: &i [int], flag: int) -> [] bool {
    return ints[i_flags()] & flag != 0;
}

fn set_flag[&i](ints: &!i [int], flag: int) -> [] int {
    ints[i_flags()] = ints[i_flags()] | flag;
    return 0;
}

// The suite's hash length, 32 or 48 (32 before a suite is named).
fn hash_len[&i](ints: &i [int]) -> [] int {
    return tls_record.hash_len(ints[i_suite()]);
}

// The suite's AEAD key length, 16 or 32.
fn key_len[&i](ints: &i [int]) -> [] int {
    return tls_record.key_len(ints[i_suite()]);
}

// The transcript hash so far under the hash `len(out)` names (32 bytes
// SHA-256, 48 SHA-384), leaving the running states as they were.
fn transcript_hash[&i, &o](ints: &i [int], out: &!o [byte]) -> [] int {
    region r {
        if len(out) == 48 {
            let copy = alloc_slice[r](crypto.sha512_state_len(), 0);
            var k = 0;
            while k < len(copy) {
                copy[k] = ints[i_transcript384() + k];
                k = k + 1;
            }
            crypto.sha384_final(copy, out);
        } else {
            let copy = alloc_slice[r](crypto.sha256_state_len(), 0);
            var k = 0;
            while k < len(copy) {
                copy[k] = ints[i_transcript() + k];
                k = k + 1;
            }
            crypto.sha256_final(copy, out);
        }
    }
    return 0;
}

fn transcript_add[&i, &m](ints: &!i [int], message: &m [byte]) -> [] int {
    crypto.sha384_update(ints[i_transcript384()..i_transcript384() + crypto.sha512_state_len()], message);
    return crypto.sha256_update(ints[i_transcript()..i_transcript() + crypto.sha256_state_len()], message);
}

fn transcript_init[&i](ints: &!i [int]) -> [] int {
    crypto.sha384_init(ints[i_transcript384()..i_transcript384() + crypto.sha512_state_len()]);
    return crypto.sha256_init(ints[i_transcript()..i_transcript() + crypto.sha256_state_len()]);
}

// The traffic key and IV of `secret` (the suite's hash length) into
// `key` (the suite's key length) and `iv` (12).
fn traffic_keys[&s, &k, &v](secret: &s [byte], key: &!k [byte], iv: &!v [byte]) -> [] int {
    hkdf.expand_label(len(secret), secret, "key", "", key);
    hkdf.expand_label(len(secret), secret, "iv", "", iv);
    return 0;
}

fn set_read_keys[&i, &b](ints: &!i [int], bytes: &!b [byte], secret_at: int) -> [] int {
    let h = hash_len(ints);
    traffic_keys(bytes[secret_at..secret_at + h], bytes[k_read_key()..k_read_key() + key_len(ints)], bytes[k_read_iv()..k_read_iv() + 12]);
    ints[i_read_seq()] = 0;
    set_flag(ints, f_read_protected());
    return 0;
}

fn set_write_keys[&i, &b](ints: &!i [int], bytes: &!b [byte], secret_at: int) -> [] int {
    let h = hash_len(ints);
    traffic_keys(bytes[secret_at..secret_at + h], bytes[k_write_key()..k_write_key() + key_len(ints)], bytes[k_write_iv()..k_write_iv() + 12]);
    ints[i_write_seq()] = 0;
    set_flag(ints, f_write_protected());
    return 0;
}

// ---- Output ----

fn out_free[&i](ints: &i [int]) -> [] int {
    return out_cap() - ints[i_out_end()];
}

// Moves what is waiting to the front of the queue, so the room after
// it is contiguous.
fn compact_out[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    let s = ints[i_out_start()];
    let e = ints[i_out_end()];
    if s > 0 {
        var k = 0;
        while k < e - s {
            bytes[b_out() + k] = bytes[b_out() + s + k];
            k = k + 1;
        }
        ints[i_out_start()] = 0;
        ints[i_out_end()] = e - s;
    }
    return 0;
}

// Queues `content` of `kind` as one record: protected when write keys
// are set, plaintext before. 0, or a refusal when it does not fit.
fn queue_record[&i, &b, &c](ints: &!i [int], bytes: &!b [byte], kind: int, content: &c [byte]) -> [] int {
    compact_out(ints, bytes);
    let at = b_out() + ints[i_out_end()];
    if !has(ints, f_write_protected()) {
        if out_free(ints) < 5 + len(content) {
            return tls_record.record_overflow();
        }
        bytes[at] = byte_of(kind);
        bytes[at + 1] = byte_of(3);
        bytes[at + 2] = byte_of(3);
        bytes[at + 3] = byte_of(len(content) >> 8);
        bytes[at + 4] = byte_of(len(content) & 255);
        var k = 0;
        while k < len(content) {
            bytes[at + 5 + k] = content[k];
            k = k + 1;
        }
        ints[i_out_end()] = ints[i_out_end()] + 5 + len(content);
        return 0;
    }
    if out_free(ints) < len(content) + 22 {
        return tls_record.record_overflow();
    }
    let n = tls_record.seal(ints[i_suite()], bytes[k_write_key()..k_write_key() + key_len(ints)], bytes[k_write_iv()..k_write_iv() + 12], ints[i_write_seq()], kind, content, bytes[at..at + len(content) + 22]);
    if n < 0 {
        return n;
    }
    ints[i_write_seq()] = ints[i_write_seq()] + 1;
    ints[i_out_end()] = ints[i_out_end()] + n;
    return 0;
}

// The alert a refusal sends (RFC 8446 §6.2).
fn alert_for(code: int) -> [] int {
    if code == tls_record.unexpected_message() {
        return 10;
    }
    if code == tls_record.bad_record_mac() {
        return 20;
    }
    if code == tls_record.record_overflow() {
        return 22;
    }
    if code == tls_record.no_shared_cipher() {
        return 40;
    }
    if code == tls_record.key_share() || code == tls_record.hello_retry() {
        return 47;
    }
    if code == tls_record.decode_error() {
        return 50;
    }
    if code == tls_record.bad_certificate_verify() || code == tls_record.bad_finished() {
        return 51;
    }
    if code == tls_record.protocol_version() {
        return 70;
    }
    if code == tls_record.unsupported_extension() {
        return 110;
    }
    if code == tls_record.x509_unknown_issuer() {
        return 48;
    }
    if code == tls_record.x509_expired() || code == tls_record.x509_not_yet_valid() {
        return 45;
    }
    if code == tls_record.x509_key_usage() || code == tls_record.x509_critical_extension() {
        return 43;
    }
    if code == tls_record.x509_bad_signature() || code == tls_record.x509_name_mismatch() || code == tls_record.x509_not_ca() || code == tls_record.x509_path_too_long() || code == tls_record.x509_name_constraint() || code == tls_record.x509_chain_too_large() {
        return 42;
    }
    if code == tls_record.x509_decode() {
        return 42;
    }
    if code == tls_record.x509_unsupported_algorithm() || code == tls_record.x509_key_size() {
        return 43;
    }
    return 80;
}

// Ends the connection with `code`: the failure is kept, and a fatal
// alert is queued unless the peer's own alert ended it.
fn fail[&i, &b](ints: &!i [int], bytes: &!b [byte], code: int) -> [] int {
    if ints[i_state()] == state_failed() {
        return ints[i_failure()];
    }
    ints[i_state()] = state_failed();
    ints[i_failure()] = code;
    if code != tls_record.alert_received() && code != tls_record.peer_closed() {
        region r {
            let alert = alloc_slice[r](2, byte_of(2));
            alert[1] = byte_of(alert_for(code));
            queue_record(ints, bytes, tls_record.type_alert(), alert);
        }
    }
    forget(bytes);
    return code;
}

// No key is used again: after a failure's alert is sealed, and once
// close_notify has gone both ways. The secrets, keys and IVs, and the
// last opened record, are overwritten (best effort, `docs/tls-core.md`
// §8); received data waiting for `recv` stays.
fn forget[&b](bytes: &!b [byte]) -> [] int {
    zero(bytes[b_keys()..b_keys() + keys_len()]);
    zero(bytes[b_plain()..b_plain() + plain_cap()]);
    return 0;
}

// ---- Starting ----

// Starts the handshake: `host` (at most 255 bytes) is the server's name;
// `random` is 96 bytes of the caller's entropy: the ClientHello random,
// the legacy session id and the X25519 secret. `now` is the time in
// seconds since 1970, against which the server's certificates are
// checked. The ClientHello is queued for `take`.
pub fn start[&i, &b, &h, &r](ints: &!i [int], bytes: &!b [byte], host: &h [byte], random: &r [byte], now: int) -> [] int {
    if len(ints) < ints_len() || len(bytes) < bytes_len() || len(random) != 96 || len(host) > 255 {
        return tls_record.bad_slot();
    }
    var k = 0;
    while k < ints_len() {
        ints[k] = 0;
        k = k + 1;
    }
    transcript_init(ints);
    copy_bytes(random[0..32], bytes[k_random()..k_random() + 32]);
    copy_bytes(random[32..64], bytes[k_session_id()..k_session_id() + 32]);
    copy_bytes(random[64..96], bytes[k_x25519()..k_x25519() + 32]);
    copy_bytes(host, bytes[k_host()..k_host() + len(host)]);
    ints[i_host_len()] = len(host);
    ints[i_now()] = now;
    ints[i_group()] = tls_message.group_x25519();
    var code = 0;
    region r {
        let share = alloc_slice[r](32, byte_of(0));
        x25519.public_key(bytes[k_x25519()..k_x25519() + 32], share);
        code = send_client_hello(ints, bytes, share, share[0..0]);
    }
    ints[i_state()] = state_wait_server_hello();
    return code;
}

// A ClientHello with one share of `ints[i_group()]` and `cookie`, added
// to the transcript and queued.
fn send_client_hello[&i, &b, &s, &c](ints: &!i [int], bytes: &!b [byte], share: &s [byte], cookie: &c [byte]) -> [] int {
    var code = 0;
    region r {
        let hello = alloc_slice[r](tls_message.max_client_hello(), byte_of(0));
        let n = tls_message.client_hello(bytes[k_random()..k_random() + 32], bytes[k_session_id()..k_session_id() + 32], ints[i_group()], share, cookie, bytes[k_host()..k_host() + ints[i_host_len()]], hello);
        transcript_add(ints, hello[0..n]);
        code = queue_record(ints, bytes, tls_record.type_handshake(), hello[0..n]);
    }
    return code;
}

// ---- The handshake ----

// The curve `std.ecdh` names a group by: 256, 384, or 0 for X25519.
fn curve_of(group: int) -> [] int {
    if group == tls_message.group_p256() {
        return 256;
    }
    if group == tls_message.group_p384() {
        return 384;
    }
    return 0;
}

// A P-256 or P-384 scalar and its public point, `share`, for the group
// a HelloRetryRequest named. The scalar is drawn from the X25519 secret,
// which this connection never otherwise uses once a retry has come:
// HKDF-Expand-Label(x25519 secret, "ecdh scalar", [attempt]), retried
// while it is not below n (`docs/tls-parity.md` §3.3). One attempt in
// 2^32 fails for P-256, so sixteen in a row never do.
fn new_ecdh_share[&i, &b, &s](ints: &!i [int], bytes: &!b [byte], curve: int, share: &!s [byte]) -> [] int {
    let size = curve / 8;
    var code = ecdh.refused_scalar_range();
    var attempt = 0;
    region r {
        let context = alloc_slice[r](1, byte_of(0));
        while code == ecdh.refused_scalar_range() && attempt < 16 {
            context[0] = byte_of(attempt);
            hkdf.expand_label(32, bytes[k_x25519()..k_x25519() + 32], "ecdh scalar", context, bytes[k_ecdh()..k_ecdh() + size]);
            code = ecdh.public_key(curve, bytes[k_ecdh()..k_ecdh() + size], share, ints[i_ecdh_work()..i_ecdh_work() + ecdh.work_len()]);
            attempt = attempt + 1;
        }
    }
    if code != 0 {
        return tls_record.no_entropy();
    }
    return 0;
}

// ServerHello, or a HelloRetryRequest: the shared secret, then the
// handshake secrets and keys (RFC 8446 §7.1).
fn on_server_hello[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    var code = 0;
    region r {
        let info = alloc_slice[r](tls_message.sh_info_len(), 0);
        code = tls_message.server_hello(message[4..len(message)], bytes[k_session_id()..k_session_id() + 32], info);
        let suite = info[tls_message.sh_suite()];
        let group = info[tls_message.sh_group()];
        if code == 0 && has(ints, f_retried()) {
            // After a retry: no second one, and the suite it named.
            if info[tls_message.sh_retry()] == 1 {
                code = tls_record.unexpected_message();
            } else if suite != ints[i_suite()] {
                code = tls_record.hello_retry();
            }
        }
        if code == 0 && info[tls_message.sh_retry()] == 1 {
            code = retry(ints, bytes, message, suite, group, info[tls_message.sh_cookie_start()], info[tls_message.sh_cookie_end()]);
        } else if code == 0 {
            if group != ints[i_group()] {
                // A share for a group the client has no share of.
                code = tls_record.key_share();
            }
            if code == 0 {
                ints[i_suite()] = suite;
                transcript_add(ints, message);
                code = handshake_secrets(ints, bytes, message[4 + info[tls_message.sh_share()]..4 + info[tls_message.sh_share()] + tls_message.share_len(group)]);
            }
            if code == 0 {
                ints[i_state()] = state_wait_extensions();
            }
        }
    }
    return code;
}

// The HelloRetryRequest `message`, already parsed: `suite` and `group`
// (0 for none), and the cookie's range in its body (0, 0 for none).
fn retry[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte], suite: int, group: int, cookie_start: int, cookie_end: int) -> [] int {
    set_flag(ints, f_retried());
    ints[i_suite()] = suite;
    let h = hash_len(ints);
    var code = 0;
    region r {
        // message_hash: type 254, the hash's length, Hash(ClientHello1).
        let synthetic = alloc_slice[r](4 + h, byte_of(0));
        synthetic[0] = byte_of(254);
        synthetic[3] = byte_of(h);
        transcript_hash(ints, synthetic[4..4 + h]);
        transcript_init(ints);
        transcript_add(ints, synthetic);
        transcript_add(ints, message);
        let cookie = message[4 + cookie_start..4 + cookie_end];
        if group == 0 {
            // Only a cookie: the same X25519 share again.
            let share = alloc_slice[r](32, byte_of(0));
            x25519.public_key(bytes[k_x25519()..k_x25519() + 32], share);
            code = send_client_hello(ints, bytes, share, cookie);
        } else {
            ints[i_group()] = group;
            let share = alloc_slice[r](tls_message.share_len(group), byte_of(0));
            code = new_ecdh_share(ints, bytes, curve_of(group), share);
            if code == 0 {
                code = send_client_hello(ints, bytes, share, cookie);
            }
        }
    }
    return code;
}

// The shared secret from the server's `share`, then the handshake
// secrets, the master secret and the read keys (RFC 8446 §7.1).
fn handshake_secrets[&i, &b, &s](ints: &!i [int], bytes: &!b [byte], share: &s [byte]) -> [] int {
    let h = hash_len(ints);
    let curve = curve_of(ints[i_group()]);
    var code = 0;
    region r {
        var size = 32;
        if curve != 0 {
            size = curve / 8;
        }
        let secret = alloc_slice[r](size, byte_of(0));
        if curve == 0 {
            if x25519.scalarmult(bytes[k_x25519()..k_x25519() + 32], share, secret) != 0 {
                code = tls_record.key_share();
            }
        } else if ecdh.shared(curve, bytes[k_ecdh()..k_ecdh() + size], share, secret, ints[i_ecdh_work()..i_ecdh_work() + ecdh.work_len()]) != 0 {
            code = tls_record.key_share();
        }
        if code == 0 {
            let zeros = alloc_slice[r](h, byte_of(0));
            let early = alloc_slice[r](h, byte_of(0));
            let derived = alloc_slice[r](h, byte_of(0));
            let empty_hash = alloc_slice[r](h, byte_of(0));
            let hs = alloc_slice[r](h, byte_of(0));
            let th = alloc_slice[r](h, byte_of(0));
            if h == 48 {
                crypto.sha384(zeros[0..0], empty_hash);
            } else {
                crypto.sha256(zeros[0..0], empty_hash);
            }
            hkdf.extract(h, zeros, zeros, early);
            hkdf.derive_secret(h, early, "derived", empty_hash, derived);
            hkdf.extract(h, derived, secret, hs);
            transcript_hash(ints, th);
            hkdf.derive_secret(h, hs, "c hs traffic", th, bytes[k_client_hs()..k_client_hs() + h]);
            hkdf.derive_secret(h, hs, "s hs traffic", th, bytes[k_server_hs()..k_server_hs() + h]);
            hkdf.derive_secret(h, hs, "derived", empty_hash, derived);
            hkdf.extract(h, derived, zeros, bytes[k_master()..k_master() + h]);
            set_read_keys(ints, bytes, k_server_hs());
            zero(secret);
            zero(hs);
            zero(early);
            zero(derived);
        }
    }
    // The key exchange's secrets have done their work.
    zero(bytes[k_x25519()..k_x25519() + 80]);
    return code;
}

// A refusal of `x509_verify` or `x509` as this package's code
// (`docs/tls-pure.md` §8): a certificate that is not well-formed is
// `x509-decode`, whatever the detail.
fn from_x509(code: int) -> [] int {
    if code == 0 {
        return 0;
    }
    if code == x509_verify.unknown_issuer() {
        return tls_record.x509_unknown_issuer();
    }
    if code == x509_verify.expired() {
        return tls_record.x509_expired();
    }
    if code == x509_verify.not_yet_valid() {
        return tls_record.x509_not_yet_valid();
    }
    if code == x509_verify.bad_signature() {
        return tls_record.x509_bad_signature();
    }
    if code == x509_verify.name_mismatch() {
        return tls_record.x509_name_mismatch();
    }
    if code == x509_verify.not_ca() {
        return tls_record.x509_not_ca();
    }
    if code == x509_verify.path_too_long() {
        return tls_record.x509_path_too_long();
    }
    if code == x509_verify.name_constraint() {
        return tls_record.x509_name_constraint();
    }
    if code == x509_verify.key_usage() {
        return tls_record.x509_key_usage();
    }
    if code == x509_verify.unsupported_algorithm() {
        return tls_record.x509_unsupported_algorithm();
    }
    if code == x509_verify.key_size() {
        return tls_record.x509_key_size();
    }
    if code == -15 {
        return tls_record.x509_critical_extension();
    }
    if code == -16 {
        return tls_record.x509_chain_too_large();
    }
    return tls_record.x509_decode();
}

// Certificate: the chain against the store, the host and the time
// (`docs/x509-verify.md`); the leaf is kept for CertificateVerify.
fn on_certificate[&i, &b, &m, &p](ints: &!i [int], bytes: &!b [byte], message: &m [byte], store: &p [byte]) -> [] int {
    var code = 0;
    region r {
        let info = alloc_slice[r](3 + 2 * tls_message.max_certificates(), 0);
        let body = message[4..len(message)];
        code = tls_message.certificate(body, info);
        if code == 0 {
            let leaf = body[info[0]..info[1]];
            let n = info[2];
            let ranges = alloc_slice[r](2 * n, 0);
            var k = 0;
            while k < 2 * n {
                ranges[k] = info[3 + k];
                k = k + 1;
            }
            if len(leaf) > leaf_cap() {
                code = tls_record.x509_chain_too_large();
            } else {
                code = from_x509(x509_verify.verify(store, body, ranges, bytes[k_host()..k_host() + ints[i_host_len()]], ints[i_now()], x509_verify.tls_max_intermediates()));
            }
            if code == 0 {
                ints[i_leaf_len()] = copy_bytes(leaf, bytes[b_leaf()..b_leaf() + len(leaf)]);
                transcript_add(ints, message);
            }
        }
    }
    if code == 0 {
        ints[i_state()] = state_wait_verify();
    }
    return code;
}

// The signature `sig` by the leaf's key, under `scheme`, over
// `content` (`docs/tls-pure.md` §3.1).
fn check_signature[&d, &c, &s](der: &d [byte], scheme: int, content: &c [byte], sig: &s [byte]) -> [] int {
    var code = 0;
    region r {
        let view = alloc_slice[r](x509.view_len(), 0);
        if x509.parse(der, view) != 0 {
            code = tls_record.x509_decode();
        }
        let alg = view[x509.key_algorithm()];
        let key = der[view[x509.key_start()]..view[x509.key_end()]];
        if code == 0 && alg == x509.oid_rsa_encryption() {
            var h = 0;
            if scheme == tls_message.rsa_pss_sha256() {
                h = 32;
            } else if scheme == tls_message.rsa_pss_sha384() {
                h = 48;
            } else if scheme == tls_message.rsa_pss_sha512() {
                h = 64;
            }
            if h == 0 {
                code = tls_record.bad_certificate_verify();
            } else {
                let digest = alloc_slice[r](h, byte_of(0));
                if h == 32 {
                    crypto.sha256(content, digest);
                } else if h == 48 {
                    crypto.sha384(content, digest);
                } else {
                    crypto.sha512(content, digest);
                }
                let work = alloc_slice[r](rsa.work_len(), 0);
                let n = der[view[x509.rsa_modulus_start()]..view[x509.rsa_modulus_end()]];
                let e = der[view[x509.rsa_exponent_start()]..view[x509.rsa_exponent_end()]];
                let v = rsa.pss_verify(h, h, h, n, e, digest, sig, work);
                if v == rsa.refused_modulus_size() || v == rsa.refused_even_modulus() {
                    code = tls_record.x509_key_size();
                } else if v != 0 {
                    code = tls_record.bad_certificate_verify();
                }
            }
        } else if code == 0 && alg == x509.oid_ec_public_key() {
            let curve_code = view[x509.key_curve()];
            var curve = 0;
            if curve_code == x509.oid_p256() && scheme == tls_message.ecdsa_p256_sha256() {
                curve = 256;
            } else if curve_code == x509.oid_p384() && scheme == tls_message.ecdsa_p384_sha384() {
                curve = 384;
            }
            if curve_code != x509.oid_p256() && curve_code != x509.oid_p384() {
                code = tls_record.x509_unsupported_algorithm();
            } else if curve == 0 {
                code = tls_record.bad_certificate_verify();
            } else {
                let digest = alloc_slice[r](curve / 8, byte_of(0));
                if curve == 256 {
                    crypto.sha256(content, digest);
                } else {
                    crypto.sha384(content, digest);
                }
                let work = alloc_slice[r](ecdsa.work_len(), 0);
                if ecdsa.verify_der(curve, digest, key, sig, work) != 0 {
                    code = tls_record.bad_certificate_verify();
                }
            }
        } else if code == 0 && alg == x509.oid_ed25519() {
            if scheme != tls_message.ed25519() || ed25519.verify(key, content, sig) != 1 {
                code = tls_record.bad_certificate_verify();
            }
        } else if code == 0 {
            code = tls_record.x509_unsupported_algorithm();
        }
    }
    return code;
}

// CertificateVerify: the server's signature over the transcript so far
// (RFC 8446 §4.4.3).
fn on_certificate_verify[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    var code = 0;
    region r {
        let info = alloc_slice[r](3, 0);
        code = tls_message.certificate_verify(message[4..len(message)], info);
        if code == 0 {
            // 64 spaces, the context string, a zero byte, the hash.
            let h = hash_len(ints);
            let content = alloc_slice[r](64 + 33 + 1 + h, byte_of(32));
            let label = "TLS 1.3, server CertificateVerify";
            copy_bytes(label, content[64..97]);
            content[97] = byte_of(0);
            transcript_hash(ints, content[98..98 + h]);
            code = check_signature(bytes[b_leaf()..b_leaf() + ints[i_leaf_len()]], info[0], content, message[4 + info[1]..4 + info[2]]);
        }
        if code == 0 {
            transcript_add(ints, message);
        }
    }
    if code == 0 {
        ints[i_state()] = state_wait_finished();
    }
    return code;
}

// HMAC(finished_key(secret), transcript hash) into `out`, under the
// suite's hash (`len(secret)` and `len(out)` bytes).
fn finished_mac[&i, &s, &o](ints: &i [int], secret: &s [byte], out: &!o [byte]) -> [] int {
    let h = len(secret);
    region r {
        let key = alloc_slice[r](h, byte_of(0));
        let th = alloc_slice[r](h, byte_of(0));
        hkdf.expand_label(h, secret, "finished", "", key);
        transcript_hash(ints, th);
        hmac.mac(h, key, th, out);
        zero(key);
    }
    return 0;
}

// The server's Finished, then the client's flight: a change_cipher_spec
// for middleboxes (RFC 8446 Appendix D.4), an empty Certificate if one
// was requested, and Finished; then the application keys.
fn on_finished[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    let h = hash_len(ints);
    var code = tls_message.finished(message[4..len(message)], h);
    region r {
        let want = alloc_slice[r](h, byte_of(0));
        if code == 0 {
            finished_mac(ints, bytes[k_server_hs()..k_server_hs() + h], want);
            // Every byte is compared, whatever the earlier ones were.
            var diff = 0;
            var k = 0;
            while k < h {
                diff = diff | int_of(want[k]) ^ int_of(message[4 + k]);
                k = k + 1;
            }
            if diff != 0 {
                code = tls_record.bad_finished();
            }
        }
        if code == 0 {
            transcript_add(ints, message);
            let th = alloc_slice[r](h, byte_of(0));
            transcript_hash(ints, th);
            hkdf.derive_secret(h, bytes[k_master()..k_master() + h], "c ap traffic", th, bytes[k_client_ap()..k_client_ap() + h]);
            hkdf.derive_secret(h, bytes[k_master()..k_master() + h], "s ap traffic", th, bytes[k_server_ap()..k_server_ap() + h]);
            let ccs = alloc_slice[r](1, byte_of(1));
            code = queue_record(ints, bytes, tls_record.type_change_cipher_spec(), ccs);
            set_write_keys(ints, bytes, k_client_hs());
            if code == 0 && has(ints, f_cert_requested()) {
                let cn = ints[i_context_len()];
                let cert = alloc_slice[r](4 + 1 + cn + 3, byte_of(0));
                cert[0] = byte_of(tls_message.type_certificate());
                cert[2] = byte_of(1 + cn + 3 >> 8);
                cert[3] = byte_of(1 + cn + 3 & 255);
                cert[4] = byte_of(cn);
                copy_bytes(bytes[k_context()..k_context() + cn], cert[5..5 + cn]);
                transcript_add(ints, cert);
                code = queue_record(ints, bytes, tls_record.type_handshake(), cert);
            }
            if code == 0 {
                let fin = alloc_slice[r](4 + h, byte_of(0));
                fin[0] = byte_of(tls_message.type_finished());
                fin[3] = byte_of(h);
                finished_mac(ints, bytes[k_client_hs()..k_client_hs() + h], fin[4..4 + h]);
                transcript_add(ints, fin);
                code = queue_record(ints, bytes, tls_record.type_handshake(), fin);
            }
            if code == 0 {
                set_write_keys(ints, bytes, k_client_ap());
                set_read_keys(ints, bytes, k_server_ap());
                // The handshake secrets and the master secret have done
                // their work; the application secrets stay for KeyUpdate.
                zero(bytes[k_client_hs()..k_client_hs() + 96]);
                zero(bytes[k_master()..k_master() + 48]);
                ints[i_state()] = state_connected();
            }
        }
    }
    return code;
}

// A KeyUpdate: the next read secret, and an answer when one is asked
// for (RFC 8446 §4.6.3).
fn on_key_update[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    let asked = tls_message.key_update(message[4..len(message)]);
    if asked < 0 {
        return asked;
    }
    ints[i_key_updates()] = ints[i_key_updates()] + 1;
    if ints[i_key_updates()] > 32 {
        return tls_record.too_many_messages();
    }
    var code = 0;
    region r {
        let h = hash_len(ints);
        let next = alloc_slice[r](h, byte_of(0));
        hkdf.expand_label(h, bytes[k_server_ap()..k_server_ap() + h], "traffic upd", "", next);
        copy_bytes(next, bytes[k_server_ap()..k_server_ap() + h]);
        set_read_keys(ints, bytes, k_server_ap());
        if asked == 1 {
            let answer = alloc_slice[r](5, byte_of(0));
            answer[0] = byte_of(tls_message.type_key_update());
            answer[3] = byte_of(1);
            code = queue_record(ints, bytes, tls_record.type_handshake(), answer);
            hkdf.expand_label(h, bytes[k_client_ap()..k_client_ap() + h], "traffic upd", "", next);
            copy_bytes(next, bytes[k_client_ap()..k_client_ap() + h]);
            set_write_keys(ints, bytes, k_client_ap());
        }
        zero(next);
    }
    return code;
}

// One whole handshake message, header included, in the current state.
fn on_message[&i, &b, &m, &p](ints: &!i [int], bytes: &!b [byte], message: &m [byte], store: &p [byte]) -> [] int {
    let kind = int_of(message[0]);
    let state = ints[i_state()];
    if state == state_wait_server_hello() && kind == tls_message.type_server_hello() {
        return on_server_hello(ints, bytes, message);
    }
    if state == state_wait_extensions() && kind == tls_message.type_encrypted_extensions() {
        let code = tls_message.encrypted_extensions(message[4..len(message)]);
        if code == 0 {
            transcript_add(ints, message);
            ints[i_state()] = state_wait_certificate();
        }
        return code;
    }
    if state == state_wait_certificate() && kind == tls_message.type_certificate_request() && !has(ints, f_cert_requested()) {
        // The context is echoed; the rest of the request is not used.
        let body = message[4..len(message)];
        if len(body) < 3 || 1 + int_of(body[0]) + 2 > len(body) {
            return tls_record.decode_error();
        }
        let cn = int_of(body[0]);
        let ext = 1 + cn;
        if ext + 2 + int_of(body[ext]) * 256 + int_of(body[ext + 1]) != len(body) {
            return tls_record.decode_error();
        }
        copy_bytes(body[1..1 + cn], bytes[k_context()..k_context() + cn]);
        ints[i_context_len()] = cn;
        set_flag(ints, f_cert_requested());
        transcript_add(ints, message);
        return 0;
    }
    if state == state_wait_certificate() && kind == tls_message.type_certificate() {
        return on_certificate(ints, bytes, message, store);
    }
    if state == state_wait_verify() && kind == tls_message.type_certificate_verify() {
        return on_certificate_verify(ints, bytes, message);
    }
    if state == state_wait_finished() && kind == tls_message.type_finished() {
        return on_finished(ints, bytes, message);
    }
    if state == state_connected() && kind == tls_message.type_new_session_ticket() {
        return tls_message.new_session_ticket(message[4..len(message)]);
    }
    if state == state_connected() && kind == tls_message.type_key_update() {
        return on_key_update(ints, bytes, message);
    }
    return tls_record.unexpected_message();
}

// Appends handshake bytes and handles every whole message they finish.
fn on_handshake_bytes[&i, &b, &c, &p](ints: &!i [int], bytes: &!b [byte], content: &c [byte], store: &p [byte]) -> [] int {
    let fill = ints[i_hs_fill()];
    if fill + len(content) > hs_cap() {
        return tls_record.record_overflow();
    }
    copy_bytes(content, bytes[b_hs() + fill..b_hs() + fill + len(content)]);
    ints[i_hs_fill()] = fill + len(content);
    var code = 0;
    var at = 0;
    var going = true;
    while going && code == 0 {
        let have = ints[i_hs_fill()] - at;
        if have < 4 {
            going = false;
        } else {
            let p = b_hs() + at;
            let n = int_of(bytes[p + 1]) * 65536 + int_of(bytes[p + 2]) * 256 + int_of(bytes[p + 3]);
            if 4 + n > hs_cap() {
                code = tls_record.record_overflow();
            } else if have < 4 + n {
                going = false;
            } else {
                region r {
                    // A message is handled from a copy, so the handler may
                    // write to the slot's bytes freely.
                    let message = alloc_slice[r](4 + n, byte_of(0));
                    copy_bytes(bytes[p..p + 4 + n], message);
                    let before = ints[i_state()];
                    code = on_message(ints, bytes, message, store);
                    at = at + 4 + n;
                    // A message must not share a record with the next key
                    // (RFC 8446 §5.1): after ServerHello and Finished, the
                    // record must end with the message.
                    let after = ints[i_state()];
                    if code == 0 && after != before && (after == state_wait_extensions() || after == state_connected()) && at < ints[i_hs_fill()] {
                        code = tls_record.unexpected_message();
                    }
                }
            }
        }
    }
    // Drop what was handled; keep a partial message at the front.
    let left = ints[i_hs_fill()] - at;
    var k = 0;
    while k < left {
        bytes[b_hs() + k] = bytes[b_hs() + at + k];
        k = k + 1;
    }
    ints[i_hs_fill()] = left;
    return code;
}

fn on_alert[&i, &b, &c](ints: &!i [int], bytes: &!b [byte], content: &c [byte]) -> [] int {
    if len(content) != 2 {
        return tls_record.decode_error();
    }
    let level = int_of(content[0]);
    let what = int_of(content[1]);
    if what == 0 {
        set_flag(ints, f_close_received());
        if has(ints, f_close_sent()) {
            forget(bytes);
        }
        return 0;
    }
    if what == 90 {
        // user_canceled: wait for the close_notify that follows.
        ints[i_warnings()] = ints[i_warnings()] + 1;
        if ints[i_warnings()] > 16 {
            return tls_record.too_many_messages();
        }
        return 0;
    }
    ints[i_alert()] = what;
    if level != 1 && level != 2 {
        return tls_record.decode_error();
    }
    return tls_record.alert_received();
}

// One whole record from `bytes[b_in()..]`, `n` bytes long.
fn on_record[&i, &b, &p](ints: &!i [int], bytes: &!b [byte], n: int, store: &p [byte]) -> [] int {
    let kind = int_of(bytes[b_in()]);
    let body = n - 5;
    let state = ints[i_state()];
    if kind == tls_record.type_change_cipher_spec() {
        // One, exactly `01`, after ServerHello (or a HelloRetryRequest)
        // and before the server's Finished (RFC 8446 Appendix D.4).
        let early = state < state_wait_extensions() && !(state == state_wait_server_hello() && has(ints, f_retried()));
        if body != 1 || int_of(bytes[b_in() + 5]) != 1 || has(ints, f_ccs_seen()) || early || state > state_wait_finished() {
            return tls_record.unexpected_message();
        }
        set_flag(ints, f_ccs_seen());
        return 0;
    }
    if !has(ints, f_read_protected()) {
        if body > tls_record.max_plaintext() {
            return tls_record.record_overflow();
        }
        if kind != tls_record.type_handshake() && kind != tls_record.type_alert() {
            return tls_record.unexpected_message();
        }
        var plain = 0;
        region r {
            let content = alloc_slice[r](body, byte_of(0));
            copy_bytes(bytes[b_in() + 5..b_in() + n], content);
            if kind == tls_record.type_handshake() {
                plain = on_handshake_bytes(ints, bytes, content, store);
            } else {
                plain = on_alert(ints, bytes, content);
            }
        }
        return plain;
    }
    if kind != tls_record.type_application_data() {
        return tls_record.unexpected_message();
    }
    var code = 0;
    var inner = 0;
    var size = 0;
    region q {
        let info = alloc_slice[q](2, 0);
        code = tls_record.open(ints[i_suite()], bytes[k_read_key()..k_read_key() + key_len(ints)], bytes[k_read_iv()..k_read_iv() + 12], ints[i_read_seq()], bytes[b_in()..b_in() + n], bytes[b_plain()..b_plain() + plain_cap()], info);
        inner = info[0];
        size = info[1];
    }
    if code != 0 {
        return code;
    }
    ints[i_read_seq()] = ints[i_read_seq()] + 1;
    region r {
        let content = alloc_slice[r](size, byte_of(0));
        copy_bytes(bytes[b_plain()..b_plain() + size], content);
        if inner == tls_record.type_handshake() {
            if size == 0 {
                code = tls_record.unexpected_message();
            } else {
                code = on_handshake_bytes(ints, bytes, content, store);
            }
        } else if inner == tls_record.type_alert() {
            code = on_alert(ints, bytes, content);
        } else if inner == tls_record.type_application_data() {
            if state != state_connected() || ints[i_hs_fill()] != 0 {
                code = tls_record.unexpected_message();
            } else {
                let e = ints[i_recv_end()];
                copy_bytes(content, bytes[b_recv() + e..b_recv() + e + size]);
                ints[i_recv_end()] = e + size;
            }
        } else {
            code = tls_record.unexpected_message();
        }
    }
    return code;
}

// ---- The interface (`docs/tls-pure.md` §2.2) ----

// Bytes the socket gave. Answers how many were consumed (all of them,
// unless output or received data must be taken first), or the
// connection's failure.
pub fn feed[&i, &b, &d, &p](ints: &!i [int], bytes: &!b [byte], data: &d [byte], store: &p [byte]) -> [] int {
    var consumed = 0;
    var code = 0;
    while consumed < len(data) && code == 0 && ints[i_state()] != state_failed() && !has(ints, f_close_received()) {
        // Room for what one record can produce: an answer, or its content.
        compact_recv(ints, bytes);
        if out_free(ints) < 1024 || recv_cap() - ints[i_recv_end()] < tls_record.max_plaintext() {
            return consumed;
        }
        let fill = ints[i_in_fill()];
        var need = 5 - fill;
        if fill >= 5 {
            need = 5 + int_of(bytes[b_in() + 3]) * 256 + int_of(bytes[b_in() + 4]) - fill;
        }
        var take = need;
        if take > len(data) - consumed {
            take = len(data) - consumed;
        }
        copy_bytes(data[consumed..consumed + take], bytes[b_in() + fill..b_in() + fill + take]);
        consumed = consumed + take;
        ints[i_in_fill()] = fill + take;
        if fill < 5 && fill + take == 5 {
            // A header is complete: check it before waiting for its body.
            let check = tls_record.record_length(bytes[b_in()..b_in() + 5], 0, 5);
            if check < 0 {
                code = check;
            }
        }
        if code == 0 && ints[i_in_fill()] >= 5 {
            let total = 5 + int_of(bytes[b_in() + 3]) * 256 + int_of(bytes[b_in() + 4]);
            if ints[i_in_fill()] == total {
                ints[i_in_fill()] = 0;
                code = on_record(ints, bytes, total, store);
            }
        }
    }
    if code != 0 {
        return fail(ints, bytes, code);
    }
    if ints[i_state()] == state_failed() {
        return ints[i_failure()];
    }
    return consumed;
}

fn compact_recv[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    let s = ints[i_recv_start()];
    let e = ints[i_recv_end()];
    if s > 0 {
        var k = 0;
        while k < e - s {
            bytes[b_recv() + k] = bytes[b_recv() + s + k];
            k = k + 1;
        }
        ints[i_recv_start()] = 0;
        ints[i_recv_end()] = e - s;
    }
    return 0;
}

// Bytes for the socket, as many as fit in `out`.
pub fn take[&i, &b, &o](ints: &!i [int], bytes: &b [byte], out: &!o [byte]) -> [] int {
    let s = ints[i_out_start()];
    var n = ints[i_out_end()] - s;
    if n > len(out) {
        n = len(out);
    }
    copy_bytes(bytes[b_out() + s..b_out() + s + n], out[0..n]);
    ints[i_out_start()] = s + n;
    return n;
}

// Once established: queues up to one record of `plaintext`. Answers how
// many bytes were taken (0 when the output queue is full), or a refusal.
pub fn send[&i, &b, &p](ints: &!i [int], bytes: &!b [byte], plaintext: &p [byte]) -> [] int {
    if ints[i_state()] != state_connected() || has(ints, f_close_sent()) {
        return tls_record.unexpected_message();
    }
    var n = len(plaintext);
    if n > tls_record.max_plaintext() {
        n = tls_record.max_plaintext();
    }
    compact_out(ints, bytes);
    if out_free(ints) < n + 22 {
        return 0;
    }
    let code = queue_record(ints, bytes, tls_record.type_application_data(), plaintext[0..n]);
    if code != 0 {
        return fail(ints, bytes, code);
    }
    return n;
}

// `recv`'s answer when nothing is waiting yet.
pub fn would_block() -> [] int {
    return -100;
}

// Received application data into `into`: how many bytes, 0 once the peer
// has closed and nothing is left, `would_block()` when nothing is waiting
// yet, or the connection's failure.
pub fn recv[&i, &b, &o](ints: &!i [int], bytes: &b [byte], into: &!o [byte]) -> [] int {
    let s = ints[i_recv_start()];
    var n = ints[i_recv_end()] - s;
    if n == 0 {
        if has(ints, f_close_received()) {
            return 0;
        }
        if ints[i_state()] == state_failed() {
            return ints[i_failure()];
        }
        return would_block();
    }
    if n > len(into) {
        n = len(into);
    }
    copy_bytes(bytes[b_recv() + s..b_recv() + s + n], into[0..n]);
    ints[i_recv_start()] = s + n;
    return n;
}

// The socket ended. After the peer's close_notify that is the clean end;
// before it, the data may have been cut short, and the connection fails
// as `tls-peer-closed` (RFC 8446 §6.1).
pub fn peer_eof[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    if has(ints, f_close_received()) || ints[i_state()] == state_failed() {
        return 0;
    }
    return fail(ints, bytes, tls_record.peer_closed());
}

// Queues close_notify.
pub fn finish[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    if has(ints, f_close_sent()) || ints[i_state()] == state_failed() {
        return 0;
    }
    set_flag(ints, f_close_sent());
    region r {
        let alert = alloc_slice[r](2, byte_of(0));
        alert[0] = byte_of(1);
        queue_record(ints, bytes, tls_record.type_alert(), alert);
    }
    if has(ints, f_close_received()) {
        forget(bytes);
    }
    return 0;
}

pub fn event[&i](ints: &i [int]) -> [] int {
    if ints[i_out_end()] > ints[i_out_start()] {
        return event_want_write();
    }
    if ints[i_state()] == state_failed() {
        return event_failed();
    }
    if has(ints, f_close_received()) || has(ints, f_close_sent()) {
        return event_closed();
    }
    if ints[i_state()] == state_connected() {
        return event_established();
    }
    return event_want_read();
}

// `event` as it will be once every queued byte is taken.
pub fn event_after_take[&i](ints: &i [int]) -> [] int {
    if ints[i_state()] == state_failed() {
        return event_failed();
    }
    if has(ints, f_close_received()) || has(ints, f_close_sent()) {
        return event_closed();
    }
    if ints[i_state()] == state_connected() {
        return event_established();
    }
    return event_want_read();
}

pub fn failure[&i](ints: &i [int]) -> [] int {
    return ints[i_failure()];
}

// The alert the peer sent, when `failure` is `tls-alert`.
pub fn alert_received[&i](ints: &i [int]) -> [] int {
    return ints[i_alert()];
}

// Overwrites the connection's secrets, keys and IVs (best effort,
// `docs/tls-core.md` §8).
pub fn drop[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    zero(bytes[b_keys()..b_keys() + keys_len()]);
    zero(bytes[b_plain()..b_plain() + plain_cap()]);
    zero(bytes[b_recv()..b_recv() + recv_cap()]);
    var k = 0;
    while k < ints_len() {
        ints[k] = 0;
        k = k + 1;
    }
    return 0;
}
