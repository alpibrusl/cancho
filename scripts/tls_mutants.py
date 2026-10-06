#!/usr/bin/env python3
"""Mutation check of `packages/tls` (docs/tls-core.md §7, §10).

    python3 scripts/tls_mutants.py <cancho binary> [--only <text in a mutant's name>]

Each mutant is one of the package's files with one deliberate bug. The
package is copied to a scratch directory, the mutant applied there, and
`tests/programs/tls_driver.cho` built against it. It runs what
`conformance/tls.rs` replays: the five tlslite-ng traces (two of them through a
HelloRetryRequest), the six TLS 1.2 traces against OpenSSL, and the 84
connections of `tests/vectors/tls/liar.txt`, each answer compared byte for
byte, and the engine's rules for offering a ticket
(`tests/vectors/tls/tickets.txt`, through `tests/programs/tls_tickets.cho`).
A mutant of the engine (`tls.cho`) also builds
`tests/programs/tls_many.cho` and serves it `tests/vectors/tls/streams.txt` from
here, 64 connections at once: one byte a read, 65,536, and then with
close_notify cut off. A
mutant is killed when any answer differs, a connection ends otherwise, or a
program traps. The unmutated package is run first and must pass. Exit
status 1 if a mutant survives or fails to build.
"""
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FILES = ["record.cho", "message.cho", "slot.cho", "client12.cho", "client.cho", "tls.cho"]

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    ("the server Finished not checked", "client.cho",
     "            if diff != 0 {\n                code = tls_record.bad_finished();",
     "            if diff != diff {\n                code = tls_record.bad_finished();"),
    ("the transcript missing EncryptedExtensions", "client.cho",
     "            tls_slot.transcript_add(ints, message);\n            ints[tls_slot.i_state()] = tls_slot.state_wait_certificate();",
     "            ints[tls_slot.i_state()] = tls_slot.state_wait_certificate();"),
    ("the ClientHello hashed with its record header", "client.cho",
     "        tls_slot.transcript_add(ints, hello[0..n]);\n        code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), hello[0..n]);",
     "        code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), hello[0..n]);\n"
     "        tls_slot.transcript_add(ints, bytes[tls_slot.b_out()..tls_slot.b_out() + 5 + n]);"),
    ("the read sequence number not incremented", "client.cho",
     "    ints[tls_slot.i_read_seq()] = ints[tls_slot.i_read_seq()] + 1;", "    ints[tls_slot.i_read_seq()] = ints[tls_slot.i_read_seq()] + 0;"),
    ("the write sequence number not incremented", "slot.cho",
     "    ints[i_write_seq()] = ints[i_write_seq()] + 1;\n    ints[i_out_end()] = ints[i_out_end()] + n;",
     "    ints[i_write_seq()] = ints[i_write_seq()] + 0;\n    ints[i_out_end()] = ints[i_out_end()] + n;"),
    ("the nonce built without the sequence", "record.cho",
     "            s = seq >> 8 * (11 - k) & 255;", "            s = 0;"),
    ("the downgrade sentinel ignored", "message.cho",
     "int_of(b[33]) <= 1 {\n            return tls_record.protocol_version();",
     "int_of(b[33]) <= 1 && false {\n            return tls_record.protocol_version();"),
    ("server_name allowed in a TLS 1.3 ServerHello", "message.cho",
     "if reneg || ems || formats || sni {", "if reneg || ems || formats {"),
    ("server_name allowed twice in a TLS 1.2 ServerHello", "message.cho",
     "            if sni || size != 0 {", "            if size != 0 {"),
    ("the HelloRetryRequest random ignored", "message.cho", "    let retry = k == 32;", "    let retry = k == 33;"),
    ("an unexpected extension accepted", "message.cho",
     "            seen_groups = true;\n        } else {\n            return tls_record.unsupported_extension();\n        }",
     "            seen_groups = true;\n        }"),
    ("the record limit off by one", "record.cho",
     "    let n = int_of(buf[at + 3]) * 256 + int_of(buf[at + 4]);\n    if n > max_ciphertext() {",
     "    let n = int_of(buf[at + 3]) * 256 + int_of(buf[at + 4]);\n    if n >= max_ciphertext() {"),
    ("inner-plaintext padding not stripped", "record.cho",
     "    while last >= 0 && int_of(out[last]) == 0 {", "    while last >= 0 && int_of(out[last]) == 256 {"),
    ("the content type taken from the outer header", "client.cho",
     "        inner = info[0];", "        inner = kind;"),
    ("the client's keys used for reading", "client.cho",
     "            tls_slot.set_read_keys(ints, bytes, tls_slot.k_server_hs());", "            tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_hs());"),
    ("a KeyUpdate not answered", "client.cho", "        if asked == 1 {", "        if asked == 2 {"),
    ("the CertificateVerify context string misspelled", "client.cho",
     'let label = "TLS 1.3, server CertificateVerify";', 'let label = "TLS 1.3, client CertificateVerify";'),
    ("a partial message not moved to the front", "client.cho",
     "        bytes[tls_slot.b_hs() + k] = bytes[tls_slot.b_hs() + at + k];", "        bytes[tls_slot.b_hs() + k] = bytes[tls_slot.b_hs() + k];"),
    ("a warning-level alert ignored", "client.cho",
     "    ints[tls_slot.i_alert()] = what;\n", "    if level == 1 {\n        return 0;\n    }\n    ints[tls_slot.i_alert()] = what;\n"),
    ("the chain not verified", "slot.cho", "code = from_x509(x509_verify.verify(store, body, ranges, bytes[k_host()..k_host() + ints[i_host_len()]], ints[i_now()], x509_verify.tls_max_intermediates()));", "code = 0;"),
    ("the time not given to the verifier", "slot.cho", "ints[i_now()], x509_verify.tls_max_intermediates()", "0, x509_verify.tls_max_intermediates()"),
    ("an unknown issuer reported as x509-decode", "slot.cho",
     "    if code == x509_verify.unknown_issuer() {\n        return tls_record.x509_unknown_issuer();",
     "    if code == x509_verify.unknown_issuer() {\n        return tls_record.x509_decode();"),
    ("a message allowed to share a record with the next key", "client.cho",
     "    if code == 0 && new_key && at < ints[tls_slot.i_hs_fill()] {",
     "    if code == 0 && new_key && at < 0 {"),
    ("the engine's DRBG key not replaced", "tls.cho", "                key[i] = stream[i];\n", ""),
    ("the engine's slots overlapping", "tls.cho",
     "    return slot * tls_client.bytes_len();", "    return slot * (tls_client.bytes_len() / 2);"),
    # ---- Suites and HelloRetryRequest (docs/tls-parity.md §3.3) ----
    ("SHA-384's transcript never chosen", "slot.cho",
     "        if len(out) == 48 {\n            let copy = alloc_slice[r](crypto.sha512_state_len(), 0);\n            var k = 0;\n            while k < len(copy) {\n                copy[k] = ints[i_transcript384() + k];\n                k = k + 1;\n            }\n            crypto.sha384_final(copy, out);",
     "        if len(out) == 32 {\n            let copy = alloc_slice[r](crypto.sha512_state_len(), 0);\n            var k = 0;\n            while k < len(copy) {\n                copy[k] = ints[i_transcript384() + k];\n                k = k + 1;\n            }\n            crypto.sha384_final(copy, out);"),
    ("SHA-384's binder transcript never chosen", "slot.cho",
     "            crypto.sha384_update(copy, extra);", "            crypto.sha256_update(copy, extra);"),
    ("AES-256-GCM given SHA-256", "record.cho",
     "    if suite == suite_aes_256_gcm_sha384() || suite == 0xc02c || suite == 0xc030 {\n        return 48;",
     "    if suite == suite_aes_256_gcm_sha384() || suite == 0xc02c || suite == 0xc030 {\n        return 32;"),
    ("AES-128-GCM given a 32-byte key", "record.cho",
     "    if suite == suite_aes_128_gcm_sha256() || suite == 0xc02b || suite == 0xc02f {\n        return 16;",
     "    if suite == suite_aes_128_gcm_sha256() || suite == 0xc02b || suite == 0xc02f {\n        return 32;"),
    ("every suite sealed with ChaCha20", "record.cho",
     "    if chacha(suite) {\n        return chacha20.seal(", "    if true {\n        return chacha20.seal("),
    ("the Finished MAC always SHA-256", "client.cho", "        hmac.mac(h, key, th, out);", "        hmac.mac(32, key, th, out);"),
    ("message_hash with the wrong type", "client.cho", "        synthetic[0] = byte_of(254);", "        synthetic[0] = byte_of(253);"),
    ("the transcript not restarted after a retry", "client.cho",
     "        tls_slot.transcript_init(ints);\n        tls_slot.transcript_add(ints, synthetic);", "        tls_slot.transcript_add(ints, synthetic);"),
    ("the cookie not echoed", "client.cho",
     "        let cookie = message[4 + cookie_start..4 + cookie_end];", "        let cookie = message[4..4];"),
    ("a second HelloRetryRequest accepted", "client.cho",
     "            if info[tls_message.sh_retry()] == 1 {\n                code = tls_record.unexpected_message();",
     "            if info[tls_message.sh_retry()] == 2 {\n                code = tls_record.unexpected_message();"),
    ("the suite after a retry not compared", "client.cho",
     "            } else if suite != ints[tls_slot.i_suite()] {", "            } else if false {"),
    # Not here: the ServerHello's group not compared with the share sent. A share of
    # another group always has the wrong length for the key held, so the key exchange
    # refuses it with the same tag; the comparison is defence in depth, and that mutant
    # is equivalent (docs/tls-parity.md §3.3.1).
    ("a retry to X25519 accepted", "message.cho",
     "                if group != group_p256() && group != group_p384() {", "                if group == 0 {"),
    ("a retry that changes nothing accepted", "message.cho",
     "            if group == 0 && cookie == 0 {", "            if group == 0 && cookie == 0 && false {"),
    ("a change_cipher_spec after a retry refused", "client.cho",
     "        let early = state < tls_slot.state_wait_extensions() && !(state == tls_slot.state_wait_server_hello() && tls_slot.has(ints, tls_slot.f_retried()));",
     "        let early = state < tls_slot.state_wait_extensions();"),
    ("the retry's share for the wrong curve", "slot.cho",
     "    if group == tls_message.group_p256() {\n        return 256;", "    if group == tls_message.group_p256() {\n        return 384;"),
    ("the end of the socket taken for close_notify", "client.cho",
     "    if tls_slot.has(ints, tls_slot.f_close_received()) || ints[tls_slot.i_state()] == tls_slot.state_failed() {\n        return 0;",
     "    if true {\n        return 0;"),
    # ---- TLS 1.2 (docs/tls-parity.md §3.4) ----
    ("the PRF's A(i) not chained", "record.cho",
     "            hmac.init(hash_len, st, secret);\n            hmac.update(hash_len, st, a);\n            hmac.final(hash_len, st, a);",
     "            hmac.init(hash_len, st, secret);\n            hmac.final(hash_len, st, a);"),
    ("the PRF's label left out of A(1)", "record.cho",
     "        hmac.update(hash_len, st, label);\n        hmac.update(hash_len, st, seed);\n        hmac.final(hash_len, st, a);",
     "        hmac.update(hash_len, st, seed);\n        hmac.final(hash_len, st, a);"),
    ("the master secret without the extension's label", "client12.cho", '"extended master secret"', '"master secret"'),
    ("the key block's randoms in the wrong order", "client12.cho",
     "        tls_slot.copy_bytes(bytes[tls_slot.k_server_random()..tls_slot.k_server_random() + 32], seed[0..32]);\n        tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], seed[32..64]);",
     "        tls_slot.copy_bytes(bytes[tls_slot.k_server_random()..tls_slot.k_server_random() + 32], seed[32..64]);\n        tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], seed[0..32]);"),
    ("the client writing with the server's key", "client12.cho",
     "        tls_slot.copy_bytes(block[0..kl], bytes[tls_slot.k_write_key()..tls_slot.k_write_key() + kl]);",
     "        tls_slot.copy_bytes(block[kl..2 * kl], bytes[tls_slot.k_write_key()..tls_slot.k_write_key() + kl]);"),
    ("AES-GCM's explicit nonce not read from the record", "record.cho",
     "        out[k] = explicit[k - 4];", "        out[k] = byte_of(0);"),
    ("the explicit nonce sent as zeros", "record.cho",
     "            out[5 + k] = byte_of(seq >> 8 * (7 - k) & 255);", "            out[5 + k] = byte_of(0);"),
    ("the additional data without the sequence number", "record.cho",
     "        out[k] = byte_of(seq >> 8 * (7 - k) & 255);\n        k = k + 1;\n    }\n    out[8]",
     "        out[k] = byte_of(0);\n        k = k + 1;\n    }\n    out[8]"),
    ("the additional data with the ciphertext's length", "record.cho",
     "        aad12(seq, content_type, n, ad);\n        code = aead_open(", "        aad12(seq, content_type, body, ad);\n        code = aead_open("),
    ("TLS 1.2 ChaCha20's nonce built as AES-GCM's", "record.cho",
     "    if chacha(suite) {\n        return nonce(iv, seq, out);", "    if false {\n        return nonce(iv, seq, out);"),
    ("the extended master secret not required", "message.cho",
     "        if !ems {\n            return tls_record.extended_master_secret();", "        if false {\n            return tls_record.extended_master_secret();"),
    ("a TLS 1.2 ServerHello echoing the session id accepted", "message.cho",
     "            if same {\n                return tls_record.decode_error();", "            if same && false {\n                return tls_record.decode_error();"),
    ("a key_share in a TLS 1.2 ServerHello accepted", "message.cho",
     "        if group != 0 || psk {\n            return tls_record.unsupported_extension();", "        if psk {\n            return tls_record.unsupported_extension();"),
    ("a pre_shared_key in a TLS 1.2 ServerHello accepted", "message.cho",
     "        if group != 0 || psk {\n            return tls_record.unsupported_extension();", "        if group != 0 {\n            return tls_record.unsupported_extension();"),
    ("the key exchange's randoms not signed", "client12.cho",
     "            tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], content[0..32]);\n", ""),
    ("the suite's kind of key not checked", "client12.cho",
     "            if tls_record.suite12_ecdsa(ints[tls_slot.i_suite()]) == rsa_key {", "            if false {"),
    ("the server's Finished computed as the client's", "client12.cho",
     'verify_data(ints, bytes, "server finished", want);', 'verify_data(ints, bytes, "client finished", want);'),
    ("the server's Finished not compared", "client12.cho",
     "    if diff != 0 {\n        return tls_record.bad_finished();", "    if diff != diff {\n        return tls_record.bad_finished();"),
    ("a HelloRequest accepted", "client12.cho", "        return tls_record.renegotiation();", "        return 0;"),
    ("the ClientKeyExchange's point length one short", "client12.cho", "            cke[4] = byte_of(size);", "            cke[4] = byte_of(size - 1);"),
    ("TLS 1.2 after a HelloRetryRequest accepted", "client.cho",
     "        if code == 0 && tls12 && tls_slot.has(ints, tls_slot.f_retried()) {", "        if code == 0 && tls12 && false {"),
    ("the server's key exchange point never kept", "client12.cho",
     "            tls_slot.copy_bytes(point, bytes[tls_slot.k_peer()..tls_slot.k_peer() + len(point)]);\n", ""),
    # Resumption (docs/tls-resumption.md).
    ("the binder over the whole ClientHello, not the truncated one", "client.cho",
     "tls_slot.transcript_hash_with(ints, hello[0..n - tls_message.binders_len(h)], th);",
     "tls_slot.transcript_hash_with(ints, hello[0..n], th);"),
    ("the binder key under the external label", "client.cho",
     'hkdf.derive_secret(h, early, "res binder", empty_hash, binder_key);',
     'hkdf.derive_secret(h, early, "ext binder", empty_hash, binder_key);'),
    ("a resumption's early secret from zeros, not the PSK", "client.cho",
     "            if tls_slot.has(ints, tls_slot.f_resumed()) {\n                early_secret(",
     "            if false {\n                early_secret("),
    ("a pre_shared_key accepted when none was offered", "client.cho",
     "if !tls_slot.has(ints, tls_slot.f_psk_offered()) {\n                    code = tls_record.unsupported_extension();",
     "if false {\n                    code = tls_record.unsupported_extension();"),
    ("a suite with another hash accepted for the ticket", "client.cho",
     "} else if tls_record.hash_len(suite) != ints[tls_slot.i_offer_hash()] {\n                    code = tls_record.illegal_psk();",
     "} else if false {\n                    code = tls_record.illegal_psk();"),
    ("a resumption still waiting for a Certificate", "client.cho",
     "            if tls_slot.has(ints, tls_slot.f_resumed()) {\n                // No Certificate",
     "            if false {\n                // No Certificate"),
    ("the resumption master secret under another label", "client.cho",
     '"res master", th2,', '"res mastr", th2,'),
    ("a ticket's PSK without its nonce", "client.cho",
     '"resumption", body[info[tls_message.nst_nonce_start()]..info[tls_message.nst_nonce_end()]],',
     '"resumption", "",'),
    ("the ticket still offered after a retry to another hash", "client.cho",
     "        tls_slot.clear_flag(ints, tls_slot.f_psk_offered());", ""),
    ("a lifetime over 7 days kept as given", "client.cho",
     "        if lifetime > 604800 {\n            lifetime = 604800;", "        if lifetime > 704800 {\n            lifetime = 604800;"),
    ("a ticket over the room kept", "client.cho",
     "te - ts <= tls_slot.ticket_cap()", "te - ts <= 4096"),
    ("a selected identity other than 0 accepted", "message.cho",
     "            if get(b, body, 2) != 0 {\n                return tls_record.illegal_psk();",
     "            if false {\n                return tls_record.illegal_psk();"),
    ("a resumption without a key share not refused as tls-key-share", "message.cho",
     "        } else if group == 0 && psk {", "        } else if false {"),
    ("the ticket offered to a name of the same length", "tls.cho",
     "        if contents(engine.tickets)[at + e_host() + k] != host[k] {\n            same = false;",
     "        if false {\n            same = false;"),
    ("the ticket offered after the trust store changed", "tls.cho",
     " || contents(engine.tmeta)[tf(e, 10)] != contents(engine.tmeta)[t_trust()] {", " {"),
    ("the ticket offered after the leaf's notAfter", "tls.cho",
     "same && now_s <= contents(engine.tmeta)[tf(e, 9)] && ", "same && "),
    ("the ticket offered past the maximum age", "tls.cho",
     " && now_s < contents(engine.tmeta)[tf(e, 8)] + contents(engine.tmeta)[t_max_age()]", ""),
    ("the ticket offered past its lifetime", "tls.cho",
     " && now_ms < received + contents(engine.tmeta)[tf(e, 6)] * 1000", ""),
    ("the ticket offered with the clock before it was received", "tls.cho",
     " && now_ms >= received;", ";"),
    ("a ticket's age counted from a whole second", "tls.cho",
     "    let age = now_unix_ms - contents(engine.tmeta)[tf(e, 5)] + ",
     "    let age = now_unix_ms - contents(engine.tmeta)[tf(e, 5)] / 1000 * 1000 + "),
    ("the ticket offered twice", "tls.cho",
     "        tls_slot.zero(random);\n    }\n    wipe(engine, e);", "        tls_slot.zero(random);\n    }"),
    ("a forgotten pool kept", "tls.cho",
     "        if pool > 0 && in_pool(engine, e, pool) {\n            wipe(engine, e);",
     "        if pool < 0 && in_pool(engine, e, pool) {\n            wipe(engine, e);"),
    ("a forgotten pool keeping its last ticket", "tls.cho",
     "        if pool > 0 && in_pool(engine, e, pool) {\n            wipe(engine, e);",
     "        if pool > 0 && in_pool(engine, e, pool) && held(engine, pool) > 1 {\n            wipe(engine, e);"),
    ("a pool over its size keeping its oldest", "tls.cho",
     "    while held(engine, p) >= contents(engine.tmeta)[t_per_pool()] {",
     "    while false && held(engine, p) >= contents(engine.tmeta)[t_per_pool()] {"),
    ("a pool offering its oldest ticket first", "tls.cho",
     "        e = pick(engine, pool, true);", "        e = pick(engine, pool, false);"),
    ("a ticket the rules refuse kept in its pool", "tls.cho",
     "!may_offer(engine, c, host, now_unix_ms) {\n            wipe(engine, c);",
     "!may_offer(engine, c, host, now_unix_ms) {\n            c = c + 0;"),
    ("a refused ticket not skipped for an older one", "tls.cho",
     "        if pool > 0 && in_pool(engine, c, pool) && !may_offer(engine, c, host, now_unix_ms) {",
     "        if pool > 0 && in_pool(engine, c, pool) && !may_offer(engine, c, host, now_unix_ms) && false {"),
    ("a handle never issued taken as a pool", "tls.cho",
     "    if p <= 0 || p >= contents(engine.tmeta)[t_next_pool()] {", "    if p <= 0 {"),
    ("every save a new pool", "tls.cho",
     "    if p <= 0 || p >= contents(engine.tmeta)[t_next_pool()] {", "    if true {"),
    ("a close_notify before the handshake taken as a clean close (review E-3)", "client.cho",
     "        if ints[tls_slot.i_state()] != tls_slot.state_connected() {\n            return tls_record.peer_closed();\n        }\n", ""),
    ("a read key installed without preparing it (step 2 of docs/crypto-builtins.md)", "slot.cho",
     "    tls_record.prepare(ints[i_suite()], bytes[k_read_key()..k_read_key() + key_len(ints)], ints[i_read_aead()..i_read_aead() + tls_record.context_len()]);\n", ""),
    ("a write key installed without preparing it", "slot.cho",
     "    tls_record.prepare(ints[i_suite()], bytes[k_write_key()..k_write_key() + key_len(ints)], ints[i_write_aead()..i_write_aead() + tls_record.context_len()]);\n", ""),
    ("TLS 1.2's keys not prepared", "client12.cho", "    tls_slot.prepare_keys(ints, bytes);\n", ""),
    ("psk_key_exchange_modes sent only with a ticket, so no server need send one", "message.cho",
     "    if modes || len(ticket) > 0 {", "    if len(ticket) > 0 {"),
    ("resumption never advertised by the engine", "tls.cho",
     "0, 0, 0, contents(engine.tmeta)[t_resume()] == 1);", "0, 0, 0, false);"),
    ("the obfuscated age without ticket_age_add", "tls.cho",
     "contents(engine.tmeta)[tf(e, 5)] + contents(engine.tmeta)[tf(e, 7)];", "contents(engine.tmeta)[tf(e, 5)];"),
    # ---- Review findings (#209) ----
    ("a TLS 1.2 record's header version not checked (review E-1)", "record.cho",
     "    if int_of(record[1]) != 3 || int_of(record[2]) != 3 {\n        return protocol_version();",
     "    if false {\n        return protocol_version();"),
    ("send admitting 22 bytes over the content whatever the suite (review E-2)", "client.cho",
     "    let overhead = tls_slot.record_overhead(ints);", "    let overhead = 22;"),
    ("send keeping no room for close_notify (review E-4)", "client.cho",
     "if tls_slot.out_free(ints) < n + overhead + 2 + overhead {", "if tls_slot.out_free(ints) < n + overhead {"),
    ("a KeyUpdate allowed to share a record with the next message (review E-8)", "client.cho",
     " || int_of(message[0]) == tls_message.type_key_update();", ";"),
]


