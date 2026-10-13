#!/usr/bin/env python3
"""Generates `std/p256_kernels.cho`, the straight-line field kernels of
`std.p256` (docs/p256-fast.md §4), and proves the bounds the code relies on.

    python3 scripts/p256_gen.py            # rewrites std/p256_kernels.cho
    python3 scripts/p256_gen.py --check    # fails when the file is not what this prints

A field element is ten limbs of 28 bits, least significant first, one limb to
a word of a caller's `work`, Montgomery radix R = 2^280. The multiplications
(docs/p256-fast.md §11) pack each pair of limbs into one of 56 bits, take each
product whole with `mul_wide`, sum a column in a two-word accumulator
(`add_carry`), and reduce in radix 2^56 (five steps: the same R). The script
computes, for each column, the largest value the accumulator and the carry out
of it can reach when every input limb is at its largest (2^28 - 1), and refuses
to write a kernel in which an accumulator reaches 2^127 or a carry 2^63. So the
`wrapping_*` cannot wrap, and a wrong bound is a failed generation, not a
silent wrong answer (§4.3). The additions and subtractions stay on 28-bit
limbs, `wrapping_*` too: the script checks every limb's sum stays below 2^31.

P-256's prime is p = 2^256 - 2^224 + 2^192 + 2^96 - 1, which is -1 modulo
2^96, so -p^-1 = 1 modulo 2^56: the Montgomery quotient digit is the low limb
of the column itself (§4.2). In radix 2^56 its limbs are 2^56 - 1, 2^40 - 1, 0,
2^24 and 2^32 - 1: no multiplication by n' and, in the reduction, constants
that `mac` multiplies like any other (§11 measured them written as shifts: no
gain).
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "std" / "p256_kernels.cho"

BITS = 28
N = 10
M = (1 << BITS) - 1
R = 1 << (BITS * N)
P = 0xFFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF
ORDER = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
B = 0x5AC635D8AA3A93E7B3EBBD55769886BC651D06B0CC53B0F63BCE3C3E27D2604B
SUB_KS = (2, 4, 8, 16, 32, 64)  # sub_K adds K * p: a subtrahend below K p is covered.


def limbs(v):
    assert 0 <= v < R
    return [(v >> (BITS * i)) & M for i in range(N)]


class Bound:
    """The largest value an accumulator (two words) and a carry (one word) can reach, tracked while emitting."""

    def __init__(self):
        self.worst = {}

    def see(self, v, where, limit):
        if v > self.worst.get(limit, (0, None))[0]:
            self.worst[limit] = (v, where)
        assert v < limit, f"{where} can reach 2^{v.bit_length() - 1}, limit 2^{limit.bit_length() - 1}"

    def note(self, limit):
        v, where = self.worst[limit]
        e = v.bit_length() - 1
        return f"{e}.{(v * 100 >> e) % 100:02d} bits (limit {limit.bit_length() - 1}), {where}"


W = 2 * BITS  # a wide limb: two 28-bit limbs packed into one word
NW = N // 2
MW = (1 << W) - 1
LIMIT_ACC = 1 << 127  # a two-word accumulator, with a bit to spare
LIMIT_WORD = 1 << 63  # a carry word, non-negative as an `int`
LIMIT_SUM = 1 << 31  # a limb of an addition or subtraction, before its carry is taken


def guard(names):
    """The slices' lengths, tested once: unreachable (each slice is ten words or the slicing trapped), and
    what lets LLVM drop the bounds check of every constant index after it (docs/p256-fast.md §11)."""
    test = " || ".join(f"len({n}) < 10" for n in names)
    return [f"    if {test} {{", "        return -1;", "    }"]


def wide_mont_mul(name, mod, square, bound, shifts=False):
    """A Montgomery multiplication kernel modulo `mod` (p or the order) on `mul_wide` (docs/p256-fast.md §11).

    The ten 28-bit limbs of each input are packed in pairs into five 56-bit limbs, the product is
    scanned column by column into a two-word accumulator (`mac`), and the five reduction steps of
    radix 2^56 give the same R = 2^280; the five 56-bit limbs of the answer are unpacked into ten
    28-bit ones. So the element's representation, its bounds and every constant stay as they were."""
    ml = [(mod >> (W * i)) & MW for i in range(NW)]
    assert sum(v << (W * i) for i, v in enumerate(ml)) == mod
    ninv = (-pow(mod, -1, 1 << W)) % (1 << W)  # -mod^-1 mod 2^56
    special = ninv == 1
    assert special == (mod == P)
    out = []
    out.append(f"pub fn {name}[&w](w: &!w [int], x: int, {'' if square else 'y: int, '}d: int) -> [] int {{")
    out.append("    let xs = w[x..x + 10];")
    if not square:
        out.append("    let ys = w[y..y + 10];")
    out.append("    let os = w[d..d + 10];")
    out.extend(guard(["xs", "os"] if square else ["xs", "ys", "os"]))
    for i in range(NW):
        out.append(f"    let a{i} = xs[{2 * i}] | xs[{2 * i + 1}] << {BITS};")
    if square:
        for i in range(1, NW):
            out.append(f"    let d{i} = a{i} << 1;")
    else:
        for i in range(NW):
            out.append(f"    let b{i} = ys[{2 * i}] | ys[{2 * i + 1}] << {BITS};")
    L = MW  # the largest packed limb of a tight input
    mmax = MW
    cmax = 0
    for k in range(2 * NW):
        terms = []  # (a, b, largest product)
        lo, hi = max(0, k - (NW - 1)), min(k, NW - 1)
        cross = [(i, k - i) for i in range(lo, hi + 1)]
        if square:
            for (i, j) in cross:
                if i == j:
                    terms.append((f"a{i}", f"a{i}", L * L))
                elif i < j:
                    terms.append((f"a{i}", f"d{j}", L * 2 * L))
        else:
            for (i, j) in cross:
                terms.append((f"a{i}", f"b{j}", L * L))
        if k < NW:
            ms = [(i, k - i) for i in range(0, k)]
        else:
            ms = [(i, k - i) for i in range(k - (NW - 1), NW)]
        for (i, j) in ms:
            if ml[j]:
                terms.append((f"m{i}", ml[j], mmax * ml[j]))
        h, l = "0", "c" if k > 0 else "0"
        if k == 2 * NW - 1:
            assert not terms
            out.append(f"    let r{k - NW} = c;")
            break
        total = cmax
        for t, (a, b, mx) in enumerate(terms):
            out.append(f"    let (h{k}, l{k}) = field25519_51.mac({h}, {l}, {a}, {b});")
            h, l = f"h{k}", f"l{k}"
            total += mx
        bound.see(total, f"{name} column {k}", LIMIT_ACC)
        if k < NW:
            if special:
                # p = -1 mod 2^56: m_k is the accumulator's low limb, and adding m_k * p_0 leaves the
                # accumulator's quotient by 2^56 plus m_k as the carry.
                out.append(f"    let m{k} = {l} & {MW};")
                out.append(f"    c = wrapping_add(shr56({h}, {l}), m{k});")
                cmax = (total >> W) + mmax
            else:
                out.append(f"    let m{k} = wrapping_mul({l}, {ninv}) & {MW};")
                out.append(f"    let (h{k}, l{k}) = field25519_51.mac({h}, {l}, m{k}, {ml[0]});")
                total += mmax * ml[0]
                bound.see(total, f"{name} column {k} with m", LIMIT_ACC)
                out.append(f"    c = shr56(h{k}, l{k});")
                cmax = total >> W
        else:
            out.append(f"    let r{k - NW} = {l} & {MW};")
            out.append(f"    c = shr56({h}, {l});")
            cmax = total >> W
        bound.see(cmax, f"{name} carry out of column {k}", LIMIT_WORD)
        if k == 0:
            out[-1] = out[-1].replace("    c = ", "    var c = ", 1)
    for i in range(NW):
        out.append(f"    os[{2 * i}] = r{i} & {M};")
        out.append(f"    os[{2 * i + 1}] = r{i} >> {BITS};")
    out.append("    return 0;")
    out.append("}")
    return "\n".join(out)


