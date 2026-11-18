# udp.sh -- docs/udp.md: datagram sockets over real sockets, programs of edition 5 that declare no `extern fn` and hold no `Ffi`: a connected socket, a bound socket and its
# tickets, `std.udps`, the sender a ticket names. Moved from crates/cancho/tests/conformance/udp.rs (21 tests, deleted; docs/gate.md, G4 wave 4). The programs are the
# sections of tests/programs/udp_programs.txt (`#### NAME`; tests/support/net.sh `make_prog` replaces PORT, BOUND, OTHER and EXPECT and adds a main); the peers and the
# clients are modes of the cancho program tests/programs/sk_udp.cho (`upper`, `empty`, `pong`, `big`, `stranger`; `ask`, `two`, `peerbytes`). The checks are
# `begin NAME` ... `end` of tests/commands/script_lib.sh. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# One backend (LLVM, through the compiler this is run with).
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/support/net.sh
T=tests/programs/udp_programs.txt

build_exe sk_udp tests/programs/sk_udp.cho || exit 1
build_exe sk_holder tests/programs/sk_holder.cho || exit 1
udp=$work/sk_udp

# runs NAME: the program run to its end, 20 seconds at most (a receive that never returns must fail the check, not hang the suite): $status, 124 when it was killed.
runs() {
    "$work/$1" > "$work/$1.out" 2> "$work/$1.err" &
    _p=$!
    _i=0
    while kill -0 $_p 2> /dev/null && [ $_i -lt 400 ]; do sleep 0.05; _i=$((_i + 1)); done
    if kill -0 $_p 2> /dev/null; then
        kill -9 $_p 2> /dev/null
        wait $_p 2> /dev/null
        status=124
    else
        wait $_p
        status=$?
    fi
}
# peer MODE PORT [ARG]: a peer of sk_udp in the background ($pid), waited for until it listens.
udp_peer() {
    "$udp" "$@" > "$work/udp-peer.out" 2> "$work/udp-peer.err" &
    pid=$!
    waitfile "$work/udp-peer.err" listening 100
}
udp_peer_end() { kill -9 $pid 2> /dev/null; wait $pid 2> /dev/null; }
# serve NAME: a server program started in the background that says `ready` on standard error, waited for; $pid.
serve() {
    : > "$work/$1.err"
    "$work/$1" > "$work/$1.out" 2> "$work/$1.err" &
    pid=$!
    waitfile "$work/$1.err" ready 100
}
# serve_end: the server program ends by itself (20 seconds at most, 60 for $1): $sstatus.
serve_end() {
    _i=0
    while kill -0 $pid 2> /dev/null && [ $_i -lt $((${1:-20} * 20)) ]; do sleep 0.05; _i=$((_i + 1)); done
    if kill -0 $pid 2> /dev/null; then
        kill -9 $pid 2> /dev/null
        wait $pid 2> /dev/null
        sstatus=124
    else
        wait $pid
        sstatus=$?
    fi
}
# against MODE ARG TEMPLATE [EXPECT]: the client program TEMPLATE (a dialling one) against a peer of that mode on a port of its own; the program's exit status in $status.
against() { # NAME MODE ARG [EXPECT [KIND]]
    pick_port
    make_prog "${5:-dial}" $T "$1" "$port" "127.0.0.1:$port" 0 "${4:-0}" || return 1
    udp_peer "$2" "$port" "$3" || return 1
    runs "$1"
    udp_peer_end
}
authority_of() { "$cancho" authority "$work/$1.cho" --std --output json > "$work/authority.json" 2>&1; }

begin a_connected_socket_exchanges_a_datagram
against echo upper 1
same "$status" 0 "the connected socket and its datagram"
end

begin an_empty_datagram_is_got_zero
against empty empty 1
same "$status" 0 "an empty datagram is Got(0)"
end

begin an_oversize_datagram_is_truncated_and_says_so
# Linux reports the real length; Darwin has no such flag, so there it reports the buffer's.
expect=100
[ "$(uname -s)" = Darwin ] && expect=16
against truncated big 1 "$expect"
same "$status" 0 "a 100-byte datagram into a 16-byte buffer is Truncated($expect)"
end

begin a_datagram_from_a_stranger_never_reaches_a_connected_socket
against stranger stranger 1
same "$status" 0 "the stranger's evil is dropped; the peer's good is what udp_recv reads"
end

begin a_nonblocking_receive_with_nothing_waiting_is_again_and_an_empty_buffer_is_einval
against again upper 1
same "$status" 0 "a non-blocking receive with nothing waiting is Again"
end

begin each_socket_gets_its_own_kernel_chosen_port
against ports upper 1
same "$status" 0 "two sockets, two ports above 1023"
end

begin a_poller_reports_a_datagram_socket_readable
against polled pong 1
same "$status" 0 "the poller reports the datagram socket readable"
end

begin connecting_outside_the_bound_traps
for which in host port; do
    pick_port
    make_prog dial $T "outside_$which" "$port" "127.0.0.1:$port"
    traps "$work/outside_$which" || fail "$which outside the bound must end the process by a signal"
done
end

