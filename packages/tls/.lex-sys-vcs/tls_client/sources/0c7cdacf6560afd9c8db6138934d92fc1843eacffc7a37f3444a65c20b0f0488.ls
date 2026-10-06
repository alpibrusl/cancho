module tls_client;
import std.crypto;
import std.ecdh;
import std.hkdf;
import std.hmac;
import std.x25519;
import tls_client12;
import tls_message;
import tls_slot;
import tls_record;

// `tls_client` -- one TLS 1.3 client connection: the state machine, the
// transcript and key schedule, certificate verification and signature checks,
// alerts, and application data (`docs/tls-core.md` §3 to §5). A
// connection is two caller-owned slices, `ints` (`tls_slot.ints_len()` words) and
// `bytes` (`tls_slot.bytes_len()` bytes), so an engine can keep many in two boxes
// (`docs/tls-pure.md` §2.1). Nothing is allocated from a size the peer
// names. Not independently reviewed (#209).

// ---- What other modules ask of a connection ----

pub fn ints_len() -> [] int {
    return tls_slot.ints_len();
}

pub fn bytes_len() -> [] int {
    return tls_slot.bytes_len();
}

pub fn event_want_read() -> [] int {
    return tls_slot.event_want_read();
}

pub fn event_want_write() -> [] int {
    return tls_slot.event_want_write();
}

pub fn event_established() -> [] int {
    return tls_slot.event_established();
}

pub fn event_closed() -> [] int {
    return tls_slot.event_closed();
}

pub fn event_failed() -> [] int {
    return tls_slot.event_failed();
}

// ---- Starting ----

// Starts the handshake: `host` (at most 255 bytes) is the server's name;
// `random` is 96 bytes of the caller's entropy: the ClientHello random,
// the legacy session id and the X25519 secret. `now` is the time in
// seconds since 1970, against which the server's certificates are
// checked. The ClientHello is queued for `take`.
pub fn start[&i, &b, &h, &r](ints: &!i [int], bytes: &!b [byte], host: &h [byte], random: &r [byte], now: int) -> [] int {
    return start_psk(ints, bytes, host, random, now, host[0..0], host[0..0], 0, 0, 0, false);
}

// `start`, offering `ticket` for resumption (`docs/tls-resumption.md`):
// `psk` is its PSK (32 or 48 bytes, the hash it was made under), `age` its
// obfuscated_ticket_age, and `verified_at` and `not_after` what the
// connection that issued it knew of the server's identity, which a
// resumed connection inherits. An empty `ticket` is `start`. Whether the
// ticket may be offered at all (`docs/tls-resumption.md` §3) is the
// engine's to decide; this only sends it. `advertise` says in the
// ClientHello that the client can resume, so a server may send it tickets
// (implied by a ticket offered).
pub fn start_psk[&i, &b, &h, &r, &t, &k](ints: &!i [int], bytes: &!b [byte], host: &h [byte], random: &r [byte], now: int, ticket: &t [byte], psk: &k [byte], age: int, verified_at: int, not_after: int, advertise: bool) -> [] int {
    if len(ints) < tls_slot.ints_len() || len(bytes) < tls_slot.bytes_len() || len(random) != 96 || len(host) > 255 {
        return tls_record.bad_slot();
    }
    if len(ticket) > tls_slot.ticket_cap() || len(ticket) > 0 && len(psk) != 32 && len(psk) != 48 {
        return tls_record.bad_slot();
    }
    var k = 0;
    while k < tls_slot.ints_len() {
        ints[k] = 0;
        k = k + 1;
    }
    tls_slot.transcript_init(ints);
    tls_slot.copy_bytes(random[0..32], bytes[tls_slot.k_random()..tls_slot.k_random() + 32]);
    tls_slot.copy_bytes(random[32..64], bytes[tls_slot.k_session_id()..tls_slot.k_session_id() + 32]);
    tls_slot.copy_bytes(random[64..96], bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32]);
    tls_slot.copy_bytes(host, bytes[tls_slot.k_host()..tls_slot.k_host() + len(host)]);
    ints[tls_slot.i_host_len()] = len(host);
    ints[tls_slot.i_now()] = now;
    ints[tls_slot.i_group()] = tls_message.group_x25519();
    if advertise {
        tls_slot.set_flag(ints, tls_slot.f_advertise());
    }
    if len(ticket) > 0 {
        tls_slot.copy_bytes(ticket, bytes[tls_slot.b_offer()..tls_slot.b_offer() + len(ticket)]);
        tls_slot.copy_bytes(psk, bytes[tls_slot.k_offer_psk()..tls_slot.k_offer_psk() + len(psk)]);
        ints[tls_slot.i_offer_len()] = len(ticket);
        ints[tls_slot.i_offer_age()] = age;
        ints[tls_slot.i_offer_hash()] = len(psk);
        ints[tls_slot.i_verified_at()] = verified_at;
        ints[tls_slot.i_not_after()] = not_after;
        tls_slot.set_flag(ints, tls_slot.f_psk_offered());
    }
    var code = 0;
    region r {
        let share = alloc_slice[r](32, byte_of(0));
        x25519.public_key(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], share);
        code = send_client_hello(ints, bytes, share, share[0..0]);
    }
    ints[tls_slot.i_state()] = tls_slot.state_wait_server_hello();
    return code;
}

