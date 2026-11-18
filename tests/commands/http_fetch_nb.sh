# http_fetch_nb.sh -- examples/http_fetch_nb (docs/http-client.md section 6): several URLs fetched at once on one thread, through packages/http-client and fetch_io.cho (the sockets, the TLS engine and the poller the package leaves
# to its caller). Moved from crates/cancho/tests/conformance/http_fetch_nb.rs (4 tests) and tls_support.rs (its helpers), both deleted (docs/gate.md, G4 wave 5). The example is built here and run against a keep-alive HTTP/1.1
# server that is the scripted client in its serving mode (tests/programs/sk_client.cho: `accept`, `readreq`, `send`; the lanes of the fetcher are sequential on a connection, so a script that serves the two connections in turn
# cannot deadlock; lane 0 dials first and loopback accepts in the order of the dials, so slot 0 is lane 0) and against examples/https_hello, the repository's own TLS server, so no program outside the repository is needed.
# scripts/http_client_test.py is the same program against Python's ssl, nginx and a hostile server (CI's `tls-assurance` job runs it). The authority report is pinned (tests/commands/http_fetch_nb.labels).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM).
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh
. tests/support/net.sh

E=tests/vectors/tls/echo
fetch_files="examples/http_fetch_nb/fetch.cho examples/http_fetch_nb/fetch_io.cho packages/http-client/slot.cho packages/http-client/wire.cho packages/http-client/client.cho $tls_package"
hello_files="examples/https_hello/hello.cho examples/https_hello/loop.cho examples/https_hello/app.cho examples/tls_echo/front.cho examples/tls_echo/identity.cho packages/http-server/server.cho $tls_package"
# sha256("hello") and sha256("hello world").
HELLO=2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
HELLO_WORLD=b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9

# fetch ARGS...: the fetcher run with standard input $stdin (/dev/null by default); its lines in $work/fetch.out, its status in $fstatus.
stdin=/dev/null
fetch() { "$work/fetch" "$@" < "$stdin" > "$work/fetch.out" 2> "$work/fetch.err"; fstatus=$?; }
# starting PREFIX, ending SUFFIX: how many lines of the output start, or end, with the text.
starting() { awk -v p="$1" 'index($0, p) == 1 { n++ } END { print n + 0 }' "$work/fetch.out"; }
ending() { awk -v p="$1" 'length($0) >= length(p) && substr($0, length($0) - length(p) + 1) == p { n++ } END { print n + 0 }' "$work/fetch.out"; }
first() { head -n 1 "$work/fetch.out"; }
last() { tail -n 1 "$work/fetch.out"; }

# The report, pinned: what the example can reach is the review of it a supervisor reads, so a label that appears or goes is a red build. No foreign code; the network only outbound (`net_out`, with no host named: the URLs are the
# operator's, so the report says it is not bounded and why, docs/net-bounds.md section 2.4); the sockets and the poller; the clock; the console and standard input; and two paths for reading, fixed at build time by one `narrow`:
# /dev/urandom for the TLS engine's entropy, and /etc/cancho/http_fetch_nb, where `--client-identity` reads the client certificate and key (docs/tls-parity.md section 6.2). No file is written and nothing is listened on.
begin "the example reports a bounded authority and no foreign code (12 labels, pinned)"
# shellcheck disable=SC2086
if ! "$cancho" authority $fetch_files --std --output json > "$work/authority.json" 2> "$work/authority.err"; then
    fail "authority said: $(head -c 300 "$work/authority.err")"
else
    printf '%s\n' '{' '  "bounded": false,' '  "unbounded_by": [' '    "net_out(\"\")"' '  ],' > "$work/authority.head"
    head -n 5 "$work/authority.json" | cmp -s - "$work/authority.head" || fail "the report does not begin by saying the network is held unnarrowed: $(head -n 6 "$work/authority.json")"
    grep -q '"foreign_symbols": \[\]' "$work/authority.json" || fail "the report names foreign symbols"
    grep '{ "name": ' "$work/authority.json" | sed 's/^ *{ "name": //; s/, "bounded".*//' > "$work/authority.labels"
    cmp -s "$work/authority.labels" tests/commands/http_fetch_nb.labels || fail "the labels are not the pinned ones: $(diff tests/commands/http_fetch_nb.labels "$work/authority.labels" | head -n 6)"
