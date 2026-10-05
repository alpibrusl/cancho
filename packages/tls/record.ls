module tls_record;
import std.chacha20;
import std.gcm;
import std.hmac;

// `tls_record` -- TLS 1.3 records: framing, protection under the suite's
// AEAD (ChaCha20-Poly1305, AES-128-GCM or AES-256-GCM) with the
// per-record nonce, and `TLSInnerPlaintext` (RFC 8446 §5;
// `docs/tls-core.md` §2, `docs/tls-parity.md` §3.3). The lowest module of `packages/tls`, so every
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

// A TLS 1.2 server without the extended master secret (RFC 7627), which
// this client requires (`docs/tls-parity.md` §2).
pub fn extended_master_secret() -> [] int {
    return -17;
}

// A TLS 1.2 HelloRequest: no renegotiation (`docs/tls-parity.md` §3.4).
pub fn renegotiation() -> [] int {
    return -18;
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

pub fn x509_expired() -> [] int {
    return -25;
}

pub fn x509_not_yet_valid() -> [] int {
    return -26;
}

pub fn x509_bad_signature() -> [] int {
    return -27;
}

pub fn x509_name_mismatch() -> [] int {
    return -28;
}

pub fn x509_not_ca() -> [] int {
    return -29;
}

pub fn x509_path_too_long() -> [] int {
    return -30;
}

pub fn x509_name_constraint() -> [] int {
    return -31;
}

pub fn x509_key_usage() -> [] int {
    return -32;
}

pub fn x509_critical_extension() -> [] int {
    return -33;
}

// A server's `pre_shared_key` the client cannot accept: one it did not
// offer, an identity other than the one it offered, or a suite whose hash
// is not the ticket's (`docs/tls-resumption.md` §5).
pub fn illegal_psk() -> [] int {
    return -34;
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
    if code == -17 {
        return "tls-extended-master-secret";
    }
    if code == -18 {
        return "tls-renegotiation";
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
    if code == -25 {
        return "x509-expired";
    }
    if code == -26 {
        return "x509-not-yet-valid";
    }
    if code == -27 {
        return "x509-bad-signature";
    }
    if code == -28 {
        return "x509-name-mismatch";
    }
    if code == -29 {
        return "x509-not-ca";
    }
    if code == -30 {
        return "x509-path-too-long";
    }
    if code == -31 {
        return "x509-name-constraint";
    }
    if code == -32 {
        return "x509-key-usage";
    }
    if code == -33 {
        return "x509-critical-extension";
    }
    if code == -34 {
        return "tls-illegal-psk";
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

// ---- Cipher suites (RFC 8446 §B.4) ----

pub fn suite_aes_128_gcm_sha256() -> [] int {
    return 0x1301;
}

pub fn suite_aes_256_gcm_sha384() -> [] int {
    return 0x1302;
}

pub fn suite_chacha20_poly1305_sha256() -> [] int {
    return 0x1303;
}

// Whether `suite` is one of the three, all of which this client offers.
pub fn suite_known(suite: int) -> [] bool {
    return suite == suite_aes_128_gcm_sha256() || suite == suite_aes_256_gcm_sha384() || suite == suite_chacha20_poly1305_sha256();
}

// The six TLS 1.2 suites offered (`docs/tls-parity.md` §2): ECDHE with an
// AEAD, ECDSA or RSA signing (RFC 5289, RFC 7905).
pub fn suite_ecdhe_ecdsa_aes_128_gcm_sha256() -> [] int {
    return 0xc02b;
}

pub fn suite_ecdhe_ecdsa_aes_256_gcm_sha384() -> [] int {
    return 0xc02c;
}

pub fn suite_ecdhe_rsa_aes_128_gcm_sha256() -> [] int {
    return 0xc02f;
}

pub fn suite_ecdhe_rsa_aes_256_gcm_sha384() -> [] int {
    return 0xc030;
}

pub fn suite_ecdhe_rsa_chacha20_poly1305() -> [] int {
    return 0xcca8;
}

pub fn suite_ecdhe_ecdsa_chacha20_poly1305() -> [] int {
    return 0xcca9;
}

pub fn suite12_known(suite: int) -> [] bool {
    return suite == 0xc02b || suite == 0xc02c || suite == 0xc02f || suite == 0xc030 || suite == 0xcca8 || suite == 0xcca9;
}

// Whether a TLS 1.2 suite's server signs with ECDSA (or Ed25519, RFC
// 8422), rather than RSA.
pub fn suite12_ecdsa(suite: int) -> [] bool {
    return suite == 0xc02b || suite == 0xc02c || suite == 0xcca9;
}

fn chacha(suite: int) -> [] bool {
    return suite == suite_chacha20_poly1305_sha256() || suite == 0xcca8 || suite == 0xcca9;
}

// The suite's hash length: 48 for SHA-384, else 32 (SHA-256).
pub fn hash_len(suite: int) -> [] int {
    if suite == suite_aes_256_gcm_sha384() || suite == 0xc02c || suite == 0xc030 {
        return 48;
    }
    return 32;
}

// The suite's AEAD key length: 16 for AES-128-GCM, else 32.
pub fn key_len(suite: int) -> [] int {
    if suite == suite_aes_128_gcm_sha256() || suite == 0xc02b || suite == 0xc02f {
        return 16;
    }
    return 32;
}

// A TLS 1.2 suite's fixed IV length: AES-GCM's 4-byte salt (RFC 5288
// §3), ChaCha20-Poly1305's 12 bytes (RFC 7905 §2).
pub fn iv12_len(suite: int) -> [] int {
    if chacha(suite) {
        return 12;
    }
    return 4;
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

// The suite's AEAD: `seal` and `open` of `std.chacha20` or `std.gcm`,
// which take and answer the same things.
fn aead_seal[&k, &n, &a, &p, &o](suite: int, key: &k [byte], nonce: &n [byte], aad: &a [byte], plaintext: &p [byte], out: &!o [byte]) -> [] int {
    if chacha(suite) {
        return chacha20.seal(key, nonce, aad, plaintext, out);
    }
    return gcm.seal(key, nonce, aad, plaintext, out);
}

fn aead_open[&k, &n, &a, &s, &o](suite: int, key: &k [byte], nonce: &n [byte], aad: &a [byte], sealed: &s [byte], out: &!o [byte]) -> [] int {
    if chacha(suite) {
        return chacha20.open(key, nonce, aad, sealed, out);
    }
    return gcm.open(key, nonce, aad, sealed, out);
}

// One protected record of `content_type` carrying `plaintext`, into
// `out`: header, then ciphertext and tag, under `suite`'s AEAD with
// `key` (`key_len(suite)` bytes). Answers its length, or a refusal.
// `out` must hold `len(plaintext) + 22` bytes.
pub fn seal[&k, &i, &p, &o](suite: int, key: &k [byte], iv: &i [byte], seq: int, content_type: int, plaintext: &p [byte], out: &!o [byte]) -> [] int {
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
        code = aead_seal(suite, key, iv_seq, out[0..5], inner, out[5..5 + body]);
        // The inner plaintext is the caller's data; it is not erased here,
        // as the caller still holds it.
    }
    if code != 0 {
        return -10;
    }
    return 5 + body;
}

// Opens the protected record `record` (header included) under `suite`'s
// AEAD into `out`, which must hold `len(record) - 21` bytes. `info[0]` gets the inner
// content type and `info[1]` the content's length. 0, or -10 for a
// record that does not authenticate, -9 for content over the limit,
// -6 for an inner plaintext with no content type.
pub fn open[&k, &i, &r, &o, &f](suite: int, key: &k [byte], iv: &i [byte], seq: int, record: &r [byte], out: &!o [byte], info: &!f [int]) -> [] int {
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
        code = aead_open(suite, key, iv_seq, record[0..5], record[5..5 + body], out[0..text]);
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

// ---- TLS 1.2 (RFC 5246 §6.2.3.3, RFC 5288 §3, RFC 7905 §2) ----

// The nonce of record `seq` under a TLS 1.2 suite into `out` (12
// bytes): ChaCha20-Poly1305's is the 12-byte IV XORed with the sequence
// number, as in TLS 1.3; AES-GCM's is the 4-byte salt, then `explicit`
// (8 bytes, sent in the record).
fn nonce12[&i, &e, &o](suite: int, iv: &i [byte], seq: int, explicit: &e [byte], out: &!o [byte]) -> [] int {
    if chacha(suite) {
        return nonce(iv, seq, out);
    }
    var k = 0;
    while k < 4 {
        out[k] = iv[k];
        k = k + 1;
    }
    while k < 12 {
        out[k] = explicit[k - 4];
        k = k + 1;
    }
    return 0;
}

// The additional data: the sequence number, the type, the version and
// the plaintext's length.
fn aad12[&o](seq: int, content_type: int, n: int, out: &!o [byte]) -> [] int {
    var k = 0;
    while k < 8 {
        out[k] = byte_of(seq >> 8 * (7 - k) & 255);
        k = k + 1;
    }
    out[8] = byte_of(content_type);
    out[9] = byte_of(3);
    out[10] = byte_of(3);
    out[11] = byte_of(n >> 8);
    out[12] = byte_of(n & 255);
    return 0;
}

// The bytes a TLS 1.2 record adds to its plaintext: the header, the tag,
// and AES-GCM's explicit nonce.
pub fn overhead12(suite: int) -> [] int {
    if chacha(suite) {
        return 5 + 16;
    }
    return 5 + 8 + 16;
}

// One TLS 1.2 record of `content_type` carrying `plaintext` under
// `suite`, into `out` (`len(plaintext) + overhead12(suite)` bytes).
// AES-GCM's explicit nonce is the sequence number, which never repeats
// under one key. Answers the record's length, or a refusal.
pub fn seal12[&k, &i, &p, &o](suite: int, key: &k [byte], iv: &i [byte], seq: int, content_type: int, plaintext: &p [byte], out: &!o [byte]) -> [] int {
    let n = len(plaintext);
    if n > max_plaintext() {
        return -9;
    }
    if seq >= max_sequence() {
        return -14;
    }
    let explicit = overhead12(suite) - 21;
    let body = explicit + n + 16;
    if len(out) < 5 + body {
        return -9;
    }
    out[0] = byte_of(content_type);
    out[1] = byte_of(3);
    out[2] = byte_of(3);
    out[3] = byte_of(body >> 8);
    out[4] = byte_of(body & 255);
    var code = 0;
    region r {
        let ad = alloc_slice[r](13, byte_of(0));
        let iv_seq = alloc_slice[r](12, byte_of(0));
        var k = 0;
        while k < explicit {
            out[5 + k] = byte_of(seq >> 8 * (7 - k) & 255);
            k = k + 1;
        }
        nonce12(suite, iv, seq, out[5..5 + explicit], iv_seq);
        aad12(seq, content_type, n, ad);
        code = aead_seal(suite, key, iv_seq, ad, plaintext, out[5 + explicit..5 + body]);
    }
    if code != 0 {
        return -10;
    }
    return 5 + body;
}

// Opens the TLS 1.2 record `record` (header included) under `suite`
// into `out`, which must hold the plaintext. `info[0]` gets the content
// type, from the header, and `info[1]` the plaintext's length. 0, or -10
// for a record that does not authenticate, -9 over the limit.
pub fn open12[&k, &i, &r, &o, &f](suite: int, key: &k [byte], iv: &i [byte], seq: int, record: &r [byte], out: &!o [byte], info: &!f [int]) -> [] int {
    let explicit = overhead12(suite) - 21;
    let body = len(record) - 5;
    if body < explicit + 16 {
        return -10;
    }
    if body > max_ciphertext() {
        return -9;
    }
    if seq >= max_sequence() {
        return -14;
    }
    let n = body - explicit - 16;
    if n > max_plaintext() {
        return -9;
    }
    if len(out) < n {
        return -9;
    }
    let content_type = int_of(record[0]);
    var code = 0;
    region s {
        let ad = alloc_slice[s](13, byte_of(0));
        let iv_seq = alloc_slice[s](12, byte_of(0));
        nonce12(suite, iv, seq, record[5..5 + explicit], iv_seq);
        aad12(seq, content_type, n, ad);
        code = aead_open(suite, key, iv_seq, ad, record[5 + explicit..5 + body], out[0..n]);
    }
    if code != 0 {
        return -10;
    }
    info[0] = content_type;
    info[1] = n;
    return 0;
}

// The TLS 1.2 PRF (RFC 5246 §5): P_hash(secret, label || seed) under
// HMAC with the suite's hash (`hash_len`, 32 or 48), into `out`.
pub fn prf[&s, &l, &d, &o](hash_len: int, secret: &s [byte], label: &l [byte], seed: &d [byte], out: &!o [byte]) -> [] int {
    region r {
        let a = alloc_slice[r](hash_len, byte_of(0));
        let block = alloc_slice[r](hash_len, byte_of(0));
        let st = alloc_slice[r](hmac.state_len(hash_len), 0);
        // A(1) = HMAC(secret, label || seed).
        hmac.init(hash_len, st, secret);
        hmac.update(hash_len, st, label);
        hmac.update(hash_len, st, seed);
        hmac.final(hash_len, st, a);
        var at = 0;
        while at < len(out) {
            hmac.init(hash_len, st, secret);
            hmac.update(hash_len, st, a);
            hmac.update(hash_len, st, label);
            hmac.update(hash_len, st, seed);
            hmac.final(hash_len, st, block);
            var k = 0;
            while k < hash_len && at + k < len(out) {
                out[at + k] = block[k];
                k = k + 1;
            }
            at = at + hash_len;
            // A(i + 1) = HMAC(secret, A(i)).
            hmac.init(hash_len, st, secret);
            hmac.update(hash_len, st, a);
            hmac.final(hash_len, st, a);
        }
    }
    return 0;
}