// A ClientHello with one share of `ints[tls_slot.i_group()]` and `cookie`, added
// to the transcript and queued.
fn send_client_hello[&i, &b, &s, &c](ints: &!i [int], bytes: &!b [byte], share: &s [byte], cookie: &c [byte]) -> [] int {
    var code = 0;
    region r {
        let hello = alloc_slice[r](tls_message.max_client_hello(), byte_of(0));
        var tn = 0;
        var h = 32;
        if tls_slot.has(ints, tls_slot.f_psk_offered()) {
            tn = ints[tls_slot.i_offer_len()];
            h = ints[tls_slot.i_offer_hash()];
        }
        let n = tls_message.client_hello(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], bytes[tls_slot.k_session_id()..tls_slot.k_session_id() + 32], ints[tls_slot.i_group()], share, cookie, bytes[tls_slot.k_host()..tls_slot.k_host() + ints[tls_slot.i_host_len()]], tls_slot.has(ints, tls_slot.f_advertise()), bytes[tls_slot.b_offer()..tls_slot.b_offer() + tn], ints[tls_slot.i_offer_age()], h, hello);
        if tn > 0 {
            binder(ints, bytes, hello[0..n]);
        }
        tls_slot.transcript_add(ints, hello[0..n]);
        code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), hello[0..n]);
    }
    return code;
}

// The binder of the ticket offered, written into the last bytes of
// `hello` (RFC 8446 §4.2.11.2): HMAC under the binder key's finished key,
// over the transcript so far and `hello` truncated before its binders.
fn binder[&i, &b, &o](ints: &i [int], bytes: &b [byte], hello: &!o [byte]) -> [] int {
    let h = ints[tls_slot.i_offer_hash()];
    let n = len(hello);
    region r {
        let early = alloc_slice[r](h, byte_of(0));
        let binder_key = alloc_slice[r](h, byte_of(0));
        let finished_key = alloc_slice[r](h, byte_of(0));
        let th = alloc_slice[r](h, byte_of(0));
        early_secret(h, bytes[tls_slot.k_offer_psk()..tls_slot.k_offer_psk() + h], early);
        let empty_hash = alloc_slice[r](h, byte_of(0));
        hash_of_nothing(empty_hash);
        hkdf.derive_secret(h, early, "res binder", empty_hash, binder_key);
        hkdf.expand_label(h, binder_key, "finished", "", finished_key);
        tls_slot.transcript_hash_with(ints, hello[0..n - tls_message.binders_len(h)], th);
        hmac.mac(h, finished_key, th, hello[n - h..n]);
        tls_slot.zero(early);
        tls_slot.zero(binder_key);
        tls_slot.zero(finished_key);
    }
    return 0;
}

// HKDF-Extract(0, `psk`): RFC 8446 §7.1's Early Secret, with zeros for no PSK.
fn early_secret[&k, &o](h: int, psk: &k [byte], out: &!o [byte]) -> [] int {
    region r {
        let zeros = alloc_slice[r](h, byte_of(0));
        hkdf.extract(h, zeros, psk, out);
    }
    return 0;
}

// The hash of no bytes, under the hash `len(out)` names.
fn hash_of_nothing[&o](out: &!o [byte]) -> [] int {
    if len(out) == 48 {
        crypto.sha384("", out);
    } else {
        crypto.sha256("", out);
    }
    return 0;
}

// ---- The handshake ----

