#!/usr/bin/env python3
"""Resident memory per TLS connection, by phase (docs/tls-memory.md).

    python3 scripts/tls_memory.py cancho  <tls_echo>   [--counts 50,100,200,10000] [--json out.json]
    python3 scripts/tls_memory.py openssl <ossl_hold> [release] [...]
    python3 scripts/tls_memory.py https   <https_hello> [...]

Starts a server for each count N (so one count's heap never carries into the next) and opens N connections one after
another from this process. It reads the server's VmRSS (Linux `/proc/<pid>/status`) after each phase. The first server
is for the phases in which the connections have not finished their handshakes:

    start         the server up, no connection (its slots allocated: the boxes of docs/tls-core.md section 3)
    connected     N TCP connections, nothing sent (the slot is started and waits for a ClientHello)
    handshake     N clients have sent a ClientHello and read the server's whole flight, and have not sent their
                  Finished: all N handshakes in progress at once, which a program that bounds them never has

A second server is for the phases of established connections, handshaken one at a time, as a bound on handshakes in
progress makes them:

    established   every client's Finished sent and one byte echoed: N established connections, idle
    echoed        16,384 bytes sent and read back on each connection in turn, one connection at a time, then idle
                  again: what the traffic of a server that is not busy leaves resident
    partial       12,000 bytes of a 16,384-byte record sent on every connection at once, the rest held back: N
                  connections each with a record in flight
    all-echoed    the rest sent and read back on every connection, idle again (the high-water mark)

The figure for a phase is (RSS - RSS at `start`) / N. The certificates are the committed test identity in
tests/vectors/tls/echo (ECDSA P-256). `cancho` and `https` run the repository's example (flags below); `openssl` runs
benches/server/ossl_hold.c (build: `gcc -O2 -o ossl_hold benches/server/ossl_hold.c -lssl -lcrypto`), with
`release` for SSL_MODE_RELEASE_BUFFERS. The RSS of the server only: the clients live in this process. Kernel socket
memory (a few KiB a connection, the same for every server) is not in RSS and not counted.

Needs Linux and `ulimit -n` of at least 2 N + 64. Timings from this script are not measurements of speed.
"""
import argparse
import json
import os
import re
import shutil
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VECTORS = os.path.join(ROOT, "tests/vectors/tls/echo")
CA = os.path.join(VECTORS, "ca.pem")
HOST = "echo.lex-sys.test"
PHASES = ["start", "connected", "handshake", "established", "echoed", "partial", "all-echoed"]


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def status(pid, key):
    with open(f"/proc/{pid}/status") as f:
        for line in f:
            if line.startswith(key + ":"):
                return int(line.split()[1])  # KiB
    return 0


def mappings(pid):
    """(size KiB, Rss KiB, name) for every mapping of the process with Rss > 0, largest Rss first."""
    out, cur = [], None
    with open(f"/proc/{pid}/smaps") as f:
        for line in f:
            head = line.split()
            if re.match(r"^[0-9a-f]+-[0-9a-f]+$", head[0]):
                cur = [0, 0, head[5] if len(head) > 5 else "[anon]"]
                out.append(cur)
            elif line.startswith("Size:"):
                cur[0] = int(head[1])
            elif line.startswith("Rss:"):
                cur[1] = int(head[1])
    return sorted((m for m in out if m[1] > 0), key=lambda m: -m[1])


class Server:
    def __init__(self, kind, exe, n, extra):
        self.kind = kind
        self.port = free_port()
        first = os.path.join(VECTORS, "first")
        if kind == "openssl":
            cmd = [exe, str(self.port), os.path.join(first, "chain.pem"), os.path.join(first, "key.pem")] + extra
        else:
            work = tempfile.mkdtemp(prefix="tls_memory_")
            for f in ("chain.pem", "key.pem", "names"):
                shutil.copyfile(os.path.join(first, f), os.path.join(work, f))
            cmd = [exe, "--port", str(self.port), "--dir", work, "--connections", str(n), "--handshakes", str(n),
                   "--rate", "1000000", "--handshake-timeout", "3600000", "--idle", "3600000"] + extra
        self.proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        line = self.proc.stdout.readline().decode()
        if not line.startswith("listening"):
            raise SystemExit(f"server did not start: {line!r}")
        # The server logs a line a connection to a pipe of 64 KiB: it blocks when nobody reads.
        threading.Thread(target=self.drain, daemon=True).start()

    def drain(self):
        while self.proc.stdout.read(65536):
            pass

    def rss(self):
        return status(self.proc.pid, "VmRSS")

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


