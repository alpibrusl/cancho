#!/usr/bin/env python3
"""Differential check of cancho's float text (docs/json.md §4, docs/float-printing.md §3.4, docs/floating-point.md §4.1).

    python3 scripts/float_differential.py [--cancho target/release/cancho] [--read 1000000] [--print 1000000] [--seed 1]

READING. `std.json.to_float` against Python's `float()` (correctly rounded, ties to even), on generated and adversarial
strings: random doubles in shortest form, 17-digit and long decimals, integers past 2^53, exponent forms, and -- the cases a
floating-point shortcut gets wrong -- the exact decimal midpoint between two adjacent doubles, a hair above it and a hair
below it, at every exponent from 2^-1075 (the boundary below the smallest subnormal) to the overflow edge, subnormals
included. Every adversarial answer is also checked against exact rational arithmetic (`fractions`), so the oracle is
certified and not trusted.

PRINTING. `std.fmt.float_into` (through `float_of_bits`) against Python's `repr`: the same digits and exponent for random bit
patterns, and for exact ties between two shortest decimals (the value ends in a 5 one digit past the shortest candidates).
Rule: shortest that reads back, then the closest to the exact value, then the even last digit. Every output also reads back
to the same bits.

Exit 1 on any difference; the first few are printed with the input, the answer and the oracle.
"""
import argparse, json, os, random, struct, subprocess, sys, tempfile
from decimal import Decimal, getcontext
from fractions import Fraction

getcontext().prec = 1400
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
U = Fraction(1, 2 ** 1074)  # the smallest subnormal


def bits(x):
    return struct.unpack(">Q", struct.pack(">d", x))[0]


def double(b):
    return struct.unpack(">d", struct.pack(">Q", b))[0]


def exact(fr):
    """The decimal text of a dyadic rational, exactly."""
    return format(Decimal(fr.numerator) / Decimal(fr.denominator), "f")


def midpoint(b):
    """The exact value halfway between the finite doubles with bits b and b + 1."""
    return (Fraction(double(b)) + Fraction(double(b + 1))) / 2


def nearest_even(fr):
    """Exact reference rounding of a non-negative rational to a double's bits (None past the range)."""
    if fr == 0:
        return 0
    lo = 0
    hi = 0x7FF0000000000000  # infinity's pattern: bisect on the finite doubles
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if Fraction(double(mid)) <= fr:
            lo = mid
        else:
            hi = mid
    if lo == 0x7FEFFFFFFFFFFFFF:  # DBL_MAX: the next "double" is 2^1024, and a tie there goes up to it (even)
        top = Fraction(2) ** 1024
        d = fr - Fraction(double(lo))
        return 0x7FF0000000000000 if d * 2 >= top - Fraction(double(lo)) else lo
    a, b = Fraction(double(lo)), Fraction(double(lo + 1))
    d = (fr - a) * 2 - (b - a)
    if d < 0:
        return lo
    if d > 0:
        return lo + 1
    return lo if lo % 2 == 0 else lo + 1


def reading_cells(r, n):
    """(text, certify) pairs; `certify` cells are also checked against exact arithmetic."""
    cells = []
    # the boundary at 2^-1075 and every small subnormal midpoint, exactly, and a hair either side
    for k in list(range(0, 64)):
        m = Fraction(2 * k + 1, 2) * U
        for sh in (0, 1, 2, 10, 53, 200):
            cells.append((exact(m), True) if sh == 0 else (exact(m + U / 2 ** (sh + 2)), True))
            if sh:
                cells.append((exact(m - U / 2 ** (sh + 2)), True))
        cells.append((exact(m) + "1", True))
        cells.append((exact(m) + "0" * 40 + "1", True))
        cells.append((exact(m)[:-1] + "49999", True))
    while len(cells) < n // 20:  # adversarial: midpoints anywhere (subnormals twice as often), +- hairs
        k = r.choice([r.getrandbits(52), r.getrandbits(r.randint(1, 52)), r.getrandbits(62) % 0x7FEFFFFFFFFFFFFF])
        m = midpoint(k) if k < 0x7FEFFFFFFFFFFFFF else Fraction(double(0x7FEFFFFFFFFFFFFF)) + (Fraction(2) ** 1024 - Fraction(double(0x7FEFFFFFFFFFFFFF))) / 2
        s = exact(m)
        cells.append((s, True))
        cells.append((s + r.choice(["1", "01", "000001", "9"]), True))
        cells.append((s[:-1] + r.choice(["4", "49", "4999999"]) if s[-1] == "5" else s + "0", True))
        if "." in s:  # the same value in exponent form: the digits as an integer, the places as the exponent
            whole, frac = s.split(".")
            cells.append(((whole + frac).lstrip("0") + "e-%d" % len(frac), True))
    while len(cells) < n:
        k = r.random()
        if k < 0.15:
            cells.append(("%.2f" % r.uniform(-1e5, 1e5), False))
        elif k < 0.40:
            cells.append((repr(double(r.getrandbits(63) % 0x7FEFFFFFFFFFFFFF)), False))
        elif k < 0.48:
            cells.append((str(r.randint(-2 ** 70, 2 ** 70)), False))
        elif k < 0.60:
            cells.append(("%de%d" % (r.randint(-10 ** 6, 10 ** 6), r.randint(-330, 310)), False))
        elif k < 0.72:
            cells.append(("%.17g" % (r.uniform(-1, 1) * 10.0 ** r.randint(-320, 300)), False))
        elif k < 0.80:
            cells.append(("%d.%se%d" % (r.randint(1, 9), "".join(r.choice("0123456789") for _ in range(r.randint(18, 800))), r.randint(-1100, 300)), False))
        elif k < 0.90:
            cells.append((repr(double(r.getrandbits(52) | r.getrandbits(1) << 63)), False))  # subnormals
        else:
            b = r.getrandbits(63)
            if b >= 0x7FEFFFFFFFFFFFFF:
                b >>= 2
            s = exact(midpoint(b)) if b < 0x0010000000000000 or r.random() < 0.5 else repr(double(b))
            cells.append((s + r.choice(["", "", "1"]), False))
    r.shuffle(cells)
    return cells


