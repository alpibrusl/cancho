#!/usr/bin/env python3
"""Every conditional jump in the hardware path of `std.gcm` (docs/gcm-wide.md §5).

    cancho build --std tests/programs/gcm_driver.cho --emit obj -o gcm.o
    python3 scripts/gcm_branches.py gcm.o

For each function of the hardware path (`FUNCTIONS`: `std.gcm`'s `seal_hardware` and `open_hardware`
and the six functions the builtins call) lists every conditional jump that does not go to a trap
(`ud2`), with the instruction that set its flags, and checks it against `ALLOWED`: the comparisons
that are reviewed to be on a public value, each with what it tests. A jump after any other
comparison fails the script, so a change that adds a branch is a change someone reads. Run it on
the object a machine builds: x86-64 (GNU objdump syntax) and aarch64 (`b.cond`, `cbz`, `tbz`) are
both read, with `OBJDUMP` naming the tool.
"""
import os
import re
import subprocess
import sys

OBJDUMP = os.environ.get("OBJDUMP", "objdump")

FUNCTIONS = [
    "lexs_std.gcm.seal_hardware",
    "lexs_std.gcm.open_hardware",
    "cancho_aes_encrypt_block",
    "cancho_aes_ctr32",
    "cancho_gh_absorb",
    "cancho_ghash_powers",
    "cancho_gcm_tag",
    "cancho_gcm_tag_diff",
]

# (function, regex on the normalised comparison, what it tests). `K` stands for a number and `R` for a register: only the
# shape of the comparison is checked, and the reasons below are what reading each jump in the object (x86-64 and
# aarch64) found; a comparison of a new shape is a jump to read again. Every one is on a length, a count made from one
# (the number of groups of eight blocks, the blocks in the tail, the bytes left), a loop counter, the round count (10, 12
# or 14: public, it is the key's length) or the answer of the tag comparison (whether the record authenticated, which
# the peer learns from the alert whatever happens). None is on a byte of the key, the data, a counter block or a hash
# value: those are only ever operands of vector instructions, which set no flags.
LENGTH = "a length, a count made from one, a loop counter or the round count against a constant or another such value"
ALLOWED = [
    (f, pattern, LENGTH)
    for f in FUNCTIONS
    for pattern in [
        r"cmp K,R|cmp R,R|cmp R,K",  # x86-64 and aarch64 comparisons of registers and constants, never of memory
        r"test R,R|test K,R",  # emptiness and bit tests of a count or a length
        r"bt R,R",  # the round count against the set {10, 12, 14}
        r"add K,R|add R,R|add R,R,R|sub R,R|sub K,R|dec R|inc R|and K,R|or R,R|shr K,R|subs R,R,K|subs R,R,R|adds R,R,K",
        r"cmp R, ?K|cmp R, ?R|subs xzr, ?R, ?K|subs xzr, ?R, ?R|tst R, ?K|ands xzr, ?R, ?K",
    ]
]


def normalise(text):
    """`cmp $0x80,%rdx` as `cmp K,R`; aarch64's `cmp x1, #128` as `cmp R, K`."""
    text = re.sub(r"[$#]?0x[0-9a-f]+|[$#]-?\b\d+\b", "K", text)
    return re.sub(r"%[a-z0-9]+|\b[xwv]\d+\b|\bxzr\b|\bwzr\b", "R", text)


def main():
    obj = sys.argv[1]
    bad = 0
    for f in FUNCTIONS:
        text = subprocess.run([OBJDUMP, "-d", "--no-show-raw-insn", f"--disassemble={f}", obj], capture_output=True, text=True, check=True).stdout
        ins = []
        for line in text.splitlines():
            m = re.match(r"\s*([0-9a-f]+):\s+(\S+)\s*(.*)", line)
            if m:
                ins.append((int(m.group(1), 16), m.group(2), m.group(3)))
        assert ins, f"no code for {f}: was the object built from gcm_driver.cho?"
        at = {a: op for a, op, _ in ins}
        print(f"{f}:")
        setter = ""
        count = 0
        for i, (_, op, arg) in enumerate(ins):
            arm = op in ("cmp", "cmn", "tst", "subs", "adds", "ands", "ccmp", "ccmn")
            x86 = op.startswith(("cmp", "test", "sub", "add", "and", "or", "xor", "dec", "inc", "shr", "shl", "bt", "neg")) and not op.startswith("j")
            if arm or x86:
                setter = f"{op} {arg}"
            conditional = (op.startswith("j") and op != "jmp") or re.match(r"b\.\w+|cbn?z|tbn?z", op)
            if not conditional:
                continue
            target = int(re.search(r"0x([0-9a-f]+)", arg).group(1), 16) if re.search(r"0x([0-9a-f]+)", arg) else None
            if at.get(target) in ("ud2", "brk"):
                continue
            count += 1
            note = next((why for fn, pattern, why in ALLOWED if fn == f and re.fullmatch(pattern, normalise(setter))), None)
            print(f"    {op:6} after `{setter}`: {note or 'NOT REVIEWED'}")
            bad += note is None
        print(f"    ({count} conditional jumps that are not traps)")
    print(f"{bad} jumps not reviewed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
