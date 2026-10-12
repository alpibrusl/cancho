#!/usr/bin/env python3
"""The CPU of a full and of a resumed TLS 1.3 handshake, on the server and on the client, cancho's beside OpenSSL's on the
same machine (docs/tls-performance.md §2.2 and §10, which use it).

    python3 scripts/tls_handshake_cost.py <bin> [--seconds 8] [--rounds 3] [--cpu 0] [--peer-cpu 1] [<row> ...]

`<bin>` is a directory holding `tls_echo` (examples/tls_echo), `tls_many` (tests/programs/tls_many.cho, both built with
`packages/tls` and `packages/x509` as CI builds them), and `tconn` and `oserv` (benches/server/, `gcc -O2 ... -lssl
-lcrypto`). Linux only (`/proc`, `taskset`). The identity is `tests/vectors/tls/echo/first` (an ECDSA P-256 leaf signed by
a P-256 CA); the cipher AES-128-GCM and the group X25519 on every row. The process measured runs on `--cpu`, its peer on
`--peer-cpu`.

Rows (all by default), each run `--rounds` times in turn so that a machine whose clock drifts moves them together:

    server-full      the server's CPU a full handshake under `openssl s_time -new`: `tls_echo`, `openssl s_server
                     -num_tickets 0`, and `oserv` (an OpenSSL server with nothing else in it)
    server-resumed   the server's CPU a resumed handshake (`psk_dhe_ke`): `tconn ... resume` against `tls_echo --tickets 2`
                     and against `openssl s_server` (two tickets, its default); `s_time -reuse` does not resume TLS 1.3
    client-full      the client's CPU a full handshake, the chain and the name verified: `tls_many` with 200 connections
                     less `tls_many` with 1, over 199 (each also seals a 16 KiB request), and `tconn` (no request), both
                     against `openssl s_server -num_tickets 0`
    client-resumed   the client's CPU a resumed handshake: `tls_many ... resume` with 200 (200 full, then 200 resumed) less
                     `tls_many` with 200, over 200; and `tconn ... resume`, both against `openssl s_server` with tickets

For each row it prints the handshakes, the CPU a handshake (user and system, from `/proc/<pid>/stat` or `wait4`), and,
where `perf stat` can count, cycles and instructions a handshake: user-space only with `kernel.perf_event_paranoid` at 2
(the default here); user and kernel with `--sudo-perf`, which runs `perf` through `sudo -n` (the process measured still
runs as the caller: a client is started through `sudo -u`). The kernel's share is where a server's sockets, `brk` and
page faults are (docs/tls-performance.md §3.5), so the comparison that counts it is the one to quote.
"""
import argparse
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
VECTORS = os.path.join(HERE, "..", "tests", "vectors", "tls", "echo")
HOST = "echo.lex-sys.test"


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def wait_port(port, timeout=10):
    end = time.time() + timeout
    while time.time() < end:
        try:
            socket.create_connection(("127.0.0.1", port), timeout=1).close()
            return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError(f"nothing listening on {port}")


def cpu_seconds(pid):
    fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


PERF = ["perf"]
EVENTS = "cycles:u,instructions:u"


def perf_ok(sudo):
    global PERF, EVENTS
    if not shutil.which("perf"):
        return False
    if sudo:
        PERF, EVENTS = ["sudo", "-n", "perf"], "cycles,instructions"
    r = subprocess.run(PERF + ["stat", "-x,", "-e", EVENTS.split(",")[0], "true"], capture_output=True, text=True,
                       env={**os.environ, "LC_ALL": "C"})
    return any(l.split(",")[0].strip().isdigit() for l in r.stderr.splitlines())


def parse_perf(text):
    """Cycles and instructions from `perf stat -x,`; on a hybrid CPU the P-core's (`cpu_core/...`), where it runs."""
    cyc = ins = None
    for line in text.splitlines():
        f = line.split(",")
        if len(f) < 3 or not f[0].strip().isdigit():
            continue
        if "cycles" in f[2] and "atom" not in f[2]:
            cyc = int(f[0])
        elif "instructions" in f[2] and "atom" not in f[2]:
            ins = int(f[0])
    return cyc, ins


def pinned(cpu, cmd):
    return ["taskset", "-c", str(cpu)] + cmd


class Server:
    """A server process pinned to a CPU, on a free port, its output thrown away."""

    def __init__(self, cmd, cpu, port, cwd):
        self.proc = subprocess.Popen(pinned(cpu, cmd), cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                     stdin=subprocess.DEVNULL)
        wait_port(port)
        self.port = port

    def stop(self):
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()


