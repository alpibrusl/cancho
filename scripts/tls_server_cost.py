#!/usr/bin/env python3
"""The CPU cost of one full handshake on `packages/tls`'s server (docs/tls-server.md §6), measured.

    python3 scripts/tls_server_cost.py <tls_serve> [<seconds>] [--tickets [--rounds <n>]]

`tls_serve` is `tests/programs/tls_serve.cho` built with the LLVM backend (the default) and `packages/tls`. It runs in
`echo` mode with a P-256 identity; `openssl s_time -new` makes full handshakes against it, one after another, for
`seconds` (default 20) a row, and the server's CPU time (user and system, from `/proc/<pid>/stat`) is divided by the
handshakes it completed. Linux only. A row per group the client sends a share of (OpenSSL's `Groups`, through
`OPENSSL_CONF`), and one where the share is P-521's, so a HelloRetryRequest to P-256 comes first. The suite is the
server's choice: AES-128-GCM where the CPU has AES instructions. A last row is `openssl s_server` with the same
identity under the same client, for scale.

With `--tickets` (docs/tls-server.md §12.9) the rows are, for X25519 and for P-256: a full handshake with tickets off,
a full handshake with tickets on (the server makes and sends two NewSessionTickets), and `openssl s_time -reuse`
against tickets on: one full handshake and then, for `seconds`, connections that offer its ticket (Python's `ssl`:
`openssl s_time -reuse` does not resume a TLS 1.3 session), the server's CPU divided by the connections whose `conn`
line says `resumed`. The first two measure what issuing tickets costs a full handshake; the third what a resumption
costs. The client's own CPU is not the server's: they are separate processes.
"""
import os
import re
import socket
import ssl
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_server_interop as interop  # noqa: E402


def cpu_seconds(pid):
    fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    tick = os.sysconf("SC_CLK_TCK")
    return (int(fields[11]) + int(fields[12])) / tick


def row(exe, work, groups, seconds, tickets="-", reuse=False):
    server = interop.Server(exe, "echo", work, tickets=tickets)
    try:
        conf = interop.openssl_conf(work, "TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256", groups)
        before = cpu_seconds(server.proc.pid)
        mark = server.mark()
        if reuse:
            ctx = ssl.create_default_context(cafile="ca.pem")
            ctx.minimum_version = ssl.TLSVersion.TLSv1_3
            ctx.set_ciphers("DEFAULT")
            ctx.set_groups(groups) if hasattr(ctx, "set_groups") else None

            def connect(session=None):
                raw = socket.create_connection(("127.0.0.1", server.port), timeout=10)
                c = ctx.wrap_socket(raw, server_hostname="srv.example", session=session)
                c.sendall(b"x")
                c.recv(1)
                done = (c.session_reused, c.session)
                c.close()
                return done

            _, session = connect()
            before = cpu_seconds(server.proc.pid)
            mark = server.mark()
            end = time.time() + seconds
            while time.time() < end:
                connect(session)
            time.sleep(1)
            after = cpu_seconds(server.proc.pid)
            with server.cond:
                lines = [l for l in server.lines[mark:] if l.startswith("conn ")]
            resumed = sum(1 for l in lines if " resumed " in l)
            return resumed, after - before, len(lines)
        r = subprocess.run(["openssl", "s_time", "-connect", f"127.0.0.1:{server.port}", "-reuse" if reuse else "-new",
                        "-time", str(seconds), "-CAfile", "ca.pem"], cwd=work, env=dict(os.environ, OPENSSL_CONF=conf),
                       capture_output=True, timeout=seconds + 60)
        time.sleep(1)
        after = cpu_seconds(server.proc.pid)
        with server.cond:
            lines = [l for l in server.lines[mark:] if l.startswith("conn ")]

        # `s_time` counts the connections whose handshake it completed; it then drops the socket without a
        # close_notify, so the server's line for each says `socket`.
        m = re.search(r"(\d+) connections in [\d.]+s", r.stdout.decode())
        return int(m.group(1)) if m else 0, after - before, len(lines)
    finally:
        server.stop()


