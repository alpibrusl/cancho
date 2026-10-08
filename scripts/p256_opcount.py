#!/usr/bin/env python3
"""Field operations per P-256 operation (docs/p256-fast.md §1).

    python3 scripts/p256_opcount.py [m_ns a_ns s_ns]

Counts the `bigmod.mul`, `.add` and `.sub` calls in the body of each point
function of `std/ecdh.cho` and `std/ecdsa.cho` (so a count cannot drift from
the source), composes them into the operations as the code runs them, and,
given the cost of one field operation in ns, the time they account for. The
difference from a measured total is the copies, the table select, `setup`
and the loops around them.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def bodies(path):
    text = (ROOT / path).read_text()
    out = {}
    for m in re.finditer(r"^(?:pub )?fn (\w+)", text, re.M):
        start = m.end()
        nxt = re.search(r"^(?:pub )?fn \w+|^static ", text[start:], re.M)
        out[m.group(1)] = text[start : start + nxt.start()] if nxt else text[start:]
    return out


def counts(body):
    return {k: len(re.findall(rf"bigmod\.{k}\(", body)) for k in ("mul", "add", "sub")}


def main():
    ecdh = bodies("std/ecdh.cho")
    ecdsa = bodies("std/ecdsa.cho")
    c = {
        "RCB add (ecdh.add_points)": counts(ecdh["add_points"]),
        "RCB double (ecdh.double_point)": counts(ecdh["double_point"]),
        "Jacobian add (ecdsa.add)": counts(ecdsa["add"]),
        "Jacobian double (ecdsa.double)": counts(ecdsa["double"]),
        "on_curve": counts(ecdh["on_curve"]),
    }
    for k, v in c.items():
        print(f"{k:34} mul {v['mul']:3}  add {v['add']:3}  sub {v['sub']:3}")
    # Fermat with p - 2 (or n - 2): 255 squarings, one multiplication per set bit below the top.
    def pow_ops(e):
        return e.bit_length() - 1, bin(e).count("1") - 1

    p = 0xFFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF
    n = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
    ip = pow_ops(p - 2)
    inn = pow_ops(n - 2)
    print(f"inversion mod p: {ip[0]} squarings + {ip[1]} multiplications (+ the conversion)")
    print(f"inversion mod n: {inn[0]} squarings + {inn[1]} multiplications")
    inv_p = ip[0] + ip[1] + 1
    inv_n = inn[0] + inn[1] + 1
    ra, rd = c["RCB add (ecdh.add_points)"], c["RCB double (ecdh.double_point)"]
    ja, jd = c["Jacobian add (ecdsa.add)"], c["Jacobian double (ecdsa.double)"]

    def tot(parts):
        return {k: sum(n_ * d[k] for n_, d in parts) for k in ("mul", "add", "sub")}

    ladder = tot([(14 + 64, ra), (256, rd)])
    ladder["mul"] += 0
    ops = {
        "ecdh.shared / public_key": (tot([(14 + 64, ra), (256, rd), (1, c["on_curve"])]), inv_p + 4),
        # 4/5 of the Shamir bits add something in expectation: bits (u1, u2) are 00 in 1/4.
        "ecdsa.verify (256 bits, 192 adds expected)": (tot([(192 + 1, ja), (256, jd), (1, c["on_curve"])]), inv_p + inv_n + 8),
        "ecdsa_sign.sign (ladder + 2 inversions)": (tot([(14 + 64, ra), (256, rd)]), inv_p + inv_n + 12),
    }
    m_ns, a_ns, s_ns = (float(x) for x in sys.argv[1:4]) if len(sys.argv) > 3 else (0.0, 0.0, 0.0)
    for name, (t, extra_mul) in ops.items():
        mul = t["mul"] + extra_mul
        share = lambda x: f"{100 * x / mul:.0f}%" if mul else ""
        line = f"{name:44} mul {mul:5}  add {t['add']:5}  sub {t['sub']:5}   inversions: {share(extra_mul - 12)}"
        if m_ns:
            time = mul * m_ns + t["add"] * a_ns + t["sub"] * s_ns
            line += f"   accounted {time / 1000:.0f} us"
        print(line)


if __name__ == "__main__":
    main()
