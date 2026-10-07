module tls_client12;
import std.ecdh;
import std.x25519;
import tls_message;
import tls_record;
import tls_slot;
import x509;

// `tls_client12` -- the TLS 1.2 handshake (RFC 5246, with the extended
// master secret of RFC 7627 required, ECDHE of RFC 8422 and the AEAD
// suites of RFC 5289 and RFC 7905; `docs/tls-parity.md` §3.4), on the
// slot `tls_slot` lays out. `tls_client` hands it a TLS 1.2 ServerHello,
// every handshake message after, and the server's change_cipher_spec.
// The server is authenticated by its chain and by its signature over the
// key exchange; there is no client certificate, no session resumption
// and no renegotiation. Not independently reviewed (#209).

// A TLS 1.2 ServerHello, already parsed (`tls_message.server_hello`).
pub fn on_server_hello12[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte], suite: int) -> [] int {
    tls_slot.set_flag(ints, tls_slot.f_tls12());
    ints[tls_slot.i_suite()] = suite;
    tls_slot.copy_bytes(message[6..38], bytes[tls_slot.k_server_random()..tls_slot.k_server_random() + 32]);
    tls_slot.transcript_add(ints, message);
    ints[tls_slot.i_state()] = tls_slot.state_wait_certificate12();
    return 0;
}

// One whole handshake message, header included, in a TLS 1.2 state.
pub fn on_message12[&i, &b, &m, &p](ints: &!i [int], bytes: &!b [byte], message: &m [byte], store: &p [byte]) -> [] int {
    let kind = int_of(message[0]);
    let state = ints[tls_slot.i_state()];
    if state == tls_slot.state_wait_certificate12() && kind == tls_message.type_certificate() {
        return on_certificate12(ints, bytes, message, store);
    }
    if state == tls_slot.state_wait_key_exchange() && kind == tls_message.type_server_key_exchange() {
        return on_key_exchange(ints, bytes, message);
    }
    if state == tls_slot.state_wait_server_done() && kind == tls_message.type_certificate_request() && !tls_slot.has(ints, tls_slot.f_cert_requested()) {
        let code = tls_message.certificate_request12(message[4..len(message)]);
        if code == 0 {
            tls_slot.set_flag(ints, tls_slot.f_cert_requested());
            tls_slot.transcript_add(ints, message);
        }
        return code;
    }
    if state == tls_slot.state_wait_server_done() && kind == tls_message.type_server_hello_done() {
        return on_server_done(ints, bytes, message);
    }
    if state == tls_slot.state_wait_finished12() && kind == tls_message.type_finished() {
        return on_finished12(ints, bytes, message);
    }
    if state == tls_slot.state_connected() && kind == tls_message.type_hello_request() {
        return tls_record.renegotiation();
    }
    return tls_record.unexpected_message();
}

// The server's change_cipher_spec: after the client's Finished, once,
// and from then every record is protected.
pub fn on_ccs12[&i](ints: &!i [int]) -> [] int {
    if ints[tls_slot.i_state()] != tls_slot.state_wait_ccs12() || tls_slot.has(ints, tls_slot.f_ccs_seen()) {
        return tls_record.unexpected_message();
    }
    tls_slot.set_flag(ints, tls_slot.f_ccs_seen());
    tls_slot.set_flag(ints, tls_slot.f_read_protected());
    ints[tls_slot.i_read_seq()] = 0;
    ints[tls_slot.i_state()] = tls_slot.state_wait_finished12();
    return 0;
}

fn on_certificate12[&i, &b, &m, &p](ints: &!i [int], bytes: &!b [byte], message: &m [byte], store: &p [byte]) -> [] int {
    var code = 0;
    region r {
        let info = alloc_slice[r](3 + 2 * tls_message.max_certificates(), 0);
        let body = message[4..len(message)];
        code = tls_message.certificate12(body, info);
        if code == 0 {
            code = tls_slot.verify_chain(ints, bytes, body, info, store);
        }
        if code == 0 {
            tls_slot.transcript_add(ints, message);
        }
    }
    if code == 0 {
        ints[tls_slot.i_state()] = tls_slot.state_wait_key_exchange();
    }
    return code;
}

