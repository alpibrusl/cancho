#!/usr/bin/env python3
"""Mutation check of the new P-256 code (docs/p256-fast.md §8).

    python3 scripts/p256_mutants.py [--list] [<name fragment> ...]

Each mutant is one of `std/p256*.cho`, `std/ecdh.cho`, `std/ecdsa.cho` or
`std/ecdsa_sign.cho`, or of the key tables' users in `packages/x509` and
`packages/tls` (docs/p256-fast.md §11), with one deliberate bug. The repository is copied to a
scratch directory (the worktree is never touched), the compiler is rebuilt
there with the mutant embedded, and the evidence is run:

1. `cargo test -p cancho --test conformance -- p256 ecdh ecdsa x509_verify
   key_tables`: the field, the table and the wNAF against Python
   (`tests/vectors/p256.txt`, with every verification also against the key's
   table), the Wycheproof and CAVP cases of `std.ecdh`, Wycheproof and NIST
   for `std.ecdsa` (each also against the key's table), RFC 6979 A.2.5 and
   the key files of `std.ecdsa_sign`, the x509 files replayed with and
   without the anchors' tables, and the tables of `packages/tls`;
2. if those pass, the differentials: RFC 6979 in Python byte for byte
   (`ecdsa_sign_differential.py reference`, 300), OpenSSL on signatures
   (`openssl`, 100) and on verification (`ecdsa_differential.py openssl`,
   100 a curve), `ecdh_differential.py` (100).

A mutant is killed when any of them disagrees. The unmutated tree is run
first and must pass. A mutant that does not build is an error, not a kill.
`EQUIVALENT` names the mutants argued to change nothing observable. Needs
`openssl` and pyca/cryptography. Exit status 1 if a mutant survives that is
not argued equivalent, or does not build.
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (file, function whose body is mutated, name, old, new). The first occurrence
# of `old` inside the function is replaced; an `old` that is not there is an error.
M = []


def mut(file, fn, name, old, new):
    M.append((file, fn, name, old, new))


K = "std/p256_kernels.cho"
F = "std/p256.cho"
PT = "std/p256_pt.cho"
VF = "std/p256_vf.cho"
CB = "std/p256_comb.cho"
EH = "std/ecdh.cho"
EA = "std/ecdsa.cho"
ES = "std/ecdsa_sign.cho"

# ---- the generated kernels ----
mut(K, "mul_p", "mul_p: a cross product dropped", "    let (h3, l3) = mac(h3, l3, a1, b2);\n", "")
mut(K, "mul_p", "mul_p: the quotient digit's carry not added", "c = wrapping_add(shr56(h0, l0), m0);", "c = shr56(h0, l0);")
mut(K, "mul_p", "mul_p: the 2^24 limb of p doubled", "mac(h3, l3, m0, 16777216)", "mac(h3, l3, m0, 33554432)")
mut(K, "mul_p", "mul_p: the 2^40 - 1 limb of p off by one", "mac(h1, l1, m0, 1099511627775)", "mac(h1, l1, m0, 1099511627776)")
mut(K, "mul_p", "mul_p: the 2^32 - 1 limb of p off by one", "mac(h4, l4, m0, 4294967295)", "mac(h4, l4, m0, 4294967296)")
mut(K, "mul_p", "mul_p: an input limb packed at 27 bits", "let a1 = xs[2] | xs[3] << 28;", "let a1 = xs[2] | xs[3] << 27;")
mut(K, "mul_p", "mul_p: an output limb not masked", "os[0] = r0 & 268435455;", "os[0] = r0;")
mut(K, "mul_p", "mul_p: the top limb of the answer dropped", "os[9] = r4 >> 28;", "os[9] = 0;")
# The two-word accumulator the kernels sum their columns in is `std.field25519_51`'s (#381).
mut("std/field25519_51.cho", "mac", "mac: the low word's carry dropped", "return (wrapping_add(wrapping_add(hi, h), c), s);", "return (wrapping_add(hi, h), s);")
mut(K, "shr56", "shr56: a carry taken at 55 bits", "return lo >> 56 & 255 | hi << 8;", "return lo >> 55 & 511 | hi << 9;")
mut(K, "shr56", "shr56: the low word's bits not masked", "return lo >> 56 & 255 | hi << 8;", "return lo >> 56 | hi << 8;")
mut(K, "sqr_p", "sqr_p: the doubling of a cross term dropped", "let d2 = a2 << 1;", "let d2 = a2;")
mut(K, "sqr_p", "sqr_p: a square left out", "    let (h4, l4) = mac(h4, l4, a2, a2);\n", "")
mut(K, "sqr_p", "sqr_p: a cross term of a column missing", "    let (h5, l5) = mac(h5, l5, a2, d3);\n", "")
mut(K, "mul_n", "mul_n: n' off by one", "wrapping_mul(l0, 59048906595548239)", "wrapping_mul(l0, 59048906595548240)")
mut(K, "sqr_n", "sqr_n: a limb of n changed", "mac(h0, l0, m0, 52295809498293585)", "mac(h0, l0, m0, 52295809498293586)")
mut(K, "mul_n", "mul_n: the quotient not masked", "let m1 = wrapping_mul(l1, 59048906595548239) & 72057594037927935;", "let m1 = wrapping_mul(l1, 59048906595548239);")
mut(K, "add", "add: the carry dropped", "v = wrapping_add(wrapping_add(xs[1], ys[1]), c);", "v = wrapping_add(xs[1], ys[1]);")
mut(K, "sub_4", "sub_4: the carry dropped", "v = wrapping_add(wrapping_sub(wrapping_add(xs[3], 536887293), ys[3]), c);", "v = wrapping_sub(wrapping_add(xs[3], 536887293), ys[3]);")
mut(K, "add", "add: the top limb masked", "os[9] = v;", "os[9] = v & 268435455;")
mut(K, "canon_p", "canon_p: the subtraction not masked", "v = w[x + 0] - (268435455 & m) - u;", "v = w[x + 0] - 268435455 - u;")
mut(K, "canon_p", "canon_p: the borrow of limb 3 lost", "v = w[x + 3] - 4095 - u;\n    u = v >> 63 & 1;", "v = w[x + 3] - 4095 - u;\n    u = 0;")
mut(K, "canon_p", "canon_p: subtracts always", "let m = value_barrier(u - 1);", "let m = value_barrier(0 - 1);")
mut(K, "canon_n", "canon_n: never subtracts", "let m = value_barrier(u - 1);", "let m = value_barrier(0);")
# Constants: a limb of each of R^2 mod p, b R mod p, R^2 mod n, p - 2, n - 2.
mut(K, "", "consts: a limb of R^2 mod p changed", "    t[30] = 327680;", "    t[30] = 327681;")
mut(K, "", "consts: a limb of b R mod p changed", "    // 4: b_mont\n    t[40] =", "    // 4: b_mont\n    t[40] = 1 +")
mut(K, "", "consts: a limb of R^2 mod n changed", "    // 6: n_r2\n    t[60] =", "    // 6: n_r2\n    t[60] = 1 +")
mut(K, "", "consts: a limb of p - 2 changed", "    // 7: p_minus_2\n    t[70] = 268435453;", "    // 7: p_minus_2\n    t[70] = 268435452;")
mut(K, "", "consts: a limb of n - 2 changed", "    // 8: n_minus_2\n    t[80] =", "    // 8: n_minus_2\n    t[80] = 1 +")
mut(K, "", "consts: R mod p changed", "    // 2: one_mont\n    t[20] = 16777216;", "    // 2: one_mont\n    t[20] = 16777217;")
mut(CB, "", "comb: the first point's x changed", 'let s = "', 'let s = "1')
mut(CB, "", "odd: the first point changed", "pub static odd: [int] {\n    let t = alloc_slice[static](2560, 0);\n    let s = \"", "pub static odd: [int] {\n    let t = alloc_slice[static](2560, 0);\n    let s = \"1")
# ---- std.p256 ----
mut(F, "load", "load: no straddle at offset 24", "if off > 20 {", "if off > 24 {")
mut(F, "load", "load: a byte not masked to its limb", "w[d + idx] = w[d + idx] | v << off & mask();", "w[d + idx] = w[d + idx] | v << off;")
mut(F, "load", "load: the high bits of a straddling byte dropped", "w[d + idx + 1] = w[d + idx + 1] | v >> 28 - off;", "w[d + idx + 1] = w[d + idx + 1];")
mut(F, "store", "store: no straddle at offset 24", "if off > 20 {", "if off > 24 {")
mut(F, "store", "store: the next limb's bits shifted wrong", "v = v | w[x + idx + 1] << 28 - off;", "v = v | w[x + idx + 1] << 27 - off;")
mut(F, "below", "below: >= taken for <", "if v < m {\n            return true;\n        }\n        if v > m {\n            return false;\n        }", "if v > m {\n            return true;\n        }\n        if v < m {\n            return false;\n        }")
mut(F, "below", "below: equal taken for below", "        i = i - 1;\n    }\n    return false;", "        i = i - 1;\n    }\n    return true;")
mut(F, "is_zero", "is_zero: ignores the top limb", "while i < 10 {\n        any = any | w[x + i];", "while i < 9 {\n        any = any | w[x + i];")
mut(F, "pow", "pow: one squaring a window too few", "while s < 4 {", "while s < 3 {")
mut(F, "pow", "pow: the table's first multiple wrong", "copy(w, x, fe(table() + 1));", "copy(w, x, fe(table() + 2));")
mut(F, "pow", "pow: a nonzero nibble not multiplied", "            if nib != 0 {\n                if scalar {\n                    smul(", "            if nib != 100 {\n                if scalar {\n                    smul(")
mut(F, "pow", "pow: the exponent's nibble from the wrong limb", "let limb = p256_kernels.cst(which + 4 * k / 28);", "let limb = p256_kernels.cst(which + 4 * k / 30);")
mut(F, "pow", "pow: the leading nibble taken twice", "copy(w, fe(table() + nib), d);\n            started = true;", "copy(w, fe(table() + nib), d);\n            started = false;")
mut(F, "invert", "invert: the exponent n - 2", "p256_kernels.c_p_minus_2()", "p256_kernels.c_n_minus_2()")
mut(F, "sinvert", "sinvert: the exponent p - 2", "p256_kernels.c_n_minus_2()", "p256_kernels.c_p_minus_2()")
mut(F, "from_mont", "from_mont: not reduced", "return p256_kernels.canon_p(w, d, d);", "return 0;")
mut(F, "sfrom_mont", "sfrom_mont: not reduced", "return p256_kernels.canon_n(w, d, d);", "return 0;")
mut(F, "sreduce", "sreduce: does nothing", "return p256_kernels.canon_n(w, x, d);", "return copy(w, x, d);")
mut(F, "to_mont", "to_mont: multiplies by 1", "return p256_kernels.mul_p(w, x, r2(), d);", "return p256_kernels.mul_p(w, x, one(), d);")
mut(F, "sto_mont", "sto_mont: multiplies by R^2 mod p", "return p256_kernels.mul_n(w, x, n_r2(), d);", "return p256_kernels.mul_n(w, x, r2(), d);")
mut(F, "init", "init: b in Montgomery form not set", "    put_const(w, p256_kernels.c_b_mont(), b_mont());\n", "    put_const(w, p256_kernels.c_one(), b_mont());\n")
mut(F, "put_n", "put_n: p for n", "return put_const(w, p256_kernels.c_n(), d);", "return put_const(w, p256_kernels.c_p(), d);")

# ---- std.p256_pt: the ladder, the comb, the points ----
mut(PT, "add_points", "add_points: a product's operand changed", "p256.mul(w, t(3), t(4), t(3));", "p256.mul(w, t(3), t(3), t(3));")
mut(PT, "add_points", "add_points: b dropped from a product", "p256.mul(w, b, y3, y3);", "p256.mul(w, y3, y3, y3);")
mut(PT, "add_points", "add_points: a sum for a difference", "p256.add(w, t(0), t(1), t(4));", "p256.sub16(w, t(0), t(1), t(4));")
mut(PT, "double_point", "double_point: a product's operand changed", "p256.mul(w, x, y, t(3));", "p256.mul(w, x, x, t(3));")
mut(PT, "double_point", "double_point: a doubling left out", "p256.add(w, z3, z3, z3);\n    p256.add(w, z3, z3, z3);\n    return", "p256.add(w, z3, z3, z3);\n    return")
mut(PT, "mixed_add", "mixed_add: Z1 copied for the wrong element", "p256.copy(w, z1, t(2));", "p256.copy(w, z1, t(1));")
mut(PT, "mixed_add", "mixed_add: Y2 Z1 for Y2", "p256.mul(w, y2, z1, t(4));", "p256.mul(w, y2, y2, t(4));")
mut(PT, "mixed_add", "mixed_add: X2 Z1 for X2", "p256.mul(w, x2, z1, y3);", "p256.mul(w, x2, x2, y3);")
mut(PT, "select", "select: the mask of entry 15 for 14", "let m = value_barrier(eq_mask(i, idx));", "let m = value_barrier(eq_mask(i % 15, idx));")
mut(PT, "select", "select: one word of an entry short", "while j < 30 {\n            sl[j] = sl[j] | en[j] & m;", "while j < 29 {\n            sl[j] = sl[j] | en[j] & m;")
mut(PT, "multiply", "multiply: the table's multiple 2 skipped", "var i = 2;\n    while i < 16 {", "var i = 3;\n    while i < 16 {")
mut(PT, "multiply", "multiply: three doublings a window", "    double_point(w, pt_acc(), pt_acc());\n        double_point(w, pt_acc(), pt_acc());\n        double_point(w, pt_acc(), pt_acc());\n        double_point(w, pt_acc(), pt_acc());", "    double_point(w, pt_acc(), pt_acc());\n        double_point(w, pt_acc(), pt_acc());\n        double_point(w, pt_acc(), pt_acc());")
mut(PT, "multiply", "multiply: the low nibble first", "let shift = 4 - 4 * (at % 2);", "let shift = 4 * (at % 2);")
mut(PT, "recode", "recode: the carry threshold 6", "let c = v + 7 >> 4;", "let c = v + 6 >> 4;")
mut(PT, "recode", "recode: the carry threshold 9 (digits to +9)", "let c = v + 7 >> 4;", "let c = v + 8 >> 4;")
mut(PT, "recode", "recode: the final carry dropped", "w[dig + 64] = carry;", "w[dig + 64] = 0;")
mut(PT, "recode", "recode: nibbles from the wrong end of the scalar", "scalar[31 - i / 2]", "scalar[i / 2]")
mut(PT, "comb_multiply", "comb_multiply: a negative digit not negated", "let neg = value_barrier(d >> 63);", "let neg = value_barrier(0);")
mut(PT, "comb_multiply", "comb_multiply: entry j for digit j", "let pick = eq_mask(j + 1, mag) | eq_mask(j, 0) & eq_mask(mag, 0);", "let pick = eq_mask(j, mag) | eq_mask(j, 0) & eq_mask(mag, 0);")
mut(PT, "comb_multiply", "comb_multiply: the zero digit adds the dummy", "let keep = value_barrier(eq_mask(mag, 0));", "let keep = value_barrier(eq_mask(mag, 100));")
mut(PT, "comb_multiply", "comb_multiply: position i + 1 of the table", "let row = p256_comb.comb_row((8 * i + j) * 20);", "let row = p256_comb.comb_row((8 * (i + 1) % 65 + j) * 20);")
mut(PT, "comb_multiply", "comb_multiply: the last position left out", "while i < 65 {", "while i < 64 {")
mut(PT, "comb_multiply", "comb_multiply: y not selected for the negation", "w[y_of(sel) + t] = y ^ (y ^ w[z_of(sel) + t]) & neg;", "w[y_of(sel) + t] = y;")
mut(PT, "comb_multiply", "comb_multiply: the accumulator not kept for a zero digit", "w[x_of(pt_acc()) + t] = new ^ (old ^ new) & keep;", "w[x_of(pt_acc()) + t] = new;")
mut(PT, "load_point", "load_point: the range check dropped", "if !p256.below_p(w, x_of(p)) || !p256.below_p(w, y_of(p)) {", "if false {")
mut(PT, "load_point", "load_point: only x checked", "|| !p256.below_p(w, y_of(p))", "")
mut(PT, "on_curve_xy", "on_curve_xy: -3x for -2x", "p256.add(w, s2, x, s2);", "p256.add(w, s2, y, s2);")
mut(PT, "on_curve_xy", "on_curve_xy: b not added", "p256.add(w, s1, p256.b_mont(), s1);", "p256.add(w, s1, s1, s1);")
mut(PT, "affine", "affine: the infinity test dropped", "if p256.is_zero(w, t(0)) {\n        return -58;\n    }", "")
mut(PT, "affine", "affine: y by x's inverse twice", "p256.mul(w, y_of(a), t(0), t(1));", "p256.mul(w, y_of(a), t(1), t(1));")
mut(PT, "point_multiply", "point_multiply: the curve check dropped", "if !on_curve(w) {\n        return -56;\n    }", "")
mut(PT, "base_multiply", "base_multiply: the work not wiped", "p256.wipe(w, 0, work_len());", "")

# ---- std.p256_vf: verification ----
mut(VF, "double", "double (Jacobian): alpha for 2 alpha", "p256.add(w, t(3), t(3), t(4));", "p256.add(w, t(3), t(2), t(4));")
mut(VF, "double", "double (Jacobian): Z3 without the delta", "p256.sub2(w, t(10), t(0), t(10));", "")
mut(VF, "add_jac", "add_jac: U1 from the wrong Z", "p256.mul(w, x_of(p), t(1), t(2));", "p256.mul(w, x_of(p), t(0), t(2));")
mut(VF, "add_jac", "add_jac: Z3 without H", "p256.mul(w, t(3), t(6), t(3));", "p256.copy(w, t(3), t(3));")
mut(VF, "add_jac", "add_jac: an equal pair not doubled", "if zero_mod(w, t(7), t(13)) {\n            return double(w, p, d);\n        }", "if zero_mod(w, t(7), t(13)) {\n            return 1;\n        }")
mut(VF, "add_jac", "add_jac: H = 0 not detected", "if zero_mod(w, t(6), t(13)) {", "if false {")
mut(VF, "madd", "madd: S2 without Z1", "p256.mul(w, y2, z1, t(2));", "p256.copy(w, y2, t(2));")
mut(VF, "madd", "madd: Z3 without HH", "p256.sub2(w, t(12), t(5), t(12));", "")
mut(VF, "madd", "madd: H = 0 not detected", "if zero_mod(w, t(3), t(13)) {", "if false {")
mut(VF, "wnaf", "wnaf: the window threshold", "if d >= 1 << width - 1 {", "if d > 1 << width - 1 {")
mut(VF, "wnaf", "wnaf: the carry added a bit early", "add_bit(w, x, i + width);", "add_bit(w, x, i + width - 1);")
mut(VF, "wnaf", "wnaf: the digit window one bit wide", "let d = bits(w, x, i, width);", "let d = bits(w, x, i, width - 1);")
mut(VF, "wnaf", "wnaf: skips width + 1", "i = i + width;\n        }", "i = i + width + 1;\n        }")
mut(VF, "bits", "bits: the high limb not read", "if off + width > 28 && idx + 1 < 10 {", "if off + width > 99 && idx + 1 < 10 {")
mut(VF, "add_bit", "add_bit: no ripple", "while v >> 28 != 0 && idx < 9 {", "while false {")
mut(VF, "load_g", "load_g: a negative digit not negated", "if d < 0 {\n        p256.sub2(", "if d < -1000 {\n        p256.sub2(")
mut(VF, "load_g", "load_g: the table entry (k - 1)", "let at = (k - 1) / 2 * 20;", "let at = (k - 1) / 2 * 20 + 20;")
mut(VF, "load_q", "load_q: a negative digit not negated", "if d < 0 {\n        p256.sub32(", "if d < -1000 {\n        p256.sub32(")
mut(VF, "load_q", "load_q: the wrong multiple", "copy_point(w, qtab((k - 1) / 2), pt_ent());", "copy_point(w, qtab(k / 2), pt_ent());")
mut(VF, "matches", "matches: r + n never tried", "if p256.below_p(w, t(3)) {", "if false {")
mut(VF, "matches", "matches: r + n tried when it is not below p", "if p256.below_p(w, t(3)) {", "if true {")
mut(VF, "matches", "matches: Z^2 forgotten", "p256.mul(w, t(1), t(0), t(1));\n    p256.sub", "p256.copy(w, t(1), t(1));\n    p256.sub")
mut(VF, "prepare", "verify: s = 0 accepted", "if !p256.below_n(w, sm) || p256.is_zero(w, sm) {", "if !p256.below_n(w, sm) {")
mut(VF, "prepare", "verify: r = 0 accepted", "if !p256.below_n(w, rr) || p256.is_zero(w, rr) {", "if !p256.below_n(w, rr) {")
mut(VF, "prepare", "verify: s not below n accepted", "if !p256.below_n(w, sm) || p256.is_zero(w, sm) {", "if p256.is_zero(w, sm) {")
mut(VF, "prepare", "verify: Q's range not checked", "if !p256.below_p(w, x_of(q1)) || !p256.below_p(w, y_of(q1)) {", "if false {")
mut(VF, "prepare", "verify: Q's curve equation not checked", "if !p256_pt.on_curve_xy(w, x_of(q1), y_of(q1), t(0), t(1), t(2)) {", "if false {")
mut(VF, "prepare", "verify: u2 from s not r", "p256.copy(w, rr, t(2));", "p256.copy(w, sm, t(2));")
mut(VF, "verify", "verify: the 3Q multiple built from Q + Q", "if add_jac(w, qtab(k - 1), pt_q2(), qtab(k)) != 0 {", "if add_jac(w, qtab(k - 1), qtab(0), qtab(k)) != 0 {")
mut(VF, "verify", "verify: the chain starts a bit low", "var i = 256;", "var i = 255;")
mut(VF, "verify", "verify: u1's digits at width 5", "wnaf(w, e_u1(), dig1(), 7);", "wnaf(w, e_u1(), dig1(), 5);")
mut(VF, "verify", "verify: a cancelled sum not reported", "} else if madd(w, pt_acc(), pt_ent(), pt_acc()) != 0 {\n                inf = true;\n            }", "} else {\n                madd(w, pt_acc(), pt_ent(), pt_acc());\n            }")
mut(VF, "verify", "verify: infinity result accepted", "if inf {\n        return -40;\n    }", "")
mut(VF, "prepare", "verify: the digest truncated to 31 bytes", "if m > 32 {\n        m = 32;\n    }", "if m > 31 {\n        m = 31;\n    }")

# ---- a key's table (docs/p256-fast.md §11): building, verifying against it, and its users ----
TL = "packages/x509/verify.cho"
TT = "packages/tls/tls.cho"
TI = "packages/tls/identity.cho"
TS = "packages/tls/server.cho"
mut(VF, "build", "table: the point's range not checked", "if !p256.below_p(w, x_of(b)) || !p256.below_p(w, y_of(b)) {", "if false {")
mut(VF, "build", "table: the curve equation not checked", "if !p256_pt.on_curve_xy(w, x_of(b), y_of(b), t(0), t(1), t(2)) {", "if false {")
mut(VF, "build", "table: one doubling short between runs", "while d < span() {", "while d < span() - 1 {")
mut(VF, "build", "table: the point's last byte not written", "while i < 65 {\n        table[i] = point[i];", "while i < 64 {\n        table[i] = point[i];")
mut(VF, "build", "table: a refusal leaves the entries", "    if code != 0 {\n        i = 0;", "    if code != 0 {\n        i = table_len();")
mut(VF, "run", "table: an odd multiple from Q + Q", "if add_jac(w, jtab(k - 1), pt_q2(), jtab(k)) != 0 {", "if add_jac(w, jtab(k - 1), jtab(0), jtab(k)) != 0 {")
mut(VF, "run", "table: the running inverse not advanced", "            p256.mul(w, e_inv(), z_of(jtab(k)), e_inv());\n", "")
mut(VF, "run", "table: y over Z^2, not Z^3", "        p256.mul(w, e_zi2(), e_zi(), e_zi2());\n", "")
mut(VF, "run", "table: a product of the Z's skipped", "p256.mul(w, pre(k - 1), z_of(jtab(k)), pre(k));", "p256.copy(w, pre(k - 1), pre(k));")
mut(VF, "put_limbs", "table: an entry left in the Montgomery form of its Montgomery form", "    p256.from_mont(w, s, s);\n", "")
mut(VF, "put_limbs", "table: a limb's top byte dropped", "table[at + 4 * i + 3] = byte_of(v >> 24);", "table[at + 4 * i + 3] = byte_of(0);")
mut(VF, "load_tab", "table: a limb's top byte not read", "| int_of(b[4 * i + 3]) << 24;", ";")
mut(VF, "load_tab", "table: a negative digit not negated", "if d < 0 {\n        p256.sub2(", "if d < -1000 {\n        p256.sub2(")
mut(VF, "load_tab", "table: the next multiple", "let at = 65 + (per_chunk() * j + (k - 1) / 2) * entry_bytes();", "let at = 65 + (per_chunk() * j + (k + 1) / 2) * entry_bytes();")
mut(VF, "load_gj", "table: the generator's run 0 for every run", "let row = p256_comb.odd_row(640 * j + (k - 1) / 2 * 20);", "let row = p256_comb.odd_row((k - 1) / 2 * 20);")
mut(VF, "load_gj", "table: the generator's negative digit not negated", "if d < 0 {\n        p256.sub2(", "if d < -1000 {\n        p256.sub2(")
mut(VF, "build_table", "table: the work not wiped", "    p256.wipe(w, 0, table_work_len());\n", "")
mut(VF, "table_for", "table: any table taken for any point", "if table[i] != point[i] {\n            return false;", "if table[i] != point[i] && i < 0 {\n            return false;")
mut(VF, "verify", "table: a key's table not recognised", "if len(point) == table_len() {", "if len(point) == table_len() + 1 {")
mut(VF, "verify_tabled", "tabled: the chain starts a position low", "var i = span() - 1;", "var i = span() - 2;")
mut(VF, "verify_tabled", "tabled: the last run left out", "while j < chunks() {", "while j < chunks() - 1 {")
mut(VF, "verify_tabled", "tabled: u2 against the generator's runs", "load_tab(table, w, j, d2);", "load_gj(w, j, d2);")
mut(VF, "verify_tabled", "tabled: u2's digits at width 5", "wnaf(w, e_u2(), dig2(), 7);", "wnaf(w, e_u2(), dig2(), 5);")
mut(VF, "verify_tabled", "tabled: infinity result accepted", "if inf {\n        return -40;\n    }\n    if !matches", "if !matches")
mut(VF, "accumulate", "tabled: a cancelled sum not reported", "return madd(w, pt_acc(), pt_ent(), pt_acc()) != 0;", "madd(w, pt_acc(), pt_ent(), pt_acc());\n    return false;")
mut(EA, "verify", "ecdsa: a P-256 key's table refused as a point", "if curve != 256 && (len(point) != 1 + 2 * size || int_of(point[0]) != 4) {", "if len(point) != 1 + 2 * size || int_of(point[0]) != 4 {")
mut(TL, "store_tables", "anchors: a table entry's length one short", "store[end + 2] = byte_of(1 + t & 255);", "store[end + 2] = byte_of(t & 255);")
mut(TL, "store_tables", "anchors: the first P-256 root only", "built = built + 1;", "built = built + most;")
mut(TL, "anchor_table", "anchors: any table taken for any root", "&& bytes.equal(store[body + 1..body + 66], key)", "")
mut(TL, "extend", "anchors: a root verified without its table", "anchor_table(store, root[rview[x509.key_start()]..rview[x509.key_end()]])", "root[0..0]")
mut(TT, "trust", "anchors: the old store not cleared", "    tls_slot.zero(contents(engine.roots)[0..contents(engine.meta)[m_roots_len()]]);\n", "")
mut(TT, "trust", "anchors: no tables built", "contents(engine.meta)[m_roots_len()] = x509_verify.store_tables(contents(engine.roots)[0..roots_cap()], info[0], most_anchor_tables(), work);", "contents(engine.meta)[m_roots_len()] = info[0];")
mut(TI, "load", "identity: no table built", "            p256_vf.build_table(pub_point, cfg[b + o_table()..b + o_table() + p256_vf.table_len()], work);\n", "")
mut(TI, "check_key", "identity: the check without the table", "        return table(cfg, id);\n    }", "        return point(cfg, id);\n    }")
mut(TI, "check_key", "identity: a table that is not the point's used", "if p256_vf.table_for(table(cfg, id), point(cfg, id)) {", "if true {")

# ---- the callers ----
mut(EH, "public_key_p256", "ecdh P-256 public key: x and y swapped", "p256_pt.base_multiply(scalar, out[1..33], out[33..65], work)", "p256_pt.base_multiply(scalar, out[33..65], out[1..33], work)")
mut(EH, "check_p256", "ecdh P-256: the scalar's range not checked", "if ok == 0 {\n        return refused_scalar_range();\n    }", "")
mut(EH, "shared_p256", "ecdh P-256: the peer's first byte not checked", "if len(peer) != 65 || int_of(peer[0]) != 4 {", "if len(peer) != 65 {")
mut(EA, "verify", "ecdsa P-256: the fast path skipped for the wrong curve", "if curve == 256 {\n        // P-256 on", "if curve == 255 {\n        // P-256 on")
mut(ES, "finish", "sign: s = k^-1 (e + r d) without e", "    p256.sadd(w, er, sr, sr);\n", "")
mut(ES, "finish", "sign: s without k^-1", "    p256.sinvert(w, kr, kr);\n", "")
mut(ES, "finish", "sign: r not reduced mod n", "    p256.sreduce(w, rr, rr);\n", "")
mut(ES, "finish", "sign: r from the secret key", "    p256.load(x, w, rr);", "    p256.load(key, w, rr);")
mut(ES, "sign", "sign: the digest not reduced for the nonce", "        p256.sreduce(work, p256.fe(p256.base()), p256.fe(p256.base()));\n", "")
mut(ES, "sign", "sign: only the words the table used wiped", "    p256.wipe(work, 0, work_len());\n", "    p256.wipe(work, 0, p256_pt.work_len());\n")

# Argued to change nothing observable (docs/p256-fast.md §8.4); each is still run, and reported.
EQUIVALENT = {
    "sfrom_mont: not reduced": "The unreduced result differs from the reduced only by n, and only when the value is 0 mod n. In verification u1 = n instead of 0 gives n G = O, the same sum (the vectors with the digest n show it); in signing s = n instead of 0 has probability 2^-256, and r, s are tested for zero before they are sent.",
    "recode: the carry threshold 9 (digits to +9)": "(v + 8) >> 4 turns a nibble of 8 into -8 and a carry; the digits still sum to k and stay within [-8, 8] (8 now also as -8 and a carry), so the table covers them. The same point by another route.",
    "affine: the infinity test dropped": "A valid scalar in [1, n) times a point of prime order is never the point at infinity, so Z is never zero: the test is a defence no input reaches (docs/ecdh.md §1 said the same of ecdh-result-infinity).",
    "wnaf: the window threshold": "The window value d is odd and 2^(width-1) is even, so d > 2^(width-1) - 1 and d >= 2^(width-1) are the same test.",
    "wnaf: the carry added a bit early": "When the digit is negative the window's top bit (i + width - 1) is set and the window is then skipped; adding 2^(i+width-1) to a set bit carries into i + width exactly as adding 2^(i+width) does, and the bits in between are never read again.",
    "load_q: the wrong multiple": "k is odd, so (k - 1) / 2 and k / 2 are the same integer division.",
    "verify: u1's digits at width 5": "A width-5 wNAF of u1 has digits in +-15, all in the generator's table of +-63: the same sum with more additions.",
    "ecdsa P-256: the fast path skipped for the wrong curve": "The generic bigmod path is still there and correct for P-256, so the answers are the same; the mutant shows the two paths agree on every case run.",
    # The key tables (docs/p256-fast.md §11.6). A table is an optimisation with the same answers, so a mutant that
    # makes a caller verify without it changes the cost and nothing else; the cost is what the guard's
    # `ecdsa_p256_verify_tabled` and `ecdsa_p256_sign_checked_tabled` rows and §11's handshakes measure.
    "tabled: u2's digits at width 5": "A width-5 wNAF of u2 has digits in +-15, all in the table's +-63 of each run: the same sum with more additions (as `verify: u1's digits at width 5`).",
    "anchors: a root verified without its table": "A chain's answer is the same with or without the anchor's table (the tabled replays of every x509 file show it); only the cost changes, measured by the client handshake of §11.",
    "identity: the check without the table": "The server's check gives the same answer under the point as under its table; only its cost changes, which the guard's `ecdsa_p256_sign_checked_tabled` row (3.18M instructions against 5.13M) and the server handshake of §11 measure.",
    "table: a refusal leaves the entries": "The entries are written only once the point is checked; the refusals after that are -40, a multiple at infinity, which no point of the curve (prime order n, multiples below 2^201) reaches. The earlier refusals return before any entry is written, onto the zeros the function starts with.",
}


def scoped(text, fn):
    if fn == "":
        return 0, len(text)
    m = re.search(r"^(?:pub )?fn %s[\[(]" % re.escape(fn), text, re.M)
    if not m:
        raise SystemExit(f"no function {fn}")
    nxt = re.search(r"^(?:pub )?(?:fn|static) ", text[m.end():], re.M)
    end = m.end() + (nxt.start() if nxt else len(text))
    return m.start(), end


def apply(text, fn, old, new):
    a, b = scoped(text, fn)
    body = text[a:b]
    if old not in body:
        raise SystemExit(f"{fn}: `{old}` not found")
    return text[:a] + body.replace(old, new, 1) + text[b:]


def run(cmd, cwd, log=None, timeout=3600):
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    return r.returncode, r.stdout + r.stderr


def build_drivers(scratch, out):
    c = os.path.join(scratch, "target/release/cancho")
    outs = {}
    for name, srcs in {
        "ecdsa": ["tests/programs/ecdsa_driver.cho"],
        "ecdh": ["tests/programs/ecdh_driver.cho"],
        "sign": ["tests/programs/ecdsa_sign_driver.cho", "packages/x509/x509.cho", "packages/x509/key.cho"],
    }.items():
        exe = os.path.join(out, name)
        code, text = run([c, "build", "--std"] + srcs + ["-o", exe], scratch)
        if code != 0:
            return None, text
        outs[name] = exe
    return outs, ""


def evidence(scratch, out):
    """None when everything agrees, else what disagreed."""
    code, text = run(["cargo", "build", "--release", "-p", "cancho"], scratch)
    if code != 0:
        return "BUILD", text[-600:]
    code, text = run(["cargo", "test", "--release", "-p", "cancho", "--test", "conformance", "--", "p256", "ecdh", "ecdsa", "x509_verify", "key_tables"], scratch)
    if code != 0:
        failed = re.findall(r"^test (\S+) \.\.\. FAILED", text, re.M)
        if not failed and "error[" in text:
            return "BUILD", text[-600:]
        return "tests", ", ".join(failed) or text[-300:]
    drivers, text = build_drivers(scratch, out)
    if drivers is None:
        return "BUILD", text[-600:]
    for label, cmd in [
        ("sign reference", ["python3", "scripts/ecdsa_sign_differential.py", "reference", drivers["sign"], "300"]),
        ("sign openssl", ["python3", "scripts/ecdsa_sign_differential.py", "openssl", drivers["sign"], drivers["ecdsa"], "100"]),
        ("ecdsa openssl", ["python3", "scripts/ecdsa_differential.py", drivers["ecdsa"], "openssl", "100"]),
        ("ecdh openssl", ["python3", "scripts/ecdh_differential.py", drivers["ecdh"], "100"]),
    ]:
        code, text = run(cmd, scratch)
        if code != 0:
            return label, text[-300:]
    return None, ""


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    mutants = [m for m in M if m[3] is not None]
    if args:
        mutants = [m for m in mutants if any(a in m[2] for a in args)]
    # Every mutant applies cleanly before anything is built.
    for file, fn, name, old, new in mutants:
        apply(open(os.path.join(ROOT, file)).read(), fn, old, new)
    if "--list" in sys.argv:
        for m in mutants:
            print(f"{m[0]:24} {m[2]}")
        print(len(mutants), "mutants, each applies")
        return
    scratch = tempfile.mkdtemp(prefix="p256-mutants-")
    out = tempfile.mkdtemp(prefix="p256-mutants-out-")
    for item in os.listdir(ROOT):
        if item in ("target", ".git"):
            continue
        src = os.path.join(ROOT, item)
        (shutil.copytree if os.path.isdir(src) else shutil.copy)(src, os.path.join(scratch, item))
    print(f"scratch copy in {scratch}", flush=True)
    began = time.time()
    why, text = evidence(scratch, out)
    if why:
        print(f"the unmutated tree fails ({why}): {text}")
        sys.exit(2)
    print(f"unmutated tree passes ({time.time() - began:.0f} s)", flush=True)
    killed, survived, broken = [], [], []
    for file, fn, name, old, new in mutants:
        path = os.path.join(scratch, file)
        original = open(path).read()
        open(path, "w").write(apply(original, fn, old, new))
        try:
            why, text = evidence(scratch, out)
        finally:
            open(path, "w").write(original)
        if why == "BUILD":
            broken.append(name)
            print(f"  ERROR    {name}: does not build: {text[-200:]}", flush=True)
        elif why:
            killed.append(name)
            print(f"  killed   {name}   [{why}]", flush=True)
        else:
            (survived).append(name)
            print(f"  SURVIVED {name}", flush=True)
    unexplained = [n for n in survived if not EQUIVALENT.get(n)]
    print(f"\n{len(mutants)} mutants: {len(killed)} killed, {len(survived)} survived, {len(broken)} did not build")
    for n in survived:
        print(f"  survivor: {n}: {EQUIVALENT.get(n) or 'NOT ARGUED'}")
    sys.exit(1 if unexplained or broken else 0)


if __name__ == "__main__":
    main()
