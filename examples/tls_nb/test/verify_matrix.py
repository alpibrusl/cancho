#!/usr/bin/env python3
"""Certificate verification: every outcome the client can report, each against a real TLS server, on both I/O modes.

    python3 verify_matrix.py          # exits 0 only if every row came out as expected

A row is (what the server presents, host name the client expects, trust store the client is given) and the outcome the
client must report as (stage, detail, status): stage 0 detail 0 is success, stage 2 is the handshake and its detail is the
OpenSSL X509_V_ERR_* number (verification) or an OpenSSL error code. The control rows turn verification off or fix the name,
so that a failure above them is shown to be the check and not the setup.
"""
import os, subprocess, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

OK = (0, 0, 200)

def fail(v):
    return (2, v, -1)

# (label, server cert, chain file, client host, client cafile, verify, default_paths, expected)
ROWS = [
    ("valid certificate",                         "good_ec",    None,                "hooks.test", "ca.pem",      1, 0, OK),
    ("valid certificate, RSA 2048",               "good_rsa",   None,                "hooks.test", "ca.pem",      1, 0, OK),
    ("wrong host name (cert is for other.test)",  "wronghost",  None,                "hooks.test", "ca.pem",      1, 0, fail(62)),
    ("  control: the client expects other.test",  "wronghost",  None,                "other.test", "ca.pem",      1, 0, OK),
    ("expired (2020-01-02)",                      "expired",    None,                "hooks.test", "ca.pem",      1, 0, fail(10)),
    ("not yet valid (2090)",                      "notyet",     None,                "hooks.test", "ca.pem",      1, 0, fail(9)),
    ("self-signed, not trusted",                  "selfsigned", None,                "hooks.test", "ca.pem",      1, 0, fail(18)),
    ("  control: verification off",               "selfsigned", None,                "hooks.test", "-",           0, 0, OK),
    ("untrusted chain (leaf, int, root sent)",    "otherleaf",  "otherpki_chain.pem","hooks.test", "ca.pem",      1, 0, fail(19)),
    ("intermediate not sent",                     "intleaf",    "noint_chain.pem",   "hooks.test", "ca.pem",      1, 0, fail(20)),
    ("  control: intermediate sent",              "intleaf",    "withint_chain.pem", "hooks.test", "ca.pem",      1, 0, OK),
    ("CA file holds the wrong CA",                "good_ec",    None,                "hooks.test", "wrongca.pem", 1, 0, fail(20)),
    ("no trust store at all",                     "good_ec",    None,                "hooks.test", "-",           1, 0, fail(20)),
    ("system store only (no CA of ours in it)",   "good_ec",    None,                "hooks.test", "-",           1, 1, fail(20)),
]

