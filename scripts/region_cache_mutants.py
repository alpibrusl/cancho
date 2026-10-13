#!/usr/bin/env python3
"""Mutation check of the per-thread stack of region chunks (docs/region-chunk-cache.md section 6), the shape of
`scripts/word_scan_mutants.py`.

    python3 scripts/region_cache_mutants.py [--only <text in a mutant's name>] [--timeout SECONDS]

Each mutant is one deliberate bug at one site in the two backends' `region_cache.rs`, the entry point that makes the key,
or the bound in `cancho-ir`. The file is changed in place, `cargo test -p cancho --test conformance -- region_cache` runs
(it builds real programs with both backends and counts their chunks under an `LD_PRELOAD` shim, so **Linux only**), and
the file is put back, whatever happens. A mutant is killed when that run fails; one that does not compile proves nothing
and is an error, not a kill. The unmutated tree is run first and must pass. A run that outlives the timeout is killed and
counts as a kill by a hang. Exit status 1 if a mutant survives, an `old` text does not occur exactly once, or a mutant
fails to build.
"""
import argparse, os, signal, subprocess, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
LLVM = "crates/cancho-codegen-llvm/src/region_cache.rs"
CRANE = "crates/cancho-codegen/src/region_cache.rs"
CRANE_MAIN = "crates/cancho-codegen/src/emit.rs"
IR = "crates/cancho-ir/src/region_cache.rs"
CMD = ["cargo", "test", "-p", "cancho", "--test", "conformance", "--", "region_cache"]

MUTANTS = [
    ("llvm: no clearing", LLVM, 'call void @llvm.memset.p0.{st}(ptr %base, i8 0, {st} %used, i1 false)\\n', ''),
    ("llvm: the stack keeps one more (sgt)", LLVM, '%full = icmp sge i64 %n, {slots}', '%full = icmp sgt i64 %n, {slots}'),
    ("llvm: take never pops", LLVM, 'br i1 %empty, label %fresh, label %pop', 'br i1 true, label %fresh, label %pop'),
    ("llvm: give never keeps", LLVM, 'br i1 %full, label %release, label %keep', 'br i1 true, label %release, label %keep'),
    ("llvm: a thread's chunks are not freed when it ends", LLVM, 'call void @free(ptr %p)\\n  %next', '%next'),
    ("llvm: main makes no key", LLVM,
     'if threaded(triple) { format!("  call void @{REGION_INIT}()\\n") } else { String::new() }',
     'if threaded(triple) { String::new() } else { String::new() }'),
    ("llvm: the key is stored without the one", LLVM, '%plus = add i64 %key, 1', '%plus = add i64 %key, 0'),
    ("cranelift: no clearing", CRANE, 'call0(module, b, memset, &[base, zero, used]);', 'let _ = (memset, zero, used);'),
    ("cranelift: the stack keeps one more", CRANE, 'IntCC::SignedGreaterThanOrEqual, n, REGION_CACHE_SLOTS',
     'IntCC::SignedGreaterThan, n, REGION_CACHE_SLOTS'),
    ("cranelift: take never pops", CRANE, 'b.ins().brif(n, pop, &[], fresh, &[]);', 'b.ins().brif(n, fresh, &[], fresh, &[]);'),
    ("cranelift: give never keeps", CRANE, 'b.ins().brif(full, release, &[], keep, &[]);',
     'b.ins().brif(full, release, &[], release, &[]);'),
    ("cranelift: a thread's chunks are not freed when it ends", CRANE, 'call0(module, b, free, &[p]);\n            let next',
     'let _ = p;\n            let next'),
    ("cranelift: main makes no key", CRANE_MAIN, 'self.call_region_init(&mut builder);', ''),
    ("ir: fifteen slots", IR, 'pub const REGION_CACHE_SLOTS: i64 = 16;', 'pub const REGION_CACHE_SLOTS: i64 = 15;'),
]


def run(timeout):
    p = subprocess.Popen(CMD, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        out, _ = p.communicate(timeout=timeout)
        return p.returncode, out, False
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, signal.SIGKILL)
        out, _ = p.communicate()
        return 1, out, True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only")
    ap.add_argument("--timeout", type=int, default=1200)
    a = ap.parse_args()
    if not sys.platform.startswith("linux"):
        sys.exit("Linux only: the tests that kill these mutants count chunks with an LD_PRELOAD shim")
    status, out, _ = run(a.timeout)
    if status != 0:
        sys.exit("the unmutated tree does not pass:\n" + out[-2000:])
    bad = 0
    for name, rel, old, new in MUTANTS:
        if a.only and a.only not in name:
            continue
        path = os.path.join(ROOT, rel)
        source = open(path).read()
        if source.count(old) != 1:
            print(f"ERROR  {name}: `old` occurs {source.count(old)} times in {rel}")
            bad += 1
            continue
        open(path, "w").write(source.replace(old, new))
        try:
            status, out, hung = run(a.timeout)
        finally:
            open(path, "w").write(source)
        if "could not compile" in out or "error[E" in out:
            print(f"ERROR  {name}: the mutant does not build")
            bad += 1
        elif status == 0:
            print(f"SURVIVED  {name}")
            bad += 1
        else:
            print(f"killed{' (hang)' if hung else ''}  {name}", flush=True)
    sys.exit(1 if bad else 0)


main()
