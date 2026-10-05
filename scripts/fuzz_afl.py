#!/usr/bin/env python3
"""AFL++ over the fuzzing harnesses (docs/tls-assurance.md §3).

    python3 scripts/fuzz_afl.py <work> <harness>:<seconds> [<harness>:<seconds> ...]
    python3 scripts/fuzz_afl.py <work> --report
    python3 scripts/fuzz_afl.py <work> --minimize

The harnesses are `tests/programs/fuzz_<harness>.ls`: der, chain, messages,
client and flight. Each named one is built with `afl-clang-fast` as the LLVM
backend's `CLANG` and the linker `CC` (so every edge is instrumented and
nothing in the compiler changes), seeded from `scripts/fuzz_corpus.py seeds`
and from the committed corpus in `tests/vectors/fuzz/<harness>/`, and run
for its seconds, all of them at once, one core each. A run already under
`<work>` is resumed, so a campaign can be split across invocations.

`--report` prints each harness's executions, run time, edges and crashes
and hangs. `--minimize` replaces `tests/vectors/fuzz/<harness>/` with
`afl-cmin`'s minimum of the queue. Exit status 1 if any harness has a crash
or a hang.

Needs `afl++` (`apt-get install afl++`) and a release build of the compiler
(`cargo build --release -p lex-sys`).
"""
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import fuzz_corpus  # noqa: E402

HARNESSES = ["der", "chain", "messages", "client", "flight"]
PACKAGES = [f"packages/tls/{f}.ls" for f in ("record", "message", "slot", "client12", "client")] + \
    [f"packages/x509/{f}.ls" for f in ("verify", "names", "x509")]
SHARED = ["tests/programs/fuzz_common.ls", "tests/programs/fuzz_fixture.ls"]
ENV = dict(os.environ, AFL_SKIP_CPUFREQ="1", AFL_NO_UI="1",
           AFL_I_DONT_CARE_ABOUT_MISSING_CRASHES="1", AFL_NO_AFFINITY="0")


def compiler():
    exe = os.path.join(ROOT, "target/release/lex-sys")
    if not os.path.exists(exe):
        raise SystemExit("build the compiler first: cargo build --release -p lex-sys")
    return exe


def build(work, harness):
    out = os.path.join(work, f"fuzz_{harness}")
    env = dict(os.environ, CLANG="afl-clang-fast", CC="afl-clang-fast", AFL_QUIET="1")
    files = [os.path.join(ROOT, f) for f in [f"tests/programs/fuzz_{harness}.ls"] + SHARED + PACKAGES]
    subprocess.run([compiler(), "build", "--std", *files, "-o", out], env=env, check=True)
    return out


def seeds(work, harness):
    """The generated seeds and the committed corpus, in one directory;
    `afl-cmin`'s minimum of them when there are many."""
    generated = os.path.join(work, "seeds")
    if not os.path.isdir(generated):
        fuzz_corpus.seeds(generated)
    into = os.path.join(work, f"in_{harness}")
    if os.path.isdir(into):
        return into
    pool = os.path.join(work, f"pool_{harness}")
    os.makedirs(pool, exist_ok=True)
    for d in (os.path.join(generated, harness), os.path.join(ROOT, "tests/vectors/fuzz", harness)):
        if os.path.isdir(d):
            for name in os.listdir(d):
                shutil.copy(os.path.join(d, name), os.path.join(pool, f"{os.path.basename(os.path.dirname(d))}_{name}"))
    if len(os.listdir(pool)) > 200:
        subprocess.run(["afl-cmin", "-i", pool, "-o", into, "--", os.path.join(work, f"fuzz_{harness}")],
                       env=ENV, check=True, stdout=subprocess.DEVNULL)
    else:
        shutil.copytree(pool, into)
    return into


def stats(work, harness):
    path = os.path.join(work, f"out_{harness}", "default", "fuzzer_stats")
    if not os.path.exists(path):
        return None
    out = {}
    for line in open(path):
        k, _, v = line.partition(":")
        out[k.strip()] = v.strip()
    return out


def report(work):
    bad = 0
    total = 0
    print(f"{'harness':10} {'executions':>14} {'hours':>7} {'per second':>11} {'edges':>13} {'crashes':>8} {'hangs':>6}")
    for h in HARNESSES:
        s = stats(work, h)
        if s is None:
            continue
        execs = int(s["execs_done"])
        hours = (int(s["last_update"]) - int(s["start_time"])) / 3600
        total += execs
        bad += int(s["saved_crashes"]) + int(s["saved_hangs"])
        rate = execs / (hours * 3600) if hours else 0
        print(f"{h:10} {execs:>14,} {hours:>7.2f} {rate:>11,.0f} {s['edges_found'] + ' of ' + s['total_edges']:>13} "
              f"{s['saved_crashes']:>8} {s['saved_hangs']:>6}")
    print(f"{'total':10} {total:>14,}")
    return bad


def minimize(work):
    for h in HARNESSES:
        queue = os.path.join(work, f"out_{h}", "default", "queue")
        if not os.path.isdir(queue):
            continue
        dest = os.path.join(ROOT, "tests/vectors/fuzz", h)
        tmp = os.path.join(work, f"cmin_{h}")
        shutil.rmtree(tmp, ignore_errors=True)
        subprocess.run(["afl-cmin", "-i", queue, "-o", tmp, "--", os.path.join(work, f"fuzz_{h}")],
                       env=ENV, check=True, stdout=subprocess.DEVNULL)
        shutil.rmtree(dest, ignore_errors=True)
        os.makedirs(dest)
        for i, name in enumerate(sorted(os.listdir(tmp))):
            shutil.copy(os.path.join(tmp, name), os.path.join(dest, f"{i:04d}"))
        print(f"{h}: {len(os.listdir(dest))} inputs in {dest}")


def run(work, plan):
    procs = []
    for harness, seconds in plan:
        exe = build(work, harness)
        out = os.path.join(work, f"out_{harness}")
        # `-i -` resumes a run already under `out`.
        source = "-" if os.path.isdir(os.path.join(out, "default", "queue")) else seeds(work, harness)
        log = open(os.path.join(work, f"{harness}.log"), "a")
        procs.append(subprocess.Popen(
            ["afl-fuzz", "-V", str(seconds), "-i", source, "-o", out, "--", exe],
            env=ENV, stdout=log, stderr=subprocess.STDOUT))
    for p in procs:
        p.wait()


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    work = os.path.abspath(sys.argv[1])
    os.makedirs(work, exist_ok=True)
    if sys.argv[2] == "--report":
        sys.exit(1 if report(work) else 0)
    if sys.argv[2] == "--minimize":
        minimize(work)
        return
    plan = []
    for arg in sys.argv[2:]:
        h, _, secs = arg.partition(":")
        if h not in HARNESSES or not secs.isdigit():
            raise SystemExit(f"not <harness>:<seconds>: {arg}")
        plan.append((h, int(secs)))
    run(work, plan)
    sys.exit(1 if report(work) else 0)


if __name__ == "__main__":
    main()
