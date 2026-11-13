# authority.sh -- the script of tests/commands/script_tests.cho for `conformance/authority.rs` (deleted): the authority report, and what it can and cannot prove about a program
# (docs/authority.md, docs/budget.md, docs/purity.md, docs/under-a-grant.md). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# Every check prints `ok NAME` when it holds and `FAILS NAME` when it does not, so the transcript is the same on every machine.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
root=$(pwd)
work=$2
cd "$work" || exit 1

has() { if grep -qF -- "$2" "$1"; then echo "ok $3"; else echo "FAILS $3"; fi; }
hasnt() { if grep -qF -- "$2" "$1"; then echo "FAILS $3"; else echo "ok $3"; fi; }
authority() { # OUT ARGS...
    out=$1
    shift
    "$cancho" authority "$@" > "$out" 2> "$out.err" || { echo "authority failed: $(cat "$out.err")"; }
}

# docs/authority.md section 2: the report names what a program performs; three examples whose surfaces differ, so this fails if the union is taken over the wrong set.
echo "== names what a program performs"
authority tally.out "$root/examples/tally.cho" --std
has tally.out io_read "tally performs io_read"
has tally.out io_write "tally performs io_write"
for what in "the filesystem" "the heap" "the command line" "foreign code"; do has tally.out "$what" "tally never touches $what"; done
authority hello.out "$root/examples/hello.cho" --std
has hello.out io_write "hello performs io_write"
for what in "the filesystem" "the heap" "the command line" "foreign code"; do has hello.out "$what" "hello never touches $what"; done
authority pipeline.out "$root/examples/pipeline.cho" --std
for what in 'ffi("libc")' io_write labs "the filesystem"; do has pipeline.out "$what" "pipeline: $what"; done

# 2.1, and the bug that section records: examples/lines.cho reads argv in main's own body.
echo "== sees what main does itself"
authority lines.out "$root/examples/lines.cho" --std
has lines.out args "lines reads argv in main"
hasnt lines.out "the command line" "lines does touch the command line"

# 2.2: an absent label is a proof rather than an absence of evidence.
echo "== an unused capability never appears"
cat > quiet.cho << 'EOF'
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(ffi); release(fs); release(heap); release(io);
    return 0;
}
EOF
authority quiet.out quiet.cho
has quiet.out "performs nothing" "quiet performs nothing"
for what in "the console" "the filesystem" "the heap" "the command line" "foreign code"; do has quiet.out "$what" "quiet: $what is untouched"; done

# docs/budget.md section 5: the report as data.
echo "== a machine-readable form"
authority tour.json "$root/examples/tour.cho" --std --output json
authority tour.txt "$root/examples/tour.cho" --std
[ "$(tr -cd '{' < tour.json | wc -c)" = "$(tr -cd '}' < tour.json | wc -c)" ] && echo "ok balanced braces" || echo "FAILS balanced braces"
[ "$(tr -cd '[' < tour.json | wc -c)" = "$(tr -cd ']' < tour.json | wc -c)" ] && echo "ok balanced brackets" || echo "FAILS balanced brackets"
for key in '"effects"' '"labels"' '"never_touches"' '"foreign_symbols"' '"fs_read"' '"labs"' '"never_touches": [' '"other programs"'; do has tour.json "$key" "tour.json has $key"; done
has tour.json '{ "name": "fs_read", "argument": "/tmp", "bounded": true, "confinement": "lexical" }' "the narrowing survives, and says it is a lexical prefix"
has tour.json '{ "name": "heap", "argument": null, "bounded": true }' "an unnarrowed label carries an explicit null"
hasnt tour.json '"the filesystem"' "tour touches the filesystem, so it cannot be proven out of reach"
has tour.txt 'fs_read("/tmp")' "the human form is unchanged by the flag"