begin a_datagram_client_reports_its_host_and_no_ffi
make_src dial $T echo 9 127.0.0.1:9
authority_of echo
grep -qF '{ "name": "net_out", "argument": "127.0.0.1:9", "bounded": true }' "$work/authority.json" || fail "the bound does not survive the move to handles: $(cat "$work/authority.json")"
grep -qF '"udp_send"' "$work/authority.json" && grep -qF '"udp_recv"' "$work/authority.json" || fail "udp_send and udp_recv are reported"
grep -qF '"ffi"' "$work/authority.json" && fail "no foreign code anywhere"
grep -qF '"net_in"' "$work/authority.json" && fail "a client needs no inbound authority"
end

begin a_bound_socket_answers_the_sender_it_heard_from
pick_port
make_prog io $T serve_echo "$port"
serve serve_echo
got=$("$udp" ask "$port" hello-udp 10000 0)
serve_end
same "$got" "reply: hello-udp" "the bound socket answered its sender through the ticket"
same "$sstatus" 0 "the server"
end

begin a_reply_can_wait_for_a_later_receive_and_still_reach_its_sender
pick_port
make_prog io $T serve_two "$port"
serve serve_two
got=$("$udp" two "$port")
serve_end
same "$got" "first: to-first
second: to-second" "two senders heard, answered in the opposite order, each gets its own answer"
same "$sstatus" 0 "the server"
end

begin a_forged_ticket_sends_nothing
pick_port
make_prog io $T forged "$port"
serve forged
got=$("$udp" ask "$port" hi 10000 300)
serve_end
same "$got" "reply: ok
extra: no" "only the real ticket sends: the client sees exactly one datagram"
same "$sstatus" 0 "the server (five refused sends)"
end

begin a_ticket_is_refused_by_a_socket_that_did_not_hear_the_sender
pick_port; port_a=$port
pick_port; port_b=$port
make_prog open $T wrong_socket "$port_a" "" "$port_b"
serve wrong_socket
got=$("$udp" ask "$port_a" hi 10000 0)
serve_end 60
same "$got" "reply: ok" "the second socket refused the ticket and the first answered"
same "$sstatus" 0 "the server"
end

begin a_ticket_older_than_the_ring_is_refused
# The program feeds itself 65,536 datagrams through a connected socket, so no client can drop any; the late reply must not arrive.
pick_port; port_a=$port
pick_port; port_b=$port
make_prog open $T stale "$port_a" "" "$port_b"
serve stale
got=$("$udp" ask "$port_a" first 3000 0)
serve_end 60
same "$got" "reply: none" "a stale ticket sent a datagram"
same "$sstatus" 0 "the server"
end

begin binding_outside_the_bound_traps
pick_port
make_prog io $T bind_outside "$port"
traps "$work/bind_outside" || fail "a port outside the bound must end the process by a signal"
end

begin a_datagram_server_reports_its_port_and_no_net_out
make_src io $T serve_echo 9
authority_of serve_echo
grep -qF '{ "name": "net_in", "argument": "9", "bounded": true }' "$work/authority.json" || fail "the port does not survive the move to handles: $(cat "$work/authority.json")"
grep -qF '"udp_send"' "$work/authority.json" && grep -qF '"udp_recv"' "$work/authority.json" || fail "udp_send and udp_recv are reported"
grep -qF '"net_out"' "$work/authority.json" && fail "a bound socket needs no outbound authority"
grep -qF '"ffi"' "$work/authority.json" && fail "no foreign code anywhere"
end

begin an_empty_ticket_slice_is_einval_and_a_failed_receive_leaves_no_ticket
pick_port
make_prog io $T no_cell "$port"
serve no_cell
serve_end
same "$sstatus" 0 "an empty ticket slice is EINVAL and the cell is still the 7 it was given"
end

begin a_datagram_ticket_redeems_once_and_a_forged_one_reaches_nothing
against tickets upper 1
same "$status" 0 "forged tickets refused (nine of them), the real one redeems once and the socket still works"
end

begin a_connections_ticket_is_not_a_datagram_sockets
# The same port number for a TCP listener and a UDP socket, so one narrowing serves both: a TCP listener that keeps what it accepts.
pick_port
"$work/sk_holder" "$port" > /dev/null 2>&1 &
holder=$!
sleep 0.4
make_prog dial $T tcp_ticket "$port" "127.0.0.1:$port"
runs tcp_ticket
kill -9 $holder 2> /dev/null; wait $holder 2> /dev/null
same "$status" 0 "a Conn's ticket is refused by udp_attach, is not spent by the refusal, and redeems as the Conn it was"
end

begin a_socket_table_finds_each_socket_by_slot
against udps upper 3 0 raw
same "$status" 0 "three sockets by slot, a closed slot refusing stale use and being reused"
end

begin a_ticket_names_its_sender_and_only_on_the_socket_that_heard_it
pick_port; port_a=$port
pick_port; port_b=$port
make_prog open $T udp_peer "$port_a" "" "$port_b"
serve udp_peer
got=$("$udp" peerbytes "$port_a")
serve_end
same "$sstatus" 0 "the server (the refusals and the 19 bytes)"
mine=$(printf '%s\n' "$got" | sed -n 's/^local port: //p')
want="bytes: 4 127 0 0 1 0 0 0 0 0 0 0 0 0 0 0 0 $((mine / 256)) $((mine % 256))"
same "$(printf '%s\n' "$got" | sed -n '1p')" "$want" "the sender's address, in the 19 bytes conn_peer uses"
end
