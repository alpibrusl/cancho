#!/usr/bin/env python3
"""Checks the value bounds `std.p256`'s lazy reduction relies on (docs/p256-fast.md §4.3, §4.4).

    python3 scripts/p256_bounds.py

An element is below B p for a bound B in units of p. The kernels of
`std.p256_kernels` need: for a multiplication, B_x * B_y <= 2^24 (so the
answer is below p + x*y/R <= 2 p, and in fact below 1 + B_x*B_y/2^24); for a
subtraction `subK`, the subtrahend below K p; for every element, a value below
2^280 = 2^24 p so its ten limbs hold it. This script reads each point
formula's straight-line source in `std/p256_pt.cho`, `std/p256_pj.cho`, ...,
propagates the bounds through every `p256.mul`, `sqr`, `add` and `sub`, and
fails when one is exceeded. A formula's inputs are assumed to be at the bound
that its own outputs reach (an induction: the outputs of one call are the
inputs of the next), found by iterating from 2.
"""
import re
import sys
from fractions import Fraction
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SUB_KS = (2, 4, 8, 16, 32, 64)
LIMIT = Fraction(1 << 24)  # 2^280 / p, rounded down: p > 2^255.99999
MUL_LIMIT = Fraction(1 << 24)

# function -> (source file, input names, output names, constants {name: bound})
# Names are the tokens in the calls; t(i) are temporaries.
FORMULAS = {
    "add_points": ("std/p256_pt.cho", ["x1", "y1", "z1", "x2", "y2", "z2"], ["x3", "y3", "z3"], {"b": Fraction(1)}),
    "mixed_add": ("std/p256_pt.cho", ["x1", "y1", "z1", "x2", "y2"], ["x3", "y3", "z3"], {"b": Fraction(1)}),
    "on_curve_xy": ("std/p256_pt.cho", ["x", "y"], [], {"p256.b_mont()": Fraction(1)}),
    "double": ("std/p256_vf.cho", ["x", "y", "z"], ["x_of(d)", "y_of(d)", "z_of(d)"], {}, 8),
    "add_jac": ("std/p256_vf.cho", ["x_of(p)", "y_of(p)", "z_of(p)", "x_of(q)", "y_of(q)", "z_of(q)"], ["x_of(d)", "y_of(d)", "z_of(d)"], {}, 8),
    "load_q": ("std/p256_vf.cho", ["y_of(pt_ent())"], ["y_of(pt_ent())"], {"e_zero()": Fraction(0)}, 2),
    "matches": ("std/p256_vf.cho", ["rr", "x_of(pt_acc())", "z_of(pt_acc())"], [], {}, 8),
    "madd": ("std/p256_vf.cho", ["x1", "y1", "z1", "x2", "y2"], ["x_of(d)", "y_of(d)", "z_of(d)"], {}, 8),
    "double_point": ("std/p256_pt.cho", ["x", "y", "z"], ["x3", "y3", "z3"], {"b": Fraction(1)}),
}
EXTRA = {}  # filled by formulas registered below, when their files exist


def body(path, name):
    text = (ROOT / path).read_text()
    m = re.search(r"fn %s\[[^\]]*\]\([^{]*\) -> \[\] \w+ \{\n(.*?)\n\}\n" % re.escape(name), text, re.S)
    if not m:
        raise SystemExit(f"no function {name} in {path}")
    return m.group(1)


CALL = re.compile(r"p256\.(mul|sqr|add|sub)\(w, ([^)]*?(?:\(\d+\))?)(?:, ([^,]*?\(\d+\)|[\w]+))?(?:, ([^,]*?\(\d+\)|[\w]+))?\);")


def args(line):
    m = re.match(r"\s*p256\.(mul|sqr|add|copy|put_n|to_mont|from_mont|sub(?:2|4|8|16|32|64)?)\(w, (.*)\);\s*$", line)
    if not m:
        return None
    parts = []
    depth = 0
    cur = ""
    for ch in m.group(2):
        if ch == "(":
            depth += 1
        if ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append(cur.strip())
            cur = ""
        else:
            cur += ch
    parts.append(cur.strip())
    return m.group(1), parts


def pick_k(by):
    for k in SUB_KS:
        if by <= k:
            return k
    return None


