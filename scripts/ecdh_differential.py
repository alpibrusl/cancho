#!/usr/bin/env python3
"""Differential test of `std.ecdh` against OpenSSL (docs/ecdh.md §4.3).

    python3 scripts/ecdh_differential.py <driver> [<count per curve>]

`driver` is `tests/programs/ecdh_driver.ls` built with `lex-sys build --std`.
OpenSSL is reached through pyca/cryptography (`pip install cryptography`).

For each curve, P-256 and P-384, and each of `count` (default 1,000) random
key pairs: the public key of a random scalar, and the shared secret of that
scalar with another random key's public point, must both equal OpenSSL's.
One case in ten uses a scalar with many leading zero bits or ones, and every
tenth case also sends a random (x, y), almost never on the curve, which must
be refused. Exit status 1 on any difference.
"""
import random
import subprocess
import sys

from cryptography.hazmat.backends.openssl.backend import backend
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ec

CURVES = {256: (ec.SECP256R1(), 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551),
          384: (ec.SECP384R1(), 0xffffffffffffffffffffffffffffffffffffffffffffffffc7634d81f4372ddf581a0db248b0a77aecec196accc52973)}


def point(key):
    return key.public_key().public_bytes(serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint)


def main():
    driver = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 1000
    rng = random.Random(3)
    cases, checks = [], []
    for bits, (curve, n) in CURVES.items():
        size = bits // 8
        for i in range(count):
            if i % 10 == 3:
                d = rng.randrange(1, 1 << rng.randrange(1, 40))
            elif i % 10 == 7:
                d = n - rng.randrange(1, 1 << 40)
            else:
                d = rng.randrange(1, n)
            mine = ec.derive_private_key(d, curve)
            theirs = ec.derive_private_key(rng.randrange(1, n), curve)
            scalar = d.to_bytes(size, "big").hex()
            cases.append(f"K {bits} {scalar}")
            checks.append(("public", bits, "0 ok " + point(mine).hex()))
            cases.append(f"S {bits} {scalar} {point(theirs).hex()}")
            checks.append(("shared", bits, "0 ok " + mine.exchange(ec.ECDH(), theirs.public_key()).hex()))
            if i % 10 == 0:
                cases.append(f"S {bits} {scalar} 04{rng.randbytes(2 * size).hex()}")
                checks.append(("off the curve", bits, None))
    out = subprocess.run([driver], input=("\n".join(cases) + "\n").encode(), capture_output=True, check=True).stdout
    lines = out.decode().splitlines()
    assert len(lines) == len(cases), f"{len(lines)} answers to {len(cases)} cases"
    bad = 0
    for (kind, bits, want), line in zip(checks, lines):
        line = line.rstrip()
        ok = line == want if want else (line.startswith("-56 ") or line.startswith("-55 "))
        if not ok:
            bad += 1
            if bad <= 10:
                print(f"DIFFERENT P-{bits} {kind}: {line[:100]}")
    print(f"{backend.openssl_version_text()}: {count} key pairs a curve, {len(cases)} checks, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