// ServerHello, or a HelloRetryRequest: the shared secret, then the
// handshake secrets and keys (RFC 8446 §7.1).
fn on_server_hello[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    var code = 0;
    region r {
        let info = alloc_slice[r](tls_message.sh_info_len(), 0);
        code = tls_message.server_hello(message[4..len(message)], bytes[tls_slot.k_session_id()..tls_slot.k_session_id() + 32], info);
        let suite = info[tls_message.sh_suite()];
        let group = info[tls_message.sh_group()];
        let tls12 = info[tls_message.sh_version()] == 0x0303;
        if code == 0 && tls12 && tls_slot.has(ints, tls_slot.f_retried()) {
            // A HelloRetryRequest committed the server to TLS 1.3.
            code = tls_record.protocol_version();
        }
        if code == 0 && tls12 {
            code = tls_client12.on_server_hello12(ints, bytes, message, suite);
        } else if code == 0 && tls_slot.has(ints, tls_slot.f_retried()) {
            // After a retry: no second one, and the suite it named.
            if info[tls_message.sh_retry()] == 1 {
                code = tls_record.unexpected_message();
            } else if suite != ints[tls_slot.i_suite()] {
                code = tls_record.hello_retry();
            }
        }
        if code == 0 && tls12 {
        } else if code == 0 && info[tls_message.sh_retry()] == 1 {
            code = retry(ints, bytes, message, suite, group, info[tls_message.sh_cookie_start()], info[tls_message.sh_cookie_end()]);
        } else if code == 0 {
            if group != ints[tls_slot.i_group()] {
                // A share for a group the client has no share of.
                code = tls_record.key_share();
            }
            if code == 0 && info[tls_message.sh_psk()] == 1 {
                // A resumption: the ticket was offered (RFC 8446 §4.2: an
                // extension the client did not send is
                // unsupported_extension), and the suite hashes as its PSK
                // was made (§4.2.11).
                if !tls_slot.has(ints, tls_slot.f_psk_offered()) {
                    code = tls_record.unsupported_extension();
                } else if tls_record.hash_len(suite) != ints[tls_slot.i_offer_hash()] {
                    code = tls_record.illegal_psk();
                } else {
                    tls_slot.set_flag(ints, tls_slot.f_resumed());
                }
            }
            if code == 0 {
                ints[tls_slot.i_suite()] = suite;
                tls_slot.transcript_add(ints, message);
                code = handshake_secrets(ints, bytes, message[4 + info[tls_message.sh_share()]..4 + info[tls_message.sh_share()] + tls_message.share_len(group)]);
            }
            if code == 0 {
                ints[tls_slot.i_state()] = tls_slot.state_wait_extensions();
            }
        }
    }
    return code;
}

// The HelloRetryRequest `message`, already parsed: `suite` and `group`
// (0 for none), and the cookie's range in its body (0, 0 for none).
fn retry[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte], suite: int, group: int, cookie_start: int, cookie_end: int) -> [] int {
    tls_slot.set_flag(ints, tls_slot.f_retried());
    ints[tls_slot.i_suite()] = suite;
    if tls_slot.has(ints, tls_slot.f_psk_offered()) && tls_record.hash_len(suite) != ints[tls_slot.i_offer_hash()] {
        // The suite the retry names cannot use the ticket: the second
        // ClientHello offers none (RFC 8446 §4.1.4), and the handshake is full.
        tls_slot.clear_flag(ints, tls_slot.f_psk_offered());
    }
    let h = tls_slot.hash_len(ints);
    var code = 0;
    region r {
        // message_hash: type 254, the hash's length, Hash(ClientHello1).
        let synthetic = alloc_slice[r](4 + h, byte_of(0));
        synthetic[0] = byte_of(254);
        synthetic[3] = byte_of(h);
        tls_slot.transcript_hash(ints, synthetic[4..4 + h]);
        tls_slot.transcript_init(ints);
        tls_slot.transcript_add(ints, synthetic);
        tls_slot.transcript_add(ints, message);
        let cookie = message[4 + cookie_start..4 + cookie_end];
        if group == 0 {
            // Only a cookie: the same X25519 share again.
            let share = alloc_slice[r](32, byte_of(0));
            x25519.public_key(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], share);
            code = send_client_hello(ints, bytes, share, cookie);
        } else {
            ints[tls_slot.i_group()] = group;
            let share = alloc_slice[r](tls_message.share_len(group), byte_of(0));
            code = tls_slot.new_ecdh_share(ints, bytes, tls_slot.curve_of(group), share);
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
    let h = tls_slot.hash_len(ints);
    let curve = tls_slot.curve_of(ints[tls_slot.i_group()]);
    var code = 0;
    region r {
        var size = 32;
        if curve != 0 {
            size = curve / 8;
        }
        let secret = alloc_slice[r](size, byte_of(0));
        if curve == 0 {
            if x25519.scalarmult(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], share, secret) != 0 {
                code = tls_record.key_share();
            }
        } else if ecdh.shared(curve, bytes[tls_slot.k_ecdh()..tls_slot.k_ecdh() + size], share, secret, ints[tls_slot.i_ecdh_work()..tls_slot.i_ecdh_work() + ecdh.work_len()]) != 0 {
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
            if tls_slot.has(ints, tls_slot.f_resumed()) {
                early_secret(h, bytes[tls_slot.k_offer_psk()..tls_slot.k_offer_psk() + h], early);
            } else {
                hkdf.extract(h, zeros, zeros, early);
            }
            hkdf.derive_secret(h, early, "derived", empty_hash, derived);
            hkdf.extract(h, derived, secret, hs);
            tls_slot.transcript_hash(ints, th);
            hkdf.derive_secret(h, hs, "c hs traffic", th, bytes[tls_slot.k_client_hs()..tls_slot.k_client_hs() + h]);
            hkdf.derive_secret(h, hs, "s hs traffic", th, bytes[tls_slot.k_server_hs()..tls_slot.k_server_hs() + h]);
            hkdf.derive_secret(h, hs, "derived", empty_hash, derived);
            hkdf.extract(h, derived, zeros, bytes[tls_slot.k_master()..tls_slot.k_master() + h]);
            tls_slot.set_read_keys(ints, bytes, tls_slot.k_server_hs());
            tls_slot.zero(secret);
            tls_slot.zero(hs);
            tls_slot.zero(early);
            tls_slot.zero(derived);
        }
    }
    // The key exchange's secrets have done their work.
    tls_slot.zero(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 80]);
    return code;
}

