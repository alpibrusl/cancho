# http_server.sh -- packages/http-server (docs/http-server.md): the pipelined requests of one loop, the replies of any type, an extra header, and a held request
# (`hold`, `answer`, a ticket) with the handles an application registers in the server's poller (docs/http-server.md section 10). Moved from
# crates/cancho/tests/conformance/http_server.rs (10 tests, deleted; docs/gate.md, G4 wave 4). The first three talk to tests/programs/server_one_per_round.cho; the
# held ones to tests/programs/server_hold.cho, which dials a peer: the peer is the scripted client in its serving mode (tests/programs/sk_client.cho), so one
# script holds the clients and the release (a byte on the peer's connection, slot 9). `readn S N MS` reads N bytes and `shows` prints them with \r and \n
# escaped, so a reply is compared whole. The checks are `begin NAME` ... `end` of tests/commands/script_lib.sh. $1 is the compiler, $2 a scratch directory; run from
# the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/support/net.sh

build_exe sk_client tests/programs/sk_client.cho || exit 1
fetch_deps "$work/dep-round" examples/api/server.lock packages/http-server/.cancho-vcs
fetch_deps "$work/dep-hold" tests/programs/server_hold.lock packages/http-server/.cancho-vcs
build_exe one_per_round tests/programs/server_one_per_round.cho "$work"/dep-round/*.cho || exit 1
build_exe server_hold tests/programs/server_hold.cho "$work"/dep-hold/*.cho || exit 1

# A request as the scripted client writes it, with \r\n spelt out.
get() { printf 'GET %s HTTP/1.1\\r\\nHost: t\\r\\n\\r\\n' "$1"; }
# What the server answers a request with BODY: the wire (real bytes, for its length) and as `shows` prints it.
ok_raw() { printf 'HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %s\r\nConnection: keep-alive\r\n\r\n%s' "${#1}" "$1"; }
ok_len() { ok_raw "$1" | wc -c | tr -d ' '; }
ok_esc() { printf 'HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\nContent-Length: %s\\r\\nConnection: keep-alive\\r\\n\\r\\n%s' "${#1}" "$1"; }
# has TEXT: the transcript in $got has a line that is TEXT; shows N: the same for the N-th `shows` line.
has_line() { printf '%s\n' "$got" | grep -qxF -- "$1"; }

# one_per_round SCRIPT: the server that answers one request per round, started on a port; the scripted client runs SCRIPT against it, then the server is killed.
one_per_round() {
    spawn "$work/server.out" "$work/one_per_round" || return 1
    got=$(sk "$1")
    if kill -0 $pid 2> /dev/null; then alive=yes; else alive=no; fi
    kill -9 $pid 2> /dev/null
    wait $pid 2> /dev/null
}

# held SCRIPT [SIZE]: a peer (the scripted client serving on a port of its own) with SCRIPT, whose `open S` dials the server on the port it was told; then the server
# (which dials the peer first, and listens); the transcript in $got.
held() {
    _peer=$(od -An -N2 -tu2 /dev/urandom | tr -d ' ')
    _peer=$((20000 + _peer % 9000))
    _srv=$((_peer + 1 + $(od -An -N1 -tu1 /dev/urandom | tr -d ' ')))
    "$work/sk_client" "$_peer" "$1" serve "$_srv" > "$work/held.out" 2> "$work/held.err" &
    _client=$!
    _i=0
    while ! grep -q '^listening$' "$work/held.err" 2> /dev/null && [ $_i -lt 60 ]; do sleep 0.05; _i=$((_i + 1)); done
    "$work/server_hold" "$_srv" "$_peer" ${2:+"$2"} > /dev/null 2>&1 &
    pid=$!
    _i=0
    while kill -0 $_client 2> /dev/null && [ $_i -lt 800 ]; do sleep 0.05; _i=$((_i + 1)); done
    kill -9 $_client $pid 2> /dev/null
    wait $_client $pid 2> /dev/null
    got=$(cat "$work/held.out")
}

# The rows of a client's replies.
begin requests_a_caller_did_not_get_to_are_not_lost
# Four clients pipeline six requests each; the server takes one per round, and every request is answered, each connection in order.
script="open 1
open 2
open 3
open 4"
for i in 0 1 2 3; do
    n=$((i + 1))
    for j in 0 1 2 3 4 5; do script="$script
put $(get "/c${i}r$j")"; done
    script="$script
flush $n"
done
size=0
for j in 0 1 2 3 4 5; do size=$((size + $(ok_len "/c0r$j"))); done
for i in 0 1 2 3; do
    script="$script
readn $((i + 1)) $size 10000
shows"
done
one_per_round "$script"
for i in 0 1 2 3; do
    want=
    for j in 0 1 2 3 4 5; do want="$want$(ok_esc "/c${i}r$j")"; done
    has_line "$want" || fail "client $i did not get its six answers whole and in order"
done
same "$(printf '%s\n' "$got" | grep -c '^readn complete: yes$')" 4 "the clients that got all their bytes"
end

begin the_checked_in_store_still_resolves_and_holds_this_source
"$cancho" vcs resolve packages/http-server/.cancho-vcs > "$work/resolve.out" 2>&1 || fail "vcs resolve said: $(head -c 300 "$work/resolve.out")"
found=0
for published in packages/http-server/.cancho-vcs/sources/*; do
    cmp -s "$published" packages/http-server/server.cho && found=1
done
same "$found" 1 "packages/http-server/server.cho is not what the store published; re-run vcs publish --std"
end

begin replies_of_any_type_and_bodiless_ones_keep_the_connection_framed
want="$(ok_esc /a)HTTP/1.1 204 No Content\\r\\nConnection: keep-alive\\r\\n\\r\\nHTTP/1.1 200 OK\\r\\nContent-Type: text/plain; charset=utf-8\\r\\nContent-Length: 2\\r\\nConnection: keep-alive\\r\\nX-Test: 1\\r\\n\\r\\nhi$(ok_esc /b)"
raw_len=$(( $(ok_len /a) + $(printf 'HTTP/1.1 204 No Content\r\nConnection: keep-alive\r\n\r\n' | wc -c) + $(printf 'HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 2\r\nConnection: keep-alive\r\nX-Test: 1\r\n\r\nhi' | wc -c) + $(ok_len /b) ))
one_per_round "open 1
put $(get /a)$(get /empty)$(get /typed)$(get /b)
flush 1
readn 1 $raw_len 10000
shows"
has_line "$want" || fail "the four replies were not as wanted: $got"
end

begin an_extra_header_without_its_crlf_is_sent_and_a_refused_one_answers_500
failure='{"error":"method not allowed"}'
fixed='{"error":"internal error"}'
head500() { printf 'HTTP/1.1 500 Internal Server Error\\r\\nContent-Type: application/json\\r\\nContent-Length: %s\\r\\nConnection: keep-alive\\r\\n\\r\\n%s' "${#fixed}" "$fixed"; }
want="$(printf 'HTTP/1.1 405 Method Not Allowed\\r\\nContent-Type: application/json\\r\\nContent-Length: %s\\r\\nConnection: keep-alive\\r\\nAllow: GET\\r\\n\\r\\n%s' "${#failure}" "$failure")$(head500)$(head500)$(head500)$(ok_esc /after)"
raw=$(printf 'HTTP/1.1 405 Method Not Allowed\r\nContent-Type: application/json\r\nContent-Length: %s\r\nConnection: keep-alive\r\nAllow: GET\r\n\r\n%s' "${#failure}" "$failure" | wc -c)
raw500=$(printf 'HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: %s\r\nConnection: keep-alive\r\n\r\n%s' "${#fixed}" "$fixed" | wc -c)
total=$((raw + 3 * raw500 + $(ok_len /after)))
one_per_round "open 1
put $(get /bare)$(get /bad)$(get /badfailure)$(get /badempty)$(get /after)
flush 1
readn 1 $total 10000
shows"
has_line "$want" || fail "the five replies were not as wanted: $got"
same "$alive" yes "the server is still running after the refused headers"
end

begin a_held_request_does_not_stop_the_loop_and_what_follows_it_waits_its_turn
want_late="$(ok_esc released)$(ok_esc /after)$(ok_esc /late)"
len_late=$(( $(ok_len released) + $(ok_len /after) + $(ok_len /late) ))
held "accept 9
open 1
open 2
put $(get /hold)$(get /after)
flush 1
send 2 $(get /one)
readn 2 $(ok_len /one) 10000
shows
send 2 $(get /two)
readn 2 $(ok_len /two) 10000
shows
send 2 $(get /three)
readn 2 $(ok_len /three) 10000
shows
read 1 zzz 300
size lt 1
send 1 $(get /late)
read 1 zzz 300
size lt 1
send 9 x
readn 1 $len_late 10000
shows
send 1 $(get /again)
readn 1 $(ok_len /again) 10000
shows
send 9 x
sleep 200
send 1 $(get /stale)
readn 1 $(ok_len 0) 10000
shows"
for body in /one /two /three; do has_line "$(ok_esc $body)" || fail "client b was not answered $body while a request was held"; done
has_line "$want_late" || fail "the held request and what followed it were not answered in order after the release: $got"
has_line "$(ok_esc /again)" || fail "a later request on the same connection was not answered"
has_line "$(ok_esc 0)" || fail "a byte written with nothing held should wait for the next hold, not refuse an answer (stale count 0)"
same "$(printf '%s\n' "$got" | grep -c '^read size < 1: yes$')" 2 "the held connection stays silent until the release"
end

begin held_requests_are_released_one_each_and_oldest_first
held "accept 9
open 1
open 2
open 3
send 1 $(get /hold)
sleep 100
send 2 $(get /hold)
sleep 100
send 3 $(get /hold)
sleep 100
send 9 x
readn 1 $(ok_len released) 10000
shows
read 2 zzz 300
size lt 1
read 3 zzz 300
size lt 1
send 9 x
readn 2 $(ok_len released) 10000
shows
read 3 zzz 300
size lt 1
send 9 x
readn 3 $(ok_len released) 10000
shows"
same "$(printf '%s\n' "$got" | grep -cxF "$(ok_esc released)")" 3 "each held request is released once, in the order they came"
same "$(printf '%s\n' "$got" | grep -c '^read size < 1: yes$')" 3 "the later ones stay silent until their turn"
end

begin a_held_connection_with_a_full_buffer_waits_without_reading_and_loses_nothing
# One connection pipelines /hold and then 2,500 more requests (over three times the server's 16 KiB buffer); the server holds the first and does not read the rest;
# after the release everything is answered, in order, and nothing is lost.
script="accept 9
open 1
put $(get /hold)"
for i in $(seq 0 2499); do script="$script
put $(get "$(printf '/p%04d' "$i")")"; done
want_sum=$( { ok_raw released; for i in $(seq 0 2499); do ok_raw "$(printf '/p%04d' "$i")"; done; } | od -An -v -tu1 | awk 'BEGIN { c = 0; n = 0 } { for (i = 1; i <= NF; i++) { c = (c * 131 + $i) % 1000000007; n++ } } END { print " n=" n " c=" c }')
want_len=$(printf '%s' "$want_sum" | sed 's/^ n=\([0-9]*\) .*/\1/')
script="$script
flushpart 1
read 1 zzz 500
size lt 1
send 9 x
exchange 1 $want_len 20000
sums"
held "$script"
has_line "read size < 1: yes" || fail "the held connection should say nothing while the release is waited for: $(printf '%s' "$got" | head -c 300)"
has_line "exchange complete: yes" || fail "not every answer came after the release"
has_line "read$want_sum" || fail "the answers were not the released one and the 2,500 in order, whole: $(printf '%s\n' "$got" | tail -3)"
end