def add_kernel(bound):
    c = 0
    for i in range(N):
        v = 2 * M + c
        bound.see(v, f"add limb {i}", LIMIT_SUM)
        c = v >> BITS
    out = ["pub fn add[&w](w: &!w [int], x: int, y: int, d: int) -> [] int {"]
    out.append("    let xs = w[x..x + 10];")
    out.append("    let ys = w[y..y + 10];")
    out.append("    let os = w[d..d + 10];")
    out.extend(guard(["xs", "ys", "os"]))
    out.append("    var c = 0;")
    out.append("    var v = 0;")
    for i in range(N):
        s = f"wrapping_add(xs[{i}], ys[{i}])"
        out.append(f"    v = {f'wrapping_add({s}, c)' if i else s};")
        if i < N - 1:
            out.append(f"    os[{i}] = v & {M};")
            out.append(f"    c = v >> {BITS};")
        else:
            out.append(f"    os[{i}] = v;")
    out.append("    return 0;")
    out.append("}")
    return "\n".join(out)


def redundant(v):
    """The limbs of v with 2^29 - 2 added to every limb above the first and 2^29 to the first
    (and 2 taken from the top one): the same number, every limb at least 2^28."""
    l = limbs(v)
    d = [l[i] + (1 << 29) - (2 if i else 0) for i in range(N - 1)]
    d[0] = l[0] + (1 << 29)
    d.append(l[N - 1] - 2)
    assert sum(x << (BITS * i) for i, x in enumerate(d)) == v
    assert all(x >= 1 << BITS for x in d[:-1])
    return d


