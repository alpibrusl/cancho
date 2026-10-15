#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""A dudect-style timing test of `std.rsa_sign` at PROCESS granularity (docs/rsa-sign.md §7 and §11, gate G7), the method of the B2 spike.

    python3 scripts/rsa_sign_timing.py <driver> <keydir> <test> <bits> <rounds> <samples per class> [outfile]

Each sample is the wall time of one run of the driver, which signs the same case `rounds` times (`T <rounds> <case>`), so start-up and
input decoding are the same for both classes; the classes are interleaved at random; Welch's t is printed for all samples and for the
slowest 10% of each class cropped. The pass line is |t| < 4.5 (cancho's gate, docs/ecdsa-sign.md §6). Tests:
    control        THE POSITIVE CONTROL: `bigmod.pow_mod` on a secret exponent (it branches on the bits): low-weight d against random d.
                   It MUST fail (|t| far over 4.5), or this harness has no power and every pass below means nothing.
    exp            the signer with dp and dq of 64 set bits against random dp and dq (the key is then inconsistent, so the check is off, mode 1)
    exp-unblinded  as `exp` with no blinding (mode 1 | 16)
    key            a fixed valid key against a random valid key of the same size (the check on, the signer exactly as shipped)
    key-unblinded  as `key`, blinding off
Samples are means of `rounds` signatures, so a leak that averages out within a run is hidden; the machine is shared and nothing is pinned;
only the exponents' weight and the identity of the key vary. A pass is "no evidence of a leak at this sensitivity", not "constant time".
"""
import math
import os
import random
import statistics
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rsa_sign_lib as L  # noqa: E402
from rsa_sign_differential import keys_of  # noqa: E402

driver, keydir, test, bits, rounds, per = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), int(sys.argv[5]), int(sys.argv[6])
rng = random.Random(4242)
allk = keys_of(keydir)
bal = [k for k in allk if k["bits"] == bits and k["e"] == 65537]
base = next(k for k in bal if k["name"].endswith("_0") and "_u" not in k["name"])
digest = L.digest_of(32, b"dudect")


def lowweight(limit):
    v = 1
    for pos in rng.sample(range(1, limit.bit_length() - 1), 63):
        v |= 1 << pos
    return v


def rand_odd(limit):
    return rng.randrange(1, limit) | 1


def line_for(cls):
    ent = rng.randbytes(16).hex()
    if test == "control":
        d = lowweight(base["n"] // 2) if cls == 0 else rand_odd(base["n"] // 2)
        k = (bits + 7) // 8
        return f"L {base['n']:0{2 * k}x} {d:0{2 * k}x} {L.emsa(32, digest, k).hex()}"
    if test.startswith("exp"):
        key = dict(base)
        key["dp"] = lowweight(key["p"] - 1) if cls == 0 else rand_odd(key["p"] - 1)
        key["dq"] = lowweight(key["q"] - 1) if cls == 0 else rand_odd(key["q"] - 1)
        mode = 1 | (16 if test.endswith("unblinded") else 0)
        return f"U 32 {L.block(key).hex()} {digest.hex()} {'-' if mode & 16 else ent} {mode}"
    key = base if cls == 0 else rng.choice([k for k in bal if k is not base])
    if test == "key-unblinded":
        return f"U 32 {L.block(key).hex()} {digest.hex()} - 16"
    return f"S 32 {L.block(key).hex()} {digest.hex()} {ent}"


def run(line):
    s = time.perf_counter_ns()
    out = subprocess.run([driver], input=f"T {rounds} {line}\n", capture_output=True, text=True, check=True).stdout
    t = time.perf_counter_ns() - s
    assert out.startswith("0 ") or test == "control" and out.startswith("0"), "the driver did not sign: " + out[:60]
    return t


for _ in range(5):
    run(line_for(0))
    run(line_for(1))
order = [0] * per + [1] * per
rng.shuffle(order)
samples = ([], [])
began = time.time()
for c in order:
    samples[c].append(run(line_for(c)))


def welch(a, b):
    ma, mb = statistics.fmean(a), statistics.fmean(b)
    va, vb = statistics.variance(a), statistics.variance(b)
    return (ma - mb) / math.sqrt(va / len(a) + vb / len(b)), ma, mb


def crop(a, frac):
    a = sorted(a)
    return a[: int(len(a) * (1 - frac))]


t_all, ma, mb = welch(*samples)
t_crop, _, _ = welch(crop(samples[0], 0.1), crop(samples[1], 0.1))
msg = (f"{test} RSA-{bits} rounds/run {rounds} samples/class {per}: class 0 mean {ma / 1e6 / rounds:.4f} ms/sign, class 1 {mb / 1e6 / rounds:.4f} ms/sign "
       f"(diff {(ma - mb) / mb * 100:+.2f}%), Welch t = {t_all:.2f} (all), {t_crop:.2f} (slowest 10% cropped); {time.time() - began:.0f} s")
print(msg)
if len(sys.argv) > 7:
    open(sys.argv[7], "a").write(msg + "\n")