fi
end

# The server's script: lane 0 asks /a (answered with a length), lane 1 asks /c (answered `hello world` in two chunks), four rounds each.
plain_script() {
    printf 'accept 0\naccept 1\n'
    _i=0
    while [ "$_i" -lt 4 ]; do
        printf 'readreq 0 5000\nreadreq 1 5000\n'
        printf 'send 0 HTTP/1.1 200 OK\\r\\nContent-Length: 5\\r\\n\\r\\nhello\n'
        printf 'send 1 HTTP/1.1 200 OK\\r\\nTransfer-Encoding: chunked\\r\\n\\r\\n6\\r\\nhello \\r\\n5\\r\\nworld\\r\\n0\\r\\n\\r\\n\n'
        _i=$((_i + 1))
    done
}

begin "it fetches over plain TCP on connections it keeps (8 requests on 2 connections; a 404, a closed port and a bad command line)"
# shellcheck disable=SC2086
if tls_build fetch $fetch_files; then
    if build_exe sk_client tests/programs/sk_client.cho && peer "$(plain_script)"; then
        fetch --repeat 4 "http://127.0.0.1:$port/a" "http://127.0.0.1:$port/c"
        peer_end > "$work/peer.log"
        same "$fstatus" 0 "the fetcher's status ($(head -c 200 "$work/fetch.out"))"
        same "$(last)" "done ok=8 failed=0 connects=2 reuses=6 retries=0" "the summary"
        same "$(starting "0 200 5 $HELLO ")" 4 "lane 0's answers"
        same "$(starting "1 200 11 $HELLO_WORLD ")" 4 "lane 1's answers"
        same "$(ending " new")" 2 "the connections made"
        same "$(ending " reused")" 6 "the connections reused"
        same "$(grep -c '^readreq bytes: ' "$work/peer.log" | tr -d ' ')" 8 "the requests the server read"
    fi
    # A 404 is a response, not a failure.
    if peer 'accept 0
readreq 0 5000
send 0 HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n'; then
        fetch "http://127.0.0.1:$port/missing"
        peer_end > /dev/null
        same "$fstatus" 0 "the status of a 404"
        case $(first) in "0 404 0 "*) ;; *) fail "a 404 is a line: '$(first)'" ;; esac
    fi
    # A closed port is `client.connect`; a bad command line is refused.
    pick_port
    fetch "http://127.0.0.1:$port/"
    same "$fstatus" 1 "the status of a closed port"
    same "$(first)" "0 failed client.connect" "a closed port"
    fetch "http://not-an-address.test/"
    same "$fstatus" 2 "the status of a name nobody resolved"
    fetch
    same "$fstatus" 2 "the status of no URL at all"
fi
end

certs=$work/certs
mkdir -p "$certs"
install() { _id=$1; shift; for f in "$@"; do cp "$E/$_id/$f" "$certs/$f"; done; }
sni=echo.lex-sys.test
printf 'not a pem\n' > "$work/not_a_pem"