def cases():
    """Every connection `conformance/tls.rs` replays, as (name, lines, answers)."""
    out = []
    names = sorted(os.listdir(os.path.join(ROOT, "tests/vectors/tls")))
    for name in [n for n in names if n.startswith("tlslite_") or n.startswith("openssl12_")]:
        asked, answered = [], []
        for line in open(os.path.join(ROOT, "tests/vectors/tls", name)):
            line = line.rstrip("\n")
            if line.startswith("= "):
                answered.append(line[2:])
            elif not line.startswith("#"):
                asked.append(line)
        out.append((name, asked, answered))
    for line in open(os.path.join(ROOT, "tests/vectors/tls/liar.txt")):
        line = line.rstrip("\n")
        if line.startswith("## "):
            out.append((line[3:], [], []))
        elif line.startswith("= "):
            out[-1][2].append(line[2:])
        elif not line.startswith("#"):
            out[-1][1].append(line)
    return out


# Mutants that change no behaviour the client can reach, each with the argument. Such a mutant must survive;
# one that is killed was not equivalent, and the run fails.
EQUIVALENT = {}


def ticket_cases():
    """`tests/vectors/tls/tickets.txt`'s cases, as (name, lines, answers)."""
    out = []
    for line in open(os.path.join(ROOT, "tests/vectors/tls/tickets.txt")):
        line = line.rstrip("\n")
        if line.startswith("## "):
            out.append((line[3:], [], []))
        elif line.startswith("= "):
            out[-1][2].append(line[2:])
        elif not line.startswith("#"):
            out[-1][1].append(line)
    return out


