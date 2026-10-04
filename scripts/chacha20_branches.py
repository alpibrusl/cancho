#!/usr/bin/env python3
"""Where the secret-handling code of `std/chacha20.ls` can branch (docs/chacha20.md §3).

    lex-sys build --std tests/programs/aead_bench.ls --emit obj -o bench.o
    python3 scripts/chacha20_branches.py bench.o

For each function that touches the key, the keystream or the Poly1305
accumulator, lists every conditional jump with the instruction that sets its
flags, and fails if any jump goes somewhere other than a trap (`ud2`): a
bounds or overflow check on public data is expected, any other conditional
jump is not. Reading what each flag-setting instruction compares (a slice
length, a constant index, a shift amount) is still the reader's job; the
script prints them so it can be done. x86-64 only (objdump syntax).
"""
import collections
import re
import subprocess
import sys

FUNCTIONS = [
    "chacha20.quarter",
    "chacha20.rotl32",
    "chacha20.m32",
    "chacha20.le32",
    "bytes.store_le32",
    "chacha20.dot5",
    "chacha20.poly_block",
    "chacha20.poly_finish",
]


def main():
    obj = sys.argv[1]
    bad = 0
    for f in FUNCTIONS:
        text = subprocess.run(
            ["objdump", "-d", "--no-show-raw-insn", f"--disassemble=lexs_std.{f}", obj],
            capture_output=True, text=True, check=True,
        ).stdout
        ins = []
        for line in text.splitlines():
            m = re.match(r"\s*([0-9a-f]+):\s+(\S+)\s*(.*)", line)
            if m:
                ins.append((int(m.group(1), 16), m.group(2), m.group(3)))
        assert ins, f"no code for {f}: was the object built from aead_bench.ls?"
        at = {a: op for a, op, _ in ins}
        kinds = collections.Counter()
        for i, (_, op, arg) in enumerate(ins):
            if op.startswith("cmov"):
                kinds[op] += 1
            if op.startswith("j") and op != "jmp":
                target = int(arg.split()[0], 16)
                setter = re.sub(r"0x[0-9a-f]+", "K", f"{ins[i - 1][1]} {ins[i - 1][2]}")
                if at.get(target) == "ud2":
                    kinds[f"{op} to a trap, after `{setter}`"] += 1
                else:
                    kinds[f"{op} NOT to a trap, after `{setter}`"] += 1
                    bad += 1
        print(f"{f}:")
        for k, n in sorted(kinds.items()):
            print(f"    {n:3} {k}")
    print(f"{bad} conditional jumps that are not traps")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
