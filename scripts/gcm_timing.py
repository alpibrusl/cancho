#!/usr/bin/env python3
"""A dudect-style timing test of `std.gcm` (docs/tls-parity.md §3.1), and with
`--chacha20` of `std.chacha20`'s AEAD (docs/tls-assurance.md §6).

    cc -O2 -c tick.c -o tick.o && ar rcs libtick.a tick.o   # tick.c below
    cancho build --std [--backend B] tests/programs/gcm_timing.cho -l tick -L . -o timing
    python3 scripts/gcm_timing.py ./timing [<samples per test>] [--chacha20] [--size <bytes>]

`tick.c` is the cycle counter the program reads:

    #include <x86intrin.h>
    long cancho_tick(void) { unsigned aux; return (long)__rdtscp(&aux); }

or on aarch64, the generic timer, whose rate `cntfrq_el0` gives (1 GHz,
measured, on an Apple M4 Max under macOS 26):

    long cancho_tick(void) { long v; __asm__ volatile("isb\n\tmrs %0, cntvct_el0" : "=r"(v)); return v; }

Every input is generated and read before anything is timed: decoding
input just before a timed call, with branches on what it decodes, was
found to give a large t for a loop of plain XORs on the Cranelift
backend. Each test times one call many times, each sample from one of two classes
picked at random (Reparaz, Balasch and Verbauwhede, "Dude, is my code
constant time?", 2017): a fixed input, all zero bytes, against a random
one. Welch's t is computed over all samples and over the samples below
each of dudect's percentile crops; the largest |t| is reported. Below
4.5 is no evidence of a leak; dudect calls above 10 a definite one.
Exit status 1 if any test reaches 4.5.

The tests, each over a 64-byte message and 13 bytes of associated data
(a TLS record's header length):
- seal, key: a fixed key against a random one;
- seal, data: one key, a fixed message and associated data against
  random ones;
- open, data: one key, a fixed sealed message against random ones, both
  refused for their tag;
- open, tag: one key and ciphertext, a tag wrong in its first byte
  against one wrong in its last, both refused: the comparison is
  over every byte.
"""
import math
import random
import subprocess
import sys

SIZE = 64
AAD = 13


def welch(a, b):
    na, nb = len(a), len(b)
    if na < 2 or nb < 2:
        return 0.0
    ma, mb = sum(a) / na, sum(b) / nb
    va = sum((x - ma) ** 2 for x in a) / (na - 1)
    vb = sum((x - mb) ** 2 for x in b) / (nb - 1)
    if va + vb == 0:
        return 0.0
    return (ma - mb) / math.sqrt(va / na + vb / nb)


def max_t(samples):
    """dudect's statistic: Welch's t over all samples and over each crop."""
    times = sorted(t for _, t in samples)
    crops = [None] + [times[int((1 - 0.5 ** (10 * (i + 1) / 100)) * (len(times) - 1))] for i in range(100)]
    worst = 0.0
    for crop in crops:
        a = [t for c, t in samples if c == 0 and (crop is None or t < crop)]
        b = [t for c, t in samples if c == 1 and (crop is None or t < crop)]
        worst = max(worst, abs(welch(a, b)))
    return worst


def run(exe, op, records):
    """Times one call per record, each a (key, nonce, aad, data) tuple."""
    k, n, a, d = (len(x) for x in records[0])
    data = f"{op} {k} {n} {a} {d}\n".encode() + b"".join(b"".join(r) for r in records).hex().encode()
    out = subprocess.run([exe], input=data, capture_output=True, check=True).stdout
    return [int(x) for x in out.decode().split()]


def main():
    global SIZE
    chacha = "--chacha20" in sys.argv
    args = [a for a in sys.argv[1:] if a != "--chacha20"]
    if "--size" in args:
        at = args.index("--size")
        SIZE = int(args[at + 1])
        del args[at:at + 2]
    exe = args[0]
    n = int(args[1]) if len(args) > 1 else 100000
    # ChaCha20-Poly1305's ops are the lowercase ones, and its key is 32 bytes.
    klen = 32 if chacha else 16
    seal, open_ = ("s", "o") if chacha else ("S", "O")
    rng = random.Random(197)
    zero = bytes(SIZE)
    key = rng.randbytes(klen)
    nonce = rng.randbytes(12)
    ct = rng.randbytes(SIZE)
    tests = {
        "seal, key": (seal, lambda: (bytes(klen), nonce, bytes(AAD), zero),
                      lambda: (rng.randbytes(klen), nonce, bytes(AAD), zero)),
        "seal, data": (seal, lambda: (key, nonce, bytes(AAD), zero),
                       lambda: (key, nonce, rng.randbytes(AAD), rng.randbytes(SIZE))),
        "open, data": (open_, lambda: (key, nonce, bytes(AAD), bytes(SIZE + 16)),
                       lambda: (key, nonce, rng.randbytes(AAD), rng.randbytes(SIZE + 16))),
        "open, tag": (open_, lambda: (key, nonce, bytes(AAD), ct + bytes([1]) + bytes(15)),
                      lambda: (key, nonce, bytes(AAD), ct + bytes(15) + bytes([1]))),
    }

    leaked = False
    for name, (op, fixed, rand) in tests.items():
        classes = [rng.randrange(2) for _ in range(n)]
        times = run(exe, op, [fixed() if c == 0 else rand() for c in classes])
        assert len(times) == n
        # The first thousand warm the caches and the branch predictor.
        samples = list(zip(classes, times))[1000:]
        t = max_t(samples)
        med = sorted(times)[len(times) // 2]
        leaked |= t >= 4.5
        print(f"{name:11} {n} samples, median {med} cycles, max |t| = {t:.2f}", flush=True)
    sys.exit(1 if leaked else 0)


if __name__ == "__main__":
    main()
