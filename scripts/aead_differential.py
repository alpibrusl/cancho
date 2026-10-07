#!/usr/bin/env python3
"""Differential test of `std.chacha20` against OpenSSL (docs/chacha20.md §4.3).

    python3 scripts/aead_differential.py <driver> [<count>]

`driver` is `tests/programs/aead_driver.cho` built with `cancho build --std`.
OpenSSL is reached through pyca/cryptography (`pip install cryptography`),
whose AEAD and Poly1305 are OpenSSL's EVP implementations.

For each of `count` (default 10,000) random (key, nonce, aad, message):
sealed here and opened by OpenSSL; sealed by OpenSSL and opened here (the
two sealed messages must also be byte-equal, since the AEAD is
deterministic); and OpenSSL's sealed message with one random bit flipped
must be refused by both. Message lengths cover 0 to 300 bytes densely,
with one case in ten up to 4,100 bytes. Exit status 1 on any difference.
"""
import os
import random
import subprocess
import sys

from cryptography.exceptions import InvalidTag
from cryptography.hazmat.backends.openssl.backend import backend
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305


def h(b):
    return b.hex() if b else "-"


def main():
    driver = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 10000
    rng = random.Random(8439)
    cases, checks = [], []
    for n in range(count):
        key = rng.randbytes(32)
        nonce = rng.randbytes(12)
        aad = rng.randbytes(rng.choice([0, 0, rng.randrange(1, 65)]))
        size = rng.randrange(0, 4101) if n % 10 == 0 else rng.randrange(0, 301)
        msg = rng.randbytes(size)
        theirs = ChaCha20Poly1305(key).encrypt(nonce, msg, aad)
        flipped = bytearray(theirs)
        bit = rng.randrange(len(flipped) * 8)
        flipped[bit // 8] ^= 1 << (bit % 8)
        try:
            ChaCha20Poly1305(key).decrypt(nonce, bytes(flipped), aad)
            openssl_refused = False
        except InvalidTag:
            openssl_refused = True
        cases.append(f"S {h(key)} {h(nonce)} {h(aad)} {h(msg)}")
        checks.append(("seal", key, nonce, aad, msg, theirs))
        cases.append(f"O {h(key)} {h(nonce)} {h(aad)} {h(theirs)}")
        checks.append(("open", key, nonce, aad, msg, theirs))
        cases.append(f"O {h(key)} {h(nonce)} {h(aad)} {h(bytes(flipped))}")
        checks.append(("flip", key, nonce, aad, msg, openssl_refused))

    out = subprocess.run([driver], input=("\n".join(cases) + "\n").encode(), capture_output=True, check=True).stdout
    lines = out.decode().splitlines()
    assert len(lines) == len(cases), f"{len(lines)} answers to {len(cases)} cases"
    bad = 0
    for (kind, key, nonce, aad, msg, extra), line in zip(checks, lines):
        code, _, got = (line.split(" ", 2) + ["", ""])[:3]
        if kind == "seal":
            ok = code == "0" and got == extra.hex()
            if ok:
                ok = ChaCha20Poly1305(key).decrypt(nonce, bytes.fromhex(got), aad) == msg
        elif kind == "open":
            ok = code == "0" and got == msg.hex()
        else:
            ok = extra and code == "-6" and got == "aa" * (len(msg))
        if not ok:
            bad += 1
            if bad <= 10:
                print(f"DIFFERENT {kind}: len(msg)={len(msg)} len(aad)={len(aad)}: {line[:120]}")
    print(f"{backend.openssl_version_text()}: {count} cases, {len(cases)} checks, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
