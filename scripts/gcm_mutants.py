#!/usr/bin/env python3
"""Mutation check of `std/aes.cho` and `std/gcm.cho` (docs/tls-parity.md §3.1).

    python3 scripts/gcm_mutants.py <cancho binary>

Each mutant is one of the two files with one deliberate bug, both built as
local modules `aes` and `gcm` beside a copy of `tests/programs/gcm_driver.cho`,
and run against the same evidence `conformance/gcm.rs` and
`scripts/gcm_differential.py` use: FIPS 197's examples, NIST CAVP's 96-bit-IV
cases, every Wycheproof case, the counter edges, a message with each bit
flipped, and 2,000 random cases against OpenSSL. A mutant is killed when any
of them disagrees. The unmutated files are run first and must pass, or the
check proves nothing. Exit status 1 if a mutant survives or fails to build.
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (file, name, the text replaced, its replacement). Each `old` must occur
# exactly once in its file.
MUTANTS = [
    ("aes", "one round fewer", "    while u < nr {", "    while u < nr - 1 {"),
    ("aes", "a gate of the S-box's top layer", "let y14 = x3 ^ x5;", "let y14 = x3 ^ x4;"),
    ("aes", "ShiftRows moves one bit pair the wrong way", "(x & 0x00000300) << 6", "(x & 0x00000300) << 4"),
    ("aes", "MixColumns misses a term", "q[2] = q1 ^ r1 ^ r2 ^ rotr16(q2 ^ r2);", "q[2] = q1 ^ r2 ^ rotr16(q2 ^ r2);"),
    ("aes", "a wrong round constant", "        return 0x1b;", "        return 0x1c;"),
    ("aes", "AES-256's extra SubWord skipped", "} else if nk > 6 && j == 4 {\n            tmp = sub_word(tmp);\n        }\n        tmp = tmp ^ skey[", "} else if nk > 8 && j == 4 {\n            tmp = sub_word(tmp);\n        }\n        tmp = tmp ^ skey["),
    ("aes", "round keys: AES-256's extra SubWord skipped", "} else if nk > 6 && j == 4 {\n            tmp = sub_word(tmp);\n        }\n        tmp = tmp ^ le32(out", "} else if nk > 8 && j == 4 {\n            tmp = sub_word(tmp);\n        }\n        tmp = tmp ^ le32(out"),
    ("aes", "the key's odd bits expanded the wrong way", "skey[2 * i + 1] = y | y >> 1;", "skey[2 * i + 1] = y | m32(y << 1);"),
    ("aes", "an ortho swap left out", "    swapn(q, 0x33333333, 0xcccccccc, 2, at + 5, at + 7);\n", ""),
    ("aes", "the counter advanced by one per pair", "        cc = cc + 2;", "        cc = cc + 1;"),
    ("aes", "the second block reuses the first's counter", "q[7] = swap32(m32(cc + 1));", "q[7] = swap32(cc);"),
    ("aes", "the keystream's second block never used", "q[2 * ((k & 15) >> 2) + (k >> 4)]", "q[2 * ((k & 15) >> 2)]"),
    ("aes", "the counter allowed to wrap", "counter + blocks - 1 > 0xffffffff", "counter + blocks - 1 > 0x100000000"),
    ("gcm", "a term of the carry-less multiply dropped", "let z3 = (x0 * y3 ^ x1 * y2 ^ x2 * y1 ^ x3 * y0) & 0x88888888;", "let z3 = (x0 * y3 ^ x1 * y2 ^ x2 * y1) & 0x88888888;"),
    ("gcm", "the reduction's x^7 term wrong", "lw ^ lw >> 1 ^ lw >> 2 ^ lw >> 7", "lw ^ lw >> 1 ^ lw >> 2 ^ lw >> 6"),
    ("gcm", "a Karatsuba middle product left uncorrected", "w[c + 17] = w[c + 17] ^ w[c + 15] ^ w[c + 16];", "w[c + 17] = w[c + 17] ^ w[c + 15];"),
    ("gcm", "H not bit-reversed for the reversed products", "ctx[c_h() + 4 + i] = rev32(ctx[c_h() + i]);", "ctx[c_h() + 4 + i] = ctx[c_h() + i];"),
    ("gcm", "the ciphertext's length in bytes, not bits", "    // The lengths block: both lengths in bits, 64 bits each.\n    let abits = len(aad) * 8;\n    let cbits = len(ciphertext) * 8;", "    // The lengths block: both lengths in bits, 64 bits each.\n    let abits = len(aad) * 8;\n    let cbits = len(ciphertext);"),
    ("gcm", "the associated data and ciphertext hashed in the wrong order", "    ghash(w, aad);\n    ghash(w, ciphertext);", "    ghash(w, ciphertext);\n    ghash(w, aad);"),
    ("gcm", "the tag masked with counter 2, not J0", "aes.ctr32_with(nr, skey, nonce, 1, blk, tag, q);", "aes.ctr32_with(nr, skey, nonce, 2, blk, tag, q);"),
    ("gcm", "encryption from counter 1", "aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, plaintext, out[0..text], q);", "aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 1, plaintext, out[0..text], q);"),
    # The hardware path (`docs/crypto-builtins.md` §6, step 4): killed only where `hw_aes_gcm()` is true.
    ("gcm", "hardware: encryption from counter 1", "ctr_hw(keys, nr, nonce, 2, plaintext, out[0..text], blk, ks);", "ctr_hw(keys, nr, nonce, 1, plaintext, out[0..text], blk, ks);"),
    ("gcm", "hardware: the last partial block not padded with zeros", "            pad[k] = byte_of(0);\n", "            pad[k] = byte_of(1);\n"),
    ("gcm", "hardware: the ciphertext's length in bytes", "    ghash_hw(h, y, ciphertext, blk);\n    let abits = len(aad) * 8;\n    let cbits = len(ciphertext) * 8;", "    ghash_hw(h, y, ciphertext, blk);\n    let abits = len(aad) * 8;\n    let cbits = len(ciphertext);"),
    ("gcm", "hardware: decrypted before the tag is checked", "        if diff == 0 {\n            ctr_hw(", "        if true {\n            ctr_hw("),
    ("gcm", "hardware: H not the encryption of zero", "aes_encrypt_block(hw[0..(nr + 1) * 16], nr, zeros, hw[hw_h()..hw_h() + 16]);", "aes.round_keys(key, hw[hw_h()..hw_h() + 16]);"),
    # The prepared key (`docs/crypto-builtins.md` §6, step 2).
    ("gcm", "an unprepared context taken as prepared", "&& (ctx[0] == 10 || ctx[0] == 14)", "&& ctx[0] >= 0"),
    ("gcm", "the last partial block not padded with zeros", "        var b = 0;\n        if at + k < len(s) {", "        var b = 255;\n        if at + k < len(s) {"),
    ("gcm", "a one-byte-short tag compare", "q, w, blk);\n        // Every byte is compared whatever the earlier ones were.\n        var i = 0;\n        while i < 16 {", "q, w, blk);\n        // Every byte is compared whatever the earlier ones were.\n        var i = 0;\n        while i < 15 {"),
    ("gcm", "hardware: a one-byte-short tag compare", "y, blk, ks);\n        // Every byte is compared whatever the earlier ones were.\n        var i = 0;\n        while i < 16 {", "y, blk, ks);\n        // Every byte is compared whatever the earlier ones were.\n        var i = 0;\n        while i < 15 {"),
    ("gcm", "plaintext released on a bad tag", "        if diff == 0 {\n            aes.ctr32_with(", "        if diff == diff {\n            aes.ctr32_with("),
    ("gcm", "a long nonce accepted", "if len(nonce) != 12 {", "if len(nonce) < 12 {"),
]


def h(b):
    return b.hex() if b else "-"


def cavp(name):
    cases, cur = [], None
    for line in open(os.path.join(ROOT, "tests/vectors/cavp", name)):
        line = line.strip()
        if line.startswith("Count = "):
            cur = {"FAIL": False}
            cases.append(cur)
        elif line == "FAIL":
            cur["FAIL"] = True
        elif cur is not None and " = " in line:
            k, v = line.split(" = ", 1)
            cur[k] = v
        elif cur is not None and line.endswith(" ="):
            cur[line[:-2]] = ""
    return cases


def evidence():
    """The cases, and a function each that says whether the answer is right."""
    cases, checks = [], []
    block = "00112233445566778899aabbccddeeff"
    for key, want in [("000102030405060708090a0b0c0d0e0f", "69c4e0d86a7b0430d8cdb78070b4c55a"),
                      ("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f", "8ea2b7ca516745bfeafc49904b496089")]:
        cases.append(f"E {key} {block}")
        checks.append(lambda a, w=want: a == f"0 ok {w}")
    for name in ["gcmEncryptExtIV128.rsp", "gcmEncryptExtIV256.rsp"]:
        for c in cavp(name):
            cases.append(f"S {c['Key']} {c['IV']} {c['AAD'] or '-'} {c['PT'] or '-'}")
            checks.append(lambda a, w=c["CT"] + c["Tag"]: a == f"0 ok {w}")
    for name in ["gcmDecrypt128.rsp", "gcmDecrypt256.rsp"]:
        for c in cavp(name):
            cases.append(f"O {c['Key']} {c['IV']} {c['AAD'] or '-'} {c['CT'] + c['Tag']}")
            if c["FAIL"]:
                checks.append(lambda a, n=len(c["CT"]) // 2: a.rstrip() == f"-6 aead-tag-mismatch {'aa' * n}".rstrip())
            else:
                checks.append(lambda a, w=c["PT"]: a.rstrip() == f"0 ok {w}".rstrip())
    wp = json.load(open(os.path.join(ROOT, "tests/vectors/wycheproof/aes_gcm_test.json")))
    for group in wp["testGroups"]:
        for t in group["tests"]:
            k, iv, aad, msg = (t[x] or "-" for x in ("key", "iv", "aad", "msg"))
            sealed = (t["ct"] + t["tag"]) or "-"
            supported = len(t["key"]) in (32, 64) and len(t["iv"]) == 24
            cases.append(f"O {k} {iv} {aad} {sealed}")
            if not supported:
                checks.append(lambda a: a.startswith("-1 ") or a.startswith("-2 "))
            elif t["result"] == "valid":
                checks.append(lambda a, m=t["msg"]: a.rstrip() == f"0 ok {m}".rstrip())
                cases.append(f"S {k} {iv} {aad} {msg}")
                checks.append(lambda a, s=t["ct"] + t["tag"]: a == f"0 ok {s}")
            else:
                checks.append(lambda a, n=len(t["ct"]) // 2: a.rstrip() == f"-6 aead-tag-mismatch {'aa' * n}".rstrip())
    key, nonce = h(bytes([7] * 16)), h(bytes([9] * 12))
    cases.append(f"C {key} {nonce} 4294967295 {h(bytes(17))}")
    checks.append(lambda a: a.startswith("-2 aes-counter-exhausted"))
    cases.append(f"C {key} {nonce} 4294967295 {h(bytes(16))}")
    checks.append(lambda a: a.startswith("0 "))
    cases.append(f"S {key} {h(bytes(16))} - 00")
    checks.append(lambda a: a.startswith("-2 gcm-nonce-length"))
    # A context `gcm.prepare` never filled is refused as a key (step 2's prepared key).
    cases.append(f"U {key} {nonce} - 00")
    checks.append(lambda a: a.startswith("-1 gcm-key-length"))
    # A CAVP case with a 51-byte message and 20 bytes of associated data,
    # each bit of its sealed message flipped in turn.
    c = [c for c in cavp("gcmEncryptExtIV256.rsp") if len(c["PT"]) == 102 and len(c["AAD"]) == 40][0]
    s = bytes.fromhex(c["CT"] + c["Tag"])
    for bit in range(len(s) * 8):
        b = bytearray(s)
        b[bit // 8] ^= 1 << (bit % 8)
        cases.append(f"O {c['Key']} {c['IV']} {c['AAD']} {b.hex()}")
        checks.append(lambda a, n=len(s) - 16: a == f"-6 aead-tag-mismatch {'aa' * n}")
    # Every seal and open again on the software path (`s`, `o`), so a machine whose CPU takes the hardware path for `S`
    # and `O` still tests the software one (docs/crypto-builtins.md §7).
    for case, check in list(zip(cases, checks)):
        if case[:2] in ("S ", "O "):
            cases.append(case[0].lower() + case[1:])
            checks.append(check)
    return cases, checks


def killed(compiler, aes_src, gcm_src, driver_src, work, cases, checks):
    files = {
        "aes.cho": aes_src.replace("module std.aes;", "module aes;", 1),
        "gcm.cho": gcm_src.replace("module std.gcm;", "module gcm;", 1).replace("import std.aes;", "import aes;", 1),
        "driver.cho": driver_src.replace("import std.aes;", "import aes;", 1).replace("import std.gcm;", "import gcm;", 1),
    }
    for name, text in files.items():
        open(os.path.join(work, name), "w").write(text)
    exe = os.path.join(work, "driver")
    build = subprocess.run([compiler, "build", "--std"] + [os.path.join(work, n) for n in ["driver.cho", "aes.cho", "gcm.cho"]] + ["-o", exe],
                           capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    run = subprocess.run([exe], input="\n".join(cases) + "\n", capture_output=True, text=True, timeout=300)
    answers = run.stdout.splitlines()
    if run.returncode != 0 or len(answers) != len(cases):
        return True, ["the driver failed or stopped early"]
    for i, (check, answer) in enumerate(zip(checks, answers)):
        if not check(answer):
            return True, [f"case {i}: {cases[i][:60]}"]
    diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/gcm_differential.py"), exe, "2000"], capture_output=True, text=True)
    if diff.returncode != 0:
        return True, ["the OpenSSL differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    sources = {m: open(os.path.join(ROOT, f"std/{m}.cho")).read() for m in ("aes", "gcm")}
    driver_src = open(os.path.join(ROOT, "tests/programs/gcm_driver.cho")).read()
    cases, checks = evidence()
    failed = 0
    with tempfile.TemporaryDirectory() as work:
        dead, why = killed(compiler, sources["aes"], sources["gcm"], driver_src, work, cases, checks)
        assert dead is False, f"the unmutated files must pass: {why}"
        print(f"unmutated: passes {len(cases)} cases and the differential")
        for module, name, old, new in MUTANTS:
            assert sources[module].count(old) == 1, f"{name}: `{old}` occurs {sources[module].count(old)} times"
            mutated = dict(sources)
            mutated[module] = sources[module].replace(old, new)
            dead, why = killed(compiler, mutated["aes"], mutated["gcm"], driver_src, work, cases, checks)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {module}: {name}: {'; '.join(why)}", flush=True)
            failed += dead is not True
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
