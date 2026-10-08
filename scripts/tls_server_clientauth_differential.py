#!/usr/bin/env python3
"""`packages/tls`'s server beside `openssl s_server -Verify/-verify` on the same client flights
(docs/tls-server.md §13.11).

    python3 scripts/tls_server_clientauth_differential.py <tls_serve> [<case name substring> ...]

The client's flight (Certificate, CertificateVerify, Finished) depends on the server's, since it is encrypted under
keys both sides derive, so the recorded bytes of `tests/vectors/tls/liar_client_auth.txt` cannot be sent to
another server as `scripts/tls_server_differential.py` does with ClientHellos. Here the client of
`scripts/tls_liar_client_auth.py` plays each case **live over a socket against both servers**: `tls_serve` (the
http mode, `--client-ca <file> required|optional`) and `openssl s_server -tls1_3 -www -Verify 5` (`-verify 5` for
optional, with `-verify_return_error`, without which s_server goes on after a verification error). Its flight is
parsed leniently here (the two servers send different CertificateRequests and EncryptedExtensions), the client's
keys are derived from what each really sent, and the client's second flight is the case's.

For each case, the two outcomes: `established` (an HTTP answer comes back under the application keys), `alert N`
(a fatal alert, read under the server's application key or in the clear), or `closed` (no alert, no data). Each line
is the case, both outcomes and `agree`; `alert` when both refuse with different alerts, which RFC 8446 §6.2 often
leaves open; `known` when they differ on accept or refuse as `EXPECTED` says, and why; `DIFFER` when they differ
otherwise, and `STALE` when a difference `EXPECTED` names is gone. Exit status 1 on any `DIFFER` or `STALE`.
"""
import os
import socket
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_liar_client_auth as auth  # noqa: E402
import tls_liar_client as liar  # noqa: E402
from tls_liar_client import HOST, Keys, derive, records  # noqa: E402

import hmac  # noqa: E402

IDLE = 0.6  # seconds of quiet that end a read of what the server sent

# Where the two differ on accept or refuse, each on purpose, with why. A difference not here fails the run, and so
# does one here that stops happening.
EXPECTED = {
    "six certificates (over the bound of five)":
        "this server reads at most five certificates in a client's Certificate (docs/tls-server.md §13.2) and answers "
        "x509-chain-too-large; OpenSSL's bound is the verify depth, which this run sets to 5 intermediates",
    "an intermediate more than the bound of three: leaf, four intermediates":
        "this server allows three intermediates between a client's leaf and its root (§13.2); OpenSSL, at depth 5, "
        "allows the four",
    "a self-signed leaf that is not a CA, pinned in the store: a store holds CAs":
        "a store holds CAs: a certificate that is not one (basicConstraints cA absent) is `x509-not-ca` even when "
        "it is in the store and is the client's own leaf (docs/tls-server.md §13.4); OpenSSL trusts any certificate "
        "of its CAfile as an anchor, so a device pinned by its self-signed certificate passes there",
}


class Wire:
    """A server process and a socket to it, with the `feed` and `take` of `tls_liar_client.Conversation`."""

    def __init__(self, kind, exe, mode, store_pem, work):
        self.kind, self.sent, self.closed, self.received = kind, b"", False, b""
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            self.port = s.getsockname()[1]
        store = os.path.join(work, "store.pem")
        open(store, "wb").write(store_pem)
        open(os.path.join(work, "main.pem"), "wb").write(liar.CHAIN)
        open(os.path.join(work, "main.key"), "wb").write(liar.key_pem(liar.MAIN_KEY))
        if kind == "cancho":
            argv = [exe, str(self.port), "http", "-", "0", "-", "-", "main.pem", "main.key", HOST.decode()]
            if mode:
                argv += ["--client-ca", "store.pem", {1: "optional", 2: "required"}[mode]]
        else:
            argv = ["openssl", "s_server", "-accept", str(self.port), "-cert", "main.pem", "-key", "main.key",
                    "-tls1_3", "-www", "-num_tickets", "0", "-quiet"]
            if mode:
                argv += ["-CAfile", "store.pem", "-Verify" if mode == 2 else "-verify", "5", "-verify_return_error"]
        self.proc = subprocess.Popen(argv, cwd=work, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT)
        for _ in range(100):
            try:
                self.sock = socket.create_connection(("127.0.0.1", self.port), timeout=1)
                break
            except OSError:
                time.sleep(0.05)
        else:
            raise RuntimeError(f"{kind} did not listen")
        self.sock.settimeout(IDLE)

    def feed(self, data):
        try:
            self.sock.sendall(data)
        except OSError:
            self.closed = True
        self.collect()
        return ["0", "ok", "1", "-", "-"]

    def collect(self):
        while not self.closed:
            try:
                chunk = self.sock.recv(65536)
            except socket.timeout:
                return
            except OSError:
                self.closed = True
                return
            if not chunk:
                self.closed = True
                return
            self.sent += chunk

    def take(self):
        out, self.sent = self.sent, b""
        return records(out)

    def ask(self, line):
        raise NotImplementedError(line)

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass
        self.proc.kill()
        self.proc.wait()


