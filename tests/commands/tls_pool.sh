# tls_pool.sh -- the script of tests/commands/tls_scripts.cho for `conformance/tls_pool.rs` (deleted): the engine's work areas (docs/tls-memory.md section 8), with no network. A client engine and a server engine in one process,
# their bytes carried between them by tests/programs/tls_pool.cho, with as many areas as slots, with fewer, and with one. It checks that every connection is established and holding no area once it is idle, that messages of any
# size in any pieces come back byte for byte, that a start with no area free is refused `tls-pool` and starts nothing, that a connection that needs an area and finds none waits rather than fails, that the areas all come back,
# that a connection resting in its area (docs/record-in-place.md section 2) holds its core there and nothing else, and that no free area or free slot keeps a key, a secret or plaintext (`tls.residue`), with server tickets
# and resumption, with a client identity, and on a server that requires client certificates.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift leg of the Rust tests went with D2.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/tls_lib.sh

E=tests/vectors/tls/echo
C=tests/vectors/tls/client_auth

# pool_input WORD...: standard input of the driver: the root, the chain and the key of the test identity, a NUL byte between them; and for `clientcert` the clients' CAs (one bundle), the client's chain and its key, and two more
# chains for that key: a longer leaf and one larger than the engine keeps outside a work area (docs/tls-memory.md section 11).
pool_input() {
    {
        cat "$E/ca.pem"; printf '\0'; cat "$E/first/chain.pem"; printf '\0'; cat "$E/first/key.pem"
        for w in "$@"; do
            if [ "$w" = clientcert ]; then
                printf '\0'; cat "$C/pool_ca.pem" "$C/pool_ca2.pem"
                for f in pool_chain pool_key pool_chain2 pool_big_chain; do printf '\0'; cat "$C/$f.pem"; done
            fi
        done
    } > "$work/pool.in"
}

# pool SLOTS AREAS STEPS SEED WORD...: the driver run; its lines are in $work/pool.out. Every line must start `ok ` and the status be 0.
pool() {
    _slots=$1; _areas=$2; _steps=$3; _seed=$4; shift 4
    pool_input "$@"
    "$work/pool" "$_slots" "$_areas" "$_steps" "$_seed" "$@" < "$work/pool.in" > "$work/pool.out" 2> "$work/pool.err"
    _st=$?
    [ "$_st" = 0 ] || { fail "$_slots slots, $_areas areas, seed $_seed: the driver ended with $_st: $(head -c 300 "$work/pool.err")"; return 1; }
    [ "$(grep -vc '^ok ' "$work/pool.out")" = 0 ] || { fail "$_slots slots, $_areas areas, seed $_seed: $(grep -v '^ok ' "$work/pool.out" | head -n 3)"; return 1; }
    return 0
}

# says WORD: some line of the last run contains WORD.
says() { grep -q -F -- "$1" "$work/pool.out" || fail "$2: no line says '$1'"; }
# says_not WORD
says_not() { ! grep -q -F -- "$1" "$work/pool.out" || fail "$2: a line says '$1'"; }

begin "an engine with fewer work areas than slots serves every connection (the driver builds)"
tls_build pool tests/programs/tls_pool.cho $tls_package
end

if [ -x "$work/pool" ]; then

begin "every slot has an area: nothing can be refused, and nothing is (8 slots, 8 areas)"
if pool 8 8 40 1; then
    cp "$work/pool.out" "$work/all.out"
    for want in "handshakes, every slot established and holding no area" "steps echoed byte for byte, wrong 0" "a connection with data waiting keeps its area while another is served" "all slots dropped, all areas free"; do
        says "$want" "8/8"
    done
    says_not "tls-pool" "8/8"
fi
end

begin "fewer areas than slots: the pool bounds the handshakes in progress (8 slots, 3 areas)"
if pool 8 3 40 2; then
    says "serve and start with no area free are tls-pool, code -72" "8/3"
fi
end

begin "one area: a record half fed holds it, a second connection that needs it waits and is served once it is back (8 slots, 1 area)"
if pool 8 1 40 3; then
    cp "$work/pool.out" "$work/one.out"
    for want in "a record half fed holds the area" "a connection fed with no area free takes nothing and stays established" "the first completes and the area comes back" "the same bytes offered again once an area is free are taken and echoed" "the socket's end with no area free fails tls-peer-closed with its keys cleared" "after every step no free area or free core holds a byte, residue 0" "after the drops no area or core holds a byte, residue 0"; do
        says "$want" "8/1"
    done
fi
end

begin "resting: an idle connection keeps its area until another needs one, its core there and nothing else (every slot an area; one area for all)"
for out in all one; do
    cp "$work/$out.out" "$work/pool.out"
    for want in "rest: three connections in turn, at most two resting, each echoed from its area or its own core, residue 0" "rest: fewer allowed, the longest resting give their areas back cleared" "rest: a handshake takes a resting connection's area" "rest: a resting connection dropped gives its area back cleared" "rest: a resting connection whose socket ends fails tls-peer-closed and gives its area back" "rest: none allowed, the areas all come back"; do
        says "$want" "rest, $out"
    done
done
end

begin "one slot, one area; and other seeds, so the sizes and the pieces differ (1/1, 5/2, 16/4)"
pool 1 1 30 4
pool 5 2 80 5
pool 16 4 80 6
end

begin "server tickets and resumption, and a client engine with a client identity: the ticket randomness, the saved tickets and the identity's hedge are cleared with the rest (3 runs)"
if pool 8 3 30 7 tickets; then says "ok tickets: every slot saved one and resumed" "tickets"; fi
if pool 8 1 30 8 tickets mutual; then says "ok tickets: every slot saved one and resumed" "tickets, mutual"; fi
pool 5 2 30 9 mutual
end

begin "a server that requires client certificates: a verified connection gives its area back and keeps its leaf, the peer_ calls answer from it; a failure, a socket's end, a drop and the slot's next connection leave nothing of it (3 runs)"
for run in "4 4 10" "5 2 11" "1 1 12"; do
    set -- $run
    if pool "$1" "$2" 30 "$3" clientcert; then
        for want in "clientcert: every client verified, its connection gives its area back and keeps its identity, residue 0" "clientcert: traffic on verified connections, an area taken and given back each time, the identity the same" "clientcert: a verified connection that fails gives no identity, before and after its alert is taken" "clientcert: dropped, every area back and every leaf cleared, residue 0" "clientcert: a slot's next connection has its own leaf and nothing of the last one" "clientcert: a leaf larger than the room keeps its area and is read from it" "clientcert: the large leaves' areas come back cleared when dropped, residue 0" "clientcert: the store replaced before the client's Finished fails the handshake, its area cleared" "clientcert: after the drops no area or core holds a byte, residue 0"; do
            says "$want" "clientcert $1/$2"
        done
        if [ "$2" -lt "$1" ]; then
            says "with every area held by a connection with a large leaf a serve is tls-pool" "clientcert $1/$2"
            says "whose socket ends gives its leaf up, residue 0, with no area free 1" "clientcert $1/$2"
        elif [ "$1" -gt 1 ]; then
            says "whose socket ends gives its leaf up, residue 0, with no area free 0" "clientcert $1/$2"
        fi
    fi
done
end

fi
