# conn_peer.sh -- the script of tests/commands/tls_scripts.cho for most of `conformance/conn_peer.rs` (docs/conn-peer.md): the other end of a connection over real sockets, and the address type (`std.addr`) against an oracle.
# Moved: the eleven slots that answer their own client's address and port, the builtin by itself (19 bytes), the authority report, the refusal to dial a host outside the bound, the address type over a corpus of 10,965 lines
# (its oracle was Rust's `std::net` parser and an RFC 5952 formatter: the answers are now a recording of them, tests/vectors/conn_peer_addr.want, written once by the Rust test's own generator, splitmix64 seed 0xc0ffee, and
# tests/vectors/conn_peer_addr.in is what it was fed), and the address API by hand. Left in Rust (conn_peer.rs): the IPv6 test (it needs a dual-stack socket made by the test and put under the program's listener) and the reset test
# (it needs `SO_LINGER` with a zero timeout on the client's socket). The programs are tests/programs/conn_peer_*.
# The client's own port, which the Rust tests took from `TcpStream::local_addr`, is read with `lsof` (the client is tests/programs/sk_client.cho, holding its connections open for a few seconds) where `lsof` exists; where it
# does not, the check keeps the rest (the address, the family, the layout, that the ports are distinct and the same after a slot is freed and taken again) and prints what a pass prints.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift legs of the Rust tests went with D2.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

P=tests/programs
V=tests/vectors

# local_ports PID PORT: the local ports of PID's connections to 127.0.0.1:PORT, one a line, sorted; nothing if `lsof` is not there or cannot say.
local_ports() { lsof -nP -a -p "$1" -iTCP@127.0.0.1:"$2" -Fn 2> /dev/null | sed -n 's/^n127\.0\.0\.1:\([0-9]*\)->.*/\1/p' | sort -n; }

begin "the address type agrees with the recorded oracle over a generated corpus (10,965 lines, 5,568 accepted)"
if tls_build addr "$P/conn_peer_addr.cho"; then
    same "$(tls_lines "$V/conn_peer_addr.in")" 10965 "the number of cases"
    same "$(grep -vc '^bad$' "$V/conn_peer_addr.want")" 5568 "the number of accepted cases"
    "$work/addr" < "$V/conn_peer_addr.in" > "$work/addr.got" 2> "$work/addr.err"
    same "$?" 0 "the program's status (a trap is a signal)"
    same "$(tls_lines "$work/addr.got")" 10965 "one answer a line"
    cmp -s "$work/addr.got" "$V/conn_peer_addr.want" || fail "the answers differ from the recorded ones: first at $(cmp "$work/addr.got" "$V/conn_peer_addr.want" 2>&1 | sed 's/.*line //')"
fi
end

begin "the address API does what the document says"
if tls_build api "$P/conn_peer_api.cho"; then
    "$work/api" > /dev/null 2>&1
    same "$?" 0 "the program's status"
fi
end

begin "knowing an address does not let a program dial it (the call traps)"
if tls_build dial "$P/conn_peer_dial.cho"; then
    ( "$work/dial" > /dev/null 2>&1; exit $? ) 2> /dev/null
    st=$?
    [ "$st" -gt 128 ] || fail "a trap is a signal, got status $st (2 would be a dial outside the bound, 3 an answer)"
fi
end

begin "asking for the peer adds nothing to the authority report"
report_of() { # NAME PEER_CALL
    sed "s/PORT/8080/g; s/COUNT/2/g; s/PEER_CALL/$2/" "$P/conn_peer_peers.tmpl" > "$work/authority_$1.cho"
    if ! "$cancho" authority --std --output json "$work/authority_$1.cho" > "$work/authority_$1.json" 2> "$work/authority.err"; then
        fail "authority said: $(head -c 300 "$work/authority.err")"
    fi
    # Everything the report says about authority; the list of pure functions is about the program's own functions (the call adds a few), not about what it can reach.
    sed -n '/"effects"/,/"pure"/p' "$work/authority_$1.json" | grep -v '"pure"' > "$work/authority_$1.part"
}
report_of with 'conns.peer(tb, slot)'
report_of without 'conns.Peered::Unavailable(0)'
cmp -s "$work/authority_with.part" "$work/authority_without.part" || fail "the call changes the report"
grep -q '"net_in"' "$work/authority_with.json" || fail "the report is not the real one"
grep -q '"ffi"' "$work/authority_with.json" && fail "foreign code"
for label in conn_accept heap io_write net_in; do
    grep -q "$label" "$work/authority_with.json" || fail "\`$label\` is expected"
