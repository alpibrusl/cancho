#!/usr/bin/env python3
"""The branches an ordinary run on loopback never reaches: a write the kernel takes in pieces, a write that waits, a TLS record that
arrives in pieces, a read with nothing yet (`shim_io.c`, LD_PRELOAD on `send` and `recv`).

    python3 partial_io_test.py       # exits 0 only if every connection still succeeds, the bytes intact

64 connections at once, three 60,000-byte POSTs each over one session, against the TLS receiver, which reads exactly Content-Length bytes
and answers 200: a lost or reordered byte is a MAC failure (a handshake or record error), not a quiet pass. The shim makes `send` take
700 bytes at most and `recv` return 300 at most, and every third call of either fail with EAGAIN.
"""
import os, subprocess, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

def main():
    binary = h.build()
    so = os.path.join(h.WORK, "shim_io.so")
    subprocess.run(["cc", "-O2", "-shared", "-fPIC", "-o", so, os.path.join(h.HERE, "shim_io.c"), "-ldl"], check=True)
    bad = 0
    def check(label, ok, detail=""):
        nonlocal bad
        print("  %-4s %s %s" % ("ok" if ok else "FAIL", label, detail))
        bad += 0 if ok else 1
    shim = {"LD_PRELOAD": so, "SHIM_SEND_MAX": "700", "SHIM_RECV_MAX": "300", "SHIM_EAGAIN_EVERY": "3"}
    with h.Server("good_ec", workers=2) as s:
        r = h.run_client(binary, s.port, total=64, conc=64, reqs=3, body=60000, resp_len=s.resp_len)
        check("control, no shim: 64 x 3 x 60,000 bytes", r["outcomes"] == {(0, 0, 200): 64}, str(r["outcomes"]))
        r = h.run_client(binary, s.port, total=64, conc=64, reqs=3, body=60000, resp_len=s.resp_len, extra_env=shim)
        check("partial and refused I/O: 64 x 3 x 60,000 bytes", r["outcomes"] == {(0, 0, 200): 64} and r["exit"] == 0, str(r["outcomes"]))
        r = h.run_client(binary, s.port, total=500, conc=64, reqs=1, body=100, resp_len=s.resp_len, extra_env=shim)
        check("partial and refused I/O: 500 handshakes", r["outcomes"] == {(0, 0, 200): 500}, str(r["outcomes"]))
        r = h.run_client(binary, s.port, total=64, conc=64, reqs=2, body=60000, resp_len=s.resp_len, extra_env=dict(shim, SHIM_EAGAIN_EVERY="2", SHIM_SEND_MAX="64", SHIM_RECV_MAX="17"))
        check("harsher: sends of 64 bytes, reads of 17, every second call refused", r["outcomes"] == {(0, 0, 200): 64}, str(r["outcomes"]))
    # A peer that says nothing: the handshake loop must go back to the poller (and so to the deadline), not spin waiting for input.
    import socket, threading
    ls = socket.socket(); ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); ls.bind(("127.0.0.1", 0)); ls.listen(256)
    held = []
    def tarpit():
        while True:
            try:
                c, _ = ls.accept(); held.append(c)
            except OSError:
                return
    threading.Thread(target=tarpit, daemon=True).start()
    for label, env in (("", None), (" under the shim", shim)):
        r = h.run_client(binary, ls.getsockname()[1], total=64, conc=64, reqs=1, deadline_ms=1000, extra_env=env, timeout=20)
        check("64 connections to a peer that never answers end at the deadline%s" % label, r["outcomes"] == {(102, 0, -1): 64} and r["wall"] < 5, "(%.2f s, %s)" % (r["wall"], r["outcomes"]))
    ls.close()
    print("FAILED: %d" % bad if bad else "all connections intact")
    return 1 if bad else 0

sys.exit(main())
