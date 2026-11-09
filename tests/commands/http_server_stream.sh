# http_server_stream.sh -- `packages/http-server`'s `stream` on a socket server (docs/http-server.md section 14), moved from
# crates/cancho/tests/conformance/http_server_stream.rs (deleted; docs/gate.md, G4 wave 3). The program tests/programs/server_stream.cho holds
# `GET /stream` and sends it one event every 100 ms, chunked, then ends it; `GET /flood` is offered 4,007-byte chunks until the connection's answer
# room will not take one whole. The server is built from the package's source (HTTP_SERVER_SOURCE names another file, as
# scripts/http_server_stream_mutants.py does); the store is checked in the last section. The client is the cancho program tests/programs/sk_client.cho.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
. tests/support/sk.sh
sk_setup
sk_build server_stream tests/programs/server_stream.cho "${HTTP_SERVER_SOURCE:-packages/http-server/server.cho}"
stream="$SK_SCRATCH/server_stream"
STREAM='GET /stream HTTP/1.1\r\nHost: t\r\n\r\n'
# One short request on a connection of its own, as slot $2 of a script: `GET path`, `Connection: close`, the answer printed.
get() { # PATH SLOT
    printf 'open %s\nsend %s GET %s HTTP/1.1\\r\\nHost: t\\r\\nConnection: close\\r\\n\\r\\n\nresp %s' "$2" "$2" "$1" "$2"
}

echo "== events arrive while the stream is open, and others are served"
sk_start "$stream"
# Another connection is answered while the stream is open; the five events are the whole stream, in reads spread over time (five events 100 ms
# apart); the ticket of the stream that is over takes nothing, and the connection is still good for the next request on it.
sk_client "open 1
send 1 $STREAM
stamp 0
sleep 150
$(get /plain 2)
read 1 0\\r\\n\\r\\n 10000
shows
spread 250 3
since lt 5000 0
$(get /stale 3)
$(get /spare 4)
send 1 GET /again HTTP/1.1\\r\\nHost: t\\r\\nConnection: close\\r\\n\\r\\n
readend 1 10000
has /again"
sk_stop

echo "== a client that leaves ends its stream, and the loop goes on"
sk_start "$stream"
# `stream` answered -1 for the ticket of the connection that left (once), and the loop answers others; a second stream still works afterwards.
sk_client "open 1
send 1 $STREAM
read 1 data:\\s1 5000
close 1
sleep 900
$(get /gone 2)
$(get /plain 3)
open 4
send 4 $STREAM
read 4 0\\r\\n\\r\\n 10000
shows"
sk_stop

echo "== a ticket of a connection that left does not reach the one that took its slot"
sk_start "$stream"
# Another stream, in the slot the first one left; while it runs, the old ticket is offered a byte.
sk_client "open 1
send 1 $STREAM
read 1 data:\\s1 5000
close 1
sleep 500
$(get /gone 2)
open 3
send 3 $STREAM
sleep 150
$(get /stale 4)
read 3 0\\r\\n\\r\\n 10000
shows"
sk_stop

echo "== a request refused while it streams is given no more bytes"
# The program streams to a request whose declared body never arrives, with a stall timer of 250 ms: the request is refused at 250 ms, and an event
# offered after that must not be added to the answer that is about to be replaced by the refusal and the end of the connection.
sk_start "$stream" 250
sk_client "open 1
send 1 GET /stream HTTP/1.1\\r\\nHost: t\\r\\nContent-Length: 10\\r\\n\\r\\n
stamp 0
read 1 0\\r\\n\\r\\n 3000
count data: 1 3
has 0\\r\\n\\r\\n
since lt 3000 0
sleep 300
$(get /gone 2)"
sk_stop

echo "== a reader that never reads is never given half a message"
sk_start "$stream"
# Connection 1 reads nothing. The program offers a 4,007-byte chunk every turn while the room takes it whole; the first turn it does not, the
# stream is ended. What the reader gets when it finally reads is the head and whole chunks, nothing cut, and the server ended its connection.
sk_client "open 1
send 1 GET /flood HTTP/1.1\\r\\nHost: t\\r\\n\\r\\n
open 2
stats 2
readend 1 20000
chunks
$(get /plain 3)"
sk_stop

echo "== stream through the store"
mkdir -p "$SK_SCRATCH/store-fetch"
"$SK_CANCHO" vcs fetch --lock tests/programs/server_stream.lock --store packages/http-server/.cancho-vcs -o "$SK_SCRATCH/store-fetch" > /dev/null 2>&1 || echo "vcs fetch failed"
sk_build server_stream_store tests/programs/server_stream.cho "$SK_SCRATCH"/store-fetch/*.cho
sk_start "$SK_SCRATCH/server_stream_store"
sk_client "open 1
send 1 $STREAM
read 1 0\\r\\n\\r\\n 10000
shows"
sk_stop
