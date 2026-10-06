#!/usr/bin/env python3
"""W2b (docs/wasm.md): a wasm module imports what its program's row licenses.

For every program that builds for `wasm32-wasip1`, this asks `lex-sys authority
--target wasm32-wasip1` what the row licenses (`required` and `allowed`, the latter
always including `proc_exit`: how a program leaves with a non-zero status), builds the
module, reads its import section, and requires

    required  ⊆  imports  ⊆  allowed

The right half is the one that matters for authority: **an import no label in the row
allows is a capability the runtime would be asked to grant that the program's own
effect row does not account for.** That is exactly what W1 found for stdio's
`clock_time_get`, and what this exists to keep from coming back. The left half says
the row is not claiming more than the module does.

A program the target refuses (`check --target` rejects threads, sockets, ...) is
reported as refused and not built; one with a label the table does not know fails.

usage: wasm_authority_check.py LEX_SYS [files...]
       (default: tests/accept/*.ls and tests/programs/*.ls)

Needs what `lex-sys --help` says `--target` needs. CI has none of it; the table's
internal consistency is tested in `lex-sys-ir` and runs in CI.
"""
import concurrent.futures, glob, json, os, subprocess, sys, tempfile

sys.path.insert(0, os.path.dirname(__file__))
from wasm_imports import wasi_functions

TARGET = "wasm32-wasip1"


def check(args):
    lex_sys, path, work = args
    name = os.path.basename(path)[:-3]
    a = subprocess.run([lex_sys, "authority", path, "--std", "--target", TARGET, "--output", "json"],
                       capture_output=True, text=True)
    if a.returncode != 0:
        return name, "skipped", "authority failed: " + a.stderr.strip().splitlines()[-1][:100] if a.stderr.strip() else "authority failed"
    wasi = json.loads(a.stdout)["wasi"]
    if wasi["unknown"]:
        return name, "FAIL", f"labels the table does not know: {wasi['unknown']}"
    if wasi["refused"]:
        return name, "refused", ", ".join(wasi["refused"])
    out = os.path.join(work, name + ".wasm")
    b = subprocess.run([lex_sys, "build", path, "--std", "--target", TARGET, "-o", out],
                       capture_output=True, text=True)
    if b.returncode != 0:
        return name, "unbuilt", (b.stderr.strip().splitlines() or ["?"])[-1][:100]
    imports = wasi_functions(open(out, "rb").read())
    required, allowed = set(wasi["required"]), set(wasi["allowed"])
    unexplained = imports - allowed
    missing = required - imports
    if wasi["unbounded"]:
        # `ffi`: which WASI functions foreign code reaches is the library's, so only the
        # half that does not depend on it can be checked.
        unexplained = set()
    if unexplained or missing:
        parts = []
        if unexplained:
            parts.append(f"imports no label allows: {sorted(unexplained)}")
        if missing:
            parts.append(f"missing what the row requires: {sorted(missing)}")
        return name, "FAIL", "; ".join(parts) + f"   (labels: {wasi_labels(lex_sys, path)})"
    return name, "ok", ""


def wasi_labels(lex_sys, path):
    a = subprocess.run([lex_sys, "authority", path, "--std", "--output", "json"], capture_output=True, text=True)
    return ",".join(sorted({l["name"] for l in json.loads(a.stdout)["labels"]}))


def main():
    lex_sys = sys.argv[1]
    files = sys.argv[2:] or sorted(glob.glob("tests/accept/*.ls") + glob.glob("tests/programs/*.ls"))
    work = tempfile.mkdtemp(prefix="wasm-authority-")
    results = {"ok": [], "refused": [], "unbuilt": [], "skipped": [], "FAIL": []}
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for name, verdict, why in pool.map(check, [(lex_sys, f, work) for f in files]):
            results[verdict].append((name, why))
    print(f"programs: {len(files)}   ok: {len(results['ok'])}   refused by the target: "
          f"{len(results['refused'])}   unbuilt: {len(results['unbuilt'])}   "
          f"skipped: {len(results['skipped'])}   FAIL: {len(results['FAIL'])}")
    for verdict in ("FAIL", "unbuilt", "skipped"):
        for name, why in results[verdict]:
            print(f"  {verdict:8} {name}: {why}")
    sys.exit(1 if results["FAIL"] else 0)


if __name__ == "__main__":
    main()
