#!/usr/bin/env python3
"""The object code of `mul_wide`, `add_carry` and `sub_borrow` (docs/wide-multiply.md §5).

    python3 scripts/wide_multiply_objdump.py target/release/cancho [--target <triple>] [--backend llvm|cranelift]

Builds `tests/programs/wide_multiply_ct.cho` to an object, disassembles it, and checks
that `words` (the three builtins and nothing else) is straight-line code: no branch, no
call (so no `__multi3`), and that it contains a widening multiply (`mul`/`mulx`/`umulh`) and a carry
instruction (`adc`/`sbb`/`adcs`/`sbcs`/`setb`/`cset`). `product4`, a bignum inner loop, may branch only
on its loop counters; the script prints its instruction mix. Exit 1 on a failure.
"""
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

root = Path(__file__).resolve().parent.parent
cancho = sys.argv[1]
extra = sys.argv[2:]
obj = Path(tempfile.mkdtemp()) / "ct.o"
subprocess.run([cancho, "build", "--std", str(root / "tests/programs/wide_multiply_ct.cho"), "--emit", "obj", "-o", str(obj), *extra], check=True)
objdump = shutil.which("llvm-objdump") or shutil.which("objdump")
text = subprocess.run([objdump, "-d", "--no-show-raw-insn", str(obj)], check=True, capture_output=True, text=True).stdout

BRANCH = re.compile(r"^(j[a-z]+|call|callq|b|b\.[a-z]+|bl|br|blr|cbz|cbnz|tbz|tbnz|ret.*)$")
def function(name):
    m = re.search(r"^[0-9a-f]+ <_?%s>:\n(.*?)(?=^\s*$|^[0-9a-f]+ <)" % re.escape(name), text, re.S | re.M)
    if not m:
        sys.exit(f"no function {name} in the disassembly")
    ops = []
    for line in m.group(1).splitlines():
        parts = line.split(":", 1)[-1].split()
        if parts:
            ops.append(parts[0])
    return ops

ok = True
words = function("lexs_words")
jumps = [o for o in words if BRANCH.match(o) and not o.startswith("ret")]
widening = [o for o in words if o in ("mul", "mulx", "umulh", "imul", "mulq", "umull")]
carry = [o for o in words if o in ("adc", "adcs", "sbb", "sbcs", "adcq", "sbbq", "setb", "cset", "adcx", "adox", "sbc")]
print(f"words: {len(words)} instructions; branches/calls: {jumps or 'none'}")
print(f"words: widening multiply {sorted(set(widening))}; carry instructions {sorted(set(carry))}")
if jumps or not widening or not carry:
    ok = False
mix = {}
for o in function("lexs_product4"):
    mix[o] = mix.get(o, 0) + 1
print("product4 (a 4x4-word schoolbook product): " + ", ".join(f"{k} {v}" for k, v in sorted(mix.items())))
if "__multi3" in text:
    print("FAIL: the object calls __multi3")
    ok = False
print("ok" if ok else "FAIL")
sys.exit(0 if ok else 1)
