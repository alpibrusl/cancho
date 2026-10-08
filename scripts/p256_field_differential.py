#!/usr/bin/env python3
"""`std.p256`'s field and scalar arithmetic against Python's integers (docs/p256-fast.md §4.4).

    cancho build --std tests/programs/p256_driver.cho -o p256_driver
    python3 scripts/p256_field_differential.py ./p256_driver [<cases per operation>]

Every operation on random operands and on the edges that matter to a limb
representation: 0, 1, 2, p - 1, p, p + 1 (loadable, not reduced), 2^256 - 1,
n - 1, n, all limbs 2^28 - 1, and values with one limb set. Reads the driver's
answer and compares it with the integer arithmetic.
"""
import random
import subprocess
import sys

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


def run(driver, lines):
    out = subprocess.run([driver], input="\n".join(lines) + "\n", capture_output=True, text=True, check=True).stdout.split("\n")
    assert out[-1] == "" and len(out) == len(lines) + 1
    return out[:-1]


def h(v):
    return f"{v:064x}"


def main():
    driver = sys.argv[1]
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 3000
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
    cases = [c for c in cases if c[1] is not None]
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
