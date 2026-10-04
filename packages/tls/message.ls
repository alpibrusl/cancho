module tls_message;
import tls_record;

// `tls_message` -- the TLS 1.3 handshake messages a client sends and
// reads (RFC 8446 §4; `docs/tls-core.md` §2). Encoding of ClientHello;
// strict parsing of everything else, each fault refused with the tag
// `docs/tls-pure.md` §8 gives it. A parser reads only inside the body
// it is given, and every length it reads is checked against what is
// left before it is used. Not independently reviewed (#209).
//
// Parsers take a message's *body*, after the 4-byte handshake header,
// and answer 0 or a refusal code (`tls_record.refusal_tag`). What they
// find goes into the caller's `info`, as offsets into the body.

pub fn type_client_hello() -> [] int {
    return 1;
}

pub fn type_server_hello() -> [] int {
    return 2;
}

pub fn type_new_session_ticket() -> [] int {
    return 4;
}

pub fn type_encrypted_extensions() -> [] int {
    return 8;
}

pub fn type_certificate() -> [] int {
    return 11;
}

pub fn type_certificate_request() -> [] int {
    return 13;
}

pub fn type_certificate_verify() -> [] int {
    return 15;
}

pub fn type_finished() -> [] int {
    return 20;
}

pub fn type_key_update() -> [] int {
    return 24;
}

// TLS_CHACHA20_POLY1305_SHA256 and x25519, the only suite and group.
pub fn chacha20_poly1305_sha256() -> [] int {
    return 0x1303;
}

pub fn group_x25519() -> [] int {
    return 0x001d;
}

// The signature schemes of `docs/tls-pure.md` §3.1, as their codes.
pub fn ecdsa_p256_sha256() -> [] int {
    return 0x0403;
}

pub fn ecdsa_p384_sha384() -> [] int {
    return 0x0503;
}

pub fn rsa_pss_sha256() -> [] int {
    return 0x0804;
}

pub fn rsa_pss_sha384() -> [] int {
    return 0x0805;
}

pub fn rsa_pss_sha512() -> [] int {
    return 0x0806;
}

pub fn ed25519() -> [] int {
    return 0x0807;
}

// Big-endian integers of `n` bytes.
fn put[&o](out: &!o [byte], at: int, v: int, n: int) -> [] int {
    var i = 0;
    while i < n {
        out[at + i] = byte_of(v >> 8 * (n - 1 - i) & 255);
        i = i + 1;
    }
    return at + n;
}

fn get[&b](b: &b [byte], at: int, n: int) -> [] int {
    var v = 0;
    var i = 0;
    while i < n {
        v = v * 256 + int_of(b[at + i]);
        i = i + 1;
    }
    return v;
}

fn copy_to[&s, &o](src: &s [byte], out: &!o [byte], at: int) -> [] int {
    var i = 0;
    while i < len(src) {
        out[at + i] = src[i];
        i = i + 1;
    }
    return at + len(src);
}

// Whether `host` is an IP literal, which gets no `server_name` (RFC 6066
// §3): digits and dots only, or any colon.
pub fn is_ip_literal[&h](host: &h [byte]) -> [] bool {
    var digits_and_dots = true;
    var i = 0;
    while i < len(host) {
        let c = int_of(host[i]);
        if c == 58 {
            return true;
        }
        if c != 46 && (c < 48 || c > 57) {
            digits_and_dots = false;
        }
        i = i + 1;
    }
    return digits_and_dots;
}

// The largest ClientHello this encodes: a 255-byte host name and the
// fixed extensions.
pub fn max_client_hello() -> [] int {
    return 512;
}

