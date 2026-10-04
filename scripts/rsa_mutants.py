#!/usr/bin/env python3
"""Mutation check of `std/bigmod.ls` and `std/rsa.ls` (docs/rsa.md §5.4).

    python3 scripts/rsa_mutants.py <lex-sys binary>

Each mutant is one of the two files with one deliberate bug. Both are built as
local modules (`bigmod`, `rsa`) beside a copy of `tests/programs/rsa_driver.ls`
and run against what `conformance/rsa.rs` runs: every supported Wycheproof
case, the NIST SigVer files, and the refusal rows. Then 2,000 rounds of
`scripts/rsa_differential.py pow` run against Python. A mutant is killed when
any of them disagrees, or the driver traps. The unmutated files are run first
and must pass. Exit status 1 if a mutant survives or fails to build.
"""
import json
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (name, file, the text replaced, its replacement). Each `old` must occur exactly once in its file.
MUTANTS = [
    ("n' not negated", "bigmod", "return (1 << 30) - inv & mask();", "return inv;"),
    ("too few Newton steps for n'", "bigmod", "    while i < 5 {\n        inv =", "    while i < 3 {\n        inv ="),
    ("the final subtraction only on a carry", "bigmod", "    if compare_n(w, t, k) >= 0 {\n        subtract_n(w, t, k);\n    }\n    copy(w, t, dst, k);",
     "    if w[t + k] != 0 {\n        subtract_n(w, t, k);\n    }\n    copy(w, t, dst, k);"),
    ("the accumulator's carry limb dropped", "bigmod", "w[t + k] = s >> 30;", "w[t + k] = 0;"),
    # Offsets are p % 30 for p = 8j, always even: `off > 23` would be the same test, so the mutants drop offset 24.
    ("a byte across two limbs loaded short", "bigmod", "if off > 22 && high != 0 {", "if off > 24 && high != 0 {"),
    ("a byte across two limbs stored short", "bigmod", "if off > 22 && idx + 1 < k {", "if off > 24 && idx + 1 < k {"),
    ("R^2 one doubling short", "bigmod", "var d = 30 * k + 1 - (bits - 1);", "var d = 30 * k - (bits - 1);"),
    ("the exponent's second bit skipped", "bigmod", "var b = eb - 2;", "var b = eb - 3;"),
    ("a = n accepted", "bigmod", "compare_n(work, slot_base(), k) >= 0", "compare_n(work, slot_base(), k) > 0"),
    ("an even modulus accepted", "bigmod", "    if int_of(n[len(n) - 1]) & 1 == 0 {\n        return -1;", "    if int_of(n[len(n) - 1]) & 1 == 2 {\n        return -1;"),
    ("a^0 as 0", "bigmod", "        work[acc] = 1;", "        work[acc] = 0;"),
    ("the DigestInfo's OID arc fixed at SHA-256", "rsa", "out[at + 14] = byte_of((hash_len - 16) / 16);", "out[at + 14] = byte_of(1);"),
    ("one FF short", "rsa", "while i < t - 1 {", "while i < t - 2 {"),
    ("the block's last byte not compared", "rsa", "while i < len(n) && code == 0 {\n                if m[i] != want[i] {",
     "while i < len(n) - 1 && code == 0 {\n                if m[i] != want[i] {"),
    ("an exponent as long as the modulus", "rsa", "ebits >= bits", "ebits > bits"),
    ("a short signature accepted", "rsa", "if len(sig) != (bits + 7) / 8 {", "if len(sig) > (bits + 7) / 8 {"),
    ("the PSS trailer misread", "rsa", "!= 0xbc {", "!= 0xbd {"),
    ("the PSS top bits not checked", "rsa", "if code == 0 && int_of(m[at]) >> (8 - top) != 0 {", "if code == 0 && int_of(m[at]) >> (8 - top) > 255 {"),
    ("the PSS separator may be 0", "rsa", "if code == 0 && int_of(db[ps]) != 1 {", "if code == 0 && int_of(db[ps]) > 1 {"),
    ("MGF1's counter from 1", "rsa", "var counter = 0;", "var counter = 1;"),
    ("M' with 7 zero bytes", "rsa", "mp[8 + i] = digest[i];", "mp[7 + i] = digest[i];"),
    ("the salt read one byte early", "rsa", "mp[8 + hash_len + i] = db[ps + 1 + i];", "mp[8 + hash_len + i] = db[ps + i];"),
    ("emLen one byte short allowed", "rsa", "em_len < hash_len + salt_len + 2", "em_len < hash_len + salt_len + 1"),
]


