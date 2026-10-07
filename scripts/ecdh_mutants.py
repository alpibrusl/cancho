#!/usr/bin/env python3
"""Mutation check of `std/ecdh.cho` and `std/bigmod.cho`'s constant-time
reduction (docs/ecdh.md §5).

    python3 scripts/ecdh_mutants.py <cancho binary>

Each mutant is one of the two files with one deliberate bug, both built as
local modules `ecdh` and `bigmod` beside a copy of
`tests/programs/ecdh_driver.cho` and `tests/programs/bigmod_driver.cho`, and
run against the evidence `conformance/ecdh.rs` and
`scripts/ecdh_differential.py` use: Wycheproof's P-256 and P-384 cases,
NIST's KAS validity cases, the scalar's edges and the refusals, modular
powers at moduli a multiple of 30 bits long against Python's `pow`, then
100 random key pairs a curve against OpenSSL. A mutant is
killed when any of them disagrees. The unmutated files are run first and
must pass. Exit status 1 if a mutant survives or fails to build.
"""
import json
import os
import random
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

DOUBLE = "        double_point(w, pt_acc(), pt_acc());\n"

# (file, name, the text replaced, its replacement). Each `old` must occur
# exactly once in its file.
MUTANTS = [
    ("ecdh", "b times the wrong product in the addition", "bigmod.mul(w, b, t(2), z3);", "bigmod.mul(w, b, t(1), z3);"),
    ("ecdh", "the doubling's Z one factor of 2 short", "    bigmod.add(w, z3, z3, z3);\n    bigmod.add(w, z3, z3, z3);\n    return", "    bigmod.add(w, z3, z3, z3);\n    return"),
    ("ecdh", "three doublings a window, not four", DOUBLE * 4, DOUBLE * 3),
    ("ecdh", "the windows of a byte in the wrong order", "let shift = 4 - 4 * (at % 2);", "let shift = 4 * (at % 2);"),
    ("ecdh", "a select that matches two indices", "(a ^ b) - 1 >> 63", "(a ^ b) - 2 >> 63"),
    ("ecdh", "the table doubled, not stepped", "add_points(w, pt_table(i - 1), pt_table(1), pt_table(i));", "add_points(w, pt_table(i - 1), pt_table(i - 1), pt_table(i));"),
    ("ecdh", "infinity with Y = 0", "bigmod.set_small(w, y_of(pt_table(0)), 1);", "bigmod.set_small(w, y_of(pt_table(0)), 0);"),
    ("ecdh", "the on-curve check skipped", "if !on_curve(work) {", "if !on_curve(work) && false {"),
    ("ecdh", "the curve equation with -2x, not -3x", "    bigmod.add(w, t(2), x, t(2));\n", ""),
    ("ecdh", "every scalar accepted", "return under & (0 - any >> 63 & 1);", "return 1;"),
    ("ecdh", "the scalar compared the wrong way", "int_of(s[i]) - int_of(n[i]) - under", "int_of(n[i]) - int_of(s[i]) - under"),
    ("ecdh", "the public key's y from X", "bigmod.mul(w, y_of(a), t(0), t(1));", "bigmod.mul(w, x_of(a), t(0), t(1));"),
    ("ecdh", "a point's first byte not checked", "if len(peer) != 1 + 2 * size || int_of(peer[0]) != 4 {", "if len(peer) != 1 + 2 * size {"),
    ("bigmod", "the reduction's mask inverted", "let m = value_barrier(under - 1);", "let m = value_barrier(0 - under);"),
    ("bigmod", "the reduction blind to the carry limb", "under = w[x + k] - under >> 63 & 1;", "under = 0 - under >> 63 & 1;"),
    ("bigmod", "a negative difference left unreduced", "let m = value_barrier(0 - under);\n    var carry", "let m = 0;\n    var carry"),
]


def kas():
    cases, checks, curve, f = [], [], 0, {}
    for line in open(os.path.join(ROOT, "tests/vectors/cavp/KASValidityTest_ECCStaticUnified_NOKC_ZZOnly_resp.fax")):
        line = line.strip()
        if line.startswith("[EC - "):
            curve = 256
        elif line.startswith("[ED - "):
            curve = 384
        elif " = " in line:
            k, v = line.split(" = ", 1)
            f[k] = v
            if k == "Result":
                reason = int(v[3:].split(" ")[0])
                iut = f"0 ok 04{f['QsIUTx']}{f['QsIUTy']}"
                cases.append(f"K {curve} {f['dsIUT']}")
                checks.append((lambda a, iut=iut: a != iut) if reason in (5, 6, 7) else (lambda a, iut=iut: a == iut))
                cases.append(f"S {curve} {f['dsIUT']} 04{f['QsCAVSx']}{f['QsCAVSy']}")
                z = f"0 ok {f['Z']}"
                if v[0] == "P":
                    checks.append(lambda a, z=z: a == z)
                elif reason in (1, 2):
                    checks.append(lambda a: a.startswith("-56 "))
                elif reason == 8:
                    checks.append(lambda a, z=z: a.startswith("0 ") and a != z)
                else:
                    checks.append(lambda a: True)
    return cases, checks


