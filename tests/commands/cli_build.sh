# cli_build.sh -- the script of tests/commands/script_tests.cho for two tests of `conformance/selfhost/cli.rs` (deleted): `the_command_line_in_cancho_builds_like_cancho_build` and
# `..._runs_like_cancho_run`. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# Their oracle was the Rust `cancho build` and `cancho run` on the programs of the repository that build without the standard library and finish by themselves in five seconds; the Rust binary was
# run once (2026-11-17) and tests/vectors/cli/build.txt holds what each program did when built by it and run (the exit status and the checksum of its standard output), and what `cancho run` of it
# said (status, checksums of the standard output and of the standard error without the linker's `ld: warning:` lines). The programs are not the same bytes, the compilers are not: what is compared is
# what they do. A program the compiler in cancho has no code for is a status 3 and is counted. The recorded programs are those of tests/commands/cli_lib.sh's corpus that have not changed since.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1
mkdir -p "$work/b"

# run5 EXE OUT ERR: EXE with no input for at most five seconds; its status in $s (`timeout` if it did not finish).
run5() {
    ( "$1" < /dev/null > "$2" 2> "$3"; exit $? ) 2> /dev/null &
    _p=$!
    _n=0
    while kill -0 "$_p" 2> /dev/null && [ "$_n" -lt 50 ]; do sleep 0.1; _n=$((_n + 1)); done
    if kill -0 "$_p" 2> /dev/null; then pkill -KILL -P "$_p" 2> /dev/null; kill -KILL "$_p" 2> /dev/null; wait "$_p" 2> /dev/null; s=timeout; else { wait "$_p"; s=$?; } 2> /dev/null; fi
}
sans_ld() { grep -v '^ld: warning:' "$1" | cks; }

