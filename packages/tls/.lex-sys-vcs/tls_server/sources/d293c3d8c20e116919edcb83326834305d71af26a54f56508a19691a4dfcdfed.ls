edition 7;
module tls_server;
import std.crypto;
import std.ecdh;
import std.ecdsa_sign;
import std.hkdf;
import std.hmac;
import std.x25519;
import tls_hello;
import tls_identity;
import tls_message;
import tls_record;
import tls_slot;

// `tls_server` -- one TLS 1.3 server connection: the ClientHello read
// and answered, the choices of `docs/tls-server.md` §5.2 (suite, group or
// one HelloRetryRequest, identity by SNI, ALPN), the server's flight
// signed, and the client's Finished checked. A connection is the same two
// slices as a client's (`tls_slot`), so `tls_client`'s `take`, `send`,
// `recv`, `finish`, `event`, `peer_eof` and `drop` serve it unchanged;
// what is a server's is `start` and `feed`. The configuration, the
// identities and ALPN, is `tls_identity`'s `cfg`, which the engine passes
// to every `feed`. TLS 1.3 only. Not independently reviewed (#209).

// ---- Starting ----

// A connection that waits for a ClientHello. `random` is 96 bytes of the
// engine's DRBG: the ServerHello random, the X25519 secret (and so the
// P-256 or P-384 scalar, `tls_slot.new_ecdh_share`), and the randomness
// the signature is hedged with (`docs/tls-server.md` §3.2).
pub fn start[&i, &b, &r](ints: &!i [int], bytes: &!b [byte], random: &r [byte]) -> [] int {
    if len(ints) < tls_slot.ints_len() || len(bytes) < tls_slot.bytes_len() || len(random) != 96 {
        return tls_record.bad_slot();
    }
    var k = 0;
    while k < tls_slot.ints_len() {
        ints[k] = 0;
        k = k + 1;
    }
    tls_slot.copy_bytes(random[0..32], bytes[tls_slot.k_random()..tls_slot.k_random() + 32]);
    tls_slot.copy_bytes(random[32..64], bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32]);
    tls_slot.copy_bytes(random[64..96], bytes[tls_slot.k_sign_extra()..tls_slot.k_sign_extra() + 32]);
    tls_slot.set_flag(ints, tls_slot.f_server());
    tls_slot.transcript_init(ints);
    ints[tls_slot.i_state()] = tls_slot.state_wait_client_hello();
    return 0;
}

// Whether the connection is still in its handshake: started, and neither
// established nor ended.
pub fn in_handshake[&i](ints: &i [int]) -> [] bool {
    let s = ints[tls_slot.i_state()];
    return s == tls_slot.state_wait_client_hello() || s == tls_slot.state_wait_client_hello2() || s == tls_slot.state_wait_client_finished();
}

// ---- The key schedule (RFC 8446 §7.1) ----

// HMAC(finished_key(secret), transcript hash) into `out`, under the
// suite's hash (`len(secret)` and `len(out)` bytes).
fn finished_mac[&i, &s, &o](ints: &i [int], secret: &s [byte], out: &!o [byte]) -> [] int {
    let h = len(secret);
    region r {
        let key = alloc_slice[r](h, byte_of(0));
        let th = alloc_slice[r](h, byte_of(0));
        hkdf.expand_label(h, secret, "finished", "", key);
        tls_slot.transcript_hash(ints, th);
        hmac.mac(h, key, th, out);
        tls_slot.zero(key);
    }
    return 0;
}

// The handshake secrets from the (EC)DHE `secret`, over the transcript
// through ServerHello, and the master secret. No PSK: the Early Secret is
// HKDF-Extract(0, 0).
fn handshake_secrets[&i, &b, &s](ints: &!i [int], bytes: &!b [byte], secret: &s [byte]) -> [] int {
    let h = tls_slot.hash_len(ints);
    region r {
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
        tls_slot.transcript_hash(ints, th);
        hkdf.derive_secret(h, hs, "c hs traffic", th, bytes[tls_slot.k_client_hs()..tls_slot.k_client_hs() + h]);
        hkdf.derive_secret(h, hs, "s hs traffic", th, bytes[tls_slot.k_server_hs()..tls_slot.k_server_hs() + h]);
        hkdf.derive_secret(h, hs, "derived", empty_hash, derived);
        hkdf.extract(h, derived, zeros, bytes[tls_slot.k_master()..tls_slot.k_master() + h]);
        tls_slot.zero(hs);
        tls_slot.zero(early);
        tls_slot.zero(derived);
    }
    return 0;
}

