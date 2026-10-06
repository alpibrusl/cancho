#!/usr/bin/env python3
"""What a hostile server can make one connection cost (docs/tls-assurance.md §7).

    python3 scripts/tls_hostile.py <driver> <tls_many> [--massif]

`driver` is `tests/programs/tls_driver.cho` and `tls_many` is
`tests/programs/tls_many.cho`, both built with the package's files.

Each case below is one connection from the driver to a server written on
`scripts/tls_liar.py`'s, which does one hostile thing. The driver is its own
process, so `os.wait4` gives that connection's CPU time and peak resident
set. Every case runs on the same driver, whose own buffers are the same
size whatever it is fed, so a peak above the honest case's is memory the
connection took. The cases:

- an honest handshake, the baseline;
- the same server's bytes fed one at a time, one `feed` each;
- 32 KeyUpdates, which the client takes, then a 33rd, which it refuses;
- 16 user_canceled warnings, then a 17th, refused;
- 10,000 NewSessionTickets, each parsed and dropped;
- a Certificate message of exactly 64 KiB, the reassembly limit: the leaf and
  seven entries of zero bytes, the most the client takes;
- a chain at the depth limit, the leaf and six intermediates, every
  signature RSA-4096 (the slowest the verifier allows) and every key but
  the leaf's RSA-4096.

Then two stalls, against `tls_many` (one connection, its own 30-second
deadline), whose CPU time over the wait is the cost: a server that
accepts and sends nothing, and one that never reads, so the client's
request never leaves its buffer.

One line a case: its outcome, CPU seconds, and peak resident set in KiB.
With `--massif`, the connections run under valgrind's massif instead (so
their CPU times are valgrind's), with the peak of every page mapped, and
the stalls are skipped. Exit status 1 if a case ends otherwise than it says.
"""
import datetime
import os
import socket
import subprocess
import tempfile
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_liar as L  # noqa: E402

from cryptography import x509  # noqa: E402
from cryptography.hazmat.primitives import hashes, serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric import padding, rsa  # noqa: E402
from cryptography.x509.oid import NameOID  # noqa: E402


def peak_kib(pid):
    """The process's peak resident set (VmHWM), in KiB. Read from /proc
    while it runs, since `wait4`'s ru_maxrss also counts the forked Python
    the child was before its exec."""
    try:
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmHWM:"):
                return int(line.split()[1])
    except OSError:
        pass
    return 0


class Measured(L.Conversation):
    """A conversation whose driver's resource use is read when it ends; with
    `MASSIF` set, the driver runs under `valgrind --tool=massif
    --pages-as-heap=yes`, and the peak of every page it mapped (the program,
    its heap and the regions' `brk`) is read from massif's output."""

    def __init__(self, driver):
        self.massif = None
        if MASSIF:
            self.massif = tempfile.mktemp(suffix=".massif")
            driver_argv = ["valgrind", "--tool=massif", "--pages-as-heap=yes", f"--massif-out-file={self.massif}",
                           driver]
            self.proc = subprocess.Popen(driver_argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                         stderr=subprocess.DEVNULL, text=True, bufsize=1)
            self.lines, self.answers, self.sent, self.received = [], [], b"", b""
        else:
            super().__init__(driver)

    def mapped_peak(self):
        if not self.massif:
            return None
        peak = max(int(line.split("=")[1]) for line in open(self.massif) if line.startswith("mem_heap_B="))
        os.unlink(self.massif)
        return peak // 1024

    def close(self):
        self.peak = peak_kib(self.proc.pid)
        self.proc.stdin.close()
        _, status, usage = os.wait4(self.proc.pid, 0)
        self.proc.returncode = status
        self.usage = usage


def honest(s):
    L.honest(s)


def one_byte(s):
    """The honest flight, one byte a line."""
    s.start()
    flight = s.hello_and_flight()
    for i in range(len(flight)):
        s.c.feed(flight[i:i + 1])
    s.check_client_finished()


def after_handshake(s):
    s.start()
    s.c.feed(s.hello_and_flight())
    s.check_client_finished()


def key_updates(s):
    after_handshake(s)
    for _ in range(32):
        s.c.feed(s.write.seal(22, L.message(24, b"\0")))
        s.write = s.write.next()
    f = s.c.feed(s.write.seal(22, L.message(24, b"\0")))
    return f


def warnings(s):
    after_handshake(s)
    f = None
    for _ in range(17):
        f = s.c.feed(s.write.seal(21, b"\1\x5a"))
    return f


