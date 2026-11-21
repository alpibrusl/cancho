# net.sh -- the network examples and the `Net` bounds, over real loopback sockets: examples/serve, fetch, report, collect, agent_guest and agent_supervisor, and
# connect/bind against a narrowed `Net` (docs/reach.md, connect.md, listen.md, net.md). Moved from crates/cancho/tests/conformance/net.rs (20 tests, deleted;
# docs/gate.md, G4 wave 4). Each check is `begin NAME` ... `end` of tests/commands/script_lib.sh; the transcript is compared with net.expected.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM, through the compiler this is run with).
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/support/net.sh

build_exe sk_client tests/programs/sk_client.cho || exit 1
build_exe sk_holder tests/programs/sk_holder.cho || exit 1

for dep in serve fetch report collect guest supervisor; do mkdir -p "$work/dep-$dep"; done
fetch_deps "$work/dep-serve" examples/serve/net.lock packages/net-sockets/.cancho-vcs
fetch_deps "$work/dep-fetch" examples/fetch/connect.lock packages/net-connect/.cancho-vcs examples/fetch/response.lock packages/http-response/.cancho-vcs
fetch_deps "$work/dep-report" examples/report/connect.lock packages/net-connect/.cancho-vcs examples/report/response.lock packages/http-response/.cancho-vcs
fetch_deps "$work/dep-collect" examples/collect/request.lock packages/http-request/.cancho-vcs
fetch_deps "$work/dep-guest" examples/agent_guest/connect.lock packages/net-connect/.cancho-vcs examples/agent_guest/response.lock packages/http-response/.cancho-vcs examples/agent_guest/wire.lock packages/agent-wire/.cancho-vcs
fetch_deps "$work/dep-supervisor" examples/agent_supervisor/request.lock packages/http-request/.cancho-vcs
build_exe serve examples/serve/serve.cho "$work"/dep-serve/*.cho
build_exe fetch examples/fetch/fetch.cho "$work"/dep-fetch/*.cho
build_exe report examples/report/report.cho "$work"/dep-report/*.cho
build_exe collect examples/collect/collect.cho "$work"/dep-collect/*.cho
build_exe agent_guest examples/agent_guest/agent_guest.cho "$work"/dep-guest/*.cho
build_exe agent_supervisor examples/agent_supervisor/agent_supervisor.cho "$work"/dep-supervisor/*.cho

# The report of the authority command (JSON) of FILES, in $work/authority.json.
authority_json() { "$cancho" authority "$@" --std --output json > "$work/authority.json" 2>&1; }

begin an_http_server_written_in_cancho_answers_a_real_request
for row in '/health|200 OK|{"ok":true}' '/nothing-here|404 Not Found|{"error":"not found"}'; do
    target=${row%%|*}
    rest=${row#*|}
    status_line="HTTP/1.1 ${rest%%|*}"
    body=${rest#*|}
    spawn "$work/serve.out" "$work/serve"
    got=$(sk "open 1
send 1 GET $target HTTP/1.1\\r\\nHost:\\slocalhost\\r\\n\\r\\n
resp 1")
    await
    printf '%s\n' "$got" | grep -qxF "< $status_line" || fail "$target: the status line was not '$status_line': $got"
    printf '%s\n' "$got" | grep -qxF "= $body" || fail "$target: the body was not '$body': $got"
    printf '%s\n' "$got" | grep -qxF "< Content-Length: ${#body}" || fail "$target: the declared length is not the body's: $got"
    same "$sstatus" 0 "$target: the server's exit status after one request"
done
end

begin the_authority_report_names_the_syscalls_the_row_cannot
authority_json examples/serve/serve.cho "$work"/dep-serve/*.cho
grep -qF '"effects": ["args", "ffi"]' "$work/authority.json" || fail "the effects are not args and ffi"
grep -qF '{ "name": "ffi", "argument": "libc", "bounded": false }' "$work/authority.json" || fail "the ffi label is not libc, unbounded"
for symbol in socket bind listen accept; do
    grep -qF "\"$symbol\"" "$work/authority.json" || fail "the report does not name $symbol"
done
end

begin the_network_programs_are_counted
# docs/net.md section 5: the files that declare the socket functions, inbound (bind, listen, accept) and outbound (connect, sendto, getaddrinfo).
inbound=$(find examples compiler std tests/accept packages -name '*.cho' -not -path '*/.cancho-vcs/*' | sort | xargs grep -lE '^[[:space:]]*(pub[[:space:]]+)?extern fn[[:space:]]+(bind|listen|accept)[[:space:]]*[[(]' | tr '\n' ' ')
outbound=$(find examples compiler std tests/accept packages -name '*.cho' -not -path '*/.cancho-vcs/*' | sort | xargs grep -lE '^[[:space:]]*(pub[[:space:]]+)?extern fn[[:space:]]+(connect|sendto|getaddrinfo)[[:space:]]*[[(]' | tr '\n' ' ')
same "$inbound" "packages/net-sockets/sockets.cho " "the files that declare bind, listen or accept"
same "$outbound" "packages/net-connect/connect.cho " "the files that declare connect, sendto or getaddrinfo"
end

begin a_cancho_client_fetches_from_a_cancho_server
for row in '/health|{"ok":true}|0' '/elsewhere|{"error":"not found"}|1'; do
    path=${row%%|*}
    rest=${row#*|}
    body=${rest%|*}
    want=${rest##*|}
    spawn "$work/serve.out" "$work/serve"
    client "$work/fetch" 127.0.0.1 "$port" "$path"
    await
    printf '%s' "$body" > "$work/want.out"
    same_file "$work/c.out" "$work/want.out" "fetch $path should print exactly the body"
    same "$cstatus" "$want" "fetch $path: the exit status"
done
end

begin fetch_speaks_http_1_0_and_streams_the_body
# A server that splits its header block inside the blank line and sends 100,000 bytes (25 of the client's 4 KiB reads).
peer "accept 1
read 1 \\r\\n\\r\\n 10000
shows
send 1 HTTP/1.0\\s200\\sOK\\r\\nX-Split:\\syes\\r\\n\\r
sleep 50
send 1 \\n
gen 1 100000 0 4096 0
close 1"
"$work/fetch" 127.0.0.1 "$port" '/a/b?c=d' > "$work/c.out" 2> "$work/c.err"
same "$?" 0 "fetch's exit status"
peer_end > "$work/peer.said"
grep -qxF 'GET /a/b?c=d HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n' "$work/peer.said" || fail "the request was not as wanted: $(cat "$work/peer.said")"
gen_file 100000 0 "$work/want.out"
same_file "$work/c.out" "$work/want.out" "the body"
end

begin fetch_refuses_a_name_it_cannot_resolve
for row in 'localhost 80|there is no name resolution' '10.0.0 80|four decimal octets' '10.0.0.256 80|four decimal octets' '10.0.0.1 0|1..65535' '10.0.0.1 65536|1..65535'; do
    args=${row%%|*}
    message=${row#*|}
    # shellcheck disable=SC2086
    "$work/fetch" $args / > "$work/c.out" 2> "$work/c.err"
    same "$?" 2 "fetch $args should be a usage error"
    grep -qF "$message" "$work/c.err" || fail "fetch $args should say '$message': $(cat "$work/c.err")"
done
end

begin the_linux_address_layout_connects_on_both_targets
# docs/connect.md section 3: struct sockaddr_in is two different byte arrays; the Linux one is accepted on both targets (BSD reads family 0 as AF_INET).
cat > "$work/layout.cho" <<'EOF'
extern fn socket[&f](ffi: &f Ffi("libc"), domain: int, kind: int, proto: int)
    -> [ffi("libc")] c_int;
extern fn connect[&f, &a](ffi: &f Ffi("libc"), fd: int, addr: &a [byte])
    -> [ffi("libc")] c_int;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap);
    let libc = narrow(ffi, "libc");
    var status = 2;
    borrow libc as &f in {
        borrow args as &g in {
            let text = arg(g, 1);
            var port = 0;
            var i = 0;
            while i < len(text) { port = port * 10 + (int_of(text[i]) - '0'); i = i + 1; }
            region r {
                let addr = alloc_slice[r](16, byte_of(0));
                if int_of(arg(g, 2)[0]) == 'b' {
                    addr[0] = byte_of(16);
                    addr[1] = byte_of(2);
                } else {
                    addr[0] = byte_of(2);
                }
                addr[2] = byte_of(port / 256);
                addr[3] = byte_of(port % 256);
                addr[4] = byte_of(127);
                addr[7] = byte_of(1);
                let fd = socket(f, 2, 1, 0);
                status = 1;
                if connect(f, fd, addr) == 0 { status = 0; }
            }
        }
    }
    release(libc);
    release(args);
    return status;
}
EOF
if build "$work/layout.cho" layout; then
    # A listener that is never accepted from: the kernel completes the handshake into the backlog, which is all connect waits for.
    pick_port
    "$work/sk_holder" "$port" > /dev/null 2>&1 &
    pid=$!
    sleep 0.4
    "$work/layout" "$port" linux; linux=$?
    "$work/layout" "$port" bsd; bsd=$?
    kill -9 $pid 2> /dev/null; wait $pid 2> /dev/null
    case $(uname -s) in
        Linux)
            same "$linux" 0 "Linux should accept its own layout, 2 0"
            [ "$bsd" != 0 ] || fail "Linux should refuse 16 2, which it reads as family 528"
            ;;
        Darwin)
            same "$bsd" 0 "macOS should accept its own layout, 16 2"
            same "$linux" 0 "macOS should accept 2 0 too, reading family 0 as AF_INET"
            ;;
    esac
fi
end

begin the_client_and_the_server_differ_only_in_their_symbols
authority_json examples/fetch/fetch.cho "$work"/dep-fetch/*.cho
client_symbols=$(sed -n 's/.*"foreign_symbols": \[\(.*\)\].*/\1/p' "$work/authority.json")
grep -qF '"effects": [' "$work/authority.json" && grep -F '"effects"' "$work/authority.json" | grep -qF '"ffi"' || fail "the client's effects do not include ffi"
authority_json examples/serve/serve.cho "$work"/dep-serve/*.cho
server_symbols=$(sed -n 's/.*"foreign_symbols": \[\(.*\)\].*/\1/p' "$work/authority.json")
grep -F '"effects"' "$work/authority.json" | grep -qF '"ffi"' || fail "the server's effects do not include ffi"
case $client_symbols in *'"connect"'*) ;; *) fail "the client does not name connect: $client_symbols" ;; esac
for inbound in bind listen accept; do
    case $client_symbols in *"\"$inbound\""*) fail "the client names $inbound: $client_symbols" ;; esac
    case $server_symbols in *"\"$inbound\""*) ;; *) fail "the server does not name $inbound: $server_symbols" ;; esac