begin answering_more_than_the_table_has_rooms_in_one_round_keeps_every_connection_queued
# Two clients send twelve /instant requests each (held and answered at once, 64 MiB buffers): all twenty-four are answered, on their own connections.
want=
for i in 1 2 3 4 5 6 7 8 9 10 11 12; do want="$want$(ok_esc instant)"; done
len=$(( 12 * $(ok_len instant) ))
req=
for i in 1 2 3 4 5 6 7 8 9 10 11 12; do req="$req$(get /instant)"; done
held "accept 9
open 1
open 2
send 1 $req
send 2 $req
readn 1 $len 10000
shows
readn 2 $len 10000
shows" 67108864
same "$(printf '%s\n' "$got" | grep -cxF "$want")" 2 "both clients get twelve answers each, whole"
end

begin a_held_request_is_not_the_idle_timeouts_to_close
# The idle timeout is 9 seconds (the server's argument); a request held for 12 is still answered when released.
held "accept 9
open 1
send 1 $(get /hold)
sleep 12000
send 9 x
readn 1 $(ok_len released) 10000
shows"
has_line "$(ok_esc released)" || fail "a request held past the idle timeout was not answered: $got"
end

begin a_ticket_answers_once_and_never_reaches_a_connection_that_took_the_slot
held "accept 9
open 1
open 2
send 1 $(get /hold)
sleep 100
send 9 x
readn 1 $(ok_len released) 10000
shows
send 2 $(get /replay/0)
readn 2 $(ok_len 1) 10000
shows
read 1 zzz 300
size lt 1
close 1
sleep 300
open 3
send 3 $(get /hold)
sleep 100
send 2 $(get /replay/0)
readn 2 $(ok_len 2) 10000
shows
read 3 zzz 300
size lt 1
send 9 x
readn 3 $(ok_len released) 10000
shows"
has_line "$(ok_esc released)" || fail "the first held request was not released"
has_line "$(ok_esc 1)" || fail "a replayed ticket was not refused once (stale count 1)"
has_line "$(ok_esc 2)" || fail "the same ticket reached a connection that took the slot (stale count 2 wanted)"
same "$(printf '%s\n' "$got" | grep -cxF "$(ok_esc released)")" 2 "released answers: the first client and the one that took the slot"
same "$(printf '%s\n' "$got" | grep -c '^read size < 1: yes$')" 2 "no answer reached a connection it was not for"
end
