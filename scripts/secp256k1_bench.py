#!/usr/bin/env python3
"""The cost of each secp256k1 operation, against libsecp256k1 and OpenSSL (docs/secp256k1.md section 4).

    cancho build --std tests/programs/secp256k1_driver.cho packages/secp256k1/secp256k1.cho packages/secp256k1/comb.cho packages/secp256k1/field.cho -o drv
    python3 -I scripts/secp256k1_bench.py ./drv        # needs: pip install coincurve cryptography

Ours: the driver's `B` command runs one operation `rounds` times; the time of one is (T(rounds) - T(1)) / (rounds - 1), the best of five runs, so
process start and the driver's set-up (a public key, a signature) cancel out, as `scripts/p256_bench.py` does it.
References, in process, best of five batches, the same key and digest:
  libsecp256k1 through coincurve (the C library Bitcoin Core and most Ethereum clients use; its verification and signing use precomputed tables),
  OpenSSL through `cryptography` (generic EC code with a secp256k1 curve; no recovery).
Python adds a few microseconds a call to the references (they are a ceiling on the C library's time, not a floor); at these costs that is under 5%.
"""
import os, subprocess, sys, time
import coincurve
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.asymmetric.utils import Prehashed

KEY = bytes.fromhex("c9afa9d845ba75166b5c215767b1d6934e50c3db36e89b127b8a622b120f6721")  # a full-width key
OPS = [("P", "public key (k G)", 0x50, 300), ("S", "sign", 0x53, 300), ("R", "recover", 0x52, 150), ("V", "verify", 0x56, 150)]


def run(drv, op, rounds):
    line = f"B {KEY.hex()} {rounds:06x} {op:02x}\n".encode()
    best = 1e9
    for _ in range(5):
        t = time.perf_counter()
        out = subprocess.run([drv], input=line, capture_output=True).stdout.strip()
        best = min(best, time.perf_counter() - t)
        assert out == b"0", out
    return best


def ours(drv, op, rounds):
    return (run(drv, op, rounds) - run(drv, op, 1)) / (rounds - 1)


def timeit(fn, n):
    best = 1e9
    for _ in range(5):
        t = time.perf_counter()
        for _ in range(n):
            fn()
        best = min(best, (time.perf_counter() - t) / n)
    return best


def main():
    drv = sys.argv[1]
    ck = coincurve.PrivateKey(KEY)
    d = KEY  # the driver signs the digest equal to the key
    sig = ck.sign_recoverable(d, hasher=None)
    pub = ck.public_key
    der = ck.sign(d, hasher=None)
    ok = ec.derive_private_key(int.from_bytes(KEY, "big"), ec.SECP256K1())
    ossl_sig = ok.sign(d, ec.ECDSA(Prehashed(hashes.SHA256())))
    ossl_pub = ok.public_key()
    refs = {
        "P": (lambda: coincurve.PrivateKey(KEY).public_key, lambda: ec.derive_private_key(int.from_bytes(KEY, "big"), ec.SECP256K1()).public_key()),
        "S": (lambda: ck.sign_recoverable(d, hasher=None), lambda: ok.sign(d, ec.ECDSA(Prehashed(hashes.SHA256())))),
        "R": (lambda: coincurve.PublicKey.from_signature_and_message(sig, d, hasher=None), None),
        "V": (lambda: pub.verify(der, d, hasher=None), lambda: ossl_pub.verify(ossl_sig, d, ec.ECDSA(Prehashed(hashes.SHA256())))),
    }
    print(f"{'operation':<18}{'cancho (us)':>13}{'libsecp256k1 (us)':>20}{'OpenSSL (us)':>15}{'vs libsecp':>13}{'vs OpenSSL':>13}")
    for letter, name, code, rounds in OPS:
        mine = ours(drv, code, rounds)
        lib = timeit(refs[letter][0], 400)
        ossl = timeit(refs[letter][1], 100) if refs[letter][1] else None
        o = f"{ossl * 1e6:15.1f}" if ossl else f"{'--':>15}"
        r2 = f"{mine / ossl:12.1f}x" if ossl else f"{'--':>13}"
        print(f"{name:<18}{mine * 1e6:13.1f}{lib * 1e6:20.1f}{o}{mine / lib:12.1f}x{r2}")


main()