class Live(auth.AuthClient):
    """The client of `tls_liar_client_auth`, over a socket to the server of `Live.server`."""

    server = "cancho"
    exe = None
    work = None

    def setup(self, **_):
        self.c = Wire(self.server, self.exe, self.mode, self.store, self.work)

    def flight(self):
        """The server's flight, whatever it is: the keys from what was sent, the messages kept in the transcript."""
        recs = self.c.take()
        sh = recs[0][5:]
        assert recs[0][0] == 22, f"a ServerHello, got {recs[0][:5].hex()}"
        _, self.suite, ks = self.parse_server_hello(sh)
        group = int.from_bytes(ks[:2], "big")
        self.transcript += sh
        h = self.hash
        n = h().digest_size
        shared = self.shared_secret(group, ks[4:])
        early = hmac.new(bytes(n), bytes(n), h).digest()
        hs = hmac.new(derive(early, b"derived", b"", h), shared, h).digest()
        self.c_hs = derive(hs, b"c hs traffic", self.transcript, h)
        self.s_hs = derive(hs, b"s hs traffic", self.transcript, h)
        self.master = hmac.new(derive(hs, b"derived", b"", h), bytes(n), h).digest()
        self.read = Keys(self.s_hs, self.suite)
        self.write = Keys(self.c_hs, self.suite)
        data = b""
        for r in recs[1:]:
            if r[0] == 20:
                continue
            kind, content = self.read.open(r)
            assert kind == 22, kind
            data += content
        kinds = []
        while data:
            n = 4 + int.from_bytes(data[1:4], "big")
            kinds.append(data[0])
            self.transcript += data[:n]
            data = data[n:]
        assert kinds[-1] == 20, kinds
        self.asked = 13 in kinds
        self.app_th = self.transcript

    def outcome(self):
        """What the server did with the client's flight: established, alert N, or closed."""
        said = self.classify(self.c.take())
        if said:
            return said
        self.c.feed(self.write.seal(23, b"GET / HTTP/1.0\r\n\r\n"))
        said = self.classify(self.c.take())
        if said:
            return said
        return "closed" if self.c.closed else "silent"

    def classify(self, recs):
        for r in recs:
            if r[0] == 21:
                return f"alert {r[6]}" if r[5] == 2 else "warning"
            try:
                kind, content = self.read.open(r)
            except Exception:  # noqa: BLE001 -- a record that does not open is a finding, not a crash
                return "unreadable"
            if kind == 21:
                return f"alert {content[1]}"
            if kind == 23:
                return "established"
        return None


def run_case(server, exe, work, fn):
    Live.server, Live.exe, Live.work = server, exe, work
    c = Live(None)
    try:
        fn(c)
        return c.outcome()
    except Exception as e:  # noqa: BLE001 -- reported
        return f"error {type(e).__name__}: {e}"
    finally:
        if c.c is not None:
            c.c.close()


# Honest connections beside the refusals: the same flight through `refused()`'s defaults.
def honest(**kw):
    def run(c):
        for k, v in kw.items():
            setattr(c, k, v)
        c.refused()
    return run