def sub_kernel(k_mult, name, bound):
    d = redundant(k_mult * P)
    c = 0
    for i in range(N):
        v = M + d[i] + c
        bound.see(v, f"{name} limb {i}", LIMIT_SUM)
        c = v >> BITS
    out = [f"pub fn {name}[&w](w: &!w [int], x: int, y: int, d: int) -> [] int {{"]
    out.append("    let xs = w[x..x + 10];")
    out.append("    let ys = w[y..y + 10];")
    out.append("    let os = w[d..d + 10];")
    out.extend(guard(["xs", "ys", "os"]))
    out.append("    var c = 0;")
    out.append("    var v = 0;")
    for i in range(N):
        s = f"wrapping_sub(wrapping_add(xs[{i}], {d[i]}), ys[{i}])"
        out.append(f"    v = {f'wrapping_add({s}, c)' if i else s};")
        if i < N - 1:
            out.append(f"    os[{i}] = v & {M};")
            out.append(f"    c = v >> {BITS};")
        else:
            out.append(f"    os[{i}] = v;")
    out.append("    return 0;")
    out.append("}")
    return "\n".join(out)


def canon_kernel(name, mod):
    """d = x - mod when x >= mod, else x, for x <= mod (tight limbs), without a branch."""
    ml = limbs(mod)
    out = [f"pub fn {name}[&w](w: &!w [int], x: int, d: int) -> [] int {{"]
    out.append("    var u = 0;")
    out.append("    var v = 0;")
    for i in range(N):
        out.append(f"    v = w[x + {i}] - {ml[i]} - u;")
        out.append("    u = v >> 63 & 1;")
    out.append("    // All ones when x >= mod; through the barrier so the optimiser cannot make the")
    out.append("    // subtraction below a branch on it (docs/value-barrier.md).")
    out.append("    let m = value_barrier(u - 1);")
    out.append("    u = 0;")
    for i in range(N):
        out.append(f"    v = w[x + {i}] - ({ml[i]} & m) - u;")
        out.append(f"    w[d + {i}] = v & {M};")
        out.append("    u = v >> 63 & 1;")
    out.append("    return 0;")
    out.append("}")
    return "\n".join(out)


def consts():
    r2p = R * R % P
    r2n = R * R % ORDER
    table = [
        ("p", P),
        ("one", 1),
        ("one_mont", R % P),
        ("r2", r2p),
        ("b_mont", B * R % P),
        ("n", ORDER),
        ("n_r2", r2n),
        ("p_minus_2", P - 2),
        ("n_minus_2", ORDER - 2),
    ]
    return table