done
case $server_symbols in *'"connect"'*) fail "the server names connect: $server_symbols" ;; esac
end

begin a_cancho_agent_reports_to_a_cancho_server
spawn "$work/serve.out" "$work/serve"
client "$work/report" 127.0.0.1 "$port" /result 42
await
printf '%s' '{"error":"not found"}' > "$work/want.out"
same_file "$work/c.out" "$work/want.out" "report should print exactly the body"
same "$cstatus" 1 "report's exit status for a 404"
end

begin report_sends_a_body_the_server_can_read_in_full
# A POST whose body (100,000 bytes) takes more than one write; the server reads the head and then as many bytes as Content-Length says.
peer "accept 1
readreq 1 10000
reqparts
send 1 HTTP/1.0\\s200\\sOK\\r\\nContent-Length:\\s2\\r\\n\\r\\nok
sleep 100
close 1"
# The client's message is 'a' + i mod 26.
awk 'BEGIN { for (i = 0; i < 100000; i++) printf "%c", 97 + i % 26 }' > "$work/message.txt"
message=$(cat "$work/message.txt")
"$work/report" 127.0.0.1 "$port" /result "$message" > "$work/c.out" 2> "$work/c.err"
same "$?" 0 "report's exit status"
peer_end > "$work/peer.said"
printf '%s' ok > "$work/want.out"
same_file "$work/c.out" "$work/want.out" "the client should print the response body"
want_sum=$(awk 'BEGIN { c = 0; for (i = 0; i < 100000; i++) c = (c * 131 + 97 + i % 26) % 1000000007; print " n=100000 c=" c }')
grep -qxF 'head: POST /result HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Length: 100000\r\nConnection: close\r\n\r\n' "$work/peer.said" || fail "the head was not as wanted: $(head -c 300 "$work/peer.said")"
grep -qxF "body$want_sum" "$work/peer.said" || fail "the body did not arrive whole: $(tail -c 100 "$work/peer.said"), wanted$want_sum"
end

