# backends.sh -- the script of tests/commands/script_tests.cho for `conformance/backends.rs` (deleted): programs built by the compiler and run, that no walk of tests/run.cho covers.
# The rest of backends.rs built a fixture of tests/accept or examples and compared its standard output with a string; each of those fixtures declares that same output in its `//~ STDOUT`
# lines (and its input in `//~ STDIN`), so the accept and examples walks of tests/run.cho already say it (docs/gate.md, G4 wave 3). What no walk reads is benches/, so it is here, with the
# programs that need arguments, or a live peer. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
root=$(pwd)

# built NAME SOURCE [FILE...]: the program NAME in $work, or a line saying the build failed.
built() {
    name=$1
    shift
    "$cancho" build "$@" --std -o "$work/$name" > "$work/$name.out" 2> "$work/$name.err" || {
        echo "$name: build failed"
        cat "$work/$name.err"
        return 1
    }
}

# same NAME WANT: the output of the last run of NAME is exactly WANT (a printf format).
same() {
    printf -- "$2" > "$work/$1.want"
    if cmp -s "$work/$1.got" "$work/$1.want"; then echo "$1: ok"; else echo "$1: DIFFERENT"; cat "$work/$1.got"; fi
}

# plain NAME SOURCE WANT: build, run with no input and no arguments, exit 0 and exactly WANT.
plain() {
    built "$1" "$2" || return
    "$work/$1" > "$work/$1.got" 2> "$work/$1.err"
    status=$?
    [ $status -eq 0 ] || echo "$1: exit $status"
    same "$1" "$3"
}

echo "== the benchmark kernels build and print what they printed on the other backend"
plain sum_checked benches/sum_checked.cho ''
plain fib_checked benches/fib_checked.cho ''
plain sum_wrapping benches/sum_wrapping.cho ''
plain fib_wrapping benches/fib_wrapping.cho ''
plain sieve_checked benches/sieve_checked.cho ''
plain sieve_wrapping benches/sieve_wrapping.cho ''
plain scan_checked benches/scan_checked.cho ''
plain scan_wrapping benches/scan_wrapping.cho ''
plain reduce_checked benches/reduce_checked.cho ''
plain reduce_wrapping benches/reduce_wrapping.cho ''
plain mandelbrot benches/three/mandelbrot.cho '39690297\n'
plain purity benches/three/purity.cho '-7463529374017724416\n'
plain three_sieve benches/three/sieve.cho '6057\n'
plain layout_aos benches/layout/aos.cho '32000000\n'
plain layout_soa benches/layout/soa.cho '32000000\n'
plain layout_ints benches/layout/ints.cho '192000000\n'
plain layout_rgb benches/layout/rgb.cho '192000000\n'
plain spectral benches/game/spectral.cho '1.274219991\n'

echo "== the games given arguments and input"
built fannkuch benches/game/fannkuch.cho && {
    "$work/fannkuch" 8 > "$work/fannkuch.got"
    echo "fannkuch exit $?"
    same fannkuch '1616\nPfannkuchen(8) = 22\n'
}
built binarytrees benches/game/binarytrees.cho && {
    "$work/binarytrees" 8 > "$work/binarytrees.got"
    echo "binarytrees exit $?"
    same binarytrees 'stretch tree of depth 9\t check: 1023\n256\t trees of depth 4\t check: 7936\n64\t trees of depth 6\t check: 8128\n16\t trees of depth 8\t check: 8176\nlong lived tree of depth 8\t check: 511\n'
}
built fasta benches/game/fasta.cho && {
    "$work/fasta" 10 > "$work/fasta.got"
    echo "fasta exit $?"
    same fasta '>ONE Homo sapiens alu\nGGCCGGGCGCGGTGGCTCAC\n>TWO IUB ambiguity codes\ncttBtatcatatgctaKggNcataaaSatg\n>THREE Homo sapiens frequency\ntaaatcttgtgcttcgttagaagtctcgactacgtgtagcctagtgtttg\n'
}
built revcomp benches/game/revcomp.cho && {
    "$work/revcomp" < benches/game/fasta-1000.txt > "$work/revcomp.got"
    echo "revcomp exit $?"
    cmp -s "$work/revcomp.got" benches/game/revcomp-1000.txt && echo "revcomp: the published answer" || echo "revcomp: DIFFERENT"
}


