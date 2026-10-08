#!/usr/bin/env python3
"""A dudect-style timing test of `std.ecdh` (docs/ecdh.md §3).

    cancho build --std [--backend B] tests/programs/ecdh_timing.cho -l tick -L . -o timing
    python3 scripts/ecdh_timing.py ./timing [<samples per test>] [--curves=256] [--public-key]

`tick` is the cycle counter `scripts/gcm_timing.py` describes. Each test
times `ecdh.shared` over one peer point many times, the scalar from one of
two classes picked at random: a fixed scalar against a random one in
[1, n). Welch's t and dudect's crops are `scripts/gcm_timing.py`'s; |t|
below 4.5 is no evidence of a leak. Exit status 1 if any test reaches 4.5. With
`--public-key` the call timed is `ecdh.public_key` (the fixed-base table,
docs/p256-fast.md §6), not `ecdh.shared`; `--curves=256` runs P-256 only.
Samples are taken in batches of 50,000, so 10^6 a test is possible.

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


def run(exe, curve, records, public=False):
    data = f"{curve}{' k' if public else ''}\n".encode() + b"".join(records).hex().encode()
    out = subprocess.run([exe], input=data, capture_output=True, check=True).stdout
    return [int(x) for x in out.decode().split()]


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    public = "--public-key" in sys.argv
    only = [int(c) for a in sys.argv[1:] if a.startswith("--curves=") for c in a[9:].split(",")]
    exe = args[0]
    n = int(args[1]) if len(args) > 1 else 20000
    batch = 50000
    rng = random.Random(207)
    leaked = False
    for curve, (cv, order) in CURVES.items():
        if only and curve not in only:
            continue
        size = curve // 8
        peer = ec.derive_private_key(rng.randrange(1, order), cv).public_key().public_bytes(
            serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint)
        fixed = rng.randrange(1, order)
        tests = [("scalar 1", 1), ("fixed scalar", fixed)]
        if public:
            # The table's other extremes: every digit 8 (the largest magnitude), and n - 1
            # (every digit negative or zero after the carries).
            tests += [("scalar 0x88..", int("88" * size, 16)), ("scalar n - 1", order - 1)]
        for name, first in tests:
            # Samples are taken in batches, each one run of the program, the classes
            # interleaved within a batch (a batch of 10^6 would be 100 MB of hex).
            samples = []
            while len(samples) < n:
                m = min(batch, n - len(samples) + 100)
                classes = [rng.randrange(2) for _ in range(m)]
                scalars = [first if c == 0 else rng.randrange(1, order) for c in classes]
                times = run(exe, curve, [s.to_bytes(size, "big") + peer for s in scalars], public)
                assert len(times) == m
                # The first calls of each run warm the caches.
                samples += list(zip(classes, times))[100:]
            t = max_t(samples)
            med = sorted(x for _, x in samples)[len(samples) // 2]
            leaked |= t >= 4.5
            what = "public_key" if public else "shared"
            print(f"P-{curve} {what} {name:12} {len(samples)} samples, median {med} cycles, max |t| = {t:.2f}", flush=True)
    sys.exit(1 if leaked else 0)


if __name__ == "__main__":
    main()