// Certificate: the chain verified (`tls_slot.verify_chain`); the leaf is
// kept for CertificateVerify.
fn on_certificate[&i, &b, &m, &p](ints: &!i [int], bytes: &!b [byte], message: &m [byte], store: &p [byte]) -> [] int {
    var code = 0;
    region r {
        let info = alloc_slice[r](3 + 2 * tls_message.max_certificates(), 0);
        let body = message[4..len(message)];
        code = tls_message.certificate(body, info);
        if code == 0 {
            code = tls_slot.verify_chain(ints, bytes, body, info, store);
        }
        if code == 0 {
            tls_slot.transcript_add(ints, message);
        }
    }
    if code == 0 {
        ints[tls_slot.i_state()] = tls_slot.state_wait_verify();
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
            let h = tls_slot.hash_len(ints);
            let content = alloc_slice[r](64 + 33 + 1 + h, byte_of(32));
            let label = "TLS 1.3, server CertificateVerify";
            tls_slot.copy_bytes(label, content[64..97]);
            content[97] = byte_of(0);
            tls_slot.transcript_hash(ints, content[98..98 + h]);
            code = tls_slot.check_signature(bytes[tls_slot.b_leaf()..tls_slot.b_leaf() + ints[tls_slot.i_leaf_len()]], info[0], content, message[4 + info[1]..4 + info[2]], false);
        }
        if code == 0 {
            tls_slot.transcript_add(ints, message);
        }
    }
    if code == 0 {
        ints[tls_slot.i_state()] = tls_slot.state_wait_finished();
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
        tls_slot.transcript_hash(ints, th);
        hmac.mac(h, key, th, out);
        tls_slot.zero(key);
    }
    return 0;
}

// The server's Finished, then the client's flight: a change_cipher_spec
// for middleboxes (RFC 8446 Appendix D.4), an empty Certificate if one
// was requested, and Finished; then the application keys.
fn on_finished[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    let h = tls_slot.hash_len(ints);
    var code = tls_message.finished(message[4..len(message)], h);
    region r {
        let want = alloc_slice[r](h, byte_of(0));
        if code == 0 {
            finished_mac(ints, bytes[tls_slot.k_server_hs()..tls_slot.k_server_hs() + h], want);
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
            tls_slot.transcript_add(ints, message);
            let th = alloc_slice[r](h, byte_of(0));
            tls_slot.transcript_hash(ints, th);
            hkdf.derive_secret(h, bytes[tls_slot.k_master()..tls_slot.k_master() + h], "c ap traffic", th, bytes[tls_slot.k_client_ap()..tls_slot.k_client_ap() + h]);
            hkdf.derive_secret(h, bytes[tls_slot.k_master()..tls_slot.k_master() + h], "s ap traffic", th, bytes[tls_slot.k_server_ap()..tls_slot.k_server_ap() + h]);
            let ccs = alloc_slice[r](1, byte_of(1));
            code = tls_slot.queue_record(ints, bytes, tls_record.type_change_cipher_spec(), ccs);
            tls_slot.set_write_keys(ints, bytes, tls_slot.k_client_hs());
            if code == 0 && tls_slot.has(ints, tls_slot.f_cert_requested()) {
                let cn = ints[tls_slot.i_context_len()];
                let cert = alloc_slice[r](4 + 1 + cn + 3, byte_of(0));
                cert[0] = byte_of(tls_message.type_certificate());
                cert[2] = byte_of(1 + cn + 3 >> 8);
                cert[3] = byte_of(1 + cn + 3 & 255);
                cert[4] = byte_of(cn);
                tls_slot.copy_bytes(bytes[tls_slot.k_context()..tls_slot.k_context() + cn], cert[5..5 + cn]);
                tls_slot.transcript_add(ints, cert);
                code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), cert);
            }
            if code == 0 {
                let fin = alloc_slice[r](4 + h, byte_of(0));
                fin[0] = byte_of(tls_message.type_finished());
                fin[3] = byte_of(h);
                finished_mac(ints, bytes[tls_slot.k_client_hs()..tls_slot.k_client_hs() + h], fin[4..4 + h]);
                tls_slot.transcript_add(ints, fin);
                code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), fin);
                // The resumption master secret, over the transcript through
                // the client's Finished (RFC 8446 §7.1), for the tickets to come.
                let th2 = alloc_slice[r](h, byte_of(0));
                tls_slot.transcript_hash(ints, th2);
                hkdf.derive_secret(h, bytes[tls_slot.k_master()..tls_slot.k_master() + h], "res master", th2, bytes[tls_slot.k_res_master()..tls_slot.k_res_master() + h]);
                if !tls_slot.has(ints, tls_slot.f_resumed()) {
                    ints[tls_slot.i_verified_at()] = ints[tls_slot.i_now()];
                }
            }
            if code == 0 {
                tls_slot.set_write_keys(ints, bytes, tls_slot.k_client_ap());
                tls_slot.set_read_keys(ints, bytes, tls_slot.k_server_ap());
                // The handshake secrets and the master secret have done
                // their work; the application secrets stay for KeyUpdate.
                tls_slot.zero(bytes[tls_slot.k_client_hs()..tls_slot.k_client_hs() + 96]);
                tls_slot.zero(bytes[tls_slot.k_master()..tls_slot.k_master() + 48]);
                ints[tls_slot.i_state()] = tls_slot.state_connected();
            }
        }
    }
    return code;
}

