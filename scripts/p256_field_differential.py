#!/usr/bin/env python3
"""`std.p256`'s field and scalar arithmetic against Python's integers (docs/p256-fast.md §4.4).

    cancho build --std tests/programs/p256_driver.cho -o p256_driver
    python3 scripts/p256_field_differential.py ./p256_driver [<cases per operation>]
    python3 scripts/p256_field_differential.py --vectors tests/vectors/p256.txt [<cases per operation>]

With `--vectors` it does not run the driver: it writes the cases and Python's answers (one
`case | answer` a line) for `crates/cancho/tests/conformance/p256.rs`, which runs them
through the driver in `cargo test`.

Every operation, `K` (the public key of a scalar, against affine addition in Python) and `W`
(the wNAF of a scalar, digit for digit against Python's) on random operands and on the edges that
matter to a limb representation: 0, 1, 2, p - 1, p, p + 1 (loadable, not reduced), 2^256 - 1,
n - 1, n, all limbs 2^28 - 1, and values with one limb set. Reads the driver's
answer and compares it with the integer arithmetic.
"""
import random
import subprocess
import sys
from pathlib import Path

P = 0xFFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF
N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
M28 = (1 << 28) - 1
MAX = (1 << 256) - 1


def edges():
    out = [0, 1, 2, 3, P - 1, P - 2, N - 1, N - 2, (1 << 255), (1 << 224) - 1, 1 << 224, (1 << 192) + 1, MAX >> 4]
    for i in range(0, 256, 28):
        out.append((M28 << i) & MAX)
        out.append((1 << i) & MAX)
    out.append(int("".join("f" * 7 + "0" for _ in range(8)), 16))
    return out


def pick(rng, e, modulus=None):
    r = rng.random()
    if r < 0.2:
        return rng.choice(e)
    if r < 0.4:
        return rng.randrange(modulus or P)
    if r < 0.5:
        return modulus - 1 - rng.randrange(1000) if modulus else P - 1 - rng.randrange(1000)
    return rng.randrange(modulus or P)


GX = 0x6B17D1F2E12C4247F8BCE6E563A440F277037D812DEB33A0F4A13945D898C296
GY = 0x4FE342E2FE1A7F9B8EE7EB4A7C0F9E162BCE33576B315ECECBB6406837BF51F5


def ec_add(a, b):
    if a is None:
        return b
    if b is None:
        return a
    (x1, y1), (x2, y2) = a, b
    if x1 == x2:
        if (y1 + y2) % P == 0:
            return None
        lam = (3 * x1 * x1 - 3) * pow(2 * y1, -1, P) % P
    else:
        lam = (y2 - y1) * pow(x2 - x1, -1, P) % P
    x3 = (lam * lam - x1 - x2) % P
    return (x3, (lam * (x1 - x3) - y1) % P)


def ec_mul(k, pt):
    acc = None
    while k:
        if k & 1:
            acc = ec_add(acc, pt)
        pt = ec_add(pt, pt)
        k >>= 1
    return acc


def wnaf(k, width):
    digits = []
    while k:
        if k & 1:
            d = k % (1 << width)
            if d >= 1 << (width - 1):
                d -= 1 << width
            k -= d
        else:
            d = 0
        digits.append(d)
        k >>= 1
    return digits + [0] * (257 - len(digits))


def scalar_edges():
    out = [0, 1, 2, 3, 7, 8, 9, 15, 16, 17, 0x88, 0x89, 0x98, 0x99, N - 1, N - 2, N, N + 1, MAX, 1 << 255, (1 << 255) + 1, 1 << 224, (1 << 252) - 1]
    for nib in "1789abcdef":
        out.append(int(nib * 64, 16))
        out.append(int(nib * 63, 16))
    out.append(int("89" * 32, 16))
    out.append(int("98" * 32, 16))
    out.append(int("8f" * 32, 16) & MAX)
    return out


def run(driver, lines):
    out = subprocess.run([driver], input="\n".join(lines) + "\n", capture_output=True, text=True, check=True).stdout.split("\n")
    assert out[-1] == "" and len(out) == len(lines) + 1
    return out[:-1]


def h(v):
    return f"{v:064x}"


def main():
    vectors = len(sys.argv) > 2 and sys.argv[1] == "--vectors"
    driver = sys.argv[1]
    count = int(sys.argv[3]) if vectors and len(sys.argv) > 3 else (int(sys.argv[2]) if not vectors and len(sys.argv) > 2 else 3000)
    rng = random.Random(256)
    e = edges()
    cases = []
    for _ in range(count):
        a, b = pick(rng, e), pick(rng, e)
        an, bn = pick(rng, e, N), pick(rng, e, N)
        cases += [
            (f"M {h(a)} {h(b)}", h(a * b % P)),
            (f"A {h(a)} {h(b)}", h((a + b) % P)),
            (f"S {h(a)} {h(b)}", h((a - b) % P)),
            (f"Q {h(a)}", h(a * a % P)),
            (f"I {h(a)}", h(pow(a, -1, P)) if a else None),
            (f"X {h(an)} {h(bn)}", h(an * bn % N)),
            (f"Z {h(an)} {h(bn)}", h((an + bn) % N)),
            (f"Y {h(an)}", h(pow(an, -1, N)) if an else None),
            (f"C {h(a)} {h(b)}", h(((a * b * 8 - b) ** 2) % P)),
        ]
        v = rng.choice([a, an, rng.randrange(1 << 256), rng.choice(e)])
        cases.append((f"L {h(v)}", h(v)))
        cases.append((f"B {h(v)}", f"p{int(v < P)}n{int(v < N)}"))
    for k in scalar_edges() + [rng.randrange(1, N) for _ in range(count // 3)] + [rng.randrange(1, N) | (rng.randrange(1 << 256) & rng.randrange(1 << 256)) for _ in range(count // 3)]:
        pt = ec_mul(k, (GX, GY)) if 0 < k < N else None
        cases.append((f"K {h(k % (1 << 256))}", h(pt[0]) + h(pt[1]) if pt else "-"))
    for k in scalar_edges() + [rng.randrange(1 << 256) for _ in range(count // 3)]:
        if k < 1 << 256:
            for width in (5, 7):
                cases.append((f"W {h(k)} {width}", " ".join(map(str, wnaf(k, width))) + " "))
    # The final check of verification: x = r, or x = r + n when that is below p. No real signature
    # reaches the second (p - n is about 2^128), so these are the check on its own.
    small = [1, 2, 3, 12345, (1 << 126) - 1, P - N - 1, P - N - 2, P - N, P - N + 1, N - 1, N - 2, P - 1, N - (1 << 127)]
    for r in small + [rng.randrange(1, N) for _ in range(count // 4)]:
        for x in sorted({r, r + 1, r - 1, r + N, r + N - 1, r + N + 1, N - r, (r + N) % P, P - 1 - r % 7}):
            if 0 <= x < P and 0 < r < N:
                cases.append((f"T {h(r)} {h(x)}", str(int(x % N == r))))
    cases = [c for c in cases if c[1] is not None]
    if vectors:
        Path(sys.argv[2]).write_text("".join(f"{line} | {want}\n" for line, want in cases))
        print(f"wrote {len(cases)} cases to {sys.argv[2]}")
        return
    answers = run(driver, [c[0] for c in cases])
    bad = 0
    for (line, want), got in zip(cases, answers):
        if got != want:
            bad += 1
            if bad <= 5:
                print("MISMATCH", line, "want", want, "got", got)
    print(f"{len(cases)} cases, {bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