// ---- The signature (`docs/tls-server.md` §3) ----

// The one place the signer is called: `digest` (SHA-256 of
// CertificateVerify's content) signed with identity `id`'s key, hedged
// with the slot's randomness, verified under the identity's public point
// before it is used (§3.3), and written into `der` as DER. Answers the
// DER's length, or `server_sign_check`.
fn sign[&i, &b, &c, &d, &o](ints: &!i [int], bytes: &b [byte], cfg: &c [byte], id: int, digest: &d [byte], der: &!o [byte]) -> [] int {
    var n = tls_record.server_sign_check();
    region r {
        let sig = alloc_slice[r](64, byte_of(0));
        // `std.ecdh`'s work, which the key exchange has finished with, is
        // the signer's (`ecdsa_sign.work_len()` is the same).
        let code = ecdsa_sign.sign_checked(digest, tls_identity.key(cfg, id), tls_identity.point(cfg, id), bytes[tls_slot.k_sign_extra()..tls_slot.k_sign_extra() + 32], sig, ints[tls_slot.i_ecdh_work()..tls_slot.i_ecdh_work() + ecdh.work_len()]);
        if code == 0 {
            n = ecdsa_sign.to_der(sig, der);
            if n < 0 {
                n = tls_record.server_sign_check();
            }
        }
    }
    return n;
}

// CertificateVerify for the transcript so far (RFC 8446 §4.4.3) into
// `out`: its length, or a refusal.
fn certificate_verify[&i, &b, &c, &o](ints: &!i [int], bytes: &b [byte], cfg: &c [byte], id: int, out: &!o [byte]) -> [] int {
    var n = 0;
    region r {
        // 64 spaces, the context string, a zero byte, the hash.
        let h = tls_slot.hash_len(ints);
        let content = alloc_slice[r](64 + 33 + 1 + h, byte_of(32));
        let label = "TLS 1.3, server CertificateVerify";
        tls_slot.copy_bytes(label, content[64..97]);
        content[97] = byte_of(0);
        tls_slot.transcript_hash(ints, content[98..98 + h]);
        let digest = alloc_slice[r](32, byte_of(0));
        crypto.sha256(content, digest);
        let der = alloc_slice[r](ecdsa_sign.der_max(), byte_of(0));
        n = sign(ints, bytes, cfg, id, digest, der);
        if n > 0 {
            n = tls_hello.certificate_verify(der[0..n], out);
        }
    }
    return n;
}

// ---- The ClientHello (`docs/tls-server.md` §5.2) ----

fn lower(c: int) -> [] int {
    if c >= 65 && c <= 90 {
        return c + 32;
    }
    return c;
}

fn same[&x, &y](a: &x [byte], b: &y [byte]) -> [] bool {
    if len(a) != len(b) {
        return false;
    }
    var k = 0;
    while k < len(a) {
        if a[k] != b[k] {
            return false;
        }
        k = k + 1;
    }
    return true;
}

// Queues `content`, handshake messages, in records of at most 2^14 bytes.
fn queue_handshake[&i, &b, &c](ints: &!i [int], bytes: &!b [byte], content: &c [byte]) -> [] int {
    var at = 0;
    var code = 0;
    while code == 0 && at < len(content) {
        var n = len(content) - at;
        if n > tls_record.max_plaintext() {
            n = tls_record.max_plaintext();
        }
        code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), content[at..at + n]);
        at = at + n;
    }
    return code;
}

// The middlebox change_cipher_spec (RFC 8446 Appendix D.4), once, after
// the server's first ServerHello or HelloRetryRequest.
fn queue_ccs[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    if tls_slot.has(ints, tls_slot.f_ccs_sent()) {
        return 0;
    }
    tls_slot.set_flag(ints, tls_slot.f_ccs_sent());
    var code = 0;
    region r {
        let ccs = alloc_slice[r](1, byte_of(1));
        code = tls_slot.queue_record(ints, bytes, tls_record.type_change_cipher_spec(), ccs);
    }
    return code;
}

