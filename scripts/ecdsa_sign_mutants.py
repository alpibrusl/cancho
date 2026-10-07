#!/usr/bin/env python3
"""Mutation check of `std/ecdsa_sign.ls`, `std.bigmod.load_secret`,
`std.ecdh.scalar_ok` and `packages/x509/key.ls` (docs/ecdsa-sign.md §5.5).

    python3 scripts/ecdsa_sign_mutants.py <lex-sys binary>

Each mutant is one of the files with one deliberate bug. The three std files
are built as local modules (`ecdsa_sign`, `bigmod`, `ecdh`) beside a copy of
`tests/programs/ecdsa_sign_driver.ls` and the package's `x509.ls` and
`key.ls`, then run against the evidence `conformance/ecdsa_sign.rs` and
`scripts/ecdsa_sign_differential.py` use:
- RFC 6979 A.2.5's signatures, their DER, the added randomness, and every
  refusal of the signer;
- `tests/vectors/ecdsa_sign/keys.txt` and `certs.txt`, every file;
- 300 signatures against RFC 6979 in Python (`reference`), 100 verified by
  OpenSSL and `std.ecdsa` (`openssl`), and 6 `openssl genpkey` keys
  (`keys`).
A mutant is killed when any of them disagrees. The unmutated files are run
first and must pass. `std.ecdh`'s ladder and `std.bigmod`'s arithmetic have
their own mutants (`scripts/ecdh_mutants.py`). Needs `openssl` on the path.
Exit status 1 if a mutant survives or fails to build.
"""
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (file, name, the text replaced, its replacement). Each `old` must occur
# exactly once in its file.
MUTANTS = [
    ("ecdsa_sign", "s with r·r, not r·d", "bigmod.mul(w, rr, dr, sr);", "bigmod.mul(w, rr, rr, sr);"),
    ("ecdsa_sign", "s without k^-1", "    bigmod.inverse(w, kr, kr);\n", ""),
    ("ecdsa_sign", "s = k^-1 (e - r·d)", "bigmod.add(w, er, sr, sr);", "bigmod.sub(w, er, sr, sr);"),
    ("ecdsa_sign", "e left out of s", "    bigmod.add(w, er, sr, sr);\n", ""),
    ("ecdsa_sign", "r from k·G's y", "code = finish(h, key, t, point[1..33], sig, work);", "code = finish(h, key, t, point[33..65], sig, work);"),
    ("ecdsa_sign", "the second reseed's separator 00, not 01", "sep[0] = byte_of(1);", "sep[0] = byte_of(0);"),
    ("ecdsa_sign", "the added randomness not fed to the DRBG", "    hmac.update(32, st, extra);\n", ""),
    ("ecdsa_sign", "the key not fed to the DRBG", "    hmac.update(32, st, key);\n", ""),
    ("ecdsa_sign", "the digest fed to the DRBG unreduced", "bigmod.store_reg(work, bigmod.reg(0), h);", "copy32(digest, h);"),
    ("ecdsa_sign", "the nonce taken from K, not V", "    return copy32(v, t);", "    return copy32(kk, t);"),
    ("ecdsa_sign", "V not updated before the candidate", "    hmac.update(32, st, v);\n    hmac.final(32, st, v);\n    return copy32", "    return copy32"),
    ("ecdsa_sign", "the key's range not checked", "if !ecdh.scalar_ok(256, key) {", "if !ecdh.scalar_ok(256, key) && false {"),
    ("ecdsa_sign", "31 bytes of randomness accepted", "if len(extra) != 0 && len(extra) != 32 {", "if len(extra) > 32 {"),
    ("ecdsa_sign", "the check after signing skipped", "if ecdsa.verify_raw(256, digest, point, sig, work) != 0 {", "if ecdsa.verify_raw(256, digest, point, sig, work) != 0 && false {"),
    ("ecdsa_sign", "DER keeps an INTEGER's leading zero bytes", "while i < len(v) - 1 && int_of(v[i]) == 0 {", "while i < 0 && int_of(v[i]) == 0 {"),
    ("ecdsa_sign", "DER without the 00 before a top bit", "    if int_of(v[i]) >= 0x80 {\n        n = n + 1;\n    }\n", ""),
    ("ecdsa_sign", "DER into a buffer too short", "if len(out) < 2 + body {", "if len(out) < body {"),
    ("bigmod", "a straddling byte's top bits dropped at offset 24", "        if off > 22 {\n            w[r + idx + 1]", "        if off > 24 {\n            w[r + idx + 1]"),
    ("bigmod", "a secret byte not masked to the limb", "w[r + idx] = w[r + idx] | v << off & mask();", "w[r + idx] = w[r + idx] | v << off;"),
    ("ecdh", "every key in range", "    return ok == 1;", "    return true;"),
    ("key", "Z not base64", "let upper = within(c, 65, 90);", "let upper = within(c, 65, 89);"),
    ("key", "lower case one off", "lower & c - 71", "lower & c - 70"),
    ("key", "+ decoded as /", "plus & 62", "plus & 63"),
    ("key", "a stray = accepted", "            if pad > 0 {\n                return -80;\n            }\n", ""),
    ("key", "EC PRIVATE KEY read as PKCS#8", "        return format_sec1();\n    }\n    if bytes.equal(label, \"ENCRYPTED", "        return format_pkcs8();\n    }\n    if bytes.equal(label, \"ENCRYPTED"),
    ("key", "a traditional encrypted key not recognised", "if find_from(pem[0..end], body, \"ENCRYPTED\") >= 0 {", "if find_from(pem[0..end], body, \"ENCRYPTED\") >= 1000000 {"),
    ("key", "the END label not compared", "|| !bytes.equal(pem[end_label..end_label + (close - label)], pem[label..close]) ", ""),
    ("key", "version 2 accepted", "if t[2] - t[1] != 1 || int_of(der[t[1]]) > 1 {", "if t[2] - t[1] != 1 || int_of(der[t[1]]) > 2 {"),
    ("key", "any named curve accepted", "if x509.oid_code(der, t[1], t[2]) != x509.oid_p256() {", "if x509.oid_code(der, t[1], t[2]) == 0 {"),
    ("key", "SEC 1 without its curve accepted", "if code == 0 && !curve {", "if code == 0 && !curve && false {"),
    ("key", "a 33-byte key accepted", "info[1] - info[0] > 32", "info[1] - info[0] > 33"),
    ("key", "SEC 1's public key not compared", "if code == 0 && info[2] >= 0 && !bytes.equal(der[info[2]..info[2] + 65], point) {", "if code == 0 && info[2] >= 0 && false {"),
    ("key", "PKCS#8 v2's two public keys not compared", "                } else if !bytes.equal(der[q..q + 65], der[info[2]..info[2] + 65]) {", "                } else if false {"),
    ("key", "a short key not left-padded", "key[32 - n + i] = der[info[0] + i];", "key[i] = der[info[0] + i];"),
    ("key", "an element after PKCS#8's key accepted", "        if code == 0 && p != end {\n            code = -84;\n        }\n    }\n    return code;\n}\n\n// The private key in `der`", "    }\n    return code;\n}\n\n// The private key in `der`"),
    ("key", "a key out of range answered with ecdh's code", "    if code == ecdh.refused_scalar_range() {\n        return -87;\n    }\n", ""),
    ("key", "the certificate's key not compared", "} else if !bytes.equal(cert[view[x509.key_start()]..view[x509.key_end()]], point) {", "} else if false {"),
]

