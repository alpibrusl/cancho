#!/usr/bin/env python3
"""W0 coverage map (docs/wasm.md): run every accept fixture for wasm32-wasip1.

For each `tests/accept/*.ls`, build and run it with `--target wasm32-wasip1` and
compare against the fixture's own `//~ STDOUT` / `//~ EXIT` / `//~ STDIN`
annotations. Each file lands in exactly one bucket:

  pass     built, ran, and matched
  trap     the fixture expects a native trap (exit >= 128) and wasmtime trapped
           (exit 134) -- the exit codes differ by design, the behaviour does not
  refused  the compiler declined, with a located error (the table entry kind)
  wrong    built and ran but disagreed with the annotations (a bug)

usage: wasm_coverage.py LEX_SYS [files...]    (default: tests/accept/*.ls)
Needs CLANG, WASI_SYSROOT and wasmtime on the path, as `lex-sys --help` says.
"""
import concurrent.futures, glob, re, subprocess, sys

def expectations(path):
    out, exit_code, stdin = [], 0, None
    for line in open(path):
        m = re.match(r"//~ (STDOUT|EXIT|STDIN)\s?(.*)$", line.rstrip("\n"))
        if not m:
            continue
        kind, rest = m.groups()
        if kind == "STDOUT": out.append(rest)
        elif kind == "EXIT": exit_code = int(rest)
        elif kind == "STDIN": stdin = (stdin or "") + rest + "\n"
    return "".join(l + "\n" for l in out), exit_code, stdin

def run_one(args):
    lex, path = args
    want_out, want_exit, stdin = expectations(path)
    try:
        p = subprocess.run([lex, "run", path, "--std", "--target", "wasm32-wasip1"],
                           input=(stdin or "").encode(), capture_output=True, timeout=120)
    except subprocess.TimeoutExpired:
        return path, "wrong", "timeout"
    out, err = p.stdout.decode(errors="replace"), p.stderr.decode(errors="replace")
    if p.returncode in (1, 3) and ("error:" in err) and not out:
        first = next((l for l in err.splitlines() if "error:" in l), err[:200])
        return path, "refused", first.split("error:", 1)[1].strip()[:160]
    trapped = p.returncode == 134 and "unreachable" in err
    if want_exit >= 128 and trapped and out == want_out:
        return path, "trap", ""
    if out == want_out and p.returncode == want_exit:
        return path, "pass", ""
    why = f"exit {p.returncode} (want {want_exit})" + ("" if out == want_out else ", stdout differs")
    return path, "wrong", why

def main():
    lex = sys.argv[1]
    files = sys.argv[2:] or sorted(glob.glob("tests/accept/*.ls"))
    buckets = {"pass": [], "trap": [], "refused": [], "wrong": []}
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        for path, bucket, why in pool.map(run_one, [(lex, f) for f in files]):
            buckets[bucket].append((path, why))
    for name in ("pass", "trap", "refused", "wrong"):
        print(f"{name}: {len(buckets[name])}")
    for name in ("wrong", "refused"):
        reasons = {}
        for path, why in buckets[name]:
            reasons.setdefault(why, []).append(path)
        for why, paths in sorted(reasons.items(), key=lambda kv: -len(kv[1])):
            print(f"\n[{name}] {len(paths)}x {why}")
            for p in paths: print("   ", p)
if __name__ == "__main__":
    main()
