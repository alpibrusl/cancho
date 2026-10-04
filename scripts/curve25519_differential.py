#!/usr/bin/env python3
"""Differential test of `std.x25519` and `std.ed25519` against OpenSSL (docs/x25519.md §4.3).

    python3 scripts/curve25519_differential.py <driver> [<x25519 count> [<ed25519 count>]]

`driver` is `tests/programs/curve25519_driver.ls` built with `lex-sys build --std`. OpenSSL is reached through
pyca/cryptography (`pip install cryptography`).

X25519 (default 10,000): a random scalar and a random 32-byte u-coordinate (so the top bit is set half the time, and a
non-canonical u, p or more, now and then), and the public key of a random scalar; each against OpenSSL's `exchange`,
which also refuses an all-zero result, as the driver must.

Ed25519 (default 1,000): a random seed and message: the public key and the signature against OpenSSL's (Ed25519 is
deterministic, so they must be equal), OpenSSL's signature verified here, and with one bit of it flipped, refused by both.
Exit status 1 on any difference.
"""
import random
import subprocess
import sys

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey

RAW = (serialization.Encoding.Raw, serialization.PublicFormat.Raw)
P = 2**255 - 19


def main():
    driver = sys.argv[1]
    nx = int(sys.argv[2]) if len(sys.argv) > 2 else 10000
    ne = int(sys.argv[3]) if len(sys.argv) > 3 else 1000
    rng = random.Random(7748)
    cases, want = [], []
    for n in range(nx):
        k = rng.randbytes(32)
        if n % 50 == 0:
            # A non-canonical u: p plus a little, with or without the top bit.
            u = ((P + rng.randrange(19)) | (rng.randrange(2) << 255)).to_bytes(32, "little")
        else:
            u = rng.randbytes(32)
        try:
            shared = X25519PrivateKey.from_private_bytes(k).exchange(X25519PublicKey.from_public_bytes(u))
            w = f"0 ok {shared.hex()}"
        except ValueError:
            w = f"-2 x25519-zero-secret {'00' * 32}"
        cases.append(f"S {k.hex()} {u.hex()}")
        want.append(w)
        pub = X25519PrivateKey.from_private_bytes(k).public_key().public_bytes(*RAW)
        cases.append(f"S {k.hex()} {(bytes([9]) + bytes(31)).hex()}")
        want.append(f"0 ok {pub.hex()}")
    for n in range(ne):
        seed = rng.randbytes(32)
        msg = rng.randbytes(rng.randrange(0, 300))
        key = Ed25519PrivateKey.from_private_bytes(seed)
        pk = key.public_key().public_bytes(*RAW)
        sig = key.sign(msg)
        m = msg.hex() or "-"
        cases.append(f"P {seed.hex()}")
        want.append(f"0 ok {pk.hex()}")
        cases.append(f"E {seed.hex()} {m}")
        want.append(f"0 ok {sig.hex()}")
        cases.append(f"V {pk.hex()} {m} {sig.hex()}")
        want.append("1 unknown")
        bad = bytearray(sig)
        bit = rng.randrange(512)
        bad[bit // 8] ^= 1 << (bit % 8)
        try:
            key.public_key().verify(bytes(bad), msg)
            openssl_refused = False
        except InvalidSignature:
            openssl_refused = True
        assert openssl_refused
        cases.append(f"V {pk.hex()} {m} {bytes(bad).hex()}")
        want.append("0 ok")

    out = subprocess.run([driver], input=("\n".join(cases) + "\n").encode(), capture_output=True, check=True).stdout
    lines = [l.rstrip() for l in out.decode().splitlines()]
    assert len(lines) == len(cases), f"{len(lines)} answers to {len(cases)} cases"
    bad = 0
    for case, w, line in zip(cases, want, lines):
        if line != w:
            bad += 1
            if bad <= 10:
                print(f"DIFFERENT {case[:40]}: got {line[:80]!r}, want {w[:80]!r}")
    print(f"{nx} X25519 and {ne} Ed25519 rounds, {len(cases)} checks, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