// The ClientHello (`docs/tls-pure.md` §7.1), handshake header included,
// into `out`. Answers its length. `random`, `session_id` and `share`
// are 32 bytes each; `host` is at most 255 bytes, and an IP literal
// sends no `server_name`.
pub fn client_hello[&r, &s, &k, &h, &o](random: &r [byte], session_id: &s [byte], share: &k [byte], host: &h [byte], out: &!o [byte]) -> [] int {
    var at = 4;
    at = put(out, at, 0x0303, 2);
    at = copy_to(random, out, at);
    at = put(out, at, 32, 1);
    at = copy_to(session_id, out, at);
    // One cipher suite, and the null compression method.
    at = put(out, at, 2, 2);
    at = put(out, at, chacha20_poly1305_sha256(), 2);
    at = put(out, at, 1, 1);
    at = put(out, at, 0, 1);
    let ext_len_at = at;
    at = at + 2;
    if len(host) > 0 && !is_ip_literal(host) {
        // server_name: one host_name entry.
        at = put(out, at, 0, 2);
        at = put(out, at, len(host) + 5, 2);
        at = put(out, at, len(host) + 3, 2);
        at = put(out, at, 0, 1);
        at = put(out, at, len(host), 2);
        at = copy_to(host, out, at);
    }
    // supported_groups: x25519.
    at = put(out, at, 10, 2);
    at = put(out, at, 4, 2);
    at = put(out, at, 2, 2);
    at = put(out, at, group_x25519(), 2);
    // signature_algorithms: the six of §3.1.
    at = put(out, at, 13, 2);
    at = put(out, at, 14, 2);
    at = put(out, at, 12, 2);
    at = put(out, at, ecdsa_p256_sha256(), 2);
    at = put(out, at, ecdsa_p384_sha384(), 2);
    at = put(out, at, rsa_pss_sha256(), 2);
    at = put(out, at, rsa_pss_sha384(), 2);
    at = put(out, at, rsa_pss_sha512(), 2);
    at = put(out, at, ed25519(), 2);
    // signature_algorithms_cert: those, and rsa_pkcs1_sha256/384/512.
    at = put(out, at, 50, 2);
    at = put(out, at, 20, 2);
    at = put(out, at, 18, 2);
    at = put(out, at, ecdsa_p256_sha256(), 2);
    at = put(out, at, ecdsa_p384_sha384(), 2);
    at = put(out, at, rsa_pss_sha256(), 2);
    at = put(out, at, rsa_pss_sha384(), 2);
    at = put(out, at, rsa_pss_sha512(), 2);
    at = put(out, at, ed25519(), 2);
    at = put(out, at, 0x0401, 2);
    at = put(out, at, 0x0501, 2);
    at = put(out, at, 0x0601, 2);
    // supported_versions: TLS 1.3 only.
    at = put(out, at, 43, 2);
    at = put(out, at, 3, 2);
    at = put(out, at, 2, 1);
    at = put(out, at, 0x0304, 2);
    // key_share: one X25519 share.
    at = put(out, at, 51, 2);
    at = put(out, at, 38, 2);
    at = put(out, at, 36, 2);
    at = put(out, at, group_x25519(), 2);
    at = put(out, at, 32, 2);
    at = copy_to(share, out, at);
    put(out, ext_len_at, at - ext_len_at - 2, 2);
    put(out, 0, type_client_hello(), 1);
    put(out, 1, at - 4, 3);
    return at;
}

// ---- ServerHello (RFC 8446 §4.1.3) ----

// SHA-256("HelloRetryRequest"), the random that marks a HelloRetryRequest.
fn hrr_random(i: int) -> [] int {
    let hex = "cf21ad74e59a6111be1d8c021e65b891c2a211167abb8c5e079e09e2c8a8339c";
    let hi = int_of(hex[2 * i]);
    let lo = int_of(hex[2 * i + 1]);
    var h = hi - 48;
    if hi >= 97 {
        h = hi - 87;
    }
    var l = lo - 48;
    if lo >= 97 {
        l = lo - 87;
    }
    return h * 16 + l;
}

// The ServerHello body `b`, against the session id the client sent.
// `info[0]` gets where the server's 32-byte X25519 share starts.
pub fn server_hello[&b, &s, &i](b: &b [byte], session_id: &s [byte], info: &!i [int]) -> [] int {
    let n = len(b);
    if n < 2 + 32 + 1 {
        return tls_record.decode_error();
    }
    if get(b, 0, 2) != 0x0303 {
        return tls_record.protocol_version();
    }
    // A HelloRetryRequest is refused before anything else in it is read.
    var k = 0;
    while k < 32 && int_of(b[2 + k]) == hrr_random(k) {
        k = k + 1;
    }
    if k == 32 {
        return tls_record.hello_retry();
    }
    // The downgrade sentinel "DOWNGRD" and 01 or 00 (§4.1.3).
    if get(b, 26, 4) == 0x444f574e && get(b, 30, 3) == 0x475244 && int_of(b[33]) <= 1 {
        return tls_record.protocol_version();
    }
    var at = 34;
    let sid = int_of(b[at]);
    if sid != len(session_id) || at + 1 + sid + 3 > n {
        return tls_record.decode_error();
    }
    var j = 0;
    while j < sid {
        if b[at + 1 + j] != session_id[j] {
            return tls_record.decode_error();
        }
        j = j + 1;
    }
    at = at + 1 + sid;
    if get(b, at, 2) != chacha20_poly1305_sha256() {
        return tls_record.no_shared_cipher();
    }
    if int_of(b[at + 2]) != 0 {
        return tls_record.decode_error();
    }
    at = at + 3;
    if at + 2 > n {
        return tls_record.decode_error();
    }
    let ext_end = at + 2 + get(b, at, 2);
    if ext_end != n {
        return tls_record.decode_error();
    }
    at = at + 2;
    var version = 0;
    var share = 0;
    while at < ext_end {
        if at + 4 > ext_end {
            return tls_record.decode_error();
        }
        let kind = get(b, at, 2);
        let size = get(b, at + 2, 2);
        let body = at + 4;
        if body + size > ext_end {
            return tls_record.decode_error();
        }
        if kind == 43 {
            if version != 0 {
                return tls_record.decode_error();
            }
            if size != 2 || get(b, body, 2) != 0x0304 {
                return tls_record.protocol_version();
            }
            version = 1;
        } else if kind == 51 {
            if share != 0 {
                return tls_record.decode_error();
            }
            if size != 36 || get(b, body, 2) != group_x25519() || get(b, body + 2, 2) != 32 {
                return tls_record.key_share();
            }
            share = body + 4;
        } else {
            return tls_record.unsupported_extension();
        }
        at = body + size;
    }
    if version == 0 {
        // No supported_versions: a TLS 1.2 (or older) ServerHello.
        return tls_record.protocol_version();
    }
    if share == 0 {
        return tls_record.decode_error();
    }
    info[0] = share;
    return 0;
}