// A HelloRetryRequest for `group` under `suite` (§4.1.4): the transcript
// restarts from message_hash(ClientHello1), and the server waits for the
// second ClientHello. Stateful: the suite and group stay in the slot.
fn retry[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte], suite: int, group: int) -> [] int {
    tls_slot.set_flag(ints, tls_slot.f_retried());
    ints[tls_slot.i_suite()] = suite;
    ints[tls_slot.i_group()] = group;
    let h = tls_slot.hash_len(ints);
    var code = 0;
    region r {
        tls_slot.transcript_add(ints, message);
        let synthetic = alloc_slice[r](4 + h, byte_of(0));
        synthetic[0] = byte_of(254);
        synthetic[3] = byte_of(h);
        tls_slot.transcript_hash(ints, synthetic[4..4 + h]);
        tls_slot.transcript_init(ints);
        tls_slot.transcript_add(ints, synthetic);
        let hrr = alloc_slice[r](128, byte_of(0));
        let sid = bytes[tls_slot.k_session_id()..tls_slot.k_session_id() + ints[tls_slot.i_session_len()]];
        let n = tls_hello.hello_retry(sid, suite, group, hrr);
        tls_slot.transcript_add(ints, hrr[0..n]);
        code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), hrr[0..n]);
        if code == 0 {
            code = queue_ccs(ints, bytes);
        }
    }
    ints[tls_slot.i_state()] = tls_slot.state_wait_client_hello2();
    return code;
}

// The name the client sent, lowercased into the slot, and the identity
// it chooses (`docs/tls-server.md` §7: a name no identity has gets the
// default, never an error that tells names apart).
fn choose_identity[&i, &b, &m, &f, &c](ints: &!i [int], bytes: &!b [byte], body: &m [byte], info: &f [int], cfg: &c [byte]) -> [] int {
    let n = info[tls_hello.ch_sni_len()];
    let s = info[tls_hello.ch_sni_start()];
    var k = 0;
    while k < n {
        bytes[tls_slot.b_sni() + k] = byte_of(lower(int_of(body[s + k])));
        k = k + 1;
    }
    ints[tls_slot.i_sni_len()] = n;
    var id = tls_identity.select(cfg, bytes[tls_slot.b_sni()..tls_slot.b_sni() + n]);
    if id >= 0 {
        tls_slot.set_flag(ints, tls_slot.f_sni_used());
    } else {
        id = 0;
    }
    ints[tls_slot.i_identity()] = id;
    return id;
}

// ALPN (RFC 7301 §3.2): the first of the server's protocols the client
// offers, kept in the slot. 0, or `server_alpn` when the client offers
// some and none is the server's. A server with no list ignores ALPN.
fn choose_alpn[&i, &b, &m, &f, &c](ints: &!i [int], bytes: &!b [byte], body: &m [byte], info: &f [int], cfg: &c [byte]) -> [] int {
    let ours = tls_identity.alpn_list(cfg);
    let start = info[tls_hello.ch_alpn_start()];
    if len(ours) == 0 || start == 0 {
        return 0;
    }
    let pick = tls_hello.choose_alpn(ours, body[start..info[tls_hello.ch_alpn_end()]]);
    if pick < 0 {
        return tls_record.server_alpn();
    }
    let m = int_of(ours[pick]);
    tls_slot.copy_bytes(ours[pick + 1..pick + 1 + m], bytes[tls_slot.b_alpn()..tls_slot.b_alpn() + m]);
    ints[tls_slot.i_alpn_len()] = m;
    return 0;
}

// The shared secret with the client's `peer` share of the slot's group,
// and the server's own share into `share`. `tls-key-share` for a share
// that is not a point, or gives X25519's all-zero secret.
fn key_exchange[&i, &b, &p, &s, &o](ints: &!i [int], bytes: &!b [byte], peer: &p [byte], share: &!s [byte], secret: &!o [byte]) -> [] int {
    let curve = tls_slot.curve_of(ints[tls_slot.i_group()]);
    if curve == 0 {
        x25519.public_key(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], share);
        if x25519.scalarmult(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], peer, secret) != 0 {
            return tls_record.key_share();
        }
        return 0;
    }
    let code = tls_slot.new_ecdh_share(ints, bytes, curve, share);
    if code != 0 {
        return code;
    }
    let size = curve / 8;
    if ecdh.shared(curve, bytes[tls_slot.k_ecdh()..tls_slot.k_ecdh() + size], peer, secret, ints[tls_slot.i_ecdh_work()..tls_slot.i_ecdh_work() + ecdh.work_len()]) != 0 {
        return tls_record.key_share();
    }
    return 0;
}