begin an_inbound_agent_reads_several_requests_in_a_row
spawn "$work/collect.out" "$work/collect" 3
got=$(sk "open 1
send 1 POST\\s/a\\sHTTP/1.0\\r\\nContent-Length:\\s3\\r\\nConnection:\\sclose\\r\\n\\r\\none
head 1
open 2
send 2 POST\\s/b\\sHTTP/1.0\\r\\nContent-Length:\\s18\\r\\nConnection:\\sclose\\r\\n\\r\\ntwo-longer-message
head 2
open 3
send 3 POST\\s/c\\sHTTP/1.0\\r\\nContent-Length:\\s5\\r\\nConnection:\\sclose\\r\\n\\r\\nthree
head 3")
await
same "$(printf '%s\n' "$got" | grep -c '^< HTTP/1.1 200 OK$')" 3 "collect should answer 200 three times"
same "$sstatus" 0 "the server's exit status"
printf '%s' 'onetwo-longer-messagethree' > "$work/want.out"
same_file "$work/collect.out" "$work/want.out" "the three bodies should print in order with nothing between them"
end

begin collect_reads_a_body_larger_than_one_read
spawn "$work/collect.out" "$work/collect" 1
got=$(sk "open 1
put POST\\s/big\\sHTTP/1.0\\r\\nContent-Length:\\s100000\\r\\nConnection:\\sclose\\r\\n\\r\\n
putgen 100000 0
flush 1
head 1")
await
printf '%s\n' "$got" | grep -qxF '< HTTP/1.1 200 OK' || fail "collect should answer 200: $got"
same "$sstatus" 0 "the server's exit status"
gen_file 100000 0 "$work/want.out"
same_file "$work/collect.out" "$work/want.out" "the body should arrive whole and in order"
end

# A program with the Net capability narrowed to $1 (edition 2), which connects to $2:$3 and answers the call's result.
net_program() { # NAME
    cat > "$work/$1.cho"
    build "$work/$1.cho" "$1"
}

begin connecting_outside_the_granted_host_traps
net_program outside_host <<'EOF'
edition 2;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(ffi); release(fs); release(io);
    let bound = narrow(net, "127.0.0.1:1");
    var fd = 0;
    borrow bound as &n in {
        fd = connect(n, "10.0.0.1", 1);
    }
    release(bound);
    return fd;
}
EOF
traps "$work/outside_host" || fail "a host outside the bound should end the process by a signal"
end

begin a_host_inside_a_shorter_bound_is_dialled_on_both_backends
net_program short_bound <<'EOF'
edition 2;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(ffi); release(fs); release(io);
    let bound = narrow(net, "127.0.0");
    var fd = 0;
    borrow bound as &n in {
        fd = connect(n, "127.0.0.1", 1);
    }
    release(bound);
    if fd < 0 { return 7; }
    return 8;
}
EOF
# Nothing listens on port 1, so the connection is refused: -1.
"$work/short_bound" > /dev/null 2>&1
same "$?" 7 "the host is inside the bound"
end

