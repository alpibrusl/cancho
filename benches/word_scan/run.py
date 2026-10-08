#!/usr/bin/env python3
"""docs/word-scan.md section 5: build scan.cho once per kernel and backend, run each binary, report per pass.

    python3 benches/word_scan/run.py [--size BYTES] [--runs N] [--backend llvm|cranelift]... [--cancho PATH] [--prefix CMD]

Instructions and cycles are the counter of the whole process (fill included), taken from `perf stat` on Linux and
`/usr/bin/time -l` on macOS; kernel 0 only fills, so (kernel - kernel 0) / 3 is one pass. Milliseconds are the process's
user CPU time (the children's `ru_utime`) less kernel 0's *of the same round*, over three, the median of the rounds. An
in-program clock was tried first and dropped, because the optimiser moved the pure call across the clock reading; the
best of the runs of each kernel against the best of kernel 0 was tried second and dropped, because the fill's variance on
a shared machine made some differences negative. `--prefix` is put before the
binary, e.g. `--prefix "nice -n 19 taskset -c 0-5"`. Nothing is run in parallel.
"""
import argparse, os, platform, re, resource, shlex, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
NAMES = {
    1: "count ','   byte loop", 2: "count ','   load_le64 + SWAR", 3: "count ','   byte_mask64",
    4: "find last   byte loop", 5: "find last   load_le64 + SWAR + tz", 6: "find last   byte_mask64 + tz",
    7: "find last   index_of_byte (memchr)", 8: "3 structural byte loop", 9: "3 structural byte_mask64 x3",
}


def counters(cmd):
    """Run `cmd`; answer (stdout words, instructions, cycles, seconds) of the process."""
    started = resource.getrusage(resource.RUSAGE_CHILDREN).ru_utime
    if platform.system() == "Darwin":
        r = subprocess.run(["/usr/bin/time", "-l"] + cmd, capture_output=True, text=True)
        ins = re.search(r"(\d+)\s+instructions retired", r.stderr)
        cyc = re.search(r"(\d+)\s+cycles elapsed", r.stderr)
    else:
        r = subprocess.run(
            ["perf", "stat", "-x,", "-e", "cpu_core/instructions/u,cpu_core/cycles/u"] + cmd,
            capture_output=True, text=True)
        ins = re.search(r"^(\d+),,cpu_core/instructions/u", r.stderr, re.M)
        cyc = re.search(r"^(\d+),,cpu_core/cycles/u", r.stderr, re.M)
    if r.returncode != 0:
        sys.exit(f"{cmd} failed: {r.stderr[-400:]}")
    seconds = resource.getrusage(resource.RUSAGE_CHILDREN).ru_utime - started
    return r.stdout.split(), int(ins.group(1)) if ins else 0, int(cyc.group(1)) if cyc else 0, seconds


def median(xs):
    xs = sorted(xs)
    return xs[len(xs) // 2]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--size", type=int, default=1 << 30)
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--backend", action="append")
    ap.add_argument("--cancho", default=os.path.join(HERE, "..", "..", "target", "release", "cancho"))
    ap.add_argument("--prefix", default="")
    a = ap.parse_args()
    source = open(os.path.join(HERE, "scan.cho")).read()
    prefix = shlex.split(a.prefix)
    tmp = tempfile.mkdtemp(prefix="word-scan-")
    for backend in a.backend or ["llvm", "cranelift"]:
        print(f"== {backend}, {a.size} bytes, median of {a.runs} rounds ({platform.machine()})")
        exes = []
        for k in range(10):
            src, exe = os.path.join(tmp, f"k{k}.cho"), os.path.join(tmp, f"k{k}-{backend}")
            open(src, "w").write(source.replace("KERNEL", str(k)).replace("SIZE", str(a.size)))
            b = subprocess.run([a.cancho, "build", "--std", src, "--backend", backend, "-o", exe],
                               capture_output=True, text=True)
            if b.returncode:
                sys.exit(b.stderr)
            exes.append(exe)
        # Each round runs every kernel once, kernel 0 first: a kernel is measured against the fill of its own round, so
        # the fill's variance on a shared machine (page faults, THP) cancels instead of leaking into the difference.
        deltas = {k: ([], [], []) for k in range(1, 10)}
        answers = {}
        for _ in range(a.runs):
            base = None
            for k in range(10):
                out, ins, cyc, seconds = counters(prefix + [exes[k]])
                if k == 0:
                    base = (ins, cyc, seconds)
                    continue
                answers[k] = int(out[0]) // 3
                for slot, v in enumerate((ins - base[0], cyc - base[1], seconds - base[2])):
                    deltas[k][slot].append(v)
        for k in range(1, 10):
            ins, cyc, sec = (median(d) for d in deltas[k])
            print(f"{k} {NAMES[k]:36} answer {answers[k]:>10}  {sec / 3 * 1000:7.1f} ms/pass  "
                  f"{ins / 3 / a.size:6.3f} instr/B  {cyc / 3 / a.size:6.3f} cycles/B")
        print()


main()
