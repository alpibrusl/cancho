#!/usr/bin/env python3
"""The cost of each primitive the TLS stack is made of, measured and guarded (docs/tls-performance.md §3.1, §6).

    cancho build --std tests/programs/tls_prims_bench.cho -o tls_prims_bench
    python3 scripts/tls_perf.py table  ./tls_prims_bench [--repeats 5] [--target 300] [--json out.json]
    python3 scripts/tls_perf.py check  ./tls_prims_bench [--baseline benches/tls_baseline.json] [--factor 1.3]
    python3 scripts/tls_perf.py record ./tls_prims_bench --machine <tag> [--baseline benches/tls_baseline.json]
    python3 scripts/tls_perf.py profile <perf script output>... [--total-us <CPU a handshake in microseconds>] [--top 40] [--memory]
    python3 scripts/tls_perf.py openssl [--seconds 3]

`table` prints, for every operation of `tests/programs/tls_prims_bench.cho`, the least of `--repeats` runs of at least
`--target` milliseconds each, in nanoseconds a call. Where `perf stat` can count user cycles (Linux, with the
permission), it also runs each operation twice with a fixed number of calls and prints cycles and instructions a call
from the *difference* of the two runs, so the setup cancels and the figure does not depend on the clock's frequency: on a
laptop-class CPU whose clock moves by a third with the load on the other cores, that is the figure to compare.

`check` is the regression guard (§6). Its first choice, when `valgrind` is installed, is to count the instructions each
operation executes (callgrind's `Ir`, from the difference of two runs with different numbers of calls, so the setup cancels):
that count does not depend on the clock, on the load or on what else shares the core, which on the machine of §2 moved the
time of the same operation by a factor of two within a minute. It fails an operation whose count is more than `--factor`
(default 1.3) times the baseline's or less than its reciprocal (a primitive that got that much cheaper re-records its baseline
in the same change, §1.3), one the baseline has and this run lacks, and one this run has and the baseline lacks. A run under
valgrind that does not exit 0 with its `checksum` line is an error, not a count. Where there is no valgrind it falls back to
the time of each operation divided by the same run's `calibrate` (a loop that no change to a primitive touches; on
x86-64 the compiler reduces it to about one instruction a step, docs/tls-performance.md §6), compared at `--time-factor` (default 2.0, because the quotient moved by 1.6 between two runs minutes apart
on that machine), and only against an entry recorded on the same system and architecture: a time from a Linux VM says
nothing about macOS on the same chip. §6 says what each catches and what neither can.

Baseline entries are keyed `<system>-<architecture>` (`linux-x86_64`, `linux-aarch64`). `record` writes the current run's
instruction counts (with valgrind) and quotients under `--machine`, by default this machine's key.

`profile` sorts the samples of a `perf record --call-graph dwarf,32768 -p <pid>` of a server (or of a client run in a loop)
into the categories of docs/tls-performance.md §3.3, by which frames each stack passes through, and prints each as a share of
the samples and, given `--total-us` (the CPU a handshake or a request costs, measured on its own), as microseconds. Make the
input with `perf script -F comm,ip,sym > file`. The categories are exclusive and tested in this order: memory (malloc, free,
brk and the page faults they cause), then the primitives (X25519, ECDSA verify, ECDSA sign, HKDF and HMAC, SHA-2, the AEADs),
then system calls, then the rest of the program's own code. `--memory` splits the memory category: how much of it is in the
kernel (the `brk` system call, page faults) and which of the program's functions made the call (docs/tls-performance.md §10.4).

`openssl` runs `openssl speed` for the same primitives (`ecdsap256`, `ecdhx25519`, `ecdhp256`, SHA-256 and SHA-384 at 64,
1,024 and 16,384 bytes, HMAC-SHA-256 at 64, and AES-128-GCM and ChaCha20-Poly1305 sealed as a TLS record is, `-aead`, at the
three sizes) and prints the time a call; where `perf stat` can count, also cycles a call, which is the time a call times the
run's cycles over its task-clock (§2.2). Pin it to the core the table ran on (`taskset -c 0`).
"""
import argparse
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys

CAL = "calibrate/0"


def run_bench(exe, target, repeats, only=0, rounds=0, size=0):
    out = subprocess.run([exe, str(target), str(repeats), str(only), str(rounds), str(size)], capture_output=True, text=True,
                         check=True).stdout
    rows = {}
    for line in out.splitlines():
        f = line.split()
        if len(f) == 4 and f[1].isdigit():
            rows[f"{f[0]}/{f[1]}"] = (int(f[2]), int(f[3]))
    return rows


def perf_counts(exe, op, size, rounds, tries=5):
    """The least user cycles and instructions of `tries` runs of `rounds` calls: a sibling hardware thread that is busy
    makes the same work take more cycles, never fewer, so the minimum is the least disturbed run."""
    cmd = ["perf", "stat", "-x,", "-e", "cycles:u,instructions:u", exe, "1", "1", str(op), str(rounds), str(size)]
    cyc = ins = None
    for _ in range(tries):
        r = subprocess.run(cmd, capture_output=True, text=True)
        for line in r.stderr.splitlines():
            f = line.split(",")
            if len(f) < 4 or not f[0].strip().isdigit():
                continue
            if "cycles" in f[2] and "atom" not in f[2]:
                cyc = int(f[0]) if cyc is None else min(cyc, int(f[0]))
            elif "instructions" in f[2] and "atom" not in f[2]:
                ins = int(f[0]) if ins is None else min(ins, int(f[0]))
    return cyc, ins


def machine():
    info = {"arch": platform.machine(), "kernel": platform.release(), "system": platform.system()}
    try:
        for line in open("/proc/cpuinfo"):
            if line.startswith("model name"):
                info["cpu"] = line.split(":", 1)[1].strip()
                break
        info["governor"] = open("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").read().strip()
    except OSError:
        pass
    try:
        info["thp"] = re.search(r"\[(\w+)\]", open("/sys/kernel/mm/transparent_hugepage/enabled").read()).group(1)
    except (OSError, AttributeError):
        pass
    info["load"] = os.getloadavg()
    return info


def can_perf():
    if not shutil.which("perf"):
        return False
    r = subprocess.run(["perf", "stat", "-x,", "-e", "cycles:u", "true"], capture_output=True, text=True)
    return any(l.split(",")[0].strip().isdigit() for l in r.stderr.splitlines())


