#!/usr/bin/env python3
"""Mutation check of `packages/tls` (docs/tls-core.md §7, §10).

    python3 scripts/tls_mutants.py <lex-sys binary>

Each mutant is one of the package's files with one deliberate bug. The
package is copied to a scratch directory, the mutant applied there, and
`tests/programs/tls_driver.ls` built against it. It runs what
`conformance/tls.rs` replays: the five tlslite-ng traces (two of them through a
HelloRetryRequest), the six TLS 1.2 traces against OpenSSL, and the 66
connections of `tests/vectors/tls/liar.txt`, each answer compared byte for
byte. A mutant of the engine (`tls.ls`) also builds
`tests/programs/tls_many.ls` and serves it `tests/vectors/tls/streams.txt` from
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
FILES = ["record.ls", "message.ls", "slot.ls", "client12.ls", "client.ls", "tls.ls"]

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    ("the server Finished not checked", "client.ls",
     "            if diff != 0 {\n                code = tls_record.bad_finished();",
     "            if diff != diff {\n                code = tls_record.bad_finished();"),
    ("the transcript missing EncryptedExtensions", "client.ls",
     "            tls_slot.transcript_add(ints, message);\n            ints[tls_slot.i_state()] = tls_slot.state_wait_certificate();",
     "            ints[tls_slot.i_state()] = tls_slot.state_wait_certificate();"),
    ("the ClientHello hashed with its record header", "client.ls",
     "        tls_slot.transcript_add(ints, hello[0..n]);\n        code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), hello[0..n]);",
     "        code = tls_slot.queue_record(ints, bytes, tls_record.type_handshake(), hello[0..n]);\n"
     "        tls_slot.transcript_add(ints, bytes[tls_slot.b_out()..tls_slot.b_out() + 5 + n]);"),
    ("the read sequence number not incremented", "client.ls",
     "    ints[tls_slot.i_read_seq()] = ints[tls_slot.i_read_seq()] + 1;", "    ints[tls_slot.i_read_seq()] = ints[tls_slot.i_read_seq()] + 0;"),
    ("the write sequence number not incremented", "slot.ls",
     "    ints[i_write_seq()] = ints[i_write_seq()] + 1;\n    ints[i_out_end()] = ints[i_out_end()] + n;",
     "    ints[i_write_seq()] = ints[i_write_seq()] + 0;\n    ints[i_out_end()] = ints[i_out_end()] + n;"),
    ("the nonce built without the sequence", "record.ls",
     "            s = seq >> 8 * (11 - k) & 255;", "            s = 0;"),
    ("the downgrade sentinel ignored", "message.ls",
     "int_of(b[33]) <= 1 {\n            return tls_record.protocol_version();",
     "int_of(b[33]) <= 1 && false {\n            return tls_record.protocol_version();"),
    ("server_name allowed in a TLS 1.3 ServerHello", "message.ls",
     "if reneg || ems || formats || sni {", "if reneg || ems || formats {"),
    ("server_name allowed twice in a TLS 1.2 ServerHello", "message.ls",
     "            if sni || size != 0 {", "            if size != 0 {"),
    ("the HelloRetryRequest random ignored", "message.ls", "    let retry = k == 32;", "    let retry = k == 33;"),
    ("an unexpected extension accepted", "message.ls",
     "            seen_groups = true;\n        } else {\n            return tls_record.unsupported_extension();\n        }",
     "            seen_groups = true;\n        }"),
    ("the record limit off by one", "record.ls",
     "    let n = int_of(buf[at + 3]) * 256 + int_of(buf[at + 4]);\n    if n > max_ciphertext() {",
     "    let n = int_of(buf[at + 3]) * 256 + int_of(buf[at + 4]);\n    if n >= max_ciphertext() {"),
    ("inner-plaintext padding not stripped", "record.ls",
     "    while last >= 0 && int_of(out[last]) == 0 {", "    while last >= 0 && int_of(out[last]) == 256 {"),
    ("the content type taken from the outer header", "client.ls",
     "        inner = info[0];", "        inner = kind;"),
    ("the client's keys used for reading", "client.ls",
     "            tls_slot.set_read_keys(ints, bytes, tls_slot.k_server_hs());", "            tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_hs());"),
    ("a KeyUpdate not answered", "client.ls", "        if asked == 1 {", "        if asked == 2 {"),
    ("the CertificateVerify context string misspelled", "client.ls",
     'let label = "TLS 1.3, server CertificateVerify";', 'let label = "TLS 1.3, client CertificateVerify";'),
    ("a partial message not moved to the front", "client.ls",
     "        bytes[tls_slot.b_hs() + k] = bytes[tls_slot.b_hs() + at + k];", "        bytes[tls_slot.b_hs() + k] = bytes[tls_slot.b_hs() + k];"),
    ("a warning-level alert ignored", "client.ls",
     "    ints[tls_slot.i_alert()] = what;\n", "    if level == 1 {\n        return 0;\n    }\n    ints[tls_slot.i_alert()] = what;\n"),
    ("the chain not verified", "slot.ls", "code = from_x509(x509_verify.verify(store, body, ranges, bytes[k_host()..k_host() + ints[i_host_len()]], ints[i_now()], x509_verify.tls_max_intermediates()));", "code = 0;"),
    ("the time not given to the verifier", "slot.ls", "ints[i_now()], x509_verify.tls_max_intermediates()", "0, x509_verify.tls_max_intermediates()"),
    ("an unknown issuer reported as x509-decode", "slot.ls",
     "    if code == x509_verify.unknown_issuer() {\n        return tls_record.x509_unknown_issuer();",
     "    if code == x509_verify.unknown_issuer() {\n        return tls_record.x509_decode();"),
    ("a message allowed to share a record with the next key", "client.ls",
     "(after == tls_slot.state_wait_extensions() || after == tls_slot.state_connected()) && at < ints[tls_slot.i_hs_fill()]",
     "(after == tls_slot.state_wait_extensions() || after == tls_slot.state_connected()) && at < 0"),
    ("the engine's DRBG key not replaced", "tls.ls", "                key[i] = stream[i];\n", ""),
    ("the engine's slots overlapping", "tls.ls",
     "    return slot * tls_client.bytes_len();", "    return slot * (tls_client.bytes_len() / 2);"),
    # ---- Suites and HelloRetryRequest (docs/tls-parity.md §3.3) ----
    ("SHA-384's transcript never chosen", "slot.ls", "        if len(out) == 48 {", "        if len(out) == 32 {"),
    ("AES-256-GCM given SHA-256", "record.ls",
     "    if suite == suite_aes_256_gcm_sha384() || suite == 0xc02c || suite == 0xc030 {\n        return 48;",
     "    if suite == suite_aes_256_gcm_sha384() || suite == 0xc02c || suite == 0xc030 {\n        return 32;"),
    ("AES-128-GCM given a 32-byte key", "record.ls",
     "    if suite == suite_aes_128_gcm_sha256() || suite == 0xc02b || suite == 0xc02f {\n        return 16;",
     "    if suite == suite_aes_128_gcm_sha256() || suite == 0xc02b || suite == 0xc02f {\n        return 32;"),
    ("every suite sealed with ChaCha20", "record.ls",
     "    if chacha(suite) {\n        return chacha20.seal(", "    if true {\n        return chacha20.seal("),
    ("the Finished MAC always SHA-256", "client.ls", "        hmac.mac(h, key, th, out);", "        hmac.mac(32, key, th, out);"),
    ("message_hash with the wrong type", "client.ls", "        synthetic[0] = byte_of(254);", "        synthetic[0] = byte_of(253);"),
    ("the transcript not restarted after a retry", "client.ls",
     "        tls_slot.transcript_init(ints);\n        tls_slot.transcript_add(ints, synthetic);", "        tls_slot.transcript_add(ints, synthetic);"),
    ("the cookie not echoed", "client.ls",
     "        let cookie = message[4 + cookie_start..4 + cookie_end];", "        let cookie = message[4..4];"),
    ("a second HelloRetryRequest accepted", "client.ls",
     "            if info[tls_message.sh_retry()] == 1 {\n                code = tls_record.unexpected_message();",
     "            if info[tls_message.sh_retry()] == 2 {\n                code = tls_record.unexpected_message();"),
    ("the suite after a retry not compared", "client.ls",
     "            } else if suite != ints[tls_slot.i_suite()] {", "            } else if false {"),
    # Not here: the ServerHello's group not compared with the share sent. A share of
    # another group always has the wrong length for the key held, so the key exchange
    # refuses it with the same tag; the comparison is defence in depth, and that mutant
    # is equivalent (docs/tls-parity.md §3.3.1).
    ("a retry to X25519 accepted", "message.ls",
     "                if group != group_p256() && group != group_p384() {", "                if group == 0 {"),
    ("a retry that changes nothing accepted", "message.ls",
     "            if group == 0 && cookie == 0 {", "            if group == 0 && cookie == 0 && false {"),
    ("a change_cipher_spec after a retry refused", "client.ls",
     "        let early = state < tls_slot.state_wait_extensions() && !(state == tls_slot.state_wait_server_hello() && tls_slot.has(ints, tls_slot.f_retried()));",
     "        let early = state < tls_slot.state_wait_extensions();"),
    ("the retry's share for the wrong curve", "slot.ls",
     "    if group == tls_message.group_p256() {\n        return 256;", "    if group == tls_message.group_p256() {\n        return 384;"),
    ("the end of the socket taken for close_notify", "client.ls",
     "    if tls_slot.has(ints, tls_slot.f_close_received()) || ints[tls_slot.i_state()] == tls_slot.state_failed() {\n        return 0;",
     "    if true {\n        return 0;"),
    # ---- TLS 1.2 (docs/tls-parity.md §3.4) ----
    ("the PRF's A(i) not chained", "record.ls",
     "            hmac.init(hash_len, st, secret);\n            hmac.update(hash_len, st, a);\n            hmac.final(hash_len, st, a);",
     "            hmac.init(hash_len, st, secret);\n            hmac.final(hash_len, st, a);"),
    ("the PRF's label left out of A(1)", "record.ls",
     "        hmac.update(hash_len, st, label);\n        hmac.update(hash_len, st, seed);\n        hmac.final(hash_len, st, a);",
     "        hmac.update(hash_len, st, seed);\n        hmac.final(hash_len, st, a);"),
    ("the master secret without the extension's label", "client12.ls", '"extended master secret"', '"master secret"'),
    ("the key block's randoms in the wrong order", "client12.ls",
     "        tls_slot.copy_bytes(bytes[tls_slot.k_server_random()..tls_slot.k_server_random() + 32], seed[0..32]);\n        tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], seed[32..64]);",
     "        tls_slot.copy_bytes(bytes[tls_slot.k_server_random()..tls_slot.k_server_random() + 32], seed[32..64]);\n        tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], seed[0..32]);"),
    ("the client writing with the server's key", "client12.ls",
     "        tls_slot.copy_bytes(block[0..kl], bytes[tls_slot.k_write_key()..tls_slot.k_write_key() + kl]);",
     "        tls_slot.copy_bytes(block[kl..2 * kl], bytes[tls_slot.k_write_key()..tls_slot.k_write_key() + kl]);"),
    ("AES-GCM's explicit nonce not read from the record", "record.ls",
     "        out[k] = explicit[k - 4];", "        out[k] = byte_of(0);"),
    ("the explicit nonce sent as zeros", "record.ls",
     "            out[5 + k] = byte_of(seq >> 8 * (7 - k) & 255);", "            out[5 + k] = byte_of(0);"),
    ("the additional data without the sequence number", "record.ls",
     "        out[k] = byte_of(seq >> 8 * (7 - k) & 255);\n        k = k + 1;\n    }\n    out[8]",
     "        out[k] = byte_of(0);\n        k = k + 1;\n    }\n    out[8]"),
    ("the additional data with the ciphertext's length", "record.ls",
     "        aad12(seq, content_type, n, ad);\n        code = aead_open(", "        aad12(seq, content_type, body, ad);\n        code = aead_open("),
    ("TLS 1.2 ChaCha20's nonce built as AES-GCM's", "record.ls",
     "    if chacha(suite) {\n        return nonce(iv, seq, out);", "    if false {\n        return nonce(iv, seq, out);"),
    ("the extended master secret not required", "message.ls",
     "        if !ems {\n            return tls_record.extended_master_secret();", "        if false {\n            return tls_record.extended_master_secret();"),
    ("a TLS 1.2 ServerHello echoing the session id accepted", "message.ls",
     "            if same {\n                return tls_record.decode_error();", "            if same && false {\n                return tls_record.decode_error();"),
    ("a key_share in a TLS 1.2 ServerHello accepted", "message.ls",
     "        if group != 0 {\n            return tls_record.unsupported_extension();", "        if false {\n            return tls_record.unsupported_extension();"),
    ("the key exchange's randoms not signed", "client12.ls",
     "            tls_slot.copy_bytes(bytes[tls_slot.k_random()..tls_slot.k_random() + 32], content[0..32]);\n", ""),
    ("the suite's kind of key not checked", "client12.ls",
     "            if tls_record.suite12_ecdsa(ints[tls_slot.i_suite()]) == rsa_key {", "            if false {"),
    ("the server's Finished computed as the client's", "client12.ls",
     'verify_data(ints, bytes, "server finished", want);', 'verify_data(ints, bytes, "client finished", want);'),
    ("the server's Finished not compared", "client12.ls",
     "    if diff != 0 {\n        return tls_record.bad_finished();", "    if diff != diff {\n        return tls_record.bad_finished();"),
    ("a HelloRequest accepted", "client12.ls", "        return tls_record.renegotiation();", "        return 0;"),
    ("the ClientKeyExchange's point length one short", "client12.ls", "            cke[4] = byte_of(size);", "            cke[4] = byte_of(size - 1);"),
    ("TLS 1.2 after a HelloRetryRequest accepted", "client.ls",
     "        if code == 0 && tls12 && tls_slot.has(ints, tls_slot.f_retried()) {", "        if code == 0 && tls12 && false {"),
    ("the server's key exchange point never kept", "client12.ls",
     "            tls_slot.copy_bytes(point, bytes[tls_slot.k_peer()..tls_slot.k_peer() + len(point)]);\n", ""),
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


def build(lexsys, program, pkg, out, engine=False):
    files = [os.path.join(pkg, f) for f in (FILES if engine else FILES[:5])]
    r = subprocess.run([lexsys, "build", "--std", os.path.join(ROOT, "tests/programs", program), *files,
                        *[os.path.join(ROOT, "packages/x509", f) for f in ["verify.ls", "names.ls", "x509.ls"]],
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


def evidence(lexsys, pkg, work, engine):
    """None when the package passes everything, else what failed."""
    driver = os.path.join(work, "driver")
    ok, err = build(lexsys, "tls_driver.ls", pkg, driver)
    if not ok:
        return "BUILD " + err.strip().splitlines()[0]
    found = replay(driver, cases())
    if found or not engine:
        return found
    many = os.path.join(work, "tls_many")
    ok, err = build(lexsys, "tls_many.ls", pkg, many, engine=True)
    if not ok:
        return "BUILD " + err.strip().splitlines()[0]
    return serve_streams(many, "1", False) or serve_streams(many, "65536", False) or serve_streams(many, "65536", True)


def main():
    lexsys = sys.argv[1]
    work = tempfile.mkdtemp(prefix="tls-mutants-")
    pkg = os.path.join(work, "tls")
    src = os.path.join(ROOT, "packages/tls")
    shutil.copytree(src, pkg)
    base = evidence(lexsys, pkg, work, True)
    if base:
        print(f"the unmutated package fails: {base}")
        sys.exit(1)
    print("unmutated: passes")
    survived = 0
    for name, file, old, new in MUTANTS:
        text = open(os.path.join(src, file)).read()
        assert text.count(old) == 1, f"{name}: the text occurs {text.count(old)} times"
        open(os.path.join(pkg, file), "w").write(text.replace(old, new))
        found = evidence(lexsys, pkg, work, file == "tls.ls" or "end of the socket" in name)
        shutil.copy(os.path.join(src, file), os.path.join(pkg, file))
        if found is None or found.startswith("BUILD"):
            survived += 1
            print(f"SURVIVED {name}" + (f": {found}" if found else ""))
        else:
            print(f"killed   {name}: {found}")
    shutil.rmtree(work, ignore_errors=True)
    print(f"{len(MUTANTS) - survived} of {len(MUTANTS)} mutants killed")
    sys.exit(1 if survived else 0)


if __name__ == "__main__":
    main()
