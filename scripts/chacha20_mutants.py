#!/usr/bin/env python3
"""Mutation check of `std/chacha20.cho` (docs/chacha20.md §5).

    python3 scripts/chacha20_mutants.py <cancho binary>

Each mutant is `std/chacha20.cho` with one deliberate bug, built as a local
module `chacha20` beside a copy of `tests/programs/aead_driver.cho`, and run
against the same evidence `conformance/aead.rs` and
`scripts/aead_differential.py` use: the RFC 8439 table, every Wycheproof
case, the one-bit flips of §2.8.2, the refusal edges, and 2,000 random cases
against OpenSSL. A mutant is killed when any of them disagrees. The unmutated
file is run first and must pass, or the check proves nothing. Exit status 1
if a mutant survives or fails to build.
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (name, the text replaced, its replacement). Each `old` must occur exactly once.
MUTANTS = [
    ("rotation 16 becomes 15", "rotl32(x[d] ^ x[a], 16)", "rotl32(x[d] ^ x[a], 15)"),
    ("rotation 7 becomes 8", "rotl32(x[b] ^ x[c], 7)", "rotl32(x[b] ^ x[c], 8)"),
    ("nine double rounds, not ten", "while round < 10", "while round < 9"),
    ("a wrong constant", "work[0] = 0x61707865", "work[0] = 0x61707866"),
    ("the input words not added back", "m32(wrapping_add(work[16 + i], work[i]))", "m32(work[16 + i])"),
    ("a skipped Poly1305 carry", "    d1 = wrapping_add(d1, c);\n", "    d1 = wrapping_add(d1, 0);\n"),
    ("r not clamped", "le32(key, 3) >> 2 & 0x3ffff03", "le32(key, 3) >> 2 & 0x3ffffff"),
    ("pad16 block without its 2^128 bit", "poly_block(st, scratch[0..16], 0x1000000)", "poly_block(st, scratch[0..16], 0)"),
    ("never reduced below p", "let keep = value_barrier(g4 >> 63);", "let keep = 0;"),
    ("h - p computed as h + 5", "let g4 = wrapping_sub(wrapping_add(h4, c), 0x4000000);", "let g4 = wrapping_add(h4, c);"),
    ("the carry into the tag's second word dropped", "f = wrapping_add(wrapping_add(w1, le32(key, 20)), f >> 32);", "f = wrapping_add(w1, le32(key, 20));"),
    ("a one-byte-short tag compare", "while i < 16 {\n            diff = diff | int_of(want[i]) ^ int_of(sealed[text + i]);", "while i < 15 {\n            diff = diff | int_of(want[i]) ^ int_of(sealed[text + i]);"),
    ("plaintext released on a bad tag", "    if diff != 0 {\n        return refused_tag_mismatch();\n    }\n    xor(key, 1, nonce, sealed[0..text], out);", "    if diff != diff {\n        return refused_tag_mismatch();\n    }\n    xor(key, 1, nonce, sealed[0..text], out);"),
    # In place (docs/record-in-place.md §3).
    ("in place: plaintext released on a bad tag", "    if diff != 0 {\n        return refused_tag_mismatch();\n    }\n    xor(key, 1, nonce, buf[0..text], buf[0..text]);", "    if diff != diff {\n        return refused_tag_mismatch();\n    }\n    xor(key, 1, nonce, buf[0..text], buf[0..text]);"),
    ("in place: a one-byte-short tag compare", "while i < 16 {\n            diff = diff | int_of(want[i]) ^ int_of(buf[text + i]);", "while i < 15 {\n            diff = diff | int_of(want[i]) ^ int_of(buf[text + i]);"),
    ("in place: encryption from counter 0", "    xor(key, 1, nonce, buf[0..text], buf[0..text]);\n    aead_tag(", "    xor(key, 0, nonce, buf[0..text], buf[0..text]);\n    aead_tag("),
    ("in place: the tag written over the start of the text", "    aead_tag(key, nonce, aad, buf[0..text], buf[text..text + 16]);", "    aead_tag(key, nonce, aad, buf[0..text], buf[0..16]);"),
    ("in place: the tag compared with the start of the text", "            diff = diff | int_of(want[i]) ^ int_of(buf[text + i]);", "            diff = diff | int_of(want[i]) ^ int_of(buf[i]);"),
    ("in place: a buffer with no room for a tag sealed", "    if len(buf) < 16 {\n        let shape = aead_lengths(key, nonce, 0);\n        if shape != 0 {\n            return shape;\n        }\n        return refused_output_length();", "    if len(buf) < 0 {\n        let shape = aead_lengths(key, nonce, 0);\n        if shape != 0 {\n            return shape;\n        }\n        return refused_output_length();"),
    ("in place: a buffer with no room for a tag opened", "    if len(buf) < 16 {\n        let shape = aead_lengths(key, nonce, 0);\n        if shape != 0 {\n            return shape;\n        }\n        return refused_too_short();", "    if len(buf) < 0 {\n        let shape = aead_lengths(key, nonce, 0);\n        if shape != 0 {\n            return shape;\n        }\n        return refused_too_short();"),
    ("in place: the tag taken over the plaintext when sealing", "    xor(key, 1, nonce, buf[0..text], buf[0..text]);\n    aead_tag(key, nonce, aad, buf[0..text], buf[text..text + 16]);", "    aead_tag(key, nonce, aad, buf[0..text], buf[text..text + 16]);\n    xor(key, 1, nonce, buf[0..text], buf[0..text]);"),
    ("the lengths block in the wrong order", "bytes.store_le32(lens, 0, m32(len(aad)));", "bytes.store_le32(lens, 0, m32(len(ct)));"),
    ("encryption from counter 0", "xor(key, 1, nonce, plaintext, out[0..text]);", "xor(key, 0, nonce, plaintext, out[0..text]);"),
    ("the counter allowed to wrap", "blocks > 0x100000000 - counter", "blocks > 0x100000001 - counter"),
    ("a wrong product in the Poly1305 multiply", "let gh = wrapping_mul(g, h);", "let gh = wrapping_mul(g, g);"),
    ("a long nonce accepted", "    if len(nonce) != 12 {\n        return refused_nonce_length();\n    }\n    if (text", "    if len(nonce) < 12 {\n        return refused_nonce_length();\n    }\n    if (text"),
]


def h(b):
    return b.hex() if b else "-"


def evidence():
    """The cases, and a function that says whether the answers are right."""
    cases, checks = [], []
    for line in open(os.path.join(ROOT, "tests/vectors/rfc8439.txt")):
        if line.startswith("#") or not line.strip():
            continue
        _, case, want = line.rstrip("\n").split(" | ")
        cases.append(case)
        checks.append(lambda a, want=want: a == f"0 ok {want}")
    wp = json.load(open(os.path.join(ROOT, "tests/vectors/wycheproof/chacha20_poly1305_test.json")))
    for group in wp["testGroups"]:
        for t in group["tests"]:
            k, iv, aad, msg, ct, tag = (t[x] or "-" for x in ("key", "iv", "aad", "msg", "ct", "tag"))
            sealed = (t["ct"] + t["tag"]) or "-"
            cases.append(f"O {k} {iv} {aad} {sealed}")
            if t["result"] == "valid":
                checks.append(lambda a, m=t["msg"]: a == f"0 ok {m}")
                cases.append(f"S {k} {iv} {aad} {msg}")
                checks.append(lambda a, s=t["ct"] + t["tag"]: a == f"0 ok {s}")
            elif "InvalidNonceSize" in t["flags"]:
                checks.append(lambda a: a.startswith("-2 chacha20-nonce-length"))
                cases.append(f"S {k} {iv} {aad} {msg}")
                checks.append(lambda a: a.startswith("-2 chacha20-nonce-length"))
            else:
                checks.append(lambda a, n=len(t["ct"]) // 2: a.rstrip() == f"-6 aead-tag-mismatch {'aa' * n}".rstrip())
    key, nonce = h(bytes([7] * 32)), h(bytes([9] * 12))
    cases.append(f"X {key} 4294967295 {nonce} {h(bytes(65))}")
    checks.append(lambda a: a.startswith("-4 "))
    cases.append(f"X {key} 4294967295 {nonce} {h(bytes(64))}")
    checks.append(lambda a: a.startswith("0 "))
    # §2.8.2 with each bit flipped.
    row = [l for l in open(os.path.join(ROOT, "tests/vectors/rfc8439.txt")) if l.startswith("rfc8439 2.8.2")][0]
    _, case, sealed = row.rstrip("\n").split(" | ")
    f = case.split(" ")
    s = bytes.fromhex(sealed)
    for bit in range(len(s) * 8):
        b = bytearray(s)
        b[bit // 8] ^= 1 << (bit % 8)
        cases.append(f"O {f[1]} {f[2]} {f[3]} {b.hex()}")
        checks.append(lambda a, n=len(s) - 16: a == f"-6 aead-tag-mismatch {'aa' * n}")
    # A buffer with no room for a tag, in place: too short to open, and to seal the output's length.
    for n in range(16):
        b = h(bytes(range(n)))
        cases.append(f"V {key} {nonce} - {b}")
        checks.append(lambda a, b=b: a.rstrip() == f"-3 chacha20-output-length {b if b != '-' else ''}".rstrip())
        cases.append(f"U {key} {nonce} - {b}")
        checks.append(lambda a, b=b: a.rstrip() == f"-5 aead-too-short {b if b != '-' else ''}".rstrip())
    # Every seal and open again in place (docs/record-in-place.md §3): a seal answers what `seal` does, an open what
    # `open` does, but for a refusal, which leaves the buffer (the sealed message) as it was.
    for case, check in list(zip(cases, checks)):
        if case[:2] in ("S ", "O "):
            cases.append({"S": "T", "O": "U"}[case[0]] + case[1:])
            if case[0] == "S":
                checks.append(check)
            else:
                sealed = case.split(" ")[4]
                n = max(len(sealed) // 2 - 16, 0) if sealed != "-" else 0
                checks.append(lambda a, c=check, s=sealed, n=n: (a.startswith("0 ") and c(a)) or
                              (a.startswith("-6 ") and a == f"-6 aead-tag-mismatch {s}" and c(f"-6 aead-tag-mismatch {'aa' * n}")) or
                              (not a.startswith(("0 ", "-6 ")) and c(" ".join(a.split(" ")[:2]) + " " + "aa" * n)))
    return cases, checks


def killed(compiler, source, driver_src, work, cases, checks):
    mod = os.path.join(work, "chacha20.cho")
    drv = os.path.join(work, "driver.cho")
    exe = os.path.join(work, "driver")
    open(mod, "w").write(source.replace("module std.chacha20;", "module chacha20;", 1))
    open(drv, "w").write(driver_src.replace("import std.chacha20;", "import chacha20;", 1))
    build = subprocess.run([compiler, "build", "--std", drv, mod, "-o", exe], capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    run = subprocess.run([exe], input="\n".join(cases) + "\n", capture_output=True, text=True, timeout=300)
    answers = run.stdout.splitlines()
    if run.returncode != 0 or len(answers) != len(cases):
        return True, ["the driver failed or stopped early"]
    for i, (check, answer) in enumerate(zip(checks, answers)):
        if not check(answer):
            return True, [f"case {i}: {cases[i][:60]}"]
    diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/aead_differential.py"), exe, "2000"], capture_output=True, text=True)
    if diff.returncode != 0:
        return True, ["the OpenSSL differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    source = open(os.path.join(ROOT, "std/chacha20.cho")).read()
    driver_src = open(os.path.join(ROOT, "tests/programs/aead_driver.cho")).read()
    cases, checks = evidence()
    failed = 0
    with tempfile.TemporaryDirectory() as work:
        dead, why = killed(compiler, source, driver_src, work, cases, checks)
        assert dead is False, f"the unmutated file must pass: {why}"
        print(f"unmutated: passes {len(cases)} cases and the differential")
        for name, old, new in MUTANTS:
            assert source.count(old) == 1, f"{name}: `{old}` occurs {source.count(old)} times"
            dead, why = killed(compiler, source.replace(old, new), driver_src, work, cases, checks)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {name}: {'; '.join(why)}")
            failed += dead is not True
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
