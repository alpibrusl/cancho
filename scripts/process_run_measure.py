#!/usr/bin/env python3
"""Measurements behind std.process.capture (docs/processes.md 7.1): channel
capacity, the write-all-then-read deadlock, the drain after an exit, and a
grandchild holding the output end. Run it on each kernel:

    python3 scripts/process_run_measure.py
"""
import os, select, signal, socket, subprocess, sys, time

DARWIN = sys.platform == "darwin"

def pair():
    a, b = socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)
    return a, b

def capacity():
    a, b = pair(); a.setblocking(False); n = 0
    try:
        while True: n += a.send(b"x" * 4096)
    except BlockingIOError: pass
    return n

def spawn(argv, stdin, stdout):
    return subprocess.Popen(argv, stdin=stdin, stdout=stdout, stderr=subprocess.DEVNULL, close_fds=True)

def naive(n):
    """Write all n bytes (blocking), then read: does it finish within 3 s?"""
    to_child, child_in = pair(); from_child, child_out = pair()
    p = spawn(["cat"], child_in.fileno(), child_out.fileno()); child_in.close(); child_out.close()
    def alarm(*_): raise TimeoutError
    signal.signal(signal.SIGALRM, alarm); signal.alarm(3)
    try:
        to_child.sendall(b"y" * n); to_child.close()
        got = 0
        while (d := from_child.recv(65536)): got += len(d)
        ok = got == n
    except TimeoutError:
        ok = False
    signal.alarm(0); p.kill(); p.wait(); from_child.close()
    return ok

def watcher(pid):
    if DARWIN:
        kq = select.kqueue()
        kq.control([select.kevent(pid, select.KQ_FILTER_PROC, select.KQ_EV_ADD | select.KQ_EV_ONESHOT, select.KQ_NOTE_EXIT)], 0)
        return kq
    return os.pidfd_open(pid)

def exit_then_drain(nbytes, rounds):
    """The child writes nbytes and exits. The parent reads whenever readable;
    once the exit is reported it drains until EAGAIN, and stops there --
    not waiting for end of stream. Count rounds where every byte arrived."""
    whole = 0; saw_end_at_exit = 0
    for _ in range(rounds):
        from_child, child_out = pair()
        p = spawn(["head", "-c", str(nbytes), "/dev/zero"], subprocess.DEVNULL, child_out.fileno()); child_out.close()
        from_child.setblocking(False); got = 0; ended = False
        w = watcher(p.pid)
        if DARWIN:
            kq = w; kq.control([select.kevent(from_child.fileno(), select.KQ_FILTER_READ, select.KQ_EV_ADD)], 0)
        else:
            ep = select.epoll(); ep.register(from_child.fileno(), select.EPOLLIN); ep.register(w, select.EPOLLIN)
        exited = False
        while not exited:
            evs = kq.control(None, 8, 5) if DARWIN else ep.poll(5)
            for e in evs:
                ident = e.ident if DARWIN else e[0]
                if ident == p.pid or ident == w and not DARWIN: exited = True
            try:
                while (d := from_child.recv(65536)): got += len(d)
                ended = True
            except BlockingIOError: pass
        try:  # the drain after the exit
            while (d := from_child.recv(65536)): got += len(d)
            ended = True
        except BlockingIOError: pass
        whole += got == nbytes; saw_end_at_exit += ended
        p.wait(); from_child.close()
        if not DARWIN: os.close(w)
    return whole, saw_end_at_exit

def grandchild():
    """sh exits at once; a background sleep holds the output end for 2 s."""
    from_child, child_out = pair()
    t = time.monotonic()
    p = spawn(["/bin/sh", "-c", "sleep 2 & echo hi"], subprocess.DEVNULL, child_out.fileno()); child_out.close()
    p.wait(); exited = time.monotonic() - t
    while from_child.recv(65536): pass
    return exited, time.monotonic() - t

print(sys.platform, os.uname().release)
print("channel capacity (bytes before EAGAIN):", capacity())
for k in range(10, 22):
    n = 1 << k
    if not naive(n):
        print(f"write-all-then-read against cat: {n >> 1} finishes, {n} deadlocks"); break
else:
    print("write-all-then-read: no deadlock up to 2 MiB")
for n in (1, 65536, 1 << 20):
    whole, ended = exit_then_drain(n, 200)
    print(f"exit then drain, {n} bytes: whole {whole}/200, end of stream already there {ended}/200")
e, eof = grandchild()
print(f"grandchild holds the end: child exited after {e:.2f}s, end of stream after {eof:.2f}s")
