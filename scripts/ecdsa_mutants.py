#!/usr/bin/env python3
"""Mutation check of `std/ecdsa.cho` and `std.bigmod`'s registers (docs/ecdsa.md §5.3).

    python3 scripts/ecdsa_mutants.py <cancho binary>

Each mutant is `std/ecdsa.cho` or `std/bigmod.cho` with one deliberate bug. Both
are built as local modules (`ecdsa`, `bigmod`) beside a copy of
`tests/programs/ecdsa_driver.cho`, and run against what `conformance/ecdsa.rs`
runs: every case of the seven Wycheproof files, the NIST SigVer cases, and the
refusal rows. Then 2,000 rounds of `scripts/ecdsa_differential.py registers`
run against Python. A mutant is killed when any of them disagrees, or the
driver traps. The unmutated files are run first and must pass. Exit status 1
if a mutant survives or fails to build.
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    ("sub's borrow not added back", "bigmod", "    if under == 1 {\n        // Below zero", "    if under == 2 {\n        // Below zero"),
    ("add not reduced", "bigmod", "    w[dst + k] = carry;\n    if compare_n(w, dst, k) >= 0 {", "    w[dst + k] = carry;\n    if compare_n(w, dst, k) > 0 {"),
    ("Fermat with n - 1", "bigmod", "    var take = 2;", "    var take = 1;"),
    ("load_reduced without its subtraction", "bigmod", "    if compare_n(w, r, k) >= 0 {\n        subtract_n(w, r, k);\n    }", "    if compare_n(w, r, k) > 0 {\n        subtract_n(w, r, k);\n    }"),
    ("alpha = 2 (X - delta)(X + delta)", "ecdsa", "bigmod.add(w, t(4), t(3), t(5));", "bigmod.copy_reg(w, t(4), t(5));"),
    ("X3 = alpha^2 - 4 beta", "ecdsa", "bigmod.sub(w, t(6), t(8), t(9));", "bigmod.sub(w, t(6), t(7), t(9));"),
    ("Y3 with 4 gamma^2", "ecdsa", "    bigmod.add(w, t(12), t(12), t(12));\n    bigmod.sub(w, t(11), t(12), t(11));", "    bigmod.sub(w, t(11), t(12), t(11));"),
    ("equal x always doubles", "ecdsa", "        if bigmod.is_zero(w, t(7)) {\n            return double(w, p, d);\n        }\n        return bigmod.set_small(w, z_of(d), 0);",
     "        return double(w, p, d);"),
    ("an infinite left input ignored", "ecdsa", "    if bigmod.is_zero(w, z_of(p)) {\n        return copy_point(w, q, d);\n    }\n", ""),
    ("Z3 without H", "ecdsa", "    bigmod.mul(w, t(3), t(6), t(3));\n    bigmod.copy_reg(w, t(11), x_of(d));", "    bigmod.copy_reg(w, t(11), x_of(d));"),
    ("the ladder's lowest bit skipped", "ecdsa", "    while i >= 0 {\n                double(w, pt_r(), pt_r());", "    while i >= 1 {\n                double(w, pt_r(), pt_r());"),
    ("G + Q never used", "ecdsa", "add(w, pt_r(), pt_gq(), pt_r());", "add(w, pt_r(), pt_g(), pt_r());"),
    ("the curve check without b", "ecdsa", "bigmod.add(w, t(1), bigmod.reg(reg_b()), t(1));", "bigmod.copy_reg(w, t(1), t(1));"),
    ("x not divided by Z^2", "ecdsa", "            bigmod.inverse(w, t(0), t(0));\n            bigmod.mul(w, x_of(pt_r()), t(0), t(0));", "            bigmod.mul(w, x_of(pt_r()), t(0), t(0));"),
    ("u2 = e/s", "ecdsa", "    bigmod.to_mont(w, rr, rr);\n    bigmod.mul(w, rr, inv, u);", "    bigmod.to_mont(w, rr, rr);\n    bigmod.mul(w, er, inv, u);"),
    ("the digest truncated a byte short", "ecdsa", "    if m > curve / 8 {\n        m = curve / 8;", "    if m > curve / 8 - 1 {\n        m = curve / 8 - 1;"),
    ("r = 0 accepted", "ecdsa", "if bigmod.load_reg(r, w, rr) != 0 || bigmod.is_zero(w, rr) {", "if bigmod.load_reg(r, w, rr) != 0 {"),
    ("a negative DER integer accepted", "ecdsa", "    if a >= 0x80 {\n        return range;\n    }", ""),
    ("a non-minimal DER integer accepted", "ecdsa", "if a == 0 && b < 0x80 || a == 0xff && b >= 0x80 {", "if a == 0xff && b >= 0x80 {"),
    ("trailing bytes after s", "ecdsa", "if code == 0 && info[2] != end {", "if code == 0 && info[2] > end {"),
    ("a long-form length under 128 accepted", "ecdsa", "    if v < 0x80 {\n        return -37;\n    }", ""),
]


def or_dash(x):
    return x or "-"


H = {"SHA-256": 32, "SHA-384": 48, "SHA-512": 64}
WY = ["ecdsa_secp256r1_sha256_test.json", "ecdsa_secp256r1_sha256_p1363_test.json", "ecdsa_secp256r1_sha512_test.json",
      "ecdsa_secp384r1_sha384_test.json", "ecdsa_secp384r1_sha384_p1363_test.json", "ecdsa_secp384r1_sha256_test.json",
      "ecdsa_secp384r1_sha512_test.json"]


def evidence():
    cases, checks = [], []

    def add(case, check):
        cases.append(case)
        checks.append(check)

    for f in WY:
        for g in json.load(open(os.path.join(ROOT, "tests/vectors/wycheproof", f)))["testGroups"]:
            curve = {"secp256r1": 256, "secp384r1": 384}[g["publicKey"]["curve"]]
            op = "R" if g["type"] == "EcdsaP1363Verify" else "V"
            for t in g["tests"]:
                add(f"{op} {curve} {H[g['sha']]} {g['publicKey']['uncompressed']} {or_dash(t['msg'])} {or_dash(t['sig'])}",
                    lambda a, v=t["result"] == "valid": a.startswith("0 ok") == v)
    curve = h = size = 0
    fields = {}
    for line in open(os.path.join(ROOT, "tests/vectors/cavp/ECDSA_SigVer_186-3.rsp")):
        line = line.strip()
        if line.startswith("[P-"):
            c, sha = line[3:-1].split(",SHA-")
            curve, h, size = int(c), int(sha) // 8, int(c) // 8
        elif " = " in line:
            k, v = line.split(" = ", 1)
            fields[k] = v.rjust(2 * size, "0") if k in ("Qx", "Qy", "R", "S") else v
            if k == "Result":
                add(f"R {curve} {h} 04{fields['Qx']}{fields['Qy']} {fields['Msg']} {fields['R']}{fields['S']}",
                    lambda a, ok=v.startswith("P"): a.startswith("0 ok") == ok)
    g = json.load(open(os.path.join(ROOT, "tests/vectors/wycheproof/ecdsa_secp256r1_sha256_p1363_test.json")))["testGroups"][0]
    point = g["publicKey"]["uncompressed"]
    t = [t for t in g["tests"] if t["result"] == "valid"][0]
    add(f"R 256 32 {point} {t['msg']} {'00' * 32}{t['sig'][64:]}", lambda a: "ecdsa-r-range" in a)
    add(f"V 256 32 {point} {t['msg']} 3006020101020181", lambda a: "ecdsa-s-range" in a)
    add(f"V 256 32 {point} {t['msg']} 3008020200010202{'0001'}", lambda a: "ecdsa-der-non-minimal" in a)
    # A digest equal to n reduces to e = 0 (`conformance/ecdsa.rs` has the provenance).
    n = "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551"
    add(f"D 256 04e43536a661c7360d0f51bc8776c785682289e6594a550492b2f5af557ce73fd40d3d5b06fae511a128f0c7e4ef7ba418912e1d603e19bb2b72e1044877d76100 {n} 30460221009b27932cd0f703d408788b6f1538eae6db5bda4c9d1d0cfe8cd794e9b1c354ad022100b528c8277a637ad2c7d5b26e04394e10d6dccb9e99904ac089426db47637d012", lambda a: a.startswith("0 ok"))
    return cases, checks


def run(compiler, sources, driver_src, work, cases, checks):
    paths = []
    for name, text in sources.items():
        text = text.replace(f"module std.{name};", f"module {name};", 1).replace("import std.bigmod;", "import bigmod;")
        path = os.path.join(work, f"{name}.cho")
        open(path, "w").write(text)
        paths.append(path)
    drv, exe = os.path.join(work, "driver.cho"), os.path.join(work, "driver")
    text = driver_src
    for name in sources:
        text = text.replace(f"import std.{name};", f"import {name};", 1)
    open(drv, "w").write(text)
    build = subprocess.run([compiler, "build", "--std", "--backend", "llvm", drv, *paths, "-o", exe], capture_output=True, text=True)
    if build.returncode != 0:
        return None, build.stderr.strip().splitlines()[:3]
    out = subprocess.run([exe], input="\n".join(cases) + "\n", capture_output=True, text=True, timeout=900)
    answers = [l.rstrip() for l in out.stdout.splitlines()]
    if out.returncode != 0 or len(answers) != len(cases):
        return True, [f"the driver stopped (status {out.returncode}) after {len(answers)} of {len(cases)} cases"]
    for i, (check, answer) in enumerate(zip(checks, answers)):
        if not check(answer):
            return True, [f"case {i}: {cases[i][:24]}...: {answer[:40]}"]
    diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/ecdsa_differential.py"), exe, "registers", "2000"],
                          capture_output=True, text=True)
    if diff.returncode != 0:
        return True, ["the registers differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    sources = {n: open(os.path.join(ROOT, f"std/{n}.cho")).read() for n in ("bigmod", "ecdsa")}
    driver_src = open(os.path.join(ROOT, "tests/programs/ecdsa_driver.cho")).read()
    cases, checks = evidence()
    failed = 0
    with tempfile.TemporaryDirectory() as work:
        dead, why = run(compiler, sources, driver_src, work, cases, checks)
        assert dead is False, f"the unmutated files must pass: {why}"
        print(f"unmutated: passes {len(cases)} cases and the differential")
        for name, file, old, new in MUTANTS:
            count = sources[file].count(old)
            assert count == 1, f"{name}: `{old[:40]}` occurs {count} times in {file}"
            mutated = dict(sources)
            mutated[file] = sources[file].replace(old, new)
            dead, why = run(compiler, mutated, driver_src, work, cases, checks)
            verdict = {True: "killed", False: "SURVIVED", None: "DID NOT BUILD"}[dead]
            print(f"{verdict:13} {file}: {name}: {'; '.join(why)}", flush=True)
            failed += dead is not True
    print(f"{len(MUTANTS)} mutants, {len(MUTANTS) - failed} killed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
