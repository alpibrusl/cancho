#!/usr/bin/env python3
"""`packages/tls`'s server beside `openssl s_server` on the same resumptions (docs/tls-server.md §12.8).

    python3 scripts/tls_server_tickets_differential.py <tls_serve> [<case name substring> ...]

Both servers run TLS 1.3 with the same P-256 identity and tickets on. The client is `scripts/tls_liar_client.py`'s,
over a socket: a full handshake with each server, which sends it tickets, and then the case's connection, a
resumption changed in the one way the case is about, to the same server. Each outcome is `resumed` (the ServerHello
selected the PSK), `full` (it did not, and the handshake went on) or `refused, alert N`. The two servers' tickets
are each their own: a ticket is opaque, and what is compared is what each server does with its own, which is what
a client sees.

A line is the case, both outcomes and `agree`; `alert` when both refuse with different alerts (RFC 8446 §6.2 often
leaves it open); `known` when they differ as `EXPECTED` says, and why; `DIFFER` when they differ otherwise;
`STALE` when a difference `EXPECTED` names has gone. Exit status 1 on any `DIFFER` or `STALE`.
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
IDLE = 0.4
NOW = int(time.time() * 1000)

# Where the two differ, each on purpose.
EXPECTED = {
    "the age claimed 60 s too high":
        "RFC 8446 §4.2.11.1 uses the age to bound replays of early data; a server that never accepts early data "
        "need not check it. OpenSSL does not; this server does, within 30 s (docs/tls-server.md §12.3 c)",
    "a ticket offered for another host name":
        "OpenSSL does not compare the name a ticket was made for with the one the new ClientHello sends; this "
        "server does (docs/tls-server.md §12.3 d)",
}


class Sock:
    """What `Client` needs of a `Conversation`, over a socket."""

    def __init__(self, port):
        self.sock = socket.create_connection(("127.0.0.1", port), 3)
        self.sent = b""
        self.received = b""
        self.closed = False

    def feed(self, data):
        try:
            self.sock.sendall(data)
        except OSError:
            self.closed = True
        self.read()
        return ["0", "ok", "3", "-", "-"]

    def read(self):
        while True:
            r, _, _ = select.select([self.sock], [], [], IDLE)
            if not r:
                return
            try:
                chunk = self.sock.recv(65536)
            except OSError:
                self.closed = True
                return
            if not chunk:
                self.closed = True
                return
            self.sent += chunk

    def take(self):
        self.read()
        out, self.sent = self.sent, b""
        return liar.records(out)

    def close(self):
        self.sock.close()


def start_openssl(work):
    port = free_port()
    proc = subprocess.Popen(
        ["openssl", "s_server", "-accept", str(port), "-tls1_3", "-cert", "main.pem", "-key", "main.key",
         "-cert_chain", "ca.pem", "-groups", "X25519:P-256:P-384", "-ciphersuites",
         "TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:TLS_AES_256_GCM_SHA384", "-num_tickets", "2", "-quiet"],
        cwd=work, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wait_listening(port)
    return proc, port


def start_ours(exe, work):
    port = free_port()
    proc = subprocess.Popen([exe, str(port), "echo", "-", "0", "-", "2:3600", "main.pem", "main.key", liar.HOST.decode()],
                            cwd=work, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    assert proc.stdout.readline().strip() == "listening"
    return proc, port


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def wait_listening(port):
    for _ in range(100):
        try:
            socket.create_connection(("127.0.0.1", port), 0.2).close()
            return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("never listened")


def client(port, **attrs):
    c = liar.Client(Sock(port))
    c.sni_ack = attrs.pop("sni_ack", None)
    for k, v in attrs.items():
        setattr(c, k, v)
    return c


def outcome(c):
    """`resumed`, `full` or `refused, alert N`, after the ClientHello has been sent."""
    recs = c.c.take()
    if not recs:
        return "refused, no alert"
    first = recs[0]
    if first[0] == 21:
        return f"refused, alert {first[6]}"
    c.c.sent = b"".join(recs)
    sh = first[5:]
    c.parse_server_hello(sh)
    return "resumed" if c.sh_psk is not None else "full"


def full(port, sni_ack, **attrs):
    """A full handshake that gets tickets: the tickets."""
    c = client(port, **attrs)
    c.expect_sni_ack = sni_ack
    c.send_hello()
    c.flight()
    c.finish()
    return c.tickets


def resumption(port, t, sni_ack, now_offset=5.0, **over):
    c = client(port, expect_sni_ack=sni_ack)
    age = int(now_offset * 1000)
    ticket = over.pop("ticket", t["ticket"])
    c.psks.append((ticket, t["psk"], t["hash"], (over.pop("age_ms", age) + t["age_add"]) % 2**32))
    for k, v in over.items():
        setattr(c, k, v)
    c.send_hello()
    return c


CASES = []


def case(name):
    def register(fn):
        CASES.append((name, fn))
        return fn
    return register


@case("an honest resumption")
def honest(port, t, ack):
    c = resumption(port, t, ack)
    return outcome(c)


@case("a ticket used a second time")
def again(port, t, ack):
    outcome(resumption(port, t, ack))
    return outcome(resumption(port, t, ack))


@case("a wrong binder")
def wrong_binder(port, t, ack):
    return outcome(resumption(port, t, ack, binder_flip=0))


@case("only psk_ke offered")
def psk_ke(port, t, ack):
    return outcome(resumption(port, t, ack, modes=b"\1\0"))


@case("pre_shared_key without psk_key_exchange_modes")
def no_modes(port, t, ack):
    return outcome(resumption(port, t, ack, modes=None))


@case("a ticket with one bit changed")
def tampered(port, t, ack):
    tk = t["ticket"]
    return outcome(resumption(port, t, ack, ticket=tk[:40] + bytes([tk[40] ^ 1]) + tk[41:]))


@case("a ticket truncated")
def truncated(port, t, ack):
    return outcome(resumption(port, t, ack, ticket=t["ticket"][:-5]))


@case("a ticket of 15,000 bytes")
def large(port, t, ack):
    return outcome(resumption(port, t, ack, ticket=bytes(15000)))


@case("a ticket offered for another host name")
def other_name(port, t, ack):
    return outcome(resumption(port, t, ack, host=b"other.example"))


@case("the age claimed 60 s too high")
def age(port, t, ack):
    return outcome(resumption(port, t, ack, age_ms=5000 + 60000))


@case("early data offered with a good ticket")
def early(port, t, ack):
    c = resumption(port, t, ack, early=True)
    out = outcome(c)
    return out


@case("the identity is the second of two, the first a made-up one")
def second(port, t, ack):
    c = client(port, expect_sni_ack=ack)
    c.psks.append((bytes(100), None, 32, 0))
    c.psks.append((t["ticket"], t["psk"], t["hash"], (5000 + t["age_add"]) % 2**32))
    c.send_hello()
    return outcome(c)


def main():
    args = sys.argv[1:]
    exe = os.path.abspath(args[0])
    only = args[1:]
    work = tempfile.mkdtemp(prefix="tls-server-tickets-differential-")
    open(os.path.join(work, "main.pem"), "wb").write(liar.pem(liar.MAIN))
    open(os.path.join(work, "ca.pem"), "wb").write(liar.pem(liar.CA))
    open(os.path.join(work, "main.key"), "wb").write(liar.key_pem(liar.MAIN_KEY))
    # `tls_serve` reads a chain: the leaf and the CA.
    open(os.path.join(work, "chain.pem"), "wb").write(liar.CHAIN)
    os.replace(os.path.join(work, "chain.pem"), os.path.join(work, "main.pem"))
    ours_proc, ours_port = start_ours(exe, work)
    ssl_proc, ssl_port = start_openssl(work)
    counts, bad = {}, 0
    try:
        ours_t = full(ours_port, True, host=liar.HOST)[0]
        # OpenSSL's s_server sends no server_name acknowledgement without a callback.
        ssl_t = full(ssl_port, False, host=liar.HOST)[0]
        for name, fn in CASES:
            if only and not any(o in name for o in only):
                continue
            results = []
            for port, t, ack in ((ours_port, ours_t, True), (ssl_port, ssl_t, False)):
                try:
                    results.append(fn(port, t, ack))
                except Exception as e:  # noqa: BLE001 -- reported
                    results.append(f"error {type(e).__name__}: {e}")
            mine, other = results
            if mine == other:
                verdict = "agree" if name not in EXPECTED else "STALE"
            elif mine.startswith("refused") and other.startswith("refused"):
                verdict = "alert"
            elif name in EXPECTED:
                verdict = "known"
            else:
                verdict = "DIFFER"
            counts[verdict] = counts.get(verdict, 0) + 1
            bad += verdict in ("DIFFER", "STALE")
            why = f"  ({EXPECTED[name]})" if verdict == "known" else ""
            print(f"{verdict:6} {name}: packages/tls {mine}; openssl {other}{why}")
    finally:
        ours_proc.kill()
        ssl_proc.kill()
    print(", ".join(f"{v} {n}" for v, n in sorted(counts.items())))
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
