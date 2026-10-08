#!/usr/bin/env python3
"""The cost of each P-256 operation (docs/p256-fast.md §1 and §8).

    cancho build --std --backend llvm tests/programs/p256_profile.cho -o p256_profile
    python3 scripts/p256_bench.py ./p256_profile [--vs ./p256_profile_before] [op ...]

The driver reads `<op> <rounds>` and runs the operation `rounds` times (its
header lists the ops). The time of one is (T(rounds) - T(1)) / (rounds - 1),
the best of five runs of each, so process start and the driver's own set-up
(a key, a signature, a public point) cancel out. Prints ns per operation, and
per second for the end-to-end ones.

With `--vs <driver>` each measurement of the first driver is followed by the
same measurement of the second, alternating over the five runs, so a machine
whose clock moves (a shared laptop part under the `powersave` governor) gives
both the same conditions; the ratio is printed.
"""
import subprocess
import sys
import time

# op letter: (name, rounds). Rounds are chosen so a run takes a few hundred ms.
OPS = {
    "m": ("field multiplication mod p", 200000),
    "q": ("field squaring mod p", 200000),
    "r": ("4 independent field multiplications", 100000),
    "p": ("field addition + subtraction mod p", 200000),
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


def once(driver, op, rounds):
    s = time.perf_counter()
    out = subprocess.run([driver], input=f"{op} {rounds}\n", capture_output=True, text=True, check=True).stdout
    t = time.perf_counter() - s
    assert out.strip(), out
    return t


def per_op(drivers, op, rounds):
    """ns per operation of each driver, alternating runs; best of five."""
    best = {d: [1e9, 1e9] for d in drivers}
    for _ in range(5):
        for d in drivers:
            best[d][0] = min(best[d][0], once(d, op, rounds))
            best[d][1] = min(best[d][1], once(d, op, 1))
    return [(best[d][0] - best[d][1]) / (rounds - 1) for d in drivers]


def main():
    args = sys.argv[1:]
    drivers = [args.pop(0)]
    if "--vs" in args:
        i = args.index("--vs")
        drivers.append(args[i + 1])
        del args[i : i + 2]
    ops = args or list(OPS)
    for op in ops:
        name, rounds = OPS[op]
        each = per_op(drivers, op, rounds)
        tail = f", {1 / each[0]:.0f} per second" if op in END_TO_END else ""
        if len(each) == 2:
            tail += f"   (other: {each[1] * 1e9:.0f} ns, {each[1] / each[0]:.1f}x)"
        print(f"{op} {name:34} {each[0] * 1e9:12.0f} ns{tail}", flush=True)


if __name__ == "__main__":
    main()
