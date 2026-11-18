# tls_lib.sh -- what the scripts of the TLS batch share (tls.sh, tls_auth.sh, tls_client_auth.sh, tls_server.sh, tls_fuzz.sh, tls_pool.sh, tls_echo.sh, tls_echo_fixed.sh, https_hello.sh; docs/gate.md G4,
# wave 4). Not a test of its own: a script sources it after script_lib.sh, with cancho=$1 and work=$2 set, from the root of the repository:   . tests/commands/tls_lib.sh
#
# The replay drivers (tests/programs/tls_driver.cho, tls_server_driver.cho, tls_tickets.cho) read one line a question on standard input and answer one line each. A recording
# (tests/vectors/tls/*.txt) is the questions and, after `= `, the answers they were given when the other end was a real TLS implementation or the script that lies, so a replay that
# gives the same bytes is a handshake that verified when it was recorded. `tls_cases` splits a recording into one file a case, `tls_run` replays one.

# The package: the engine, the client it is built on, the server, and packages/x509 with the key parser (the list of `package_files()` of the Rust tests).
tls_package="packages/tls/tls.cho packages/tls/record.cho packages/tls/message.cho packages/tls/slot.cho packages/tls/cident.cho packages/tls/client12.cho packages/tls/client.cho packages/tls/hello.cho packages/tls/identity.cho packages/tls/ticket.cho packages/tls/clientauth.cho packages/tls/server.cho packages/tls/saved.cho packages/x509/verify.cho packages/x509/names.cho packages/x509/x509.cho packages/x509/key.cho"
# What tls_driver.cho and record_open.cho are built with (the client alone).
tls_client_package="packages/tls/record.cho packages/tls/message.cho packages/tls/slot.cho packages/tls/cident.cho packages/tls/client12.cho packages/tls/client.cho packages/x509/verify.cho packages/x509/names.cho packages/x509/x509.cho packages/x509/key.cho"

# tls_build NAME SOURCE FILES...: SOURCE and FILES built as $work/NAME (the check fails, and the rest of it is skipped, if they do not build).
tls_build() {
    _n=$1; _src=$2; shift 2
    # shellcheck disable=SC2086
    if ! "$cancho" build "$_src" "$@" --std -o "$work/$_n" > "$work/build-$_n.txt" 2>&1; then
        fail "building $_src said: $(head -c 400 "$work/build-$_n.txt")"
        return 1
    fi
}

# tls_cases FILE DIR MODE: a recording split into DIR/N.{asked,answered,tag,name} for N from 1. MODE is `tagged` (`## TAG NAME` starts a case), `named` (`## NAME`) or `trace` (the whole file is one case).
# Prints the number of cases. Lines that start with `#` are comments.
tls_cases() {
    rm -rf "$2"
    mkdir -p "$2"
    awk -v dir="$2" -v mode="$3" '
        function flush() {
            if (n > 0) {
                printf "%s", asked > (dir "/" n ".asked"); close(dir "/" n ".asked")
                printf "%s", answered > (dir "/" n ".answered"); close(dir "/" n ".answered")
                print tag > (dir "/" n ".tag"); close(dir "/" n ".tag")
                print name > (dir "/" n ".name"); close(dir "/" n ".name")
            }
        }
        BEGIN { if (mode == "trace") { n = 1; tag = ""; name = "trace" } }
        /^## / && mode != "trace" {
            flush(); n++; asked = ""; answered = ""
            head = substr($0, 4)
            if (mode == "tagged") { i = index(head, " "); tag = substr(head, 1, i - 1); name = substr(head, i + 1) } else { tag = ""; name = head }
            next
        }
        /^#/ { next }
        n == 0 { next }
        /^= / { answered = answered substr($0, 3) "\n"; next }
        { asked = asked $0 "\n" }
        END { flush(); print n }
    ' "$1"
}

# tls_field FILE N: field N (from 1) of the last line of FILE, the fields being cut at each single space.
tls_field() { tail -n 1 "$1" | awk -F'[ ]' -v n="$2" '{ print $n }'; }

# tls_lines FILE: the number of lines of FILE.
tls_lines() { wc -l < "$1" | tr -d ' '; }

