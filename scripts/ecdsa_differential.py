#!/usr/bin/env python3
"""Differential tests of `std.ecdsa` and `std.bigmod`'s registers (docs/ecdsa.md §4).

    python3 scripts/ecdsa_differential.py <driver> registers [<count>]
    python3 scripts/ecdsa_differential.py <driver> openssl [<count per curve>]

`driver` is `tests/programs/ecdsa_driver.ls` built with `lex-sys build --std`.

`registers` (default 100,000): `mul`, `add`, `sub`, `inverse` and
`load_reduced` modulo P-256's and P-384's p and n against Python's integers.
Operands are random below the modulus, with 0, 1 and n - 1 drawn often;
`load_reduced`'s are below 2n, with n itself drawn often.

`openssl` (default 1,000 a curve): for P-256 with SHA-256 and P-384 with
SHA-384, three keys a curve made by `openssl genpkey`. Each round signs a
random message with `openssl dgst -sign` (DER), then verifies it with
`openssl dgst -verify` and with `std.ecdsa`, as made and with one random bit
of the DER signature flipped. The two must agree every time, and both must
accept every unflipped signature.

Exit status 1 on any difference.
"""
import os
import random
import subprocess
import sys
import tempfile

CURVES = {
    256: (0xFFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF,
          0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551),
    384: (int("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFF0000000000000000FFFFFFFF", 16),
          int("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC7634D81F4372DDF581A0DB248B0A77AECEC196ACCC52973", 16)),
}


def run(driver, cases):
    out = subprocess.run([driver], input="\n".join(cases) + "\n", capture_output=True, text=True, check=True)
    lines = out.stdout.splitlines()
    assert len(lines) == len(cases), (len(lines), len(cases))
    return lines


def registers(driver, count, rng):
    moduli = [m for pair in CURVES.values() for m in pair]
    bad = done = 0
    while done < count:
        cases, want = [], []
        for _ in range(min(10000, count - done)):
            n = rng.choice(moduli)
            size = (n.bit_length() + 7) // 8
            pick = lambda: rng.choice([0, 1, n - 1, rng.randrange(n), rng.randrange(n), rng.randrange(n)])
            a, b, op = pick(), pick(), rng.choice("masir")
            if op == "i" and a == 0:
                a = 1
            if op == "r":
                # `load_reduced` takes anything below 2n that fits n's bytes.
                top = min(2 * n, 1 << (8 * size))
                a = rng.choice([n, n + 1, top - 1, rng.randrange(top), rng.randrange(n)])
            r = {"m": lambda: a * b % n, "a": lambda: (a + b) % n, "s": lambda: (a - b) % n, "i": lambda: pow(a, -1, n),
                 "r": lambda: a % n}[op]()
            cases.append(f"F {n:0{2 * size}x} {op} {a:0{2 * size}x} {b:0{2 * size}x}")
            want.append(f"0 ok {r:0{2 * size}x}")
        for c, g, w in zip(cases, run(driver, cases), want):
            if g != w:
                bad += 1
                if bad < 5:
                    print(f"{c}\n  got  {g}\n  want {w}")
        done += len(cases)
    print(f"registers: {count} operations, {bad} differences")
    return bad


def openssl(driver, count, rng):
    from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat, load_pem_private_key

    tmp = tempfile.mkdtemp()
    msg_path, sig_path = os.path.join(tmp, "msg"), os.path.join(tmp, "sig")
    cases, theirs = [], []
    for curve, name, dgst, h in ((256, "P-256", "-sha256", 32), (384, "P-384", "-sha384", 48)):
        keys = []
        for j in range(3):
            path = os.path.join(tmp, f"{name}_{j}.pem")
            subprocess.run(["openssl", "genpkey", "-algorithm", "EC", "-pkeyopt", f"ec_paramgen_curve:{name}", "-out", path],
                           check=True, capture_output=True)
            pub = path + ".pub"
            subprocess.run(["openssl", "pkey", "-in", path, "-pubout", "-out", pub], check=True, capture_output=True)
            point = load_pem_private_key(open(path, "rb").read(), None).public_key().public_bytes(
                Encoding.X962, PublicFormat.UncompressedPoint).hex()
            keys.append((path, pub, point))
        for _ in range(count):
            path, pub, point = rng.choice(keys)
            msg = rng.randbytes(rng.randint(0, 300))
            open(msg_path, "wb").write(msg)
            sig = subprocess.run(["openssl", "dgst", dgst, "-sign", path, msg_path], capture_output=True, check=True).stdout
            bad = bytearray(sig)
            bit = rng.randrange(len(bad) * 8)
            bad[bit // 8] ^= 1 << (bit % 8)
            for s in (sig, bytes(bad)):
                open(sig_path, "wb").write(s)
                r = subprocess.run(["openssl", "dgst", dgst, "-verify", pub, "-signature", sig_path, msg_path], capture_output=True)
                theirs.append(r.returncode == 0)
                cases.append(f"V {curve} {h} {point} {msg.hex() or '-'} {s.hex()}")
    bad = 0
    accepted = 0
    for i, (line, t) in enumerate(zip(run(driver, cases), theirs)):
        ours = line.startswith("0 ok")
        accepted += t
        if ours != t or (i % 2 == 0 and not t):
            bad += 1
            print(f"case {i} ({'as made' if i % 2 == 0 else 'one bit flipped'}): openssl {t}, std.ecdsa {line}")
    print(f"openssl: {count} signatures a curve and each flipped; openssl accepted {accepted} of {len(cases)}; {bad} differences")
    return bad


def main():
    driver, mode = sys.argv[1], sys.argv[2]
    rng = random.Random(204)
    if mode == "registers":
        bad = registers(driver, int(sys.argv[3]) if len(sys.argv) > 3 else 100000, rng)
    else:
        bad = openssl(driver, int(sys.argv[3]) if len(sys.argv) > 3 else 1000, rng)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
