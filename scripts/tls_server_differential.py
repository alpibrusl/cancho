#!/usr/bin/env python3
"""`packages/tls`'s server beside `openssl s_server` on the same ClientHellos (docs/tls-server.md §8, step 2).

    python3 scripts/tls_server_differential.py [<case name substring> ...]

`tests/vectors/tls/liar_client.txt` holds the connections of `scripts/tls_liar_client.py`, each the bytes a lying
client sent `packages/tls`'s server and what the server answered. A case whose outcome is decided on the
ClientHello (the first, or the second after a HelloRetryRequest) is one whose client bytes do not depend on what
the server sent: the ClientHellos are fixed, and nothing before the server's ServerHello is encrypted. Those bytes
are sent here, line by line as the recording has them, to `openssl s_server` (TLS 1.3, the same P-256 identity,
X25519, P-256 and P-384, all three suites, ALPN `h2` and `http/1.1`), and what it answers is read until it alerts,
closes, or sends its ServerHello (or, after the bytes of the second ClientHello, its second).

For each case, the two outcomes: refused with alert N (a fatal alert in the clear), refused with no alert (the
connection closed), or accepted (a ServerHello; a HelloRetryRequest is followed by the case's next line). Each
line is the case, both outcomes and `agree`; `alert` when both refuse with different alerts, which RFC 8446 §6.2
often leaves open; `known` when they differ on accept or refuse as `EXPECTED` below says, and why; `DIFFER` when
they differ otherwise, and `STALE` when a difference `EXPECTED` names is gone. Exit status 1 on any `DIFFER` or
`STALE`.
"""
import os
import select
import socket
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_liar_client as liar  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
IDLE = 0.5  # seconds of quiet that end a read of what the server sent
HRR = liar.HRR

# Where the two differ on accept or refuse, each on purpose, with why. A difference not here fails the run, and so
# does one here that stops happening.
EXPECTED = {
    "honest: a host name of 300 bytes, which no identity can have: the default":
        "RFC 6066 §3 lets a server that does not recognise a name go on (with the default identity) or refuse with "
        "unrecognized_name; this server goes on, never telling names apart (docs/tls-server.md §7); OpenSSL refuses "
        "a name over 255 bytes",
    "two X25519 shares":
        "RFC 8446 §4.2.8 lets a server refuse two shares of one group (illegal_parameter); OpenSSL takes the first",
    "after a retry, another session id":
        "RFC 8446 §4.1.2: the second ClientHello is the first with only the share (and cookie, early data, padding) "
        "changed, which a server may check; OpenSSL does not compare the session id",
    "a ClientHello header saying 16 KiB and one byte":
        "this server reassembles a ClientHello of at most 16 KiB (docs/tls-server.md §5.2); OpenSSL's limit is "
        "larger, and it waits for the rest of this one, which never comes",
    "a ClientHello of 16 KiB and one byte, in two records":
        "the same limit; OpenSSL takes this ClientHello",
    "an unknown extension twice":
        "RFC 8446 §4.2: no extension twice in one block; OpenSSL checks only the extensions it knows",
}


def cases():
    out = []
    for line in open(os.path.join(ROOT, "tests/vectors/tls/liar_client.txt")):
        line = line.rstrip("\n")
        if line.startswith("## "):
            tag, _, name = line[3:].partition(" ")
            out.append((tag, name, [], []))
        elif line.startswith("= "):
            out[-1][3].append(line[2:])
        elif not line.startswith("#"):
            out[-1][2].append(line)
    return out


def records(data):
    out = []
    while len(data) >= 5 and len(data) >= 5 + int.from_bytes(data[3:5], "big"):
        n = 5 + int.from_bytes(data[3:5], "big")
        out.append(data[:n])
        data = data[n:]
    return out