// A NewSessionTicket: kept as the connection's newest, with its PSK
// (RFC 8446 §4.6.1), or dropped if it is not one to keep: a lifetime of
// 0, or a ticket larger than `tls_slot.ticket_cap()`. Either way the
// connection goes on. A lifetime over the 7 days servers may use is
// capped at 7 days.
fn on_ticket[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    var code = 0;
    region r {
        let info = alloc_slice[r](tls_message.nst_info_len(), 0);
        let body = message[4..len(message)];
        code = tls_message.new_session_ticket(body, info);
        let ts = info[tls_message.nst_ticket_start()];
        let te = info[tls_message.nst_ticket_end()];
        var lifetime = info[tls_message.nst_lifetime()];
        if lifetime > 604800 {
            lifetime = 604800;
        }
        if code == 0 && lifetime > 0 && te - ts <= tls_slot.ticket_cap() {
            let h = tls_slot.hash_len(ints);
            tls_slot.copy_bytes(body[ts..te], bytes[tls_slot.b_ticket()..tls_slot.b_ticket() + te - ts]);
            hkdf.expand_label(h, bytes[tls_slot.k_res_master()..tls_slot.k_res_master() + h], "resumption", body[info[tls_message.nst_nonce_start()]..info[tls_message.nst_nonce_end()]], bytes[tls_slot.k_ticket_psk()..tls_slot.k_ticket_psk() + h]);
            let hn = ints[tls_slot.i_host_len()];
            tls_slot.copy_bytes(bytes[tls_slot.k_host()..tls_slot.k_host() + hn], bytes[tls_slot.b_ticket_host()..tls_slot.b_ticket_host() + hn]);
            ints[tls_slot.i_ticket_len()] = te - ts;
            ints[tls_slot.i_ticket_lifetime()] = lifetime;
            ints[tls_slot.i_ticket_age_add()] = info[tls_message.nst_age_add()];
            ints[tls_slot.i_ticket_hash()] = h;
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
    ints[tls_slot.i_key_updates()] = ints[tls_slot.i_key_updates()] + 1;
    if ints[tls_slot.i_key_updates()] > 32 {
        return tls_record.too_many_messages();
    }
    var code = 0;
    region r {
        let h = tls_slot.hash_len(ints);
        let next = alloc_slice[r](h, byte_of(0));
        hkdf.expand_label(h, bytes[tls_slot.k_server_ap()..tls_slot.k_server_ap() + h], "traffic upd", "", next);
        tls_slot.copy_bytes(next, bytes[tls_slot.k_server_ap()..tls_slot.k_server_ap() + h]);
        tls_slot.set_read_keys(ints, bytes, tls_slot.k_server_ap());
        if asked == 1 {
            let answer = alloc_slice[r](5, byte_of(0));
            answer[0] = byte_of(tls_message.type_key_update());
            answer[3] = byte_of(1);
            code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), answer);
            hkdf.expand_label(h, bytes[tls_slot.k_client_ap()..tls_slot.k_client_ap() + h], "traffic upd", "", next);
            tls_slot.copy_bytes(next, bytes[tls_slot.k_client_ap()..tls_slot.k_client_ap() + h]);
            tls_slot.set_write_keys(ints, bytes, tls_slot.k_client_ap());
        }
        tls_slot.zero(next);
    }
    return code;
}

