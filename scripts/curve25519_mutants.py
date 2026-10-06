#!/usr/bin/env python3
"""Mutation check of `std/field25519.cho`, `std/x25519.cho` and the ported `std/ed25519.cho` (docs/x25519.md §5).

    python3 scripts/curve25519_mutants.py <cancho binary>

Each mutant is one of the three files with one deliberate bug. All three are built as local modules (`field25519`,
`x25519`, `ed25519`) beside a copy of `tests/programs/curve25519_driver.cho`, and run against: the RFC 7748 table, every
Wycheproof X25519 and Ed25519 case, `tests/accept/ed25519.cho`'s three OpenSSL keypairs (public key, signature, verify, a tampered
signature refused), and 300 rounds of `scripts/curve25519_differential.py`. A mutant is killed when any of them
disagrees or the driver traps. The unmutated files are run first and must pass. Exit status 1 if a mutant survives or
fails to build.
"""
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

MUTANTS = [
    ("field25519", "the carry's 2^256 = 38 fold as 37", "o[0] = wrapping_add(o[0], wrapping_mul(38, wrapping_sub(c, 1)));", "o[0] = wrapping_add(o[0], wrapping_mul(37, wrapping_sub(c, 1)));"),
    ("field25519", "the product's fold of the top limbs as 19", "        t[i] = wrapping_add(t[i], wrapping_mul(38, t[i + 16]));\n        i = i + 1;\n    }\n    i = 0;\n    while i < 16 {\n        o[i] = t[i];", "        t[i] = wrapping_add(t[i], wrapping_mul(19, t[i + 16]));\n        i = i + 1;\n    }\n    i = 0;\n    while i < 16 {\n        o[i] = t[i];"),
    ("field25519", "one carry after a product, not two", "        o[i] = t[i];\n        i = i + 1;\n    }\n    carry(o);\n    carry(o);", "        o[i] = t[i];\n        i = i + 1;\n    }\n    carry(o);"),
    ("field25519", "cswap that swaps only fifteen limbs", "    let mask = value_barrier(wrapping_sub(0, bit));\n    var i = 0;\n    while i < 16 {\n        let x = mask", "    let mask = value_barrier(wrapping_sub(0, bit));\n    var i = 0;\n    while i < 15 {\n        let x = mask"),
    ("field25519", "the inversion's exponent off (bit 3 skipped, not 4)", "        if bit != 2 && bit != 4 {", "        if bit != 2 && bit != 3 {"),
    ("field25519", "pack keeps h when h - p was the answer", "        cswap_at(t, 0, 16, 1 - under);", "        cswap_at(t, 0, 16, under);"),
    ("field25519", "pack does one trial subtraction, not two", "    while pass < 2 {", "    while pass < 1 {"),
    ("field25519", "unpack keeps the top bit", "    o[15] = o[15] & 0x7fff;", "    o[15] = o[15] & 0xffff;"),
    ("field25519", "pow2523's exponent off", "        if bit != 1 {", "        if bit != 0 {"),
    ("x25519", "a24 as 121666", "        field25519.set_small(a24, 0xdb41);", "        field25519.set_small(a24, 0xdb42);"),
    ("x25519", "bit 254 not set by the clamp", "    if i == 254 {\n        return 1;\n    }", "    if i == 254 {\n        return int_of(k[31]) >> 6 & 1;\n    }"),
    ("x25519", "the low three bits not cleared", "    if i < 3 || i == 255 {", "    if i == 255 {"),
    ("x25519", "the final swap left out", "        field25519.cswap(x2, x3, swap);\n        field25519.cswap(z2, z3, swap);\n\n        field25519.invert", "        field25519.cswap(z2, z3, swap);\n\n        field25519.invert"),
    ("x25519", "the zero secret accepted", "    if diff == 0 {\n        return refused_zero_secret();", "    if diff == 256 {\n        return refused_zero_secret();"),
    ("x25519", "z3 not multiplied by x1", "            field25519.mul(z3, z3, x1, t);\n", ""),
    ("ed25519", "the ladder's swap back left out", "            point_copy(sum, acc);\n            point_cswap(acc, other, b);", "            point_copy(sum, acc);"),
    ("ed25519", "the square root's second candidate never tried", "                field25519.mul(cand, cand, sqrt_m1_limbs, w);", "                field25519.mul(cand, cand, d_limbs, w);"),
    ("ed25519", "the sign bit ignored on decompression", "                if field25519.parity(cand, w) != sign {", "                if field25519.parity(cand, w) != 0 {"),
    ("ed25519", "point_equal compares x only", "        same = xs & field25519.equal(a, b, w);", "        same = xs;"),
    ("ed25519", "x = 0 with the sign bit set accepted", "            if have_root == 1 && sign == 1 && field25519.equal(cand, zero, w) == 1 {", "            if have_root == 1 && sign == 2 && field25519.equal(cand, zero, w) == 1 {"),
    ("ed25519", "a signature of any length passed on", "    if len(pk) != 32 || len(sig) != 64 {", "    if len(pk) != 32 || len(sig) < 64 {"),
]