def tickets(s):
    after_handshake(s)
    ticket = L.message(4, (7200).to_bytes(4, "big") + bytes(4) + b"\1\0" + L.u16(3) + b"abc" + L.u16(0))
    batch = 100
    for _ in range(10000 // batch):
        s.c.feed(b"".join(s.write.seal(22, ticket) for _ in range(batch)))
    return s.c.feed(s.write.seal(21, b"\1\0"))


def largest_certificate(s):
    """A Certificate message of 65,536 bytes with its header, the reassembly
    limit: the leaf, then seven entries of zero bytes filling the rest, as
    many as the eight-certificate limit leaves (`tls_message.max_certificates`)."""
    leaf = L.u24(len(L.DER)) + L.DER + L.u16(0)
    room = 65536 - 4 - 4 - len(leaf)
    each = room // 7
    sizes = [each - 5] * 6 + [room - 6 * each - 5]
    s.filler = [bytes(n) for n in sizes]
    s.start()
    s.c.feed(s.hello_and_flight())


def chain():
    """A root and six intermediates, RSA-4096, and a leaf for the liar's
    Ed25519 key: (the root's PEM, the leaf's DER, the intermediates' DER,
    leaf's issuer first)."""
    start = datetime.datetime(2026, 1, 1)
    end = start + datetime.timedelta(days=3650)
    keys = [rsa.generate_private_key(65537, 4096) for _ in range(7)]
    names = [x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, f"tls_hostile CA {i}")]) for i in range(7)]
    certs = []
    for i in range(7):
        issuer = names[i - 1] if i else names[0]
        signer = keys[i - 1] if i else keys[0]
        certs.append(x509.CertificateBuilder().subject_name(names[i]).issuer_name(issuer)
                     .public_key(keys[i].public_key()).serial_number(i + 1)
                     .not_valid_before(start).not_valid_after(end)
                     .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
                     .sign(signer, hashes.SHA256(), padding.PKCS1v15()))
    host = L.HOST.decode()
    leaf = (x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, host)]))
            .issuer_name(names[6]).public_key(L.KEY.public_key()).serial_number(100)
            .not_valid_before(start).not_valid_after(end)
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(host)]), False)
            .sign(keys[6], hashes.SHA256(), padding.PKCS1v15()))
    der = lambda c: c.public_bytes(serialization.Encoding.DER)  # noqa: E731
    return (certs[0].public_bytes(serialization.Encoding.PEM), der(leaf),
            [der(c) for c in reversed(certs[1:])])


CHAIN = None


def deep_chain(s):
    root, leaf, intermediates = CHAIN
    s.cert_der = leaf
    s.filler = intermediates
    L.ROOTS = root
    try:
        L.honest(s)
    finally:
        L.ROOTS = ORIGINAL_ROOTS


ORIGINAL_ROOTS = L.ROOTS
MASSIF = "--massif" in sys.argv

# (name, the outcome it must end with, the server's script)
CASES = [
    ("an honest handshake", "ok", honest),
    ("the same, one byte a feed", "ok", one_byte),
    ("32 KeyUpdates, then a 33rd", "tls-too-many-messages", key_updates),
    ("16 user_canceled warnings, then a 17th", "tls-too-many-messages", warnings),
    ("10,000 NewSessionTickets", "ok", tickets),
    ("a Certificate message of 64 KiB", "ok", largest_certificate),
    ("a chain at the depth limit, RSA-4096 throughout", "ok", deep_chain),
]


def connection(driver, fn):
    conv = Measured(driver)
    s = L.Server(conv)
    error = None
    try:
        fn(s)
    except (L.Failed, AssertionError) as e:
        error = str(e)
    conv.close()
    tag = conv.answers[-1].split(" ")[1] if conv.answers else "none"
    if os.WIFSIGNALED(conv.proc.returncode):
        tag = f"signal {os.WTERMSIG(conv.proc.returncode)}"
    return tag, error, conv.usage, conv.peak, conv.mapped_peak()


def stall(tls_many, reads):
    """`tls_many` against a server that accepts and then sends nothing, and
    if `reads` is False never reads either. Its CPU seconds and peak KiB."""
    sock = socket.socket()
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 4096)
    sock.bind(("127.0.0.1", 0))
    sock.listen(1)
    held = []

    def accept():
        conn, _ = sock.accept()
        held.append(conn)
        while reads:
            if not conn.recv(65536):
                return

    threading.Thread(target=accept, daemon=True).start()
    proc = subprocess.Popen([tls_many, "127.0.0.1", str(sock.getsockname()[1]), "stall.lex-sys.test", "1", "65536"],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    proc.stdin.write(ORIGINAL_ROOTS)
    proc.stdin.close()
    started = time.monotonic()
    peak = 0
    while True:
        peak = max(peak, peak_kib(proc.pid))
        pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
        if pid:
            break
        time.sleep(0.5)
    waited = time.monotonic() - started
    out = proc.stdout.read().decode().splitlines()
    sock.close()
    for c in held:
        c.close()
    return out, waited, usage, peak


def main():
    global CHAIN
    driver, tls_many = [a for a in sys.argv[1:] if a != "--massif"][:2]
    CHAIN = chain()
    bad = 0
    print(f"{'case':48} {'outcome':24} {'CPU s':>7} {'peak KiB':>9}" + (f" {'mapped KiB':>11}" if MASSIF else ""))
    for name, want, fn in CASES:
        tag, error, usage, peak, mapped = connection(driver, fn)
        ok = tag == want
        bad += not ok
        cpu = usage.ru_utime + usage.ru_stime
        note = "" if ok else f"  WANTED {want}" + (f" ({error})" if error else "")
        extra = f" {mapped:11}" if mapped is not None else ""
        print(f"{name:48} {tag:24} {cpu:7.3f} {peak:9}{extra}{note}", flush=True)
    if MASSIF:
        sys.exit(1 if bad else 0)
    for name, reads in (("a server that sends nothing", True), ("a server that never reads", False)):
        out, waited, usage, peak = stall(tls_many, reads)
        cpu = usage.ru_utime + usage.ru_stime
        tag = out[0].split()[2] if out and len(out[0].split()) > 2 else "none"
        ok = tag == "timeout"
        bad += not ok
        note = "" if ok else f"  WANTED timeout: {out}"
        print(f"{name + f', {waited:.0f} s':48} {tag:24} {cpu:7.3f} {peak:9}{note}", flush=True)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