def run(cmd, data):
    p = subprocess.run(cmd, input=data, capture_output=True)
    if p.returncode != 0:
        sys.exit("driver failed (%d): %s" % (p.returncode, p.stderr.decode()[:300]))
    return p.stdout.decode().split("\n")


def check_reading(exe, cells):
    text = "[" + ",".join(c for c, _ in cells) + "]"
    out = run([exe], text.encode())
    bad = []
    for (c, certify), line in zip(cells, out):
        got = int(line.split()[0]) & (2 ** 64 - 1)
        v = float(c)
        want = bits(v)
        if certify and not (c.lstrip("-")[:1] == "0" and "e" in c and False):
            sign = c.startswith("-")
            fr = Fraction(Decimal(c)) if "e" not in c.lower() else Fraction(Decimal(c))
            ref = nearest_even(abs(fr))
            if ref != (want & ~(1 << 63)):
                bad.append(("ORACLE DISAGREES WITH EXACT", c, want, ref))
        if got != want:
            bad.append(("read", c, got, want))
    return len(cells), bad


def tie_bits(r, n):
    """Doubles whose exact decimal ends in a 5 one digit past a shortest candidate: j * 2^-k, j odd, 17-18 digits."""
    out = []
    while len(out) < n:
        k = r.randint(4, 60)
        digits = r.choice([17, 18])
        lo, hi = -(-(10 ** (digits - 1)) // 5 ** k), (10 ** digits - 1) // 5 ** k
        if hi < 1 or lo >= 2 ** 53 or lo > hi:
            continue
        j = r.randint(max(lo, 1), min(hi, 2 ** 53 - 1)) | 1
        x = j / 2 ** k
        if Fraction(x) == Fraction(j, 2 ** k):
            out.append(bits(x))
    return out


def shape(text):
    """(sign, digits, exponent) of a decimal text: `d[.ddd]e[-]k` and Python's `repr` both reduce to it."""
    neg = text.startswith("-")
    d = Decimal(text.lstrip("-"))
    if d == 0:
        return (neg, "0", 0)
    t = d.as_tuple()
    digs = "".join(map(str, t.digits)).rstrip("0") or "0"
    exp = len(t.digits) + t.exponent - 1
    return (neg, digs, exp)


def check_printing(exe, patterns):
    patterns = [p if p < 2 ** 63 else p - 2 ** 64 for p in patterns]
    out = run([exe], ("[" + ",".join(map(str, patterns)) + "]").encode())
    bad = []
    for p, line in zip(patterns, out):
        b = p & (2 ** 64 - 1)
        x = double(b)
        if x != x:
            if line != "NaN":
                bad.append(("print", hex(b), line, "NaN"))
            continue
        if x in (float("inf"), float("-inf")):
            if line != ("inf" if x > 0 else "-inf"):
                bad.append(("print", hex(b), line, "inf"))
            continue
        if bits(float(line)) != b:
            bad.append(("does not read back", hex(b), line, repr(x)))
        elif shape(line) != shape(repr(x)):
            bad.append(("print", hex(b), line, repr(x)))
    return len(patterns), bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cancho", default=os.path.join(ROOT, "target/release/cancho"))
    ap.add_argument("--read", type=int, default=1_000_000)
    ap.add_argument("--print", dest="count", type=int, default=1_000_000)
    ap.add_argument("--seed", type=int, default=1)
    a = ap.parse_args()
    r = random.Random(a.seed)
    d = tempfile.mkdtemp()
    reader, printer = os.path.join(d, "read"), os.path.join(d, "print")
    for src, exe in (("json_floats.cho", reader), ("float_print.cho", printer)):
        p = subprocess.run([a.cancho, "build", os.path.join(ROOT, "tests/programs", src), "--std", "-o", exe], capture_output=True)
        if p.returncode:
            sys.exit(p.stderr.decode())
    failed = False
    cells = reading_cells(r, a.read)
    for part in range(0, len(cells), 100_000):
        n, bad = check_reading(reader, cells[part:part + 100_000])
        print("read   %7d cells, %d wrong" % (n, len(bad)))
        for b in bad[:5]:
            print("   ", b[0], b[1][:60] + ("..." if len(b[1]) > 60 else ""), hex(b[2]), hex(b[3]))
        failed |= bool(bad)
    ties = tie_bits(r, max(a.count // 10, 1000))
    others = [r.getrandbits(64) for _ in range(a.count - len(ties))]
    others += [0x8000000000000000, 1, 0x000FFFFFFFFFFFFF, 0x0010000000000000, 0x7FEFFFFFFFFFFFFF, 0x7FF0000000000000, 0xFFF0000000000000]
    others += [bits(2.0 ** e) for e in range(-1074, 1024)] + [bits(2.0 ** -25), bits(5e-324), bits(1e23), bits(9.999999999999999e22)]
    allp = ties + others
    r.shuffle(allp)
    for part in range(0, len(allp), 100_000):
        n, bad = check_printing(printer, allp[part:part + 100_000])
        print("print  %7d doubles, %d wrong" % (n, len(bad)))
        for b in bad[:5]:
            print("   ", *b)
        failed |= bool(bad)
    print("tie doubles among them: %d" % len(ties))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
