#!/usr/bin/env python3
"""What `std.gcm` costs per record size, before and after, beside OpenSSL's `speed` (docs/gcm-wide.md §8).

    python3 scripts/gcm_cost.py <gcm_cost> [--baseline <gcm_cost built by the old compiler>] [--pin <cpu>] [--runs 3] [--mb 128]

`gcm_cost` is `tests/programs/gcm_cost.cho` built with `cancho build --std`. Each of the programs and
`openssl speed -evp aes-128-gcm aes-256-gcm` (64, 1,024 and 16,384 byte records; the comparison is with the
seal, which is what `speed` times) is run `runs` times in turn, on the one CPU `--pin` names when it is given
(`taskset -c` on Linux), and the best run of each is kept: the machines this is run on are shared and the
fastest run is the one least disturbed. The table is MB/s, 10^6 bytes a second, for a seal with a prepared key
and 13 bytes of associated data (an open costs the same within a few percent: the program prints both).
On Linux with `perf`, `--cycles` also prints cycles per byte, which does not depend on the clock the CPU
chose; the CPU's model and the load average come first, since they belong with the numbers.
"""
import os
import platform
import re
import shutil
import subprocess
import sys

SIZES = (64, 1024, 16384)


def pinned(cmd, cpu):
    return (["taskset", "-c", str(cpu)] if cpu is not None and shutil.which("taskset") else []) + cmd


def cpu_model():
    try:
        if platform.system() == "Darwin":
            return subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True).stdout.strip()
        for line in open("/proc/cpuinfo"):
            if line.startswith(("model name", "Model name")):
                return line.split(":", 1)[1].strip()
        return platform.processor() or platform.machine()
    except OSError:
        return platform.machine()


def run_cost(exe, key, cpu, mb):
    """{(op, size): MB/s} from one run of `gcm_cost`."""
    out = subprocess.run(pinned([exe, str(key), "hw", str(mb)], cpu), capture_output=True, text=True, check=True).stdout
    rows = {}
    for line in out.splitlines():
        op, size, ns, mbs = line.split()
        rows[(op, int(size))] = float(mbs)
    return rows


def run_openssl(cipher, size, cpu):
    out = subprocess.run(pinned(["openssl", "speed", "-evp", cipher, "-seconds", "2", "-bytes", str(size)], cpu),
                         capture_output=True, text=True).stdout
    m = re.search(r"^" + re.escape(cipher.upper()) + r"\s+([0-9.]+)k", out, re.M)
    return float(m.group(1)) / 1000 if m else None


def openssl_cycles_per_byte(cipher, size, cpu):
    """OpenSSL's user cycles per byte, from `perf stat` around `openssl speed`, or None."""
    if not shutil.which("perf") or not shutil.which("openssl"):
        return None
    for event in ("cpu_core/cycles/u", "cycles:u"):
        r = subprocess.run(pinned(["perf", "stat", "-x,", "-e", event, "openssl", "speed", "-evp", cipher, "-seconds", "2", "-bytes", str(size)], cpu),
                           capture_output=True, text=True)
        done = re.search(r":\s*(\d+) " + re.escape(cipher.upper()) + r" ops in", r.stdout + r.stderr)
        for line in r.stderr.splitlines():
            if "cycles" in line and done:
                try:
                    return float(line.split(",")[0]) / (int(done.group(1)) * size)
                except ValueError:
                    pass
    return None


def best_of(n, f, *args):
    """The smallest of `n` answers of `f`, ignoring the ones that are None."""
    values = [v for v in (f(*args) for _ in range(n)) if v is not None]
    return min(values) if values else None


def cycles_per_byte(exe, key, cpu, size, mb=32):
    """User cycles per byte sealed and opened, from `perf stat`, or None."""
    if not shutil.which("perf"):
        return None
    for event in ("cpu_core/cycles/u", "cycles:u"):
        r = subprocess.run(pinned(["perf", "stat", "-x,", "-e", event, exe, str(key), "hw", str(mb), str(size)], cpu),
                           capture_output=True, text=True)
        for line in r.stderr.splitlines():
            if event.split("/")[-2 if "/" in event else 0] in line or "cycles" in line:
                try:
                    return float(line.split(",")[0]) / (6 * mb * 1048576)
                except ValueError:
                    pass
    return None


def main():
    args = sys.argv[1:]
    exe = os.path.abspath(args[0])
    opt = lambda name, default=None: args[args.index(name) + 1] if name in args else default
    base = os.path.abspath(opt("--baseline")) if opt("--baseline") else None
    cpu = opt("--pin")
    runs = int(opt("--runs", 3))
    mb = int(opt("--mb", 128))
    load = os.getloadavg()
    print(f"{cpu_model()}, {platform.system()} {platform.machine()}, load average {load[0]:.1f} {load[1]:.1f} {load[2]:.1f}, "
          f"pinned to {cpu if cpu is not None else 'no CPU'}, best of {runs}")
    for key in (16, 32):
        cipher = f"aes-{key * 8}-gcm"
        best = {"after": {}, "before": {}, "openssl": {}}
        for _ in range(runs):
            for name, program in (("before", base), ("after", exe)):
                if program:
                    for k, v in run_cost(program, key, cpu, mb).items():
                        best[name][k] = max(best[name].get(k, 0), v)
            if shutil.which("openssl"):
                for size in SIZES:
                    v = run_openssl(cipher, size, cpu)
                    if v:
                        best["openssl"][("seal", size)] = max(best["openssl"].get(("seal", size), 0), v)
        print(f"\n{cipher.upper()} seal, MB/s" + ("" if shutil.which("openssl") else " (no openssl on PATH)"))
        print(f"{'size':>7} {'before':>9} {'after':>9} {'x':>6} {'openssl':>9} {'after/openssl':>14}   open after")
        for size in SIZES:
            a, b, o = best["after"].get(("seal", size)), best["before"].get(("seal", size)), best["openssl"].get(("seal", size))
            gain = f"{a / b:.1f}" if a and b else "-"
            ratio = f"{100 * a / o:.0f}%" if a and o else "-"
            print(f"{size:>7} {b or 0:>9.0f} {a or 0:>9.0f} {gain:>6} {o or 0:>9.0f} {ratio:>14}   {best['after'].get(('open', size), 0):.0f}")
        if "--cycles" in args:
            for size in SIZES:
                after = best_of(3, cycles_per_byte, exe, key, cpu, size)
                before = best_of(3, cycles_per_byte, base, key, cpu, size) if base else None
                ssl = best_of(3, openssl_cycles_per_byte, cipher, size, cpu)
                print(f"  cycles/byte at {size}: before {before and round(before, 2)} after {after and round(after, 2)} "
                      f"openssl {ssl and round(ssl, 2)}" + (f" (OpenSSL takes {ssl / after:.0%} of the cycles a byte)" if ssl and after else ""))


if __name__ == "__main__":
    main()
