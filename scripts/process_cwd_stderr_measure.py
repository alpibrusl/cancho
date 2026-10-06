#!/usr/bin/env python3
"""Measurements behind the spawn working directory and std.process.capture_both
(docs/processes.md 4.10 and 7.2). Run it on each kernel:

    python3 scripts/process_cwd_stderr_measure.py

Part A drives `posix_spawn_file_actions_addfchdir_np` through ctypes, with the
same flags `exec_spawn` uses. Part B measures what reading a child's standard
error beside its standard output costs and needs.
"""
import ctypes, os, select, signal, socket, stat, subprocess, sys, tempfile, time

DARWIN = sys.platform == "darwin"
libc = ctypes.CDLL(None, use_errno=True)
CLOEXEC_DEFAULT = 0x4000 if DARWIN else 0
SETSIGDEF, SETSIGMASK = 0x4, 0x8


def have(name):
    try:
        getattr(libc, name)
        return True
    except AttributeError:
        return False


def spawn(path, argv, actions, default_close=True):
    """posix_spawn with the exec_spawn flags; (error number, pid)."""
    attr = ctypes.create_string_buffer(512)
    libc.posix_spawnattr_init(attr)
    flags = SETSIGDEF | SETSIGMASK | (CLOEXEC_DEFAULT if default_close else 0)
    libc.posix_spawnattr_setflags(attr, ctypes.c_short(flags))
    c_argv = (ctypes.c_char_p * (len(argv) + 1))(*[a.encode() for a in argv], None)
    c_env = (ctypes.c_char_p * 1)(None)
    pid = ctypes.c_int(0)
    err = libc.posix_spawn(ctypes.byref(pid), path.encode(), actions, attr, c_argv, c_env)
    libc.posix_spawnattr_destroy(attr)
    return err, pid.value


def new_actions():
    a = ctypes.create_string_buffer(512)
    libc.posix_spawn_file_actions_init(a)
    return a


def run(path, argv, cwd_fd=None, chdir_last=False, closefrom=True, default_close=True):
    """Spawn with stdout on a socket pair; (spawn error, exit status, output)."""
    parent, child = socket.socketpair()
    a = new_actions()
    libc.posix_spawn_file_actions_addopen(a, 0, b"/dev/null", os.O_RDWR, 0)
    libc.posix_spawn_file_actions_adddup2(a, child.fileno(), 1)
    libc.posix_spawn_file_actions_addopen(a, 2, b"/dev/null", os.O_RDWR, 0)
    if cwd_fd is not None and not chdir_last:
        libc.posix_spawn_file_actions_addfchdir_np(a, cwd_fd)
    if closefrom and not DARWIN:
        libc.posix_spawn_file_actions_addclosefrom_np(a, 3)
    if cwd_fd is not None and chdir_last:
        libc.posix_spawn_file_actions_addfchdir_np(a, cwd_fd)
    err, pid = spawn(path, argv, a, default_close)
    libc.posix_spawn_file_actions_destroy(a)
    child.close()
    out = b""
    status = None
    if err == 0:
        while (d := parent.recv(4096)):
            out += d
        _, st = os.waitpid(pid, 0)
        status = os.waitstatus_to_exitcode(st)
    parent.close()
    return err, status, out.decode(errors="replace").strip()