def build(cancho, program, pkg, out, engine=False):
    files = [os.path.join(pkg, f) for f in (FILES if engine else FILES[:5])]
    r = subprocess.run([cancho, "build", "--std", os.path.join(ROOT, "tests/programs", program), *files,
                        *[os.path.join(ROOT, "packages/x509", f) for f in ["verify.cho", "names.cho", "x509.cho"]],
                        "-o", out], capture_output=True, text=True)
    return r.returncode == 0, r.stderr


def replay(driver, connections):
    """The first connection whose answers differ, or None."""
    for name, asked, answered in connections:
        try:
            r = subprocess.run([driver], input="\n".join(asked) + "\n", capture_output=True, text=True, timeout=60)
        except subprocess.TimeoutExpired:
            return f"{name}: timed out"
        if r.stdout.splitlines() != answered:
            return f"{name} (exit {r.returncode})"
    return None


def read_record(conn, got):
    while True:
        if len(got) >= 5 and len(got) >= 5 + int.from_bytes(got[3:5], "big"):
            n = 5 + int.from_bytes(got[3:5], "big")
            rec = bytes(got[:n])
            del got[:n]
            return rec
        try:
            chunk = conn.recv(65536)
        except OSError:
            return None
        if not chunk:
            return None
        got += chunk


