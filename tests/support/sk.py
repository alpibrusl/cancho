"""The scripted socket client of the command tests (docs/gate.md, G4 wave 3), standard library only.

A scenario is a python file in tests/commands/ that the test's script (tests/commands/NAME.sh) runs from the root of the repository:

    python3 tests/commands/NAME.py CANCHO SCRATCH

It builds the program under test with `build`, starts it with `Server`, drives it with `Client` (several at once on threads, a body written
in pieces with pauses, a read with a timeout, a connection held open while the server runs), and states each case with `run`, which prints
`ok: NAME` or `FAIL NAME: why` for it and leaves with status 1 if any case failed. Nothing here prints a time or a port, so the transcript
is the same on every run. Cancho programs have no way to be two clients at once, and the harness has no sockets of its own: this is the
smallest thing that can write "a body in pieces, with pauses, while another client is served".
"""
import os
import socket
import subprocess
import sys
import threading
import time
import traceback


class Failed(Exception):
    pass


def check(condition, why):
    if not condition:
        raise Failed(why() if callable(why) else why)


def same(got, want, why=""):
    if got != want:
        raise Failed("%s: got %r, wanted %r" % (why, clip(got), clip(want)))


def clip(value, size=300):
    text = repr(value)
    return text if len(text) <= size else text[:size] + "...(%d)" % len(text)


def run(cases):
    """Run each (name, function) pair; print its line; exit 1 if any failed. Cases run in the order given."""
    failed = 0
    for name, case in cases:
        try:
            case()
            print("ok: " + name)
        except Failed as why:
            failed += 1
            print("FAIL %s: %s" % (name, why))
        except Exception:
            failed += 1
            print("FAIL %s: %s" % (name, traceback.format_exc().strip().splitlines()[-1]))
            traceback.print_exc(file=sys.stderr)
        sys.stdout.flush()
    sys.exit(1 if failed else 0)


def build(cancho, scratch, name, files, flags=("--std",)):
    """Compile `files` with the compiler `cancho` into scratch/name; the path."""
    exe = os.path.join(scratch, name)
    done = subprocess.run([cancho, "build"] + list(files) + list(flags) + ["-o", exe], capture_output=True, text=True)
    check(done.returncode == 0, lambda: "`%s` should build %s, but said:\n%s" % (cancho, files, done.stderr))
    return exe


def fetch_dependencies(cancho, scratch, name, pairs):
    """`vcs fetch` each (lock, store) pair fresh into one directory of scratch (re-verifying every pin); the `.cho` files it wrote, sorted."""
    out = os.path.join(scratch, name + "-fetch")
    os.makedirs(out, exist_ok=True)
    for lock, store in pairs:
        done = subprocess.run([cancho, "vcs", "fetch", "--lock", lock, "--store", store, "-o", out], capture_output=True, text=True)
        check(done.returncode == 0, lambda: "vcs fetch %s said:\n%s" % (lock, done.stderr))
    return sorted(os.path.join(out, f) for f in os.listdir(out) if f.endswith(".cho"))


def free_port():
    probe = socket.socket()
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]
    probe.close()
    return port


class Server:
    """A compiled server on a port of its own: `Server(exe, [args after the port])`. The port is the first argument unless `port_at` says
    otherwise (None: the program takes none). `pick` is a function that offers a port (default: a free one). A server that is gone within
    300 ms of starting found its port taken: it is started again on another, up to five times. With `announce` (a format such as
    "listening on %d") the server is trusted to listen only once it has written that line to standard error, which it does after its `listen`
    succeeded: a connect proves nothing about who is listening (a port taken by another listener answers all the same). Use as a context
    manager; leaving it kills the server."""

    def __init__(self, exe, args=(), port_at=0, stdout=None, announce=None, pick=None):
        self.exe = exe
        self.stderr = None
        for attempt in range(5):
            self.port = (pick or free_port)()
            argv = [exe] + [str(a) for a in args]
            if port_at is not None:
                argv.insert(1 + port_at, str(self.port))
            self.child = subprocess.Popen(argv, stdout=stdout or subprocess.DEVNULL, stderr=subprocess.PIPE if announce else subprocess.DEVNULL)
            if announce:
                said = []
                reader = threading.Thread(target=lambda: said.append(self.child.stderr.readline()), daemon=True)
                reader.start()
                reader.join(30)
                if said and said[0].strip() == (announce % self.port).encode():
                    return
                self.stop()
                continue
            time.sleep(0.3)
            if self.child.poll() is None:
                return
        raise Failed("the server never stayed up")

    def client(self, timeout=10.0):
        return Client.connect(self.port, timeout)

    def get(self, path):
        """One short request on a connection of its own; the body of the answer."""
        c = self.client()
        c.send(("GET %s HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n" % path).encode())
        text = c.read_to_end().decode("latin-1")
        c.close()
        return text.split("\r\n\r\n", 1)[1] if "\r\n\r\n" in text else ""

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.stop()

    def stop(self):
        if self.child.poll() is None:
            self.child.kill()
        self.child.wait()
        if self.child.stderr:
            self.child.stderr.close()