def ours(asked, answered):
    """The client's bytes for each `F` line up to the decision, and `packages/tls`'s outcome; None when the case is
    not decided on the ClientHello."""
    feeds, sent = [], b""
    for q, a in zip(asked, answered):
        if not q.startswith("F "):
            continue
        feeds.append(bytes.fromhex(q[2:]) if q[2:] != "-" else b"")
        f = a.split(" ")
        out = bytes.fromhex(f[3]) if f[3] != "-" else b""
        for r in records(out):
            if r[0] == 22 and r[5] == 2 and r[5 + 4 + 2:5 + 4 + 2 + 32] != HRR:
                # A ServerHello: the server took the ClientHello.
                return feeds, "accepted"
            if r[0] == 21 and len(r) == 7:
                return feeds, f"refused, alert {r[6]}"
        if f[2] == "5":
            return feeds, "refused, no alert"
    return None


class SServer:
    def __init__(self, work):
        self.work = work
        self.port = liar_port = None

    def start(self):
        s = socket.socket()
        s.bind(("127.0.0.1", 0))
        self.port = s.getsockname()[1]
        s.close()
        self.proc = subprocess.Popen(
            ["openssl", "s_server", "-accept", str(self.port), "-tls1_3", "-cert", "main.pem", "-key", "main.key",
             "-groups", "X25519:P-256:P-384", "-ciphersuites",
             "TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:TLS_AES_256_GCM_SHA384", "-alpn", "h2,http/1.1",
             "-num_tickets", "0", "-naccept", "1", "-quiet"],
            cwd=self.work, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            try:
                self.sock = socket.create_connection(("127.0.0.1", self.port), 0.2)
                return
            except OSError:
                time.sleep(0.05)
        raise RuntimeError("s_server never listened")

    def exchange(self, data):
        """Sends `data`; what came back until quiet, and whether the connection closed."""
        try:
            self.sock.sendall(data)
        except OSError:
            return b"", True
        got, closed = b"", False
        while True:
            r, _, _ = select.select([self.sock], [], [], IDLE)
            if not r:
                break
            try:
                chunk = self.sock.recv(65536)
            except OSError:
                closed = True
                break
            if not chunk:
                closed = True
                break
            got += chunk
        return got, closed

    def stop(self):
        try:
            self.sock.close()
        except OSError:
            pass
        self.proc.kill()
        self.proc.wait()


def theirs(work, feeds):
    srv = SServer(work)
    srv.start()
    try:
        for data in feeds:
            got, closed = srv.exchange(data)
            for r in records(got):
                if r[0] == 22 and r[5] == 2 and r[5 + 4 + 2:5 + 4 + 2 + 32] != HRR:
                    return "accepted"
                if r[0] == 21 and len(r) == 7:
                    return f"refused, alert {r[6]}"
            if closed:
                return "refused, no alert"
        return "refused, no alert" if not feeds else "no answer"
    finally:
        srv.stop()


def main():
    only = sys.argv[1:]
    work = tempfile.mkdtemp(prefix="tls-server-differential-")
    open(os.path.join(work, "main.pem"), "wb").write(liar.CHAIN)
    open(os.path.join(work, "main.key"), "wb").write(liar.key_pem(liar.MAIN_KEY))
    counts = {}
    bad = 0
    for tag, name, asked, answered in cases():
        if only and not any(o in name for o in only):
            continue
        if name.startswith("tickets:"):
            # Several connections on one engine: `scripts/tls_server_tickets_differential.py`'s.
            continue
        decided = ours(asked, answered)
        if decided is None:
            continue
        feeds, mine = decided
        if tag != "ok" and mine == "accepted":
            # Decided later, on records encrypted under keys this server chose: not a ClientHello's case.
            continue
        other = theirs(work, feeds)
        if mine == other:
            verdict = "agree" if name not in EXPECTED else "STALE"
        elif mine.startswith("refused") and other.startswith("refused"):
            verdict = "alert" if name not in EXPECTED else "known"
        elif name in EXPECTED:
            verdict = "known"
        else:
            verdict = "DIFFER"
        counts[verdict] = counts.get(verdict, 0) + 1
        bad += verdict in ("DIFFER", "STALE")
        why = f"  ({EXPECTED[name]})" if verdict == "known" else ""
        print(f"{verdict:6} {name}: packages/tls {tag} {mine}; openssl {other}{why}")
    print(", ".join(f"{v} {n}" for v, n in sorted(counts.items())))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