// ServerKeyExchange: the server's ECDHE point, signed by the leaf's key
// over both randoms and the parameters (RFC 8422 §5.4). The suite says
// which kind of key: ECDSA (or Ed25519) for an ECDHE_ECDSA suite, RSA
// for ECDHE_RSA.
fn on_key_exchange[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    let body = message[4..len(message)];
    var code = 0;
    region r {
        let info = alloc_slice[r](tls_message.ske_info_len(), 0);
        code = tls_message.server_key_exchange(body, info);
        if code == 0 {
            let alg = tls_slot.leaf_key_algorithm(ints, bytes);
            let rsa_key = alg == x509.oid_rsa_encryption();
            if tls_record.suite12_ecdsa(ints[tls_slot.i_suite()]) == rsa_key {
                code = tls_record.bad_certificate_verify();
            }
        }
        if code == 0 {
            let params = info[tls_message.ske_params_end()];
            let content = alloc_slice[r](64 + params, byte_of(0));
            tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], content[0..32]);
            tls_slot.copy_bytes(bytes[tls_slot.k_server_random()..tls_slot.k_server_random() + 32], content[32..64]);
            tls_slot.copy_bytes(body[0..params], content[64..64 + params]);
            code = tls_slot.check_signature(bytes[tls_slot.b_leaf()..tls_slot.b_leaf() + ints[tls_slot.i_leaf_len()]], info[tls_message.ske_scheme()], content, body[info[tls_message.ske_sig_start()]..len(body)], true);
        }
        if code == 0 {
            let group = info[tls_message.ske_group()];
            let point = body[info[tls_message.ske_point_start()]..info[tls_message.ske_point_end()]];
            ints[tls_slot.i_group()] = group;
            tls_slot.copy_bytes(point, bytes[tls_slot.k_peer()..tls_slot.k_peer() + len(point)]);
            tls_slot.transcript_add(ints, message);
            ints[tls_slot.i_state()] = tls_slot.state_wait_server_done();
        }
    }
    return code;
}

// ServerHelloDone: the client's flight. An empty Certificate if one was
// requested, ClientKeyExchange, then the keys: the extended master
// secret over the session hash (RFC 7627 §4), the key block, and
// change_cipher_spec and Finished under the new write key.
fn on_server_done[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    if len(message) != 4 {
        return tls_record.decode_error();
    }
    tls_slot.transcript_add(ints, message);
    let suite = ints[tls_slot.i_suite()];
    let h = tls_record.hash_len(suite);
    let group = ints[tls_slot.i_group()];
    let size = tls_message.share_len(group);
    let curve = tls_slot.curve_of(group);
    var code = 0;
    region r {
        if tls_slot.has(ints, tls_slot.f_cert_requested()) {
            let cert = alloc_slice[r](7, byte_of(0));
            cert[0] = byte_of(tls_message.type_certificate());
            cert[3] = byte_of(3);
            tls_slot.transcript_add(ints, cert);
            code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), cert);
        }
        let share = alloc_slice[r](size, byte_of(0));
        var secret_len = 32;
        if curve != 0 {
            secret_len = curve / 8;
        }
        let pms = alloc_slice[r](secret_len, byte_of(0));
        let peer = bytes[tls_slot.k_peer()..tls_slot.k_peer() + size];
        if code == 0 && curve == 0 {
            x25519.public_key(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], share);
            if x25519.scalarmult(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], peer, pms) != 0 {
                code = tls_record.key_share();
            }
        } else if code == 0 {
            code = tls_slot.new_ecdh_share(ints, bytes, curve, share);
            if code == 0 && ecdh.shared(curve, bytes[tls_slot.k_ecdh()..tls_slot.k_ecdh() + secret_len], peer, pms, ints[tls_slot.i_ecdh_work()..tls_slot.i_ecdh_work() + ecdh.work_len()]) != 0 {
                code = tls_record.key_share();
            }
        }
        if code == 0 {
            // ClientKeyExchange: the point, with a one-byte length.
            let cke = alloc_slice[r](5 + size, byte_of(0));
            cke[0] = byte_of(tls_message.type_client_key_exchange());
            cke[3] = byte_of(1 + size);
            cke[4] = byte_of(size);
            tls_slot.copy_bytes(share, cke[5..5 + size]);
            tls_slot.transcript_add(ints, cke);
            code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), cke);
        }
        if code == 0 {
            let session_hash = alloc_slice[r](h, byte_of(0));
            tls_slot.transcript_hash(ints, session_hash);
            let master = bytes[tls_slot.k_master()..tls_slot.k_master() + 48];
            tls_record.prf(h, pms, "extended master secret", session_hash, master);
            key_block(ints, bytes);
            let ccs = alloc_slice[r](1, byte_of(1));
            code = tls_slot.queue_record(ints, bytes, tls_record.type_change_cipher_spec(), ccs);
            tls_slot.set_flag(ints, tls_slot.f_write_protected());
            ints[tls_slot.i_write_seq()] = 0;
        }
        if code == 0 {
            let fin = alloc_slice[r](16, byte_of(0));
            fin[0] = byte_of(tls_message.type_finished());
            fin[3] = byte_of(12);
            verify_data(ints, bytes, "client finished", fin[4..16]);
            tls_slot.transcript_add(ints, fin);
            code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), fin);
        }
        tls_slot.zero(pms);
    }
    // The key exchange's secrets have done their work.
    tls_slot.zero(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 80]);
    if code == 0 {
        ints[tls_slot.i_state()] = tls_slot.state_wait_ccs12();
    }
    return code;
}