// The answer to the ClientHello `message` that is final (the first, with
// a share this server takes, or the second): ServerHello,
// change_cipher_spec, then under the handshake key EncryptedExtensions,
// Certificate, CertificateVerify and Finished, in one flight
// (`docs/tls-server.md` §5.2, step 3). The application keys follow, and
// the server waits for the client's Finished.
fn respond[&i, &b, &m, &f, &c](ints: &!i [int], bytes: &!b [byte], message: &m [byte], info: &f [int], cfg: &c [byte]) -> [] int {
    let body = message[4..len(message)];
    let group = ints[tls_slot.i_group()];
    let id = choose_identity(ints, bytes, body, info, cfg);
    if !tls_identity.in_use(cfg, id) {
        return tls_record.server_no_identity();
    }
    var code = choose_alpn(ints, bytes, body, info, cfg);
    if code != 0 {
        return code;
    }
    tls_slot.transcript_add(ints, message);
    let h = tls_slot.hash_len(ints);
    region r {
        let at = tls_hello.share_at(info, group);
        let size = tls_message.share_len(group);
        let share = alloc_slice[r](size, byte_of(0));
        var secret_len = 32;
        if group == tls_message.group_p384() {
            secret_len = 48;
        }
        let secret = alloc_slice[r](secret_len, byte_of(0));
        code = key_exchange(ints, bytes, body[at..at + size], share, secret);
        if code == 0 {
            let hello = alloc_slice[r](256, byte_of(0));
            let sid = bytes[tls_slot.k_session_id()..tls_slot.k_session_id() + ints[tls_slot.i_session_len()]];
            let n = tls_hello.server_hello(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], sid, ints[tls_slot.i_suite()], group, share, hello);
            tls_slot.transcript_add(ints, hello[0..n]);
            code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), hello[0..n]);
            if code == 0 {
                code = queue_ccs(ints, bytes);
            }
            handshake_secrets(ints, bytes, secret);
        }
        tls_slot.zero(secret);
        // The key exchange's secrets have done their work.
        tls_slot.zero(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 80]);
    }
    if code != 0 {
        return code;
    }
    tls_slot.set_write_keys(ints, bytes, tls_slot.k_server_hs());
    region q {
        // The encrypted flight: at most a 6 + 4 + 3 + 255-byte
        // EncryptedExtensions, the Certificate, a CertificateVerify and
        // a Finished.
        let flight = alloc_slice[q](tls_identity.chain_cap() + 1024, byte_of(0));
        var n = 0;
        let used = tls_slot.has(ints, tls_slot.f_sni_used());
        n = tls_hello.encrypted_extensions(used, bytes[tls_slot.b_alpn()..tls_slot.b_alpn() + ints[tls_slot.i_alpn_len()]], flight);
        tls_slot.transcript_add(ints, flight[0..n]);
        let c = n + tls_hello.certificate(tls_identity.list(cfg, id), flight[n..len(flight)]);
        tls_slot.transcript_add(ints, flight[n..c]);
        let v = certificate_verify(ints, bytes, cfg, id, flight[c..len(flight)]);
        if v < 0 {
            code = v;
        } else {
            tls_slot.transcript_add(ints, flight[c..c + v]);
            let f = c + v;
            flight[f] = byte_of(tls_message.type_finished());
            flight[f + 3] = byte_of(h);
            finished_mac(ints, bytes[tls_slot.k_server_hs()..tls_slot.k_server_hs() + h], flight[f + 4..f + 4 + h]);
            tls_slot.transcript_add(ints, flight[f..f + 4 + h]);
            code = queue_handshake(ints, bytes, flight[0..f + 4 + h]);
        }
        tls_slot.zero(bytes[tls_slot.k_sign_extra()..tls_slot.k_sign_extra() + 32]);
    }
    if code != 0 {
        return code;
    }
    // The application secrets, over the transcript through the server's
    // Finished; the server writes under its own from here, though `send`
    // waits for the client's Finished.
    region s {
        let th = alloc_slice[s](h, byte_of(0));
        tls_slot.transcript_hash(ints, th);
        hkdf.derive_secret(h, bytes[tls_slot.k_master()..tls_slot.k_master() + h], "c ap traffic", th, bytes[tls_slot.k_client_ap()..tls_slot.k_client_ap() + h]);
        hkdf.derive_secret(h, bytes[tls_slot.k_master()..tls_slot.k_master() + h], "s ap traffic", th, bytes[tls_slot.k_server_ap()..tls_slot.k_server_ap() + h]);
    }
    tls_slot.set_write_keys(ints, bytes, tls_slot.k_server_ap());
    tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_hs());
    ints[tls_slot.i_state()] = tls_slot.state_wait_client_finished();
    return 0;
}

