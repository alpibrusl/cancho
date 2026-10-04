#!/usr/bin/env python3
"""Nothing may leak a connection: descriptors, memory and OpenSSL objects, on the success path and on every failure path.

    python3 leak_test.py          # exits 0 only if every check holds

 * descriptors: the client runs under `ulimit -n 200` and makes 10,000 connections through 64 slots; one leaked descriptor per
   connection would stop it at connection 136. Success, an expired certificate (handshake failure), and a server that accepts and
   never answers (deadline) are each run, on both I/O modes;
 * memory: the bytes malloc still has outstanding when the client exits (an `LD_PRELOAD` shim, `shim_heap.c`) after 2,000 connections and
   after 40,000 differ by less than 8 KiB (a leak of one byte a connection would be 38 KB), on both transports, success and failure;
 * OpenSSL objects: valgrind --leak-check=full on 40 connections through 16 slots, success and failure, reports nothing.
"""
import os, re, shutil, socket, subprocess, sys, threading, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import harness as h

def limited(binary, port, n, **kw):
    return h.run_client(binary, port, total=n, conc=64, reqs=1, resp_len=kw.pop("resp_len", 40), prefix=["prlimit", "--nofile=200"], **kw)

def main():
    binary = h.build()
    bad = 0
    def check(label, ok, detail=""):
        nonlocal bad
        print("  %-4s %s %s" % ("ok" if ok else "FAIL", label, detail))
        bad += 0 if ok else 1
    ls = socket.socket(); ls.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); ls.bind(("127.0.0.1", 0)); ls.listen(1024)
    held = []
    def tarpit():
        while True:
            try:
                c, _ = ls.accept(); held.append(c)
                if len(held) > 150:
                    held.pop(0).close()
            except OSError:
                return
    threading.Thread(target=tarpit, daemon=True).start()
    with h.Server("good_ec", workers=2) as good, h.Server("expired", workers=1) as expired:
        for io in (0, 1):
            for label, port, kw, want in (("success", good.port, {}, {(0, 0, 200): 10000}),
                                          ("expired certificate", expired.port, {}, {(2, 10, -1): 10000})):
                r = limited(binary, port, 10000, io=io, resp_len=good.resp_len, sigpipe=True, **kw)
                check("%s, io=%d, 10,000 connections under `ulimit -n 200`" % (label, io), r["outcomes"] == want and r["exit"] == 0, str(r["outcomes"]))
            r = limited(binary, ls.getsockname()[1], 300, io=io, deadline_ms=200, sigpipe=True)
            check("deadline (server never answers), io=%d, 300 connections under `ulimit -n 200`" % io, r["outcomes"] == {(102, 0, -1): 300} and r["exit"] == 0, str(r["outcomes"]))
        # What malloc still has outstanding when the client exits (shim_heap.c, `mallinfo2`), after 2,000 and after 40,000 connections: the
        # same number to the byte if everything was freed. (Peak resident size is not used: it grows by the client's own 40-byte record of
        # each connection and moves with fragmentation, and it is a poor detector.)
        subprocess.run(["cc", "-O2", "-shared", "-fPIC", "-o", os.path.join(h.WORK, "shim_heap.so"), os.path.join(h.HERE, "shim_heap.c")], check=True)
        preload = {"LD_PRELOAD": os.path.join(h.WORK, "shim_heap.so")}
        def heap(port, n, io, resp_len):
            r = h.run_client(binary, port, total=n, conc=64, reqs=1, resp_len=resp_len, io=io, sigpipe=True, extra_env=preload)
            m = re.search(r"heap_in_use=(\d+)", r["stderr"])
            return int(m.group(1)) if m else None
        for label, port, io in (("success", good.port, 0), ("expired certificate", expired.port, 0), ("success", good.port, 1), ("expired certificate", expired.port, 1)):
            a, b = heap(port, 10000, io, good.resp_len), heap(port, 40000, io, good.resp_len)
            # The number rises over the first few thousand connections (to 244,992 bytes in every configuration: something in OpenSSL fills
            # up to a bound, found by running 500, 2,000, 5,000, 10,000 and 20,000) and then stays; a leak of one byte a connection would put
            # 10,000 and 40,000 30,000 bytes apart.
            check("heap in use at exit, %s, io=%d: %s bytes after 10,000 connections, %s after 40,000" % (label, io, a, b), a is not None and b is not None and abs(b - a) < 8192)
    ls.close()
    if shutil.which("valgrind"):
        with h.Server("good_ec", workers=1) as good, h.Server("expired", workers=1) as expired:
            for label, port in (("success", good.port), ("expired", expired.port)):
                C = h.ensure_certs()
                p = subprocess.run(["valgrind", "--leak-check=full", "--errors-for-leak-kinds=definite,indirect,possible", "--error-exitcode=9", "-q", binary,
                                    "127.0.0.1", str(port), "hooks.test", "40", "16", "2", "1", C + "/ca.pem", "0", "100", str(good.resp_len), "0", "0", "0"],
                                   capture_output=True, text=True, timeout=300)
                check("valgrind --leak-check=full, %s" % label, p.returncode == 0 and "lost" not in p.stderr, "(exit %d)" % p.returncode)
    else:
        print("  skip valgrind is not installed")
    print("FAILED: %d" % bad if bad else "no leak found")
    return 1 if bad else 0

sys.exit(main())