echo "== a program with its own free and malloc links and runs beside the arena and the boxes"
cat > "$work/own_free.cho" <<'EOF'
edition 5;
extern fn strdup[&f, &s](ffi: &f Ffi("libc"), s: &s [byte]) -> [ffi("libc")] c_ptr;
extern fn free[&f](ffi: &f Ffi("libc"), p: c_ptr) -> [ffi("libc")] int;
extern fn malloc[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] c_ptr;

fn copy_and_free[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    let p = strdup(ffi, "x\0");
    free(ffi, p);
    let q = malloc(ffi, 16);
    free(ffi, q);
    return 0;
}

fn boxed[&h](heap: &!h Heap) -> [heap] int {
    let b = box_slice(heap, 5, byte_of(7));
    return unbox_slice(heap, b);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(fs); release(args); release(net); release(clock);
    let libc = narrow(ffi, "libc");
    var status = 1;
    region a {
        let kept = alloc_slice[a](4, byte_of(7));
        borrow libc as &f in { status = copy_and_free(f); }
        status = status + len(kept);
    }
    borrow mut heap as &!h in { status = status + boxed(h); }
    release(libc);
    release(heap);
    return status;
}
EOF
built own_free "$work/own_free.cho" && {
    "$work/own_free" > /dev/null 2>&1
    echo "own free and malloc: exit $? (0 from the libc round trip, 4 from the arena, 5 from the box)"
}