// A ClientHello, the first or the one after a HelloRetryRequest.
fn on_client_hello[&i, &b, &m, &c](ints: &!i [int], bytes: &!b [byte], message: &m [byte], cfg: &c [byte]) -> [] int {
    let body = message[4..len(message)];
    let second = ints[tls_slot.i_state()] == tls_slot.state_wait_client_hello2();
    var code = 0;
    region r {
        let info = alloc_slice[r](tls_hello.ch_info_len(), 0);
        code = tls_hello.client_hello(body, info);
        if code == 0 && second {
            // RFC 8446 §4.1.2: the same ClientHello but for the share, so
            // the same session id and the suite the retry named, a share
            // of the group it named, and no early data.
            let sid = info[tls_hello.ch_session_start()];
            let n = info[tls_hello.ch_session_len()];
            if n != ints[tls_slot.i_session_len()] || !same(body[sid..sid + n], bytes[tls_slot.k_session_id()..tls_slot.k_session_id() + n]) {
                code = tls_record.server_retry_share();
            } else if info[tls_hello.ch_suites()] & tls_hello.suite_bit(ints[tls_slot.i_suite()]) == 0 || info[tls_hello.ch_early()] != 0 {
                code = tls_record.server_retry_share();
            } else if tls_hello.share_at(info, ints[tls_slot.i_group()]) == 0 {
                code = tls_record.server_retry_share();
            }
        }
        if code == 0 && second {
            code = respond(ints, bytes, message, info, cfg);
        } else if code == 0 {
            let sid = info[tls_hello.ch_session_start()];
            let n = info[tls_hello.ch_session_len()];
            tls_slot.copy_bytes(body[sid..sid + n], bytes[tls_slot.k_session_id()..tls_slot.k_session_id() + n]);
            ints[tls_slot.i_session_len()] = n;
            if info[tls_hello.ch_early()] != 0 {
                tls_slot.set_flag(ints, tls_slot.f_early());
            }
            let suite = tls_hello.choose_suite(info[tls_hello.ch_suites()], hw_aes_gcm());
            ints[tls_slot.i_suite()] = suite;
            let group = tls_hello.share_group(info);
            if group == 0 {
                code = retry(ints, bytes, message, suite, tls_hello.retry_group(info));
            } else {
                ints[tls_slot.i_group()] = group;
                code = respond(ints, bytes, message, info, cfg);
            }
        }
    }
    return code;
}

// The client's Finished, compared in constant time with the MAC expected
// over the transcript through the server's Finished; then the client's
// application key, and the connection is established.
fn on_client_finished[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    let h = tls_slot.hash_len(ints);
    var code = tls_message.finished(message[4..len(message)], h);
    region r {
        let want = alloc_slice[r](h, byte_of(0));
        if code == 0 {
            finished_mac(ints, bytes[tls_slot.k_client_hs()..tls_slot.k_client_hs() + h], want);
            // Every byte is compared, whatever the earlier ones were.
            var diff = 0;
            var k = 0;
            while k < h {
                diff = diff | int_of(want[k]) ^ int_of(message[4 + k]);
                k = k + 1;
            }
            if diff != 0 {
                code = tls_record.server_finished();
            }
        }
        tls_slot.zero(want);
    }
    if code == 0 {
        tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_ap());
        // The handshake secrets and the master secret have done their
        // work; the application secrets stay for KeyUpdate.
        tls_slot.zero(bytes[tls_slot.k_client_hs()..tls_slot.k_client_hs() + 96]);
        tls_slot.zero(bytes[tls_slot.k_master()..tls_slot.k_master() + 48]);
        ints[tls_slot.i_state()] = tls_slot.state_connected();
    }
    return code;
}