def run(path, name, inputs, outputs, consts, b_in, rewrite=None):
    bound = {k: Fraction(b_in) for k in inputs}
    bound.update(consts)
    worst_mul = Fraction(0)
    worst_sub = Fraction(0)
    worst_val = Fraction(0)
    for line in body(path, name).split("\n"):
        a = args(line)
        if a is None:
            continue
        op, parts = a
        if op == "put_n":
            bound[parts[0]] = Fraction(1)
            continue
        if op in ("sqr", "copy", "to_mont", "from_mont"):
            x, d = parts
            y = x
        else:
            x, y, d = parts
        if x not in bound or y not in bound:
            raise SystemExit(f"{name}: {x if x not in bound else y} used before it is set: {line.strip()}")
        bx, by = bound[x], bound[y]
        if op == "copy":
            bound[d] = bx
            continue
        if op == "from_mont":
            # Multiplied by 1 and reduced: in [0, p).
            bound[d] = Fraction(1)
            continue
        if op == "to_mont":
            # A multiplication by R^2 mod p, which is below p.
            prod = bx
            worst_mul = max(worst_mul, prod)
            bound[d] = 1 + prod / LIMIT
            continue
        if op in ("mul", "sqr"):
            prod = bx * by
            worst_mul = max(worst_mul, prod)
            if prod > MUL_LIMIT:
                return None, f"{name}: {line.strip()} multiplies {float(bx):.1f} p by {float(by):.1f} p"
            bound[d] = 1 + prod / LIMIT
        elif op == "add":
            bound[d] = bx + by
        else:
            k = int(op[3:]) if len(op) > 3 else None
            if k is None:
                if rewrite is None:
                    return None, f"{name}: {line.strip()} has no K (run with --rewrite)"
                k = pick_k(by)
                if k is None:
                    return None, f"{name}: {line.strip()} subtracts {float(by):.1f} p (limit {SUB_KS[-1]})"
                rewrite.append((line, line.replace("p256.sub(", f"p256.sub{k}(")))
            if by > k:
                return None, f"{name}: {line.strip()} subtracts {float(by):.1f} p from a K = {k} subtraction"
            worst_sub = max(worst_sub, by)
            bound[d] = bx + k
        worst_val = max(worst_val, bound[d])
        if bound[d] >= LIMIT:
            return None, f"{name}: {line.strip()} makes {float(bound[d])} p"
    return ({o: bound[o] for o in outputs}, worst_mul, worst_sub, worst_val), None


# Functions whose outputs feed each other's inputs share one bound: the smallest B
# for which every output is at most B when every input is at most B.
GROUPS = [
    ["add_points", "mixed_add", "double_point"],
    ["double", "add_jac", "madd", "load_q", "matches"],
    ["on_curve_xy"],
]


def main():
    do_rewrite = "--rewrite" in sys.argv
    failed = False
    table = dict(FORMULAS)
    table.update(EXTRA)
    for group in GROUPS:
        group = [g for g in group if g in table and (ROOT / table[g][0]).exists()]
        if not group:
            continue
        b_in = Fraction(max((table[g][4] if len(table[g]) > 4 else 2) for g in group))
        for _ in range(12):
            results, edits, err = {}, {}, None
            for name in group:
                path, ins, outs, consts = table[name][:4]
                edits[name] = [] if do_rewrite else None
                res, err = run(path, name, ins, outs, consts, b_in, edits[name])
                if err:
                    break
                results[name] = res
            if err:
                print("FAIL", err)
                failed = True
                break
            top = max((max(r[0].values(), default=Fraction(0)) for r in results.values()), default=Fraction(0))
            if top <= b_in:
                for name in group:
                    out, wm, ws, wv = results[name]
                    print(f"{name:12} inputs <= {float(b_in):5.1f} p   outputs <= {float(max(out.values(), default=Fraction(0))):5.1f} p   "
                          f"largest product {float(wm):8.1f} p^2 (limit {int(MUL_LIMIT)})   largest subtrahend {float(ws):5.1f} p   "
                          f"largest value {float(wv):5.1f} p (limit {int(LIMIT)})")
                    if edits[name]:
                        path = table[name][0]
                        text = (ROOT / path).read_text()
                        start = text.index(f"fn {name}[")
                        head, tail = text[:start], text[start:]
                        for old, new in edits[name]:
                            tail = tail.replace(old, new, 1)
                        (ROOT / path).write_text(head + tail)
                break
            b_in = Fraction(-(-top // 1))
        else:
            print(f"FAIL {group}: the bound does not close")
            failed = True
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
