#!/usr/bin/env python3
"""The cost of each P-256 operation (docs/p256-fast.md §1 and §8).

    cancho build --std --backend llvm tests/programs/p256_profile.cho -o p256_profile
    python3 scripts/p256_bench.py ./p256_profile [op ...]

The driver reads `<op> <rounds>` and runs the operation `rounds` times (its
header lists the ops). The time of one is (T(rounds) - T(1)) / (rounds - 1),
the best of five runs of each, so process start and the driver's own set-up
(a key, a signature, a public point) cancel out. Prints ns per operation, and
per second for the end-to-end ones.
"""
import subprocess
import sys
import time

# op letter: (name, rounds). Rounds are chosen so a run takes a few hundred ms.
OPS = {
    "m": ("field multiplication mod p", 200000),
    "q": ("field squaring mod p", 200000),
    "a": ("field addition mod p", 200000),
    "s": ("field subtraction mod p", 200000),
    "i": ("inversion mod p", 2000),
    "n": ("inversion mod n", 2000),
    "o": ("multiplication mod n", 200000),
    "g": ("ecdh.public_key (k*G)", 300),
    "h": ("ecdh.shared (k*P)", 300),
    "k": ("ecdsa_sign.sign", 300),
    "c": ("ecdsa_sign.sign_checked", 300),
    "v": ("ecdsa.verify_raw", 300),
}
END_TO_END = "ghkcv"


def best(driver, op, rounds):
    data = f"{op} {rounds}\n"
    t = 1e9
    for _ in range(5):
        s = time.perf_counter()
        out = subprocess.run([driver], input=data, capture_output=True, text=True, check=True).stdout
        t = min(t, time.perf_counter() - s)
        assert out.strip(), out
    return t


def main():
    driver = sys.argv[1]
    ops = sys.argv[2:] or list(OPS)
    for op in ops:
        name, rounds = OPS[op]
        each = (best(driver, op, rounds) - best(driver, op, 1)) / (rounds - 1)
        tail = f", {1 / each:.0f} per second" if op in END_TO_END else ""
        print(f"{op} {name:34} {each * 1e9:12.0f} ns{tail}", flush=True)


if __name__ == "__main__":
    main()
