#!/usr/bin/env python3
"""What a client certificate costs `packages/tls`'s server (docs/tls-server.md §13.12), measured.

    python3 scripts/tls_server_clientauth_cost.py <tls_serve> [<seconds> [<rounds>]] [--pki <directory>]
    python3 scripts/tls_server_clientauth_cost.py --make-pki <directory>

`scripts/tls_server_cost.py`'s method with another load: `openssl s_time` sends the leaf of a client certificate and
never the intermediate behind it, and counts a handshake complete before the server has judged the client's
certificate, so it cannot measure a chain of two. Python's `ssl` sends the whole chain file (`load_cert_chain`) and,
asked for one byte back from the echo server, shows that the server accepted the client. Each handshake is a TLS 1.3
connection with an X25519 share, one byte out and back, and a close; one after another for `seconds` (default 10) a
cell; the server process's CPU time (user and system, `/proc/<pid>/stat`, or `ps` on macOS) is divided by the
handshakes completed, and a cell with one that failed is reported as such, not as a measurement. The rows, in
rounds (default 5) so that the machine's slow moments fall on every row:

    no client authentication         the server of §6
    optional, the client sends none  the CertificateRequest, and an empty Certificate back
    required, <key>                  the client's chain verified and its CertificateVerify checked: a P-256 key under the
                                     CA, a P-256 key under an intermediate (a chain of two), P-384, RSA-2048 (PSS), Ed25519

The table is each row's median and range of the rounds, in milliseconds of server CPU a handshake. Linux or macOS. Pin
the server and the client to the cores you mean to measure on (`taskset -c 6 python3 ...` pins both).
"""
import os
import socket
import ssl
import statistics
import subprocess
import sys
import tempfile
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))


def cpu_seconds(pid):
    """A process's user + system CPU time: /proc on Linux, `ps` (to a hundredth of a second) on macOS."""
    if os.path.exists(f"/proc/{pid}/stat"):
        fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
        return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
    out = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    days, _, rest = out.rpartition("-")
    total = 0.0
    for part in rest.split(":"):
        total = total * 60 + float(part)
    return total + (int(days) * 86400 if days else 0)

