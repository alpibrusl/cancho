#!/usr/bin/env python3
"""The client's CPU for the extra signature of a client certificate (docs/tls-parity.md §6.11), measured.

    python3 scripts/tls_client_auth_cost.py <fetch> [<handshakes>] [<rounds>]

`fetch` is `examples/http_fetch_nb/fetch.cho` built with `packages/tls` (the LLVM backend, the default). Against
`openssl s_server -www` on loopback, with a P-256 certificate and a P-256 client certificate under one CA, it makes
`handshakes` (default 300) full handshakes in a row (`--close`: a connection for each request, no tickets), and the
client's own CPU time (user and system, from `wait4`'s resource usage of that process alone) is divided by the
handshakes. Per version:

    no request     the server asks for nothing
    declined       the server asks, and the client has no identity: the empty Certificate
    answered       the server asks, and the client signs: its chain and a CertificateVerify

and the cost of the signature is `answered` less `declined` (the request and the empty Certificate are in both). The best
of `rounds` (default 3) runs of each, rows interleaved so a slow minute hits all. Prints a table.
"""
import os
import resource
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_auth_interop as I  # noqa: E402


def cpu_of(argv, stdin):
    p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    p.stdin.write(stdin)
    p.stdin.close()
    _, status, usage = os.wait4(p.pid, 0)
    p.returncode = 0
    return usage.ru_utime + usage.ru_stime, os.waitstatus_to_exitcode(status)


def measure(fetch, m, work, version, mode, client, handshakes):
    server = I.launch("openssl", m, work, mode, version, "")
    try:
        argv = [fetch, "--resolve", f"{I.HOST}=127.0.0.1", "--timeout", "120", "--quiet", "--close", "--slots", "1",
                "--repeat", str(handshakes)]
        if client:
            argv += ["--client-cert", m.path("good.pem"), "--client-key", m.path("good.key")]
        argv += [f"https://{I.HOST}:{server.port}/"]
        cpu, code = cpu_of(argv, open(m.path("ca.pem"), "rb").read())
        assert code == 0, f"fetch ended with {code}"
        return cpu / handshakes * 1000
    finally:
        server.stop()


def main():
    fetch = os.path.abspath(sys.argv[1])
    handshakes = int(sys.argv[2]) if len(sys.argv) > 2 else 300
    rounds = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    work = tempfile.mkdtemp(prefix="tls-client-auth-cost-")
    m = I.Material(work)
    rows = [(v, name, mode, client) for v in ("1.3", "1.2")
            for name, mode, client in (("no request", "none", False), ("declined", "optional", False),
                                       ("answered", "optional", True))]
    best = {}
    for _ in range(rounds):
        for v, name, mode, client in rows:
            ms = measure(fetch, m, work, v, mode, client, handshakes)
            best[(v, name)] = min(best.get((v, name), ms), ms)
    print(f"client CPU (user + system) per full handshake, ms; best of {rounds} runs of {handshakes}")
    print(f"{'':6} {'no request':>11} {'declined':>10} {'answered':>10} {'the signature':>14}")
    for v in ("1.3", "1.2"):
        a, b, c = best[(v, "no request")], best[(v, "declined")], best[(v, "answered")]
        print(f"TLS {v} {a:>11.3f} {b:>10.3f} {c:>10.3f} {c - b:>14.3f}")


if __name__ == "__main__":
    main()