done
# The labels it has, and no other: a new one would be a widening.
grep -o '"name": "[a-z_]*"' "$work/authority_with.json" | grep -c 'peer\|addr' | grep -qx 0 || fail "a label for the peer"
same "$(grep -c '"name": "' "$work/authority_with.json")" "$(grep -c '"name": "' "$work/authority_without.json")" "the number of labels"
end

begin "the builtin fills nineteen bytes and refuses fewer without writing"
if tls_build sk_client "$P/sk_client.cho"; then
    # The program names its port in its own text (a `Net("PORT")` is a literal), so it is made for the port that was chosen: the first one free is looked for by starting it, as tls_start does.
    n=0
    ok=0
    while [ "$n" -lt 5 ] && [ "$ok" = 0 ]; do
        n=$((n + 1))
        port=$(awk -v a="$(date +%s)" -v b="$$" -v c="$n" 'BEGIN { srand(a + b * 7 + c * 1013); print 20000 + int(rand() * 30000) }')
        sed "s/PORT/$port/g" "$P/conn_peer_raw.tmpl" > "$work/raw.cho"
        tls_build raw "$work/raw.cho" || break
        "$work/raw" > "$work/raw.out" 2> "$work/raw.err" &
        spid=$!
        w=0
        while [ "$w" -lt 100 ] && ! grep -q ready "$work/raw.err" && kill -0 "$spid" 2> /dev/null; do sleep 0.05; w=$((w + 1)); done
        if grep -q ready "$work/raw.err"; then ok=1; else wait "$spid" 2> /dev/null; fi
    done
    if [ "$ok" = 1 ]; then
        "$work/sk_client" "$port" "open 0
sleep 3000
close 0" > /dev/null 2>&1 &
        cpid=$!
        tls_wait "$work/raw.out" big 1
        mine=$(local_ports "$cpid" "$port")
        tls_end
        same "$status" 0 "the program's status"
        wait "$cpid" 2> /dev/null
        a=$(awk 'BEGIN { for (i = 0; i < 18; i++) printf "170 "; }' | sed 's/ $//')
        same "$(sed -n 1p "$work/raw.out")" "short 22 $a" "an 18-byte buffer is refused and not written"
        exact=$(sed -n 2p "$work/raw.out")
        hi=$(echo "$exact" | awk '{ print $20 }'); lo=$(echo "$exact" | awk '{ print $21 }')
        z="0 0 0 0 0 0 0 0 0 0 0 0"
        same "$exact" "exact 0 4 127 0 0 1 $z $hi $lo" "19 bytes are filled: the family, the address, twelve zeros, the port"
        same "$(sed -n 3p "$work/raw.out")" "empty 22" "an empty buffer is refused"
        same "$(sed -n 4p "$work/raw.out")" "big 0 4 127 0 0 1 $z $hi $lo 170 170 170 170 170 170" "only 19 bytes are written"
        peer_port=$((hi * 256 + lo))
        [ "$peer_port" -ge 1024 ] || fail "the peer port is $peer_port"
        if [ -n "$mine" ]; then same "$mine" "$peer_port" "the port is the client's own (lsof)"; fi
    else
        fail "the program never listened: $(head -c 300 "$work/raw.err")"
    fi
fi
end

