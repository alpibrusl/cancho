# api.sh -- examples/api (docs/server.md): the server loop over real sockets, moved from crates/cancho/tests/conformance/api.rs (deleted; docs/gate.md, G4
# wave 3). Every section starts the compiled server on a port of its own, talks to it from the cancho program tests/programs/sk_client.cho (several
# clients at once are several processes of it) and kills it. The things worth testing about a server are not the routes (those are std.route's
# tests) but what a loop around them can get wrong: a request split across segments, several in one, a client that goes silent, a client that vanishes,
# and a refusal that must close the connection. `ask S STATUS BODY REQUEST` prints nothing when the answer is as wanted (`tally` counts them).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
. tests/support/sk.sh
sk_setup
mkdir -p "$SK_SCRATCH/api-fetch"
"$SK_CANCHO" vcs fetch --lock examples/api/server.lock --store packages/http-server/.cancho-vcs -o "$SK_SCRATCH/api-fetch" > /dev/null 2>&1 || echo "vcs fetch failed"
sk_build api examples/api/api.cho "$SK_SCRATCH"/api-fetch/*.cho
api="$SK_SCRATCH/api"
get() { printf 'GET %s HTTP/1.1\\r\\nHost: t\\r\\n\\r\\n' "$1"; }
post() { printf 'POST %s HTTP/1.1\\r\\nHost: t\\r\\nContent-Length: %s\\r\\n\\r\\n%s' "$1" "${#2}" "$2"; }
chunked() { # PATH PIECE... (a chunked POST of the pieces)
    path=$1; shift
    printf 'POST %s HTTP/1.1\\r\\nHost: t\\r\\nTransfer-Encoding: chunked\\r\\n\\r\\n' "$path"
    for piece in "$@"; do printf '%x\\r\\n%s\\r\\n' "${#piece}" "$piece"; done
    printf '0\\r\\n\\r\\n'
}

echo "== the routes answer"
sk_start "$api"
sk_client "open 1
send 1 $(get /health)
resp 1
ask 1 200 {\"id\":42,\"name\":\"user-42\"} $(get /users/42)
ask 1 400 {\"error\":\"id\smust\sbe\sa\snumber\"} $(get /users/abc)
ask 1 404 - $(get /users/)
send 1 $(get '/search?q=a%20b+c%C3%A9')
resp 1
ask 1 400 {\"error\":\"q\sis\srequired\"} $(get /search)
ask 1 400 - $(get '/search?q=%zz')
ask 1 200 {\"sum\":42} $(post /add '{"a": 40, "b": 2}')
ask 1 422 {\"error\":\"a\sand\sb\smust\sbe\sintegers\"} $(post /add '{"a": 1.5, "b": 2}')
ask 1 422 - $(post /add '{"a": 1')
send 1 $(post /health '')
resp 1
send 1 $(get /add)
resp 1
send 1 $(get /nothing)
resp 1
tally"
sk_stop

echo "== one connection serves many requests, and pipelined ones in order"
sk_start "$api"
script="open 1"
for i in $(seq 0 59); do
    script="$script
ask 1 200 {\"id\":$i,\"name\":\"user-$i\"} $(get /users/$i)"
done
# Thirty requests in one write: thirty answers, in order.
for i in $(seq 1000 1029); do
    script="$script
put $(get /users/$i)"
done
script="$script
flush 1"
for i in $(seq 1000 1029); do
    script="$script
want 1 200 {\"id\":$i,\"name\":\"user-$i\"}"
done
sk_client "$script
tally"
sk_stop

echo "== a request split across segments is answered once whole"
sk_start "$api"
# Cut inside the request line, inside a header, at the blank line, and inside the body: each piece is its own segment.
sk_client "open 1
nodelay 1
send 1 POST\s
sleep 40
send 1 /add HTTP/1.1\\r\\nHo
sleep 40
send 1 st: t\\r\\nContent-Length: 16\\r\\n
sleep 40
send 1 \\r\\n{\"a\": 5,
sleep 40
send 1 \s\"b\": 6}
resp 1
ask 1 200 - $(get /health)
tally"
sk_stop

echo "== the start of the next request after a whole one is kept and completed"
sk_start "$api"
# One read holding a whole request and the front of the next: the first is answered, the rest is moved to the start of the buffer, and the other half,
# arriving later, completes it. Without that move the second request is parsed as its own tail.
script="open 1
nodelay 1"
for n in $(seq 0 19); do
    second=$(post /add "{\"a\": $n, \"b\": 100}")
    cut=$((11 + n % 7))
    front=$(printf '%s' "$second" | cut -c1-$cut)
    back=$(printf '%s' "$second" | cut -c$((cut + 1))-)
    script="$script
put $(get /users/$n)
put $front
flush 1
want 1 200 {\"id\":$n,\"name\":\"user-$n\"}
sleep 30
send 1 $back
want 1 200 {\"sum\":$((n + 100))}"
done
sk_client "$script
tally"
sk_stop

echo "== closing a connection moves the last one with its buffered bytes"
sk_start "$api"
# Connections are kept in a dense array and a closed one is replaced by the last. The last may be holding the front of a request; it must still be
# holding it afterwards, and still be the same client's.
script=
for n in $(seq 0 29); do
    r=$(get /users/$n)
    script="$script
open 1
open 2
nodelay 2
send 2 $(printf '%s' "$r" | cut -c1-14)
sleep 20
close 1
sleep 30
send 2 $(printf '%s' "$r" | cut -c15-)
want 2 200 {\"id\":$n,\"name\":\"user-$n\"}
close 2"
done
sk_client "$script
tally"
sk_stop

echo "== what cannot be trusted is refused, and the connection closes"
sk_start "$api"
refuse() { # NAME STATUS REQUEST (a request the server must answer, with the status, and close on)
    sk_client "open 1
send 1 $3
line 1
ends 1 5000" | sed "s/^/$1: /"
}
refuse 'both lengths' 400 'POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n{}{}'
refuse 'line folding' 400 'GET /health HTTP/1.1\r\nHost: t\r\nX: a\r\n b\r\n\r\n'
refuse 'space before colon' 400 'GET /health HTTP/1.1\r\nHost\s: t\r\n\r\n'
refuse 'no host' 400 'GET /health HTTP/1.1\r\n\r\n'
refuse 'garbage method' 400 'G@T /health HTTP/1.1\r\nHost: t\r\n\r\n'
refuse 'version' 400 'GET /health HTTP/2.0\r\nHost: t\r\n\r\n'
refuse 'two lengths' 400 'POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\n{}'
refuse 'chunk size not hex' 400 'POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n{}\r\n0\r\n\r\n'
refuse 'chunk extension' 400 'POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n2;x=1\r\n{}\r\n0\r\n\r\n'
refuse 'chunked trailer' 400 'POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\nX: 1\r\n\r\n'
refuse 'chunk framing' 400 'POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n2\n{}\r\n0\r\n\r\n'
refuse 'other encoding' 400 'POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n'
refuse 'chunked body too large' 413 'POST /add HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n186a0\r\n'
refuse 'body too large' 413 'POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 100000\r\n\r\n'
# A head with 20,000 bytes of header.
sk_client "open 1
send 1 GET /health HTTP/1.1\r\nHost: t\r\nX:\s
rep 1 20000 a
send 1 \r\n\r\n
line 1
ends 1 5000" | sed "s/^/head too large: /"
# None of that hurt the server.
sk_client "open 1
ask 1 200 {\"ok\":true} $(get /health)
tally"
sk_stop

echo "== Connection: close is honoured"
sk_start "$api"
sk_client "open 1
send 1 GET /health HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n
resp 1
ends 1 5000
open 2
send 2 GET /health HTTP/1.0\r\n\r\n
resp 2
ends 2 5000"
sk_stop

echo "== a silent client does not hold up the others"
sk_start "$api"
# Fifty connections that open and say nothing (processes of their own), one that sends half a head; everyone else is served promptly all the while.
idle=
for i in $(seq 1 50); do
    sk_client "open 1
sleep 6000" > /dev/null &
    idle="$idle $!"
done
sleep 1
script="open 1
send 1 GET /health HTT
stamp 0"
for i in $(seq 0 39); do
    script="$script
open 2
ask 2 200 - $(get /users/$i)
close 2"
done
sk_client "$script
since lt 3000 0
tally"
kill $idle 2>/dev/null
wait $idle 2>/dev/null
sk_stop

echo "== many clients at once each get their own answers"
sk_start "$api"
clients=
for t in $(seq 0 63); do
    script="open 1"
    for i in $(seq 0 39); do
        n=$((t * 1000 + i))
        if [ $((i % 4)) = 3 ]; then
            script="$script
ask 1 200 {\"sum\":$((n + 1))} $(post /add "{\"a\": $n, \"b\": 1}")"
        else
            script="$script
ask 1 200 {\"id\":$n,\"name\":\"user-$n\"} $(get /users/$n)"
        fi
    done
    sk_client "$script
tally" > "$SK_SCRATCH/many$t.out" &
    clients="$clients $!"
done
wait $clients
cat "$SK_SCRATCH"/many*.out | sort | uniq -c
sk_stop

echo "== clients that vanish do not hurt the server"
sk_start "$api"
for n in $(seq 0 149); do
    case $((n % 3)) in
        0) sk_client "open 1
send 1 $(get /health)
close 1" ;;
        1) sk_client "open 1
send 1 GET /hea
close 1" ;;
        *) sk_client "open 1
send 1 POST /add HTTP/1.1\r\nHost: t\r\nContent-Length: 50\r\n\r\n{\"a\"
close 1" ;;
    esac
done
sleep 0.1
sk_client "open 1
ask 1 200 {\"ok\":true} $(get /health)
tally"
sk_stop

echo "== an idle connection is closed after the timeout"
sk_start "$api" 0 1
# `api <port> reuseport 1`: one second of silence. The busy connection keeps talking and is never closed; the quiet one has been closed meanwhile.
script="open 1
open 2"
for i in 1 2 3 4 5; do
    script="$script
ask 2 200 - $(get /health)
sleep 600"
done
sk_client "$script
ends 1 3000
ask 2 200 - $(get /health)
tally"
sk_stop

echo "== a client that stops reading stalls only itself"
sk_start "$api"
# 4000 requests for 32 KiB each: the answers are 125 MiB, far more than the kernel holds for a client that reads none of it (the Rust test measured the
# loopback buffers and took twice that). A server that *blocks* in `write` stops dead there; this one parks the rest of that connection's answers and
# serves everyone else; and the stalled client loses nothing: it then reads every answer, whole and in order.
script="open 1
putrep 4000 GET /blob/32768 HTTP/1.1\r\nHost: t\r\n\r\n
flush 1
sleep 500
stamp 0"
for i in $(seq 0 29); do
    script="$script
open 2
ask 2 200 - $(get /users/$i)
close 2"
done
sk_client "$script
since lt 2000 0
wantn 1 4000 200 32768
tally"
sk_stop

echo "== a slow reader gets every byte in the right order"
# Answers of a thousand different sizes, read a little at a time. `1500` is the fourth argument: no `send` is handed more than that, so every answer is
# sent in pieces and the rest of a partial one has to be resumed. A byte dropped, repeated or reordered there changes a length or a byte.
sk_start "$api" 0 9 1500
script="open 1"
for i in $(seq 0 999); do
    script="$script
put $(get /blob/$((1 + (i * 7919) % 32768)))"
done
script="$script
flush 1"
for i in $(seq 0 999); do
    script="$script
blob 1 $((1 + (i * 7919) % 32768))"
    if [ $((i % 50)) = 0 ]; then
        script="$script
sleep 20
open 2
ask 2 200 - $(get /health)
close 2"
    fi
done
sk_client "$script
tally"
sk_stop

echo "== a client that never reads is closed after the idle timeout"
# Two seconds without progress. The connection has output waiting and no readable interest, so nothing it sends can keep it alive. Whatever the kernel
# was holding comes out, and then the connection ends; it must not still be open, and not every answer was delivered to it.
sk_start "$api" 0 2
sk_client "open 1
putrep 4000 GET /blob/32768 HTTP/1.1\r\nHost: t\r\n\r\n
flush 1
sleep 5000
readend 1 20000
size lt 131072000
open 2
ask 2 200 - $(get /health)
tally"
sk_stop

echo "== a connection that is to close closes only once its answer has gone"
# `Connection: close` on a 20 KB answer, with `send` limited to 1500 bytes at a time: the answer goes out in pieces over several rounds, and the
# connection must stay open until the last of them. And so does a refusal that is bigger than one piece: 20,000 bytes of header.
sk_start "$api" 0 30 1500
sk_client "open 1
send 1 GET /blob/20000 HTTP/1.1\r\nHost: t\r\nConnection: close\r\n\r\n
line 1
ends 1 5000
open 2
send 2 GET /health HTTP/1.1\r\nHost: t\r\nX:\s
rep 2 20000 a
send 2 \r\n\r\n
line 2"
sk_stop

echo "== the server announces its port on standard error"
# On the unbuffered stream: a piped standard output is held until the process ends, and a server that announces itself only then has not announced itself.
port=$(awk 'BEGIN { srand(); print 30000 + int(rand() * 20000) }')
"$api" "$port" > /dev/null 2> "$SK_SCRATCH/announce.err" &
pid=$!
sleep 1
sed -n 1p "$SK_SCRATCH/announce.err" | sed "s/$port/PORT/"
sk_stop

echo "== the server holds no foreign authority"
# The point of docs/native-sockets.md: the same server that reported `ffi("libc")` now reports exactly what it does.
"$SK_CANCHO" authority examples/api/api.cho "$SK_SCRATCH"/api-fetch/*.cho --std --output json > "$SK_SCRATCH/authority.json"
if grep -q '"ffi"' "$SK_SCRATCH/authority.json"; then echo "foreign code: yes"; else echo "foreign code: no"; fi
for label in net_in conn_accept conn_read conn_write poll clock heap; do
    if grep -q "\"name\": \"$label\"" "$SK_SCRATCH/authority.json"; then echo "reports $label: yes"; else echo "reports $label: no"; fi
done

echo "== a chunked body is decoded, and the handler cannot tell"
sk_start "$api"
sk_client "open 1
ask 1 200 {\"sum\":42} $(chunked /add '{"a": 40,' ' "b": 2}')
ask 1 200 {\"sum\":3} $(chunked /add '{"a": 1, "b": 2}')
ask 1 200 {\"sum\":15} $(chunked /add '{"a' '": 7, "b"' ': 8}')
ask 1 422 - $(chunked /add)
ask 1 200 - $(get /health)
tally"
sk_stop

echo "== a chunked body arriving a byte at a time is answered once and whole"
sk_start "$api"
sk_client "open 1
nodelay 1
bytewise 1 1 $(chunked /add '{"a": 20,' ' "b": 22}')
want 1 200 {\"sum\":42}
send 1 $(chunked /add '{"a": 1, "b": 1}')$(get /health)
want 1 200 {\"sum\":2}
want 1 200 {\"ok\":true}
tally"
sk_stop

echo "== a body larger than the default buffer needs the buffer argument"
padded=$(printf '{"a": 1, "b": 2}%40000s' '')
# By default a request must fit 16 KiB, and is refused at once if it cannot.
sk_start "$api"
sk_client "open 1
ask 1 413 - $(post /add "$padded")
tally"
sk_stop
# With the sixth argument -- a 64 KiB buffer -- the same request is answered. (`no` is not `1` or `r`, so the port is not shared; 9 seconds idle; 0, no
# write quantum.) A chunked one too, across two chunks; still refused past the bigger buffer.
sk_start "$api" no 9 0 65536
first=$(printf '%s' "$padded" | cut -c1-20000)
second=$(printf '%s' "$padded" | cut -c20001-)
huge=$(printf '{"a": 1, "b": 2}%70000s' '')
sk_client "open 1
ask 1 200 {\"sum\":3} $(post /add "$padded")
ask 1 200 {\"sum\":3} $(chunked /add "$first" "$second")
open 2
ask 2 413 - $(post /add "$huge")
tally"
sk_stop

echo "== a buffer the server cannot honour is refused at start"
# Below a page, or above a megabyte: the server says nothing and exits 2.
for size in 100 2097152; do
    port=$(awk 'BEGIN { srand(); print 30000 + int(rand() * 20000) }')
    "$api" "$port" no 9 0 "$size" > /dev/null 2>&1
    echo "buffer $size: exit $?"
done

echo "== a port taken by another listener is not mistaken for the server"
# Issue #675. Another listener takes the port between the test finding it free and the server listening on it. The server then exits, finding the port
# busy; a connect to that port succeeds all the same, to the other listener. A start that trusted the connect would talk to the wrong one.
sk_build sk_holder tests/programs/sk_holder.cho
sk_start "$SK_SCRATCH/sk_holder"
taken=$port
holder=$pid
"$api" "$taken" > /dev/null 2>&1 &
pid=$!
sleep 0.5
if kill -0 $pid 2>/dev/null; then echo "the server stayed up on a busy port: yes"; else echo "the server stayed up on a busy port: no"; fi
if sk_client "open 1"; then echo "a connect to the busy port succeeded: yes"; else echo "a connect to the busy port succeeded: no"; fi
pid=$holder
sk_stop
sk_start "$api"
sk_client "open 1
ask 1 200 {\"ok\":true} $(get /health)
tally"
sk_stop