// The key block (RFC 5246 §6.3): PRF(master, "key expansion", server
// random || client random) cut into the client's and the server's write
// keys, then their IVs. The client writes with its own, and reads with
// the server's once the server's change_cipher_spec comes.
fn key_block[&i, &b](ints: &!i [int], bytes: &!b [byte]) -> [] int {
    let suite = ints[tls_slot.i_suite()];
    let kl = tls_record.key_len(suite);
    let il = tls_record.iv12_len(suite);
    region r {
        let seed = alloc_slice[r](64, byte_of(0));
        tls_slot.copy_bytes(bytes[tls_slot.k_server_random()..tls_slot.k_server_random() + 32], seed[0..32]);
        tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], seed[32..64]);
        let block = alloc_slice[r](2 * kl + 2 * il, byte_of(0));
        tls_record.prf(tls_record.hash_len(suite), bytes[tls_slot.k_master()..tls_slot.k_master() + 48], "key expansion", seed, block);
        tls_slot.copy_bytes(block[0..kl], bytes[tls_slot.k_write_key()..tls_slot.k_write_key() + kl]);
        tls_slot.copy_bytes(block[kl..2 * kl], bytes[tls_slot.k_read_key()..tls_slot.k_read_key() + kl]);
        tls_slot.copy_bytes(block[2 * kl..2 * kl + il], bytes[tls_slot.k_write_iv()..tls_slot.k_write_iv() + il]);
        tls_slot.copy_bytes(block[2 * kl + il..2 * kl + 2 * il], bytes[tls_slot.k_read_iv()..tls_slot.k_read_iv() + il]);
        tls_slot.zero(block);
    }
    tls_slot.prepare_keys(ints, bytes);
    return 0;
}

// Finished's verify_data (RFC 5246 §7.4.9): PRF(master, `label`,
// Hash(handshake messages)), 12 bytes, into `out`.
fn verify_data[&i, &b, &l, &o](ints: &i [int], bytes: &b [byte], label: &l [byte], out: &!o [byte]) -> [] int {
    let h = tls_record.hash_len(ints[tls_slot.i_suite()]);
    region r {
        let th = alloc_slice[r](h, byte_of(0));
        tls_slot.transcript_hash(ints, th);
        tls_record.prf(h, bytes[tls_slot.k_master()..tls_slot.k_master() + 48], label, th, out);
    }
    return 0;
}

// The server's Finished, compared over all twelve bytes.
fn on_finished12[&i, &b, &m](ints: &!i [int], bytes: &!b [byte], message: &m [byte]) -> [] int {
    if len(message) != 16 {
        return tls_record.decode_error();
    }
    var diff = 0;
    region r {
        let want = alloc_slice[r](12, byte_of(0));
        verify_data(ints, bytes, "server finished", want);
        var k = 0;
        while k < 12 {
            diff = diff | int_of(want[k]) ^ int_of(message[4 + k]);
            k = k + 1;
        }
    }
    if diff != 0 {
        return tls_record.bad_finished();
    }
    tls_slot.transcript_add(ints, message);
    // The master secret has done its work; the keys stay.
    tls_slot.zero(bytes[tls_slot.k_master()..tls_slot.k_master() + 48]);
    ints[tls_slot.i_state()] = tls_slot.state_connected();
    return 0;
}
