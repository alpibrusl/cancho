#!/usr/bin/env python3
"""Mutation check of `packages/tls`'s server (docs/tls-server.md §8, step 2), the shape of `scripts/tls_mutants.py`.

    python3 scripts/tls_server_mutants.py <cancho binary> [--only <text in a mutant's name>]

Each mutant is one of the server's files (`hello.cho`, `identity.cho`, `server.cho`, and the server's parts of
`tls.cho` and `slot.cho`) with one deliberate bug. The package is copied to a scratch directory, the mutant
applied there, and `tests/programs/tls_server_driver.cho` built against it. It replays every connection of
`tests/vectors/tls/liar_client.txt` (`scripts/tls_liar_client.py`), each answer compared byte for byte, as
`crates/cancho/tests/conformance/tls_server.rs` does. A mutant is killed when any answer differs or the driver
traps. The unmutated package is run first and must pass. A mutant that changes nothing a client can reach is
listed in EQUIVALENT with the argument, and must survive. Exit status 1 if a mutant survives or fails to build.
"""
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TLS = ["tls.cho", "record.cho", "message.cho", "slot.cho", "client12.cho", "client.cho", "hello.cho", "identity.cho",
       "ticket.cho", "server.cho"]
X509 = ["verify.cho", "names.cho", "x509.cho", "key.cho"]

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    # ---- The ClientHello's rules (hello.cho) ----
    ("AES-128-GCM first whatever the CPU", "hello.cho", "    if gcm && mask & 1 != 0 {", "    if mask & 1 != 0 {"),
    ("AES-256-GCM taken for ChaCha20", "hello.cho",
     "    if suite == tls_record.suite_aes_256_gcm_sha384() {\n        return 2;",
     "    if suite == tls_record.suite_aes_256_gcm_sha384() {\n        return 4;"),
    ("TLS 1.3 not required in supported_versions", "hello.cho",
     "    if !versions || !tls13 {", "    if !versions {"),
    ("a compression method other than null allowed", "hello.cho", "    if !null_only {", "    if false {"),
    ("key_share not required", "hello.cho",
     "    if !sigalgs || !groups_seen || !shares_seen || info[ch_psk_count()] != 0 && info[ch_modes_seen()] == 0 {", "    if !sigalgs || !groups_seen || info[ch_psk_count()] != 0 && info[ch_modes_seen()] == 0 {"),
    ("signature_algorithms not required", "hello.cho",
     "    if !sigalgs || !groups_seen || !shares_seen || info[ch_psk_count()] != 0 && info[ch_modes_seen()] == 0 {", "    if !groups_seen || !shares_seen || info[ch_psk_count()] != 0 && info[ch_modes_seen()] == 0 {"),
    ("a share outside supported_groups allowed", "hello.cho",
     "    if shared & info[ch_groups()] != shared {", "    if false {"),
    ("ecdsa_secp256r1_sha256 not required", "hello.cho", "    if !p256_sha256 {", "    if false {"),
    ("no group in common not refused", "hello.cho",
     "    if info[ch_groups()] == 0 {\n        return tls_record.server_group();", "    if false {\n        return tls_record.server_group();"),
    ("an extension twice allowed", "hello.cho",
     "                    seen[kind >> 3] = byte_of(int_of(seen[kind >> 3]) | 1 << (kind & 7));\n", ""),
    ("pre_shared_key allowed before others", "hello.cho",
     "                        if body + size != ext_end {", "                        if false {"),
    ("a share's length not checked", "hello.cho",
     "            if seen & bit != 0 || n != tls_message.share_len(group) {", "            if seen & bit != 0 {"),
    ("two shares of one group allowed", "hello.cho",
     "            if seen & bit != 0 || n != tls_message.share_len(group) {",
     "            if n != tls_message.share_len(group) {"),
    ("two host names allowed", "hello.cho",
     "            if seen {\n                return tls_record.server_illegal_parameter();",
     "            if false {\n                return tls_record.server_illegal_parameter();"),
    ("an empty ALPN name allowed", "hello.cho", "                                if m == 0 || q + 1 + m > body + size {",
     "                                if q + 1 + m > body + size {"),
    ("a HelloRetryRequest to P-256 before X25519", "hello.cho",
     "    let mask = info[ch_groups()];\n    if mask & 1 != 0 {", "    let mask = info[ch_groups()];\n    if mask & 1 == 2 {"),
    ("a P-256 share taken before an X25519 one", "hello.cho",
     "    if info[ch_share_x25519()] != 0 {\n        return tls_message.group_x25519();",
     "    if info[ch_share_x25519()] != 0 && info[ch_share_p256()] == 0 {\n        return tls_message.group_x25519();"),
    ("ALPN: no protocol ever matches", "hello.cho", "                if same {\n                    return p;",
     "                if !same {\n                    return p;"),
    ("the HelloRetryRequest random not SHA-256(\"HelloRetryRequest\")", "hello.cho",
     "        out[at + k] = byte_of(tls_message.hrr_random(k));", "        out[at + k] = byte_of(k);"),
    ("server_name never acknowledged", "hello.cho", "    if sni_used {", "    if false {"),
    ("CertificateVerify naming rsa_pss_rsae_sha256", "hello.cho",
     "    at = tls_message.put(out, at, tls_message.ecdsa_p256_sha256(), 2);", "    at = tls_message.put(out, at, 0x0804, 2);"),
    ("early_data offered not noticed", "hello.cho", "                        info[ch_early()] = 1;\n", ""),
    ("a ClientHello of 16 KiB refused", "hello.cho", "    return 16384;", "    return 16383;"),
    ("an over-long host name kept", "hello.cho", "            if n <= 255 {", "            if n <= 4096 {"),
    # ---- The handshake (server.cho) ----
    ("the client's Finished not checked", "server.cho",
     "            if diff != 0 {\n                code = tls_record.server_finished();",
     "            if diff != diff {\n                code = tls_record.server_finished();"),
    ("only the Finished's first byte compared", "server.cho",
     "                diff = diff | int_of(want[k]) ^ int_of(message[4 + k]);\n                k = k + 1;",
     "                diff = diff | int_of(want[k]) ^ int_of(message[4 + k]);\n                k = k + h;"),
    ("the client's application key never read under", "server.cho",
     "        issue_tickets(ints, bytes, cfg, tk);\n        tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_ap());",
     "        issue_tickets(ints, bytes, cfg, tk);\n"),
    ("the server's application key never written under", "server.cho",
     "    tls_slot.set_write_keys(ints, bytes, tls_slot.k_server_ap());\n    tls_slot.set_read_keys(", "    tls_slot.set_read_keys("),
    ("the flight sealed under the client's handshake key", "server.cho",
     "    tls_slot.set_write_keys(ints, bytes, tls_slot.k_server_hs());", "    tls_slot.set_write_keys(ints, bytes, tls_slot.k_client_hs());"),
    ("the application secrets' labels swapped", "server.cho", '"c ap traffic", th, bytes[tls_slot.k_client_ap()',
     '"s ap traffic", th, bytes[tls_slot.k_client_ap()'),
    ("message_hash with the wrong type", "server.cho", "        synthetic[0] = byte_of(254);", "        synthetic[0] = byte_of(253);"),
    ("the transcript not restarted after a retry", "server.cho",
     "        tls_slot.transcript_init(ints);\n        tls_slot.transcript_add(ints, synthetic);",
     "        tls_slot.transcript_add(ints, synthetic);"),
    ("change_cipher_spec sent twice after a retry", "server.cho",
     "    if tls_slot.has(ints, tls_slot.f_ccs_sent()) {\n        return 0;\n    }\n", ""),
    ("the CertificateVerify context string misspelled", "server.cho",
     'let label = "TLS 1.3, server CertificateVerify";', 'let label = "TLS 1.3, client CertificateVerify";'),
    ("the signature not hedged (RFC 6979's nonce alone)", "server.cho",
     "bytes[tls_slot.k_sign_extra()..tls_slot.k_sign_extra() + 32], sig,", "bytes[0..0], sig,"),
    ("the signature checked under another point", "server.cho",
     "tls_identity.point(cfg, id), bytes[", "tls_identity.point(cfg, id + 1), bytes["),
    ("the name never chooses an identity", "server.cho",
     "    var id = tls_identity.select(cfg, bytes[tls_slot.b_sni()..tls_slot.b_sni() + n]);", "    var id = 0 - 1;"),
    ("the name not lowercased", "server.cho", "byte_of(sb.to_lower(int_of(body[s + k])))", "body[s + k]"),
    ("ALPN with nothing in common accepted", "server.cho", "    if pick < 0 {\n        return tls_record.server_alpn();",
     "    if pick < 0 {\n        return 0;"),
    ("the second ClientHello's session id not compared", "server.cho",
     "            if n != ints[tls_slot.i_session_len()] || !sb.equal(", "            if false && !sb.equal("),
    ("the second ClientHello's suite not checked", "server.cho",
     "info[tls_hello.ch_suites()] & tls_hello.suite_bit(ints[tls_slot.i_suite()]) == 0 || ", ""),
    ("early data allowed in the second ClientHello", "server.cho", " || info[tls_hello.ch_early()] != 0 {", " {"),
    ("the second ClientHello's share not required", "server.cho",
     "            } else if tls_hello.share_at(info, ints[tls_slot.i_group()]) == 0 {", "            } else if false {"),
    ("early data of exactly 16 KiB refused", "server.cho", "    if ints[tls_slot.i_early_skipped()] > 16384 {",
     "    if ints[tls_slot.i_early_skipped()] >= 16384 {"),
    ("early data skipped after a record opened", "server.cho", "    tls_slot.clear_flag(ints, tls_slot.f_early());\n", ""),
    ("change_cipher_spec before the ClientHello taken", "server.cho", " || tls_slot.has(ints, tls_slot.f_ccs_seen()) || !between {",
     " || tls_slot.has(ints, tls_slot.f_ccs_seen()) {"),
    ("a second change_cipher_spec taken", "server.cho", " || tls_slot.has(ints, tls_slot.f_ccs_seen()) || !between {",
     " || !between {"),
    ("a message allowed to share a record with what follows", "server.cho", "            } else if have > 4 + n {",
     "            } else if false {"),
    ("a ClientHello over 16 KiB taken", "server.cho", "            } else if hello && n > tls_hello.max_client_hello() {",
     "            } else if hello && n > tls_slot.hs_cap() {"),
    ("a plaintext alert after the flight not read", "server.cho",
     "    let plain_alert = kind == tls_record.type_alert() && state == tls_slot.state_wait_client_finished();",
     "    let plain_alert = false;"),
    ("a KeyUpdate not answered", "server.cho", "        if asked == 1 {", "        if asked == 2 {"),
    ("33 KeyUpdates allowed", "server.cho", "    if ints[tls_slot.i_key_updates()] > 32 {", "    if ints[tls_slot.i_key_updates()] > 33 {"),
    ("the read key after a KeyUpdate not changed", "server.cho",
     "        tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_ap());\n        if asked == 1 {",
     "        if asked == 1 {"),
    ("the shared secret not checked for zero", "server.cho",
     "        if x25519.scalarmult(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], peer, secret) != 0 {",
     "        if x25519.scalarmult(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], peer, secret) == 99 {"),
    ("the handshake secret's label wrong", "server.cho", '"s hs traffic", th,', '"c hs traffic", th,'),
    ("the early secret over a non-zero PSK", "server.cho", "        hkdf.extract(h, zeros, zeros, early);",
     "        hkdf.extract(h, zeros, empty_hash, early);"),
    # ---- Identities (identity.cho) ----
    ("an expired leaf accepted", "identity.cho", "                    } else if view[x509.not_after()] < now {",
     "                    } else if false {"),
    ("a key mismatch reported as a key type", "identity.cho", "            if m == -92 {", "            if m == -93 {"),
    ("a leaf of another curve accepted", "identity.cho",
     " || view[x509.key_curve()] != x509.oid_p256() {", " {"),
    ("names matched by any pattern", "identity.cho", "                if e > p && x509_names.dns_matches(names[p..e], host) {",
     "                if e > p {"),
    ("an ALPN name over 255 bytes taken", "identity.cho", "                if e - p > 255 || n + 1 + e - p > alpn_cap() {",
     "                if n + 1 + e - p > alpn_cap() {"),
    ("identities beyond 16 allowed to be counted full", "identity.cho",
     "    return id >= 0 && id < max_identities() && int_of(cfg[at(id)]) == 1;",
     "    return id >= 0 && id < max_identities() - 1 && int_of(cfg[at(id)]) == 1;"),
    ("names longer than the room accepted", "identity.cho", "    if !keep_names && len(names) > names_cap() {",
     "    if false {"),
    ("a replacement written before its checks", "identity.cho", "        if code == 0 {\n            let b = at(id);",
     "        if code == 0 || code == tls_record.server_key_mismatch() {\n            let b = at(id);"),
    # ---- The engine (tls.cho) and alerts (slot.cho) ----
    ("serve on a client engine", "tls.cho",
     "pub fn serve[&e](engine: &!e Engine, slot: int, now_unix_ms: int) -> [] int {\n    if !is_server(engine) {",
     "pub fn serve[&e](engine: &!e Engine, slot: int, now_unix_ms: int) -> [] int {\n    if false {"),
    ("start on a server engine", "tls.cho",
     "pub fn start[&e, &h](engine: &!e Engine, slot: int, host: &h [byte], now_unix_ms: int) -> [] int {\n    if is_server(engine) {",
     "pub fn start[&e, &h](engine: &!e Engine, slot: int, host: &h [byte], now_unix_ms: int) -> [] int {\n    if false {"),
    ("serve with no identity", "tls.cho", "    if tls_identity.count(contents(engine.ids)) == 0 {", "    if false {"),
    ("a 17th identity added", "tls.cho", "    if id == tls_identity.max_identities() {\n        return tls_record.server_identities_full();",
     "    if id == tls_identity.max_identities() + 1 {\n        return tls_record.server_identities_full();"),
    ("handshakes in progress not counted", "tls.cho", "            n = n + 1;\n        }\n        s = s + 1;",
     "            n = n + 0;\n        }\n        s = s + 1;"),
    ("no_application_protocol sent as handshake_failure", "slot.cho", "        return 120;", "        return 40;"),
    ("missing_extension sent as decode_error", "slot.cho", "        return 109;", "        return 50;"),
    # ---- Session tickets (docs/tls-server.md §12): the rules (ticket.cho) ----
    ("a ticket never expires", "ticket.cho", "    if now_ms >= t_expiry(plain) {", "    if false {"),
    ("a ticket expires one millisecond late", "ticket.cho", "    if now_ms >= t_expiry(plain) {", "    if now_ms > t_expiry(plain) {"),
    ("an age window of 60 s", "ticket.cho", "    return 30000;", "    return 60000;"),
    ("an age window of 10 s", "ticket.cho", "    return 30000;", "    return 10000;"),
    ("a client age below the server's allowed", "ticket.cho", "    if d < 0 {\n        d = 0 - d;\n    }",
     "    if d < 0 {\n        d = 0;\n    }"),
    ("the obfuscated age not undone", "ticket.cho",
     "    let client_age = (obfuscated_age - t_age_add(plain) + 4294967296) % 4294967296;",
     "    let client_age = obfuscated_age % 4294967296;"),
    ("the host name not compared", "ticket.cho", "    if !bytes.equal(t_sni(plain), sni) {", "    if false {"),
    ("the certificate's fingerprint not compared", "ticket.cho", "    if !bytes.equal(t_fp(plain), fp) {", "    if false {"),
    ("the suite's hash not compared", "ticket.cho", "    if t_hash_len(plain) != hash_len {", "    if false {"),
    ("the ALPN protocol not compared", "ticket.cho", "    if !bytes.equal(t_alpn(plain), alpn) {", "    if false {"),
    ("a ticket of a client that authenticated accepted", "ticket.cho", "    if t_auth(plain) != 0 {", "    if false {"),
    ("trailing bytes in a ticket's plaintext allowed", "ticket.cho",
     "    return at + 1 + s + 1 + a == len(plain);", "    return at + 1 + s + 1 + a <= len(plain);"),
    ("a ticket's version not checked", "ticket.cho", "    if len(plain) < 25 || int_of(plain[0]) != 1 {", "    if len(plain) < 25 {"),
    ("a PSK of any length", "ticket.cho", "    if h != 32 && h != 48 {", "    if h == 0 {"),
    ("a retired key opens for ever", "ticket.cho", "    return until == 0 || now_ms < until;", "    return true;"),
    ("a retired key opens a millisecond past its end", "ticket.cho", "    return until == 0 || now_ms < until;",
     "    return until == 0 || now_ms <= until;"),
    ("the ring evicts its newest key", "ticket.cho",
     "tls_message.get(tk, kat(s) + e_created(), 8) < tls_message.get(tk, kat(best) + e_created(), 8)",
     "tls_message.get(tk, kat(s) + e_created(), 8) > tls_message.get(tk, kat(best) + e_created(), 8)"),
    ("a rotation does not end the retired key", "ticket.cho",
     "                tls_message.put(tk, kat(cur) + e_until(), now_ms + lifetime(tk) * 1000, 8);",
     "                tls_message.put(tk, kat(cur) + e_until(), 0, 8);"),
    ("keys the program dropped are kept", "ticket.cho", "            if keep[s] == 0 {\n                wipe_slot(tk, s);",
     "            if false {\n                wipe_slot(tk, s);"),
    ("a supplied previous key never ends", "ticket.cho", "            } else if fresh || s == before {", "            } else if false {"),
    ("the engine keeps rotating after the program supplies keys", "ticket.cho",
     "    tk[o_auto()] = byte_of(0);\n    return 0;", "    return 0;"),
    ("the engine's rotation a millisecond late", "ticket.cho",
     "    return now_ms >= tls_message.get(tk, kat(cur) + e_created(), 8) + lifetime(tk) * 1000;",
     "    return now_ms > tls_message.get(tk, kat(cur) + e_created(), 8) + lifetime(tk) * 1000;"),
    ("the engine makes keys with tickets off", "ticket.cho", "    if count(tk) == 0 || int_of(tk[o_auto()]) != 1 {",
     "    if int_of(tk[o_auto()]) != 1 {"),
    ("a count of 9 allowed", "ticket.cho", "    if n < 0 || n > max_count() ||", "    if n < 0 || n > max_count() + 1 ||"),
    ("a lifetime of 0 allowed", "ticket.cho", "lifetime_s < 1 ||", "lifetime_s < 0 ||"),
    ("a lifetime over 7 days allowed", "ticket.cho", "lifetime_s > max_lifetime() {", "lifetime_s > 2 * max_lifetime() {"),
    ("5 keys allowed", "ticket.cho", "    if len(keys) == 0 || len(keys) % 32 != 0 || n > max_keys() {",
     "    if len(keys) == 0 || len(keys) % 32 != 0 || n > max_keys() + 1 {"),
    ("a key list not a multiple of 32 allowed", "ticket.cho", "    if len(keys) == 0 || len(keys) % 32 != 0 || n > max_keys() {",
     "    if len(keys) == 0 || n > max_keys() {"),
    ("an empty key list allowed", "ticket.cho", "    if len(keys) == 0 || len(keys) % 32 != 0 || n > max_keys() {",
     "    if len(keys) % 32 != 0 || n > max_keys() {"),
    ("the salt left out of the ticket's key", "ticket.cho", "        hkdf.extract(32, salt, key, prk);",
     "        hkdf.extract(32, key, key, prk);"),
    ("the version left out of the associated data", "ticket.cho", "    out[0] = byte_of(1);\n    tls_slot.copy_bytes(ticket[0..48]",
     "    out[0] = byte_of(0);\n    tls_slot.copy_bytes(ticket[0..48]"),
    # ---- the ClientHello's PSK (hello.cho) ----
    ("psk_dhe_ke taken for psk_ke", "hello.cho", "info[ch_modes()] = info[ch_modes()] | 1 << int_of(b[q]);",
     "info[ch_modes()] = info[ch_modes()] | 2 >> int_of(b[q]);"),
    ("pre_shared_key without modes allowed", "hello.cho",
     " || info[ch_psk_count()] != 0 && info[ch_modes_seen()] == 0 {", " {"),
    ("binders and identities counted apart allowed", "hello.cho", "    if k != n {", "    if false {"),
    ("a binder shorter than 32 bytes allowed", "hello.cho", "        if m < 32 || p + 1 + m > at + size {",
     "        if p + 1 + m > at + size {"),
    ("an empty identity allowed", "hello.cho", "        if m == 0 || p + 2 + m + 4 > ids_end {",
     "        if p + 2 + m + 4 > ids_end {"),
    ("an identity running past its list allowed", "hello.cho", "        if m == 0 || p + 2 + m + 4 > ids_end {",
     "        if m == 0 {"),
    ("every identity read as the first", "hello.cho", "    var p = info[ch_psk_ids()];\n    var j = 0;\n    while j < k {",
     "    var p = info[ch_psk_ids()];\n    var j = 0;\n    while j < 0 {"),
    ("every binder read as the first", "hello.cho", "    var q = info[ch_psk_binders()] + 2;\n    j = 0;\n    while j < k {",
     "    var q = info[ch_psk_binders()] + 2;\n    j = 0;\n    while j < 0 {"),
    ("the identity selected always 0", "hello.cho", "        at = tls_message.put(out, at, psk, 2);",
     "        at = tls_message.put(out, at, 0, 2);"),
    ("a NewSessionTicket offering early data", "hello.cho",
     "    at = tls_message.put(out, at, 0, 2);\n    tls_message.put(out, 0, tls_message.type_new_session_ticket(), 1);",
     "    at = tls_message.put(out, at, 8, 2);\n    at = tls_message.put(out, at, 42, 2);\n    at = tls_message.put(out, at, 4, 2);\n"
     "    at = tls_message.put(out, at, 16384, 4);\n    tls_message.put(out, 0, tls_message.type_new_session_ticket(), 1);"),
    ("every ticket's nonce 0", "hello.cho", "    at = tls_message.put(out, at, nonce, 1);", "    at = tls_message.put(out, at, 0, 1);"),
    # ---- the handshake (server.cho) ----
    ("5 identities tried", "server.cho", "    if tries > 4 {\n        tries = 4;", "    if tries > 5 {\n        tries = 5;"),
    ("the binder not checked", "server.cho", "    return diff == 0;", "    return true;"),
    ("only the binder's first byte compared", "server.cho",
     "            diff = diff | int_of(want[k]) ^ int_of(got[k]);\n            k = k + 1;",
     "            diff = diff | int_of(want[k]) ^ int_of(got[k]);\n            k = k + h;"),
    ("a binder of any length", "server.cho", "    if len(got) != h {\n        return false;\n    }", ""),
    ("the binder over the ClientHello without the transcript before it", "server.cho",
     "        tls_slot.transcript_hash_with(ints, truncated, th);",
     "        if h == 48 {\n            crypto.sha384(truncated, th);\n        } else {\n            crypto.sha256(truncated, th);\n        }"),
    ("the binder over the whole ClientHello", "server.cho", "message[0..4 + info[tls_hello.ch_psk_binders()]]", "message[0..len(message)]"),
    ("the PSK not in the early secret", "server.cho",
     "        if tls_slot.has(ints, tls_slot.f_resumed()) {\n            hkdf.extract(h, zeros,",
     "        if false {\n            hkdf.extract(h, zeros,"),
    ("a resumption not noted", "server.cho", "        tls_slot.set_flag(ints, tls_slot.f_resumed());\n        ints[tls_slot.i_srv_verdict()]",
     "        ints[tls_slot.i_srv_verdict()]"),
    ("psk_ke accepted as psk_dhe_ke", "server.cho", "    if info[tls_hello.ch_modes()] & 2 == 0 {\n        ints[tls_slot.i_srv_verdict()] = tls_ticket.v_no_psk_dhe_ke();",
     "    if false {\n        ints[tls_slot.i_srv_verdict()] = tls_ticket.v_no_psk_dhe_ke();"),
    ("tickets off not honoured", "server.cho", "    if tls_ticket.count(tk) == 0 {\n        ints[tls_slot.i_srv_verdict()] = tls_ticket.v_off();",
     "    if false {\n        ints[tls_slot.i_srv_verdict()] = tls_ticket.v_off();"),
    ("tickets sent to a client without psk_dhe_ke", "server.cho", " || !tls_slot.has(ints, tls_slot.f_advertise())", ""),
    ("psk_ke counted as the client able to resume", "server.cho",
     "    if info[tls_hello.ch_modes()] & 2 != 0 {\n        tls_slot.set_flag(ints, tls_slot.f_advertise());",
     "    if info[tls_hello.ch_modes()] & 1 != 0 {\n        tls_slot.set_flag(ints, tls_slot.f_advertise());"),
    ("a ticket outliving its certificate", "server.cho",
     "    if tls_identity.not_after(cfg, id) - now / 1000 < lifetime {", "    if false {"),
    ("a lifetime in the ticket of seconds, not milliseconds", "server.cho", "now, now + lifetime * 1000, age_add", "now, now + lifetime, age_add"),
    ("more tickets than randomness drawn", "server.cho", "    if n > ints[tls_slot.i_srv_drawn()] {", "    if false {"),
    ("the client's Finished left out of the resumption secret", "server.cho",
     "        tls_slot.transcript_add(ints, message);\n        // The resumption master secret needs",
     "        // The resumption master secret needs"),
    ("every ticket of a connection with one PSK", "server.cho", "let index = alloc_slice[q](1, byte_of(k));",
     "let index = alloc_slice[q](1, byte_of(0));"),
    ("a wrong binder ignored", "server.cho", "                    code = tls_record.server_binder();", "                    accepted = 0 - 1;"),
    ("the binder's alert illegal_parameter", "slot.cho", "code == tls_record.server_finished() || code == tls_record.server_binder()",
     "code == tls_record.server_finished()"),
    # ---- the engine (tls.cho) and the identity (identity.cho) ----
    ("set_tickets on a client engine", "tls.cho",
     "pub fn set_tickets[&e](engine: &!e Engine, count: int, lifetime_s: int) -> [] int {\n    if !is_server(engine) {",
     "pub fn set_tickets[&e](engine: &!e Engine, count: int, lifetime_s: int) -> [] int {\n    if false {"),
    ("set_ticket_keys on a client engine", "tls.cho",
     "pub fn set_ticket_keys[&e, &k](engine: &!e Engine, keys: &k [byte], now_ms: int) -> [] int {\n    if !is_server(engine) {",
     "pub fn set_ticket_keys[&e, &k](engine: &!e Engine, keys: &k [byte], now_ms: int) -> [] int {\n    if false {"),
    ("rotate_ticket_key on a client engine", "tls.cho",
     "pub fn rotate_ticket_key[&e](engine: &!e Engine, now_ms: int) -> [] int {\n    if !is_server(engine) {",
     "pub fn rotate_ticket_key[&e](engine: &!e Engine, now_ms: int) -> [] int {\n    if false {"),
    ("set_time on a client engine", "tls.cho",
     "pub fn set_time[&e](engine: &!e Engine, now_ms: int) -> [] int {\n    if !is_server(engine) {",
     "pub fn set_time[&e](engine: &!e Engine, now_ms: int) -> [] int {\n    if false {"),
    ("a rotation before the engine is seeded", "tls.cho",
     "    if contents(engine.meta)[m_seeded()] != 1 {\n        return tls_record.no_entropy();\n    }\n    advance(engine, now_ms);",
     "    advance(engine, now_ms);"),
    ("the ticket randomness not drawn", "tls.cho",
     "            draw(engine, contents(engine.bytes)[b + tls_slot.b_ticket_random()..b + tls_slot.b_ticket_random() + 36 * n]);\n", ""),
    ("the engine never makes a ticket key", "tls.cho",
     "    if contents(engine.meta)[m_seeded()] == 1 && tls_ticket.due(", "    if false && tls_ticket.due("),
    ("the engine's clock never moves", "tls.cho", "    if now_ms > contents(engine.meta)[m_now(engine)] {", "    if false {"),
    ("a fingerprint of the whole chain", "identity.cho", "crypto.sha256(chain[3..3 + tls_message.get(chain, 0, 3)], cfg[b + o_fp()..b + o_fp() + 32]);",
     "crypto.sha256(chain[0..n], cfg[b + o_fp()..b + o_fp() + 32]);"),
    ("the fingerprint kept when an identity is replaced", "identity.cho",
     "            crypto.sha256(chain[3..3 + tls_message.get(chain, 0, 3)], cfg[b + o_fp()..b + o_fp() + 32]);\n", ""),
    ("the certificate's end not recorded", "identity.cho", "tls_message.put(cfg, b + o_not_after(), leaf_not_after, 8);",
     "tls_message.put(cfg, b + o_not_after(), 0, 8);"),
]

