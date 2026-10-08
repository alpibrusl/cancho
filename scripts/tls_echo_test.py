#!/usr/bin/env python3
"""`examples/tls_echo` against real TLS clients (docs/tls-server.md §8, step 3; §11 has the results).

    python3 scripts/tls_echo_test.py <tls_echo> [--cost <seconds>] [<case> ...]

`tls_echo` is `examples/tls_echo/` built with `packages/tls` and `packages/x509`. The certificates are the committed
test identities in `tests/vectors/tls/echo/` (a CA, and two leaves for `echo.lex-sys.test`: `first` and `renewed`,
both valid to 2125). Needs python3 (its `ssl` module) and `openssl` (`s_client`, `s_time`). Each case starts its own
server on a free port with the options it needs, and says `ok` or what failed:

    suites     `openssl s_client`: 3 suites x 3 groups, and one HelloRetryRequest; the echo comes back and the
               server's line says the suite, group, SNI and ALPN the client asked for
    many       200 connections at once (Python `ssl`, one thread each), 20 echoes of 1 to 20,000 bytes each,
               every byte checked
    reload     a connection open before SIGHUP keeps echoing on the certificate it got; a connection after it
               gets the renewed one; a refused reload (the key not the leaf's) leaves the renewed one serving
    bound      `--handshakes 2`: two peers that connect and send nothing hold both places, and an honest client
               waits behind them (not refused) until their `--handshake-timeout` frees one
    rate       `--rate 5`: 15 clients at once are started 5 a second, all complete
    full       `--connections 4`: a fifth connection is closed at once, before a handshake
    idle       `--idle 1000`: an established connection with no traffic gets close_notify
    shutdown   SIGTERM with 10 connections open: each gets close_notify, the server exits 0
    peer       every line about a connection names its peer (`peer=127.0.0.1:<the client's source port>`), the
               established, closed and refused lines alike (docs/conn-peer.md)
    tickets    `--tickets 2`: a client that kept its session resumes (Python `ssl`, `session_reused`), three times
               from the one session, and the server's line says `resumed=yes`; without the flag nothing resumes
    ticket-keys  `--ticket-keys <file>` shared by two processes: a session from one resumes at the other and not
               at a third with another file; a rotation by SIGHUP (a new key on top) keeps the old session for
               one lifetime, a second rotation that drops its key refuses it; a bad file is refused and the keys
               stay
    ticket-identity  a certificate renewed and loaded by SIGHUP refuses the tickets of the old (they were made
               for the other certificate), and a reload of the same certificate keeps them
    ticket-lifetime  `--ticket-lifetime 2`: a session offered after 3 seconds does not resume
    per-address  `--per-address 3`: a fourth connection from one address is closed at once (`per-address`), one
               from another address (127.0.0.2: Linux has all of 127.0.0.0/8, macOS only 127.0.0.1) is not
               affected, and a place freed by a close is usable again
    addr-rate  `--per-address-rate 2`: six clients from one address are started two a second, while a client from
               another address (Linux only, as above) is not delayed by them

curl and mosquitto's clients are not here: they speak HTTP and MQTT, which an echo answers with their own request.
Their interop with this server is docs/tls-server.md §10.2's matrix, against `tests/programs/tls_serve.cho`.

With `--cost <seconds>`: `openssl s_time -new` against the server for that long, with 1 and with 4 clients at once
and the bounds lifted, then 4 clients against the default bounds, and the server's CPU (Linux `/proc/<pid>/stat`;
elsewhere `ps`) divided by the handshakes: connections a second, and milliseconds of CPU a handshake. One line a case, then a count; exit status 1 if any failed.
"""
import os
import re
import select
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import random
import shutil

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
VECTORS = os.path.join(ROOT, "tests/vectors/tls/echo")
CA = os.path.join(VECTORS, "ca.pem")
HOST = "echo.lex-sys.test"


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


