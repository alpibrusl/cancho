# https_hello.sh -- the script of tests/commands/tls_scripts.cho for `conformance/https_hello.rs` (deleted): examples/https_hello (docs/http-server.md section 11), an HTTPS server made of the TLS engine (packages/tls) and the
# byte-fed `http.server` (packages/http-server), built and run against packages/tls's own client (tests/programs/tls_many.cho), so no TLS library outside this repository is needed. (scripts/https_hello_test.py is the same
# server against `openssl s_client`, curl and Python's ssl: keep-alive, pipelining, 200 connections, a large body, a client that stops reading; CI's `tls-assurance` job runs it.)
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM).
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

E=tests/vectors/tls/echo
example="examples/https_hello/hello.cho examples/https_hello/loop.cho examples/https_hello/app.cho examples/tls_echo/front.cho examples/tls_echo/identity.cho packages/http-server/server.cho $tls_package"
install() { _id=$1; shift; for f in "$@"; do cp "$E/$_id/$f" "$certs/$f"; done; }

# The report, pinned, as the echo's is: no foreign code, and the same labels. The HTTP server adds none (it is all `conn_*` and `poll`, which the loop has), so a label that appears here is the example, or the package,
# gaining authority.
begin "the example reports a bounded authority and no foreign code (15 labels, pinned)"
tls_authority tests/commands/https_hello.labels $example
end

certs=$work/certs
mkdir -p "$certs"

# An HTTP/1.0 request is answered `Connection: close` and closed, so the answer is the same 99 bytes for every connection: `HTTP/1.1 200 OK`, three headers, and `hello over TLS`.
begin "the server answers many clients, reloads and stops cleanly (64 connections, 4 handshakes at a time)"
if tls_build hello $example && tls_build many tests/programs/tls_many.cho $tls_package; then
    install first chain.pem key.pem names
    if tls_start "$work/hello.out" listening -- "$work/hello" --port @PORT@ --dir "$certs" --idle 1000 --handshakes 4 --connections 64; then
        tls_clients 64 99
        tls_wait "$work/hello.out" " established suite=" 64
        same "$(grep -F ' established suite=' "$work/hello.out" | grep -vc ' sni=echo.lex-sys.test ')" 0 "every connection says its server name"
        tls_wait "$work/hello.out" " closed ok " 64
        install renewed chain.pem key.pem
        kill -HUP "$spid"
        tls_wait "$work/hello.out" "reload 0 ok" 1
        install first chain.pem
        kill -HUP "$spid"
        tls_wait "$work/hello.out" "reload 0 refused tls-server-key-mismatch" 1
        tls_clients 8 99
        tls_wait "$work/hello.out" " closed ok " 72
        kill -TERM "$spid"
        tls_end
        same "$status" 0 "the server's status after SIGTERM"
        tls_wait "$work/hello.out" stopping 1
        tls_kill
    fi
fi
end

begin "the server resumes the package's own client when tickets are on (--tickets 2: 8 resumed)"
if [ -x "$work/hello" ] && [ -x "$work/many" ]; then
    install first chain.pem key.pem names
    if tls_start "$work/hello.out" listening -- "$work/hello" --port @PORT@ --dir "$certs" --idle 1000 --handshakes 4 --connections 64 --tickets 2; then
        tls_clients 8 "" resume
        same "$resumed" 8 "every second-round connection resumed"
        tls_wait "$work/hello.out" " resumed=yes " 8
        tls_wait "$work/hello.out" " resumed=no " 8
        kill -TERM "$spid"
        tls_end
        same "$status" 0 "the server's status after SIGTERM"
        tls_kill
    fi
fi
end
