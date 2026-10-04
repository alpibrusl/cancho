module tls_record;
import std.chacha20;

// `tls_record` -- TLS 1.3 records: framing, ChaCha20-Poly1305 protection
// with the per-record nonce, and `TLSInnerPlaintext` (RFC 8446 §5;
// `docs/tls-core.md` §2). The lowest module of `packages/tls`, so every
// refusal code of the package is defined here, once. Not independently
// reviewed (#209).
//
// The key and IV are secret; nothing here branches on them, or on the
// plaintext, except the scan for the inner content type over padding,
// whose length the peer chose and is public.

// ---- Refusals (`docs/tls-pure.md` §8) ----

pub fn peer_closed() -> [] int {
    return -1;
}

pub fn alert_received() -> [] int {
    return -2;
}

pub fn protocol_version() -> [] int {
    return -3;
}

pub fn no_shared_cipher() -> [] int {
    return -4;
}

pub fn hello_retry() -> [] int {
    return -5;
}

pub fn unexpected_message() -> [] int {
    return -6;
}

pub fn decode_error() -> [] int {
    return -7;
}

pub fn unsupported_extension() -> [] int {
    return -8;
}

pub fn record_overflow() -> [] int {
    return -9;
}

pub fn bad_record_mac() -> [] int {
    return -10;
}

pub fn key_share() -> [] int {
    return -11;
}

pub fn bad_certificate_verify() -> [] int {
    return -12;
}

pub fn bad_finished() -> [] int {
    return -13;
}

pub fn too_many_messages() -> [] int {
    return -14;
}

pub fn no_entropy() -> [] int {
    return -15;
}

pub fn bad_slot() -> [] int {
    return -16;
}

pub fn x509_decode() -> [] int {
    return -20;
}

pub fn x509_unknown_issuer() -> [] int {
    return -21;
}

pub fn x509_unsupported_algorithm() -> [] int {
    return -22;
}

pub fn x509_key_size() -> [] int {
    return -23;
}

pub fn x509_chain_too_large() -> [] int {
    return -24;
}

pub fn refusal_tag(code: int) -> [] &static [byte] {
    if code == 0 {
        return "ok";
    }
    if code == -1 {
        return "tls-peer-closed";
    }
    if code == -2 {
        return "tls-alert";
    }
    if code == -3 {
        return "tls-protocol-version";
    }
    if code == -4 {
        return "tls-no-shared-cipher";
    }
    if code == -5 {
        return "tls-hello-retry";
    }
    if code == -6 {
        return "tls-unexpected-message";
    }
    if code == -7 {
        return "tls-decode-error";
    }
    if code == -8 {
        return "tls-unsupported-extension";
    }
    if code == -9 {
        return "tls-record-overflow";
    }
    if code == -10 {
        return "tls-bad-record-mac";
    }
    if code == -11 {
        return "tls-key-share";
    }
    if code == -12 {
        return "tls-bad-certificate-verify";
    }
    if code == -13 {
        return "tls-bad-finished";
    }
    if code == -14 {
        return "tls-too-many-messages";
    }
    if code == -15 {
        return "tls-no-entropy";
    }
    if code == -16 {
        return "tls-slot";
    }
    if code == -20 {
        return "x509-decode";
    }
    if code == -21 {
        return "x509-unknown-issuer";
    }
    if code == -22 {
        return "x509-unsupported-algorithm";
    }
    if code == -23 {
        return "x509-key-size";
    }
    if code == -24 {
        return "x509-chain-too-large";
    }
    return "unknown";
}

// ---- Record framing (RFC 8446 §5.1, §5.2) ----

pub fn type_change_cipher_spec() -> [] int {
    return 20;
}

pub fn type_alert() -> [] int {
    return 21;
}

pub fn type_handshake() -> [] int {
    return 22;
}

pub fn type_application_data() -> [] int {
    return 23;
}

// A plaintext's limit, and a protected record's (`docs/tls-pure.md` §7.1).
pub fn max_plaintext() -> [] int {
    return 16384;
}

pub fn max_ciphertext() -> [] int {
    return 16384 + 256;
}

pub fn header_len() -> [] int {
    return 5;
}