class Client:
    def __init__(self, sock):
        self.sock = sock

    @staticmethod
    def connect(port, timeout=10.0, host="127.0.0.1"):
        """Dial, retrying for ten seconds while nothing listens."""
        deadline = time.time() + 10
        while True:
            try:
                sock = socket.create_connection((host, port))
                sock.settimeout(timeout)
                return Client(sock)
            except OSError as why:
                if time.time() > deadline:
                    raise Failed("never listened: %s" % why)
                time.sleep(0.02)

    def send(self, data):
        self.sock.sendall(data)

    def pieces(self, data, size, pause=0.0):
        """Write `data` in pieces of `size` bytes, `pause` seconds between them."""
        for at in range(0, len(data), size):
            self.sock.sendall(data[at:at + size])
            if pause:
                time.sleep(pause)

    def read_exact(self, n):
        got = b""
        while len(got) < n:
            more = self.sock.recv(n - len(got))
            if not more:
                raise Failed("the connection ended after %d of %d bytes" % (len(got), n))
            got += more
        return got

    def head(self):
        """One response's head (to the blank line, inclusive), or what arrived before the connection ended."""
        got = b""
        while not got.endswith(b"\r\n\r\n"):
            one = self.sock.recv(1)
            if not one:
                break
            got += one
        return got.decode("latin-1")

    def response(self):
        """(head, body) of one response; a `100 Continue` comes back as it is, with an empty body."""
        head = self.head()
        n = 0
        for line in head.split("\r\n"):
            if line.startswith("Content-Length: "):
                n = int(line.split(": ", 1)[1].strip())
        return head, self.read_exact(n).decode("latin-1")

    def ends(self, within=5.0):
        """Does the connection end (within `within` seconds) with nothing more to read?"""
        self.sock.settimeout(within)
        while True:
            try:
                if not self.sock.recv(64):
                    return True
            except (ConnectionResetError, BrokenPipeError):
                return True
            except OSError:
                return False

    def read_to_end(self, within=10.0):
        self.sock.settimeout(within)
        got = b""
        while True:
            more = self.sock.recv(65536)
            if not more:
                return got
            got += more

    def read_until(self, needle, limit):
        """Read until `needle` has arrived, the connection ends or `limit` seconds pass: (bytes, [(time, total so far)] of each read)."""
        self.sock.settimeout(0.05)
        end = time.time() + limit
        got = b""
        marks = []
        while time.time() < end and needle not in got:
            try:
                more = self.sock.recv(8192)
            except socket.timeout:
                continue
            if not more:
                break
            got += more
            marks.append((time.time(), len(got)))
        return got, marks

    def reply(self):
        """One response off the connection, read by its Content-Length from a buffer of this client's own (use it instead of `head`/`response`, not
        with them): (status, head, body), or None at the end of the stream (a reset is the end too)."""
        if not hasattr(self, "buf"):
            self.buf = b""
        while b"\r\n\r\n" not in self.buf:
            try:
                more = self.sock.recv(4096)
            except ConnectionResetError:
                return None
            if not more:
                return None
            self.buf += more
        end = self.buf.index(b"\r\n\r\n")
        head = self.buf[:end].decode("latin-1")
        n = None
        for line in head.split("\r\n"):
            if line.startswith("Content-Length: "):
                n = int(line.split(": ", 1)[1])
        check(n is not None, "every response declares its length")
        while len(self.buf) < end + 4 + n:
            more = self.sock.recv(4096)
            check(more, "the connection closed inside a body")
            self.buf += more
        body = self.buf[end + 4:end + 4 + n].decode("latin-1")
        self.buf = self.buf[end + 4 + n:]
        return int(head.split(" ")[1]), head, body

    def ask(self, request):
        self.send(request.encode() if isinstance(request, str) else request)
        got = self.reply()
        check(got is not None, "an answer")
        return got

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass


def concurrently(jobs):
    """Run each function of `jobs` on a thread of its own, all started at once; the list of their results (an exception is re-raised here)."""
    results = [None] * len(jobs)
    errors = []

    def one(at, job):
        try:
            results[at] = job()
        except BaseException as why:
            errors.append(why)

    threads = [threading.Thread(target=one, args=(at, job)) for at, job in enumerate(jobs)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    if errors:
        raise errors[0]
    return results


def body_of(n, seed):
    """Byte i of stream `seed` is 97 + (7 i + i / 13 + seed) mod 26: the test bodies of the http-server tests. The pattern repeats every 338 bytes."""
    period = bytes(97 + (i * 7 + i // 13 + seed) % 26 for i in range(338))
    return (period * (n // 338 + 1))[:n]


def checksum(body):
    c = 0
    for b in body:
        c = (c * 131 + b) % 1_000_000_007
    return c


def tally(body):
    return "n=%d c=%d" % (len(body), checksum(body))