def evidence():
    cases, checks = kas()
    for curve in (256, 384):
        size = curve // 8
        wp = json.load(open(os.path.join(ROOT, f"tests/vectors/wycheproof/ecdh_secp{curve}r1_ecpoint_test.json")))
        for group in wp["testGroups"]:
            for t in group["tests"]:
                d = int(t["private"], 16).to_bytes(size, "big").hex()
                cases.append(f"S {curve} {d} {t['public'] or '-'}")
                if t["result"] == "valid":
                    checks.append(lambda a, s=t["shared"]: a == f"0 ok {s}")
                else:
                    checks.append(lambda a: a.startswith("-"))
    for curve, n in [(256, "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551"),
                     (384, "ffffffffffffffffffffffffffffffffffffffffffffffffc7634d81f4372ddf581a0db248b0a77aecec196accc52973")]:
        for bad in ("00" * (curve // 8), n):
            cases.append(f"K {curve} {bad}")
            checks.append(lambda a: a.startswith("-53 "))
    g = "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c2964fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5"
    for first in ("00", "02", "03", "05"):
        cases.append(f"S 256 {'07' * 32} {first}{g}")
        checks.append(lambda a: a.startswith("-54 "))
    return cases, checks


def powers():
    """a^e mod n at moduli of 30 to 330 bits, each a multiple of 30 and
    near its top, where the reduction's carry limb is reached."""
    rng = random.Random(30)
    cases, wants = [], []
    for limbs in range(1, 12):
        bits = 30 * limbs
        size = (bits + 7) // 8
        for _ in range(20):
            n = rng.randrange(2 ** (bits - 1), 2 ** bits) | 1 | (2 ** bits - 2 ** (bits - 8))
            a, e = rng.randrange(n), rng.randrange(1, 2 ** 64)
            cases.append(f"{n.to_bytes(size, 'big').hex()} {e.to_bytes(8, 'big').hex()} {a.to_bytes(size, 'big').hex()}")
            wants.append("0 " + pow(a, e, n).to_bytes(size, "big").hex())
    return cases, wants


def killed(compiler, sources, driver_src, work, cases, checks):
    files = {
        "bigmod.cho": sources["bigmod"].replace("module std.bigmod;", "module bigmod;", 1),
        "ecdh.cho": sources["ecdh"].replace("module std.ecdh;", "module ecdh;", 1).replace("import std.bigmod;", "import bigmod;", 1),
        "driver.cho": driver_src.replace("import std.ecdh;", "import ecdh;", 1),
        "powers.cho": open(os.path.join(ROOT, "tests/programs/bigmod_driver.cho")).read().replace("import std.bigmod;", "import bigmod;", 1),
    }
    for name, text in files.items():
        open(os.path.join(work, name), "w").write(text)
    pexe = os.path.join(work, "powers")
    build = subprocess.run([compiler, "build", "--std", os.path.join(work, "powers.cho"), os.path.join(work, "bigmod.cho"), "-o", pexe],
                           capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    pcases, pwants = powers()
    got = subprocess.run([pexe], input="\n".join(pcases) + "\n", capture_output=True, text=True, timeout=600).stdout.splitlines()
    for i, (want, line) in enumerate(zip(pwants, got + [""] * len(pwants))):
        if line.rstrip() != want:
            return True, [f"power {i}: {pcases[i][:60]}"]
    exe = os.path.join(work, "driver")
    build = subprocess.run([compiler, "build", "--std"] + [os.path.join(work, n) for n in ["driver.cho", "ecdh.cho", "bigmod.cho"]] + ["-o", exe],
                           capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    run = subprocess.run([exe], input="\n".join(cases) + "\n", capture_output=True, text=True, timeout=600)
    answers = [a.rstrip() for a in run.stdout.splitlines()]
    if run.returncode != 0 or len(answers) != len(cases):
        return True, ["the driver failed or stopped early"]
    for i, (check, answer) in enumerate(zip(checks, answers)):
        if not check(answer):
            return True, [f"case {i}: {cases[i][:60]}"]
    diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/ecdh_differential.py"), exe, "100"], capture_output=True, text=True)
    if diff.returncode != 0:
        return True, ["the OpenSSL differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    sources = {m: open(os.path.join(ROOT, f"std/{m}.cho")).read() for m in ("ecdh", "bigmod")}
    driver_src = open(os.path.join(ROOT, "tests/programs/ecdh_driver.cho")).read()
    cases, checks = evidence()
    failed = 0
    with tempfile.TemporaryDirectory() as work:
        dead, why = killed(compiler, sources, driver_src, work, cases, checks)
        assert dead is False, f"the unmutated files must pass: {why}"
        print(f"unmutated: passes {len(cases)} cases and the differential")
        for module, name, old, new in MUTANTS:
            assert sources[module].count(old) == 1, f"{name}: `{old}` occurs {sources[module].count(old)} times"
            mutated = dict(sources)
            mutated[module] = sources[module].replace(old, new)
            dead, why = killed(compiler, mutated, driver_src, work, cases, checks)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {module}: {name}: {'; '.join(why)}", flush=True)
            failed += dead is not True
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
