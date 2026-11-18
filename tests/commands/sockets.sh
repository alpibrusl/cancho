# sockets.sh -- docs/native-sockets.md: the socket handles over real sockets, programs of edition 5 that declare no `extern fn` and hold no `Ffi`; the poller, the clocks, the
# tickets and the connection table. Moved from crates/cancho/tests/conformance/sockets.rs (27 tests, deleted; docs/gate.md, G4 wave 4). The programs are the sections of
# tests/programs/sockets_programs.txt (`#### NAME`, PORT, BOUND and OTHER replaced, a main added by tests/support/net.sh `make_prog`); their peers are the scripted client
# (tests/programs/sk_client.cho, as a client and, with `serve`, as a server), `sk_holder` and `sk_full`. A server that says `ready` on standard error is waited for in the
# file it said it in; a step the Rust test took after a line the program said is a `wait` of the script that the shell releases (`go`) when it has seen the line.
# The checks are `begin NAME` ... `end` of tests/commands/script_lib.sh. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# One backend (LLVM, through the compiler this is run with).
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/support/net.sh
T=tests/programs/sockets_programs.txt

build_exe sk_client tests/programs/sk_client.cho || exit 1
build_exe sk_holder tests/programs/sk_holder.cho || exit 1
build_exe sk_full tests/programs/sk_full.cho || exit 1
sync_open

# serve NAME: the program started in the background, its standard output in $work/NAME.out and its standard error in $work/NAME.err (pid in $pid).
serve() {
    : > "$work/$1.err"
    "$work/$1" > "$work/$1.out" 2> "$work/$1.err" &
    pid=$!
}
# runs NAME: the program run to its end: its exit status in $status.
runs() {
    "$work/$1" > "$work/$1.out" 2> "$work/$1.err"
    status=$?
}
# shown TEXT: a line of the transcript in $got is TEXT.
shown() { printf '%s\n' "$got" | grep -qxF -- "$1"; }
# The authority report of the program NAME (as made by make_src) in $work/authority.json.
authority_of() { "$cancho" authority "$work/$1.cho" --std --output json > "$work/authority.json" 2>&1; }