def evidence():
    cases, checks = [], []
    for line in open(os.path.join(ROOT, "tests/vectors/rfc7748.txt")):
        if line.startswith("#") or not line.strip():
            continue
        _, case, want = line.rstrip("\n").split(" | ")
        cases.append(case)
        checks.append(lambda a, w=want: a == f"0 ok {w}")
    wp = json.load(open(os.path.join(ROOT, "tests/vectors/wycheproof/x25519_test.json")))
    for g in wp["testGroups"]:
        for t in g["tests"]:
            cases.append(f"S {t['private']} {t['public']}")
            if set(t["shared"]) == {"0"}:
                checks.append(lambda a, w=t["shared"]: a == f"-2 x25519-zero-secret {w}")
            else:
                checks.append(lambda a, w=t["shared"]: a == f"0 ok {w}")
    ed = json.load(open(os.path.join(ROOT, "tests/vectors/wycheproof/ed25519_test.json")))
    for g in ed["testGroups"]:
        for t in g["tests"]:
            cases.append(f"V {g['publicKey']['pk']} {t['msg'] or '-'} {t['sig'] or '-'}")
            checks.append(lambda a, v=t["result"] == "valid": (a.split(" ")[0] == "1") == v)
    # tests/accept/ed25519.cho: three OpenSSL keypairs, their signatures, verify and a tampered signature.
    src = open(os.path.join(ROOT, "tests/accept/ed25519.cho")).read()
    sigs = re.findall(r"//~ STDOUT ([0-9a-f]{128})", src)
    calls = re.findall(r'check_one\(i, "([0-9a-f]{64})", "([^"]*)"\);', src)
    assert len(sigs) == 3 and len(calls) == 3
    for (seed, msg), sig in zip(calls, sigs):
        m = msg.encode().hex() or "-"
        cases.append(f"E {seed} {m}")
        checks.append(lambda a, w=sig: a == f"0 ok {w}")
    return cases, checks


def run(compiler, sources, driver_src, work, cases, checks):
    paths = []
    for name, text in sources.items():
        text = text.replace(f"module std.{name};", f"module {name};", 1).replace("import std.field25519;", "import field25519;")
        path = os.path.join(work, f"{name}.cho")
        open(path, "w").write(text)
        paths.append(path)
    drv = os.path.join(work, "driver.cho")
    exe = os.path.join(work, "driver")
    text = driver_src.replace("import std.x25519;", "import x25519;").replace("import std.ed25519;", "import ed25519;")
    open(drv, "w").write(text)
    build = subprocess.run([compiler, "build", "--std", drv, *paths, "-o", exe], capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    out = subprocess.run([exe], input="\n".join(cases) + "\n", capture_output=True, text=True, timeout=900)
    answers = [l.rstrip() for l in out.stdout.splitlines()]
    if out.returncode != 0 or len(answers) != len(cases):
        return True, [f"the driver stopped (status {out.returncode}) after {len(answers)} of {len(cases)} cases"]
    for i, (check, answer) in enumerate(zip(checks, answers)):
        if not check(answer):
            return True, [f"case {i}: {cases[i][:50]}"]
    diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/curve25519_differential.py"), exe, "300", "60"],
                          capture_output=True, text=True)
    if diff.returncode != 0:
        return True, ["the OpenSSL differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    sources = {n: open(os.path.join(ROOT, f"std/{n}.cho")).read() for n in ("field25519", "x25519", "ed25519")}
    driver_src = open(os.path.join(ROOT, "tests/programs/curve25519_driver.cho")).read()
    cases, checks = evidence()
    failed = 0
    with tempfile.TemporaryDirectory() as work:
        dead, why = run(compiler, sources, driver_src, work, cases, checks)
        assert dead is False, f"the unmutated files must pass: {why}"
        print(f"unmutated: passes {len(cases)} cases and the differential")
        for file, name, old, new in MUTANTS:
            n = sources[file].count(old)
            assert n == 1, f"{name}: occurs {n} times in {file}"
            mutated = dict(sources)
            mutated[file] = sources[file].replace(old, new)
            dead, why = run(compiler, mutated, driver_src, work, cases, checks)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {file}: {name}: {'; '.join(why)}")
            failed += dead is not True
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
