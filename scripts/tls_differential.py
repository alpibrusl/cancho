#!/usr/bin/env python3
"""`packages/tls` beside `openssl s_client` (docs/tls-assurance.md §4).

    python3 scripts/tls_differential.py [<case name substring> ...]
    python3 scripts/tls_differential.py --handshakes <tls_many>

**The same handshakes** (`--handshakes`). Each row is one server, `openssl
s_server` or rustls (scripts/interop/rustls_server, built as
`scripts/tls_interop.py` builds it), made to offer one version and one suite,
or for TLS 1.3 one group, which with P-256 or P-384 makes it answer the
ClientHello's X25519 share with a HelloRetryRequest. `tls_many` (one
connection) and `openssl s_client`, offering what `packages/tls` offers (its
groups, suites in its order and signature schemes), connect through a relay
that reads what the server sends in the clear: the ServerHello's version,
suite and group, a HelloRetryRequest's group, and TLS 1.2's
ServerKeyExchange curve and signature scheme. The two clients must have been
answered the same, and both must have completed. (TLS 1.3's CertificateVerify
scheme is encrypted, so it is not compared.)

**The lying server**, the default:

`scripts/tls_liar.py` holds 66 connections, each a server that changes one
thing, and what `packages/tls` must do with each: accept, or refuse with a
tag and the alert RFC 8446 §6.2 names. Its expectations were written from the
RFC. This runs the same cases, the same server code unchanged, against
OpenSSL's client instead, over a socket: `openssl s_client` with the liar's
CA as its only root (`-verify_return_error`), its host name, and the time the
liar's certificates are valid at (`-attime`). The client's request and its
second write come in on `s_client`'s standard input, where the liar's script
asks the driver to send them.

For each case, OpenSSL's outcome:
- refused, alert N: OpenSSL sent a fatal alert (read in the clear, or opened
  under the client's handshake or application key, whichever the server
  holds);
- accepted: the case's script ran to its end, or the handshake completed
  (the server checked OpenSSL's Finished) and OpenSSL sent no alert. The
  second is for the steps where the script expects what `packages/tls` does
  and OpenSSL, as legally, does otherwise: the step is printed beside it.
  One is the answer to a KeyUpdate that asks for one, which `packages/tls`
  sends at once and OpenSSL with its next write, as RFC 8446 §4.6.3 allows;
- refused, no alert: it stopped before the handshake completed without
  sending one, or it reported an error on standard error: the server's own
  fatal alert ("SSL alert number"), or one of its own (":error:").

`packages/tls`'s outcome is the case's expectation, which
`crates/lex-sys/tests/conformance/tls.rs` checks against the client on every
test run (`tests/vectors/tls/liar.txt`). Each line is the case, both outcomes
and `agree`; `alert` when they differ in the alert alone, since RFC 8446 lets an
implementation choose between some (§6.2: "SHOULD"); `known` when they differ
on accept or refuse as `EXPECTED` below says, and why; `DIFFER` when they
differ otherwise, and `STALE` when a difference `EXPECTED` names is gone.
Exit status 1 on any `DIFFER` or `STALE`.
"""
import os
import select
import socket
import subprocess
import sys
import tempfile
import threading

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_interop  # noqa: E402
import tls_liar  # noqa: E402

IDLE = 0.4  # seconds of quiet that end a read of what the client sent
CCS = bytes([20, 3, 3, 0, 1, 1])
# What `packages/tls` offers (`packages/tls/message.ls`, `client_hello`), so
# the two clients are asked the same question: its groups, its TLS 1.3 and
# TLS 1.2 suites in its order, and its signature schemes.
OFFER = [
    "-groups", "X25519:P-256:P-384",
    "-ciphersuites", "TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:TLS_AES_128_GCM_SHA256",
    "-cipher", "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:"
               "ECDHE-RSA-CHACHA20-POLY1305:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256",
    "-sigalgs", "ECDSA+SHA256:ECDSA+SHA384:rsa_pss_rsae_sha256:rsa_pss_rsae_sha384:rsa_pss_rsae_sha512:ed25519:"
                "RSA+SHA256:RSA+SHA384:RSA+SHA512",
]


