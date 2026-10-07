module tls_message;
import tls_record;
import x509;

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

// The groups offered (`docs/tls-parity.md` §2): X25519, with a share in
// the first ClientHello, and P-256 and P-384, which a server can ask
// for with a HelloRetryRequest. The suites are `tls_record`'s.
pub fn group_x25519() -> [] int {
    return 0x001d;
}

pub fn group_p256() -> [] int {
    return 0x0017;
}

pub fn group_p384() -> [] int {
    return 0x0018;
}

// The length of a share of `group`: X25519's 32 bytes, or an
// uncompressed point (RFC 8446 §4.2.8.2). 0 for a group not offered.
pub fn share_len(group: int) -> [] int {
    if group == group_x25519() {
        return 32;
    }
    if group == group_p256() {
        return 65;
    }
    if group == group_p384() {
        return 97;
    }
    return 0;
}

// The longest cookie a HelloRetryRequest may carry here. RFC 8446 allows
// 2^16 - 1 bytes; a stateless server's cookie is a few hundred, and a
// longer one is refused rather than grow every slot (`docs/tls-parity.md`
// §3.3).
pub fn max_cookie() -> [] int {
    return 2048;
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

// RSASSA-PKCS1-v1_5, for certificates, and a TLS 1.2 ServerKeyExchange.
pub fn rsa_pkcs1_sha256() -> [] int {
    return 0x0401;
}

pub fn rsa_pkcs1_sha384() -> [] int {
    return 0x0501;
}

pub fn rsa_pkcs1_sha512() -> [] int {
    return 0x0601;
}

// ECDSA with SHA-512: in TLS 1.2 the scheme names the hash, not the
// curve, so a P-256 or P-384 key may sign with it.
pub fn ecdsa_sha512() -> [] int {
    return 0x0603;
}

// Big-endian integers of `n` bytes (`tls_hello` and `tls_identity` use them too).
pub fn put[&o](out: &!o [byte], at: int, v: int, n: int) -> [] int {
    var i = 0;
    while i < n {
        out[at + i] = byte_of(v >> 8 * (n - 1 - i) & 255);
        i = i + 1;
    }
    return at + n;
}

pub fn get[&b](b: &b [byte], at: int, n: int) -> [] int {
    var v = 0;
    var i = 0;
    while i < n {
        v = v * 256 + int_of(b[at + i]);
        i = i + 1;
    }
    return v;
}

pub fn copy_to[&s, &o](src: &s [byte], out: &!o [byte], at: int) -> [] int {
    var i = 0;
    while i < len(src) {
        out[at + i] = src[i];
        i = i + 1;
    }
    return at + len(src);
}

// The largest ClientHello this encodes: a 255-byte host name, a P-384
// share, the longest cookie, the fixed extensions, and a ticket of
// `max_ticket()` bytes with a SHA-384 binder.
pub fn max_client_hello() -> [] int {
    return 704 + max_cookie() + 6 + 4 + 2 + 2 + max_ticket() + 4 + 2 + 1 + 48;
}

// The largest ticket offered or kept (`tls_slot.ticket_cap()`).
pub fn max_ticket() -> [] int {
    return 2048;
}

// The bytes after a ClientHello's binder starts: what a binder for a hash
// of `hash_len` bytes leaves out of the truncated ClientHello it is over
// (RFC 8446 §4.2.11.2): the binders' list length, the binder's length and
// the binder.
pub fn binders_len(hash_len: int) -> [] int {
    return 2 + 1 + hash_len;
}

// The ClientHello (`docs/tls-pure.md` §7.1, `docs/tls-parity.md` §3.3),
// handshake header included, into `out`. Answers its length. `random`
// and `session_id` are 32 bytes each; `share` is one key share of
// `group`; `cookie` is a HelloRetryRequest's cookie, echoed, or empty;
// `host` is at most 255 bytes, and an IP literal sends no `server_name`.
// `modes` sends `psk_key_exchange_modes` with `psk_dhe_ke` only, which a
// server needs to see before it sends tickets (RFC 8446 §4.2.9). `ticket`,
// if not empty, is offered as a PSK with (EC)DHE, and `modes` is implied
// (`docs/tls-resumption.md` §5): `pre_shared_key` last, its one identity with
// `age` as its obfuscated_ticket_age and a binder of `hash_len` zero
// bytes, which the caller computes over the truncated ClientHello and
// writes in its place (the last `hash_len` bytes).
pub fn client_hello[&r, &s, &k, &c, &h, &t, &o](random: &r [byte], session_id: &s [byte], group: int, share: &k [byte], cookie: &c [byte], host: &h [byte], modes: bool, ticket: &t [byte], age: int, hash_len: int, out: &!o [byte]) -> [] int {
    var at = 4;
    at = put(out, at, 0x0303, 2);
    at = copy_to(random, out, at);
    at = put(out, at, 32, 1);
    at = copy_to(session_id, out, at);
    // The three TLS 1.3 suites, then the six TLS 1.2 ones, in OpenSSL's
    // order (`docs/tls-parity.md` §2), the renegotiation SCSV (RFC 5746
    // §3.4), and the null compression method.
    at = put(out, at, 20, 2);
    at = put(out, at, tls_record.suite_aes_256_gcm_sha384(), 2);
    at = put(out, at, tls_record.suite_chacha20_poly1305_sha256(), 2);
    at = put(out, at, tls_record.suite_aes_128_gcm_sha256(), 2);
    at = put(out, at, tls_record.suite_ecdhe_ecdsa_aes_256_gcm_sha384(), 2);
    at = put(out, at, tls_record.suite_ecdhe_rsa_aes_256_gcm_sha384(), 2);
    at = put(out, at, tls_record.suite_ecdhe_ecdsa_chacha20_poly1305(), 2);
    at = put(out, at, tls_record.suite_ecdhe_rsa_chacha20_poly1305(), 2);
    at = put(out, at, tls_record.suite_ecdhe_ecdsa_aes_128_gcm_sha256(), 2);
    at = put(out, at, tls_record.suite_ecdhe_rsa_aes_128_gcm_sha256(), 2);
    at = put(out, at, 0x00ff, 2);
    at = put(out, at, 1, 1);
    at = put(out, at, 0, 1);
    let ext_len_at = at;
    at = at + 2;
    if len(host) > 0 && !x509.is_ip_literal(host) {
        // server_name: one host_name entry.
        at = put(out, at, 0, 2);
        at = put(out, at, len(host) + 5, 2);
        at = put(out, at, len(host) + 3, 2);
        at = put(out, at, 0, 1);
        at = put(out, at, len(host), 2);
        at = copy_to(host, out, at);
    }
    // ec_point_formats: uncompressed (RFC 8422 §5.1.2), for TLS 1.2.
    at = put(out, at, 11, 2);
    at = put(out, at, 2, 2);
    at = put(out, at, 1, 1);
    at = put(out, at, 0, 1);
    // extended_master_secret (RFC 7627), for TLS 1.2, where it is required.
    at = put(out, at, 23, 2);
    at = put(out, at, 0, 2);
    // supported_groups: x25519, secp256r1, secp384r1.
    at = put(out, at, 10, 2);
    at = put(out, at, 8, 2);
    at = put(out, at, 6, 2);
    at = put(out, at, group_x25519(), 2);
    at = put(out, at, group_p256(), 2);
    at = put(out, at, group_p384(), 2);
    // signature_algorithms: the six of §3.1, and for a TLS 1.2
    // ServerKeyExchange, rsa_pkcs1_sha256/384/512 (TLS 1.3 never accepts
    // those, RFC 8446 §4.2.3).
    at = put(out, at, 13, 2);
    at = put(out, at, 20, 2);
    at = put(out, at, 18, 2);
    at = put(out, at, ecdsa_p256_sha256(), 2);
    at = put(out, at, ecdsa_p384_sha384(), 2);
    at = put(out, at, rsa_pss_sha256(), 2);
    at = put(out, at, rsa_pss_sha384(), 2);
    at = put(out, at, rsa_pss_sha512(), 2);
    at = put(out, at, ed25519(), 2);
    at = put(out, at, rsa_pkcs1_sha256(), 2);
    at = put(out, at, rsa_pkcs1_sha384(), 2);
    at = put(out, at, rsa_pkcs1_sha512(), 2);
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
    at = put(out, at, rsa_pkcs1_sha256(), 2);
    at = put(out, at, rsa_pkcs1_sha384(), 2);
    at = put(out, at, rsa_pkcs1_sha512(), 2);
    // supported_versions: TLS 1.3, then 1.2.
    at = put(out, at, 43, 2);
    at = put(out, at, 5, 2);
    at = put(out, at, 4, 1);
    at = put(out, at, 0x0304, 2);
    at = put(out, at, 0x0303, 2);
    // key_share: one share.
    at = put(out, at, 51, 2);
    at = put(out, at, len(share) + 6, 2);
    at = put(out, at, len(share) + 4, 2);
    at = put(out, at, group, 2);
    at = put(out, at, len(share), 2);
    at = copy_to(share, out, at);
    if len(cookie) > 0 {
        // cookie: the HelloRetryRequest's, unchanged (RFC 8446 §4.2.2).
        at = put(out, at, 44, 2);
        at = put(out, at, len(cookie) + 2, 2);
        at = put(out, at, len(cookie), 2);
        at = copy_to(cookie, out, at);
    }
    if modes || len(ticket) > 0 {
        // psk_key_exchange_modes: psk_dhe_ke (1) only.
        at = put(out, at, 45, 2);
        at = put(out, at, 2, 2);
        at = put(out, at, 1, 1);
        at = put(out, at, 1, 1);
    }
    if len(ticket) > 0 {
        // pre_shared_key, last (RFC 8446 §4.2.11): one identity, one binder.
        at = put(out, at, 41, 2);
        at = put(out, at, 2 + 2 + len(ticket) + 4 + binders_len(hash_len), 2);
        at = put(out, at, 2 + len(ticket) + 4, 2);
        at = put(out, at, len(ticket), 2);
        at = copy_to(ticket, out, at);
        at = put(out, at, age & 0xffffffff, 4);
        at = put(out, at, 1 + hash_len, 2);
        at = put(out, at, hash_len, 1);
        var z = 0;
        while z < hash_len {
            out[at + z] = byte_of(0);
            z = z + 1;
        }
        at = at + hash_len;
    }
    put(out, ext_len_at, at - ext_len_at - 2, 2);
    put(out, 0, type_client_hello(), 1);
    put(out, 1, at - 4, 3);
    return at;
}

// ---- ServerHello (RFC 8446 §4.1.3) ----

// SHA-256("HelloRetryRequest"), the random that marks a HelloRetryRequest.
pub fn hrr_random(i: int) -> [] int {
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

// What `server_hello` finds, as indices into its `info`.
pub fn sh_share() -> [] int {
    return 0;
}

// 1 for a HelloRetryRequest, else 0.
pub fn sh_retry() -> [] int {
    return 1;
}

pub fn sh_suite() -> [] int {
    return 2;
}

// The share's group, or the group a HelloRetryRequest selects.
pub fn sh_group() -> [] int {
    return 3;
}

// A HelloRetryRequest's cookie: its range in the body, or 0 and 0.
pub fn sh_cookie_start() -> [] int {
    return 4;
}

pub fn sh_cookie_end() -> [] int {
    return 5;
}

// The version chosen: 0x0304 (TLS 1.3, `supported_versions`), or 0x0303
// (TLS 1.2, none).
pub fn sh_version() -> [] int {
    return 6;
}

// 1 if the server accepted the ticket offered (`pre_shared_key`,
// selected_identity 0), else 0.
pub fn sh_psk() -> [] int {
    return 7;
}

pub fn sh_info_len() -> [] int {
    return 8;
}

// The ServerHello body `b`, against the session id the client sent: a
// TLS 1.3 ServerHello, a HelloRetryRequest (RFC 8446 §4.1.3, §4.1.4),
// told apart by the random, or a TLS 1.2 ServerHello, told apart by
// having no `supported_versions` (RFC 5246 §7.4.1.3; `docs/tls-parity.md`
// §3.4). Checks everything that needs no memory of the connection;
// `tls_client` checks the rest (a second HelloRetryRequest, the group a
// share is for, the suite against the retry's).
pub fn server_hello[&b, &s, &i](b: &b [byte], session_id: &s [byte], info: &!i [int]) -> [] int {
    let n = len(b);
    if n < 2 + 32 + 1 {
        return tls_record.decode_error();
    }
    if get(b, 0, 2) != 0x0303 {
        return tls_record.protocol_version();
    }
    var k = 0;
    while k < 32 && int_of(b[2 + k]) == hrr_random(k) {
        k = k + 1;
    }
    let retry = k == 32;
    var at = 34;
    let sid = int_of(b[at]);
    if sid > 32 || at + 1 + sid + 3 > n {
        return tls_record.decode_error();
    }
    let sid_at = at + 1;
    at = at + 1 + sid;
    let suite = get(b, at, 2);
    if !tls_record.suite_known(suite) && !tls_record.suite12_known(suite) {
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
    var group = 0;
    var cookie = 0;
    var cookie_end = 0;
    var psk = false;
    // TLS 1.2's: renegotiation_info, extended_master_secret, ec_point_formats,
    // and server_name, empty, which a server that used the name sends
    // (RFC 6066 §3; nginx does). TLS 1.3 sends that one in
    // EncryptedExtensions instead.
    var sni = false;
    var reneg = false;
    var ems = false;
    var formats = false;
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
            if group != 0 {
                return tls_record.decode_error();
            }
            if retry {
                // A HelloRetryRequest names a group, with no share: one
                // offered, and not X25519, whose share was sent.
                if size != 2 {
                    return tls_record.decode_error();
                }
                group = get(b, body, 2);
                if group != group_p256() && group != group_p384() {
                    return tls_record.key_share();
                }
            } else {
                if size < 4 {
                    return tls_record.decode_error();
                }
                group = get(b, body, 2);
                let want = share_len(group);
                if want == 0 || size != 4 + want || get(b, body + 2, 2) != want {
                    return tls_record.key_share();
                }
                share = body + 4;
            }
        } else if kind == 44 && retry {
            if cookie != 0 {
                return tls_record.decode_error();
            }
            let c = get(b, body, 2);
            if size < 3 || c + 2 != size {
                return tls_record.decode_error();
            }
            if c > max_cookie() {
                return tls_record.hello_retry();
            }
            cookie = body + 2;
            cookie_end = body + size;
        } else if kind == 41 && !retry {
            // pre_shared_key: the identity the server selected, which must
            // be the one offered (RFC 8446 §4.2.11). Whether one was
            // offered at all is `tls_client`'s to check.
            if psk || size != 2 {
                return tls_record.decode_error();
            }
            if get(b, body, 2) != 0 {
                return tls_record.illegal_psk();
            }
            psk = true;
        } else if kind == 0xff01 && !retry {
            // An empty renegotiated_connection: a first handshake.
            if reneg || size != 1 || int_of(b[body]) != 0 {
                return tls_record.decode_error();
            }
            reneg = true;
        } else if kind == 0 && !retry {
            if sni || size != 0 {
                return tls_record.decode_error();
            }
            sni = true;
        } else if kind == 23 && !retry {
            if ems || size != 0 {
                return tls_record.decode_error();
            }
            ems = true;
        } else if kind == 11 && !retry {
            // ec_point_formats: a list that includes uncompressed (0).
            if formats || size < 2 || int_of(b[body]) + 1 != size {
                return tls_record.decode_error();
            }
            var uncompressed = false;
            var j = 1;
            while j < size {
                if int_of(b[body + j]) == 0 {
                    uncompressed = true;
                }
                j = j + 1;
            }
            if !uncompressed {
                return tls_record.decode_error();
            }
            formats = true;
        } else {
            return tls_record.unsupported_extension();
        }
        at = body + size;
    }
    if version == 0 {
        // TLS 1.2. A HelloRetryRequest exists only in 1.3.
        if retry {
            return tls_record.protocol_version();
        }
        // The downgrade sentinels, "DOWNGRD" and 01 or 00 (RFC 8446 §4.1.3):
        // a client that offered TLS 1.3 MUST refuse either in a ServerHello
        // for TLS 1.2 or below. In a TLS 1.3 ServerHello they are random
        // bytes like any others, as OpenSSL takes them
        // (`docs/tls-assurance.md` §4).
        if get(b, 26, 4) == 0x444f574e && get(b, 30, 3) == 0x475244 && int_of(b[33]) <= 1 {
            return tls_record.protocol_version();
        }
        if !tls_record.suite12_known(suite) {
            return tls_record.no_shared_cipher();
        }
        if group != 0 || psk {
            return tls_record.unsupported_extension();
        }
        // Resuming a session needs one; the client offered none, so the
        // server echoing the random session id it sent is not a 1.2
        // session it can have.
        if sid == len(session_id) {
            var same = true;
            var j = 0;
            while j < sid {
                if b[sid_at + j] != session_id[j] {
                    same = false;
                }
                j = j + 1;
            }
            if same {
                return tls_record.decode_error();
            }
        }
        if !ems {
            return tls_record.extended_master_secret();
        }
    } else {
        if !tls_record.suite_known(suite) {
            return tls_record.no_shared_cipher();
        }
        if reneg || ems || formats || sni {
            // TLS 1.2's extensions in a TLS 1.3 ServerHello.
            return tls_record.unsupported_extension();
        }
        if sid != len(session_id) {
            return tls_record.decode_error();
        }
        var j = 0;
        while j < sid {
            if b[sid_at + j] != session_id[j] {
                return tls_record.decode_error();
            }
            j = j + 1;
        }
        if retry {
            // A retry that would change nothing in the ClientHello (§4.1.4).
            if group == 0 && cookie == 0 {
                return tls_record.hello_retry();
            }
        } else if group == 0 && psk {
            // A resumption with no key share is psk_ke, which was not offered.
            return tls_record.key_share();
        } else if group == 0 {
            return tls_record.decode_error();
        }
    }
    info[sh_share()] = share;
    info[sh_retry()] = 0;
    if retry {
        info[sh_retry()] = 1;
    }
    info[sh_suite()] = suite;
    info[sh_group()] = group;
    info[sh_cookie_start()] = cookie;
    info[sh_cookie_end()] = cookie_end;
    info[sh_psk()] = 0;
    if psk {
        info[sh_psk()] = 1;
    }
    info[sh_version()] = 0x0304;
    if version == 0 {
        info[sh_version()] = 0x0303;
    }
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

// `hash_len`: the suite's, 32 or 48.
pub fn finished[&b](b: &b [byte], hash_len: int) -> [] int {
    if len(b) != hash_len {
        return tls_record.decode_error();
    }
    return 0;
}

// ---- After the handshake (§4.6) ----

// What `new_session_ticket` finds, as indices into its `info`.
pub fn nst_lifetime() -> [] int {
    return 0;
}

pub fn nst_age_add() -> [] int {
    return 1;
}

// The nonce's and the ticket's ranges in the body.
pub fn nst_nonce_start() -> [] int {
    return 2;
}

pub fn nst_nonce_end() -> [] int {
    return 3;
}

pub fn nst_ticket_start() -> [] int {
    return 4;
}

pub fn nst_ticket_end() -> [] int {
    return 5;
}

pub fn nst_info_len() -> [] int {
    return 6;
}

// A NewSessionTicket (RFC 8446 §4.6.1): its fields into `info`. Its
// extensions (early_data is the only one defined) are checked for shape
// and not used: this client sends no early data.
pub fn new_session_ticket[&b, &i](b: &b [byte], info: &!i [int]) -> [] int {
    let n = len(b);
    if n < 9 {
        return tls_record.decode_error();
    }
    info[nst_lifetime()] = get(b, 0, 4);
    info[nst_age_add()] = get(b, 4, 4);
    var at = 8;
    info[nst_nonce_start()] = at + 1;
    at = at + 1 + int_of(b[at]);
    info[nst_nonce_end()] = at;
    if at + 2 > n {
        return tls_record.decode_error();
    }
    let ticket = get(b, at, 2);
    if ticket == 0 {
        return tls_record.decode_error();
    }
    info[nst_ticket_start()] = at + 2;
    at = at + 2 + ticket;
    info[nst_ticket_end()] = at;
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

// ---- TLS 1.2 (RFC 5246 §7.4, RFC 8422 §5.4; `docs/tls-parity.md` §3.4) ----

pub fn type_hello_request() -> [] int {
    return 0;
}

pub fn type_server_key_exchange() -> [] int {
    return 12;
}

pub fn type_server_hello_done() -> [] int {
    return 14;
}

pub fn type_client_key_exchange() -> [] int {
    return 16;
}

// A TLS 1.2 Certificate: the chain, with no request context and no
// per-certificate extensions. `info` as `certificate` fills it.
pub fn certificate12[&b, &i](b: &b [byte], info: &!i [int]) -> [] int {
    let n = len(b);
    if n < 3 || 3 + get(b, 0, 3) != n {
        return tls_record.decode_error();
    }
    var at = 3;
    var count = 0;
    while at < n {
        if at + 3 > n {
            return tls_record.decode_error();
        }
        let size = get(b, at, 3);
        if size == 0 || at + 3 + size > n {
            return tls_record.decode_error();
        }
        if count == max_certificates() {
            return tls_record.x509_chain_too_large();
        }
        info[3 + 2 * count] = at + 3;
        info[4 + 2 * count] = at + 3 + size;
        count = count + 1;
        at = at + 3 + size;
    }
    if count == 0 {
        return tls_record.decode_error();
    }
    info[0] = info[3];
    info[1] = info[4];
    info[2] = count;
    return 0;
}

// What `server_key_exchange` finds, as indices into its `info`: the
// named group, the point's range, where the signed parameters end, the
// signature scheme and the signature's range.
pub fn ske_group() -> [] int {
    return 0;
}

pub fn ske_point_start() -> [] int {
    return 1;
}

pub fn ske_point_end() -> [] int {
    return 2;
}

pub fn ske_params_end() -> [] int {
    return 3;
}

pub fn ske_scheme() -> [] int {
    return 4;
}

pub fn ske_sig_start() -> [] int {
    return 5;
}

pub fn ske_info_len() -> [] int {
    return 6;
}

// An ECDHE ServerKeyExchange: a named curve (curve type 3) the client
// offered, its point at that curve's length, and a signature. The
// signature ends the message.
pub fn server_key_exchange[&b, &i](b: &b [byte], info: &!i [int]) -> [] int {
    let n = len(b);
    if n < 4 || int_of(b[0]) != 3 {
        return tls_record.decode_error();
    }
    let group = get(b, 1, 2);
    let size = int_of(b[3]);
    let want = share_len(group);
    if want == 0 {
        return tls_record.key_share();
    }
    if size != want || 4 + size + 4 > n {
        return tls_record.decode_error();
    }
    let params = 4 + size;
    let sig = get(b, params + 2, 2);
    if params + 4 + sig != n || sig == 0 {
        return tls_record.decode_error();
    }
    info[ske_group()] = group;
    info[ske_point_start()] = 4;
    info[ske_point_end()] = params;
    info[ske_params_end()] = params;
    info[ske_scheme()] = get(b, params, 2);
    info[ske_sig_start()] = params + 4;
    return 0;
}

// A TLS 1.2 CertificateRequest, checked for shape: certificate types,
// signature algorithms and authorities. Its content is not used.
pub fn certificate_request12[&b](b: &b [byte]) -> [] int {
    let n = len(b);
    if n < 1 {
        return tls_record.decode_error();
    }
    var at = 1 + int_of(b[0]);
    if int_of(b[0]) == 0 || at + 2 > n {
        return tls_record.decode_error();
    }
    at = at + 2 + get(b, at, 2);
    if at + 2 > n || at + 2 + get(b, at, 2) != n {
        return tls_record.decode_error();
    }
    return 0;
}
