# net.sh -- what the socket command tests share (docs/gate.md, G4 wave 4): sourced after tests/commands/script_lib.sh, which sets `cancho` (the compiler under
# test) and `work` (a scratch directory); run from the root of the repository.  `. tests/support/net.sh`
#
#   fetch_deps DIR LOCK STORE...   `vcs fetch` the locked packages into DIR (one directory per program, as the Rust helpers did)
#   build_exe NAME FILES...        build FILES with --std into $work/NAME (fails the check if it does not build)
#   pick_port                      a port of 30000 to 49999 in $port
#   spawn OUT EXE ARGS...          start a server in the background on a port of its own (the first argument), its standard output in OUT: $port and $pid;
#                                  a server that is gone within 0.4 seconds found the port taken, and is started again on another (five times)
#   await                          wait for the server to end by itself (10 seconds), else kill it; $sstatus is its exit status, or `killed`
#   client CMD...                  run a client until its status is not 3 (nothing listening yet; 10 seconds): stdout in $work/c.out, stderr in $work/c.err, $cstatus
#   sk SCRIPT                      run the scripted client (tests/programs/sk_client.cho) against $port
#   peer SCRIPT                    start the scripted client as a server on a port of its own ($port, $pid, output in $work/peer.out) and wait until it listens
#   peer_end                       wait for the peer to end (20 seconds) and print what it said
#   gen_file N SEED OUT            the bytes `gen` of the scripted client sends: byte i is 97 + (7 i + i / 13 + SEED) mod 26
#   gen_sum N SEED                 the length and checksum of those bytes, as the client prints them (` n=N c=C`)
#   same_file GOT WANT WHAT        two files hold the same bytes
#   traps EXE [ARGS...]            the program was ended by a signal (a trap), not by an exit
#   gnu TOOL                       succeeds when /usr/bin/TOOL is GNU coreutils

pick_port() { port=$((30000 + $(od -An -N2 -tu2 /dev/urandom | tr -d ' ') % 20000)); }