begin a_listener_echoes_a_real_client_on_both_backends
pick_port
make_prog raw $T echo "$port"
serve echo
got=$(sk "open 1
send 1 ping
readn 1 4 5000
shows
send 1 \\sand\\spong
readn 1 9 5000
shows
close 1")
await
same "$sstatus" 0 "the server should see End and finish"
shown 'ping' || fail "the connection should echo what was sent: $got"
shown ' and pong' || fail "the connection should echo the second write: $got"
end

begin non_blocking_accept_and_read_answer_again
pick_port
make_prog io $T nonblocking "$port"
serve nonblocking
waitfile "$work/nonblocking.err" ready
sk_bg "open 1
wait
send 1 hi
readn 1 2 5000
shows
close 1"
waitfile "$work/nonblocking.err" idle
go
sk_end
await
shown hi || fail "the echo: $got"
same "$sstatus" 0 "both Agains and the echo should happen"
end

begin a_write_the_kernel_has_no_room_for_answers_again
pick_port
make_prog io $T fill "$port"
serve fill
waitfile "$work/fill.err" ready
# Connect and do not read: the server's sends pile up until the kernel says no, then it ends and the client reads everything.
sk_bg "open 1
wait
readend 1 30000
sums"
waitfile "$work/fill.err" full
go
sk_end
await
same "$sstatus" 0 "Again, not Failed"
wrote=$(tr -d ' \n' < "$work/fill.out")
[ "${wrote:-0}" -gt 0 ] || fail "something was queued before the buffer filled"
want=$(awk -v n="${wrote:-0}" 'BEGIN { c = 0; for (i = 0; i < n; i++) c = (c * 131 + 120) % 1000000007; print "read n=" n " c=" c }')
shown "$want" || fail "everything queued is delivered, intact and nothing else (wrote ${wrote:-0}; $(printf '%s\n' "$got" | grep '^read n=')"
end

begin a_write_to_a_peer_that_has_gone_fails_instead_of_killing_the_process
pick_port
make_prog io $T gone "$port"
serve gone
sk "open 1
close 1" > /dev/null
await
same "$sstatus" 0 "the process must exit, not be killed by a signal"
end

begin listening_on_a_port_that_is_taken_answers_failed
# A live listener on the wildcard address (what tcp_listen binds; BSD lets a wildcard bind sit beside one on 127.0.0.1).
pick_port
"$work/sk_holder" "$port" > /dev/null 2>&1 &
holder=$!
sleep 0.4
make_prog io $T port_taken "$port"
runs port_taken
kill -9 $holder 2> /dev/null; wait $holder 2> /dev/null
same "$status" 0 "tcp_listen on a taken port: $(head -c 200 "$work/port_taken.err")"
end

begin listening_on_a_port_outside_the_bound_traps
pick_port
make_prog io $T wrong_port "$port"
traps "$work/wrong_port" || fail "a port outside the bound must end the process by a signal"
end

begin reading_into_an_empty_buffer_is_refused_not_an_end_of_stream
pick_port
make_prog io $T empty_buffer "$port"
serve empty_buffer
sk "open 1
sleep 500
close 1" > /dev/null
await
same "$sstatus" 0 "the read into an empty buffer"
end

begin the_reuse_port_flag_lets_two_listeners_share_a_port
pick_port
make_prog io $T reuse_port "$port"
runs reuse_port
same "$status" 0 "two listeners with the flag, one without"
end

begin a_socket_program_reports_its_port_and_no_ffi
make_src raw $T echo 8080
authority_of echo
grep -qF '{ "name": "net_in", "argument": "8080", "bounded": true }' "$work/authority.json" || fail "the port does not survive the move to handles: $(cat "$work/authority.json")"
for label in conn_accept conn_read conn_write; do
    grep -qF "{ \"name\": \"$label\", \"argument\": null, \"bounded\": true }" "$work/authority.json" || fail "$label is path-free and bounded"
done
grep -qF '"ffi"' "$work/authority.json" && fail "no foreign code anywhere"
end

begin a_dialled_connection_exchanges_bytes_with_a_real_server
pick_port; dial=$port
make_prog dial $T dial "$dial" "127.0.0.1:$dial"
peer_on "$dial" "accept 1
readn 1 4 5000
shows
send 1 PING
sleep 200
close 1"
runs dial
same "$status" 0 "the program: $(head -c 200 "$work/dial.err")"
peer_end > "$work/peer.said"
grep -qxF ping "$work/peer.said" || fail "the server received what was written: $(cat "$work/peer.said")"
end

begin dialling_a_closed_port_answers_failed_with_an_errno
pick_port
make_prog dial $T refused "$port" "127.0.0.1:$port"
runs refused
same "$status" 0 "a refused dial"
end

begin an_unresolvable_name_is_failed_minus_one
make_prog dial $T unresolved 0 "no-such-host.invalid:80"
runs unresolved
same "$status" 0 "an unresolvable name"
end

begin dialling_outside_the_bound_traps
for which in host port; do
    pick_port
    make_prog dial $T "outside_$which" "$port" "127.0.0.1:$port"
    traps "$work/outside_$which" || fail "$which outside the bound must end the process by a signal"
done
end

begin a_client_reports_its_host_and_no_ffi
make_src dial $T dial 9 127.0.0.1:9
authority_of dial
grep -qF '{ "name": "net_out", "argument": "127.0.0.1:9", "bounded": true }' "$work/authority.json" || fail "the bound does not survive the move to handles"
grep -qF '"conn_write"' "$work/authority.json" && grep -qF '"conn_read"' "$work/authority.json" || fail "conn_write and conn_read are reported"
grep -qF '"ffi"' "$work/authority.json" && fail "no foreign code anywhere"
end

begin a_started_connection_works_once_it_is_writable
pick_port; dial=$port
make_prog dial $T dial_start "$dial" "127.0.0.1:$dial"
peer_on "$dial" "accept 1
readn 1 4 5000
shows
send 1 PING
sleep 200
close 1"
runs dial_start
same "$status" 0 "the program: $(head -c 200 "$work/dial_start.err")"
peer_end > /dev/null
end

begin a_started_connection_to_a_closed_port_reports_the_errno
pick_port
make_prog dial $T start_refused "$port" "127.0.0.1:$port"
runs start_refused
same "$status" 0 "a refused started connection"
end

begin a_started_connection_does_not_wait_for_a_peer_that_never_answers
# A listener whose accept queue is full: the kernel drops the SYN and a blocking connect waits for as long as it retries.
pick_port
: > "$work/sk_full.err"
"$work/sk_full" "$port" > /dev/null 2> "$work/sk_full.err" &
full=$!
if waitfile "$work/sk_full.err" full 800; then
    make_prog dial $T start_full "$port" "127.0.0.1:$port"
    started=$(date +%s)
    runs start_full
    elapsed=$(( $(date +%s) - started ))
    same "$status" 0 "the program: $(head -c 200 "$work/start_full.err")"
    [ "$elapsed" -lt 10 ] || fail "took $elapsed seconds: it blocked"
fi
kill -9 $full 2> /dev/null; wait $full 2> /dev/null
end

begin a_started_connection_reports_the_same_authority
make_src dial $T dial_start 9 127.0.0.1:9
authority_of dial_start
grep -qF '{ "name": "net_out", "argument": "127.0.0.1:9", "bounded": true }' "$work/authority.json" || fail "the report does not name the host and port"
grep -qF '"ffi"' "$work/authority.json" && fail "no foreign code anywhere"
end

begin a_poller_reports_readiness_by_token
pick_port
make_prog io $T poller "$port"
serve poller
waitfile "$work/poller.err" ready
sk_bg "open 1
send 1 hello
wait
send 1 more
wait
close 1"
waitfile "$work/poller.err" removed
go
waitfile "$work/poller.err" writable
go
sk_end
await
same "$sstatus" 0 "all seven checks (otherwise 100 plus how many passed)"
end

begin the_clock_measures_a_wait_in_milliseconds
make_prog raw $T clock
runs clock
same "$status" 0 "a 1.1 s wait on an empty poller is seen as 1000 to 3000 milliseconds"
end

begin a_program_that_reads_the_clock_reports_it
make_src raw $T clock
authority_of clock
grep -qF '{ "name": "clock", "argument": null, "bounded": true }' "$work/authority.json" || fail "clock is a path-free label"
grep -qF '"poll"' "$work/authority.json" || fail "the poller is reported too"
end

begin the_wall_clock_reads_unix_milliseconds
make_prog raw $T wall_clock
before=$(date +%s)
runs wall_clock
after=$(date +%s)
same "$status" 0 "the wall clock program"
printed=$(tr -d ' \n' < "$work/wall_clock.out")
case $printed in
    '' | *[!0-9]*) fail "a number, got '$printed'" ;;
    *) [ $((printed / 1000)) -ge "$before" ] && [ $((printed / 1000)) -le "$after" ] || fail "$printed is not between the seconds $before and $after" ;;