# The cases where the two clients differ on accept or refuse, each on
# purpose, with why (docs/tls-assurance.md §4.1). A difference not here
# fails the run, and so does one here that stops happening.
EXPECTED = {
    "a HelloRetryRequest with a cookie over 2,048 bytes":
        "the slot keeps a cookie of at most 2,048 bytes (docs/tls-parity.md §3.3); OpenSSL takes this one, of 2,049",
    "a Certificate over 64 KiB":
        "the slot's handshake buffer is 64 KiB (docs/tls-pure.md §7.4); OpenSSL's default limit is 100 KiB "
        "(SSL_MAX_CERT_LIST_DEFAULT)",
    "TLS 1.2, the 1.1 downgrade sentinel":
        "RFC 8446 §4.1.3: a client that offered TLS 1.3 MUST refuse either sentinel in a TLS 1.2 ServerHello; "
        "OpenSSL refuses DOWNGRD\\x01 there and takes DOWNGRD\\x00",
    "TLS 1.2, no extended master secret":
        "required (#207, RFC 7627); OpenSSL 3.0 does not require it",
    "TLS 1.2, a ServerHello echoing the client's session id":
        "RFC 5246 §7.4.1.3: an echoed session id resumes that session, and the client offered none (its id is "
        "random, for middlebox compatibility); OpenSSL takes it as a new session",
    "TLS 1.2, a HelloRequest":
        "no renegotiation (#207); OpenSSL renegotiates",
}