def openssl_row(work, seconds):
    port = interop.free_port()
    proc = subprocess.Popen(["openssl", "s_server", "-accept", str(port), "-tls1_3", "-cert", "main.pem", "-key",
                             "main.key", "-num_tickets", "0", "-quiet"], cwd=work, stdin=subprocess.PIPE,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(1)
        conf = interop.openssl_conf(work, "TLS_AES_128_GCM_SHA256", "X25519")
        before = cpu_seconds(proc.pid)
        r = subprocess.run(["openssl", "s_time", "-connect", f"127.0.0.1:{port}", "-new", "-time", str(seconds),
                            "-CAfile", "ca.pem"], cwd=work, env=dict(os.environ, OPENSSL_CONF=conf),
                           capture_output=True, timeout=seconds + 60)
        time.sleep(1)
        after = cpu_seconds(proc.pid)
        m = re.search(r"(\d+) connections in [\d.]+s", r.stdout.decode())
        return int(m.group(1)) if m else 0, after - before
    finally:
        proc.kill()
        proc.wait()


def tickets_rows(exe, work, seconds, rounds):
    """The three rows of each group, `rounds` times round, alternating, so a machine whose speed changes over the minutes
    (a shared one, a laptop) moves all three together; the median and the best ms a handshake of each."""
    import statistics
    kinds = [("full, tickets off", "-", False), ("full, tickets on (2 sent)", "2:3600", False),
             ("resumed (Python ssl), tickets on", "2:3600", True)]
    print(f"{'row':44} {'median ms':>10} {'best ms':>8} {'handshakes in all':>18}  ({rounds} rounds of {seconds} s)")
    for name, groups in [("X25519", "X25519"), ("P-256", "P-256")]:
        per = {k[0]: [] for k in kinds}
        total = {k[0]: 0 for k in kinds}
        for _ in range(rounds):
            for what, tickets, reuse in kinds:
                n, cpu, lines = row(exe, work, groups, seconds, tickets, reuse)
                if n:
                    per[what].append(cpu / n * 1000)
                total[what] += n
        full = statistics.median(per[kinds[0][0]])
        for what, _, _ in kinds:
            med, best = statistics.median(per[what]), min(per[what])
            print(f"{name + ' ' + what:44} {med:>10.2f} {best:>8.2f} {total[what]:>18}   "
                  f"{med / full:.2f} of the median full handshake", flush=True)


def main():
    exe = os.path.abspath(sys.argv[1])
    seconds = int(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2].isdigit() else 20
    work = tempfile.mkdtemp(prefix="tls-server-cost-")
    os.chdir(work)
    interop.authority(work)
    if "--tickets" in sys.argv:
        rounds = int(sys.argv[sys.argv.index("--rounds") + 1]) if "--rounds" in sys.argv else 5
        tickets_rows(exe, work, seconds, rounds)
        return
    print(f"{'client share':28} {'handshakes':>10} {'server CPU s':>12} {'ms a handshake':>15} {'a second a core':>16}")
    for name, groups in [("X25519", "X25519"), ("P-256", "P-256"), ("P-384", "P-384"),
                         ("P-521, then a retry to P-256", "P-521:P-256")]:
        n, cpu, lines = row(exe, work, groups, seconds)
        per = cpu / n * 1000 if n else float("nan")
        print(f"{name:28} {n:>10} {cpu:>12.2f} {per:>15.2f} {1000 / per if n else 0:>16.0f}"
              + ("" if n == lines else f"   ({lines - n} connections not completed)"))
    n, cpu = openssl_row(work, seconds)
    per = cpu / n * 1000 if n else float("nan")
    print(f"{'openssl s_server, X25519':28} {n:>10} {cpu:>12.2f} {per:>15.2f} {1000 / per if n else 0:>16.0f}")


if __name__ == "__main__":
    main()