def serve_streams(tls_many, chunk, cut):
    """`tls_many` against the recorded streams; None when it ends as it must."""
    streams, seed, want = {}, None, []
    for line in open(os.path.join(ROOT, "tests/vectors/tls/streams.txt")):
        if "seed " in line:
            seed = line.split("seed ")[1][:64]
        if line.startswith("#"):
            continue
        hello, flight, reply, n, digest = line.split()
        streams[bytes.fromhex(hello)] = (bytes.fromhex(flight), bytes.fromhex(reply))
        want.append(f"{n} {digest}")
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(128)

    def handle(conn):
        try:
            got = bytearray()
            hello = read_record(conn, got)
            if hello not in streams:
                return
            flight, reply = streams[hello]
            conn.sendall(flight)
            seen = 0
            while seen < 2:
                r = read_record(conn, got)
                if r is None:
                    return
                seen += r[0] == 23
            if cut:
                conn.sendall(reply[:-24])
                conn.shutdown(socket.SHUT_WR)
            else:
                conn.sendall(reply)
            while read_record(conn, got) is not None:
                pass
        except OSError:
            pass
        finally:
            conn.close()

    def accept():
        for _ in range(64):
            try:
                conn, _ = listener.accept()
            except OSError:
                return
            threading.Thread(target=handle, args=(conn,), daemon=True).start()

    threading.Thread(target=accept, daemon=True).start()
    pem = open(os.path.join(ROOT, "tests/vectors/tls/streams.pem"), "rb").read()
    try:
        r = subprocess.run([tls_many, "127.0.0.1", str(listener.getsockname()[1]), "liar.lex-sys.test", "64",
                            chunk, seed], input=pem, capture_output=True, timeout=90)
    except subprocess.TimeoutExpired:
        listener.close()
        return "streams: timed out"
    listener.close()
    lines = r.stdout.decode().splitlines()
    if cut:
        if lines[-1:] != ["done ok=0 failed=64"] or any(l.split()[2] != "tls-peer-closed" for l in lines[:64]):
            return f"streams cut short: {lines[-1:]}"
        return None
    if lines[-1:] != ["done ok=64 failed=0"] or sorted(" ".join(l.split()[3:5]) for l in lines[:64]) != sorted(want):
        return f"streams: {lines[-1:]}"
    return None


