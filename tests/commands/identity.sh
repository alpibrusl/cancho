# identity.sh -- the script of tests/commands/script_tests.cho for `conformance/identity.rs` (deleted): content hashes and canonical printing, what moves an identity and what does not.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. Everything is asked of the commands (`print`, `ingest`, `ids`) and nothing else.

print_dirs="tests/accept tests/reject examples examples/wordfreq examples/buffer examples/slab examples/modular examples/serve examples/fetch examples/report examples/collect packages/net-sockets packages/net-connect packages/agent-wire packages/http-request packages/http-response packages/x509 packages/tls benches examples/base64 examples/sort examples/seek benches/three std"
json_dirs="tests/accept tests/reject examples std packages/net-sockets packages/net-connect packages/agent-wire packages/http-request packages/http-response"

# files DIRS: the `.cho` files directly in each directory (the extension filter keeps a multi-file example out of the single-file walks), in order.
files() { for d in $1; do ls "$d"/*.cho 2> /dev/null; done | sort; }

# edition FILE: the `edition N;` line of the file, or nothing. The printer does not write it (an edition is not part of the tree).
edition() { grep '^edition ' "$1" | head -n 1; }

# same_ids SOURCE TEXTFILE: 0 when the identities of SOURCE (when it stands as a program) are those of TEXTFILE with SOURCE's edition line put in front; 1 when they differ; 2 when SOURCE
# has no identities (a reject fixture, or one that needs its sibling files), so nothing was compared.
same_ids() {
    "$cancho" ids "$1" --std > "$w/ids.before" 2> /dev/null || return 2
    { edition "$1"; cat "$2"; } > "$w/ids.text.cho"
    "$cancho" ids "$w/ids.text.cho" --std > "$w/ids.after" 2> "$w/ids.err" || { echo "  $1: the text that came back is not a program ids accepts"; return 1; }
    cmp -s "$w/ids.before" "$w/ids.after" || { echo "  $1: an identity changed"; return 1; }
    return 0
}

# `ids --std` reads the whole standard library (0.3 s), so the walks run a file at a time, eight processes together: `sh identity.sh --one MODE COMPILER SCRATCH FILE` prints a line for each
# contract the file broke, then `skipped` (it does not print), `checked` or `compared` (it also stood as a program for ids).
if [ "$1" = --one ]; then
    mode=$2
    cancho=$3
    f=$5
    w=$4/one.$$
    mkdir -p "$w"
    status=9
    if [ "$mode" = print ]; then
        if ! "$cancho" print "$f" > "$w/p1.cho" 2> /dev/null; then
            echo skipped
        elif ! "$cancho" print "$w/p1.cho" > "$w/p2.cho" 2> /dev/null; then
            echo "  $f: printed output does not parse"
            echo checked
        else
            cmp -s "$w/p1.cho" "$w/p2.cho" || echo "  $f: printing is not a fixed point"
            same_ids "$f" "$w/p1.cho"
            status=$?
        fi
    elif ! "$cancho" print "$f" > /dev/null 2>&1; then
        echo skipped
    elif ! "$cancho" print "$f" --output json > "$w/a.json" 2> /dev/null; then
        echo "  $f: a program that prints has no JSON"
        echo checked
    elif ! "$cancho" ingest "$w/a.json" > "$w/ingested.cho" 2> /dev/null; then
        echo "  $f: the JSON we wrote does not build"
        echo checked
    elif ! "$cancho" print "$w/ingested.cho" > /dev/null 2>&1; then
        echo "  $f: ingested text does not parse"
        echo checked
    else
        same_ids "$f" "$w/ingested.cho"
        status=$?
        if ! "$cancho" print "$w/ingested.cho" --output json > "$w/again.json" 2> /dev/null; then
            echo "  $f: the ingested text has no JSON"
        elif ! "$cancho" ingest "$w/again.json" > "$w/ingested2.cho" 2> /dev/null; then
            echo "  $f: the second JSON does not build"
        else
            cmp -s "$w/ingested.cho" "$w/ingested2.cho" || echo "  $f: the JSON round trip is not a fixed point"
        fi
    fi
    case $status in 0) echo compared ;; 1 | 2) echo checked ;; esac
    rm -rf "$w"
    exit 0
fi
cancho=$1
work=$2

# walk MODE DIRS: the contracts the files broke, then the counts.
walk() {
    files "$2" | xargs -P 8 -n 1 sh "$0" --one "$1" "$cancho" "$work" > "$work/walk.$1" 2> /dev/null
    checked=$(grep -c -E '^(checked|compared)$' "$work/walk.$1")
    compared=$(grep -c '^compared$' "$work/walk.$1")
    grep '^  ' "$work/walk.$1" | sort
    [ "$checked" -gt 100 ] && echo "the walk found the whole suite" || echo "the walk found only $checked files"
    [ "$compared" -gt 50 ] && echo "more than 50 files stand as programs for ids" || echo "only $compared files stand as programs for ids"
    echo "files that broke a contract: $(grep -c '^  ' "$work/walk.$1")"
}

echo "== printing preserves every identity and is idempotent"
walk print "$print_dirs"

echo "== ingesting JSON preserves every identity and is idempotent"
walk json "$json_dirs"

echo "== moving a function into a module changes no hash"
mkdir -p "$work/mod"
printf 'fn twice(n: int) -> [] int { return n + n; }\nfn caller() -> [] int { return twice(21); }\n' > "$work/mod/flat.cho"
printf 'import m;\nfn caller() -> [] int { return m.twice(21); }\n' > "$work/mod/user.cho"
printf 'module m;\npub fn twice(n: int) -> [] int { return n + n; }\n' > "$work/mod/lib.cho"
"$cancho" ids "$work/mod/flat.cho" 2>&1 | sort > "$work/mod/flat.ids"
"$cancho" ids "$work/mod/user.cho" "$work/mod/lib.cho" 2>&1 | sort > "$work/mod/modular.ids"
cmp -s "$work/mod/flat.ids" "$work/mod/modular.ids" && echo "the four hashes are the same" || { echo "a module reached the hash, and it must not"; cat "$work/mod/flat.ids" "$work/mod/modular.ids"; }

echo "== identity is content, not location"
body='fn double(n: int) -> [] int { return n + n; }'
main='fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi); release(io);
    return double(0);
}'
printf '%s\n%s\n' "$body" "$main" > "$work/alone.cho"
printf 'fn unrelated(n: int) -> [] int { return n * 3; }\n%s\n%s\n' "$body" "$main" > "$work/crowded.cho"
"$cancho" ids "$work/alone.cho" 2>&1 | grep double > "$work/alone.ids"
"$cancho" ids "$work/crowded.cho" 2>&1 | grep double > "$work/crowded.ids"
[ -s "$work/alone.ids" ] && echo "double has hashes" || echo "double has no hashes"
cmp -s "$work/alone.ids" "$work/crowded.ids" && echo "a unit hashes its content, not where it sits" || echo "the hashes differ"

echo "== a tuple emits the same object as the struct it replaces"
cat > "$work/declared.cho" <<'EOF'
res struct Pair { held: Box[int], tag: int }
fn make[&h](heap: &!h Heap, n: int) -> [heap] Pair {
    return Pair { held: box(heap, n), tag: n + 1 };
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(ffi); release(fs); release(io);
    var status = 0;
    borrow mut heap as &!h in {
        let p = make(h, 41);
        let Pair { held, tag } = p;
        status = unbox(h, held) + tag;
    }
    release(heap);
    return status - 83;
}
EOF
cat > "$work/anonymous.cho" <<'EOF'
fn make[&h](heap: &!h Heap, n: int) -> [heap] (Box[int], int) {
    return (box(heap, n), n + 1);
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(ffi); release(fs); release(io);
    var status = 0;
    borrow mut heap as &!h in {
        let p = make(h, 41);
        let (held, tag) = p;
        status = unbox(h, held) + tag;
    }
    release(heap);
    return status - 83;
}
EOF
"$cancho" build "$work/declared.cho" --emit obj -o "$work/declared.o" > /dev/null 2>&1 || echo "the declared pair does not build to an object"
"$cancho" build "$work/anonymous.cho" --emit obj -o "$work/anonymous.o" > /dev/null 2>&1 || echo "the tuple does not build to an object"
cmp -s "$work/declared.o" "$work/anonymous.o" && echo "the two object files are the same bytes" || echo "the object files differ"

echo "== ids are stable across runs and survive a body rewrite"
"$cancho" ids examples/rational.cho --std > "$work/r1.ids" 2>&1
"$cancho" ids examples/rational.cho --std > "$work/r2.ids" 2>&1
cmp -s "$work/r1.ids" "$work/r2.ids" && echo "hashing is a function of the program alone" || echo "two runs differ"
grep -q 'sig  harmonic' "$work/r1.ids" && echo "sig harmonic is listed" || echo "sig harmonic is missing"
grep -q 'type Rational' "$work/r1.ids" && echo "type Rational is listed" || echo "type Rational is missing"
perl -0pe 's/fn gcd\(a: int, b: int\) -> \[\] int \{\n    if b == 0 \{\n        return math\.abs\(a\);\n    \}\n    return gcd\(b, a % b\);\n\}/fn gcd(a: int, b: int) -> [] int {\n    if b != 0 {\n        return gcd(b, a % b);\n    }\n    return math.abs(a);\n}/' examples/rational.cho > "$work/rational.cho"
cmp -s examples/rational.cho "$work/rational.cho" && echo "the body rewrite did not apply"
"$cancho" ids "$work/rational.cho" --std > "$work/r3.ids" 2>&1
grep -E '^(sig |type )' "$work/r1.ids" > "$work/r1.sigs"
grep -E '^(sig |type )' "$work/r3.ids" > "$work/r3.sigs"
cmp -s "$work/r1.sigs" "$work/r3.sigs" && echo "a body rewrite moves no signature" || echo "a signature moved"
cmp -s "$work/r1.ids" "$work/r3.ids" && echo "the rewrite moved no body" || echo "it moves a body"

echo "== the operator set grew and no hash moved"
printf 'fn mix(a: int, b: int) -> [] bool {\n    return a + b * 2 - 1 < 10 && !(a == b);\n}\n' > "$work/mix.cho"
"$cancho" ids "$work/mix.cho" > "$work/mix.ids" 2>&1
grep -q 3b464a04c87cde626d82df771890f9fec4066346305c0701f79ffed7da920c39 "$work/mix.ids" && echo "the signature is where it was" || echo "a signature moved; the append-only rule was broken"
grep -q cdcb1e97cde120b3c3752cd21806f888d2885e76f14ba6fb43edcbb7cf154ff7 "$work/mix.ids" && echo "the body is where it was" || echo "a body moved; the append-only rule was broken"

echo "== a character literal hashes as its integer"
printf 'fn f(c: int) -> [] int { return c + 48 + 10; }\n' > "$work/numbers.cho"
printf "fn f(c: int) -> [] int { return c + '0' + '\\\\n'; }\\n" > "$work/characters.cho"
"$cancho" ids "$work/numbers.cho" > "$work/numbers.ids" 2>&1
"$cancho" ids "$work/characters.cho" > "$work/characters.ids" 2>&1
cmp -s "$work/numbers.ids" "$work/characters.ids" && echo "the two spellings are one program" || { echo "the spellings differ"; cat "$work/characters.ids"; }
