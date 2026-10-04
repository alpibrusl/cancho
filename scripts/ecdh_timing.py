#!/usr/bin/env python3
"""A dudect-style timing test of `std.ecdh` (docs/ecdh.md §3).

    lex-sys build --std [--backend B] tests/programs/ecdh_timing.ls -l tick -L . -o timing
    python3 scripts/ecdh_timing.py ./timing [<samples per test>]

`tick` is the cycle counter `scripts/gcm_timing.py` describes. Each test
times `ecdh.shared` over one peer point many times, the scalar from one of
two classes picked at random: a fixed scalar against a random one in
[1, n). Welch's t and dudect's crops are `scripts/gcm_timing.py`'s; |t|
below 4.5 is no evidence of a leak. Exit status 1 if any test reaches 4.5.

The tests, on P-256 and on P-384:
- scalar 1 (every window but the last selects the point at infinity)
  against random scalars;
- one random scalar, fixed, against random scalars.
"""
import os
import random
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gcm_timing import max_t  # noqa: E402

from cryptography.hazmat.primitives import serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import ec  # noqa: E402

CURVES = {256: (ec.SECP256R1(), 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551),
          384: (ec.SECP384R1(), 0xffffffffffffffffffffffffffffffffffffffffffffffffc7634d81f4372ddf581a0db248b0a77aecec196accc52973)}


def run(exe, curve, records):
    data = f"{curve}\n".encode() + b"".join(records).hex().encode()
    out = subprocess.run([exe], input=data, capture_output=True, check=True).stdout
    return [int(x) for x in out.decode().split()]


def main():
    exe = sys.argv[1]
    n = int(sys.argv[2]) if len(sys.argv) > 2 else 20000
    rng = random.Random(207)
    leaked = False
    for curve, (cv, order) in CURVES.items():
        size = curve // 8
        peer = ec.derive_private_key(rng.randrange(1, order), cv).public_key().public_bytes(
            serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint)
        fixed = rng.randrange(1, order)
        for name, first in (("scalar 1", 1), ("fixed scalar", fixed)):
            classes = [rng.randrange(2) for _ in range(n)]
            scalars = [first if c == 0 else rng.randrange(1, order) for c in classes]
            times = run(exe, curve, [s.to_bytes(size, "big") + peer for s in scalars])
            assert len(times) == n
            samples = list(zip(classes, times))[100:]
            t = max_t(samples)
            med = sorted(times)[len(times) // 2]
            leaked |= t >= 4.5
            print(f"P-{curve} {name:12} {n} samples, median {med} cycles, max |t| = {t:.2f}", flush=True)
    sys.exit(1 if leaked else 0)


if __name__ == "__main__":
    main()
