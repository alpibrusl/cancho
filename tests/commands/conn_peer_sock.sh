# conn_peer_sock.sh -- what was left of `conformance/conn_peer.rs` after tests/commands/conn_peer.sh (docs/conn-peer.md, docs/gate.md G4 wave 6): a connection that ends before the program asks, with and without a reset, and a dual-stack
# IPv6 socket put under the program's listener. Both needed a socket the test made (`SO_LINGER` with a zero timeout on the client's; an AF_INET6 socket under the listener), which the Rust test did with `setsockopt` and `pre_exec`; the
# program tests/programs/conn_peer_sock.tmpl now does it itself, with the `Ffi("libc")` it holds for the purpose, in one process (the client's connect completes in the listener's backlog), so there is nothing to race and no second program.
# The number of the error that a reset connection's peer answers is the kernel's (`EINVAL`, 22, on macOS; `ENOTCONN`, 107, on Linux: docs/conn-peer.md section 2), so the check names it for the system it runs on and the transcript does not.
# The client's own port, which the Rust test took from `TcpStream::local_addr`, is not known to the shell: the check is that the port is there and that two connections from one address have different ones.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift legs of the Rust tests went with D2.
cancho=$1
work=$2
. tests/commands/script_lib.sh

P=tests/programs
case $(uname -s) in
    Darwin) plat=darwin; want_errno=22 ;;
    *) plat=linux; want_errno=107 ;;
esac

# run_case MODE: the program of conn_peer_sock.tmpl for MODE (reset, close or v6), built for a port that was chosen at random and run; its transcript is in $work/sock.out. A port that is taken (the program says
# `listen failed` or `setup failed`) is another try. Returns 1, after saying why, if no try gave a transcript.
run_case() {
    n=0
    while [ "$n" -lt 8 ]; do
        n=$((n + 1))
        port=$(awk -v a="$(date +%s)" -v b="$$" -v c="$n" 'BEGIN { srand(a + b * 7 + c * 1013); print 20000 + int(rand() * 30000) }')
        sed "s/PORT/$port/g" "$P/conn_peer_sock.tmpl" > "$work/sock.cho"
        if ! "$cancho" build "$work/sock.cho" "$P/conn_peer_connect.cho" --std -o "$work/sock" > "$work/build-sock.txt" 2>&1; then
            fail "building said: $(head -c 400 "$work/build-sock.txt")"
            return 1
        fi
        "$work/sock" "$plat" "$1" > "$work/sock.out" 2> "$work/sock.err"
        st=$?
        if ! grep -q 'listen failed\|setup failed' "$work/sock.out"; then
            same "$st" 0 "the program's status"
            return 0
        fi
    done
    fail "no port could be used in 8 tries: $(head -c 200 "$work/sock.out")"
    return 1
}

# shape FILE: the transcript with the client's port and the error number made into words.
shape() { sed 's/\(peer [^ ]*\):[0-9][0-9]* /\1:PORT /; s/none [0-9][0-9]*/none ERRNO/' "$1"; }
# port_of FILE N: the client's port in the line of slot N.
port_of() { sed -n "s/^slot $2 peer [^ ]*:\([0-9][0-9]*\) .*/\1/p" "$1"; }

begin "a connection that closed normally still has a peer, address and port"
if run_case close; then
    same "$(shape "$work/sock.out")" "read end
slot 0 peer 127.0.0.1:PORT key 4 2130706433" "the transcript"
    [ "$(port_of "$work/sock.out" 0)" -gt 0 ] 2> /dev/null || fail "the client's port is missing"
fi
end

begin "a connection that the client reset has none, and the kernel says why (ENOTCONN on Linux, EINVAL on macOS)"
if run_case reset; then
    same "$(shape "$work/sock.out")" "read failed
slot 0 none ERRNO" "the transcript"
    same "$(sed -n 's/^slot 0 none //p' "$work/sock.out")" "$want_errno" "the error number"
fi
end

begin "an IPv6 peer is read, and an IPv4 one on the same dual-stack socket is the IPv4 address"
if run_case v6; then
    same "$(shape "$work/sock.out")" "slot 0 peer [::1]:PORT key 6 0
slot 1 peer 127.0.0.1:PORT key 4 2130706433
slot 2 peer [::1]:PORT key 6 0" "the transcript"
    a=$(port_of "$work/sock.out" 0)
    b=$(port_of "$work/sock.out" 2)
    [ -n "$a" ] && [ -n "$b" ] && [ "$a" != "$b" ] || fail "two connections from ::1 should have two ports, got '$a' and '$b'"
fi
end
