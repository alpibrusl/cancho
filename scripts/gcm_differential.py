#!/usr/bin/env python3
"""Differential test of `std.gcm` against OpenSSL (docs/tls-parity.md §3.1).

    python3 scripts/gcm_differential.py <driver> [<count>]

`driver` is `tests/programs/gcm_driver.cho` built with `cancho build --std`.
OpenSSL is reached through pyca/cryptography (`pip install cryptography`),
whose AESGCM is OpenSSL's EVP implementation.

For each of `count` (default 10,000) random (key, nonce, aad, message),
the key 16 or 32 bytes at random:
sealed here and opened by OpenSSL; sealed by OpenSSL and opened here (the
two sealed messages must also be byte-equal, since the AEAD is
deterministic); and OpenSSL's sealed message with one random bit flipped
must be refused by both. Message lengths cover 0 to 300 bytes densely,
with one case in ten up to 4,100 bytes. Then a sweep (docs/gcm-wide.md §7)
that does not depend on the random draw: every message length from 0 to 600
bytes (so every tail of the 128-byte groups of `aes_ctr32` and the 16-byte
blocks of `gcm_tag`), every associated-data length from 0 to 300, and the
lengths around 4 KiB and 16 KiB (a region is 64 KiB, which bounds the driver). Every sealed case is also sealed and
opened on the software path (the driver's `s` and `o`), so both paths of
`std.gcm` are compared with OpenSSL. Exit status 1 on any difference.
"""
import os
import random
import subprocess
import sys

from cryptography.exceptions import InvalidTag
from cryptography.hazmat.backends.openssl.backend import backend
from cryptography.hazmat.primitives.ciphers.aead import AESGCM


def h(b):
    return b.hex() if b else "-"


def main():
    driver = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 10000
    rng = random.Random(38)
    cases, checks = [], []
    # (aad length, message length): the random draw, then the sweep.
    sweep = [(a, n) for n in range(601) for a in (0, 13)]
    sweep += [(a, m) for a in range(301) for m in (0, 64, 1000)]
    sweep += [(a, m) for m in (127, 128, 129, 255, 256, 257, 4095, 4096, 4097, 8191, 8192, 8193, 16383, 16384, 16385,
                                16384 + 127, 16384 + 129) for a in (0, 5, 13, 16, 129)]
    for n in range(count + len(sweep)):
        key = rng.randbytes(rng.choice([16, 32]))
        nonce = rng.randbytes(12)
        if n < count:
            aad = rng.randbytes(rng.choice([0, 0, rng.randrange(1, 65), rng.randrange(65, 300)]))
            size = rng.randrange(0, 4101) if n % 10 == 0 else rng.randrange(0, 301)
        else:
            alen, size = sweep[n - count]
            aad = rng.randbytes(alen)
        msg = rng.randbytes(size)
        theirs = AESGCM(key).encrypt(nonce, msg, aad)
        flipped = bytearray(theirs)
        bit = rng.randrange(len(flipped) * 8)
        flipped[bit // 8] ^= 1 << (bit % 8)
        try:
            AESGCM(key).decrypt(nonce, bytes(flipped), aad)
            openssl_refused = False
        except InvalidTag:
            openssl_refused = True
        cases.append(f"S {h(key)} {h(nonce)} {h(aad)} {h(msg)}")
        checks.append(("seal", key, nonce, aad, msg, theirs))
        cases.append(f"O {h(key)} {h(nonce)} {h(aad)} {h(theirs)}")
        checks.append(("open", key, nonce, aad, msg, theirs))
        cases.append(f"O {h(key)} {h(nonce)} {h(aad)} {h(bytes(flipped))}")
        checks.append(("flip", key, nonce, aad, msg, openssl_refused))
        # The software path, on the same inputs.
        cases.append(f"s {h(key)} {h(nonce)} {h(aad)} {h(msg)}")
        checks.append(("seal", key, nonce, aad, msg, theirs))
        cases.append(f"o {h(key)} {h(nonce)} {h(aad)} {h(theirs)}")
        checks.append(("open", key, nonce, aad, msg, theirs))

    out = subprocess.run([driver], input=("\n".join(cases) + "\n").encode(), capture_output=True, check=True).stdout
    lines = out.decode().splitlines()
    assert len(lines) == len(cases), f"{len(lines)} answers to {len(cases)} cases"
    bad = 0
    for (kind, key, nonce, aad, msg, extra), line in zip(checks, lines):
        code, _, got = (line.split(" ", 2) + ["", ""])[:3]
        if kind == "seal":
            ok = code == "0" and got == extra.hex()
            if ok:
                ok = AESGCM(key).decrypt(nonce, bytes.fromhex(got), aad) == msg
        elif kind == "open":
            ok = code == "0" and got == msg.hex()
        else:
            ok = extra and code == "-6" and got == "aa" * (len(msg))
        if not ok:
            bad += 1
            if bad <= 10:
                print(f"DIFFERENT {kind}: len(msg)={len(msg)} len(aad)={len(aad)}: {line[:120]}")
    print(f"{backend.openssl_version_text()}: {count} random + {len(sweep)} swept cases, {len(cases)} checks, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
