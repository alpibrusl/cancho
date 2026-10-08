#!/usr/bin/env python3
"""Generates `std/p256_kernels.cho`, the straight-line field kernels of
`std.p256` (docs/p256-fast.md §4), and proves the bounds the code relies on.

    python3 scripts/p256_gen.py            # rewrites std/p256_kernels.cho
    python3 scripts/p256_gen.py --check    # fails when the file is not what this prints

A field element is ten limbs of 28 bits, least significant first, one limb to
a word of a caller's `work`, Montgomery radix R = 2^280. The multiplications
are product scanning (every column is summed in a register before one carry is
taken), written as `wrapping_*`: the script computes, for each column, the
largest value the accumulator can hold when every input limb is at its largest
(2^28 - 1), and refuses to write a kernel in which one reaches 2^63. So the
`wrapping_*` cannot wrap, and a wrong bound is a failed generation, not a
silent wrong answer (docs/p256-fast.md §4.3).

P-256's prime is p = 2^256 - 2^224 + 2^192 + 2^96 - 1. Its limbs in radix 2^28
are all of the form 2^k - 1 or 2^k (or zero), and p = -1 mod 2^28, so
-p^-1 = 1: the Montgomery quotient digit is the low limb itself, and m * p_j is
a shift and a subtraction, no multiplication (§4.2).
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


def form(v):
    """How to multiply m by the constant v: ('zero',), ('pow', k), ('ones', k) or ('mul', v)."""
    if v == 0:
        return ("zero",)
    if v & (v - 1) == 0:
        return ("pow", v.bit_length() - 1)
    if v & (v + 1) == 0:
        return ("ones", v.bit_length())
    return ("mul", v)


class Bound:
    """The largest value a column can reach, tracked while emitting."""

    def __init__(self):
        self.worst = 0
        self.worst_at = None

    def see(self, v, where):
        if v > self.worst:
            self.worst, self.worst_at = v, where
        assert v < (1 << 63), f"accumulator can reach 2^63 at {where}"


def mont_mul(name, mod, square, bound):
    """A Montgomery multiplication kernel modulo `mod` (p or the order)."""
    ml = limbs(mod)
    ninv = (-pow(mod, -1, 1 << BITS)) % (1 << BITS)  # -mod^-1 mod 2^28
    special = mod == P
    assert not special or ninv == 1
    out = []
    out.append(f"pub fn {name}[&w](w: &!w [int], x: int, {'' if square else 'y: int, '}d: int) -> [] int {{")
    out.append("    let xs = w[x..x + 10];")
    for i in range(N):
        out.append(f"    let a{i} = xs[{i}];")
    if not square:
        out.append("    let ys = w[y..y + 10];")
        for i in range(N):
            out.append(f"    let b{i} = ys[{i}];")
    out.append("    let os = w[d..d + 10];")
    b = "a" if square else "b"
    out.append("    var s = 0;")
    out.append("    var c = 0;")
    cmax = 0
    L = M
    mmax = M
    for k in range(2 * N):
        pre = []  # let statements
        terms = []  # (expression, largest value)
        if k > 0:
            terms.append(("c", cmax))
        lo, hi = max(0, k - (N - 1)), min(k, N - 1)
        cross = [(i, k - i) for i in range(lo, hi + 1)]
        if square:
            pairs = [(i, j) for (i, j) in cross if i < j]
            if pairs:
                e = f"wrapping_mul(a{pairs[0][0]}, a{pairs[0][1]})"
                for (i, j) in pairs[1:]:
                    e = f"wrapping_add({e}, wrapping_mul(a{i}, a{j}))"
                terms.append((f"{e} << 1", 2 * len(pairs) * L * L))
            for (i, j) in cross:
                if i == j:
                    terms.append((f"wrapping_mul(a{i}, a{i})", L * L))
        else:
            for (i, j) in cross:
                terms.append((f"wrapping_mul(a{i}, b{j})", L * L))
        # Reduction terms: m_i * mod_j for i + j = k, i < k below column N, i >= k - (N-1) above.
        if k < N:
            ms = [(i, k - i) for i in range(0, k)]
        else:
            ms = [(i, k - i) for i in range(k - (N - 1), N)]
        if special:
            groups = {}
            for (i, j) in ms:
                f = form(ml[j])
                if f[0] != "zero":
                    groups.setdefault(f, []).append(i)
            for gi, (f, idx) in enumerate(sorted(groups.items())):
                if len(idx) > 1:
                    sm = f"g{k}_{gi}"
                    pre.append(f"    let {sm} = {' + '.join(f'm{i}' for i in idx)};")
                else:
                    sm = f"m{idx[0]}"
                if f[0] == "ones":
                    terms.append((f"wrapping_sub({sm} << {f[1]}, {sm})", len(idx) * mmax * ((1 << f[1]) - 1)))
                elif f[0] == "pow":
                    terms.append((f"{sm} << {f[1]}", len(idx) * mmax * (1 << f[1])))
                else:
                    raise AssertionError(f)
        else:
            for (i, j) in ms:
                if ml[j]:
                    terms.append((f"wrapping_mul(m{i}, {ml[j]})", mmax * ml[j]))
        out.extend(pre)
        for ti, (e, _) in enumerate(terms):
            out.append(f"    s = {e};" if ti == 0 else f"    s = wrapping_add(s, {e});")
        total = sum(mx for _, mx in terms)
        bound.see(total, f"{name} column {k}")
        if k < N:
            if special:
                # P0 = 2^28 - 1 = -1 mod 2^28: m_k is the low limb of s; adding m_k * P0 leaves
                # s's quotient by 2^28 plus m_k as the carry.
                out.append(f"    let m{k} = s & {M};")
                out.append(f"    c = wrapping_add(s >> {BITS}, m{k});")
                cmax = (total >> BITS) + mmax
            else:
                out.append(f"    let m{k} = wrapping_mul(s & {M}, {ninv}) & {M};")
                out.append(f"    s = wrapping_add(s, wrapping_mul(m{k}, {ml[0]}));")
                total += mmax * ml[0]
                bound.see(total, f"{name} column {k} with m")
                out.append(f"    c = s >> {BITS};")
                cmax = total >> BITS
        elif k < 2 * N - 1:
            out.append(f"    os[{k - N}] = s & {M};")
            out.append(f"    c = s >> {BITS};")
            cmax = total >> BITS
        else:
            out.append(f"    os[{k - N}] = s;")
    out.append("    return 0;")
    out.append("}")
    return "\n".join(out)


def add_kernel():
    out = ["pub fn add[&w](w: &!w [int], x: int, y: int, d: int) -> [] int {"]
    out.append("    let xs = w[x..x + 10];")
    out.append("    let ys = w[y..y + 10];")
    out.append("    let os = w[d..d + 10];")
    out.append("    var c = 0;")
    out.append("    var v = 0;")
    for i in range(N):
        out.append(f"    v = xs[{i}] + ys[{i}]{' + c' if i else ''};")
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


def sub_kernel(k_mult, name="sub"):
    d = redundant(k_mult * P)
    out = [f"pub fn {name}[&w](w: &!w [int], x: int, y: int, d: int) -> [] int {{"]
    out.append("    let xs = w[x..x + 10];")
    out.append("    let ys = w[y..y + 10];")
    out.append("    let os = w[d..d + 10];")
    out.append("    var c = 0;")
    out.append("    var v = 0;")
    for i in range(N):
        out.append(f"    v = xs[{i}] + {d[i]} - ys[{i}]{' + c' if i else ''};")
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
    hdr = f"""edition 6;
module std.p256_kernels;

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
// The multiplications are `wrapping_*` because their accumulators are proved, below, to stay under
// 2^63 for tight inputs (the script refuses to write a kernel for which one cannot be).
"""
    parts.append(hdr)
    cs, names = const_static(consts())
    parts.append(cs)
    parts.append(names)
    parts.append(mont_mul("mul_p", P, False, bound))
    worst_p = (bound.worst, bound.worst_at)
    parts.append(mont_mul("sqr_p", P, True, bound))
    parts.append(mont_mul("mul_n", ORDER, False, bound))
    parts.append(mont_mul("sqr_n", ORDER, True, bound))
    parts.append(add_kernel())
    for k in SUB_KS:
        parts.append(sub_kernel(k, f"sub_{k}"))
    parts.append(canon_kernel("canon_p", P))
    parts.append(canon_kernel("canon_n", ORDER))
    text = "\n\n".join(parts) + "\n"
    note = f"// Largest accumulator in any column, tight inputs: {bound.worst.bit_length() - 1}.{(bound.worst * 100 >> (bound.worst.bit_length() - 1)) % 100:02d} bits (limit 63), {bound.worst_at}.\n"
    return text.replace("\n\npub static consts", "\n" + note + "\npub static consts", 1)


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