# fetched DIR LOCK STORE [LOCK STORE...]: the packages of the locks, written into DIR by `vcs fetch`.
fetched() {
    dir=$1
    shift
    mkdir -p "$dir"
    while [ $# -ge 2 ]; do
        "$cancho" vcs fetch --lock "$1" --store "$2" -o "$dir" > /dev/null 2>> "$dir.err" || echo "fetch of $1 failed: $(cat "$dir.err")"
        shift 2
    done
}
echo "== every example that declares sockets builds, with the packages it fetches"
ns=packages/net-sockets/.cancho-vcs
nc=packages/net-connect/.cancho-vcs
hr=packages/http-response/.cancho-vcs
hq=packages/http-request/.cancho-vcs
aw=packages/agent-wire/.cancho-vcs
fetched "$work/f_collect" examples/collect/request.lock $hq
built collect examples/collect/collect.cho "$work"/f_collect/*.cho && echo "collect: built"
fetched "$work/f_report" examples/report/connect.lock $nc examples/report/response.lock $hr
built report examples/report/report.cho "$work"/f_report/*.cho && echo "report: built"
fetched "$work/f_vsock" examples/vsock/net.lock $ns examples/vsock/connect.lock $nc examples/vsock/wire.lock $aw
built vsock examples/vsock/vsock.cho "$work"/f_vsock/*.cho && echo "vsock: built"
fetched "$work/f_guest" examples/agent_guest/connect.lock $nc examples/agent_guest/response.lock $hr examples/agent_guest/wire.lock $aw
built agent_guest examples/agent_guest/agent_guest.cho "$work"/f_guest/*.cho && echo "agent_guest: built"
fetched "$work/f_supervisor" examples/agent_supervisor/request.lock $hq
built agent_supervisor examples/agent_supervisor/agent_supervisor.cho "$work"/f_supervisor/*.cho && echo "agent_supervisor: built"
fetched "$work/f_serve" examples/serve/net.lock $ns
built serve examples/serve/serve.cho "$work"/f_serve/*.cho && echo "serve: built"
fetched "$work/f_fetch" examples/fetch/connect.lock $nc examples/fetch/response.lock $hr
built fetch examples/fetch/fetch.cho "$work"/f_fetch/*.cho && echo "fetch: built"


# alive PID: 0 if the process is still there after a moment (a server that could not bind its port ends at once), 1 if it ended.
alive() {
    sleep 0.3
    kill -0 "$1" 2> /dev/null
}

# listener_source PORT: a program that binds PORT, listens, accepts one connection and answers 0 when it got one.
listener_source() {
    cat > "$work/listener.cho" <<EOF2
edition 2;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io); release(fs); release(heap); release(args); release(ffi);
    let bound = narrow(net, "$1");
    var status = 1;
    borrow bound as &n in {
        let listener = bind(n, $1);
        if listener >= 0 {
            listen(listener, 1);
            let conn = accept(listener);
            if conn >= 0 {
                status = 0;
            }
        }
    }
    release(bound);
    return status;
}
EOF2
}

# start_listener: build and start the listener on a port that is free (the first of a few from the process number that it manages to bind); sets $port and $pid, or $pid to nothing.
start_listener() {
    pid=
    attempt=0
    while [ $attempt -lt 6 ]; do
        port=$((20000 + ($$ * 7 + attempt * 131) % 30000))
        listener_source $port
        built listener "$work/listener.cho" || return
        "$work/listener" &
        pid=$!
        alive $pid && return
        attempt=$((attempt + 1))
    done
    echo "listener: no free port"
    pid=
}

built tcp_probe tests/programs/tcp_probe.cho

echo "== a program that binds and accepts takes a real connection"
start_listener
if [ -n "$pid" ]; then
    "$work/tcp_probe" 127.0.0.1 $port ping > /dev/null 2>&1
    echo "client: exit $?"
    wait $pid
    echo "listener: exit $?"
fi

echo "== a program that connects reaches a real listener"
start_listener
if [ -n "$pid" ]; then
    cat > "$work/client.cho" <<EOF2
edition 2;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net } = split(world);
    release(io); release(fs); release(heap); release(args); release(ffi);
    let bound = narrow(net, "127.0.0.1:$port");
    var fd = 0 - 1;
    borrow bound as &n in {
        fd = connect(n, "127.0.0.1", $port);
    }
    release(bound);
    if fd >= 0 {
        return 0;
    }
    return 1;
}
EOF2
    built client "$work/client.cho" && {
        "$work/client"
        echo "client: exit $?"
    }
    wait $pid
    echo "listener: exit $?"
fi

echo "== the results stub answers a request over loopback and logs it"
fetched "$work/f_stub" examples/results_stub/net.lock $ns
built results_stub examples/results_stub/results_stub.cho "$work"/f_stub/*.cho && {
    attempt=0
    pid=
    while [ $attempt -lt 6 ]; do
        port=$((20000 + ($$ * 11 + attempt * 137) % 30000))
        "$work/results_stub" --listen "127.0.0.1:$port" > "$work/stub.log" 2>&1 &
        pid=$!
        alive $pid && break
        pid=
        attempt=$((attempt + 1))
    done
    if [ -z "$pid" ]; then
        echo "results_stub: no free port"
    else
        request=$(printf 'POST /report HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 17\r\nConnection: close\r\n\r\n{"hello":"world"}')
        reply=$("$work/tcp_probe" 127.0.0.1 $port "$request" 2> /dev/null)
        kill $pid
        wait $pid 2> /dev/null
        case $reply in "HTTP/1.1 200 OK"*) echo "status line: HTTP/1.1 200 OK" ;; *) echo "status line: wrong: $reply" ;; esac
        case $reply in *"Content-Type: application/json"*) echo "json content type: yes" ;; *) echo "json content type: no" ;; esac
        case $reply in *'{"ok":true,"stub":true}') echo "fixed body: yes" ;; *) echo "fixed body: no" ;; esac
        for want in 'results-stub: listening' 'req="POST /report HTTP/1.1"' 'body_len=17' 'body_preview="{"hello":"world"}"'; do
            grep -qF -- "$want" "$work/stub.log" && echo "logged: $want" || echo "NOT logged: $want"
        done
    fi
}
