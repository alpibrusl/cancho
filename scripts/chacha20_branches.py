#!/usr/bin/env python3
"""Where the secret-handling code of `std/chacha20.ls` can branch (docs/chacha20.md §3).

    lex-sys build --std tests/programs/aead_driver.ls --emit obj -o aead.o
    python3 scripts/chacha20_branches.py aead.o
    python3 scripts/chacha20_branches.py kdf.o crypto.compress crypto.compress512   # docs/hkdf.md §3

For each function that does arithmetic on the key, the keystream or the
Poly1305 accumulator (`FUNCTIONS`), lists every conditional jump with the
instruction that sets its flags, and fails if any jump goes somewhere other
than a trap (`ud2`): a bounds or overflow check on public data is expected,
any other conditional jump is not. Reading what each flag-setting instruction
compares (a slice length, a constant index, a shift amount) is still the
reader's job; the script prints them so it can be done. Function names after
the object file (`module.function`) replace the default list, which is
`std.chacha20`'s.

The functions that only move secret words between those (`LOOPS`: they hold
the key, the keystream or `r`, and loop over a message) are listed too, but
they branch on purpose, on lengths, loop counters and the result of the tag
comparison, so their other jumps are printed for the reader and not counted
(docs/chacha20.md §3; review finding B-6, #209). Not with names given.
`aead_driver.ls` calls every function here; `aead_bench.ls` does not call
`poly1305` or `open`. x86-64 only (GNU objdump syntax).
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

LOOPS = [
    "chacha20.block_into",
    "chacha20.xor",
    "chacha20.poly_init",
    "chacha20.poly_padded",
    "chacha20.poly1305",
    "chacha20.aead_tag",
    "chacha20.seal",
    "chacha20.open",
]


def main():
    obj = sys.argv[1]
    bad = 0
    loops = 0
    named = sys.argv[2:]
    for f in named or FUNCTIONS + LOOPS:
        strict = f not in LOOPS or bool(named)
        text = subprocess.run(
            ["objdump", "-d", "--no-show-raw-insn", f"--disassemble=lexs_std.{f}", obj],
            capture_output=True, text=True, check=True,
        ).stdout
        ins = []
        for line in text.splitlines():
            m = re.match(r"\s*([0-9a-f]+):\s+(\S+)\s*(.*)", line)
            if m:
                ins.append((int(m.group(1), 16), m.group(2), m.group(3)))
        assert ins, f"no code for {f}: was the object built from aead_driver.ls?"
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
                elif strict:
                    kinds[f"{op} NOT to a trap, after `{setter}`"] += 1
                    bad += 1
                else:
                    kinds[f"{op} on public data (to be read), after `{setter}`"] += 1
                    loops += 1
        print(f"{f}:" if strict else f"{f} (branches on lengths, counters and the tag comparison):")
        for k, n in sorted(kinds.items()):
            print(f"    {n:3} {k}")
    if loops:
        print(f"{loops} conditional jumps in the looping functions, for the reader")
    print(f"{bad} conditional jumps that are not traps")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