class Client:
    def __init__(self, port, ctx):
        self.sock = socket.create_connection(("127.0.0.1", port))
        self.sock.settimeout(60)
        self.inc, self.out = ssl.MemoryBIO(), ssl.MemoryBIO()
        self.obj = ctx.wrap_bio(self.inc, self.out, server_hostname=HOST)
        self.held = b""

    def handshake_to_finished(self):
        """Send the ClientHello, read the server's flight; keep the client's Finished unsent."""
        try:
            self.obj.do_handshake()
        except ssl.SSLWantReadError:
            pass
        self.sock.sendall(self.out.read())
        while True:
            self.inc.write(self.sock.recv(65536))
            try:
                self.obj.do_handshake()
                break
            except ssl.SSLWantReadError:
                continue
        self.held = self.out.read()

    def finish(self):
        self.sock.sendall(self.held)
        self.held = b""

    def write(self, data):
        self.obj.write(data)
        return self.out.read()

    def http_get(self, path):
        """A keep-alive GET (for `https_hello`): the body's length."""
        self.sock.sendall(self.write(f"GET {path} HTTP/1.1\r\nHost: {HOST}\r\n\r\n".encode()))
        head = b""
        while b"\r\n\r\n" not in head:
            head += self.read_exactly(1)
        want = int(re.search(rb"(?i)content-length: *(\d+)", head).group(1))
        return len(self.read_exactly(want)) if want else 0

    def read_exactly(self, n):
        got = b""
        while len(got) < n:
            try:
                got += self.obj.read(n - len(got))
            except ssl.SSLWantReadError:
                self.inc.write(self.sock.recv(65536))
        return got


def run(kind, exe, n, extra):
    ctx = ssl.create_default_context(cafile=CA)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3
    result = {"n": n, "rss": {}}

    def sample(srv, phase):
        time.sleep(1.0 + n / 5000)
        result["rss"][phase] = srv.rss()
        print(f"  {kind} n={n:<6} {phase:<12} VmRSS {result['rss'][phase]:>9} KiB", flush=True)

    # First server: the connections that have not begun, and the handshakes all waiting for the client's Finished.
    srv = Server(kind, exe, n, extra)
    time.sleep(0.5)
    sample(srv, "start")
    raw = [socket.create_connection(("127.0.0.1", srv.port)) for _ in range(n)]
    sample(srv, "connected")
    for s in raw:
        s.close()
    srv.stop()
    srv = Server(kind, exe, n, extra)
    time.sleep(0.5)
    base = srv.rss()
    conns = []
    for _ in range(n):
        c = Client(srv.port, ctx)
        c.handshake_to_finished()
        conns.append(c)
    sample(srv, "handshake")
    del conns
    srv.stop()

    # Second server: the handshakes done as a program that bounds them does (one at a time here), then idle.
    srv = Server(kind, exe, n, extra)
    time.sleep(0.5)
    conns = []
    for _ in range(n):
        c = Client(srv.port, ctx)
        c.handshake_to_finished()
        c.finish()
        if kind == "https":
            c.http_get("/hello/x")
        else:
            c.sock.sendall(c.write(b"x"))
            c.read_exactly(1)
        conns.append(c)
    sample(srv, "established")
    if kind in ("cancho", "https"):
        result["smaps_established"] = mappings(srv.proc.pid)[:12]
    # One record each, one connection at a time: what the traffic of a server that is not busy leaves resident.
    for c in conns:
        if kind == "https":
            c.http_get("/big/16384")
        else:
            c.sock.sendall(c.write(b"\0" * 16384))
            c.read_exactly(16384)
    sample(srv, "echoed")
    if kind == "https":
        # `https_hello` answers a request, it does not echo: no record can be held half way through it here.
        result["start_after_restart"] = base
        srv.stop()
        return result
    # Then every connection holds 12,000 bytes of a record at once.
    held = []
    for c in conns:
        data = c.write(b"\0" * 16384)
        c.sock.sendall(data[:12000])
        held.append(data[12000:])
    sample(srv, "partial")
    for c, rest in zip(conns, held):
        c.sock.sendall(rest)
    for c in conns:
        c.read_exactly(16384)
    sample(srv, "all-echoed")
    result["start_after_restart"] = base
    srv.stop()
    return result


def table(results):
    lines = ["| N | " + " | ".join(PHASES[1:]) + " |", "|---|" + "---|" * (len(PHASES) - 1)]
    for r in results:
        n, rss = r["n"], r["rss"]
        cells = [f"{(rss[p] - rss['start']) / n:.1f}" if p in rss else "-" for p in PHASES[1:]]
        lines.append(f"| {n} | " + " | ".join(cells) + " |")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("kind", choices=["cancho", "openssl", "https"])
    ap.add_argument("exe")
    ap.add_argument("extra", nargs="*")
    ap.add_argument("--counts", default="50,100,200")
    ap.add_argument("--json")
    a = ap.parse_args()
    results = []
    for n in [int(x) for x in a.counts.split(",")]:
        results.append(run(a.kind, a.exe, n, a.extra))
    print()
    print(f"KiB of server RSS per connection, over RSS at start ({a.kind} {' '.join(a.extra)}):")
    print(table(results))
    if a.json:
        with open(a.json, "w") as f:
            json.dump(results, f, indent=1)


if __name__ == "__main__":
    main()