# --- the_command_line_in_cancho_builds_like_cancho_build
begin the_command_line_in_cancho_builds_like_cancho_build
# answer CASE KIND: `b`: built and run: `STATUS CKSUM`, or `-` if it was not built or did not finish; `r`: `cancho run`: `STATUS OUT ERR`.
b_answer() {
    rm -rf "$work/b"/*
    cp "$root/$1" "$work/b/x.cho"
    ( cd "$work/b" || exit 1
      if ! "$cancho" build x.cho -o prog.out > /dev/null 2>&1; then echo "nobuild"; exit 0; fi
      left=$(ls | LC_ALL=C sort | tr '\n' ' ')
      [ "$left" = "prog.out x.cho " ] || echo "left over: $left"
      run5 ./prog.out o.txt e.txt
      if [ "$s" = timeout ]; then echo "timeout"; else echo "$s $(ck o.txt)"; fi )
}
r_answer() {
    rm -rf "$work/b"/*
    cp "$root/$1" "$work/b/x.cho"
    ( cd "$work/b" || exit 1
      "$cancho" run x.cho < /dev/null > o.txt 2> e.txt
      echo "$? $(ck o.txt) $(sans_ld e.txt)"
      ls | grep -q cancho-run && echo "left over: $(ls)" )
}
if recording; then
    # The programs the Rust command builds and that finish: the kind `b` for each, and `r` for the same.
    corpus | grep -v '/selfhost/\|/reject/' | while read -r case; do
        ( cd "$work/b" && rm -rf ./* && cp "$root/$case" x.cho && "$cancho" build x.cho -o rust.out > /dev/null 2>&1 ) || continue
        ( cd "$work/b" && run5 ./rust.out o.txt e.txt; [ "$s" != timeout ] ) || continue
        echo "$case|$(ck "$root/$case")|b"
        echo "$case|$(ck "$root/$case")|r"
    done > "$work/cases.txt"
else
    vector_cases build
fi
: > "$work/got.txt"
while IFS='|' read -r case sum kind; do
    case $kind in
        b) a=$(b_answer "$case") ;;
        r) a=$(r_answer "$case") ;;
    esac
    echo "$case|$sum|$kind|$a" >> "$work/got.txt"
done < "$work/cases.txt"
if recording; then
    settle build "$work/got.txt"
else
    awk -F'|' 'NR == FNR { keep[$1 "|" $2 "|" $3] = 1; next } (($1 "|" $2 "|" $3) in keep)' "$work/cases.txt" "$vec/build.txt" > "$work/want.txt"
    awk -F'|' -v out="$work/build-report.txt" '
        NR == FNR { want[$1 "|" $3] = $4; next }
        {
            w = want[$1 "|" $3]
            if ($4 == w) { if ($3 == "b") bsame++; else rsame++; next }
            split($4, g, " ")
            # a program the compiler in cancho has no code for: `build` is a status 3 (no program made), `run` a status 3 with nothing on the standard output.
            if ($3 == "b" && $4 == "nobuild") { cannot++; next }
            if ($3 == "r" && g[1] == 3 && g[2] == "4294967295") { rcannot++; next }
            different[++nd] = $1 " " $3 ": Rust " w " against " $4
        }
        END {
            for (i = 1; i <= nd; i++) print "different: " different[i] > out
            print "bsame " bsame + 0 > out; print "rsame " rsame + 0 > out; print "cannot " cannot + 0 > out; print "rcannot " rcannot + 0 > out
        }' "$work/want.txt" "$work/got.txt"
    [ "$(grep -c '^different' "$work/build-report.txt")" = 0 ] || fail "build and run: $(grep '^different' "$work/build-report.txt" | head -3 | cut -c1-240 | tr '\n' '~')"
    bsame=$(sed -n 's/^bsame //p' "$work/build-report.txt"); rsame=$(sed -n 's/^rsame //p' "$work/build-report.txt")
    [ "$bsame" -gt 60 ] || fail "only $bsame programs were built and run alike"
    echo "build: $(tr '\n' ' ' < "$work/build-report.txt")" >&2
fi
# The options.
rm -rf "$work/o"
mkdir -p "$work/o"
cp examples/hello.cho "$work/o/h.cho"
cb() { ( cd "$work/o" && "$cancho" build "$@" > o.txt 2> e.txt; echo $? > s.txt ); cat "$work/o/s.txt"; }
same "$(cb h.cho)" 0 "build h.cho"
[ -f "$work/o/h" ] || fail "the default name is the stem of the first file"
same "$(cb --emit obj h.cho)" 0 "build --emit obj"
head -c 4 "$work/o/h.o" | od -An -tx1 | tr -d ' \n' > "$work/o/magic.txt"
case $(cat "$work/o/magic.txt") in 7f454c46 | cffaedfe) ;; *) fail "h.o is not an object file: $(cat "$work/o/magic.txt")" ;; esac
same "$(cb -o named -l m -L /usr/lib h.cho)" 0 "build -o named -l m -L /usr/lib"
[ -f "$work/o/named" ] || fail "-o names the program"
for f in "$work"/o/*; do case $(basename "$f") in h.cho | h | h.o | named | o.txt | e.txt | s.txt | magic.txt) ;; *) fail "left over: $(basename "$f")" ;; esac; done
printf 'fn main(world: World) -> [] int { return x; }' > "$work/o/bad.cho"
same "$(cb bad.cho)" 1 "a refused program"
[ ! -e "$work/o/bad" ] && [ ! -e "$work/o/bad.ll" ] || fail "a refused program made a file"
same "$(cb)" 2 "build with no file"
same "$(cb --emit wasm h.cho)" 2 "build --emit wasm"
same "$(cb h.cho -o)" 2 "build h.cho -o"
end

# --- the_command_line_in_cancho_runs_like_cancho_run
begin the_command_line_in_cancho_runs_like_cancho_run
# (the programs of the repository are run in the section above, kind `r`) The standard input is empty unless `--stdin` forwards ours; the status of the program is the status of the command, except that
# a program that ends with a status above 255 is a trap (3); a trap is a status 3.
rm -rf "$work/r"
mkdir -p "$work/r"
printf 'fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args);\n    release(heap);\n    release(fs);\n    release(ffi);\n    var n = 0;\n    borrow mut io as &!i in {\n        var c = getchar(i);\n        while c >= 0 {\n            n = n + 1;\n            putchar(i, c);\n            c = getchar(i);\n        }\n    }\n    release(io);\n    return n;\n}\n' > "$work/r/echo.cho"
( cd "$work/r" && printf 'abc' | "$cancho" run --stdin echo.cho > o.txt 2> e.txt; echo $? > s.txt )
same "$(cat "$work/r/s.txt")" 3 "run --stdin: the status of the program"
same "$(cat "$work/r/o.txt")" abc "run --stdin forwards the input"
( cd "$work/r" && printf 'abc' | "$cancho" run echo.cho > o.txt 2> e.txt; echo $? > s.txt )
same "$(cat "$work/r/s.txt")" 0 "run: no standard input"
[ ! -s "$work/r/o.txt" ] || fail "run without --stdin read the input"
printf 'fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args);\n    release(heap);\n    release(fs);\n    release(ffi);\n    release(io);\n    var zero = 0;\n    return 1 / zero;\n}\n' > "$work/r/trap.cho"
( cd "$work/r" && "$cancho" run trap.cho > o.txt 2> e.txt; echo $? > s.txt )
same "$(cat "$work/r/s.txt")" 3 "run: a program that traps"
ls "$work/r" | grep -q cancho-run && fail "left over: $(ls "$work/r")"
end