// A KeyUpdate from the client: its next secret, and an answer when one is
// asked for (RFC 8446 §4.6.3), at most 32 on a connection
// (`docs/tls-pure.md` §7.1).
fn on_key_update[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    let asked = tls_message.key_update(message[4..len(message)]);
    if asked < 0 {
        return asked;
    }
    ints[tls_slot.i_key_updates()] = ints[tls_slot.i_key_updates()] + 1;
    if ints[tls_slot.i_key_updates()] > 32 {
        return tls_record.too_many_messages();
    }
    var code = 0;
    region r {
        let h = tls_slot.hash_len(ints);
        let next = alloc_slice[r](h, byte_of(0));
        hkdf.expand_label(h, bytes[tls_slot.k_client_ap()..tls_slot.k_client_ap() + h], "traffic upd", "", next);
        tls_slot.copy_bytes(next, bytes[tls_slot.k_client_ap()..tls_slot.k_client_ap() + h]);
        tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_ap());
        if asked == 1 {
            let answer = alloc_slice[r](5, byte_of(0));
            answer[0] = byte_of(tls_message.type_key_update());
            answer[3] = byte_of(1);
            code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), answer);
            hkdf.expand_label(h, bytes[tls_slot.k_server_ap()..tls_slot.k_server_ap() + h], "traffic upd", "", next);
            tls_slot.copy_bytes(next, bytes[tls_slot.k_server_ap()..tls_slot.k_server_ap() + h]);
            tls_slot.set_write_keys(ints, bytes, tls_slot.k_server_ap());
        }
        tls_slot.zero(next);
    }
    return code;
}

// One whole handshake message, header included, in the current state.
fn on_message[&i, &b, &m, &c](ints: &!i [int], bytes: &!b [byte], message: &m [byte], cfg: &c [byte]) -> [] int {
    let kind = int_of(message[0]);
    let state = ints[tls_slot.i_state()];
    let hello = state == tls_slot.state_wait_client_hello() || state == tls_slot.state_wait_client_hello2();
    if hello && kind == tls_message.type_client_hello() {
        return on_client_hello(ints, bytes, message, cfg);
    }
    if state == tls_slot.state_wait_client_finished() && kind == tls_message.type_finished() {
        return on_client_finished(ints, bytes, message);
    }
    if state == tls_slot.state_connected() && kind == tls_message.type_key_update() {
        return on_key_update(ints, bytes, message);
    }
    return tls_record.unexpected_message();
}

// Appends handshake bytes and handles every whole message they finish.
// A ClientHello is bounded at `tls_hello.max_client_hello()` as soon as
// its header is in; anything else at the slot's 64 KiB.
fn on_handshake_bytes[&i, &b, &d, &c](ints: &!i [int], bytes: &!b [byte], content: &d [byte], cfg: &c [byte]) -> [] int {
    let fill = ints[tls_slot.i_hs_fill()];
    if fill + len(content) > tls_slot.hs_cap() {
        return tls_record.record_overflow();
    }
    tls_slot.copy_bytes(content, bytes[tls_slot.b_hs() + fill..tls_slot.b_hs() + fill + len(content)]);
    ints[tls_slot.i_hs_fill()] = fill + len(content);
    var code = 0;
    var at = 0;
    var going = true;
    while going && code == 0 {
        let have = ints[tls_slot.i_hs_fill()] - at;
        let state = ints[tls_slot.i_state()];
        let hello = state == tls_slot.state_wait_client_hello() || state == tls_slot.state_wait_client_hello2();
        if have < 4 {
            going = false;
        } else {
            let p = tls_slot.b_hs() + at;
            let n = int_of(bytes[p + 1]) * 65536 + int_of(bytes[p + 2]) * 256 + int_of(bytes[p + 3]);
            if hello && int_of(bytes[p]) != tls_message.type_client_hello() {
                code = tls_record.unexpected_message();
            } else if hello && n > tls_hello.max_client_hello() {
                code = tls_record.server_client_hello_length();
            } else if 4 + n > tls_slot.hs_cap() {
                code = tls_record.record_overflow();
            } else if have < 4 + n {
                going = false;
            } else if have > 4 + n {
                // Every message a server takes is followed by a new key or
                // a new flight: a ClientHello, a Finished, a KeyUpdate. So
                // none may share a record with what follows it (RFC 8446
                // §5.1), and that is refused before the message is
                // handled, so nothing is sent for it.
                code = tls_record.unexpected_message();
            } else {
                region r {
                    // A message is handled from a copy, so the handler may
                    // write to the slot's bytes freely.
                    let message = alloc_slice[r](4 + n, byte_of(0));
                    tls_slot.copy_bytes(bytes[p..p + 4 + n], message);
                    code = on_message(ints, bytes, message, cfg);
                    at = at + 4 + n;
                }
            }
        }
    }
    let left = ints[tls_slot.i_hs_fill()] - at;
    var k = 0;
    while k < left {
        bytes[tls_slot.b_hs() + k] = bytes[tls_slot.b_hs() + at + k];
        k = k + 1;
    }
    ints[tls_slot.i_hs_fill()] = left;
    return code;
}

