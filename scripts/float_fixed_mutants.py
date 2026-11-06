#!/usr/bin/env python3
"""The deliberate bugs of docs/float-printing.md section 10.4 (F4): each is one edit of a copy of `std/fmt.cho`, built into the driver
`tests/programs/float_fixed.cho` from the files of `std/`, and the differential of `scripts/float_differential.py` must notice it
(wrong text, or a driver that traps). Exit 1 if one survives.

    python3 scripts/float_fixed_mutants.py [REPO_ROOT]     (REPO_ROOT has target/release/cancho; default: this repository)
"""
import importlib.util, os, random, shutil, subprocess, sys, tempfile
ROOT = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.dirname(os.path.abspath(__file__))); CANCHO = os.path.join(ROOT, "target/release/cancho")
spec = importlib.util.spec_from_file_location("fd", os.path.join(ROOT, "scripts/float_differential.py")); fd = importlib.util.module_from_spec(spec); spec.loader.exec_module(fd)
M = [
 ("ties rounded up", "if c > 0 || c == 0 && digits[q - 1] % 2 == 1 {", "if c >= 0 {"),
 ("ties rounded down", "if c > 0 || c == 0 && digits[q - 1] % 2 == 1 {", "if c > 0 {"),
 ("the carry through nines dropped", "                if digits[j] == 9 {\n                    digits[j] = 0;", "                if false {\n                    digits[j] = 0;"),
 ("the new leading digit of a carry dropped", "                digits[0] = 1;\n                q = q + 1;", "                digits[0] = 0;\n                q = q + 1;"),
 ("the sign dropped on a negative that rounds to zero", "    if bits < 0 {\n        at = put(out, 0, \"-\");\n        if at < 0 {\n            return 0 - 1;\n        }\n    }\n    if bits & 0x7fffffffffffffff == 0x7ff0000000000000 {\n        return put(out, at, \"inf\");\n    }\n\n    let m = mantissa(bits);", "    if bits < 0 && x < 0.0 - 0.5 {\n        at = put(out, 0, \"-\");\n        if at < 0 {\n            return 0 - 1;\n        }\n    }\n    if bits & 0x7fffffffffffffff == 0x7ff0000000000000 {\n        return put(out, at, \"inf\");\n    }\n\n    let m = mantissa(bits);"),
 ("the zeros before a short number one too few", "} else if f >= prec - q {", "} else if f > prec - q {"),
 ("S one bit too wide for a fraction", "            bignum.shift_left(s, 0 - e);\n        }\n\n        // S becomes", "            bignum.shift_left(s, 0 - e + 1);\n        }\n\n        // S becomes"),
 ("R not scaled by the precision for a whole number", "            bignum.shift_left(r, e);\n            bignum.mul_pow10(r, prec);", "            bignum.shift_left(r, e);"),
 ("the digit count one too many", "                bignum.copy(s, t);\n                q = q + 1;", "                bignum.copy(s, t);\n                q = q + 2;"),
]
r = random.Random(3)
cases = fd.fixed_cases(r, 4000)
survivors = 0
for name, old, new in M:
    d = tempfile.mkdtemp()
    for f in os.listdir(os.path.join(ROOT, "std")):
        shutil.copy(os.path.join(ROOT, "std", f), d)
    t = open(os.path.join(d, "fmt.cho")).read()
    assert t.count(old) == 1, (name, t.count(old))
    open(os.path.join(d, "fmt.cho"), "w").write(t.replace(old, new))
    exe = os.path.join(d, "drv")
    p = subprocess.run([CANCHO, "build", os.path.join(ROOT, "tests/programs/float_fixed.cho")] + [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith(".cho")] + ["-o", exe], capture_output=True)
    if p.returncode:
        print("BUILD FAIL", name, p.stderr.decode()[:300]); survivors += 1; continue
    try:
        n, bad = fd.check_fixed(exe, cases)
        how = "%d of %d wrong" % (len(bad), n)
    except SystemExit:                       # the driver itself died: an index out of range is a kill too
        bad, how = [1], "the driver trapped"
    print("killed  " if bad else "SURVIVED", name, "(%s)" % how)
    survivors += 0 if bad else 1
sys.exit(1 if survivors else 0)