def const_static(table):
    out = ["pub static consts: [int] {"]
    out.append(f"    let t = alloc_slice[static]({N * len(table)}, 0);")
    for ci, (name, v) in enumerate(table):
        out.append(f"    // {ci}: {name}")
        for i, l in enumerate(limbs(v)):
            out.append(f"    t[{ci * N + i}] = {l};")
    out.append("    return t;")
    out.append("}")
    out.append("")
    out.append("// Limb `i` of the constants (another module reaches a static through a function).")
    out.append("pub fn cst(i: int) -> [] int {")
    out.append("    return consts[i];")
    out.append("}")
    names = []
    for ci, (name, v) in enumerate(table):
        names.append(f"pub fn c_{name}() -> [] int {{\n    return {ci * N};\n}}")
    return "\n".join(out), "\n\n".join(names)


def render():
    bound = Bound()
    parts = []
    hdr = f"""edition 7;
module std.p256_kernels;
import std.field25519_51;

// GENERATED by `scripts/p256_gen.py`; do not edit. The test `p256_kernels_are_generated`
// (and `python3 scripts/p256_gen.py --check`) fails when this file is not what the script prints.
//
// `std.p256_kernels` -- the straight-line field arithmetic under `std.p256`
// (`docs/p256-fast.md` §4): ten limbs of 28 bits, one to a word of the caller's `work`,
// Montgomery radix R = 2^280, for P-256's prime p and for its group order n.
//
// Invariants (§4.3), every function of this file:
// - an input is tight: every limb below 2^28;
// - an output is tight;
// - `mul_*` and `sqr_*` answer x * y / R reduced to below p (n) + x * y / R^2 * p, which is under
//   2p (2n) whenever x * y < 2^24 * p^2 (2^24 * n^2): an input may be up to 4096 p;
// - `add` is the sum, not reduced; `sub_K` is x - y + K p, for y below K p (K = 2, 4, 8, 16, 32, 64);
// - nothing here branches on, or indexes by, a value: every loop is unrolled.
//
// The multiplications (§11) pack each pair of 28-bit limbs into one of 56 bits, take every
// product whole with `mul_wide` into a two-word accumulator (`field25519_51.mac`, X25519's), and reduce in radix
// 2^56: five steps, the same R. Their accumulators are proved, below, to stay under 2^127 and
// their carries under 2^63 for tight inputs (the script refuses to write a kernel for which one
// cannot be), so the `wrapping_*` cannot wrap. `add` and `sub_K` stay on 28-bit limbs, also
// `wrapping_*`, with every limb's sum below 2^31 (checked below too). Each kernel tests its
// slices' lengths once, which no call fails: it lets LLVM drop the bounds check of every index.
"""
    parts.append(hdr)
    cs, names = const_static(consts())
    parts.append(cs)
    parts.append(names)
    parts.append(HELPERS)
    parts.append(wide_mont_mul("mul_p", P, False, bound))
    parts.append(wide_mont_mul("sqr_p", P, True, bound))
    parts.append(wide_mont_mul("mul_n", ORDER, False, bound))
    parts.append(wide_mont_mul("sqr_n", ORDER, True, bound))
    parts.append(add_kernel(bound))
    for k in SUB_KS:
        parts.append(sub_kernel(k, f"sub_{k}", bound))
    parts.append(canon_kernel("canon_p", P))
    parts.append(canon_kernel("canon_n", ORDER))
    text = "\n\n".join(parts) + "\n"
    note = (f"// Largest accumulator in any column, tight inputs: {bound.note(LIMIT_ACC)}.\n"
            f"// Largest carry between columns: {bound.note(LIMIT_WORD)}.\n"
            f"// Largest limb of an addition or subtraction before its carry: {bound.note(LIMIT_SUM)}.\n")
    return text.replace("\n\npub static consts", "\n" + note + "\npub static consts", 1)


HELPERS = """// Bits 56.. of the accumulator `hi * 2^64 + lo`, as a word. `lo >> 56` is arithmetic, so its
// eight meaningful bits are masked.
fn shr56(hi: int, lo: int) -> [] int {
    return lo >> 56 & 255 | hi << 8;
}"""


if __name__ == "__main__":
    text = render()
    if "--check" in sys.argv:
        if OUT.read_text() != text:
            print("std/p256_kernels.cho is not what scripts/p256_gen.py prints")
            sys.exit(1)
        print("std/p256_kernels.cho is current")
    else:
        OUT.write_text(text)
        print(f"wrote {OUT} ({len(text.splitlines())} lines)")