begin "eleven slots answer their own client's address and port, a closed slot has none, and a slot freed and taken again answers the new client (11 clients)"
if [ -x "$work/sk_client" ]; then
    sed 's/COUNT/11/g; s/PEER_CALL/conns.peer(tb, slot)/' "$P/conn_peer_peers.tmpl" > "$work/peers.tmpl"
    # tls_start gives the server its port as a command-line argument; this one has it in its text, so the text is made again for each port tried.
    n=0
    ok=0
    while [ "$n" -lt 5 ] && [ "$ok" = 0 ]; do
        n=$((n + 1))
        port=$(awk -v a="$(date +%s)" -v b="$$" -v c="$n" 'BEGIN { srand(a + b * 7 + c * 1013); print 20000 + int(rand() * 30000) }')
        sed "s/PORT/$port/g" "$work/peers.tmpl" > "$work/peers.cho"
        tls_build peers "$work/peers.cho" || break
        "$work/peers" > "$work/peers.out" 2> "$work/peers.err" &
        spid=$!
        w=0
        while [ "$w" -lt 100 ] && ! grep -q ready "$work/peers.out" && kill -0 "$spid" 2> /dev/null; do sleep 0.05; w=$((w + 1)); done
        if grep -q ready "$work/peers.out"; then ok=1; else wait "$spid" 2> /dev/null; fi
    done
    if [ "$ok" = 1 ]; then
        "$work/sk_client" "$port" "open 0
open 1
open 2
open 3
open 4
open 5
open 6
open 7
open 8
open 9
open 10
sleep 1500
open 11
sleep 4000
close 0" > /dev/null 2>&1 &
        cpid=$!
        tls_wait "$work/peers.out" "slot 1 peer" 2
        mine=$(local_ports "$cpid" "$port")
        tls_end
        same "$status" 0 "the program's status"
        wait "$cpid" 2> /dev/null
        same "$(tls_lines "$work/peers.out")" 17 "the lines the program says"
        same "$(sed -n 1p "$work/peers.out")" ready "the first line"
        n=0
        : > "$work/slot.ports"
        while [ "$n" -lt 11 ]; do
            line=$(sed -n "$((n + 2))p" "$work/peers.out")
            p=$(echo "$line" | sed -n "s/^slot $n peer 127\.0\.0\.1:\([0-9]*\) key 4 2130706433\$/\1/p")
            [ -n "$p" ] || fail "slot $n: '$line'"
            echo "$p" >> "$work/slot.ports"
            n=$((n + 1))
        done
        same "$(sort -u "$work/slot.ports" | wc -l | tr -d ' ')" 11 "eleven distinct source ports"
        same "$(sed -n 13p "$work/peers.out")" "slot 99 none 9" "past the table"
        same "$(sed -n 14p "$work/peers.out")" "slot -1 none 9" "a negative slot"
        same "$(sed -n 15p "$work/peers.out")" "slot 0 none 9" "a closed slot has no peer"
        extra=$(sed -n "s/^slot 0 peer 127\.0\.0\.1:\([0-9]*\) key 4 2130706433\$/\1/p" "$work/peers.out" | tail -n 1)
        [ -n "$extra" ] || fail "the reused slot: '$(sed -n 16p "$work/peers.out")'"
        same "$(sed -n 16p "$work/peers.out")" "slot 0 peer 127.0.0.1:$extra key 4 2130706433" "the reused slot answers the new client"
        [ "$extra" != "$(sed -n 1p "$work/slot.ports")" ] || fail "the reused slot answers the old client's port"
        same "$(sed -n 17p "$work/peers.out")" "slot 1 peer 127.0.0.1:$(sed -n 2p "$work/slot.ports") key 4 2130706433" "slot 1 is untouched"
        # The client's own ports, twelve of them (the eleven and the extra), are the ones the program reported.
        if [ -n "$mine" ]; then
            { cat "$work/slot.ports"; echo "$extra"; } | sort -n > "$work/theirs"
            same "$mine" "$(cat "$work/theirs")" "the ports are the client's own (lsof)"
        fi
    else
        fail "the program never listened: $(head -c 300 "$work/peers.err")"
    fi
fi
end