def numbers(exe, target, repeats, with_perf):
    # `repeats` full passes, each op's least time kept: the machine's noise is mostly slow outliers.
    best = {}
    for _ in range(repeats):
        for key, (ns, rounds) in run_bench(exe, target, 1).items():
            if key not in best or ns < best[key][0]:
                best[key] = (ns, rounds)
    result = {k: {"ns": ns, "rounds": rounds} for k, (ns, rounds) in best.items()}
    if with_perf:
        ops = {key: op for key, op, _ in op_keys(exe)}
        for key, row in result.items():
            size = key.split("/")[1]
            op = ops[key]
            n2 = max(4, row["rounds"])
            n1 = max(1, n2 // 4)
            c1, i1 = perf_counts(exe, op, int(size), n1)
            c2, i2 = perf_counts(exe, op, int(size), n2)
            if None not in (c1, c2, i1, i2) and n2 > n1:
                row["cycles"] = round((c2 - c1) / (n2 - n1), 1)
                row["instructions"] = round((i2 - i1) / (n2 - n1), 1)
    return result


def cmd_table(a):
    info = machine()
    print("machine:", json.dumps(info))
    res = numbers(a.exe, a.target, a.repeats, can_perf())
    print(f"{'operation':34} {'size':>6} {'ns/call':>10} {'cycles/call':>12} {'instr/call':>12} {'IPC':>5}")
    for key, row in res.items():
        name, size = key.split("/")
        cyc, ins = row.get("cycles"), row.get("instructions")
        print(f"{name:34} {size:>6} {row['ns']:>10} {cyc if cyc is not None else '':>12} {ins if ins is not None else '':>12} "
              f"{(round(ins / cyc, 2) if cyc and ins else ''):>5}")
    if a.json:
        json.dump({"machine": info, "ops": res}, open(a.json, "w"), indent=1)


def quotients(exe, target, repeats):
    res = {}
    for _ in range(repeats):
        rows = run_bench(exe, target, 1)
        cal = rows[CAL][0]
        for key, (ns, _) in rows.items():
            q = ns / cal
            res[key] = min(res.get(key, q), q)
    return res


def op_keys(exe):
    """(key, operation number, size) for every row the program prints: the numbers are the order of first appearance by name."""
    rows = run_bench(exe, 1, 1)
    names = []
    for key in rows:
        n = key.split("/")[0]
        if n not in names:
            names.append(n)
    return [(key, names.index(key.split("/")[0]) + 1, int(key.split("/")[1])) for key in rows]


def rounds_for(name, size):
    """Two numbers of calls whose difference is a few million instructions or more, whatever the operation costs."""
    if name.startswith(("x25519", "p256", "ecdsa")):
        return 1, 3
    if size == 16384:
        return 2, 6
    if size == 1024:
        return 20, 60
    if name.startswith(("sha", "hmac", "aes128gcm", "chacha")) and size == 64:
        return 200, 600
    return 100, 300


def callgrind_ir(exe, op, size, rounds):
    """The instructions one run executes under callgrind. The run must exit 0 and print its `checksum` line: a program
    that trapped part way would otherwise be counted as cheap."""
    cmd = ["valgrind", "--tool=callgrind", "--callgrind-out-file=/dev/null", exe, "1", "1", str(op), str(rounds), str(size)]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0 or not any(l.startswith("checksum ") for l in r.stdout.splitlines()):
        raise RuntimeError(f"operation {op} ({size} bytes, {rounds} calls) did not finish under valgrind: exit {r.returncode}, "
                           f"stdout {r.stdout[-200:]!r}, stderr {r.stderr[-300:]!r}")
    m = re.search(r"Collected : (\d+)", r.stderr)
    if not m:
        raise RuntimeError(f"no instruction count from valgrind: {r.stderr[-300:]}")
    return int(m.group(1))


def instruction_counts(exe):
    """Instructions a call executes, per row, from two runs under callgrind."""
    out = {}
    for key, op, size in op_keys(exe):
        n1, n2 = rounds_for(key.split("/")[0], size)
        out[key] = (callgrind_ir(exe, op, size, n2) - callgrind_ir(exe, op, size, n1)) / (n2 - n1)
    return out


def arch_tag():
    """The baseline entry for this machine: operating system and architecture (`linux-x86_64`, `linux-aarch64`,
    `darwin-aarch64`). Instruction counts depend on the code the compiler emits for that pair; times depend on the machine."""
    arch = {"x86_64": "x86_64", "amd64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}.get(platform.machine().lower(),
                                                                                          platform.machine().lower())
    return f"{platform.system().lower()}-{arch}"


def cmd_check(a):
    base = json.load(open(a.baseline))
    tag = a.machine or arch_tag()
    if tag not in base["machines"]:
        print(f"no baseline for {tag!r} in {a.baseline} (has {sorted(base['machines'])}); run `record`", file=sys.stderr)
        return 2
    entry = base["machines"][tag]
    mode = a.mode
    if mode == "auto":
        mode = "instructions" if shutil.which("valgrind") and "instructions" in entry else "time"
    if mode == "time" and tag != arch_tag():
        # A time is a property of the machine that recorded it; another system's entry measures nothing here.
        print(f"time mode against {tag!r} on a {arch_tag()} machine: refused; record a baseline here", file=sys.stderr)
        return 2
    bad = 0
    if mode == "instructions":
        want, got, factor, unit = entry["instructions"], instruction_counts(a.exe), a.factor, "instructions a call"
    else:
        want, got, factor, unit = entry["quotients"], quotients(a.exe, a.target, a.repeats), a.time_factor, "ns over the calibration's"
    print(f"baseline {tag} ({entry['description']}); {mode}: {unit}")
    print(f"{'operation':34} {'size':>6} {'now':>14} {'recorded':>14} {'ratio':>7}")
    for key, w in want.items():
        name, size = key.split("/")
        if key not in got:
            print(f"{name:34} {size:>6} {'missing':>14}  NOT MEASURED: the baseline has it")
            bad += 1
            continue
        ratio = got[key] / w
        flag = ""
        if ratio > factor:
            flag = f"  SLOWER THAN {factor}x"
            bad += 1
        elif ratio < 1 / factor:
            # §1.3: a primitive that got cheaper re-records its baseline in the same change, so it cannot drift back.
            flag = f"  CHEAPER THAN 1/{factor}: re-record the baseline"
            bad += 1
        print(f"{name:34} {size:>6} {got[key]:>14.2f} {w:>14.2f} {ratio:>7.2f}{flag}")
    for key in got:
        if key not in want:
            name, size = key.split("/")
            print(f"{name:34} {size:>6} {got[key]:>14.2f} {'none':>14}  NEW ROW: record it in the baseline")
            bad += 1
    total = len(set(want) | set(got))
    print(f"{total - bad} of {total} rows within {factor}x either way of the baseline")
    return 1 if bad else 0


def cmd_record(a):
    try:
        base = json.load(open(a.baseline))
    except OSError:
        base = {"note": "docs/tls-performance.md §6: per machine, the instructions a call of each primitive executes (valgrind) and "
                        "the quotients of ns a call over the `calibrate` operation's", "machines": {}}
    info = machine()
    entry = {"description": a.description or f"{info.get('cpu', info['arch'])}, {info['system']} {info['kernel']}",
             "quotients": {k: round(v, 3) for k, v in quotients(a.exe, a.target, a.repeats).items()}}
    if shutil.which("valgrind"):
        entry["instructions"] = {k: round(v, 1) for k, v in instruction_counts(a.exe).items()}
    base["machines"][a.machine] = entry
    json.dump(base, open(a.baseline, "w"), indent=1, sort_keys=True, ensure_ascii=False)
    open(a.baseline, "a").write("\n")
    print(f"recorded {len(entry['quotients'])} quotients and {len(entry.get('instructions', {}))} instruction counts for {a.machine} in {a.baseline}")
    return 0


# ---- profile ----

ALLOC = {"__libc_malloc", "_int_malloc", "_int_free", "__libc_free", "malloc", "free", "__brk", "__sbrk", "sysmalloc", "systrim",
         "cfree", "__libc_malloc2", "_int_free_chunk", "__default_morecore", "__glibc_morecore", "__GI___libc_free",
         "__GI___libc_malloc", "__GI___sbrk", "__x64_sys_brk", "__arm64_sys_brk", "__do_sys_brk"}
FAULT = {"asm_exc_page_fault", "exc_page_fault", "el0_da", "do_page_fault", "handle_mm_fault"}


def load_stacks(path):
    """Stacks root first, from `perf script -F comm,ip,sym`: a blank line ends a sample, a tab starts a frame."""
    stacks, cur = [], []
    for line in open(path, errors="replace"):
        line = line.rstrip("\n")
        if not line.strip():
            if cur:
                stacks.append(cur[::-1])
                cur = []
        elif line[0] == "\t":
            parts = line.split(None, 1)
            cur.append(parts[1].strip() if len(parts) > 1 else "?")
    if cur:
        stacks.append(cur[::-1])
    return stacks


def category(stack):
    def has(*prefixes):
        return any(f.startswith(prefixes) for f in stack)

    if any(f in ALLOC for f in stack):
        return "memory: malloc, free, brk"
    if any(f in FAULT for f in stack):
        return "memory: page faults (first touch, or pages the kernel took back)"
    # `std.field25519_51` (#393) is X25519's field alone in a TLS process; a ladder inlined into its caller leaves only it.
    if has("lexs_std.x25519.scalarmult", "lexs_std.field25519_51"):
        return "X25519 (key pair and shared secret)"
    if has("lexs_std.ecdh.shared", "lexs_std.ecdh.public_key") and not has("lexs_std.ecdsa_sign"):
        return "P-256/P-384 ECDH"
    # Since #397 a P-256 verification is `std.p256_vf.verify`, and the server's check after signing reaches it from
    # `tls_server.sign` with `ecdsa_sign.sign_checked` and `ecdsa.verify_raw` inlined away (docs/tls-performance.md §10).
    if has("lexs_std.ecdsa.verify", "lexs_std.p256_vf.verify"):
        if has("lexs_x509_verify"):
            return "ECDSA verify: the chain (CA signature on the leaf)"
        if has("lexs_tls_client.on_certificate_verify"):
            return "ECDSA verify: CertificateVerify"
        return "ECDSA verify: the server's check before sending"
    if has("lexs_std.ecdsa_sign"):
        return "ECDSA sign"
    if has("lexs_std.hkdf", "lexs_std.hmac"):
        return "HKDF, HMAC (key schedule, Finished)"
    if has("lexs_std.crypto"):
        return "SHA-256/384 (transcript, digests)"
    if has("lexs_tls_record", "lexs_std.gcm", "lexs_std.aes", "lexs_std.chacha", "cancho_ghash", "cancho_aes"):
        return "record AEAD, traffic-key setup"
    if has("lexs_x509"):
        return "certificate parsing"
    if any(f.startswith(("el0_svc", "entry_SYSCALL")) or f == "do_syscall_64" for f in stack):
        return "system calls (sockets, epoll)"
    if has("lexs_"):
        return "engine and event loop (parsing, copies, bookkeeping)"
    return "not attributed (interrupts, samples with no stack)"


def cmd_profile(a):
    stacks = []
    for path in a.files:
        stacks += load_stacks(path)
    n = len(stacks)
    counts = {}
    for st in stacks:
        counts[category(st)] = counts.get(category(st), 0) + 1
    print(f"{n} samples from {len(a.files)} file(s)")
    print(f"{'category':58} {'share':>7}" + (f" {'us':>9}" if a.total_us else ""))
    for k, v in sorted(counts.items(), key=lambda kv: -kv[1]):
        print(f"{k:58} {v / n * 100:6.2f}%" + (f" {v / n * a.total_us:9.1f}" if a.total_us else ""))
    if a.top:
        inc = {}
        for st in stacks:
            for f in set(st):
                inc[f] = inc.get(f, 0) + 1
        print(f"\ninclusive share of the {a.top} most common frames named lexs_*:")
        shown = 0
        for f, v in sorted(inc.items(), key=lambda kv: -kv[1]):
            if f.startswith("lexs_") and shown < a.top:
                print(f"{v / n * 100:6.2f}%  {f}")
                shown += 1
    if a.memory:
        memory_split(stacks)
    return 0


def memory_split(stacks):
    """The memory category's samples: in the kernel (the `brk` system call, page faults) or in the allocator's user code, and
    the program's function nearest the allocator on each stack (a `region`'s `malloc` or `free` is emitted into the function
    that opens it)."""
    n = len(stacks)
    mem = [st for st in stacks if any(f in ALLOC for f in st)]

    def share(pred):
        return sum(1 for st in mem if pred(st)) / n * 100 if n else 0.0

    brk = lambda st: any("brk" in f for f in st)  # noqa: E731
    fault = lambda st: any(f in FAULT for f in st)  # noqa: E731
    trim = lambda st: any(f.startswith("systrim") for f in st)  # noqa: E731
    print(f"\nmemory: {len(mem) / n * 100:.1f}% of the samples; in the `brk` system call {share(brk):.1f}%, in page faults "
          f"{share(fault):.1f}%, under glibc's `systrim` (giving the top of the heap back) {share(trim):.1f}%, neither "
          f"{share(lambda st: not brk(st) and not fault(st)):.1f}%")
    callers = {}
    for st in mem:
        own = [f for f in st if f.startswith("lexs_")]
        k = own[-1] if own else "(none)"
        callers[k] = callers.get(k, 0) + 1
    print("the program's function nearest the allocator:")
    for k, v in sorted(callers.items(), key=lambda kv: -kv[1])[:12]:
        print(f"{v / n * 100:6.2f}%  {k}")


SPEED = [("ecdsap256", ["ecdsap256"]), ("ecdhx25519", ["ecdhx25519"]), ("ecdhp256", ["ecdhp256"])]
SPEED += [(h, ["-evp", h, "-bytes", str(b)]) for h in ("sha256", "sha384") for b in (64, 1024, 16384)]
SPEED += [("hmac-sha256", ["-hmac", "sha256", "-bytes", "64"])]
SPEED += [(c, ["-aead", "-evp", c, "-bytes", str(b)]) for c in ("aes-128-gcm", "chacha20-poly1305") for b in (64, 1024, 16384)]


def cmd_openssl(a):
    """OpenSSL's cost of the same primitives on this machine (§2.2)."""
    env = {**os.environ, "LC_ALL": "C"}
    print("machine:", json.dumps(machine()))
    print(subprocess.run(["openssl", "version"], capture_output=True, text=True).stdout.strip())
    with_perf = can_perf()
    for name, args in SPEED:
        cmd = ["openssl", "speed", "-seconds", str(a.seconds)] + args
        if with_perf:
            cmd = ["perf", "stat", "-x,", "-e", "cycles:u,task-clock"] + cmd
        r = subprocess.run(cmd, capture_output=True, text=True, env=env)
        hz = None
        if with_perf:
            cyc = clock = None
            for line in r.stderr.splitlines():
                f = line.split(",")
                if len(f) < 3 or not f[0].replace(".", "").isdigit():
                    continue
                if "cycles" in f[2] and "atom" not in f[2]:
                    cyc = float(f[0])
                elif "task-clock" in f[2]:
                    clock = float(f[0]) / 1e3
            hz = cyc / clock if cyc and clock else None
        for line in (r.stderr + r.stdout).splitlines():
            m = re.search(r": (\d+) (.+?) in ([\d.]+)s", line)
            if m:
                ns = float(m.group(3)) / int(m.group(1)) * 1e9
                cyc = f"{ns * 1e-9 * hz:12.0f} cycles  ({hz / 1e9:.2f} GHz)" if hz else ""
                print(f"{name:18} {' '.join(args):38} {ns:12.1f} ns  {cyc}  | {line.strip()}")
    return 0


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    pp = sub.add_parser("profile")
    pp.add_argument("files", nargs="+")
    pp.add_argument("--total-us", type=float, default=0)
    pp.add_argument("--top", type=int, default=0)
    pp.add_argument("--memory", action="store_true", help="split the memory category: kernel or not, and by caller")
    po = sub.add_parser("openssl")
    po.add_argument("--seconds", type=int, default=3)
    for name in ("table", "check", "record"):
        p = sub.add_parser(name)
        p.add_argument("exe")
        p.add_argument("--repeats", type=int, default=5 if name == "table" else 3)
        p.add_argument("--target", type=int, default=300 if name == "table" else 100)
        if name == "table":
            p.add_argument("--json")
        else:
            p.add_argument("--baseline", default=os.path.join(here, "..", "benches", "tls_baseline.json"))
            p.add_argument("--machine", default=None if name == "check" else arch_tag(),
                           help="the baseline entry, by default this machine's `<system>-<architecture>`")
        if name == "check":
            p.add_argument("--factor", type=float, default=1.3, help="instruction counts: how many times the baseline fails")
            p.add_argument("--time-factor", type=float, default=2.0, help="timed quotients, without valgrind")
            p.add_argument("--mode", choices=("auto", "instructions", "time"), default="auto")
        if name == "record":
            p.add_argument("--description")
    a = ap.parse_args()
    if a.cmd == "profile":
        return cmd_profile(a)
    if a.cmd == "openssl":
        return cmd_openssl(a)
    a.exe = os.path.abspath(a.exe)
    return {"table": cmd_table, "check": cmd_check, "record": cmd_record}[a.cmd](a) or 0


if __name__ == "__main__":
    sys.exit(main())