def measure_server(server, client_cmd, peer_cpu, use_perf, count_re):
    """Run the client to completion against `server`; the server's CPU, cycles and instructions over the client's
    count of handshakes."""
    perf = None
    if use_perf:
        perf = subprocess.Popen(PERF + ["stat", "-x,", "-e", EVENTS, "-p", str(server.proc.pid)],
                                stderr=subprocess.PIPE, stdout=subprocess.DEVNULL, text=True, env={**os.environ, "LC_ALL": "C"})
        time.sleep(0.3)
    before = cpu_seconds(server.proc.pid)
    out = subprocess.run(pinned(peer_cpu, client_cmd), capture_output=True, text=True)
    time.sleep(0.5)
    cpu = cpu_seconds(server.proc.pid) - before
    cyc = ins = None
    if perf:
        perf.send_signal(signal.SIGINT)
        cyc, ins = parse_perf(perf.communicate()[1])
    m = re.search(count_re, out.stdout)
    if not m:
        raise RuntimeError(f"no count from {client_cmd[0]}: {out.stdout[-300:]!r} {out.stderr[-300:]!r}")
    return int(m.group(1)), cpu, cyc, ins


def run_client(cmd, cpu, use_perf, stdin=None):
    """A client run to completion on `cpu`: its CPU (wait4), and its user cycles and instructions."""
    full = pinned(cpu, cmd)
    if use_perf:
        if PERF[0] == "sudo":
            # sudo clears the environment; GLIBC_TUNABLES (the allocator experiment, §10.4) is passed on by name.
            keep = [f"GLIBC_TUNABLES={os.environ['GLIBC_TUNABLES']}"] if "GLIBC_TUNABLES" in os.environ else []
            full = ["sudo", "-n", "-u", os.environ.get("USER", "nobody"), "env"] + keep + full
        full = PERF + ["stat", "-x,", "-e", EVENTS, "--"] + full
    p = subprocess.Popen(full, stdin=open(stdin) if stdin else subprocess.DEVNULL, stdout=subprocess.PIPE,
                         stderr=subprocess.PIPE, text=True, env={**os.environ, "LC_ALL": "C"})
    out, err = p.stdout.read(), p.stderr.read()
    _, status, ru = os.wait4(p.pid, 0)
    p.returncode = os.waitstatus_to_exitcode(status)
    cyc, ins = parse_perf(err) if use_perf else (None, None)
    return out, err, ru.ru_utime + ru.ru_stime, cyc, ins, p.returncode