begin connecting_to_the_wrong_port_traps
net_program wrong_port <<'EOF'
edition 2;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(ffi); release(fs); release(io);
    let bound = narrow(net, "127.0.0.1:1");
    var fd = 0;
    borrow bound as &n in {
        fd = connect(n, "127.0.0.1", 2);
    }
    release(bound);
    return fd;
}
EOF
traps "$work/wrong_port" || fail "a port outside the bound should end the process by a signal"
end

begin a_cancho_listener_accepts_a_real_connection
pick_port
cat > "$work/listener.cho" <<EOF
edition 2;
// \`close\` is deliberately not declared here: this compiler's \`extern fn\` convention crosses every \`int\` as 64 bits (docs/reach.md section 3), while \`bind\`'s own
// internal \`close\` is declared against libc's true 32-bit \`int\`: two signatures for one linker symbol. A short-lived test process needs no explicit close.
extern fn read[&f, &b](ffi: &f Ffi("libc"), fd: int, buf: &!b [byte]) -> [ffi("libc")] int;
extern fn write[&f, &b](ffi: &f Ffi("libc"), fd: int, buf: &b [byte]) -> [ffi("libc")] int;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io); release(fs); release(heap); release(args);
    let libc = narrow(ffi, "libc");
    let bound = narrow(net, "$port");
    var status = 1;
    borrow bound as &n in {
        borrow libc as &f in {
            let listener = bind(n, $port);
            if listener >= 0 {
                listen(listener, 1);
                let conn = accept(listener);
                if conn >= 0 {
                    region scratch {
                        let buf = alloc_slice[scratch](64, byte_of(0));
                        let got = read(f, conn, buf);
                        if got > 0 {
                            write(f, conn, buf[0..got]);
                            status = 0;
                        }
                    }
                }
            }
        }
    }
    release(libc);
    release(bound);
    return status;
}
EOF
if build "$work/listener.cho" listener; then
    "$work/listener" > /dev/null 2>&1 &
    pid=$!
    got=$(sk "open 1
send 1 ping
read 1 ping 5000
close 1")
    printf '%s\n' "$got" | grep -qxF 'read ping: yes' || fail "the accepted connection should echo what was sent: $got"
    await
    same "$sstatus" 0 "the listener should report success"
fi
end

begin binding_the_wrong_port_traps
net_program wrong_bind_port <<'EOF'
edition 2;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(ffi); release(fs); release(io);
    let bound = narrow(net, "1");
    var fd = 0;
    borrow bound as &n in {
        fd = bind(n, 2);
    }
    release(bound);
    return fd;
}
EOF
traps "$work/wrong_bind_port" || fail "a port outside the bound should end the process by a signal"
end