def or_dash(x):
    return x or "-"


H = {"SHA-256": 32, "SHA-384": 48, "SHA-512": 64, "SHA256": 32, "SHA384": 48, "SHA512": 64}


def evidence():
    cases, checks = [], []

    def add(case, check):
        cases.append(case)
        checks.append(check)

    wy = os.path.join(ROOT, "tests/vectors/wycheproof")
    for f in sorted(os.listdir(wy)):
        if not f.startswith("rsa_"):
            continue
        for g in json.load(open(os.path.join(wy, f)))["testGroups"]:
            n, e = g["publicKey"]["modulus"], g["publicKey"]["publicExponent"]
            h = H.get(g["sha"])
            pss = g["type"] == "RsassaPssVerify"
            mgf = H.get(g["mgfSha"]) if pss else h
            if h is None or mgf is None:
                continue
            for t in g["tests"]:
                msg, sig = or_dash(t["msg"]), or_dash(t["sig"])
                case = f"S {h} {mgf} {g['sLen']} {n} {e} {msg} {sig}" if pss else f"P {h} {n} {e} {msg} {sig}"
                add(case, lambda a, v=t["result"] == "valid": a.startswith("0 ok") == v)
    for f, pss in (("SigVer15_186-3.rsp", False), ("SigVerPSS_186-3.rsp", True)):
        modulus, fields = 0, {}
        for line in open(os.path.join(ROOT, "tests/vectors/cavp", f)):
            line = line.strip()
            if line.startswith("[mod = "):
                modulus = int(line[7:-1])
            elif " = " in line:
                k, v = line.split(" = ", 1)
                fields[k] = v
                if k == "Result":
                    h, e = H[fields["SHAAlg"]], fields["e"]
                    e = "0" + e if len(e) % 2 else e
                    if pss:
                        salt = 0 if fields["SaltVal"] == "00" else len(fields["SaltVal"]) // 2
                        case = f"S {h} {h} {salt} {fields['n']} {e} {fields['Msg']} {fields['S']}"
                    else:
                        case = f"P {h} {fields['n']} {e} {fields['Msg']} {fields['S']}"
                    if modulus < 2048:
                        add(case, lambda a: a.startswith("-10 rsa-modulus-size"))
                    else:
                        add(case, lambda a, v=v.startswith("P"): a.startswith("0 ok") == v)
    for case, tag in (("M 10 03 01", "bigmod-even-modulus"), ("M 0b 03 0b", "bigmod-not-reduced"), ("M 0b 00 05", "ok 01")):
        add(case, lambda a, t=tag: t in a)
    g = json.load(open(os.path.join(wy, "rsa_signature_2048_sha256_test.json")))["testGroups"][0]
    n, e = g["publicKey"]["modulus"], g["publicKey"]["publicExponent"]
    t = [t for t in g["tests"] if t["result"] == "valid"][0]
    add(f"P 32 {n} {n} {t['msg']} {t['sig']}", lambda a: "rsa-exponent" in a)
    add(f"P 32 {n} {e} {t['msg']} {t['sig'][2:]}", lambda a: "rsa-signature-length" in a)
    add(f"S 32 32 223 {n} {e} {t['msg']} {t['sig']}", lambda a: "rsa-pss-length" in a)
    return cases, checks


def run(compiler, sources, driver_src, work, cases, checks):
    paths = []
    for name, text in sources.items():
        text = text.replace(f"module std.{name};", f"module {name};", 1).replace("import std.bigmod;", "import bigmod;")
        path = os.path.join(work, f"{name}.ls")
        open(path, "w").write(text)
        paths.append(path)
    drv, exe = os.path.join(work, "driver.ls"), os.path.join(work, "driver")
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
            return True, [f"case {i}: {cases[i][:30]}...: {answer[:40]}"]
    diff = subprocess.run([sys.executable, os.path.join(ROOT, "scripts/rsa_differential.py"), exe, "pow", "2000"],
                          capture_output=True, text=True)
    if diff.returncode != 0:
        return True, ["the pow_mod differential"]
    return False, []


def main():
    compiler = os.path.abspath(sys.argv[1])
    sources = {n: open(os.path.join(ROOT, f"std/{n}.ls")).read() for n in ("bigmod", "rsa")}
    driver_src = open(os.path.join(ROOT, "tests/programs/rsa_driver.ls")).read()
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
