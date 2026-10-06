#!/usr/bin/env python3
"""Verifications per second of `std.ecdsa` (docs/ecdsa.md §5.4).

    cancho build --std [--backend llvm] tests/programs/ecdsa_driver.cho -o ecdsa_driver
    python3 scripts/ecdsa_bench.py ecdsa_driver

For P-256 with SHA-256 and P-384 with SHA-384, a DER signature is made with
pyca/cryptography, and the driver's `T <rounds>` op verifies it 1 and `rounds`
times. The time per verification is the difference divided by `rounds - 1`,
the best of five runs, so process start and reading the input cancel out.
"""
import subprocess
import sys
import time

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat


def best(driver, line, rounds):
    data = f"T {rounds} {line}\n"
    t = 1e9
    for _ in range(5):
        s = time.perf_counter()
        out = subprocess.run([driver], input=data, capture_output=True, text=True, check=True).stdout
        t = min(t, time.perf_counter() - s)
        assert out.startswith("0 ok"), out
    return t


def main():
    driver = sys.argv[1]
    msg = b"cancho ecdsa bench"
    for curve, cls, h, alg, rounds in ((256, ec.SECP256R1, 32, hashes.SHA256(), 1001), (384, ec.SECP384R1, 48, hashes.SHA384(), 501)):
        key = ec.generate_private_key(cls())
        point = key.public_key().public_bytes(Encoding.X962, PublicFormat.UncompressedPoint).hex()
        sig = key.sign(msg, ec.ECDSA(alg)).hex()
        line = f"V {curve} {h} {point} {msg.hex()} {sig}"
        each = (best(driver, line, rounds) - best(driver, line, 1)) / (rounds - 1)
        print(f"P-{curve}: {each * 1e6:.0f} us per verification, {1 / each:.0f} per second")


if __name__ == "__main__":
    main()