esac
end

begin reading_the_wall_clock_reports_clock
make_src raw $T wall_clock
authority_of wall_clock
grep -qF '{ "name": "clock", "argument": null, "bounded": true }' "$work/authority.json" || fail "the wall clock is the clock capability"
end

begin a_forged_ticket_reaches_nothing
make_prog raw $T forged
runs forged
same "$status" 0 "forged tickets"
end

begin a_connection_table_finds_each_client_by_slot
pick_port
make_prog raw $T tickets "$port"
serve tickets
waitfile "$work/tickets.err" ready
sk_bg "open 1
send 1 A
open 2
send 2 B
readend 1 10000
shows
readn 2 2 10000
shows
wait
open 3
send 3 C
readend 2 10000
sums
readend 3 10000
sums"
waitfile "$work/tickets.err" freed
go
sk_end
await
same "$sstatus" 0 "the server's score"
shown 'a!' || fail "slot 0's client got its own answer, then the end: $got"
shown 'b!' || fail "slot 1's client got its own answer: $got"
same "$(printf '%s\n' "$got" | grep -c '^read n=0 c=0$')" 2 "ending the table closed slot 1 and the reused slot (nothing more to read)"
same "$(printf '%s\n' "$got" | grep -c '^readend closed: yes$')" 3 "the connections that ended"
end

begin a_ticket_is_spent_when_it_is_redeemed
pick_port
"$work/sk_holder" "$port" > /dev/null 2>&1 &
holder=$!
sleep 0.4
make_prog dial $T spent "$port" "127.0.0.1:$port"
runs spent
kill -9 $holder 2> /dev/null; wait $holder 2> /dev/null
same "$status" 0 "a ticket redeemed once"
end

begin conn_nodelay_turns_the_option_on_for_an_accepted_connection
pick_port
make_prog io $T nodelay "$port"
if command -v strace > /dev/null 2>&1; then
    : > "$work/nodelay.err"
    strace -f -e trace=setsockopt -o "$work/trace.txt" "$work/nodelay" > /dev/null 2> "$work/nodelay.err" &
    pid=$!
else
    serve nodelay
fi
waitfile "$work/nodelay.err" ready
sk "open 1
send 1 x
readn 1 1 5000
shows
close 1" > /dev/null
await
same "$sstatus" 0 "the call should answer 0"
if [ -f "$work/trace.txt" ]; then
    same "$(grep TCP_NODELAY "$work/trace.txt" | grep -c '= 0')" 1 "one successful TCP_NODELAY"
fi
end