KINDS = ["p256", "p256-via-intermediate", "p384", "rsa2048", "ed25519"]


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Server:
    """`tls_serve` in `echo` mode on a free port, with `--client-ca clients_ca.pem <how>` when asked."""

    def __init__(self, exe, work, how):
        self.port = free_port()
        argv = [exe, str(self.port), "echo", "http/1.1,mqtt", "0", "-", "-", "main.pem", "main.key",
                "srv.example,*.wild.example", "other.pem", "other.key", "other.example"]
        if how:
            argv += ["--client-ca", "clients_ca.pem", how]
        self.proc = subprocess.Popen(argv, cwd=work, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
        if self.proc.stdout.readline().strip() != "listening":
            raise RuntimeError("tls_serve did not start")
        threading.Thread(target=lambda: [None for _ in self.proc.stdout], daemon=True).start()

    def stop(self):
        self.proc.kill()
        self.proc.wait()


def busy(cpu):
    """Seconds `cpu` has spent not idle, from /proc/stat (Linux)."""
    for line in open("/proc/stat"):
        f = line.split()
        if f[0] == f"cpu{cpu}":
            ticks = sum(int(x) for x in f[1:9]) - int(f[4]) - int(f[5])  # not idle, not iowait
            return ticks / os.sysconf("SC_CLK_TCK")
    return 0.0


def neighbours():
    """(the cpu this process is pinned to, its hyper-thread siblings) on Linux, else (None, [])."""
    cpus = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else []
    if len(cpus) != 1:
        return None, []
    try:
        text = open(f"/sys/devices/system/cpu/cpu{cpus[0]}/topology/thread_siblings_list").read().strip()
    except OSError:
        return cpus[0], []
    sibs = []
    for part in text.split(","):
        lo, _, hi = part.partition("-")
        sibs += list(range(int(lo), int(hi or lo) + 1))
    return cpus[0], [c for c in sibs if c != cpus[0]]


PERF_EVENTS = "cpu_core/instructions/u,cpu_core/cycles/u"


def perf_works():
    """Whether `perf stat` counts user-space instructions of a process here (Linux, a hybrid Intel P-core, pinned)."""
    try:
        r = subprocess.run(["perf", "stat", "-x,", "-e", PERF_EVENTS, "--", "true"], capture_output=True, text=True)
    except OSError:
        return False
    return r.returncode == 0 and "not counted" not in r.stderr and "not supported" not in r.stderr and r.stderr.strip() != ""


def perf_start(pid):
    return subprocess.Popen(["perf", "stat", "-x,", "-e", PERF_EVENTS, "-p", str(pid)], stdout=subprocess.DEVNULL,
                            stderr=subprocess.PIPE, text=True)


def perf_stop(proc):
    """(user instructions, user cycles) the process counted, after SIGINT."""
    proc.send_signal(2)
    out = proc.communicate()[1]
    got = {}
    for line in out.splitlines():
        f = line.split(",")
        if len(f) > 2 and f[0].isdigit():
            got[f[2]] = int(f[0])
    return got.get("cpu_core/instructions/u", 0), got.get("cpu_core/cycles/u", 0)


def cell(exe, work, how, cert, seconds, counting=False):
    """(handshakes, server CPU seconds, handshakes that failed, how much of the pinned cpu and of its siblings was not
    ours) for one server and one client certificate. The measurement is worth taking only when the last two are small:
    another process on the cpu, or on the hyper-thread that shares its core, slows both of ours."""
    cpu, sibs = neighbours()
    server = Server(exe, work, how)
    try:
        ctx = ssl.create_default_context(cafile=os.path.join(work, "ca.pem"))
        ctx.minimum_version = ssl.TLSVersion.TLSv1_3
        ctx.set_ecdh_curve("X25519")
        if cert:
            ctx.load_cert_chain(os.path.join(work, f"{cert}.pem"), os.path.join(work, f"{cert}.key"))
        done = failed = 0
        counter = perf_start(server.proc.pid) if counting else None
        before = cpu_seconds(server.proc.pid)
        mine_before = time.process_time()
        busy_before = busy(cpu) if cpu is not None else 0.0
        sib_before = sum(busy(c) for c in sibs)
        wall_before = time.monotonic()
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            try:
                with socket.create_connection(("127.0.0.1", server.port), timeout=10) as raw:
                    with ctx.wrap_socket(raw, server_hostname="srv.example") as tls:
                        tls.sendall(b"x")
                        if tls.recv(1) == b"x":
                            done += 1
                        else:
                            failed += 1
            except (ssl.SSLError, OSError):
                failed += 1
        wall = time.monotonic() - wall_before
        served = cpu_seconds(server.proc.pid) - before
        ours = served + time.process_time() - mine_before
        other = max(0.0, (busy(cpu) - busy_before - ours) / wall) if cpu is not None else 0.0
        sibling = (sum(busy(c) for c in sibs) - sib_before) / wall if sibs else 0.0
        instructions = perf_stop(counter) if counter else (0, 0)
        return done, served, failed, other, sibling, instructions
    finally:
        server.stop()


def main():
    args = [a for i, a in enumerate(sys.argv) if not a.startswith("--") and not (i > 0 and sys.argv[i - 1] in ("--pki", "--make-pki"))]
    exe = os.path.abspath(args[1]) if len(args) > 1 else ""
    seconds = int(args[2]) if len(args) > 2 else 10
    rounds = int(args[3]) if len(args) > 3 else 5
    if "--make-pki" in sys.argv:
        # The certificates (pyca/cryptography), into a directory to carry to a machine that has none:
        # `--pki <dir>` there.
        import tls_server_clientauth_interop as auth
        import tls_server_interop as interop
        work = sys.argv[sys.argv.index("--make-pki") + 1]
        os.makedirs(work, exist_ok=True)
        interop.authority(work)
        auth.clients_authority(work)
        print(f"the certificates are in {work}")
        return
    if "--pki" in sys.argv:
        work = os.path.abspath(sys.argv[sys.argv.index("--pki") + 1])
    else:
        import tls_server_clientauth_interop as auth
        import tls_server_interop as interop
        work = tempfile.mkdtemp(prefix="tls-clientauth-cost-")
        interop.authority(work)
        auth.clients_authority(work)
    os.chdir(work)
    rows = [("no client authentication", None, None), ("optional, the client sends none", "optional", None)]
    rows += [(f"required, {kind}", "required", kind) for kind in KINDS]
    per = {name: [] for name, _, _ in rows}
    bad = {name: 0 for name, _, _ in rows}
    skipped = 0
    counting = perf_works()
    instructions = {name: [] for name, _, _ in rows}
    for r in range(1, rounds + 1):
        for name, how, cert in rows:
            # Instructions retired do not depend on who else is on the machine, so a counted cell is taken as it comes;
            # milliseconds do, and a cell is kept for them only when nobody else was on the cpu or its sibling. Without
            # `perf`, a cell that was not clean is run again.
            for attempt in range(1 if counting else 6):
                done, cpu, failed, other, sibling, instr = cell(exe, work, how, cert, seconds, counting)
                clean = other < 0.05 and sibling < 0.15
                print(f"round {r} {name:36} {done:>6} handshakes {failed:>3} failed {cpu:>6.2f} s "
                      f"{cpu / done * 1000 if done else float('nan'):>7.2f} ms  others on the cpu {other:.0%}, "
                      f"on its sibling {sibling:.0%}"
                      + (f"  {instr[0] / done / 1e6:.2f} M instructions" if instr[0] and done else "")
                      + ("" if clean else "  (milliseconds not taken)"), flush=True)
                bad[name] += failed
                if done and not failed:
                    if instr[0]:
                        instructions[name].append(instr[0] / done / 1e6)
                    if clean:
                        per[name].append(cpu / done * 1000)
                if clean:
                    break
                skipped += 1
                if not counting:
                    time.sleep(20)
    print(f"\n{'row':38} {'median':>8} {'min':>7} {'max':>7} {'vs none':>8}  (ms of server CPU a handshake, {rounds} rounds)")
    base = statistics.median(per["no client authentication"]) if per["no client authentication"] else float("nan")
    for name, _, _ in rows:
        v = per[name]
        if v:
            print(f"{name:38} {statistics.median(v):>8.2f} {min(v):>7.2f} {max(v):>7.2f} {statistics.median(v) - base:>+8.2f}"
                  + (f"   ({bad[name]} handshakes failed)" if bad[name] else ""))
        else:
            print(f"{name:38} no round that was both clean and without a failed handshake")
    if counting:
        print(f"\n{'row':38} {'median':>8} {'min':>7} {'max':>7} {'vs none':>8}  (millions of user-space instructions a handshake, perf stat, {rounds} rounds)")
        ibase = statistics.median(instructions["no client authentication"]) if instructions["no client authentication"] else float("nan")
        for name, _, _ in rows:
            v = instructions[name]
            if v:
                print(f"{name:38} {statistics.median(v):>8.2f} {min(v):>7.2f} {max(v):>7.2f} {statistics.median(v) - ibase:>+8.2f}")
    print(f"{skipped} cells had another process on the cpu or its hyper-thread sibling")
    print(f"the files of the run are in {work}")


if __name__ == "__main__":
    main()