def main():
    binary = h.build()
    bad = 0
    for io in (0, 1):
        print("I/O mode %d (%s)" % (io, "memory BIOs" if io == 0 else "SSL_set_fd"))
        for label, cert, chain, host, ca, verify, dp, want in ROWS:
            with h.Server(cert, chain=chain) as s:
                r = h.run_client(binary, s.port, host=host, cafile=ca, verify=verify, default_paths=dp, resp_len=s.resp_len, io=io)
            got = list(r["outcomes"].items())
            ok = got == [(want, 1)] and r["exit"] == 0
            bad += 0 if ok else 1
            print("  %-4s %-44s -> stage=%d detail=%d status=%d%s" % ("ok" if ok else "FAIL", label, *(got[0][0] if got else (-1, -1, -1)),
                  "" if ok else "   expected %s" % (want,)))
    # An alert from the server is a different code from "the peer closed": a server that speaks only TLS 1.1.
    C = h.ensure_certs()
    port = h.free_port()
    srv = subprocess.Popen(["openssl", "s_server", "-accept", str(port), "-cert", C + "/good_ec.pem", "-key", C + "/good_ec.key",
                            "-max_protocol", "TLSv1.1", "-min_protocol", "TLSv1", "-cipher", "DEFAULT:@SECLEVEL=0", "-quiet"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1)
    for io in (0, 1):
        r = h.run_client(binary, port, io=io)
        (stage, detail, status), = r["outcomes"].keys()
        # 0x0A00042E: library 20 (SSL), reason 1000 + 70: the alert `protocol_version` received from the peer.
        ok = stage == 2 and detail == 0x0A00042E
        bad += 0 if ok else 1
        print("  %-4s server offers only TLS 1.1 (mode %d)                   -> stage=%d detail=%d (0x%X)" % ("ok" if ok else "FAIL", io, stage, detail, detail))
    srv.terminate()
    # A server that only closes: the client says the peer closed (-1), a different code again.
    with h.Server("good_ec", tls_min="1.1", tls_max="1.1", ciphers="DEFAULT:@SECLEVEL=0") as s:
        r = h.run_client(binary, s.port, resp_len=s.resp_len)
        (stage, detail, status), = r["outcomes"].keys()
        ok = (stage, detail) == (2, -1)
        bad += 0 if ok else 1
        print("  %-4s server closes without an alert                        -> stage=%d detail=%d" % ("ok" if ok else "FAIL", stage, detail))
    # The trust store from the environment: the system store's own variable names a file, so default paths can be given a CA.
    with h.Server("good_ec") as s:
        r = h.run_client(binary, s.port, cafile="-", verify=1, default_paths=1, resp_len=s.resp_len,
                         extra_env={"SSL_CERT_FILE": C + "/ca.pem"})
        ok = list(r["outcomes"].items()) == [((0, 0, 200), 1)]
        bad += 0 if ok else 1
        print("  %-4s system store (SSL_CTX_set_default_verify_paths) honours SSL_CERT_FILE -> %s" % ("ok" if ok else "FAIL", r["outcomes"]))
    # A peer that closes before the handshake: the memory-BIO transport reports it; OpenSSL's socket BIO (io=1) writes the fatal alert
    # itself with write(2) and a closed peer kills the process with SIGPIPE unless the process ignores it.
    import socket, threading
    ls = socket.socket(); ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); ls.bind(("127.0.0.1", 0)); ls.listen(256)
    def closer():
        while True:
            try:
                c, _ = ls.accept(); c.close()
            except OSError:
                return
    threading.Thread(target=closer, daemon=True).start()
    cport = ls.getsockname()[1]
    # (a closed peer is seen as "closed" (-1) or, if the reset arrived before the read, as ECONNRESET (104): both are stage 2)
    for io, sigpipe, label, want_exit, killed in ((0, False, "memory BIOs", 0, False), (1, False, "SSL_set_fd, SIGPIPE default", -13, True),
                                                  (1, True, "SSL_set_fd, sigpipe=ignore", 0, False)):
        r = h.run_client(binary, cport, total=20, conc=4, reqs=1, io=io, sigpipe=sigpipe)
        ok = r["exit"] == want_exit and (killed or (sum(r["outcomes"].values()) == 20 and all(k[0] == 2 and k[1] in (-1, 104) for k in r["outcomes"])))
        bad += 0 if ok else 1
        print("  %-4s peer closes before the handshake, %-28s -> exit %d %s" % ("ok" if ok else "FAIL", label, r["exit"], r["outcomes"]))
    ls.close()
    # Nothing listening: a connect failure, not a TLS one.
    r = h.run_client(binary, h.free_port())
    (stage, detail, status), = r["outcomes"].keys()
    ok = stage == 1 and detail == 111
    bad += 0 if ok else 1
    print("  %-4s nothing listening                                     -> stage=%d detail=%d (ECONNREFUSED)" % ("ok" if ok else "FAIL", stage, detail))
    print("FAILED: %d" % bad if bad else "all rows as expected")
    return 1 if bad else 0

sys.exit(main())