// One whole handshake message, header included, in the current state.
fn on_message[&i, &b, &m, &p](ints: &!i [int], bytes: &!b [byte], message: &m [byte], store: &p [byte]) -> [] int {
    let kind = int_of(message[0]);
    let state = ints[tls_slot.i_state()];
    if tls_slot.has(ints, tls_slot.f_tls12()) {
        return tls_client12.on_message12(ints, bytes, message, store);
    }
    if state == tls_slot.state_wait_server_hello() && kind == tls_message.type_server_hello() {
        return on_server_hello(ints, bytes, message);
    }
    if state == tls_slot.state_wait_extensions() && kind == tls_message.type_encrypted_extensions() {
        let code = tls_message.encrypted_extensions(message[4..len(message)]);
        if code == 0 {
            tls_slot.transcript_add(ints, message);
            ints[tls_slot.i_state()] = tls_slot.state_wait_certificate();
            if tls_slot.has(ints, tls_slot.f_resumed()) {
                // No Certificate, CertificateRequest or CertificateVerify in
                // a resumption: anything but Finished is out of order.
                ints[tls_slot.i_state()] = tls_slot.state_wait_finished();
            }
        }
        return code;
    }
    if state == tls_slot.state_wait_certificate() && kind == tls_message.type_certificate_request() && !tls_slot.has(ints, tls_slot.f_cert_requested()) {
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
        tls_slot.copy_bytes(body[1..1 + cn], bytes[tls_slot.k_context()..tls_slot.k_context() + cn]);
        ints[tls_slot.i_context_len()] = cn;
        tls_slot.set_flag(ints, tls_slot.f_cert_requested());
        tls_slot.transcript_add(ints, message);
        return 0;
    }
    if state == tls_slot.state_wait_certificate() && kind == tls_message.type_certificate() {
        return on_certificate(ints, bytes, message, store);
    }
    if state == tls_slot.state_wait_verify() && kind == tls_message.type_certificate_verify() {
        return on_certificate_verify(ints, bytes, message);
    }
    if state == tls_slot.state_wait_finished() && kind == tls_message.type_finished() {
        return on_finished(ints, bytes, message);
    }
    if state == tls_slot.state_connected() && kind == tls_message.type_new_session_ticket() {
        return on_ticket(ints, bytes, message);
    }
    if state == tls_slot.state_connected() && kind == tls_message.type_key_update() {
        return on_key_update(ints, bytes, message);
    }
    return tls_record.unexpected_message();
}

// Appends handshake bytes and handles every whole message they finish.
fn on_handshake_bytes[&i, &b, &c, &p](ints: &!i [int], bytes: &!b [byte], content: &c [byte], store: &p [byte]) -> [] int {
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
        if have < 4 {
            going = false;
        } else {
            let p = tls_slot.b_hs() + at;
            let n = int_of(bytes[p + 1]) * 65536 + int_of(bytes[p + 2]) * 256 + int_of(bytes[p + 3]);
            if 4 + n > tls_slot.hs_cap() {
                code = tls_record.record_overflow();
            } else if have < 4 + n {
                going = false;
            } else {
                region r {
                    // A message is handled from a copy, so the handler may
                    // write to the slot's bytes freely.
                    let message = alloc_slice[r](4 + n, byte_of(0));
                    tls_slot.copy_bytes(bytes[p..p + 4 + n], message);
                    let before = ints[tls_slot.i_state()];
                    code = on_message(ints, bytes, message, store);
                    at = at + 4 + n;
                    // A message must not share a record with the next key
                    // (RFC 8446 §5.1): after ServerHello and Finished, the
                    // record must end with the message.
                    let after = ints[tls_slot.i_state()];
                    if code == 0 && after != before && (after == tls_slot.state_wait_extensions() || after == tls_slot.state_connected()) && at < ints[tls_slot.i_hs_fill()] {
                        code = tls_record.unexpected_message();
                    }
                }
            }
        }
    }
    // Drop what was handled; keep a partial message at the front.
    let left = ints[tls_slot.i_hs_fill()] - at;
    var k = 0;
    while k < left {
        bytes[tls_slot.b_hs() + k] = bytes[tls_slot.b_hs() + at + k];
        k = k + 1;
    }
    ints[tls_slot.i_hs_fill()] = left;
    return code;
}