// An alert, as the client reads one (`tls_client`): close_notify is a
// clean end only once established.
fn on_alert[&i, &b, &c](ints: &!i [int], bytes: &!b [byte], content: &c [byte]) -> [] int {
    if len(content) != 2 {
        return tls_record.decode_error();
    }
    let level = int_of(content[0]);
    let what = int_of(content[1]);
    if what == 0 {
        if ints[tls_slot.i_state()] != tls_slot.state_connected() {
            return tls_record.peer_closed();
        }
        tls_slot.set_flag(ints, tls_slot.f_close_received());
        if tls_slot.has(ints, tls_slot.f_close_sent()) {
            tls_slot.forget(ints, bytes);
        }
        return 0;
    }
    if what == 90 {
        ints[tls_slot.i_warnings()] = ints[tls_slot.i_warnings()] + 1;
        if ints[tls_slot.i_warnings()] > 16 {
            return tls_record.too_many_messages();
        }
        return 0;
    }
    ints[tls_slot.i_alert()] = what;
    if level != 1 && level != 2 {
        return tls_record.decode_error();
    }
    return tls_record.alert_received();
}

// A record of early data, skipped (RFC 8446 §4.2.10): `body` bytes more,
// at most 16 KiB in all.
fn skip_early[&i](ints: &!i [int], body: int) -> [] int {
    ints[tls_slot.i_early_skipped()] = ints[tls_slot.i_early_skipped()] + body;
    if ints[tls_slot.i_early_skipped()] > 16384 {
        return tls_record.server_early_data_size();
    }
    return 0;
}