// The record starting at `buf[at]`, whose bytes end at `end`: its whole
// length (header included) when all of it is there, 0 when more bytes
// are needed, or a refusal for a header no TLS 1.3 peer sends.
pub fn record_length[&b](buf: &b [byte], at: int, end: int) -> [] int {
    if end - at < 5 {
        return 0;
    }
    let kind = int_of(buf[at]);
    if kind < 20 || kind > 23 {
        return -6;
    }
    if int_of(buf[at + 1]) != 3 {
        return -3;
    }
    let n = int_of(buf[at + 3]) * 256 + int_of(buf[at + 4]);
    if n > max_ciphertext() {
        return -9;
    }
    if n == 0 {
        return -7;
    }
    if end - at < 5 + n {
        return 0;
    }
    return 5 + n;
}

// ---- Protection (RFC 8446 §5.2, §5.3) ----

// The per-record nonce: the 64-bit sequence number, left-padded to the
// IV's 12 bytes, XORed with the IV.
pub fn nonce[&i, &o](iv: &i [byte], seq: int, out: &!o [byte]) -> [] int {
    var k = 0;
    while k < 12 {
        var s = 0;
        if k >= 4 {
            s = seq >> 8 * (11 - k) & 255;
        }
        out[k] = byte_of(int_of(iv[k]) ^ s);
        k = k + 1;
    }
    return 0;
}

// The largest sequence number used; a connection that reaches it is
// closed rather than wrap (`docs/tls-pure.md` §7.1).
pub fn max_sequence() -> [] int {
    return 1 << 62;
}

// One protected record of `content_type` carrying `plaintext`, into
// `out`: header, then ciphertext and tag. Answers its length, or a
// refusal. `out` must hold `len(plaintext) + 22` bytes.
pub fn seal[&k, &i, &p, &o](key: &k [byte], iv: &i [byte], seq: int, content_type: int, plaintext: &p [byte], out: &!o [byte]) -> [] int {
    let n = len(plaintext);
    if n > max_plaintext() {
        return -9;
    }
    if seq >= max_sequence() {
        return -14;
    }
    let body = n + 1 + 16;
    if len(out) < 5 + body {
        return -9;
    }
    out[0] = byte_of(23);
    out[1] = byte_of(3);
    out[2] = byte_of(3);
    out[3] = byte_of(body >> 8);
    out[4] = byte_of(body & 255);
    var code = 0;
    region r {
        let inner = alloc_slice[r](n + 1, byte_of(0));
        let iv_seq = alloc_slice[r](12, byte_of(0));
        var j = 0;
        while j < n {
            inner[j] = plaintext[j];
            j = j + 1;
        }
        inner[n] = byte_of(content_type);
        nonce(iv, seq, iv_seq);
        code = chacha20.seal(key, iv_seq, out[0..5], inner, out[5..5 + body]);
        // The inner plaintext is the caller's data; it is not erased here,
        // as the caller still holds it.
    }
    if code != 0 {
        return -10;
    }
    return 5 + body;
}

// Opens the protected record `record` (header included) into `out`,
// which must hold `len(record) - 21` bytes. `info[0]` gets the inner
// content type and `info[1]` the content's length. 0, or -10 for a
// record that does not authenticate, -9 for content over the limit,
// -6 for an inner plaintext with no content type.
pub fn open[&k, &i, &r, &o, &f](key: &k [byte], iv: &i [byte], seq: int, record: &r [byte], out: &!o [byte], info: &!f [int]) -> [] int {
    let body = len(record) - 5;
    if body < 17 {
        return -10;
    }
    if body > max_ciphertext() {
        return -9;
    }
    if seq >= max_sequence() {
        return -14;
    }
    if int_of(record[0]) != 23 {
        return -6;
    }
    let text = body - 16;
    if len(out) < text {
        return -9;
    }
    var code = 0;
    region s {
        let iv_seq = alloc_slice[s](12, byte_of(0));
        nonce(iv, seq, iv_seq);
        code = chacha20.open(key, iv_seq, record[0..5], record[5..5 + body], out[0..text]);
    }
    if code != 0 {
        return -10;
    }
    // The content type is the last non-zero byte; zeros after it are
    // padding (RFC 8446 §5.4).
    var last = text - 1;
    while last >= 0 && int_of(out[last]) == 0 {
        last = last - 1;
    }
    if last < 0 {
        return -6;
    }
    if last > max_plaintext() {
        return -9;
    }
    info[0] = int_of(out[last]);
    info[1] = last;
    return 0;
}