def fmt(name, n, cpu, cyc, ins):
    per = f"{cpu / n * 1e3:8.3f} ms CPU" if n else "no handshakes"
    unit = "" if PERF[0] == "sudo" else ":u"
    extra = f"  {cyc / n / 1e6:7.3f} M cycles{unit}  {ins / n / 1e6:7.3f} M instructions{unit}" if n and cyc and ins else ""
    return f"{name:44} {n:>7} handshakes  {per}{extra}"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("bin")
    ap.add_argument("rows", nargs="*", default=["server-full", "server-resumed", "client-full", "client-resumed"])
    ap.add_argument("--seconds", type=float, default=8)
    ap.add_argument("--rounds", type=int, default=3)
    ap.add_argument("--cpu", type=int, default=0)
    ap.add_argument("--peer-cpu", type=int, default=1)
    ap.add_argument("--sudo-perf", action="store_true", help="count the kernel's cycles too, through `sudo -n perf`")
    a = ap.parse_intermixed_args()
    b = os.path.abspath(a.bin)
    use_perf = perf_ok(a.sudo_perf)
    work = tempfile.mkdtemp(prefix="tls_hs_cost_")
    for f in ("chain.pem", "key.pem", "names"):
        shutil.copyfile(os.path.join(VECTORS, "first", f), os.path.join(work, f))
    ca = os.path.abspath(os.path.join(VECTORS, "ca.pem"))
    what = ("user and kernel" if a.sudo_perf else "user space") if use_perf else "none"
    print(f"perf counters: {what}; load {os.getloadavg()}", flush=True)
    secs = str(int(a.seconds))
    big = ["--connections", "256", "--handshakes", "256", "--rate", "100000", "--per-address", "1024",
           "--per-address-rate", "100000"]

    def ossl_server(port, tickets):
        return ["openssl", "s_server", "-accept", str(port), "-tls1_3", "-groups", "X25519", "-ciphersuites",
                "TLS_AES_128_GCM_SHA256", "-cert", "chain.pem", "-key", "key.pem", "-num_tickets", str(tickets), "-quiet",
                "-HTTP"]

    s_time = ["openssl", "s_time", "-new", "-time", secs, "-CAfile", ca]

    try:
        for rnd in range(a.rounds):
            print(f"-- round {rnd + 1} of {a.rounds}, load {os.getloadavg()}", flush=True)
            if "server-full" in a.rows:
                for name, cmd in (("server-full cancho tls_echo", lambda p: [f"{b}/tls_echo", "--port", str(p), "--dir", work] + big),
                                  ("server-full openssl s_server", lambda p: ossl_server(p, 0)),
                                  ("server-full openssl oserv", lambda p: [f"{b}/oserv", str(p), "chain.pem", "key.pem"])):
                    port = free_port()
                    srv = Server(cmd(port), a.cpu, port, work)
                    try:
                        n, cpu, cyc, ins = measure_server(srv, s_time + ["-connect", f"127.0.0.1:{port}"], a.peer_cpu, use_perf,
                                                          r"(\d+) connections in [\d.]+s")
                    finally:
                        srv.stop()
                    print(fmt(name, n, cpu, cyc, ins), flush=True)
            if "server-resumed" in a.rows:
                for name, cmd in (("server-resumed cancho tls_echo --tickets 2",
                                   lambda p: [f"{b}/tls_echo", "--port", str(p), "--dir", work, "--tickets", "2"] + big),
                                  ("server-resumed openssl s_server", lambda p: ossl_server(p, 2))):
                    port = free_port()
                    srv = Server(cmd(port), a.cpu, port, work)
                    try:
                        n, cpu, cyc, ins = measure_server(srv, [f"{b}/tconn", str(port), secs, ca, HOST, "resume"], a.peer_cpu,
                                                          use_perf, r"^(\d+)")
                    finally:
                        srv.stop()
                    print(fmt(name, n, cpu, cyc, ins), flush=True)
            for kind, tickets in (("client-full", 0), ("client-resumed", 2)):
                if kind not in a.rows:
                    continue
                port = free_port()
                srv = Server(ossl_server(port, tickets), a.peer_cpu, port, work)
                try:
                    many = [f"{b}/tls_many", "127.0.0.1", str(port), HOST]
                    if kind == "client-full":
                        r200 = run_client(many + ["200", "65536"], a.cpu, use_perf, ca)
                        r1 = run_client(many + ["1", "65536"], a.cpu, use_perf, ca)
                        for r in (r200, r1):
                            if "failed=0" not in r[0]:
                                raise RuntimeError(f"tls_many failed: {r[0][-200:]!r} {r[1][-200:]!r}")
                        n = 199
                        cpu = r200[2] - r1[2]
                        cyc = r200[3] - r1[3] if use_perf else None
                        ins = r200[4] - r1[4] if use_perf else None
                        print(fmt("client-full cancho tls_many (200 less 1)", n, cpu, cyc, ins), flush=True)
                        t = run_client([f"{b}/tconn", str(port), secs, ca, HOST], a.cpu, use_perf)
                        print(fmt("client-full openssl tconn", int(t[0].split()[0]), t[2], t[3], t[4]), flush=True)
                    else:
                        rr = run_client(many + ["200", "65536", "resume"], a.cpu, use_perf, ca)
                        rf = run_client(many + ["200", "65536"], a.cpu, use_perf, ca)
                        resumed = rr[0].count(" resumed")
                        if resumed != 200 or "failed=0" not in rf[0]:
                            raise RuntimeError(f"tls_many resume: {resumed} resumed: {rr[0][-300:]!r}")
                        cpu = rr[2] - rf[2]
                        cyc = rr[3] - rf[3] if use_perf else None
                        ins = rr[4] - rf[4] if use_perf else None
                        print(fmt("client-resumed cancho tls_many (resume less full)", 200, cpu, cyc, ins), flush=True)
                        t = run_client([f"{b}/tconn", str(port), secs, ca, HOST, "resume"], a.cpu, use_perf)
                        if t[5] != 0:
                            raise RuntimeError(f"tconn resume failed: {t[1][-300:]!r}")
                        print(fmt("client-resumed openssl tconn", int(t[0].split()[0]), t[2], t[3], t[4]), flush=True)
                finally:
                    srv.stop()
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
