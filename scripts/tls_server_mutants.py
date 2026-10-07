#!/usr/bin/env python3
"""Mutation check of `packages/tls`'s server (docs/tls-server.md §8, step 2), the shape of `scripts/tls_mutants.py`.

    python3 scripts/tls_server_mutants.py <lex-sys binary> [--only <text in a mutant's name>]

Each mutant is one of the server's files (`hello.ls`, `identity.ls`, `server.ls`, and the server's parts of
`tls.ls` and `slot.ls`) with one deliberate bug. The package is copied to a scratch directory, the mutant
applied there, and `tests/programs/tls_server_driver.ls` built against it. It replays every connection of
`tests/vectors/tls/liar_client.txt` (`scripts/tls_liar_client.py`), each answer compared byte for byte, as
`crates/lex-sys/tests/conformance/tls_server.rs` does. A mutant is killed when any answer differs or the driver
traps. The unmutated package is run first and must pass. A mutant that changes nothing a client can reach is
listed in EQUIVALENT with the argument, and must survive. Exit status 1 if a mutant survives or fails to build.
"""
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TLS = ["tls.ls", "record.ls", "message.ls", "slot.ls", "client12.ls", "client.ls", "hello.ls", "identity.ls",
       "server.ls"]
X509 = ["verify.ls", "names.ls", "x509.ls", "key.ls"]

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    # ---- The ClientHello's rules (hello.ls) ----
    ("AES-128-GCM first whatever the CPU", "hello.ls", "    if gcm && mask & 1 != 0 {", "    if mask & 1 != 0 {"),
    ("AES-256-GCM taken for ChaCha20", "hello.ls",
     "    if suite == tls_record.suite_aes_256_gcm_sha384() {\n        return 2;",
     "    if suite == tls_record.suite_aes_256_gcm_sha384() {\n        return 4;"),
    ("TLS 1.3 not required in supported_versions", "hello.ls",
     "    if !versions || !tls13 {", "    if !versions {"),
    ("a compression method other than null allowed", "hello.ls", "    if !null_only {", "    if false {"),
    ("key_share not required", "hello.ls",
     "    if !sigalgs || !groups_seen || !shares_seen {", "    if !sigalgs || !groups_seen {"),
    ("signature_algorithms not required", "hello.ls",
     "    if !sigalgs || !groups_seen || !shares_seen {", "    if !groups_seen || !shares_seen {"),
    ("a share outside supported_groups allowed", "hello.ls",
     "    if shared & info[ch_groups()] != shared {", "    if false {"),
    ("ecdsa_secp256r1_sha256 not required", "hello.ls", "    if !p256_sha256 {", "    if false {"),
    ("no group in common not refused", "hello.ls",
     "    if info[ch_groups()] == 0 {\n        return tls_record.server_group();", "    if false {\n        return tls_record.server_group();"),
    ("an extension twice allowed", "hello.ls",
     "                    seen[kind >> 3] = byte_of(int_of(seen[kind >> 3]) | 1 << (kind & 7));\n", ""),
    ("pre_shared_key allowed before others", "hello.ls",
     "                        if body + size != ext_end {", "                        if false {"),
    ("a share's length not checked", "hello.ls",
     "            if seen & bit != 0 || n != tls_message.share_len(group) {", "            if seen & bit != 0 {"),
    ("two shares of one group allowed", "hello.ls",
     "            if seen & bit != 0 || n != tls_message.share_len(group) {",
     "            if n != tls_message.share_len(group) {"),
    ("two host names allowed", "hello.ls",
     "            if seen {\n                return tls_record.server_illegal_parameter();",
     "            if false {\n                return tls_record.server_illegal_parameter();"),
    ("an empty ALPN name allowed", "hello.ls", "                                if m == 0 || q + 1 + m > body + size {",
     "                                if q + 1 + m > body + size {"),
    ("a HelloRetryRequest to P-256 before X25519", "hello.ls",
     "    let mask = info[ch_groups()];\n    if mask & 1 != 0 {", "    let mask = info[ch_groups()];\n    if mask & 1 == 2 {"),
    ("a P-256 share taken before an X25519 one", "hello.ls",
     "    if info[ch_share_x25519()] != 0 {\n        return tls_message.group_x25519();",
     "    if info[ch_share_x25519()] != 0 && info[ch_share_p256()] == 0 {\n        return tls_message.group_x25519();"),
    ("ALPN: no protocol ever matches", "hello.ls", "                if same {\n                    return p;",
     "                if !same {\n                    return p;"),
    ("the HelloRetryRequest random not SHA-256(\"HelloRetryRequest\")", "hello.ls",
     "        out[at + k] = byte_of(tls_message.hrr_random(k));", "        out[at + k] = byte_of(k);"),
    ("server_name never acknowledged", "hello.ls", "    if sni_used {", "    if false {"),
    ("CertificateVerify naming rsa_pss_rsae_sha256", "hello.ls",
     "    at = put(out, at, tls_message.ecdsa_p256_sha256(), 2);", "    at = put(out, at, 0x0804, 2);"),
    ("early_data offered not noticed", "hello.ls", "                        info[ch_early()] = 1;\n", ""),
    ("a ClientHello of 16 KiB refused", "hello.ls", "    return 16384;", "    return 16383;"),
    ("an over-long host name kept", "hello.ls", "            if n <= 255 {", "            if n <= 4096 {"),
    # ---- The handshake (server.ls) ----
    ("the client's Finished not checked", "server.ls",
     "            if diff != 0 {\n                code = tls_record.server_finished();",
     "            if diff != diff {\n                code = tls_record.server_finished();"),
    ("only the Finished's first byte compared", "server.ls",
     "                diff = diff | int_of(want[k]) ^ int_of(message[4 + k]);\n                k = k + 1;",
     "                diff = diff | int_of(want[k]) ^ int_of(message[4 + k]);\n                k = k + h;"),
    ("the client's application key never read under", "server.ls",
     "    if code == 0 {\n        tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_ap());",
     "    if code == 0 {\n"),
    ("the server's application key never written under", "server.ls",
     "    tls_slot.set_write_keys(ints, bytes, tls_slot.k_server_ap());\n    tls_slot.set_read_keys(", "    tls_slot.set_read_keys("),
    ("the flight sealed under the client's handshake key", "server.ls",
     "    tls_slot.set_write_keys(ints, bytes, tls_slot.k_server_hs());", "    tls_slot.set_write_keys(ints, bytes, tls_slot.k_client_hs());"),
    ("the application secrets' labels swapped", "server.ls", '"c ap traffic", th, bytes[tls_slot.k_client_ap()',
     '"s ap traffic", th, bytes[tls_slot.k_client_ap()'),
    ("message_hash with the wrong type", "server.ls", "        synthetic[0] = byte_of(254);", "        synthetic[0] = byte_of(253);"),
    ("the transcript not restarted after a retry", "server.ls",
     "        tls_slot.transcript_init(ints);\n        tls_slot.transcript_add(ints, synthetic);",
     "        tls_slot.transcript_add(ints, synthetic);"),
    ("change_cipher_spec sent twice after a retry", "server.ls",
     "    if tls_slot.has(ints, tls_slot.f_ccs_sent()) {\n        return 0;\n    }\n", ""),
    ("the CertificateVerify context string misspelled", "server.ls",
     'let label = "TLS 1.3, server CertificateVerify";', 'let label = "TLS 1.3, client CertificateVerify";'),
    ("the signature not hedged (RFC 6979's nonce alone)", "server.ls",
     "bytes[tls_slot.k_sign_extra()..tls_slot.k_sign_extra() + 32], sig,", "bytes[0..0], sig,"),
    ("the signature checked under another point", "server.ls",
     "tls_identity.point(cfg, id), bytes[", "tls_identity.point(cfg, id + 1), bytes["),
    ("the name never chooses an identity", "server.ls",
     "    var id = tls_identity.select(cfg, bytes[tls_slot.b_sni()..tls_slot.b_sni() + n]);", "    var id = 0 - 1;"),
    ("the name not lowercased", "server.ls", "byte_of(lower(int_of(body[s + k])))", "body[s + k]"),
    ("ALPN with nothing in common accepted", "server.ls", "    if pick < 0 {\n        return tls_record.server_alpn();",
     "    if pick < 0 {\n        return 0;"),
    ("the second ClientHello's session id not compared", "server.ls",
     "            if n != ints[tls_slot.i_session_len()] || !same(", "            if false && !same("),
    ("the second ClientHello's suite not checked", "server.ls",
     "info[tls_hello.ch_suites()] & tls_hello.suite_bit(ints[tls_slot.i_suite()]) == 0 || ", ""),
    ("early data allowed in the second ClientHello", "server.ls", " || info[tls_hello.ch_early()] != 0 {", " {"),
    ("the second ClientHello's share not required", "server.ls",
     "            } else if tls_hello.share_at(info, ints[tls_slot.i_group()]) == 0 {", "            } else if false {"),
    ("early data of exactly 16 KiB refused", "server.ls", "    if ints[tls_slot.i_early_skipped()] > 16384 {",
     "    if ints[tls_slot.i_early_skipped()] >= 16384 {"),
    ("early data skipped after a record opened", "server.ls", "    tls_slot.clear_flag(ints, tls_slot.f_early());\n", ""),
    ("change_cipher_spec before the ClientHello taken", "server.ls", " || tls_slot.has(ints, tls_slot.f_ccs_seen()) || !between {",
     " || tls_slot.has(ints, tls_slot.f_ccs_seen()) {"),
    ("a second change_cipher_spec taken", "server.ls", " || tls_slot.has(ints, tls_slot.f_ccs_seen()) || !between {",
     " || !between {"),
    ("a message allowed to share a record with what follows", "server.ls", "            } else if have > 4 + n {",
     "            } else if false {"),
    ("a ClientHello over 16 KiB taken", "server.ls", "            } else if hello && n > tls_hello.max_client_hello() {",
     "            } else if hello && n > tls_slot.hs_cap() {"),
    ("a plaintext alert after the flight not read", "server.ls",
     "    let plain_alert = kind == tls_record.type_alert() && state == tls_slot.state_wait_client_finished();",
     "    let plain_alert = false;"),
    ("a KeyUpdate not answered", "server.ls", "        if asked == 1 {", "        if asked == 2 {"),
    ("33 KeyUpdates allowed", "server.ls", "    if ints[tls_slot.i_key_updates()] > 32 {", "    if ints[tls_slot.i_key_updates()] > 33 {"),
    ("the read key after a KeyUpdate not changed", "server.ls",
     "        tls_slot.set_read_keys(ints, bytes, tls_slot.k_client_ap());\n        if asked == 1 {",
     "        if asked == 1 {"),
    ("the shared secret not checked for zero", "server.ls",
     "        if x25519.scalarmult(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], peer, secret) != 0 {",
     "        if x25519.scalarmult(bytes[tls_slot.k_x25519()..tls_slot.k_x25519() + 32], peer, secret) == 99 {"),
    ("the handshake secret's label wrong", "server.ls", '"s hs traffic", th,', '"c hs traffic", th,'),
    ("the early secret over a non-zero PSK", "server.ls", "        hkdf.extract(h, zeros, zeros, early);",
     "        hkdf.extract(h, zeros, empty_hash, early);"),
    # ---- Identities (identity.ls) ----
    ("an expired leaf accepted", "identity.ls", "                    } else if view[x509.not_after()] < now {",
     "                    } else if false {"),
    ("a key mismatch reported as a key type", "identity.ls", "            if m == -92 {", "            if m == -93 {"),
    ("a leaf of another curve accepted", "identity.ls",
     " || view[x509.key_curve()] != x509.oid_p256() {", " {"),
    ("names matched by any pattern", "identity.ls", "                if e > p && x509_names.dns_matches(names[p..e], host) {",
     "                if e > p {"),
    ("an ALPN name over 255 bytes taken", "identity.ls", "                if e - p > 255 || n + 1 + e - p > alpn_cap() {",
     "                if n + 1 + e - p > alpn_cap() {"),
    ("identities beyond 16 allowed to be counted full", "identity.ls",
     "    return id >= 0 && id < max_identities() && int_of(cfg[at(id)]) == 1;",
     "    return id >= 0 && id < max_identities() - 1 && int_of(cfg[at(id)]) == 1;"),
    ("names longer than the room accepted", "identity.ls", "    if !keep_names && len(names) > names_cap() {",
     "    if false {"),
    ("a replacement written before its checks", "identity.ls", "        if code == 0 {\n            let b = at(id);",
     "        if code == 0 || code == tls_record.server_key_mismatch() {\n            let b = at(id);"),
    # ---- The engine (tls.ls) and alerts (slot.ls) ----
    ("serve on a client engine", "tls.ls",
     "pub fn serve[&e](engine: &!e Engine, slot: int, now_unix_ms: int) -> [] int {\n    if !is_server(engine) {",
     "pub fn serve[&e](engine: &!e Engine, slot: int, now_unix_ms: int) -> [] int {\n    if false {"),
    ("start on a server engine", "tls.ls",
     "pub fn start[&e, &h](engine: &!e Engine, slot: int, host: &h [byte], now_unix_ms: int) -> [] int {\n    if is_server(engine) {",
     "pub fn start[&e, &h](engine: &!e Engine, slot: int, host: &h [byte], now_unix_ms: int) -> [] int {\n    if false {"),
    ("serve with no identity", "tls.ls", "    if tls_identity.count(contents(engine.ids)) == 0 {", "    if false {"),
    ("a 17th identity added", "tls.ls", "    if id == tls_identity.max_identities() {\n        return tls_record.server_identities_full();",
     "    if id == tls_identity.max_identities() + 1 {\n        return tls_record.server_identities_full();"),
    ("handshakes in progress not counted", "tls.ls", "            n = n + 1;\n        }\n        s = s + 1;",
     "            n = n + 0;\n        }\n        s = s + 1;"),
    ("no_application_protocol sent as handshake_failure", "slot.ls", "        return 120;", "        return 40;"),
    ("missing_extension sent as decode_error", "slot.ls", "        return 109;", "        return 50;"),
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


def build(lexsys, pkg, out):
    r = subprocess.run([lexsys, "build", "--std", os.path.join(ROOT, "tests/programs/tls_server_driver.ls"),
                        *[os.path.join(pkg, f) for f in TLS],
                        *[os.path.join(ROOT, "packages/x509", f) for f in X509], "-o", out],
                       capture_output=True, text=True)
    return r.returncode == 0, r.stderr


def evidence(lexsys, pkg, work):
    """None when the package passes every connection, else the first that differs."""
    driver = os.path.join(work, "driver")
    ok, err = build(lexsys, pkg, driver)
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
    lexsys = sys.argv[1]
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    work = tempfile.mkdtemp(prefix="tls-server-mutants-")
    pkg = os.path.join(work, "tls")
    src = os.path.join(ROOT, "packages/tls")
    shutil.copytree(src, pkg)
    base = evidence(lexsys, pkg, work)
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
        found = evidence(lexsys, pkg, work)
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