HONEST = [
    ("honest: P-256 leaf alone", honest()),
    ("honest: P-256 leaf and its intermediate", honest(auth=dict(chain=[auth.VIA_INT, auth.INTERMEDIATE], key=auth.P256_KEY))),
    ("honest: P-256 leaf, intermediate and root", honest(auth=dict(chain=[auth.VIA_INT, auth.INTERMEDIATE, auth.CLIENT_CA], key=auth.P256_KEY))),
    ("honest: P-384 leaf", honest(suites=[0x1302], auth=dict(chain=[auth.DEVICE_384], key=auth.P384_KEY))),
    ("honest: RSA-PSS sha256", honest(auth=dict(chain=[auth.DEVICE_RSA], key=auth.RSA_KEY, scheme=0x0804))),
    ("honest: RSA-PSS sha384", honest(auth=dict(chain=[auth.DEVICE_RSA], key=auth.RSA_KEY, scheme=0x0805))),
    ("honest: Ed25519 leaf", honest(auth=dict(chain=[auth.DEVICE_ED], key=auth.ED_KEY))),
    ("honest: optional, a certificate", honest(mode=1)),
    ("honest: optional, none", honest(mode=1, auth=dict(chain=None))),
    ("honest: required, no change_cipher_spec", honest(ccs=False)),
    ("honest: the three messages in one record", lambda c: c.refused(together=True)),
]

# The refusals of `tls_liar_client_auth` that need only `refused()` and `auth_flight()`: by name.
REFUSALS = [
    "required, an empty Certificate",
    "required, Finished where the Certificate belongs",
    "required, a CertificateVerify where the Certificate belongs",
    "required, a Certificate and no CertificateVerify: Finished next",
    "optional, an empty Certificate and then a CertificateVerify",
    "a second Certificate where the CertificateVerify belongs",
    "a Certificate with a request context of one byte",
    "a CertificateEntry with an extension",
    "a Certificate whose list is cut short",
    "a Certificate whose length is wrong",
    "a certificate that is not DER",
    "six certificates (over the bound of five)",
    "a Certificate message over 16 KiB",
    "an intermediate more than the bound of three: leaf, four intermediates",
    "a P-521 key (no verifier for it)",
    "a leaf of a CA that is not in the store",
    "a self-signed leaf that is not in the store",
    "a self-signed leaf that is not a CA, pinned in the store: a store holds CAs",
    "optional, a certificate of a CA that is not in the store: refused, not ignored",
    "an expired leaf",
    "a leaf that is not yet valid",
    "a leaf whose extendedKeyUsage is serverAuth only",
    "a leaf whose keyUsage lacks digitalSignature",
    "a SAN outside the CA's name constraint",
    "a CertificateVerify signature with a bit flipped",
    "a CertificateVerify under the server's context string",
    "a CertificateVerify over the transcript before the client's Certificate",
    "a CertificateVerify signed by another key than the certificate's",
    "a scheme the request did not list (rsa_pkcs1_sha256, which is listed for certificates only)",
    "a scheme the request listed that the key does not fit (a P-256 key under ecdsa_secp384r1_sha384)",
    "an RSA key under a PKCS#1 scheme (forbidden in TLS 1.3)",
    "an Ed25519 key under an ECDSA scheme",
    "a client Finished with a wrong MAC after a good Certificate and CertificateVerify",
    "a Finished that leaves out the client's Certificate from its transcript",
]


def main():
    exe = os.path.abspath(sys.argv[1])
    only = sys.argv[2:]
    by_name = {name: fn for name, tag, alert, where, fn in auth.CASES}
    cases = list(HONEST) + [(n, by_name[n]) for n in REFUSALS]
    work = tempfile.mkdtemp(prefix="tls-clientauth-diff-")
    bad = 0
    counts = {"agree": 0, "alert": 0, "known": 0}
    for name, fn in cases:
        if only and not any(o in name for o in only):
            continue
        mine = run_case("cancho", exe, work, fn)
        theirs = run_case("openssl", exe, work, fn)
        if mine == theirs:
            verdict = "agree"
        elif mine.startswith("alert") and theirs.startswith("alert"):
            verdict = "alert"
        elif name in EXPECTED:
            verdict = "known"
        else:
            verdict = "DIFFER"
        if name in EXPECTED and verdict != "known":
            verdict = "STALE"
        why = f"  [{EXPECTED[name]}]" if verdict == "known" else ""
        print(f"{verdict:6} {name}: packages/tls {mine}; openssl {theirs}{why}", flush=True)
        if verdict in counts:
            counts[verdict] += 1
        else:
            bad += 1
    print(f"{counts['agree']} agree, {counts['alert']} differ in the alert only, {counts['known']} differ as EXPECTED says, "
          f"{bad} otherwise")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
