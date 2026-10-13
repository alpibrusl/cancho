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
    # The hardware path (`docs/crypto-builtins.md` §6, step 4; `docs/gcm-wide.md` §3.4): killed only where `hw_aes_gcm()` is
    # true. The builtins' own code is mutated by `scripts/gcm_wide_mutants.py`.
    ("gcm", "hardware: encryption from counter 1", "aes_ctr32(keys, nr, nonce, 2, plaintext, out[0..text]);", "aes_ctr32(keys, nr, nonce, 1, plaintext, out[0..text]);"),
    ("gcm", "hardware: decryption from counter 1", "aes_ctr32(keys, nr, nonce, 2, sealed[0..text], out);", "aes_ctr32(keys, nr, nonce, 1, sealed[0..text], out);"),
    ("gcm", "hardware: decrypted before the tag is checked", "    if diff != 0 {\n        return refused_tag_mismatch();\n    }\n    aes_ctr32(keys, nr, nonce, 2, sealed[0..text], out);", "    if diff == 99 {\n        return refused_tag_mismatch();\n    }\n    aes_ctr32(keys, nr, nonce, 2, sealed[0..text], out);"),
    ("gcm", "hardware: the tag taken from the start of the sealed message", "sealed[0..text], sealed[text..text + 16]);", "sealed[0..text], sealed[0..16]);"),
    ("gcm", "hardware: the associated data not hashed when sealing", "nonce, aad, out[0..text], out[text..text + 16]);", "nonce, nonce[0..0], out[0..text], out[text..text + 16]);"),
    ("gcm", "hardware: H made with AES-128's round count", "ghash_powers(hw[0..(nr + 1) * 16], nr, hw[", "ghash_powers(hw[0..176], 10, hw["),
    ("gcm", "hardware: the powers overlap the round keys", "fn hw_table() -> [] int {\n    return 240;", "fn hw_table() -> [] int {\n    return 232;"),
    ("gcm", "hardware: AES-256's round keys cut to AES-128's", "    let nr = ctx[0];\n    let keys = hw[0..(nr + 1) * 16];\n    aes_ctr32(keys, nr, nonce, 2, plaintext", "    let nr = ctx[0];\n    let keys = hw[0..176];\n    aes_ctr32(keys, nr, nonce, 2, plaintext"),
    # The prepared key (`docs/crypto-builtins.md` §6, step 2).
    ("gcm", "an unprepared context taken as prepared", "&& (ctx[0] == 10 || ctx[0] == 14)", "&& ctx[0] >= 0"),
    ("gcm", "the last partial block not padded with zeros", "        var b = 0;\n        if at + k < len(s) {", "        var b = 255;\n        if at + k < len(s) {"),
    ("gcm", "a one-byte-short tag compare", "tag_of(ctx, nonce, aad, sealed[0..text], want, q, w, blk);\n        // Every byte is compared whatever the earlier ones were.\n        var i = 0;\n        while i < 16 {", "tag_of(ctx, nonce, aad, sealed[0..text], want, q, w, blk);\n        // Every byte is compared whatever the earlier ones were.\n        var i = 0;\n        while i < 15 {"),
    ("gcm", "plaintext released on a bad tag", "        if diff == 0 {\n            aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, sealed[0..text], out, q);", "        if diff == diff {\n            aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, sealed[0..text], out, q);"),
    # In place (docs/record-in-place.md §3).
    ("gcm", "in place, hardware: decrypted before the tag is checked", "    if diff != 0 {\n        return refused_tag_mismatch();\n    }\n    aes_ctr32(keys, nr, nonce, 2, buf[0..text], buf[0..text]);", "    if diff == 99 {\n        return refused_tag_mismatch();\n    }\n    aes_ctr32(keys, nr, nonce, 2, buf[0..text], buf[0..text]);"),
    ("gcm", "in place, hardware: the tag written over the start of the text", "gcm_tag(keys, nr, hw[hw_table()..hw_table() + hw_table_len()], nonce, aad, buf[0..text], buf[text..text + 16]);", "gcm_tag(keys, nr, hw[hw_table()..hw_table() + hw_table_len()], nonce, aad, buf[0..text], buf[0..16]);"),
    ("gcm", "in place, hardware: the tag compared with the start of the text", "gcm_tag_diff(keys, nr, hw[hw_table()..hw_table() + hw_table_len()], nonce, aad, buf[0..text], buf[text..text + 16]);", "gcm_tag_diff(keys, nr, hw[hw_table()..hw_table() + hw_table_len()], nonce, aad, buf[0..text], buf[0..16]);"),
    ("gcm", "in place, hardware: encryption from counter 1", "    aes_ctr32(keys, nr, nonce, 2, buf[0..text], buf[0..text]);\n    gcm_tag(", "    aes_ctr32(keys, nr, nonce, 1, buf[0..text], buf[0..text]);\n    gcm_tag("),
    ("gcm", "in place, software: plaintext released on a bad tag", "        if diff == 0 {\n            aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, buf[0..text], buf[0..text], q);", "        if diff == diff {\n            aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, buf[0..text], buf[0..text], q);"),
    ("gcm", "in place, software: encryption from counter 1", "        aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 2, buf[0..text], buf[0..text], q);\n        tag_of(", "        aes.ctr32_with(ctx[0], ctx[c_skey()..c_h()], nonce, 1, buf[0..text], buf[0..text], q);\n        tag_of("),
    ("gcm", "in place, software: the tag read from the text", "            diff = diff | int_of(want[i]) ^ int_of(buf[text + i]);", "            diff = diff | int_of(want[i]) ^ int_of(buf[i]);"),
    ("gcm", "in place: a buffer with no room for a tag taken", "    if len(buf) < 16 {\n        let shape = shape_ok(nonce, 0);", "    if len(buf) < 0 {\n        let shape = shape_ok(nonce, 0);"),
    ("gcm", "in place: a buffer too short to seal refused as too short to open", "    let text = in_place_text(ctx, nonce, buf, refused_output_length());\n    if text < 0 {\n        return text;\n    }\n    region t {", "    let text = in_place_text(ctx, nonce, buf, refused_too_short());\n    if text < 0 {\n        return text;\n    }\n    region t {"),
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
    # A buffer with no room for a tag, in place on both paths: too short to open, and to seal the output's length.
    for n in range(16):
        b = h(bytes(range(n)))
        for op, tag in (("Y", "gcm-output-length"), ("y", "gcm-output-length"), ("I", "gcm-too-short"), ("i", "gcm-too-short")):
            cases.append(f"{op} {key} {nonce} - {b}")
            checks.append(lambda a, t=tag, b=b: a.rstrip() == f"{'-3' if t == 'gcm-output-length' else '-5'} {t} {b if b != '-' else ''}".rstrip())
    # And every one in place (docs/record-in-place.md §3), on both paths: a seal answers what `seal` does; an open
    # what `open` does, but for a refusal, which leaves the buffer (the sealed message) as it was.
    for case, check in list(zip(cases, checks)):
        if case[0] in "SsOo" and case[1] == " ":
            sealed = case.split(" ")[4]
            op = {"S": "P", "s": "p", "O": "I", "o": "i"}[case[0]]
            cases.append(op + case[1:])
            if op in "Pp":
                checks.append(check)
            else:
                n = len(sealed) // 2 - 16 if sealed != "-" else 0
                checks.append(lambda a, c=check, s=sealed, n=n: (a.startswith("0 ") and c(a)) or
                              (a.startswith("-6 ") and a == f"-6 aead-tag-mismatch {s}" and c(f"-6 aead-tag-mismatch {'aa' * max(n, 0)}")) or
                              (not a.startswith(("0 ", "-6 ")) and c(" ".join(a.split(" ")[:2]) + " " + "aa" * max(n, 0))))
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
