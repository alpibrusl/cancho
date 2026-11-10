# thread_state.sh -- docs/unsafe-and-global-state.md section 5 (#593, #594): the process-wide tables the socket builtins keep, held against real
# threads. A generated program runs two threads at the same moment (moved from crates/cancho/tests/conformance/thread_state.rs, deleted; docs/gate.md,
# G4 wave 3). A race is only visible if the threads arrive together, so each round waits for the same millisecond on the clock in both and then acts;
# the number of rounds is what turns a window of a few instructions into a measurement. One backend (LLVM, through the compiler this is run with).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
. tests/support/sk.sh
sk_build ticket_race tests/programs/ticket_race.cho
sk_build udp_ring_race tests/programs/udp_ring_race.cho
sk_build sk_holder tests/programs/sk_holder.cho
sk_build sk_udp_client tests/programs/sk_udp_client.cho

echo "== a ticket redeemed by two threads at once is given out once"
# #593. Measured before the fix on x86-64 (4 cores): 4 to 35 of 300 rounds, on every code generator (a load, a compare and a store let both threads
# through); after it, none. The driver dials a server that accepts every connection and keeps it (tests/programs/sk_holder.cho), and prints
# `rounds N doubled D`. Twice, so one unlucky schedule cannot hide a window the first run missed.
sk_start "$SK_SCRATCH/sk_holder"
for run in 1 2; do
    "$SK_SCRATCH/ticket_race" "$port" 400
done
sk_stop

echo "== two threads receiving at once are answered to their own peers"
# #594. The driver (tests/programs/udp_ring_race.cho) runs two server threads that receive and echo; two clients (tests/programs/sk_udp_client.cho)
# send at the same moment each round and expect their own bytes back. Measured before the fix (x86-64): 300 and 269 of 600 replies went to the other
# client in two of three runs on Cranelift, and 346 on LLVM in one (both server threads had read the same ticket from the shared counter before
# either datagram arrived, so both entries were one entry); after it, none.
"$SK_SCRATCH/udp_ring_race" 300 > "$SK_SCRATCH/ring.out" &
ring=$!
tries=0
while ! grep -q '^ports [0-9][0-9]* [0-9][0-9]*$' "$SK_SCRATCH/ring.out" && [ $tries -lt 100 ]; do
    sleep 0.1
    tries=$((tries + 1))
done
pa=$(sed -n 's/^ports \([0-9]*\) .*/\1/p' "$SK_SCRATCH/ring.out")
pb=$(sed -n 's/^ports [0-9]* //p' "$SK_SCRATCH/ring.out")
begin=$(( $(date +%s) * 1000 + 3000 ))
"$SK_SCRATCH/sk_udp_client" A "$pa" 300 $begin > "$SK_SCRATCH/clientA.out" &
a=$!
"$SK_SCRATCH/sk_udp_client" B "$pb" 300 $begin > "$SK_SCRATCH/clientB.out" &
b=$!
wait $a $b
# The driver ends by itself after 300 datagrams on each socket; one that lost any would wait for the rest for ever.
tries=0
while kill -0 $ring 2>/dev/null && [ $tries -lt 50 ]; do
    sleep 0.1
    tries=$((tries + 1))
done
kill -9 $ring 2>/dev/null
wait $ring 2>/dev/null
cat "$SK_SCRATCH/clientA.out" "$SK_SCRATCH/clientB.out"
sed -n '/^refused/p' "$SK_SCRATCH/ring.out"