class Server:
    """`tls_echo` on a free port, its lines collected as they come."""

    def __init__(self, exe, identity="first", extra=(), files=None):
        self.work = tempfile.mkdtemp(prefix="tls_echo_")
        self.install(identity)
        for name, data in (files or {}).items():
            open(os.path.join(self.work, name), "wb").write(data)
        self.port = free_port()
        self.lines = []
        self.cond = threading.Condition()
        self.proc = subprocess.Popen([exe, "--port", str(self.port), "--dir", self.work, *extra],
                                     stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        threading.Thread(target=self.read, daemon=True).start()
        self.wait_for(lambda l: l.startswith("listening"), 10)

    def install(self, identity, files=("chain.pem", "key.pem", "names")):
        for f in files:
            shutil.copyfile(os.path.join(VECTORS, identity, f), os.path.join(self.work, f))

    def read(self):
        for raw in self.proc.stdout:
            with self.cond:
                self.lines.append((time.monotonic(), raw.decode(errors="replace").rstrip("\n")))
                self.cond.notify_all()

    def wait_for(self, pred, timeout, count=1):
        end = time.monotonic() + timeout
        with self.cond:
            while sum(1 for _, l in self.lines if pred(l)) < count:
                left = end - time.monotonic()
                if left <= 0 or self.proc.poll() is not None and not self.proc.stdout.readable():
                    raise AssertionError(f"timed out waiting; lines: {self.text()[-8:]}")
                self.cond.wait(left)
            return [l for _, l in self.lines if pred(l)]

    def text(self):
        return [l for _, l in self.lines]

    def signal(self, sig):
        self.proc.send_signal(sig)

    def stop(self):
        if self.proc.poll() is None:
            self.proc.send_signal(signal.SIGTERM)
            try:
                self.proc.wait(5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        shutil.rmtree(self.work, ignore_errors=True)


def context(alpn=None):
    ctx = ssl.create_default_context(cafile=CA)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3
    if alpn:
        ctx.set_alpn_protocols(alpn)
    return ctx


def connect(port, alpn=None, timeout=10):
    raw = socket.create_connection(("127.0.0.1", port), timeout=timeout)
    return context(alpn).wrap_socket(raw, server_hostname=HOST)


def echo(conn, data):
    conn.sendall(data)
    got = b""
    while len(got) < len(data):
        part = conn.recv(65536)
        if not part:
            raise AssertionError(f"the connection ended after {len(got)} of {len(data)} bytes")
        got += part
    if got != data:
        raise AssertionError("the echo differs from what was sent")


def serial(conn):
    der = conn.getpeercert(binary_form=True)
    out = subprocess.run(["openssl", "x509", "-inform", "DER", "-noout", "-serial"], input=der, capture_output=True)
    return out.stdout.decode().strip()


def file_serial(identity):
    out = subprocess.run(["openssl", "x509", "-in", os.path.join(VECTORS, identity, "chain.pem"), "-noout", "-serial"],
                         capture_output=True)
    return out.stdout.decode().strip()


def field(line, name):
    m = re.search(rf"\b{name}=(\S+)", line)
    return m.group(1) if m else None


# ---- the cases ----

def case_suites(exe):
    server = Server(exe, extra=["--alpn", "echo,other"])
    try:
        rows = []
        for suite, sname in [("TLS_AES_128_GCM_SHA256", "TLS_AES_128_GCM_SHA256"),
                             ("TLS_AES_256_GCM_SHA384", "TLS_AES_256_GCM_SHA384"),
                             ("TLS_CHACHA20_POLY1305_SHA256", "TLS_CHACHA20_POLY1305_SHA256")]:
            for groups, gname, hrr in [("X25519", "x25519", "no"), ("P-256", "secp256r1", "no"),
                                       ("P-384", "secp384r1", "no")]:
                rows.append((suite, groups, sname, gname, hrr))
        rows.append(("TLS_AES_128_GCM_SHA256", "P-521:P-256", "TLS_AES_128_GCM_SHA256", "secp256r1", "yes"))
        for n, (suite, groups, sname, gname, hrr) in enumerate(rows):
            message = f"row {n} {suite} {groups}\n".encode()
            p = subprocess.Popen(["openssl", "s_client", "-connect", f"127.0.0.1:{server.port}", "-servername", HOST,
                                  "-CAfile", CA, "-verify_return_error", "-tls1_3", "-ciphersuites", suite,
                                  "-groups", groups, "-alpn", "echo", "-quiet"],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            p.stdin.write(message)
            p.stdin.flush()
            got = b""
            end = time.monotonic() + 10
            while len(got) < len(message) and time.monotonic() < end:
                r, _, _ = select.select([p.stdout], [], [], 0.5)
                if r:
                    part = os.read(p.stdout.fileno(), 4096)
                    if not part:
                        break
                    got += part
            p.kill()
            p.wait()
            if got != message:
                return f"row {n} ({suite} {groups}): echo {got!r}"
            line = server.wait_for(lambda l: " established " in l, 5, n + 1)[n]
            want = {"suite": sname, "group": gname, "sni": HOST, "alpn": "echo", "hrr": hrr}
            for k, v in want.items():
                if field(line, k) != v:
                    return f"row {n}: {k}={field(line, k)}, wanted {v}: {line}"
        return f"ok ({len(rows)} rows)"
    finally:
        server.stop()


def case_many(exe):
    server = Server(exe, extra=["--connections", "256", "--handshakes", "64", "--rate", "100000"])
    try:
        errors = []
        start = threading.Barrier(200)

        def client(k):
            try:
                rnd = random.Random(k)
                start.wait(30)
                with connect(server.port, timeout=60) as c:
                    for _ in range(20):
                        echo(c, rnd.randbytes(rnd.randint(1, 20000)))
                    c.unwrap()
            except Exception as e:  # noqa: BLE001 -- every failure is reported
                errors.append(f"{k}: {e!r}")

        threads = [threading.Thread(target=client, args=(k,)) for k in range(200)]
        t0 = time.monotonic()
        for t in threads:
            t.start()
        for t in threads:
            t.join(120)
        elapsed = time.monotonic() - t0
        if errors:
            return f"{len(errors)} of 200 failed, first: {errors[0]}"
        ended = server.wait_for(lambda l: " closed ok " in l, 10, 200)
        return f"ok (200 connections, 4,000 echoes, {elapsed:.1f} s; {len(ended)} closed ok)"
    finally:
        server.stop()


def case_reload(exe):
    server = Server(exe, identity="first")
    try:
        old = connect(server.port)
        echo(old, b"before the reload")
        if serial(old) != file_serial("first"):
            return f"first connection's certificate is {serial(old)}"
        server.install("renewed", files=("chain.pem", "key.pem"))
        server.signal(signal.SIGHUP)
        server.wait_for(lambda l: l == "reload 0 ok", 5)
        new = connect(server.port)
        echo(new, b"after the reload")
        if serial(new) != file_serial("renewed"):
            return f"a connection after the reload got {serial(new)}, not the renewed {file_serial('renewed')}"
        echo(old, b"the old connection still echoes")
        # A refused reload: the first chain with the renewed key.
        server.install("first", files=("chain.pem",))
        server.signal(signal.SIGHUP)
        server.wait_for(lambda l: l == "reload 0 refused tls-server-key-mismatch", 5)
        third = connect(server.port)
        echo(third, b"after a refused reload")
        if serial(third) != file_serial("renewed"):
            return f"after a refused reload the server sent {serial(third)}"
        for c in (old, new, third):
            c.unwrap()
            c.close()
        return "ok (old kept, new renewed, refused reload left renewed serving)"
    finally:
        server.stop()


# Python's ssl hands a session only to the SSLContext it came from.
SESSION_CONTEXT = context()


def make_session(port):
    """A full connection that echoes (so the client has read the NewSessionTickets): its session."""
    raw = socket.create_connection(("127.0.0.1", port), timeout=10)
    c = SESSION_CONTEXT.wrap_socket(raw, server_hostname=HOST)
    echo(c, b"to get a ticket")
    sess = c.session
    reused = c.session_reused
    c.unwrap()
    c.close()
    return sess, reused


def resume_with(port, sess):
    """A connection offering `sess`: whether it resumed, and the session it ends with."""
    raw = socket.create_connection(("127.0.0.1", port), timeout=10)
    c = SESSION_CONTEXT.wrap_socket(raw, server_hostname=HOST, session=sess)
    echo(c, b"offering a ticket")
    reused = c.session_reused
    new = c.session
    c.unwrap()
    c.close()
    return reused, new


def case_tickets(exe):
    off = Server(exe)
    try:
        sess, _ = make_session(off.port)
        reused, _ = resume_with(off.port, sess)
        if reused:
            return "a server without --tickets resumed"
    finally:
        off.stop()
    server = Server(exe, extra=["--tickets", "2"])
    try:
        sess, first = make_session(server.port)
        if first:
            return "the first connection resumed"
        for n in range(3):
            reused, _ = resume_with(server.port, sess)
            if not reused:
                return f"resumption {n} from one session did not resume"
        lines = server.wait_for(lambda l: " established " in l, 5, 4)
        got = [field(l, "resumed") for l in lines]
        if got != ["no", "yes", "yes", "yes"]:
            return f"the server's lines say resumed={got}"
        return "ok (full, then resumed 3 times from one session; off without the flag)"
    finally:
        server.stop()


def case_ticket_keys(exe):
    k1, k2, k3 = (os.urandom(32).hex().encode() for _ in range(3))
    a = Server(exe, extra=["--tickets", "1", "--ticket-keys", "keys"], files={"keys": k1 + b"\n"})
    b = Server(exe, extra=["--tickets", "1", "--ticket-keys", "keys"], files={"keys": k1 + b"\n"})
    c = Server(exe, extra=["--tickets", "1", "--ticket-keys", "keys"], files={"keys": k3 + b"\n"})
    try:
        sess, _ = make_session(a.port)
        if not resume_with(b.port, sess)[0]:
            return "process B, with A's key file, did not resume A's session"
        if resume_with(c.port, sess)[0]:
            return "process C, with another key, resumed A's session"
        # A rotation: the new key on top, the old below it, then SIGHUP.
        open(os.path.join(a.work, "keys"), "wb").write(k2 + b"\n" + k1 + b"\n")
        a.signal(signal.SIGHUP)
        a.wait_for(lambda l: l == "reload tickets ok", 5)
        reused, newer = resume_with(a.port, sess)
        if not reused:
            return "after a rotation the previous key no longer opened the session"
        # The session it ends with was made under the new key; drop the old key.
        sess2, _ = make_session(a.port)
        open(os.path.join(a.work, "keys"), "wb").write(k2 + b"\n")
        a.signal(signal.SIGHUP)
        a.wait_for(lambda l: l == "reload tickets ok", 5, 2)
        if resume_with(a.port, sess)[0]:
            return "a session from a key that was dropped resumed"
        if not resume_with(a.port, sess2)[0]:
            return "a session from the current key did not resume after the old key was dropped"
        # A bad file is refused and the keys stay.
        open(os.path.join(a.work, "keys"), "wb").write(b"not hex\n")
        a.signal(signal.SIGHUP)
        a.wait_for(lambda l: l == "reload tickets refused tls-server-ticket-key", 5)
        if not resume_with(a.port, sess2)[0]:
            return "a refused key file took the keys away"
        return "ok (A's session resumed at B, not at C; rotated by SIGHUP; the dropped key refused; a bad file refused)"
    finally:
        for s in (a, b, c):
            s.stop()


def case_ticket_identity(exe):
    server = Server(exe, identity="first", extra=["--tickets", "1"])
    try:
        sess, _ = make_session(server.port)
        if not resume_with(server.port, sess)[0]:
            return "no resumption before the reload"
        server.install("first", files=("chain.pem", "key.pem"))
        server.signal(signal.SIGHUP)
        server.wait_for(lambda l: l == "reload 0 ok", 5)
        reused, sess = resume_with(server.port, sess)
        if not reused:
            return "a reload of the same certificate refused the ticket"
        server.install("renewed", files=("chain.pem", "key.pem"))
        server.signal(signal.SIGHUP)
        server.wait_for(lambda l: l == "reload 0 ok", 5, 2)
        reused, sess = resume_with(server.port, sess)
        if reused:
            return "a ticket made for the old certificate resumed after the identity was replaced"
        # The session it ends with is for the renewed certificate.
        if not resume_with(server.port, sess)[0]:
            return "the renewed certificate's own session did not resume"
        return "ok (kept across a reload of the same certificate, refused after a renewal, the new one resumes)"
    finally:
        server.stop()


def case_ticket_lifetime(exe):
    server = Server(exe, extra=["--tickets", "1", "--ticket-lifetime", "2"])
    try:
        sess, _ = make_session(server.port)
        if not resume_with(server.port, sess)[0]:
            return "no resumption inside the lifetime"
        time.sleep(3.2)
        if resume_with(server.port, sess)[0]:
            return "a session resumed after its lifetime"
        return "ok (inside 2 s resumed, 3.2 s later not)"
    finally:
        server.stop()


def case_bound(exe):
    server = Server(exe, extra=["--handshakes", "2", "--handshake-timeout", "1500"])
    try:
        stalls = [socket.create_connection(("127.0.0.1", server.port)) for _ in range(2)]
        time.sleep(0.3)
        t0 = time.monotonic()
        c = connect(server.port, timeout=10)
        took = time.monotonic() - t0
        echo(c, b"behind two stalled handshakes")
        c.unwrap()
        c.close()
        timeouts = server.wait_for(lambda l: "closed handshake-timeout" in l, 5, 2)
        line = server.wait_for(lambda l: " established " in l, 5)[0]
        waited = int(field(line, "waited"))
        for s in stalls:
            s.close()
        if took < 1.0:
            return f"the honest client was not delayed ({took:.2f} s)"
        if waited < 1000:
            return f"the server says it waited {waited} ms"
        return f"ok (delayed {took:.2f} s, server: waited={waited} ms; {len(timeouts)} stalled handshakes timed out)"
    finally:
        server.stop()


def case_rate(exe):
    server = Server(exe, extra=["--rate", "5"])
    try:
        errors = []

        def client(k):
            try:
                with connect(server.port, timeout=30) as c:
                    echo(c, f"client {k}".encode())
                    c.unwrap()
            except Exception as e:  # noqa: BLE001
                errors.append(f"{k}: {e!r}")

        threads = [threading.Thread(target=client, args=(k,)) for k in range(15)]
        t0 = time.monotonic()
        for t in threads:
            t.start()
        for t in threads:
            t.join(60)
        elapsed = time.monotonic() - t0
        if errors:
            return f"{len(errors)} failed, first: {errors[0]}"
        lines = server.wait_for(lambda l: " established " in l, 5, 15)
        waits = sorted(int(field(l, "waited")) for l in lines)
        if elapsed < 1.8 or waits[-1] < 1500:
            return f"not rate-limited: {elapsed:.2f} s, waits {waits}"
        return f"ok (15 in {elapsed:.2f} s; waited, ms: {waits[0]} to {waits[-1]})"
    finally:
        server.stop()


def case_full(exe):
    server = Server(exe, extra=["--connections", "4"])
    try:
        held = [socket.create_connection(("127.0.0.1", server.port)) for _ in range(4)]
        time.sleep(0.3)
        extra = socket.create_connection(("127.0.0.1", server.port), timeout=5)
        t0 = time.monotonic()
        try:
            got = extra.recv(1)
        except ConnectionResetError:
            got = b""
        took = time.monotonic() - t0
        server.wait_for(lambda l: l.endswith(" full"), 5)
        for s in held + [extra]:
            s.close()
        if got != b"" or took > 1.0:
            return f"the fifth connection was not closed at once ({got!r}, {took:.2f} s)"
        return f"ok (closed in {took * 1000:.0f} ms)"
    finally:
        server.stop()


def case_idle(exe):
    server = Server(exe, extra=["--idle", "1000"])
    try:
        c = connect(server.port)
        echo(c, b"then nothing")
        t0 = time.monotonic()
        c.settimeout(5)
        got = c.recv(10)
        took = time.monotonic() - t0
        server.wait_for(lambda l: "closed idle" in l, 5)
        c.close()
        if got != b"" or took < 0.8 or took > 2.0:
            return f"not closed at the idle timeout ({got!r} after {took:.2f} s)"
        return f"ok (close_notify after {took:.2f} s)"
    finally:
        server.stop()


def case_shutdown(exe):
    server = Server(exe)
    try:
        conns = [connect(server.port) for _ in range(10)]
        for k, c in enumerate(conns):
            echo(c, f"connection {k}".encode())
        t0 = time.monotonic()
        server.signal(signal.SIGTERM)
        notified = 0
        for c in conns:
            c.settimeout(5)
            if c.recv(10) == b"":
                notified += 1
            c.close()
        status = server.proc.wait(5)
        took = time.monotonic() - t0
        closed = server.wait_for(lambda l: "closed shutdown" in l, 5, 10)
        if notified != 10 or status != 0:
            return f"{notified} of 10 got close_notify; exit status {status}"
        return f"ok (10 close_notify, {len(closed)} logged, exit 0 in {took:.2f} s)"
    finally:
        server.stop()


def second_source():
    """A second local address to connect from, or None where there is only 127.0.0.1 (macOS)."""
    s = socket.socket()
    try:
        s.bind(("127.0.0.2", 0))
        return "127.0.0.2"
    except OSError:
        return None
    finally:
        s.close()


def connect_from(port, source, alpn=None, timeout=10):
    raw = socket.socket()
    raw.settimeout(timeout)
    raw.bind((source, 0))
    raw.connect(("127.0.0.1", port))
    return context(alpn).wrap_socket(raw, server_hostname=HOST)


def case_peer(exe):
    server = Server(exe, extra=["--connections", "2"])
    try:
        c = connect(server.port)
        local = c.getsockname()[1]
        echo(c, b"who am i")
        established = server.wait_for(lambda l: " established " in l, 5)[0]
        c.unwrap()
        c.close()
        closed = server.wait_for(lambda l: " closed " in l, 5)[0]
        want = f"127.0.0.1:{local}"
        for name, line in (("established", established), ("closed", closed)):
            if field(line, "peer") != want:
                return f"the {name} line says peer={field(line, 'peer')}, wanted {want}: {line}"
        # Over the table: refused at once, and the line still says who.
        held = [socket.create_connection(("127.0.0.1", server.port)) for _ in range(2)]
        time.sleep(0.3)
        extra = socket.create_connection(("127.0.0.1", server.port), timeout=5)
        refused = server.wait_for(lambda l: l.startswith("refused ") and l.endswith(" full"), 5)[0]
        if field(refused, "peer") != f"127.0.0.1:{extra.getsockname()[1]}":
            return f"the refused line says peer={field(refused, 'peer')}: {refused}"
        for h in held + [extra]:
            h.close()
        return f"ok (peer={want} on established and closed; refused names its peer too)"
    finally:
        server.stop()


def case_per_address(exe):
    server = Server(exe, extra=["--per-address", "3"])
    try:
        held = [socket.create_connection(("127.0.0.1", server.port)) for _ in range(3)]
        time.sleep(0.3)
        extra = socket.create_connection(("127.0.0.1", server.port), timeout=5)
        t0 = time.monotonic()
        try:
            got = extra.recv(1)
        except ConnectionResetError:
            got = b""
        took = time.monotonic() - t0
        refused = server.wait_for(lambda l: l.startswith("refused ") and l.endswith(" per-address"), 5)[0]
        if field(refused, "peer") != f"127.0.0.1:{extra.getsockname()[1]}":
            return f"the refused line names {field(refused, 'peer')}: {refused}"
        if got != b"" or took > 1.0:
            return f"the fourth connection was not closed at once ({got!r}, {took:.2f} s)"
        note = "no second address here (macOS): the unaffected-client step is skipped"
        other = second_source()
        if other:
            c = connect_from(server.port, other)
            echo(c, b"from another address")
            line = server.wait_for(lambda l: " established " in l and f"peer={other}:" in l, 5)[0]
            c.unwrap()
            c.close()
            note = f"a client from {other} is not affected ({field(line, 'peer')})"
        # A place freed by a close is usable again.
        held.pop().close()
        time.sleep(0.3)
        again = connect(server.port)
        echo(again, b"after a close")
        again.unwrap()
        again.close()
        for h in held + [extra]:
            h.close()
        return f"ok (fourth refused in {took * 1000:.0f} ms as per-address; {note}; a freed place is reusable)"
    finally:
        server.stop()


def case_addr_rate(exe):
    server = Server(exe, extra=["--per-address-rate", "2"])
    try:
        errors = []
        done = []

        def client(k):
            try:
                with connect(server.port, timeout=30) as c:
                    echo(c, f"client {k}".encode())
                    c.unwrap()
                done.append(time.monotonic())
            except Exception as e:  # noqa: BLE001
                errors.append(f"{k}: {e!r}")

        threads = [threading.Thread(target=client, args=(k,)) for k in range(6)]
        t0 = time.monotonic()
        for t in threads:
            t.start()
        note = "no second address here (macOS): the unaffected-client step is skipped"
        other = second_source()
        if other:
            time.sleep(0.3)
            t1 = time.monotonic()
            c = connect_from(server.port, other, timeout=30)
            echo(c, b"another address")
            c.unwrap()
            c.close()
            quick = time.monotonic() - t1
            if quick > 1.0:
                return f"a client from {other} waited {quick:.2f} s behind the other address"
            note = f"a client from {other} took {quick * 1000:.0f} ms meanwhile"
        for t in threads:
            t.join(60)
        elapsed = time.monotonic() - t0
        if errors:
            return f"{len(errors)} failed, first: {errors[0]}"
        lines = server.wait_for(lambda l: " established " in l and "peer=127.0.0.1:" in l, 5, 6)
        waits = sorted(int(field(l, "waited")) for l in lines)
        if elapsed < 1.8 or waits[-1] < 1500:
            return f"not limited per address: {elapsed:.2f} s, waits {waits}"
        return f"ok (6 from one address in {elapsed:.2f} s, waited {waits[0]} to {waits[-1]} ms; {note})"
    finally:
        server.stop()


CASES = {"suites": case_suites, "many": case_many, "reload": case_reload, "bound": case_bound,
         "rate": case_rate, "full": case_full, "idle": case_idle, "shutdown": case_shutdown,
         "peer": case_peer, "per-address": case_per_address, "addr-rate": case_addr_rate,
         "tickets": case_tickets, "ticket-keys": case_ticket_keys, "ticket-identity": case_ticket_identity,
         "ticket-lifetime": case_ticket_lifetime}


# ---- the cost ----

def cpu_seconds(pid):
    if os.path.exists(f"/proc/{pid}/stat"):
        fields = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
        return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
    text = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True).stdout.decode().strip()
    parts = [float(p) for p in text.replace("-", ":").split(":")]
    total = 0.0
    for p in parts:
        total = total * 60 + p
    return total


UNBOUNDED = ["--connections", "1024", "--handshakes", "1024", "--rate", "1000000"]


def cost(exe, seconds, clients, extra):
    server = Server(exe, extra=extra)
    try:
        before = cpu_seconds(server.proc.pid)
        procs = [subprocess.Popen(["openssl", "s_time", "-connect", f"127.0.0.1:{server.port}", "-new",
                                   "-time", str(seconds), "-CAfile", CA],
                                  stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) for _ in range(clients)]
        done = 0
        for p in procs:
            out = p.communicate(timeout=seconds + 60)[0].decode()
            m = re.search(r"(\d+) connections in [\d.]+s", out)
            done += int(m.group(1)) if m else 0
        time.sleep(1)
        cpu = cpu_seconds(server.proc.pid) - before
        established = sum(1 for l in server.text() if " established " in l)
        return done, cpu, established
    finally:
        server.stop()


def cost_resumed(exe, seconds, extra):
    """The server's CPU for connections that resume: one full handshake, then `seconds` of connections offering its
    session (Python's `ssl`; the server's lines say how many resumed)."""
    server = Server(exe, extra=extra)
    try:
        sess, _ = make_session(server.port)
        before = cpu_seconds(server.proc.pid)
        mark = len(server.lines)
        end = time.time() + seconds
        while time.time() < end:
            raw = socket.create_connection(("127.0.0.1", server.port), timeout=10)
            c = SESSION_CONTEXT.wrap_socket(raw, server_hostname=HOST, session=sess)
            c.sendall(b"x")
            c.recv(1)
            c.close()
        time.sleep(1)
        cpu = cpu_seconds(server.proc.pid) - before
        resumed = sum(1 for l in server.text()[mark:] if " established " in l and field(l, "resumed") == "yes")
        return resumed, cpu
    finally:
        server.stop()


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 2
    exe = os.path.abspath(args.pop(0))
    seconds = 0
    if "--cost" in args:
        k = args.index("--cost")
        seconds = int(args[k + 1])
        del args[k:k + 2]
    args = [a for a in args if a != "--tickets"]
    names = args or list(CASES)
    failed = 0
    for name in names:
        try:
            result = CASES[name](exe)
        except Exception as e:  # noqa: BLE001 -- a case that raises has failed
            result = f"failed: {e!r}"
        ok = result.startswith("ok")
        failed += 0 if ok else 1
        print(f"{name:10} {result}", flush=True)
    rows = [(1, UNBOUNDED, "no bounds"), (4, UNBOUNDED, "no bounds"), (4, [], "the defaults: --rate 100")]
    for clients, extra, what in (rows if seconds else []):
        done, cpu, established = cost(exe, seconds, clients, extra)
        rate = done / seconds
        per = cpu / done * 1000 if done else 0
        print(f"cost       {clients} client(s), {what}, {seconds} s: {done} handshakes ({established} logged), "
              f"{rate:.0f} a second, {cpu:.2f} s of server CPU ({cpu / seconds:.0%} of a core), "
              f"{per:.2f} ms a handshake", flush=True)
    if seconds and "--tickets" in sys.argv:
        for what, extra, resumed in [("full, tickets off", UNBOUNDED, False),
                                     ("full, tickets on (2 sent)", UNBOUNDED + ["--tickets", "2"], False),
                                     ("resumed, tickets on", UNBOUNDED + ["--tickets", "2"], True)]:
            if resumed:
                done, cpu = cost_resumed(exe, seconds, extra)
            else:
                done, cpu, _ = cost(exe, seconds, 1, extra)
            per = cpu / done * 1000 if done else 0
            print(f"cost       {what}: {done} handshakes in {seconds} s, {cpu:.2f} s of server CPU, "
                  f"{per:.2f} ms a handshake", flush=True)
    print(f"{len(names) - failed} of {len(names)} cases ok")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
