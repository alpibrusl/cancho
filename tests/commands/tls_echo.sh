# tls_echo.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls_echo.rs` (deleted): examples/tls_echo (docs/tls-server.md section 8, step 3), the TLS echo server the broker and the gateway copy, built and run against
# packages/tls's own client (tests/programs/tls_many.cho), so no TLS library outside this repository is needed. The authority report is pinned (tests/commands/tls_echo.labels); 64 connections on one thread with at most 4
# handshakes in progress, a reload (SIGHUP) taken and one refused, a stop (SIGTERM); session tickets, resumption and the reload of the ticket keys.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM).
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

E=tests/vectors/tls/echo
example="examples/tls_echo/tls_echo.cho examples/tls_echo/echo.cho examples/tls_echo/front.cho examples/tls_echo/identity.cho $tls_package"

# install IDENTITY FILE...: the files of a test identity copied into the directory the server reads.
install() { _id=$1; shift; for f in "$@"; do cp "$E/$_id/$f" "$certs/$f"; done; }

begin "the example reports a bounded authority and no foreign code (15 labels, pinned)"
tls_authority tests/commands/tls_echo.labels $example
end

certs=$work/certs
mkdir -p "$certs"

begin "the echo serves many connections, reloads and stops cleanly (64 connections, 4 handshakes at a time)"
if tls_build echo $example && tls_build many tests/programs/tls_many.cho $tls_package; then
    install first chain.pem key.pem names
    if tls_start "$work/echo.out" listening -- "$work/echo" --port @PORT@ --dir "$certs" --idle 1000 --handshakes 4 --connections 64; then
        tls_clients 64 16384
        tls_wait "$work/echo.out" " established suite=" 64
        same "$(grep -F ' established suite=' "$work/echo.out" | grep -vc ' sni=echo.lex-sys.test ')" 0 "every connection says its server name"
        tls_wait "$work/echo.out" " closed idle " 64
        install renewed chain.pem key.pem
        kill -HUP "$spid"
        tls_wait "$work/echo.out" "reload 0 ok" 1
        install first chain.pem
        kill -HUP "$spid"
        tls_wait "$work/echo.out" "reload 0 refused tls-server-key-mismatch" 1
        tls_clients 8 16384
        tls_wait "$work/echo.out" " closed idle " 72
        kill -TERM "$spid"
        tls_end
        same "$status" 0 "the server's status after SIGTERM"
        tls_wait "$work/echo.out" stopping 1
        tls_kill
    fi
fi
end

begin "the echo resumes the package's own client and reloads its ticket keys (8 resumed, then 8, then 8)"
if [ -x "$work/echo" ] && [ -x "$work/many" ]; then
    install first chain.pem key.pem names
    key() { printf '%s\n' "$(i=0; while [ $i -lt 32 ]; do printf '%02x' "$1"; i=$((i + 1)); done)"; }
    key 1 > "$certs/keys"
    if tls_start "$work/echo.out" listening -- "$work/echo" --port @PORT@ --dir "$certs" --idle 1000 --handshakes 4 --connections 64 --tickets 2 --ticket-keys keys; then
        tls_clients 8 "" resume
        same "$resumed" 8 "every second-round connection resumed"
        tls_wait "$work/echo.out" " resumed=yes " 8
        tls_wait "$work/echo.out" " resumed=no " 8
        # A new key on top, the old below it: reloaded, and still serving.
        { key 2; key 1; } > "$certs/keys"
        kill -HUP "$spid"
        tls_wait "$work/echo.out" "reload tickets ok" 1
        tls_clients 8 "" resume
        same "$resumed" 8 "after the reload"
        tls_wait "$work/echo.out" " resumed=yes " 16
        # A file that is not hex is refused, and the keys stay.
        echo "not hex" > "$certs/keys"
        kill -HUP "$spid"
        tls_wait "$work/echo.out" "reload tickets refused tls-server-ticket-key" 1
        tls_clients 8 "" resume
        same "$resumed" 8 "after the refused reload"
        tls_wait "$work/echo.out" " resumed=yes " 24
        kill -TERM "$spid"
        tls_end
        same "$status" 0 "the server's status after SIGTERM"
        tls_kill
    fi
fi
end
