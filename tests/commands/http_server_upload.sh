# http_server_upload.sh -- `packages/http-server`'s streaming request bodies on the socket path (docs/http-server.md section 12), moved from
# crates/cancho/tests/conformance/http_server_upload.rs (deleted; docs/gate.md, G4 wave 3). The program tests/programs/server_upload.cho serves
# uploads over real sockets, so `step`, `settle`, `wait` and the timers are exercised as well as the byte-fed path
# tests/packages/http_server_body_test.cho covers without any. The server is built from the package's source (HTTP_SERVER_SOURCE names another
# file, as scripts/http_server_body_mutants.py does). The client is the cancho program tests/programs/sk_client.cho; every `n=.. c=..` the server
# answers is the length and checksum of what it read, and the line the client printed as it sent is above it in the transcript.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
. tests/support/sk.sh
sk_setup
sk_build server_upload tests/programs/server_upload.cho "${HTTP_SERVER_SOURCE:-packages/http-server/server.cho}"
upload="$SK_SCRATCH/server_upload"
POST='POST /upload HTTP/1.1\r\nHost: t\r\n'
REFUSE='POST /refuse HTTP/1.1\r\nHost: t\r\n'
WHOLE='GET /whole HTTP/1.1\r\nHost: t\r\n\r\n'

echo "== uploads arrive byte for byte, with 100-continue, pipelining and keep-alive"
sk_start "$upload" 4096 67108864 2000 2000 0
# A length body of 3 MiB, the client waiting for `100 Continue`: the server answers it only because the application proceeds.
# Then chunked, with extensions and a trailer, in pieces the buffer (4096) does not line up with; then head, body and two more requests in one write.
sk_client "open 1
send 1 ${POST}Content-Length: 3145728\r\nExpect: 100-continue\r\n\r\n
head 1
gen 1 3145728 1 65536 0
resp 1
send 1 ${POST}Transfer-Encoding: chunked\r\n\r\n
chunked 1 1048576 2 777 ;k=v X-T: 1\r\n
resp 1
put ${POST}Content-Length: 100000\r\n\r\n
putgen 100000 3
put GET /whole HTTP/1.1\r\nHost: t\r\n\r\nPOST /whole HTTP/1.1\r\nHost: t\r\nContent-Length: 5\r\n\r\nhello
flush 1
resp 1
resp 1
resp 1
send 1 ${POST}Content-Length: 0\r\n\r\n
resp 1"
echo "== four uploads at the same time on four connections, each its own bytes"
clients=
for i in 0 1 2 3; do
    n=$((2097152 + i * 1001))
    sk_client "open 1
send 1 ${POST}Content-Length: $n\r\n\r\n
gen 1 $n $((10 + i)) 7919 0
resp 1" > "$SK_SCRATCH/four$i.out" &
    clients="$clients $!"
done
wait $clients
cat "$SK_SCRATCH"/four[0-3].out
echo "== sizes around the buffer"
for n in 1 4095 4096 4097 8192 65535 65536 65537; do
    sk_client "open 1
send 1 ${POST}Content-Length: $n\r\n\r\n
gen 1 $n $n 65536 0
resp 1"
done
sk_stop

echo "== a refusal before the body sends no 100, and the faults are refused with their rules"
sk_start "$upload" 4096 1048576 2000 2000 0
# Refused on the head: no `100`, and the client never sent the body, so the connection goes on. The same without `Expect`, the client sending the
# body anyway: refused, and the connection closes. A length over the server's maximum (1 MiB) is refused by the server, with its rule, before a
# `100`. Another expectation: 417.
sk_client "open 1
send 1 ${REFUSE}Content-Length: 500000\r\nExpect: 100-continue\r\n\r\n
resp 1
send 1 $WHOLE
resp 1
open 2
send 2 ${REFUSE}Content-Length: 1048576\r\n\r\n
rep 2 5000 x
head 2
ends 2 5000
open 3
send 3 ${POST}Content-Length: 1048577\r\nExpect: 100-continue\r\n\r\n
resp 3
ends 3 5000
open 4
send 4 ${POST}Content-Length: 10\r\nExpect: gimme\r\n\r\n
resp 4"
# Bad chunking in the middle of an upload: 400 with the rule, and the server serves the next client.
for wire in 'zz\r\n' '5\r\nhelloXX' '0\r\nT: v\nU\r\n\r\n' 'fffffff\r\n'; do
    sk_client "open 1