begin connect_and_bind_are_operands_on_both_backends
# connect and bind are operands like any int call. The dialled port is free, so the connection is refused; the bound one is free, so the bind succeeds.
pick_port; refused=$port
pick_port; free=$port
cat > "$work/operand.cho" <<EOF
edition 2;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(args); release(heap); release(ffi); release(fs); release(io);
    var score = 0;
    borrow net as &n in {
        if connect(n, "127.0.0.1", $refused) < 0 { score = score + 1; }
        if bind(n, $free) + 1 > 0 { score = score + 2; }
    }
    release(net);
    return score;
}
EOF
if build "$work/operand.cho" operand; then
    "$work/operand" > /dev/null 2>&1
    same "$?" 3 "both comparisons should hold"
fi
end

begin a_cancho_guest_exchanges_a_view_with_a_cancho_supervisor
spawn "$work/sup.out" "$work/agent_supervisor" "write the report" 3
client "$work/agent_guest" 127.0.0.1 "$port"
await
printf 'goal: write the report\nstep: 3\n' > "$work/want.out"
same_file "$work/c.out" "$work/want.out" "agent_guest should print exactly the goal and step it decoded"
same "$cstatus" 0 "agent_guest should exit 0 on a decoded view"
end

begin a_goal_needing_json_escaping_survives_the_round_trip
spawn "$work/sup.out" "$work/agent_supervisor" 'say "hi" and go' 42
client "$work/agent_guest" 127.0.0.1 "$port"
await
# The decoder does not unescape (examples/vsock/vsock.cho's own documented limit): what the guest prints is the escaped form.
printf 'goal: say \\"hi\\" and go\nstep: 42\n' > "$work/want.out"
same_file "$work/c.out" "$work/want.out" "the escaped goal should survive the round trip"
same "$cstatus" 0 "agent_guest should exit 0 on a decoded view"
end
