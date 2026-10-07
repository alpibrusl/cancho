#!/usr/bin/env python3
"""The CPU cost of one full handshake on `packages/tls`'s server (docs/tls-server.md §6), measured.

    python3 scripts/tls_server_cost.py <tls_serve> [<seconds>]

`tls_serve` is `tests/programs/tls_serve.ls` built with the LLVM backend (the default) and `packages/tls`. It runs in
`echo` mode with a P-256 identity; `openssl s_time -new` makes full handshakes against it, one after another, for
`seconds` (default 20) a row, and the server's CPU time (user and system, from `/proc/<pid>/stat`) is divided by the
handshakes it completed. Linux only. A row per group the client sends a share of (OpenSSL's `Groups`, through
`OPENSSL_CONF`), and one where the share is P-521's, so a HelloRetryRequest to P-256 comes first. The suite is the
server's choice: AES-128-GCM where the CPU has AES instructions. A last row is `openssl s_server` with the same
identity under the same client, for scale.
"""
import os
import re
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


def row(exe, work, groups, seconds):
    server = interop.Server(exe, "echo", work)
    try:
        conf = interop.openssl_conf(work, "TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256", groups)
        before = cpu_seconds(server.proc.pid)
        mark = server.mark()
        r = subprocess.run(["openssl", "s_time", "-connect", f"127.0.0.1:{server.port}", "-new", "-time", str(seconds),
                        "-CAfile", "ca.pem"], cwd=work, env=dict(os.environ, OPENSSL_CONF=conf),
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


def main():
    exe = os.path.abspath(sys.argv[1])
    seconds = int(sys.argv[2]) if len(sys.argv) > 2 else 20
    work = tempfile.mkdtemp(prefix="tls-server-cost-")
    os.chdir(work)
    interop.authority(work)
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