def part_a():
    print("A. the working directory")
    print("addfchdir_np present:", have("posix_spawn_file_actions_addfchdir_np"))
    if not have("posix_spawn_file_actions_addfchdir_np"):
        return
    root = os.path.realpath(tempfile.mkdtemp())
    where = os.path.join(root, "work")
    os.mkdir(where)
    tool = os.path.join(where, "tool")
    with open(tool, "w") as f:
        f.write("#!/bin/sh\npwd\n")
    os.chmod(tool, 0o755)
    elsewhere = os.path.join(root, "elsewhere")
    os.mkdir(elsewhere)
    os.chdir(elsewhere)
    fd = os.open(where, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)

    r = run("/bin/pwd", ["/bin/pwd"], fd)
    print("child's pwd under the Dir:", r[2] == where, r)
    r = run("/bin/pwd", ["/bin/pwd"])
    print("without a Dir the child starts in the parent's:", r[2] == elsewhere, r)
    r = run("./tool", ["./tool"], fd)
    print("relative program path resolves against the Dir:", r)
    r = run("./tool", ["./tool"])
    print("relative program path without a Dir (parent's cwd has no tool):", r[0], os.strerror(r[0]) if r[0] else "")
    r = run("/bin/ls", ["/bin/ls", "/dev/fd" if DARWIN else "/proc/self/fd"], fd)
    base = run("/bin/ls", ["/bin/ls", "/dev/fd" if DARWIN else "/proc/self/fd"])
    print("descriptors the child holds (listing's own included), with the Dir / without:",
          sorted(r[2].split()), "/", sorted(base[2].split()), "equal:", r[2] == base[2])
    print("parent's cwd unchanged by the spawn:", os.getcwd() == elsewhere)

    if not DARWIN:
        r = run("/bin/pwd", ["/bin/pwd"], fd, chdir_last=True)
        print("chdir ordered after addclosefrom_np:", r[0], os.strerror(r[0]) if r[0] else r)
    else:
        r = run("/bin/pwd", ["/bin/pwd"], fd, chdir_last=True)
        print("chdir ordered last under CLOEXEC_DEFAULT:", r[0], os.strerror(r[0]) if r[0] else r)
        r = run("/bin/pwd", ["/bin/pwd"], fd, default_close=False)
        print("without CLOEXEC_DEFAULT:", r)

    plain = os.open(tool, os.O_RDONLY | os.O_CLOEXEC)
    r = run("/bin/pwd", ["/bin/pwd"], plain)
    print("a descriptor of a regular file:", r[0], os.strerror(r[0]) if r[0] else r)
    os.close(plain)
    r = run("/bin/pwd", ["/bin/pwd"], 999)
    print("a descriptor that is not open:", r[0], os.strerror(r[0]) if r[0] else r)
    locked = os.path.join(root, "locked")
    os.mkdir(locked)
    lfd = os.open(locked, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    os.chmod(locked, 0)
    r = run("/bin/pwd", ["/bin/pwd"], lfd)
    print("a directory without search permission (opened before chmod 0):",
          r[0] if r[0] else r)
    os.chmod(locked, 0o755)
    # A Dir whose directory has since been removed.
    gone = os.path.join(root, "gone")
    os.mkdir(gone)
    gfd = os.open(gone, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
    os.rmdir(gone)
    r = run("/bin/pwd", ["/bin/pwd"], gfd)
    print("a Dir whose directory was removed:", r[0], os.strerror(r[0]) if r[0] else r)
    os.chdir("/")
    subprocess.run(["rm", "-rf", root])


# ---- B ----

def pair():
    return socket.socketpair(socket.AF_UNIX, socket.SOCK_STREAM)


def capacity():
    a, b = pair(); a.setblocking(False); n = 0
    try:
        while True: n += a.send(b"x" * 4096)
    except BlockingIOError: pass
    return n


def two_channels(script, reader):
    """Run `sh -c script` with stdout and stderr on their own socket pairs and
    hand the parent ends to `reader`, under a 3 s alarm. -> reader's value, or
    None when it hung."""
    po, co = pair(); pe, ce = pair()
    p = subprocess.Popen(["/bin/sh", "-c", script], stdin=subprocess.DEVNULL,
                         stdout=co.fileno(), stderr=ce.fileno(), close_fds=True)
    co.close(); ce.close()
    def alarm(*_): raise TimeoutError
    signal.signal(signal.SIGALRM, alarm); signal.alarm(3)
    try:
        got = reader(po, pe)
    except TimeoutError:
        got = None
    signal.alarm(0); p.kill(); p.wait(); po.close(); pe.close()
    return got


def sequential(po, pe):
    a = 0
    while (d := po.recv(65536)): a += len(d)
    b = 0
    while (d := pe.recv(65536)): b += len(d)
    return a, b


def polling(po, pe):
    sel = select.poll() if not DARWIN else None
    got = {po.fileno(): 0, pe.fileno(): 0}
    live = {po.fileno(): po, pe.fileno(): pe}
    while live:
        ready, _, _ = select.select(list(live.values()), [], [], 3)
        for s in ready:
            d = s.recv(65536)
            if d: got[s.fileno()] += len(d)
            else: del live[s.fileno()]
    return got[po.fileno()], got[pe.fileno()]


def exit_then_drain(nbytes, rounds):
    """The child writes nbytes to each stream and exits. The parent reads both
    whenever readable; once the exit is reported it drains each until EAGAIN
    and stops, not waiting for end of stream. -> (rounds with both whole,
    rounds where both ends had already arrived)."""
    whole = ended_both = 0
    script = f"head -c {nbytes} /dev/zero; head -c {nbytes} /dev/zero >&2"
    for _ in range(rounds):
        po, co = pair(); pe, ce = pair()
        p = subprocess.Popen(["/bin/sh", "-c", script], stdin=subprocess.DEVNULL,
                             stdout=co.fileno(), stderr=ce.fileno(), close_fds=True)
        co.close(); ce.close()
        po.setblocking(False); pe.setblocking(False)
        got = [0, 0]; ended = [False, False]
        if DARWIN:
            kq = select.kqueue()
            kq.control([select.kevent(p.pid, select.KQ_FILTER_PROC, select.KQ_EV_ADD | select.KQ_EV_ONESHOT, select.KQ_NOTE_EXIT),
                        select.kevent(po.fileno(), select.KQ_FILTER_READ, select.KQ_EV_ADD),
                        select.kevent(pe.fileno(), select.KQ_FILTER_READ, select.KQ_EV_ADD)], 0)
        else:
            watch = os.pidfd_open(p.pid)
            ep = select.epoll(); ep.register(po.fileno(), select.EPOLLIN)
            ep.register(pe.fileno(), select.EPOLLIN); ep.register(watch, select.EPOLLIN)
        def pull():
            for i, s in enumerate((po, pe)):
                try:
                    while (d := s.recv(65536)): got[i] += len(d)
                    ended[i] = True
                except BlockingIOError: pass
        exited = False
        while not exited:
            evs = kq.control(None, 8, 5) if DARWIN else ep.poll(5)
            for e in evs:
                ident = e.ident if DARWIN else e[0]
                if DARWIN and ident == p.pid or (not DARWIN and ident == watch): exited = True
            pull()
        pull()  # the drain after the exit
        whole += got == [nbytes, nbytes]; ended_both += all(ended)
        p.wait(); po.close(); pe.close()
        if not DARWIN: os.close(watch)
    return whole, ended_both


def merged_order(rounds):
    """stdout and stderr on the same socket (2>&1): is the child's write order
    the order read? A child alternating 500 lines on each stream."""
    kept = 0
    script = 'i=0; while [ $i -lt 500 ]; do echo o$i; echo e$i >&2; i=$((i+1)); done'
    for _ in range(rounds):
        parent, child = pair()
        p = subprocess.Popen(["/bin/sh", "-c", script], stdin=subprocess.DEVNULL,
                             stdout=child.fileno(), stderr=child.fileno(), close_fds=True)
        child.close(); data = b""
        while (d := parent.recv(65536)): data += d
        p.wait(); parent.close()
        want = "".join(f"o{i}\ne{i}\n" for i in range(500)).encode()
        kept += data == want
    return kept


def part_b():
    print("B. standard error beside standard output")
    print("channel capacity per channel (bytes before EAGAIN):", capacity())
    for k in range(10, 22):
        n = 1 << k
        got = two_channels(f"head -c {n} /dev/zero >&2; echo done", sequential)
        if got is None:
            print(f"read stdout to its end, then stderr: a child writing {n >> 1} bytes of stderr finishes, {n} deadlocks"); break
    else:
        print("read stdout then stderr: no deadlock up to 2 MiB")
    n = 1 << 20
    got = two_channels(f"head -c {n} /dev/zero >&2; head -c {n} /dev/zero", polling)
    print(f"both on one poll set, 1 MiB each: stdout, stderr = {got}")
    for n in (1, 65536, 1 << 20):
        whole, ended = exit_then_drain(n, 200)
        print(f"exit then drain both, {n} bytes each: whole {whole}/200, both ends already there {ended}/200")
    print("merged 2>&1, 500 alternating lines, order kept:", merged_order(50), "/ 50")


print(sys.platform, os.uname().release)
part_a()
part_b()