// ---- EncryptedExtensions (RFC 8446 §4.3.1) ----

// Only what the client offered may come back: an empty `server_name`
// acknowledgement, and `supported_groups` (which a server may send).
pub fn encrypted_extensions[&b](b: &b [byte]) -> [] int {
    let n = len(b);
    if n < 2 || 2 + get(b, 0, 2) != n {
        return tls_record.decode_error();
    }
    var at = 2;
    var seen_name = false;
    var seen_groups = false;
    while at < n {
        if at + 4 > n {
            return tls_record.decode_error();
        }
        let kind = get(b, at, 2);
        let size = get(b, at + 2, 2);
        if at + 4 + size > n {
            return tls_record.decode_error();
        }
        if kind == 0 {
            if seen_name || size != 0 {
                return tls_record.decode_error();
            }
            seen_name = true;
        } else if kind == 10 {
            if seen_groups {
                return tls_record.decode_error();
            }
            seen_groups = true;
        } else {
            return tls_record.unsupported_extension();
        }
        at = at + 4 + size;
    }
    return 0;
}

// ---- Certificate (RFC 8446 §4.4.2) ----

pub fn max_certificates() -> [] int {
    return 8;
}

// `info[0]`, `info[1]`: the leaf's DER range; `info[2]` the number of
// certificates; `info[3 + 2i]`, `info[4 + 2i]` each certificate's range
// (`info` must hold 3 + 2 * max_certificates() words).
pub fn certificate[&b, &i](b: &b [byte], info: &!i [int]) -> [] int {
    let n = len(b);
    if n < 4 || int_of(b[0]) != 0 {
        // A server's request context is empty.
        return tls_record.decode_error();
    }
    if 4 + get(b, 1, 3) != n {
        return tls_record.decode_error();
    }
    var at = 4;
    var count = 0;
    while at < n {
        if at + 3 > n {
            return tls_record.decode_error();
        }
        let size = get(b, at, 3);
        if size == 0 || at + 3 + size + 2 > n {
            return tls_record.decode_error();
        }
        if count == max_certificates() {
            return tls_record.x509_chain_too_large();
        }
        info[3 + 2 * count] = at + 3;
        info[4 + 2 * count] = at + 3 + size;
        count = count + 1;
        at = at + 3 + size;
        // No CertificateEntry extension was asked for (`status_request`
        // and SCTs are not offered).
        if get(b, at, 2) != 0 {
            return tls_record.unsupported_extension();
        }
        at = at + 2;
    }
    if count == 0 {
        return tls_record.decode_error();
    }
    info[0] = info[3];
    info[1] = info[4];
    info[2] = count;
    return 0;
}

// ---- CertificateVerify (§4.4.3), Finished (§4.4.4) ----

// `info[0]` the scheme, `info[1]`, `info[2]` the signature's range.
pub fn certificate_verify[&b, &i](b: &b [byte], info: &!i [int]) -> [] int {
    let n = len(b);
    if n < 4 || 4 + get(b, 2, 2) != n {
        return tls_record.decode_error();
    }
    info[0] = get(b, 0, 2);
    info[1] = 4;
    info[2] = n;
    return 0;
}

pub fn finished[&b](b: &b [byte]) -> [] int {
    if len(b) != 32 {
        return tls_record.decode_error();
    }
    return 0;
}

// ---- After the handshake (§4.6) ----

// A NewSessionTicket is checked for shape and dropped (no resumption).
pub fn new_session_ticket[&b](b: &b [byte]) -> [] int {
    let n = len(b);
    if n < 9 {
        return tls_record.decode_error();
    }
    var at = 8;
    at = at + 1 + int_of(b[at]);
    if at + 2 > n {
        return tls_record.decode_error();
    }
    let ticket = get(b, at, 2);
    if ticket == 0 {
        return tls_record.decode_error();
    }
    at = at + 2 + ticket;
    if at + 2 > n || at + 2 + get(b, at, 2) != n {
        return tls_record.decode_error();
    }
    return 0;
}

// A KeyUpdate: 0 (update_not_requested) or 1 (update_requested), or a
// refusal.
pub fn key_update[&b](b: &b [byte]) -> [] int {
    if len(b) != 1 || int_of(b[0]) > 1 {
        return tls_record.decode_error();
    }
    return int_of(b[0]);
}