def evidence(cancho, pkg, work, engine):
    """None when the package passes everything, else what failed."""
    driver = os.path.join(work, "driver")
    ok, err = build(cancho, "tls_driver.cho", pkg, driver)
    if not ok:
        return "BUILD " + err.strip().splitlines()[0]
    found = replay(driver, cases())
    if found:
        return found
    tickets = os.path.join(work, "tickets")
    ok, err = build(cancho, "tls_tickets.cho", pkg, tickets, engine=True)
    if not ok:
        return "BUILD " + err.strip().splitlines()[0]
    found = replay(tickets, ticket_cases())
    if found or not engine:
        return found
    many = os.path.join(work, "tls_many")
    ok, err = build(cancho, "tls_many.cho", pkg, many, engine=True)
    if not ok:
        return "BUILD " + err.strip().splitlines()[0]
    return serve_streams(many, "1", False) or serve_streams(many, "65536", False) or serve_streams(many, "65536", True)


def main():
    cancho = sys.argv[1]
    # `--only <text>`: just the mutants whose name contains it (the unmutated package still runs first).
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    work = tempfile.mkdtemp(prefix="tls-mutants-")
    pkg = os.path.join(work, "tls")
    src = os.path.join(ROOT, "packages/tls")
    shutil.copytree(src, pkg)
    base = evidence(cancho, pkg, work, True)
    if base:
        print(f"the unmutated package fails: {base}")
        sys.exit(1)
    print("unmutated: passes")
    survived = 0
    run = [m for m in MUTANTS if only is None or only in m[0]]
    for name, file, old, new in run:
        text = open(os.path.join(src, file)).read()
        assert text.count(old) == 1, f"{name}: the text occurs {text.count(old)} times"
        open(os.path.join(pkg, file), "w").write(text.replace(old, new))
        found = evidence(cancho, pkg, work, file == "tls.cho" or "end of the socket" in name)
        shutil.copy(os.path.join(src, file), os.path.join(pkg, file))
        if name in EQUIVALENT:
            if found is None:
                print(f"equivalent {name}: {EQUIVALENT[name]}")
            else:
                survived += 1
                print(f"KILLED, so not equivalent: {name}: {found}")
        elif found is None or found.startswith("BUILD"):
            survived += 1
            print(f"SURVIVED {name}" + (f": {found}" if found else ""))
        else:
            print(f"killed   {name}: {found}")
    shutil.rmtree(work, ignore_errors=True)
    equivalent = sum(1 for m in run if m[0] in EQUIVALENT)
    print(f"{len(run) - survived - equivalent} of {len(run)} mutants killed, {equivalent} equivalent (argued in EQUIVALENT)")
    sys.exit(1 if survived else 0)


if __name__ == "__main__":
    main()