fn on_alert[&i, &b, &c](ints: &!i [int], bytes: &!b [byte], content: &c [byte]) -> [] int {
    if len(content) != 2 {
        return tls_record.decode_error();
    }
    let level = int_of(content[0]);
    let what = int_of(content[1]);
    if what == 0 {
        // A clean close only once established (review finding E-3,
        // #209): before that an alert may be plaintext, which anyone on
        // the path can send, and nothing was authenticated to end
        // cleanly, so the connection fails as a peer that closed.
        if ints[tls_slot.i_state()] != tls_slot.state_connected() {
            return tls_record.peer_closed();
        }
        tls_slot.set_flag(ints, tls_slot.f_close_received());
        if tls_slot.has(ints, tls_slot.f_close_sent()) {
            tls_slot.forget(bytes);
        }
        return 0;
    }
    if what == 90 {
        // user_canceled: wait for the close_notify that follows.
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

// One whole record from `bytes[tls_slot.b_in()..]`, `n` bytes long.
fn on_record[&i, &b, &p](ints: &!i [int], bytes: &!b [byte], n: int, store: &p [byte]) -> [] int {
    let kind = int_of(bytes[tls_slot.b_in()]);
    let body = n - 5;
    let state = ints[tls_slot.i_state()];
    if kind == tls_record.type_change_cipher_spec() && tls_slot.has(ints, tls_slot.f_tls12()) {
        if body != 1 || int_of(bytes[tls_slot.b_in() + 5]) != 1 {
            return tls_record.unexpected_message();
        }
        return tls_client12.on_ccs12(ints);
    }
    if kind == tls_record.type_change_cipher_spec() {
        // One, exactly `01`, after ServerHello (or a HelloRetryRequest)
        // and before the server's Finished (RFC 8446 Appendix D.4).
        let early = state < tls_slot.state_wait_extensions() && !(state == tls_slot.state_wait_server_hello() && tls_slot.has(ints, tls_slot.f_retried()));
        if body != 1 || int_of(bytes[tls_slot.b_in() + 5]) != 1 || tls_slot.has(ints, tls_slot.f_ccs_seen()) || early || state > tls_slot.state_wait_finished() {
            return tls_record.unexpected_message();
        }
        tls_slot.set_flag(ints, tls_slot.f_ccs_seen());
        return 0;
    }
    if !tls_slot.has(ints, tls_slot.f_read_protected()) {
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
            if kind == tls_record.type_handshake() {
                plain = on_handshake_bytes(ints, bytes, content, store);
            } else {
                plain = on_alert(ints, bytes, content);
            }
        }
        return plain;
    }
    let tls12 = tls_slot.has(ints, tls_slot.f_tls12());
    if kind != tls_record.type_application_data() && !tls12 {
        return tls_record.unexpected_message();
    }
    var code = 0;
    var inner = 0;
    var size = 0;
    region q {
        let info = alloc_slice[q](2, 0);
        if tls12 {
            // TLS 1.2: the type is the header's, and every record after
            // change_cipher_spec is an AEAD record.
            code = tls_record.open12(ints[tls_slot.i_suite()], bytes[tls_slot.k_read_key()..tls_slot.k_read_key() + tls_slot.key_len(ints)], bytes[tls_slot.k_read_iv()..tls_slot.k_read_iv() + 12], ints[tls_slot.i_read_seq()], bytes[tls_slot.b_in()..tls_slot.b_in() + n], bytes[tls_slot.b_plain()..tls_slot.b_plain() + tls_slot.plain_cap()], info);
        } else {
            code = tls_record.open(ints[tls_slot.i_suite()], bytes[tls_slot.k_read_key()..tls_slot.k_read_key() + tls_slot.key_len(ints)], bytes[tls_slot.k_read_iv()..tls_slot.k_read_iv() + 12], ints[tls_slot.i_read_seq()], bytes[tls_slot.b_in()..tls_slot.b_in() + n], bytes[tls_slot.b_plain()..tls_slot.b_plain() + tls_slot.plain_cap()], info);
        }
        inner = info[0];
        size = info[1];
    }
    if code != 0 {
        return code;
    }
    ints[tls_slot.i_read_seq()] = ints[tls_slot.i_read_seq()] + 1;
    region r {
        let content = alloc_slice[r](size, byte_of(0));
        tls_slot.copy_bytes(bytes[tls_slot.b_plain()..tls_slot.b_plain() + size], content);
        if inner == tls_record.type_handshake() {
            if size == 0 {
                code = tls_record.unexpected_message();
            } else {
                code = on_handshake_bytes(ints, bytes, content, store);
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

// ---- The interface (`docs/tls-pure.md` §2.2) ----

// Bytes the socket gave. Answers how many were consumed (all of them,
// unless output or received data must be taken first), or the
// connection's failure.
pub fn feed[&i, &b, &d, &p](ints: &!i [int], bytes: &!b [byte], data: &d [byte], store: &p [byte]) -> [] int {
    var consumed = 0;
    var code = 0;
    while consumed < len(data) && code == 0 && ints[tls_slot.i_state()] != tls_slot.state_failed() && !tls_slot.has(ints, tls_slot.f_close_received()) {
        // Room for what one record can produce: an answer, or its content.
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
                code = on_record(ints, bytes, total, store);
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

// Bytes for the socket, as many as fit in `out`.
pub fn take[&i, &b, &o](ints: &!i [int], bytes: &b [byte], out: &!o [byte]) -> [] int {
    let s = ints[tls_slot.i_out_start()];
    var n = ints[tls_slot.i_out_end()] - s;
    if n > len(out) {
        n = len(out);
    }
    tls_slot.copy_bytes(bytes[tls_slot.b_out() + s..tls_slot.b_out() + s + n], out[0..n]);
    ints[tls_slot.i_out_start()] = s + n;
    return n;
}

// Once established: queues up to one record of `plaintext`. Answers how
// many bytes were taken (0 when the output queue is full), or a refusal.
pub fn send[&i, &b, &p](ints: &!i [int], bytes: &!b [byte], plaintext: &p [byte]) -> [] int {
    if ints[tls_slot.i_state()] != tls_slot.state_connected() || tls_slot.has(ints, tls_slot.f_close_sent()) {
        return tls_record.unexpected_message();
    }
    var n = len(plaintext);
    if n > tls_record.max_plaintext() {
        n = tls_record.max_plaintext();
    }
    tls_slot.compact_out(ints, bytes);
    if tls_slot.out_free(ints) < n + 22 {
        return 0;
    }
    let code = tls_slot.queue_record(ints, bytes, tls_record.type_application_data(), plaintext[0..n]);
    if code != 0 {
        return tls_slot.fail(ints, bytes, code);
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
    let s = ints[tls_slot.i_recv_start()];
    var n = ints[tls_slot.i_recv_end()] - s;
    if n == 0 {
        if tls_slot.has(ints, tls_slot.f_close_received()) {
            return 0;
        }
        if ints[tls_slot.i_state()] == tls_slot.state_failed() {
            return ints[tls_slot.i_failure()];
        }
        return would_block();
    }
    if n > len(into) {
        n = len(into);
    }
    tls_slot.copy_bytes(bytes[tls_slot.b_recv() + s..tls_slot.b_recv() + s + n], into[0..n]);
    ints[tls_slot.i_recv_start()] = s + n;
    return n;
}

// The socket ended. After the peer's close_notify that is the clean end;
// before it, the data may have been cut short, and the connection fails
// as `tls-peer-closed` (RFC 8446 §6.1).
pub fn peer_eof[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    if tls_slot.has(ints, tls_slot.f_close_received()) || ints[tls_slot.i_state()] == tls_slot.state_failed() {
        return 0;
    }
    return tls_slot.fail(ints, bytes, tls_record.peer_closed());
}

// Queues close_notify.
pub fn finish[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    if tls_slot.has(ints, tls_slot.f_close_sent()) || ints[tls_slot.i_state()] == tls_slot.state_failed() {
        return 0;
    }
    tls_slot.set_flag(ints, tls_slot.f_close_sent());
    region r {
        let alert = alloc_slice[r](2, byte_of(0));
        alert[0] = byte_of(1);
        tls_slot.queue_record(ints, bytes, tls_record.type_alert(), alert);
    }
    if tls_slot.has(ints, tls_slot.f_close_received()) {
        tls_slot.forget(bytes);
    }
    return 0;
}

pub fn event[&i](ints: &i [int]) -> [] int {
    if ints[tls_slot.i_out_end()] > ints[tls_slot.i_out_start()] {
        return tls_slot.event_want_write();
    }
    if ints[tls_slot.i_state()] == tls_slot.state_failed() {
        return tls_slot.event_failed();
    }
    if tls_slot.has(ints, tls_slot.f_close_received()) || tls_slot.has(ints, tls_slot.f_close_sent()) {
        return tls_slot.event_closed();
    }
    if ints[tls_slot.i_state()] == tls_slot.state_connected() {
        return tls_slot.event_established();
    }
    return tls_slot.event_want_read();
}

// `event` as it will be once every queued byte is taken.
pub fn event_after_take[&i](ints: &i [int]) -> [] int {
    if ints[tls_slot.i_state()] == tls_slot.state_failed() {
        return tls_slot.event_failed();
    }
    if tls_slot.has(ints, tls_slot.f_close_received()) || tls_slot.has(ints, tls_slot.f_close_sent()) {
        return tls_slot.event_closed();
    }
    if ints[tls_slot.i_state()] == tls_slot.state_connected() {
        return tls_slot.event_established();
    }
    return tls_slot.event_want_read();
}

pub fn failure[&i](ints: &i [int]) -> [] int {
    return ints[tls_slot.i_failure()];
}

// The alert the peer sent, when `failure` is `tls-alert`.
// Whether the connection resumed a session (known once established).
pub fn resumed[&i](ints: &i [int]) -> [] bool {
    return tls_slot.has(ints, tls_slot.f_resumed());
}

pub fn alert_received[&i](ints: &i [int]) -> [] int {
    return ints[tls_slot.i_alert()];
}

// Overwrites the connection's secrets, keys and IVs (best effort,
// `docs/tls-core.md` §8).
pub fn drop[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    tls_slot.zero(bytes[tls_slot.b_keys()..tls_slot.b_keys() + tls_slot.keys_len()]);
    tls_slot.zero(bytes[tls_slot.b_plain()..tls_slot.b_plain() + tls_slot.plain_cap()]);
    tls_slot.zero(bytes[tls_slot.b_recv()..tls_slot.b_recv() + tls_slot.recv_cap()]);
    tls_slot.zero(bytes[tls_slot.b_offer()..tls_slot.bytes_len()]);
    var k = 0;
    while k < tls_slot.ints_len() {
        ints[k] = 0;
        k = k + 1;
    }
    return 0;
}