fetch_deps() { # DIR LOCK STORE [LOCK STORE]...
    _dir=$1; shift
    mkdir -p "$_dir"
    while [ $# -ge 2 ]; do
        "$cancho" vcs fetch --lock "$1" --store "$2" -o "$_dir" > /dev/null 2>&1 || fail "vcs fetch --lock $1 failed"
        shift 2
    done
}

build_exe() { # NAME FILES...
    _name=$1; shift
    if ! "$cancho" build "$@" --std -o "$work/$_name" > "$work/build-$_name.txt" 2>&1; then
        fail "building $_name said: $(head -c 300 "$work/build-$_name.txt")"
        return 1
    fi
}

spawn() { # OUT EXE ARGS...
    _out=$1; _exe=$2; shift 2
    _try=0
    while [ $_try -lt 5 ]; do
        pick_port
        "$_exe" "$port" "$@" > "$_out" 2>&1 &
        pid=$!
        sleep 0.4
        kill -0 $pid 2> /dev/null && return 0
        _try=$((_try + 1))
    done
    fail "the server never stayed up"
    return 1
}

await() {
    _i=0
    while kill -0 $pid 2> /dev/null && [ $_i -lt 200 ]; do sleep 0.05; _i=$((_i + 1)); done
    if kill -0 $pid 2> /dev/null; then
        kill -9 $pid 2> /dev/null
        wait $pid 2> /dev/null
        sstatus=killed
    else
        wait $pid
        sstatus=$?
    fi
}

client() { # CMD...
    _i=0
    while :; do
        "$@" > "$work/c.out" 2> "$work/c.err"
        cstatus=$?
        [ $cstatus != 3 ] && break
        _i=$((_i + 1))
        [ $_i -gt 500 ] && break
        sleep 0.02
    done
}

sk() { "$work/sk_client" "$port" "$1"; }

peer() { # SCRIPT
    _try=0
    while [ $_try -lt 5 ]; do
        pick_port
        : > "$work/peer.out"
        "$work/sk_client" "$port" "$1" serve > "$work/peer.out" 2>&1 &
        pid=$!
        _i=0
        while [ $_i -lt 60 ]; do
            grep -q '^listening$' "$work/peer.out" 2> /dev/null && return 0
            kill -0 $pid 2> /dev/null || break
            sleep 0.05
            _i=$((_i + 1))
        done
        kill -9 $pid 2> /dev/null
        wait $pid 2> /dev/null
        _try=$((_try + 1))
    done
    fail "the peer never listened"
    return 1
}

peer_end() {
    _i=0
    while kill -0 $pid 2> /dev/null && [ $_i -lt 400 ]; do sleep 0.05; _i=$((_i + 1)); done
    kill -9 $pid 2> /dev/null
    wait $pid 2> /dev/null
    grep -v '^listening$' "$work/peer.out"
}

gen_file() { # N SEED OUT
    awk -v n="$1" -v s="$2" 'BEGIN { for (i = 0; i < n; i++) printf "%c", 97 + (7 * i + int(i / 13) + s) % 26 }' > "$3"
}
gen_sum() { # N SEED
    awk -v n="$1" -v s="$2" 'BEGIN { c = 0; for (i = 0; i < n; i++) c = (c * 131 + 97 + (7 * i + int(i / 13) + s) % 26) % 1000000007; print " n=" n " c=" c }'
}

same_file() { # GOT WANT WHAT
    cmp -s "$1" "$2" || fail "$3: the bytes differ ($(wc -c < "$1" | tr -d ' ') bytes, wanted $(wc -c < "$2" | tr -d ' '))"
}

traps() { # EXE [ARGS...]
    { "$@" > /dev/null 2>&1; } 2> /dev/null
    _s=$?
    [ $_s -gt 128 ] && [ $_s -lt 160 ]
}

gnu() { "/usr/bin/$1" --version 2> /dev/null < /dev/null | grep -q 'GNU coreutils'; }

# --- program templates and the scripted client in the background (sockets.sh, udp.sh) ---
#   make_src KIND FILE NAME [PORT [BOUND [OTHER [EXPECT]]]]   the section NAME of the template file FILE as $work/NAME.cho; KIND io (main narrows the network to PORT and calls
#                                run(bound, io)), dial (narrows to BOUND), open (the whole network, run(net, io)) or raw (the section is the whole program);
#                                PORT, BOUND, OTHER and EXPECT are replaced in the text
#   make_prog ...              make_src, then build it into $work/NAME (fails the check if it does not build)
#   waitfile FILE WORD [N]     wait (N times 50 ms, 200 by default) until a line of FILE is WORD
#   sync_open                  the fifo the scripted client's `wait` reads, held open on descriptor 4;  go  writes one byte to it
#   sk_bg SCRIPT               start the scripted client (a client of $port) in the background, its standard input the fifo; sk_end waits for it ($got is what it printed)
#   peer_on PORT SCRIPT        `peer` on a given port
make_src() {
    _kind=$1; _file=$2; _n=$3; _p=${4:-0}; _b=${5:-}; _o=${6:-0}; _e=${7:-0}
    {
        case $_kind in raw) ;; *) printf 'edition 5;\nimport std.io;\n' ;; esac
        awk -v n="$_n" '/^#### /{ on = ($2 == n); next } on' "$_file"
        case $_kind in
            io) printf '\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi); release(fs); release(heap); release(args); release(clock);\n    let bound = narrow(net, "PORT");\n    return run(bound, io);\n}\n' ;;
            dial) printf '\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi); release(fs); release(heap); release(args); release(clock);\n    let bound = narrow(net, "BOUND");\n    return run(bound, io);\n}\n' ;;
            open) printf '\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock } = split(world);\n    release(ffi); release(fs); release(heap); release(args); release(clock);\n    return run(net, io);\n}\n' ;;
        esac
    } | sed -e "s/OTHER/$_o/g" -e "s/BOUND/$_b/g" -e "s/PORT/$_p/g" -e "s/EXPECT/$_e/g" > "$work/$_n.cho"
}
make_prog() {
    make_src "$@"
    build "$work/$3.cho" "$3"
}

waitfile() { # FILE WORD [N]
    _i=0
    while [ $_i -lt "${3:-200}" ]; do
        grep -qx "$2" "$1" 2> /dev/null && return 0
        sleep 0.05
        _i=$((_i + 1))
    done
    fail "$1 never said $2"
    return 1
}

sync_open() { rm -f "$work/sync"; mkfifo "$work/sync"; exec 4<> "$work/sync"; }
go() { printf . >&4; }
sk_bg() { # SCRIPT
    "$work/sk_client" "$port" "$1" < "$work/sync" > "$work/sk.out" 2>&1 &
    skpid=$!
}
sk_end() {
    _i=0
    while kill -0 $skpid 2> /dev/null && [ $_i -lt 400 ]; do sleep 0.05; _i=$((_i + 1)); done
    kill -9 $skpid 2> /dev/null
    wait $skpid 2> /dev/null
    got=$(cat "$work/sk.out")
}

peer_on() { # PORT SCRIPT
    port=$1
    : > "$work/peer.out"
    "$work/sk_client" "$port" "$2" serve > "$work/peer.out" 2>&1 &
    pid=$!
    _i=0
    while [ $_i -lt 100 ]; do
        grep -q '^listening$' "$work/peer.out" 2> /dev/null && return 0
        sleep 0.05
        _i=$((_i + 1))
    done
    fail "the peer never listened on $port"
    return 1
}