# Mutants that change no behaviour a client can reach, each with the argument. Such a mutant must survive.
EQUIVALENT = {}


def cases():
    out = []
    for line in open(os.path.join(ROOT, "tests/vectors/tls/liar_client.txt")):
        line = line.rstrip("\n")
        if line.startswith("## "):
            out.append((line[3:], [], []))
        elif line.startswith("= "):
            out[-1][2].append(line[2:])
        elif not line.startswith("#"):
            out[-1][1].append(line)
    return out


def build(cancho, pkg, out):
    r = subprocess.run([cancho, "build", "--std", os.path.join(ROOT, "tests/programs/tls_server_driver.cho"),
                        *[os.path.join(pkg, f) for f in TLS],
                        *[os.path.join(ROOT, "packages/x509", f) for f in X509], "-o", out],
                       capture_output=True, text=True)
    return r.returncode == 0, r.stderr


def evidence(cancho, pkg, work):
    """None when the package passes every connection, else the first that differs."""
    driver = os.path.join(work, "driver")
    ok, err = build(cancho, pkg, driver)
    if not ok:
        return "BUILD " + (err.strip().splitlines() or ["?"])[0]
    for name, asked, answered in cases():
        try:
            r = subprocess.run([driver], input="\n".join(asked) + "\n", capture_output=True, text=True, timeout=60)
        except subprocess.TimeoutExpired:
            return f"{name}: timed out"
        if r.stdout.splitlines() != answered:
            return f"{name} (exit {r.returncode})"
    return None


def main():
    cancho = sys.argv[1]
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    work = tempfile.mkdtemp(prefix="tls-server-mutants-")
    pkg = os.path.join(work, "tls")
    src = os.path.join(ROOT, "packages/tls")
    shutil.copytree(src, pkg)
    base = evidence(cancho, pkg, work)
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
        found = evidence(cancho, pkg, work)
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