# tls_run EXE CASEPATH NAME: the driver EXE fed CASEPATH.asked; it must end 0, say one line for each question, and say CASEPATH.answered (byte for byte, which is the line-by-line comparison of the Rust tests
# and strictly more: the same count too). The answers are left in CASEPATH.got. Returns 1 (after saying why) if one does not hold.
tls_run() {
    "$1" < "$2.asked" > "$2.got" 2> "$2.err"
    _st=$?
    [ "$_st" = 0 ] || { fail "$3: the driver ended with $_st: $(head -c 200 "$2.err")"; return 1; }
    [ "$(tls_lines "$2.asked")" = "$(tls_lines "$2.got")" ] || { fail "$3: $(tls_lines "$2.got") answers to $(tls_lines "$2.asked") questions"; return 1; }
    cmp -s "$2.got" "$2.answered" || {
        _line=$(cmp "$2.got" "$2.answered" 2>&1 | sed 's/.*line //')
        fail "$3: the answers are not the recorded ones (first difference at line ${_line:-?})"
        return 1
    }
    return 0
}

# tls_ask EXE FILE NAME: the driver fed FILE (questions only); the answers in $work/ask.got; 0 if it ended 0 and answered every line.
tls_ask() {
    "$1" < "$2" > "$work/ask.got" 2> "$work/ask.err"
    _st=$?
    [ "$_st" = 0 ] || { fail "$3: the driver ended with $_st: $(head -c 200 "$work/ask.err")"; return 1; }
    [ "$(tls_lines "$2")" = "$(tls_lines "$work/ask.got")" ] || { fail "$3: $(tls_lines "$work/ask.got") answers to $(tls_lines "$2") questions"; return 1; }
    return 0
}

# tls_bytewise IN OUT: IN with every `F hex` line made one line a byte (`F xx`).
tls_bytewise() {
    awk '/^F / { d = substr($0, 3); for (k = 1; k <= length(d); k += 2) print "F " substr(d, k, 2); next } { print }' "$1" > "$2"
}

# tls_total ASKED GOT N OPS: the field N (from 1) of the answers whose question starts with one of the letters of OPS (all if OPS is empty), joined, a `-` being nothing.
tls_total() {
    awk -F'[ ]' -v n="$3" -v ops="$4" '
        NR == FNR { q[FNR] = substr($0, 1, 1); next }
        { if (ops == "" || index(ops, q[FNR]) > 0) { f = $n; if (f != "-") out = out f } }
        END { print out }
    ' "$1" "$2"
}

# tls_tag_case N: the end of a case of a lying peer, by its tag (CASES/N.tag) and event: the last answer says TAG as its field 2 and EVENT as its field 3.
tls_ends() { # GOT TAG EVENT NAME
    _f2=$(tls_field "$1" 2)
    _f3=$(tls_field "$1" 3)
    [ "$_f2" = "$2" ] || fail "$4: the connection ends with '$_f2', wanted '$2'"
    [ "$_f3" = "$3" ] || fail "$4: the last event is '$_f3', wanted '$3'"
}

# tls_unhex_flip HEX BYTE MASK: the hex text with one byte (counted from 0) xor-ed with MASK.
tls_flip() {
    _b=$(printf '%s' "$1" | awk -v at="$2" '{ print substr($0, at * 2 + 1, 2) }')
    _x=$(printf '%02x' $((0x$_b ^ $3)))
    printf '%s' "$1" | awk -v at="$2" -v x="$_x" '{ print substr($0, 1, at * 2) x substr($0, at * 2 + 3) }'
}

# ---- the authority report of an example ----

# tls_authority WANTED FILES...: `cancho authority --std --output json` of the files; the report must say that the network is held unnarrowed and why (docs/net-bounds.md section 2.4: the operator names the hosts and ports at
# run time), no foreign code, and exactly the labels in the file WANTED, one `"name", "argument": X` a line, in the report's order. What it bounds is everything else.
tls_authority() {
    _wanted=$1; shift
    if ! "$cancho" authority "$@" --std --output json > "$work/authority.json" 2> "$work/authority.err"; then
        fail "authority said: $(head -c 300 "$work/authority.err")"
        return 1
    fi
    printf '%s\n' '{' '  "bounded": false,' '  "unbounded_by": [' '    "net_in(\"\")"' '  ],' > "$work/authority.head"
    head -n 5 "$work/authority.json" | cmp -s - "$work/authority.head" || fail "the report does not begin by saying the network is held unnarrowed: $(head -n 6 "$work/authority.json")"
    grep -q '"foreign_symbols": \[\]' "$work/authority.json" || fail "the report names foreign symbols"
    grep '{ "name": ' "$work/authority.json" | sed 's/^ *{ "name": //; s/, "bounded".*//' > "$work/authority.labels"
    cmp -s "$work/authority.labels" "$_wanted" || fail "the labels are not the pinned ones: $(diff "$_wanted" "$work/authority.labels" | head -n 6)"
}

