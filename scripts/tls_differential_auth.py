#!/usr/bin/env python3
"""`packages/tls` beside `openssl s_client` on the lying server's client-certificate and ALPN flights (docs/tls-parity.md §6.10).

    python3 scripts/tls_differential_auth.py [<case name substring> ...]

The cases of `scripts/tls_liar_auth.py` (the same server code, unchanged) run against OpenSSL's client instead, over a socket, as
`scripts/tls_differential.py` runs the 84: `openssl s_client` with the liar's CA as its only root, its host name and the time the
certificates are valid at, and, where a case configures them, `-cert` and `-key` for the identity (the driver's `I` line) and
`-alpn` for the offer (`L`). The server checks what OpenSSL sends exactly as it checks `packages/tls`'s: the Certificate, the
CertificateVerify under the identity's key (so OpenSSL's TLS 1.3 and TLS 1.2 signatures are verified by the same code), and the
Finished. `packages/tls`'s outcome is the case's expectation (`tests/vectors/tls/liar_auth.txt`). Each line is the case, both
outcomes and `agree`; `alert` when they differ in the alert alone; `known` when they differ on accept or refuse as `EXPECTED`
says, and why; `DIFFER` otherwise; `STALE` when a difference `EXPECTED` names is gone. Exit status 1 on any `DIFFER` or `STALE`.
What the client reports about itself (`Y`) has no OpenSSL counterpart here, so those checks are skipped under OpenSSL.
"""
import os
import select
import subprocess
import sys
import tempfile
import threading

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tls_differential as D  # noqa: E402
import tls_liar as L  # noqa: E402
import tls_liar_auth as A  # noqa: E402

# The cases where the two clients differ on accept or refuse, each on purpose, with why.
EXPECTED = {
    "client auth: no signature_algorithms at all: empty Certificate":
        "RFC 8446 §4.3.2 requires signature_algorithms in a CertificateRequest; OpenSSL refuses the request, this client "
        "treats it as one it cannot satisfy and answers with the empty Certificate",
    "ALPN: an unoffered selection (spdy/3)": "RFC 7301 §3.1: the server's choice is one of the client's; OpenSSL 3.0 takes any name",
    "ALPN: a selection that is a prefix of an offered name":
        "the same: OpenSSL does not compare the server's choice with the client's offer",
    "ALPN: a selection of an offered name's length and other bytes (h3 for h2)": "the same",
    "ALPN: a 255-byte selection never offered": "the same",
    "TLS 1.2 ALPN: an unoffered selection": "the same",
    "client auth: a CertificateRequest in a resumed handshake":
        "not run under OpenSSL: it needs the session OpenSSL saved from the first connection's ticket",
}
SKIPPED = {"client auth: a CertificateRequest in a resumed handshake"}


class Any:
    """Equal to anything: what the client says about itself has no counterpart under OpenSSL."""

    def __eq__(self, other):
        return True

    def __getitem__(self, item):
        return self

    def __iter__(self):
        return iter((self, self))


A.PLACEMENT = False  # OpenSSL sends no signature_algorithms_cert here
A.AuthServer.strict = A.AuthServer12.strict = False
A.AuthServer.query = lambda self: Any()
A.AuthServer12.query = lambda self: Any()
for _cls, _step in ((A.AuthServer, "client_flight"), (A.AuthServer12, "client_flight")):
    setattr(_cls, _step, D.completed(_cls, _step))


class AuthSClient(D.SClient):
    """`tls_differential.SClient` that takes the driver's `L` (the ALPN offer) and `I` (the identity) lines."""

    def __init__(self):
        super().__init__()
        self.alpn = None
        self.cert = None
        self.key = None

    def ask(self, line):
        if line[0] == "L":
            self.alpn = bytes.fromhex(line.split()[1]).decode().replace(" ", ",")
            return ["0", "ok", "0", "-", "-"]
        if line[0] == "I":
            f = line.split()
            self.cert, self.key = os.path.join(self.dir.name, "client.pem"), os.path.join(self.dir.name, "client.key")
            open(self.cert, "wb").write(bytes.fromhex(f[2]))
            open(self.key, "wb").write(bytes.fromhex(f[3]))
            return ["0", "ok", "0", "-", "-"]
        return super().ask(line)

    def _start(self, resume=False):
        port = self.listener.getsockname()[1]
        host = L.HOST.decode()
        extra = []
        if self.alpn:
            extra += ["-alpn", self.alpn]
        if self.cert:
            extra += ["-cert", self.cert, "-key", self.key]
        self.proc = subprocess.Popen(
            ["openssl", "s_client", "-connect", f"127.0.0.1:{port}", "-servername", host, "-CAfile", self.ca,
             "-verify_return_error", "-verify_hostname", host, "-attime", str(L.NOW), "-quiet", *extra, *D.OFFER],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        threading.Thread(target=self._stdout, daemon=True).start()
        threading.Thread(target=self._stderr, daemon=True).start()
        self.listener.settimeout(10)
        self.sock, _ = self.listener.accept()


def openssl_outcome(name, fn, server_class):
    D.SClient, original = AuthSClient, D.SClient
    try:
        return D.openssl_outcome(name, fn, server_class)
    finally:
        D.SClient = original


def main():
    wanted = sys.argv[1:]
    differ = alerts = agree = known = 0
    for name, tag, alert, encrypted, fn, server_class, config in A.CASES:
        if config or name in SKIPPED or (wanted and not any(w in name for w in wanted)):
            continue
        A.LAST_SENT = None
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
        if A.LAST_SENT:
            note += f" | openssl sent the {A.LAST_SENT} Certificate"
        if verdict == "known":
            note += f" | {EXPECTED[name]}"
        print(f"{verdict:6} | {name} | packages/tls: {show(ours)} ({tag}) | openssl: {show(theirs)}{note}", flush=True)
    print(f"differential: {agree} agree, {alerts} differ in the alert only, {known} differ as documented, "
          f"{differ} differ otherwise")
    sys.exit(1 if differ else 0)


if __name__ == "__main__":
    main()