# docs/purity.md section 2: the predicate, checked against cases chosen to break it; each case is a whole program, because only what main reaches is reported.
echo "== purity is the row plus what a reference may do"
prologue='    let Split { io, ffi, fs, heap, args } = split(world);'
case0() { cat << EOF
fn double(x: int) -> [] int { return x + x; }
fn main(world: World) -> [] int {
$prologue
    release(args); release(heap); release(fs); release(ffi); release(io);
    return double(0);
}
EOF
}
case1() { cat << EOF
fn first[&r](s: &r [byte]) -> [] int { return int_of(s[0]); }
fn main(world: World) -> [] int {
$prologue
    release(args); release(heap); release(fs); release(ffi); release(io);
    return first("a") - 97;
}
EOF
}
case2() { cat << EOF
fn poke[&r](s: &!r [byte]) -> [] int { s[0] = byte_of(1); return 0; }
fn main(world: World) -> [] int {
$prologue
    release(args); release(heap); release(fs); release(ffi); release(io);
    var out = 0;
    region a { let s = alloc_slice[a](2, byte_of(0)); out = poke(s); }
    return out;
}
EOF
}
case3() { cat << EOF
fn pair[&r](t: (int, &!r [byte])) -> [] int { return t.0; }
fn main(world: World) -> [] int {
$prologue
    release(args); release(heap); release(fs); release(ffi); release(io);
    var out = 0;
    region a { let s = alloc_slice[a](2, byte_of(0)); out = pair((0, s)); }
    return out;
}
EOF
}
case4() { cat << EOF
fn shout[&i](io: &!i Io) -> [io_write] int { putchar(io, 10); return 0; }
fn main(world: World) -> [] int {
$prologue
    release(args); release(heap); release(fs); release(ffi);
    var out = 0;
    borrow mut io as &!i in { out = shout(i); }
    release(io);
    return out;
}
EOF
}
case5() { cat << EOF
fn scratch(n: int) -> [] int {
    var total = 0;
    region a { let s = alloc_slice[a](n, byte_of(1)); total = len(s); }
    return total;
}
fn main(world: World) -> [] int {
$prologue
    release(args); release(heap); release(fs); release(ffi); release(io);
    return scratch(4) - 4;
}
EOF
}
# (case, function, pure?) -- the report's `pure` list names the functions the predicate holds for.
for row in "0 double yes" "1 first yes" "2 poke no" "3 pair no" "4 shout no" "5 scratch yes"; do
    set -- $row
    "case$1" > "case$1.cho"
    authority "case$1.json" "case$1.cho" --output json
    line=$(grep '"pure":' "case$1.json")
    if [ "$3" = yes ]; then
        case $line in *"\"$2\""*) echo "ok $2 is pure" ;; *) echo "FAILS $2 is pure: $line" ;; esac
    else
        case $line in *"\"$2\""*) echo "FAILS $2 is not pure: $line" ;; *) echo "ok $2 is not pure" ;; esac
    fi
done


# The programs that import fetched packages: `net.sockets` (packages/net-sockets/) fetched fresh by `vcs fetch` into one file.
mkdir fetched
"$cancho" vcs fetch --lock "$root/examples/serve/net.lock" --store "$root/packages/net-sockets/.cancho-vcs" -o fetched > fetch.out 2>&1 || { echo "vcs fetch failed:"; cat fetch.out; }
echo "fetched $(ls fetched | grep -c '\.cho$') file"
sockets=$(ls fetched/*.cho | head -1)

# docs/under-a-grant.md section 2: a network program reports no network (a gap, pinned so it cannot close silently).
echo "== a network program reports no network"
authority serve.json "$root/examples/serve/serve.cho" "$sockets" --std --output json
has serve.json '"effects": ["args", "ffi"]' "serve reports only args and ffi"
for sym in socket bind listen accept; do has serve.json "\"$sym\"" "serve lists $sym"; done

# Section 5.1: the report fails closed, in both directions, and the prose form says so first.
echo "== the report fails closed"
head -2 serve.json | tail -1
authority cut.json "$root/examples/cut/cut.cho" --std --output json
head -2 cut.json | tail -1
authority serve.txt "$root/examples/serve/serve.cho" "$sockets" --std
head -1 serve.txt | cut -c1-9
has serve.txt 'ffi("libc")    <- unbounded' "the prose report marks the label"

# Section 3: the symbol list is a proof about names; six lines open a socket and report only `syscall`.
echo "== the symbol list is a proof about names"
cat > opaque.cho << 'EOF'
extern fn syscall[&f](ffi: &f Ffi("libc"), n: int, a: int, b: int, c: int)
    -> [ffi("libc")] int;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    var r = 0;
    let libc = narrow(ffi, "libc");
    borrow libc as &f in { r = syscall(f, 41, 2, 1, 0); }
    release(libc);
    if r < 0 { return 1; }
    return 0;
}
EOF
authority opaque.json opaque.cho --output json
has opaque.json '"effects": ["ffi"]' "opaque reports only ffi"
has opaque.json '"foreign_symbols": ["syscall"]' "and names only syscall"

# docs/authority.md section 3: foreign_symbols is reachability-pruned.
echo "== foreign_symbols names only what the program calls"
cat > two_externs.cho << 'EOF'
extern fn used[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;
extern fn unused[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    var r = 0;
    let libc = narrow(ffi, "libc");
    borrow libc as &f in { r = used(f, 41); }
    release(libc);
    return r;
}
EOF
authority two_externs.json two_externs.cho --output json
has two_externs.json '"foreign_symbols": ["used"]' "unused is declared but never called, so it is not reported"

# Sections 2 and 4: the dimension that does work -- the path survives into the report.
echo "== the filesystem dimension is enforceable"
authority fs.json "$root/tests/accept/fs_narrowed.cho" --std --output json
has fs.json '"name": "fs_write", "argument": "/' "the report carries the path prefix, not just the dimension"
exit 0