# ---- servers in the background ----

tls_ports=0
# tls_launch PORT EXE ARGS...: EXE in the background with `@PORT@` in an argument replaced (output to $_log, input from $_in, as `tls_start` set them).
tls_launch() {
    _p=$1; shift
    _n=$#; _i=0
    while [ "$_i" -lt "$_n" ]; do _a=$1; shift; case $_a in @PORT@) _a=$_p ;; esac; set -- "$@" "$_a"; _i=$((_i + 1)); done
    "$@" < "$_in" > "$_log" 2> "$_log.err" &
}
# tls_start LOG WORD [stdin FILE] -- EXE ARGS...: EXE started on a free port (an argument that is `@PORT@` is replaced by it), its standard output in LOG, its standard input FILE (default none), and waited for
# until LOG (or its standard error, LOG.err) says WORD (20 seconds at most). A server that is gone before it says WORD found its port taken and is started again on another (five times). Leaves $port and $spid.
tls_start() {
    _log=$1; _word=$2; shift 2
    _in=/dev/null
    if [ "$1" = stdin ]; then _in=$2; shift 2; fi
    [ "$1" = -- ] && shift
    _try=0
    while [ "$_try" -lt 5 ]; do
        tls_ports=$((tls_ports + 1))
        port=$(awk -v a="$(date +%s)" -v b="$$" -v c="$tls_ports" 'BEGIN { srand(a + b * 7 + c * 1013); print 20000 + int(rand() * 30000) }')
        : > "$_log"
        tls_launch "$port" "$@"
        spid=$!
        _waited=0
        while [ "$_waited" -lt 400 ]; do
            grep -q -F -- "$_word" "$_log" "$_log.err" && return 0
            kill -0 "$spid" 2> /dev/null || break
            sleep 0.05
            _waited=$((_waited + 1))
        done
        kill -KILL "$spid" 2> /dev/null
        wait "$spid" 2> /dev/null
        _try=$((_try + 1))
    done
    fail "the server never said '$_word': $(head -c 300 "$_log.err")"
    return 1
}

# tls_clients CONC SIZE [resume]: CONC connections of tests/programs/tls_many.cho (built as $work/many) to $port at once, each verifying the chain against the test CA and the name; every one must end ok, and, if SIZE
# is not empty, with that many bytes back and the same bytes for every one (`<slot> <code> <tag> <bytes> <sha256>` a line, then `done ok=N failed=N`). With `resume` a second round offers the tickets the first was sent, and
# $resumed is how many of the lines say `resumed`.
tls_clients() {
    "$work/many" 127.0.0.1 "$port" echo.lex-sys.test "$1" 65536 $3 < tests/vectors/tls/echo/ca.pem > "$work/clients.out" 2> "$work/clients.err"
    same "$(tail -n 1 "$work/clients.out")" "done ok=$1 failed=0" "the clients"
    if [ -n "$2" ]; then
        same "$(head -n "$1" "$work/clients.out" | awk -v s="$2" 'NR == 1 { h = $5 } { if ($4 != s) bad++; if ($5 != h) other++ } END { print bad + 0, other + 0 }')" "0 0" "every answer whole ($2 bytes) and the same bytes"
    fi
    resumed=$(grep -c ' resumed$' "$work/clients.out")
}

# tls_wait LOG WORD COUNT: until LOG has COUNT lines that contain WORD (30 seconds at most); the check fails if it does not.
tls_wait() {
    _w=0
    while [ "$(grep -c -F -- "$2" "$1")" -lt "$3" ]; do
        _w=$((_w + 1))
        if [ "$_w" -gt 600 ]; then fail "waited for $3 '$2' in the server's output, found $(grep -c -F -- "$2" "$1")"; return 1; fi
        sleep 0.05
    done
    return 0
}

# tls_end: the server is waited for (30 seconds at most, then killed); its status is in $status.
tls_end() {
    ( sleep 30; kill -KILL "$spid" ) > /dev/null 2>&1 &
    _wd=$!
    wait "$spid"
    status=$?
    kill "$_wd" > /dev/null 2>&1
    wait "$_wd" > /dev/null 2>&1
    return 0
}

# tls_kill: the server is not left behind.
tls_kill() { kill -KILL "$spid" > /dev/null 2>&1; wait "$spid" > /dev/null 2>&1; return 0; }