begin "it fetches over TLS from https_hello, verifying the name and closing with close_notify (9 requests on 3 connections)"
# shellcheck disable=SC2086
if tls_build fetch $fetch_files && tls_build hello $hello_files; then
    install first chain.pem key.pem names
    if tls_start "$work/hello.out" listening -- "$work/hello" --port @PORT@ --dir "$certs" --idle 60000 --connections 16; then
        stdin=$E/ca.pem
        fetch --resolve "$sni=127.0.0.1" --repeat 3 "https://$sni:$port/hello/n1" "https://$sni:$port/big/100000" "https://$sni:$port/hello/n2"
        same "$fstatus" 0 "the fetcher's status ($(head -c 200 "$work/fetch.out"))"
        same "$(last)" "done ok=9 failed=0 connects=3 reuses=6 retries=0" "the summary"
        same "$(starting "1 200 100000 ")" 3 "the big body, three times"
        same "$(grep '^1 200 ' "$work/fetch.out" | awk '{ print $4 }' | sort -u | wc -l | tr -d ' ')" 1 "the same bytes every time"
        # Three handshakes for nine requests, and each connection was ended with close_notify (the pool is closed before the exit).
        tls_wait "$work/hello.out" " established suite=" 3
        same "$(grep -F ' established suite=' "$work/hello.out" | grep -vc " sni=$sni ")" 0 "every connection says its server name"
        tls_wait "$work/hello.out" " closed ok " 3
        # The name is checked: a certificate for the test name is refused for any other name, and so are the wrong roots.
        fetch --resolve other.example=127.0.0.1 "https://other.example:$port/hello/x"
        same "$fstatus" 1 "the status of a certificate for another name"
        same "$(first)" "0 failed client.tls" "a certificate for another name"
        stdin=$work/not_a_pem
        fetch --resolve "$sni=127.0.0.1" "https://$sni:$port/hello/x"
        same "$fstatus" 2 "a trust store with no root is refused before anything is dialled"
        stdin=/dev/null
        tls_kill
    fi
fi
end

# ALPN and client certificates through the example (docs/tls-parity.md section 6.6, 6.2): against https_hello serving `mqtt` and `http/1.1`, an offer of `h2,http/1.1` is answered `http/1.1` and the response line says so, with no
# certificate asked for (`auth=0`); an offer the server does not speak is refused by the server (`no_application_protocol`); and the identity options name files that can be read (the live mutual-TLS rows are
# scripts/tls_auth_interop.py's).
begin "it offers ALPN to https_hello and refuses what it cannot send"
if [ -x "$work/fetch" ] && [ -x "$work/hello" ]; then
    install first chain.pem key.pem names
    if tls_start "$work/hello.out" listening -- "$work/hello" --port @PORT@ --dir "$certs" --idle 60000 --connections 16 --alpn mqtt,http/1.1; then
        stdin=$E/ca.pem
        url="https://$sni:$port/hello/n1"
        fetch --resolve "$sni=127.0.0.1" --alpn h2,http/1.1 "$url"
        same "$fstatus" 0 "the status of an offer of h2,http/1.1"
        same "$(ending " new alpn=http/1.1 auth=0")" 1 "the choice is reported: $(first)"
        fetch --resolve "$sni=127.0.0.1" --alpn http/1.1 "$url"
        same "$fstatus" 0 "the status of an offer of http/1.1"
        same "$(ending " new alpn=http/1.1 auth=0")" 1 "the choice is reported: $(first)"
        fetch --resolve "$sni=127.0.0.1" --alpn h2 "$url"
        same "$fstatus" 1 "nothing in common ends the handshake"
        same "$(first)" "0 failed client.tls" "nothing in common"
        fetch --resolve "$sni=127.0.0.1" "$url"
        same "$fstatus" 0 "the status of no offer"
        same "$(ending " new")" 1 "no offer, no suffix: $(first)"
        # The identity is not a path the operator names: `--client-cert` is no option, and `--client-identity` reads the fixed directory, whose files a test machine does not have (the live mutual-TLS rows install them).
        fetch --resolve "$sni=127.0.0.1" --client-cert chain.pem "$url"
        same "$fstatus" 2 "--client-cert is no option"
        if [ ! -e /etc/cancho/http_fetch_nb/client.pem ]; then
            fetch --resolve "$sni=127.0.0.1" --client-identity "$url"
            same "$fstatus" 2 "the identity's files cannot be read"
        fi
        stdin=/dev/null
        tls_kill
    fi
fi
end