KEY = "c9afa9d845ba75166b5c215767b1d6934e50c3db36e89b127b8a622b120f6721"
POINT = ("0460fed4ba255a9d31c961eb74c6356d68c049b8923b61fa6ce669622e60f29fb6"
         "7903fe1008b8bc99a41ae9e95628bc64f2f1b20c2d7e9f5177a3c294d4462299")
SAMPLE = "af2bdbe1aa9b6ec1e2ade1d694f41fc71a831d0268e9891562113d8a62add1bf"
TEST = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
SAMPLE_SIG = ("efd48b2aacb6a8fd1140dd9cd45e81d69d2c877b56aaf991c34d0ea84eaf3716"
              "f7cb1c942d657c41d436c7a1b6e29f65f3e900dbb9aff4064dc4ab2f843acda8")
TEST_SIG = ("f1abb023518351cd71d881567b1ea663ed3efcf6c5132b354f28d3b0b7d38367"
            "019f4113742a2b14bd25926b49c649155f267e60d3814b4c0cc84250e46f0083")
N = "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551"
G = ("046b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296"
     "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5")


def evidence():
    """(case, the exact answer) pairs: conformance/ecdsa_sign.rs's cases."""
    sample_der = f"3046022100{SAMPLE_SIG[:64]}022100{SAMPLE_SIG[64:]}"
    test_der = f"3045022100{TEST_SIG[:64]}0220{TEST_SIG[64:]}"
    pairs = [
        (f"S {SAMPLE} {KEY} -", f"0 ok {SAMPLE_SIG}"),
        (f"S {TEST} {KEY} -", f"0 ok {TEST_SIG}"),
        (f"C {SAMPLE} {KEY} {POINT} -", f"0 ok {SAMPLE_SIG}"),
        (f"E {SAMPLE_SIG}", f"72 ok {sample_der}"),
        (f"E {TEST_SIG}", f"71 ok {test_der}"),
        (f"M {b'sample'.hex()} {KEY} -", f"72 ok {sample_der}"),
        (f"S {SAMPLE[:62]} {KEY} -", "-70 ecdsa-sign-digest-length -"),
        (f"S {SAMPLE} {KEY[:62]} -", "-71 ecdsa-sign-key-length -"),
        (f"S {SAMPLE} {'00' * 32} -", "-72 ecdsa-sign-key-range -"),
        (f"S {SAMPLE} {N} -", "-72 ecdsa-sign-key-range -"),
        (f"S {SAMPLE} {KEY} {'00' * 31}", "-73 ecdsa-sign-extra-length -"),
        (f"F {SAMPLE_SIG} 71", "-74 ecdsa-sign-output-length -"),
        (f"W {SAMPLE} {KEY} -", "-75 ecdsa-sign-work-length -"),
        (f"C {SAMPLE} {KEY} {G} -", "-77 ecdsa-sign-check -"),
    ]
    for name in ("keys.txt", "certs.txt"):
        for line in open(os.path.join(ROOT, "tests/vectors/ecdsa_sign", name)):
            if line.startswith("#"):
                continue
            tag, _, data, want = line.split()
            if name == "keys.txt":
                code = "0" if tag == "ok" else None
                pairs.append((f"K {data}", (code, tag, want if tag == "ok" else "-")))
            else:
                pairs.append((f"X {data} {want}", (None, tag, "-")))
    return pairs


