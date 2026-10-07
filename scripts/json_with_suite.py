#!/usr/bin/env python3
"""`std.json.parse` and `parse_with` on the JSONTestSuite (docs/json.md §3.1).

usage: json_with_suite.py CANCHO SUITE_DIR

SUITE_DIR is a checkout of https://github.com/nst/JSONTestSuite (its `test_parsing/`
holds 318 files: `y_` must be accepted, `n_` must be refused, `i_` is implementation
defined). Each file goes to `tests/programs/json_with_diff.cho`, which runs `parse` and
`parse_with` on it (and on a too-short tape, with a dirty state) and fails if their
answers or tapes differ. This prints how many of each class were accepted and refused,
the files whose class the library gets "wrong" (none are expected for `y_`/`n_`: the
library is strict, and `docs/json.md` lists the one policy it has, depth 128), and exits
1 if `parse` and `parse_with` ever differ. The suite is not vendored: it is read from
where the caller has it, and `conformance/json.rs` carries a corpus of its own.
"""
import os, subprocess, sys, tempfile

cancho, suite = sys.argv[1], sys.argv[2]
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
work = tempfile.mkdtemp(prefix="json-suite-")
exe = os.path.join(work, "diff")
subprocess.run([cancho, "build", "--std", os.path.join(root, "tests/programs/json_with_diff.cho"), "-o", exe],
               check=True, capture_output=True)

counts = {}
odd = []
files = sorted(f for f in os.listdir(os.path.join(suite, "test_parsing")) if f.endswith(".json"))
for name in files:
    data = open(os.path.join(suite, "test_parsing", name), "rb").read()
    out = subprocess.run([exe], input=b"%d\n" % len(data) + data, capture_output=True)
    text = out.stdout.decode()
    if out.returncode != 0 or not text.startswith("ok "):
        print(f"DIFFER {name}: {text.strip()} {out.stderr.decode().strip()} (exit {out.returncode})")
        sys.exit(1)
    _, _, accepted, _ = text.split()
    kind = name[0]
    key = (kind, "accepted" if accepted == "1" else "refused")
    counts[key] = counts.get(key, 0) + 1
    if (kind == "y" and accepted != "1") or (kind == "n" and accepted == "1"):
        odd.append((name, key[1]))
for (kind, what), n in sorted(counts.items()):
    print(f"{kind}_ {what}: {n}")
print(f"{len(files)} files, parse and parse_with agree on every one (answers and tapes)")
for name, what in odd:
    print(f"class mismatch: {name} {what}")
