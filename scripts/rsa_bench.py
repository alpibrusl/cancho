#!/usr/bin/env python3
"""Verifications per second of `std.rsa` (docs/rsa.md §5.5).

    cancho build --std [--backend llvm] tests/programs/rsa_driver.cho -o rsa_driver
    python3 scripts/rsa_bench.py rsa_driver

For each of RSA-2048 and RSA-4096, PKCS#1 v1.5 and PSS (SHA-256, e = 65537),
a valid signature is made with pyca/cryptography, and the driver's
`T <rounds>` op verifies it 1 and `rounds` times. The time per verification
is the difference divided by `rounds - 1`, the best of five runs, so process
start and reading the input cancel out. It includes decoding the case's hex
each round, which is under 1% of a verification.
"""
import subprocess
import sys
import time

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding, rsa


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
    msg = b"cancho rsa bench"
    for bits, rounds in ((2048, 2001), (4096, 501)):
        key = rsa.generate_private_key(65537, bits)
        pub = key.public_key().public_numbers()
        n = pub.n.to_bytes(bits // 8, "big").hex()
        for name, pad, op in (
            ("pkcs1", padding.PKCS1v15(), "P 32"),
            ("pss", padding.PSS(padding.MGF1(hashes.SHA256()), 32), "S 32 32 32"),
        ):
            sig = key.sign(msg, pad, hashes.SHA256()).hex()
            line = f"{op} {n} 010001 {msg.hex()} {sig}"
            each = (best(driver, line, rounds) - best(driver, line, 1)) / (rounds - 1)
            print(f"RSA-{bits} {name}: {each * 1e6:.0f} us per verification, {1 / each:.0f} per second")


if __name__ == "__main__":
    main()