def agrees(answer, want):
    if isinstance(want, str):
        return answer == want
    code, tag, hexes = want
    parts = answer.split(" ")
    return len(parts) == 3 and parts[1] == tag and parts[2] == hexes and (code is None or parts[0] == code)


def killed(compiler, sources, work, pairs):
    std_local = lambda text: (text.replace("import std.bigmod;", "import bigmod;")
                              .replace("import std.ecdh;", "import ecdh;")
                              .replace("import std.ecdsa_sign;", "import ecdsa_sign;"))
    files = {
        "bigmod.ls": sources["bigmod"].replace("module std.bigmod;", "module bigmod;", 1),
        "ecdh.ls": std_local(sources["ecdh"].replace("module std.ecdh;", "module ecdh;", 1)),
        "ecdsa_sign.ls": std_local(sources["ecdsa_sign"].replace("module std.ecdsa_sign;", "module ecdsa_sign;", 1)),
        "key.ls": std_local(sources["key"]),
        "x509.ls": open(os.path.join(ROOT, "packages/x509/x509.ls")).read(),
        "driver.ls": std_local(open(os.path.join(ROOT, "tests/programs/ecdsa_sign_driver.ls")).read()),
    }
    for name, text in files.items():
        open(os.path.join(work, name), "w").write(text)
    exe = os.path.join(work, "driver")
    build = subprocess.run([compiler, "build", "--std"] + [os.path.join(work, n) for n in files] + ["-o", exe],
                           capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    run = subprocess.run([exe], input="\n".join(c for c, _ in pairs) + "\n", capture_output=True, text=True, timeout=600)
    answers = [a.rstrip() for a in run.stdout.splitlines()]
    if run.returncode != 0 or len(answers) != len(pairs):
        return True, ["the driver failed or stopped early"]
    for (case, want), answer in zip(pairs, answers):
        if not agrees(answer, want):
            return True, [f"{case[:40]}: {answer[:60]}"]
    script = os.path.join(ROOT, "scripts/ecdsa_sign_differential.py")
    for args in (["reference", exe, "300"], ["openssl", exe, ECDSA_DRIVER, "100"], ["keys", exe, "6"]):
        diff = subprocess.run([sys.executable, script] + args, capture_output=True, text=True)
        if diff.returncode != 0:
            return True, [f"the {args[0]} differential"]
    return False, []


ECDSA_DRIVER = None


def main():
    global ECDSA_DRIVER
    compiler = os.path.abspath(sys.argv[1])
    sources = {m: open(os.path.join(ROOT, f"std/{m}.ls")).read() for m in ("ecdsa_sign", "bigmod", "ecdh")}
    sources["key"] = open(os.path.join(ROOT, "packages/x509/key.ls")).read()
    pairs = evidence()
    failed = 0
    with tempfile.TemporaryDirectory() as work:
        ECDSA_DRIVER = os.path.join(work, "ecdsa_driver")
        subprocess.run([compiler, "build", "--std", os.path.join(ROOT, "tests/programs/ecdsa_driver.ls"), "-o", ECDSA_DRIVER],
                       check=True, capture_output=True)
        dead, why = killed(compiler, sources, work, pairs)
        assert dead is False, f"the unmutated files must pass: {why}"
        print(f"unmutated: passes {len(pairs)} cases and the three differentials")
        for module, name, old, new in MUTANTS:
            assert sources[module].count(old) == 1, f"{name}: `{old}` occurs {sources[module].count(old)} times"
            mutated = dict(sources)
            mutated[module] = sources[module].replace(old, new)
            dead, why = killed(compiler, mutated, work, pairs)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {module}: {name}: {'; '.join(why)}", flush=True)
            failed += dead is not True
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