class SClient:
    """A `tls_liar.Conversation` whose client is `openssl s_client`."""

    def __init__(self):
        self.lines = []
        self.answers = []
        self.sent = b""
        self.received = b""
        self.buffer = b""
        self.closed = False
        self.ccs_given = False
        self.tls13 = True
        self.errors = b""
        self.dir = tempfile.TemporaryDirectory()
        self.ca = os.path.join(self.dir.name, "ca.pem")
        open(self.ca, "wb").write(tls_liar.ROOTS)
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(1)
        self.proc = None
        self.sock = None

    def _start(self):
        port = self.listener.getsockname()[1]
        host = tls_liar.HOST.decode()
        self.proc = subprocess.Popen(
            ["openssl", "s_client", "-connect", f"127.0.0.1:{port}", "-servername", host, "-CAfile", self.ca,
             "-verify_return_error", "-verify_hostname", host, "-attime", str(tls_liar.NOW), "-quiet", *OFFER],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        threading.Thread(target=self._stdout, daemon=True).start()
        threading.Thread(target=self._stderr, daemon=True).start()
        self.listener.settimeout(10)
        self.sock, _ = self.listener.accept()

    def _stdout(self):
        while True:
            chunk = self.proc.stdout.read1(65536)
            if not chunk:
                return
            self.received += chunk

    def _stderr(self):
        for line in self.proc.stderr:
            self.errors += line

    def _drain(self):
        """What the client sends until it is quiet for IDLE seconds."""
        while not self.closed:
            ready, _, _ = select.select([self.sock], [], [], IDLE)
            if not ready:
                return
            try:
                chunk = self.sock.recv(65536)
            except OSError:
                chunk = b""
            if not chunk:
                self.closed = True
                return
            self.sent += chunk

    def _answer(self, code):
        if self.closed and self.proc.poll() is None:
            try:
                self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
        event = "3"
        if self.closed:
            event = "4"
        f = [str(code), "ok", event, "-", "-"]
        self.answers.append(" ".join(f))
        return f

    def ask(self, line):
        op = line[0]
        if op == "C":
            self._start()
            self._drain()
            return self._answer(0)
        if op == "W":
            data = bytes.fromhex(line.split()[1])
            try:
                self.proc.stdin.write(data)
                self.proc.stdin.flush()
            except OSError:
                pass
            self._drain()
            return self._answer(len(data))
        raise ValueError(line)

    def feed(self, data):
        try:
            self.sock.sendall(data)
        except OSError:
            self.closed = True
        self._drain()
        return self._answer(len(data))

    def take(self):
        """What the client sent since the last call, as records, with its one
        change_cipher_spec moved to where `packages/tls` sends it: just before
        its first protected record. RFC 8446 Appendix D.4 lets a client send
        it there or straight after its first ClientHello's answer, and OpenSSL
        sends it before a second ClientHello; the liar's script expects
        `packages/tls`'s place."""
        out, self.sent = self.sent, b""
        recs = tls_liar.records(out)
        if not self.tls13:
            return recs
        recs = [r for r in recs if r != CCS]
        if recs and recs[0][:1] == b"\x17" and not self.ccs_given:
            self.ccs_given = True
            recs.insert(0, CCS)
        return recs

    def close(self):
        for f in (self.proc.stdin, self.sock):
            try:
                f.close()
            except OSError:
                pass
        self.proc.kill()
        self.proc.wait()
        self.listener.close()
        self.dir.cleanup()


def alert_in(server, recs):
    """The fatal alert among `recs`, if one: in the clear, or under a key the
    server holds for reading what the client writes."""
    for rec in recs:
        if rec[:1] == b"\x15" and len(rec) == 7:
            if rec[5] == 2:
                return rec[6]
        # TLS 1.3 protects an alert as application_data; TLS 1.2 keeps its
        # type, 21, and a longer body.
        if rec[:1] == b"\x17" or (rec[:1] == b"\x15" and len(rec) > 7):
            for keys in [getattr(server, "read", None)]:
                if keys is None:
                    continue
                try:
                    kind, content = keys.open(rec)
                except Exception:  # noqa: BLE001 -- not under this key
                    continue
                if kind == 21 and len(content) == 2 and content[0] == 2:
                    return content[1]
    return None


def completed(cls):
    """`cls.check_client_finished`, noting on the server that it passed."""
    check = cls.check_client_finished

    def wrapped(self, *a, **k):
        out = check(self, *a, **k)
        self.handshake_done = True
        return out
    return wrapped


for _cls in (tls_liar.Server, tls_liar.Server12):
    if "check_client_finished" in _cls.__dict__:
        _cls.check_client_finished = completed(_cls)


def openssl_outcome(name, fn, server_class):
    """(outcome, alert, the step where OpenSSL left the script, if it did)."""
    conv = SClient()
    s = (server_class or tls_liar.Server)(conv)
    conv.tls13 = not isinstance(s, tls_liar.Server12)
    s.handshake_done = False
    finished = False
    step = None
    try:
        fn(s)
        finished = True
    except Exception as e:  # noqa: BLE001 -- where OpenSSL stopped is the answer
        step = f"{type(e).__name__}: {e}"
    conv._drain()
    alert = alert_in(s, tls_liar.records(conv.sent))
    conv.close()
    if alert is not None:
        return "refused", alert, step
    if b"SSL alert number" in conv.errors or b":error:" in conv.errors:
        # The server's fatal alert ended the connection (OpenSSL fails it
        # and, as RFC 8446 §6.2 says, sends none back), or OpenSSL failed
        # it with an error of its own and the alert was not one the server
        # could read.
        return "refused", None, step
    if finished or s.handshake_done:
        return "accepted", None, step
    return "refused", None, step


class Relay:
    """A TCP relay to `port` that records, per connection, what the server
    sent in the clear before its first protected record."""

    def __init__(self, port):
        self.target = port
        self.seen = []
        self.sock = socket.socket()
        self.sock.bind(("127.0.0.1", 0))
        self.sock.listen(16)
        self.port = self.sock.getsockname()[1]
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while True:
            try:
                client, _ = self.sock.accept()
            except OSError:
                return
            server = socket.create_connection(("127.0.0.1", self.target))
            info = {}
            self.seen.append(info)
            threading.Thread(target=self.pump, args=(client, server, None), daemon=True).start()
            threading.Thread(target=self.pump, args=(server, client, info), daemon=True).start()

    def pump(self, src, dst, info):
        buf = b""
        handshake = b""
        while True:
            try:
                data = src.recv(65536)
            except OSError:
                data = b""
            if not data:
                for s in (src, dst):
                    try:
                        s.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass
                return
            try:
                dst.sendall(data)
            except OSError:
                return
            if info is None or info.get("done"):
                continue
            buf += data
            while len(buf) >= 5 and len(buf) >= 5 + int.from_bytes(buf[3:5], "big"):
                n = int.from_bytes(buf[3:5], "big")
                kind, body, buf = buf[0], buf[5:5 + n], buf[5 + n:]
                if kind != 22:
                    # Protection starts at TLS 1.3's first application_data
                    # record, or after TLS 1.2's change_cipher_spec; TLS
                    # 1.3's own (after a HelloRetryRequest, say) is passed by.
                    if kind == 23 or (kind == 20 and info.get("version") == "0303"):
                        info["done"] = True
                    continue
                handshake += body
                while len(handshake) >= 4 and len(handshake) >= 4 + int.from_bytes(handshake[1:4], "big"):
                    m = int.from_bytes(handshake[1:4], "big")
                    msg, handshake = handshake[:4 + m], handshake[4 + m:]
                    read_message(msg, info)


def read_message(msg, info):
    """A ServerHello's or ServerKeyExchange's fields into `info`."""
    kind, b = msg[0], msg[4:]
    if kind == 2:
        retry = b[2:34] == tls_liar.HRR
        at = 34
        at += 1 + b[at]
        suite = b[at:at + 2].hex()
        at += 3
        end = at + 2 + int.from_bytes(b[at:at + 2], "big")
        at += 2
        version, group = "0303", None
        while at < end:
            t, n = int.from_bytes(b[at:at + 2], "big"), int.from_bytes(b[at + 2:at + 4], "big")
            body = b[at + 4:at + 4 + n]
            if t == 43:
                version = body.hex()
            if t == 51:
                group = body[:2].hex()
            at += 4 + n
        if retry:
            info["retry"] = group
        else:
            info.update(version=version, suite=suite, group=group)
    elif kind == 12:
        point = b[3]
        info.update(group=b[1:3].hex(), scheme=b[4 + point:6 + point].hex())


def handshake_rows():
    """(server, certificate, version, suite, group) for each row."""
    rows = []
    for server in ("openssl", "rustls"):
        for suite in tls_interop.SUITES13:
            rows.append((server, "p256", "1.3", suite, None))
        if server == "openssl":
            for group in ("P-256", "P-384"):
                rows.append((server, "p256", "1.3", None, group))
            rows.append((server, "rsa2048", "1.3", None, None))
        rows += [(server, "p256", "1.2", s, None) for s in tls_interop.ECDSA12]
        rows += [(server, "rsa2048", "1.2", s, None) for s in tls_interop.RSA12]
    return rows


def start_server(server, rustls, cert, key, version, suite, group):
    work = tempfile.mkdtemp()
    open(f"{work}/c.pem", "wb").write(cert)
    open(f"{work}/k.pem", "wb").write(key)
    port = tls_interop.tls_live.free_port()
    if server == "openssl":
        if version == "1.3":
            args = ["-tls1_3", "-ciphersuites", suite or "TLS_AES_256_GCM_SHA384", "-groups", group or "X25519"]
        else:
            args = ["-tls1_2", "-cipher", suite]
        argv = ["openssl", "s_server", "-accept", f"127.0.0.1:{port}", "-cert", "c.pem", "-key", "k.pem", *args,
                "-HTTP", "-quiet"]
    else:
        argv, _ = tls_interop.argv_for("rustls", rustls, port, version, suite)
    return tls_interop.Server(argv, work, port)


def handshakes(tls_many):
    built = tls_interop.build(tempfile.mkdtemp())
    rustls = built["rustls"]
    keys = {k: tls_interop.tls_live.certificate(k) for k in ("p256", "rsa2048")}
    host = tls_interop.tls_live.HOST
    differ = agree = 0
    for server, cert, version, suite, group in handshake_rows():
        cert_pem, key_pem, ca = keys[cert]
        srv = start_server(server, rustls, cert_pem, key_pem, version, suite, group)
        relay = Relay(srv.port)
        code, lines = tls_interop.tls_live.run(tls_many, relay, ca, 1, 65536)
        ours_ok = bool(lines) and lines[-1] == "done ok=1 failed=0"
        with tempfile.NamedTemporaryFile(suffix=".pem") as f:
            f.write(ca)
            f.flush()
            r = subprocess.run(["openssl", "s_client", "-connect", f"127.0.0.1:{relay.port}", "-servername", host,
                                "-CAfile", f.name, "-verify_return_error", "-verify_hostname", host, "-brief", *OFFER],
                               input=b"GET / HTTP/1.0\r\n\r\n", capture_output=True, timeout=30)
        theirs_ok = b"CONNECTION ESTABLISHED" in r.stderr and b"Verification: OK" in r.stderr
        srv.stop()
        relay.sock.close()
        ours, theirs = (relay.seen + [{}, {}])[:2]
        for d in (ours, theirs):
            d.pop("done", None)
        same = ours == theirs and ours_ok and theirs_ok
        agree += same
        differ += not same
        label = f"{server:8} TLS {version} {cert:8} {suite or '*'} {group or ''}".rstrip()
        verdict = "agree " if same else "DIFFER"
        print(f"{verdict} | {label} | packages/tls {'ok' if ours_ok else lines[-1:]}: {ours} | openssl "
              f"{'ok' if theirs_ok else 'failed'}: {theirs}", flush=True)
    print(f"handshakes: {agree} agree, {differ} differ")
    return differ


def main():
    if sys.argv[1:2] == ["--handshakes"]:
        sys.exit(1 if handshakes(os.path.abspath(sys.argv[2])) else 0)
    wanted = sys.argv[1:]
    differ = alerts = agree = known = 0
    for name, tag, alert, encrypted, fn, server_class in tls_liar.CASES:
        if wanted and not any(w in name for w in wanted):
            continue
        ours = ("accepted", None) if tag == "ok" else ("refused", alert)
        *theirs, step = openssl_outcome(name, fn, server_class)
        if ours[0] != theirs[0] and name in EXPECTED:
            verdict = "known"
            known += 1
        elif ours[0] != theirs[0]:
            verdict = "DIFFER"
            differ += 1
        elif name in EXPECTED:
            verdict = "STALE"
            differ += 1
        elif ours[1] != theirs[1]:
            verdict = "alert"
            alerts += 1
        else:
            verdict = "agree"
            agree += 1
        show = lambda o: o[0] + ("" if o[1] is None else f" {o[1]}")  # noqa: E731
        note = f" | stopped at: {step}" if step else ""
        if verdict == "known":
            note += f" | {EXPECTED[name]}"
        print(f"{verdict:6} | {name} | packages/tls: {show(ours)} ({tag}) | openssl: {show(theirs)}{note}", flush=True)
    print(f"differential: {agree} agree, {alerts} differ in the alert only, {known} differ as documented, "
          f"{differ} differ otherwise")
    sys.exit(1 if differ else 0)


if __name__ == "__main__":
    main()
