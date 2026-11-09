# thread_state.py -- docs/unsafe-and-global-state.md section 5 (#593, #594): the process-wide tables the socket builtins keep, held against real
# threads. A generated program that runs two threads at the same moment. Moved from crates/cancho/tests/conformance/thread_state.rs
# (docs/gate.md, G4 wave 3). A race is only visible if the threads arrive together, so each round waits for the same millisecond on the clock
# in both and then acts; the number of rounds is what turns a window of a few instructions into a measurement.
#     python3 tests/commands/thread_state.py CANCHO SCRATCH     (from the root of the repository)
import socket
import subprocess
import sys
import threading

sys.path.insert(0, "tests/support")
from sk import build, check, run, same

cancho, scratch = sys.argv[1], sys.argv[2]


def holder():
    """A server that accepts every connection and keeps it, until stopped: (port, stop)."""
    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(64)
    port = listener.getsockname()[1]
    held = []
    done = threading.Event()

    def accept():
        while True:
            try:
                conn, _ = listener.accept()
            except OSError:
                return
            if done.is_set():
                conn.close()
                return
            held.append(conn)

    threading.Thread(target=accept, daemon=True).start()

    def stop():
        done.set()
        try:
            socket.create_connection(("127.0.0.1", port), timeout=2).close()
        except OSError:
            pass
        listener.close()
        for c in held:
            c.close()

    return port, stop


def a_ticket_redeemed_by_two_threads_at_once_is_given_out_once():
    # #593. Measured before the fix on x86-64 (4 cores): 4 to 35 of 300 rounds on Cranelift, on LLVM through the Rust compiler and on LLVM
    # through the compiler in cancho (a load, a compare and a store let both threads through); after it, none.
    rounds = 400
    exe = build(cancho, scratch, "ticket_race", ["tests/programs/ticket_race.cho"])
    # Twice, so one unlucky schedule cannot hide a window the first run missed.
    for run_number in range(2):
        port, stop = holder()
        out = subprocess.run([exe, str(port), str(rounds)], capture_output=True, text=True)
        stop()
        same(out.returncode, 0, "the driver's status: " + out.stderr)
        words = out.stdout.split()
        check(len(words) == 4, "a `rounds N doubled D` line: %r" % out.stdout)
        same(int(words[1]), rounds, "run %d: every round was dialled" % run_number)
        same(int(words[3]), 0, "run %d: rounds that gave one ticket to two threads" % run_number)


def two_threads_receiving_at_once_are_answered_to_their_own_peers():
    # #594. The driver (tests/programs/udp_ring_race.cho) runs two server threads that receive and echo; two clients on threads of the test send
    # at the same moment each round and expect their own bytes back. Measured before the fix (x86-64): on Cranelift, 300 and 269 of 600 replies
    # went to the other client in two of three runs, and 346 on LLVM in one (both server threads had read the same ticket from the shared counter
    # before either datagram arrived, so both entries were one entry); after it, none.
    rounds = 300
    exe = build(cancho, scratch, "udp_ring_race", ["tests/programs/udp_ring_race.cho"])
    server = subprocess.Popen([exe, str(rounds)], stdout=subprocess.PIPE, text=True)
    line = server.stdout.readline()
    ports = [int(p) for p in line.split()[1:]]
    same(len(ports), 2, "a `ports P Q` line: %r" % line)
    barrier = threading.Barrier(2)
    results = []

    def client(name, port):
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.bind(("127.0.0.1", 0))
        sock.settimeout(2)
        wrong = lost = 0
        for n in range(rounds):
            barrier.wait()
            message = ("%s%d" % (name, n)).encode()
            sock.sendto(message, ("127.0.0.1", port))
            try:
                got, _ = sock.recvfrom(64)
                if got != message:
                    wrong += 1
            except OSError:
                lost += 1
        results.append((wrong, lost))

    threads = [threading.Thread(target=client, args=(name, port)) for name, port in zip("AB", ports)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    rest = server.stdout.read()
    same(server.wait(), 0, "the driver's status")
    wrong = sum(w for w, _ in results)
    lost = sum(l for _, l in results)
    check(wrong == 0 and lost == 0 and rest.strip() == "refused 0", "%d replies went to the wrong peer, %d were lost, the server said %r" % (wrong, lost, rest))


run([
    ("a ticket redeemed by two threads at once is given out once", a_ticket_redeemed_by_two_threads_at_once_is_given_out_once),
    ("two threads receiving at once are answered to their own peers", two_threads_receiving_at_once_are_answered_to_their_own_peers),
])
