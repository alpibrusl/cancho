#!/usr/bin/env python3
# Copyright (c) 2026 Alpibru S.L. Licensed under the EUPL-1.2.
"""Branch audit of the compiled secret path of `std.rsa_sign` (docs/rsa-sign.md §7 and §11, gate G6), arm64 or x86-64 objdump.

    cancho build --std --backend llvm tests/programs/rsa_sign_driver.cho packages/x509/{x509,key,rsa_key}.cho --emit obj -o rsa_sign_driver.o
    python3 scripts/rsa_sign_branches.py rsa_sign_driver.o [function ...]

For each function of the secret path (default list below) it lists every CONDITIONAL branch (arm64: b.cond, cbz, cbnz, tbz, tbnz) whose
target is not a trap block (`udf`/`brk`, a bounds or overflow check), with the flag-setting instruction before it and whether it goes
backwards (a loop edge) or forwards; and counts the conditional selects (csel, cset, ...), which are branch-free. It does not decide what
a compare is ON: the READER classifies each forward branch (a loop guard on a count, a length, a position, a return code, or a limb or
byte of a secret) from the source and the disassembly, and docs/rsa-sign.md §13 says which were read and which were not. It does not see
memory addressing that depends on a secret; that is audited by reading the source (`select` reads every entry; every other index is a
loop counter or a position). The method of scripts/chacha20_branches.py (x86-64 only), redone for arm64. One compiler, one architecture.
Needs `llvm-objdump` (Homebrew's llvm).
"""
import re
import subprocess
import sys

LLVM = "/opt/homebrew/opt/llvm/bin/llvm-objdump"
SECRET = [
    "std.bigmod." + f for f in ("load_secret", "setup_secret", "r_squared", "neg_inverse", "mont_mul", "ct_reduce", "reduce_once", "add", "sub", "mul", "to_mont",
                                 "from_mont", "copy", "set_small", "store", "store_reg", "bit", "select", "pow_secret", "blind_exponent")
] + [
    "std.rsa_sign." + f for f in ("sign_core", "crt_half", "garner", "recombine", "limbs_from", "limbs_to", "mul_limbs", "kappa_at", "flip", "wide_out", "emsa_pkcs1")
]
COND = re.compile(r"^(b\.\w+|cbz|cbnz|tbz|tbnz|j(?!mp)\w+)$")
FLAGS = re.compile(r"^(cmp|cmn|tst|subs|adds|ands|bics|ccmp|ccmn|negs|sbcs|adcs|test|cmpq|cmpl|testq)$")
CSEL = re.compile(r"^(csel|csinc|csinv|csneg|cset|csetm|cinc|cinv|cneg|cmov\w*|set\w+)$")
TRAP = ("brk", "udf", "ud2")


def disasm(obj):
    out = subprocess.run([LLVM, "-d", "--no-show-raw-insn", obj], capture_output=True, text=True, check=True).stdout
    funcs, cur = {}, None
    for line in out.splitlines():
        m = re.match(r"^[0-9a-f]+ <(.+)>:$", line)
        if m:
            cur = m.group(1).lstrip("_")
            funcs[cur] = []
            continue
        m = re.match(r"^\s*([0-9a-f]+):\s+(\S+)\s*(.*)$", line)
        if m and cur:
            funcs[cur].append((int(m.group(1), 16), m.group(2), m.group(3)))
    return funcs


NOWRITE = ("str", "stp", "strb", "strh", "stur", "cmp", "cmn", "tst", "b", "bl", "ret", "cbz", "cbnz", "tbz", "tbnz", "nop")


def operands(mn, op):
    """The registers a flag-setting instruction or cbz/cbnz reads (arm64 text; x86 is not parsed)."""
    regs = re.findall(r"\b([xw]\d+|sp)\b", op.split("<")[0])
    if mn in ("cmp", "cmn", "tst"):
        return regs
    if mn in ("subs", "adds", "ands", "negs", "sbcs", "adcs", "bics"):
        return regs[1:]
    if mn in ("cbz", "cbnz", "tbz", "tbnz"):
        return regs[:1]
    return regs


def origin(ins, idx, reg):
    """The last instruction before `idx` that writes `reg` (or its other-width alias), straight-line scan: not exact data flow."""
    base = reg[1:]
    for a, mn, op in reversed(ins[:idx]):
        if mn in NOWRITE:
            continue
        first = op.split(",")[0].strip()
        if first[1:] == base and first[:1] in "xw":
            return f"{mn} {op.split('<')[0].strip()}"
    return "(not in this function)"


def main():
    obj = sys.argv[1]
    names = sys.argv[2:] or SECRET
    funcs = disasm(obj)
    allins = {a: (mn, op) for f in funcs.values() for a, mn, op in f}
    total, fwd_total, back_total = 0, 0, 0
    for name in names:
        key = "lexs_" + name
        if key not in funcs:
            print(f"{name}: NOT FOUND")
            continue
        ins = funcs[key]
        nontrap, trap, csel = [], 0, 0
        last_flag = None
        for idx, (a, mn, op) in enumerate(ins):
            if FLAGS.match(mn):
                last_flag = f"{mn} {op}"
            if CSEL.match(mn):
                csel += 1
            if COND.match(mn):
                tgt = re.search(r"0x([0-9a-f]+)", op)
                t = int(tgt.group(1), 16) if tgt else None
                tm = allins.get(t, ("?", ""))[0]
                if tm in TRAP:
                    trap += 1
                    continue
                if mn.startswith("b.") or mn.startswith("j"):
                    why = last_flag
                else:
                    reg = op.split(",")[0].strip()
                    why = f"{mn} {reg}; {reg} last set by: "
                    for a2, mn2, op2 in reversed(ins[:idx]):
                        if op2.split(",")[0].strip() == reg and mn2 not in ("str", "stp", "cmp", "cmn", "tst", "b"):
                            why += f"{mn2} {op2}"
                            break
                    else:
                        why += "(argument or earlier)"
                direction = "back" if t is not None and t <= a else "fwd"
                src = []
                fi = idx
                if mn.startswith("b."):
                    for j in range(idx - 1, -1, -1):
                        if FLAGS.match(ins[j][1]):
                            fi = j
                            break
                    regs = operands(ins[fi][1], ins[fi][2]) if fi != idx else []
                else:
                    regs = operands(mn, op)
                loaded = False
                for r in regs:
                    o = origin(ins, fi, r)
                    src.append(f"{r}<={o}")
                    if o.startswith("ldr") or o.startswith("ld"):
                        loaded = True
                nontrap.append((a, mn, direction, op.split("\t")[-1].split("<")[0].strip(), why + ("   [LOADED OPERAND: " + "; ".join(src) + "]" if loaded else "")))
        fwd = sum(1 for x in nontrap if x[2] == "fwd")
        total += len(nontrap)
        fwd_total += fwd
        back_total += len(nontrap) - fwd
        print(f"{name}: {len(ins)} instructions, {trap} conditional branches to a trap, {len(nontrap)} elsewhere ({len(nontrap) - fwd} back, {fwd} forward), {csel} conditional selects")
        for a, mn, d, op, why in nontrap:
            print(f"    {a:x}: {mn} {d:4s} {op}    <- {why}")
    print(f"TOTAL conditional branches not to a trap: {total} ({back_total} back, {fwd_total} forward)")


main()