// One whole record from `bytes[tls_slot.b_in()..]`, `n` bytes long.
fn on_record[&i, &b, &c](ints: &!i [int], bytes: &!b [byte], n: int, cfg: &c [byte]) -> [] int {
    let kind = int_of(bytes[tls_slot.b_in()]);
    let body = n - 5;
    let state = ints[tls_slot.i_state()];
    if kind == tls_record.type_change_cipher_spec() {
        // One, exactly `01`, after the first ClientHello and before the
        // client's Finished (RFC 8446 Appendix D.4).
        let between = state == tls_slot.state_wait_client_hello2() || state == tls_slot.state_wait_client_finished();
        if body != 1 || int_of(bytes[tls_slot.b_in() + 5]) != 1 || tls_slot.has(ints, tls_slot.f_ccs_seen()) || !between {
            return tls_record.unexpected_message();
        }
        tls_slot.set_flag(ints, tls_slot.f_ccs_seen());
        return 0;
    }
    let plain_alert = kind == tls_record.type_alert() && state == tls_slot.state_wait_client_finished();
    if !tls_slot.has(ints, tls_slot.f_read_protected()) || plain_alert {
        if kind == tls_record.type_application_data() && tls_slot.has(ints, tls_slot.f_early()) && state == tls_slot.state_wait_client_hello2() {
            // Early data, before a second ClientHello.
            return skip_early(ints, body);
        }
        if body > tls_record.max_plaintext() {
            return tls_record.record_overflow();
        }
        if kind != tls_record.type_handshake() && kind != tls_record.type_alert() {
            return tls_record.unexpected_message();
        }
        var plain = 0;
        region r {
            let content = alloc_slice[r](body, byte_of(0));
            tls_slot.copy_bytes(bytes[tls_slot.b_in() + 5..tls_slot.b_in() + n], content);
            if kind == tls_record.type_handshake() && !plain_alert {
                plain = on_handshake_bytes(ints, bytes, content, cfg);
            } else {
                // A client that refuses the server's flight before it has
                // the handshake key alerts in plaintext.
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
        code = tls_record.open(ints[tls_slot.i_suite()], bytes[tls_slot.k_read_key()..tls_slot.k_read_key() + tls_slot.key_len(ints)], tls_slot.read_aead(ints), bytes[tls_slot.k_read_hw()..tls_slot.k_read_hw() + tls_record.hw_len()], bytes[tls_slot.k_read_iv()..tls_slot.k_read_iv() + 12], ints[tls_slot.i_read_seq()], bytes[tls_slot.b_in()..tls_slot.b_in() + n], bytes[tls_slot.b_plain()..tls_slot.b_plain() + tls_slot.plain_cap()], info);
        inner = info[0];
        size = info[1];
    }
    if code == tls_record.bad_record_mac() && tls_slot.has(ints, tls_slot.f_early()) {
        // Early data under a key this server never derives: skipped.
        return skip_early(ints, body);
    }
    if code != 0 {
        return code;
    }
    // The first record that opens ends the early data.
    tls_slot.clear_flag(ints, tls_slot.f_early());
    ints[tls_slot.i_read_seq()] = ints[tls_slot.i_read_seq()] + 1;
    region r {
        let content = alloc_slice[r](size, byte_of(0));
        tls_slot.copy_bytes(bytes[tls_slot.b_plain()..tls_slot.b_plain() + size], content);
        if inner == tls_record.type_handshake() {
            if size == 0 {
                code = tls_record.unexpected_message();
            } else {
                code = on_handshake_bytes(ints, bytes, content, cfg);
            }
        } else if inner == tls_record.type_alert() {
            code = on_alert(ints, bytes, content);
        } else if inner == tls_record.type_application_data() {
            if state != tls_slot.state_connected() || ints[tls_slot.i_hs_fill()] != 0 {
                code = tls_record.unexpected_message();
            } else {
                let e = ints[tls_slot.i_recv_end()];
                tls_slot.copy_bytes(content, bytes[tls_slot.b_recv() + e..tls_slot.b_recv() + e + size]);
                ints[tls_slot.i_recv_end()] = e + size;
            }
        } else {
            code = tls_record.unexpected_message();
        }
    }
    return code;
}

fn compact_recv[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    let s = ints[tls_slot.i_recv_start()];
    let e = ints[tls_slot.i_recv_end()];
    if s > 0 {
        var k = 0;
        while k < e - s {
            bytes[tls_slot.b_recv() + k] = bytes[tls_slot.b_recv() + s + k];
            k = k + 1;
        }
        ints[tls_slot.i_recv_start()] = 0;
        ints[tls_slot.i_recv_end()] = e - s;
    }
    return 0;
}

// ---- The interface ----

// Bytes the socket gave, as `tls_client.feed`: how many were consumed
// (all of them, unless output or received data must be taken first), or
// the connection's failure. `cfg` is the engine's `tls_identity` slice.
pub fn feed[&i, &b, &d, &c](ints: &!i [int], bytes: &!b [byte], data: &d [byte], cfg: &c [byte]) -> [] int {
    var consumed = 0;
    var code = 0;
    while consumed < len(data) && code == 0 && ints[tls_slot.i_state()] != tls_slot.state_failed() && !tls_slot.has(ints, tls_slot.f_close_received()) {
        compact_recv(ints, bytes);
        if tls_slot.out_free(ints) < 1024 || tls_slot.recv_cap() - ints[tls_slot.i_recv_end()] < tls_record.max_plaintext() {
            return consumed;
        }
        let fill = ints[tls_slot.i_in_fill()];
        var need = 5 - fill;
        if fill >= 5 {
            need = 5 + int_of(bytes[tls_slot.b_in() + 3]) * 256 + int_of(bytes[tls_slot.b_in() + 4]) - fill;
        }
        var take = need;
        if take > len(data) - consumed {
            take = len(data) - consumed;
        }
        tls_slot.copy_bytes(data[consumed..consumed + take], bytes[tls_slot.b_in() + fill..tls_slot.b_in() + fill + take]);
        consumed = consumed + take;
        ints[tls_slot.i_in_fill()] = fill + take;
        if fill < 5 && fill + take == 5 {
            // A header is complete: check it before waiting for its body.
            let check = tls_record.record_length(bytes[tls_slot.b_in()..tls_slot.b_in() + 5], 0, 5);
            if check < 0 {
                code = check;
            }
        }
        if code == 0 && ints[tls_slot.i_in_fill()] >= 5 {
            let total = 5 + int_of(bytes[tls_slot.b_in() + 3]) * 256 + int_of(bytes[tls_slot.b_in() + 4]);
            if ints[tls_slot.i_in_fill()] == total {
                ints[tls_slot.i_in_fill()] = 0;
                code = on_record(ints, bytes, total, cfg);
            }
        }
    }
    if code != 0 {
        return tls_slot.fail(ints, bytes, code);
    }
    if ints[tls_slot.i_state()] == tls_slot.state_failed() {
        return ints[tls_slot.i_failure()];
    }
    return consumed;
}