send 1 ${POST}Transfer-Encoding: chunked\r\n\r\n
send 1 $wire
resp 1
ends 1 5000"
done
# Answered before the body is read, and the client still sending: the answer comes, and the connection closes.
sk_client "open 1
send 1 POST /early HTTP/1.1\r\nHost: t\r\nContent-Length: 1048576\r\n\r\n
rep 1 10000 y
resp 1
ends 1 5000
open 2
send 2 $WHOLE
resp 2"
sk_stop

echo "== a client that stalls in a head or a body is timed out, and the others are served"
sk_start "$upload" 4096 1048576 600 600 0
# A head that never ends (slot 1), one that is trickled (slot 2) and a body that stops (slot 3), all at once; ten other clients are each served
# in under 400 ms while the three stall; the three are then timed out with their rules, not before 600 ms and not after five seconds.
script="stamp 1
open 1
send 1 POST /upload HTTP/1.1\r\nHo
open 2
open 3
send 3 ${POST}Content-Length: 100000\r\n\r\n
rep 3 1000 z"
for c in G E T / t H T T P /; do
    script="$script
send 2 $c
stamp
open 4
send 4 $WHOLE
resp 4
since lt 400
close 4
sleep 100"
done
script="$script
resp 1
ends 1 5000
resp 2
ends 2 5000
resp 3
ends 3 5000
since gt 600 1
since lt 5000 1"
sk_client "$script"
sk_stop

echo "== a body the application takes slowly stops being read, and a client that leaves costs nothing"
sk_start "$upload" 4096 1073741824 5000 5000 0
# `/slow` takes 50 bytes a turn of 20 ms: the kernel's buffers fill and the client's writes block; what the server buffers is 4096 bytes, so the
# client got no more than the sockets' own buffers in. The others are served meanwhile. The client leaves in the middle, twenty more do the same, and
# the server serves after.
sk_client "open 1
send 1 POST /slow HTTP/1.1\r\nHost: t\r\nContent-Length: 268435456\r\n\r\n
fill 1 200
open 2
send 2 $WHOLE
resp 2
close 1"
for i in 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19; do
    sk_client "open 1
send 1 ${POST}Content-Length: 1048576\r\n\r\n
gen 1 $((10000 + i)) $i 65536 0
close 1"
done
sk_client "open 1
send 1 ${POST}Content-Length: 5\r\n\r\nhello
resp 1"
sk_stop

echo "== a refusal found while reading is sent even when the application is slow to call next"
# The program visits connections every tenth turn, so a fault `wait` finds waits through turns in which the client sends more.
sk_start "$upload" 4096 1048576 2000 2000 1
for wire in 'zz\r\n' '5\r\nhelloXX'; do
    sk_client "open 1
send 1 ${POST}Transfer-Encoding: chunked\r\n\r\n
sleep 100
send 1 $wire
ticks 1 20 3 0123456789
resp 1
ends 1 5000"
done
sk_client "open 1
send 1 ${POST}Content-Length: 5\r\n\r\nhello
resp 1"
sk_stop

echo "== a connection that reads never writes into its neighbour's buffer"
# Connection 1 (slot 0 of the server) uploads in small writes; connection 2 is slow and has a full buffer of untaken bytes. Whatever the first one's
# reads do to the room `body_take` gave back, the second one's body arrives whole.
sk_start "$upload" 4096 67108864 5000 5000 0
sk_client "open 1
send 1 ${POST}Content-Length: 300000\r\n\r\n
sleep 100
open 2
send 2 POST /slow HTTP/1.1\r\nHost: t\r\nContent-Length: 6000\r\n\r\n
gen 2 6000 99 65536 0
sleep 500
gen 1 300000 7 1000 1
resp 1
resp 2"
sk_stop
