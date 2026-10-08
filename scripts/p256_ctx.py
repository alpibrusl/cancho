#!/usr/bin/env python3
"""Every non-trap conditional jump and conditional move of a function, with the instructions before it.

    python3 scripts/p256_ctx.py <object> <module.function> [<instructions of context>]

The companion of `scripts/chacha20_branches.py` (which counts them, docs/chacha20.md §3): that one says
what each jump's flags were set by, this one shows the code around it so the reader can say what the
registers hold (a loop counter, a length, an overlap test of two pointers, or a secret). x86-64, GNU
objdump. Used for docs/p256-fast.md §8.2.
"""
import re, subprocess, sys
obj, fn = sys.argv[1], sys.argv[2]
sym = fn if fn.startswith("lexs_") else "lexs_std." + fn
text = subprocess.run(["objdump", "-d", "--no-show-raw-insn", f"--disassemble={sym}", obj], capture_output=True, text=True, check=True).stdout
ins = []
for line in text.splitlines():
    m = re.match(r"\s*([0-9a-f]+):\s+(\S+)\s*(.*)", line)
    if m: ins.append((int(m.group(1), 16), m.group(2), m.group(3)))
at = {a: op for a, op, _ in ins}
ctx = int(sys.argv[3]) if len(sys.argv) > 3 else 6
for i, (a, op, arg) in enumerate(ins):
    if (op.startswith("j") and op != "jmp") or op.startswith("cmov"):
        if op.startswith("j"):
            target = int(arg.split()[0], 16)
            if at.get(target) == "ud2": continue
        print(f"--- {op} at {a:x}")
        for j in range(max(0, i - ctx), min(len(ins), i + 3)):
            print(("=> " if j == i else "   ") + f"{ins[j][0]:x}: {ins[j][1]} {ins[j][2]}")
