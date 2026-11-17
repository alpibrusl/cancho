# fuzz.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/fuzz.rs (docs/gate.md G4, wave 4; docs/fuzzing.md): a mutation fuzzer for the compiler, in awk and shell. Every `.cho` file of the
# repository is a seed; `tests/programs/fuzz_mutate.awk` makes the mutants (a token deleted, duplicated, swapped, replaced by one of the same kind from anywhere in the corpus, or a run of tokens spliced in from another
# file), from a fixed seed so that a run is reproduced exactly (by the corpus and the two numbers: it is the same set of files that fixes it, so a new `.cho` file in the repository changes which mutants a run holds,
# as docs/fuzzing.md section 4.5 found for the Rust fuzzer). Each mutant goes through the compiler under test, and four properties must hold:
#
#   1. Nothing crashes: `print` and `check` answer 0 or 1 (a signal, a trap or any other status is a finding), and every refusal says a rule tag (`ERR rule` for a file that does not parse, a `"rule"` of the JSON for one that does not check).
#   2. What parses prints, and the printed text prints as itself (a fixed point), and parses to the same tree (`print --output json` of the mutant and of its printed text are the same document).
#   3. What the checker accepts, the backend builds (`check` generates the code, so `build` of an accepted program answers 0); an `internal` refusal is the compiler failing on a program it accepted.
#   4. It reaches: at least 10% of the mutants parse, 2% check and 2% build, so a change that stopped them reaching the checker or the backend cannot pass by testing nothing.
#
# The oracle was the Rust crates (`cancho_syntax::parse`, `print`, `cancho_ir::lower_all`, `cancho_codegen::compile_object` under `catch_unwind`); the properties are the same, and the pipeline is the compiler in cancho's, run
# as a process. CANCHO_FUZZ_ITERATIONS and CANCHO_FUZZ_SEED (decimal) override the defaults for a long run; CANCHO_FUZZ_REPLAY=FILE runs one saved finding through the same properties and says what it did
# (a finding is copied to ${TMPDIR:-/tmp}/cancho-fuzz/finding-N.cho). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
mkdir -p "$work"
work=$(cd "$work" && pwd -P)
iterations=${CANCHO_FUZZ_ITERATIONS:-1500}
seed=${CANCHO_FUZZ_SEED:-1577}
workers=4
# A value that is not a number is refused, not read as 0 (as the Rust test refused it): a run that silently ignored its seed would report the default seed's result as a new one.
case "$iterations$seed" in
    '' | *[!0-9]*) echo "fuzz: CANCHO_FUZZ_ITERATIONS=$iterations and CANCHO_FUZZ_SEED=$seed must be decimal numbers" >&2; exit 2 ;;
esac

# exercise FILE RESULTS: one mutant through the properties; a line of RESULTS says how far it got (`reach P C B`: parsed, checked, built, each 0 or 1) and one more line `finding STAGE MESSAGE FILE` for each property that fails.
exercise() {
    _f=$1; _r=$2; _t=$_r.tmp
    _p=0; _c=0; _b=0
    "$cancho" print "$_f" > "$_t.p1" 2> "$_t.err"
    _code=$?
    if [ "$_code" = 1 ]; then
        # Refused: the line says `FILE: ERR rule A B`.
        grep -q ': ERR [a-z][a-z0-9-]*' "$_t.p1" "$_t.err" || echo "finding print 'a refusal that says no rule tag' $_f" >> "$_r"
    elif [ "$_code" != 0 ]; then
        echo "finding print 'exit $_code' $_f" >> "$_r"
    else
        _p=1
        cp "$_t.p1" "$_t.p1.cho"
        if ! "$cancho" print "$_t.p1.cho" > "$_t.p2" 2> /dev/null; then
            echo "finding reparse 'printed text does not parse' $_f" >> "$_r"
        elif ! cmp -s "$_t.p1" "$_t.p2"; then
            echo "finding reparse 'printing is not a fixed point' $_f" >> "$_r"
        else
            "$cancho" print --output json "$_f" > "$_t.j1" 2> /dev/null
            "$cancho" print --output json "$_t.p1.cho" > "$_t.j2" 2> /dev/null
            if ! cmp -s "$_t.j1" "$_t.j2"; then
                # The printer writes a float of a large exponent in full and the reader refuses more than 120 digits (the GAP of the large float, below): the tree is not compared for those.
                if grep -q 'a floating-point literal of more than 120 significant digits' "$_t.j2"; then echo "known float" >> "$_r"; else echo "finding reparse 'the printed text is a different tree' $_f" >> "$_r"; fi
            fi
        fi
        "$cancho" check "$_f" --std --output json > "$_t.c" 2> "$_t.err"
        _code=$?
        if [ "$_code" = 0 ]; then
            _c=1
            "$cancho" build "$_f" --std --emit obj -o "$_t.o" > /dev/null 2> "$_t.err"
            _code=$?
            if [ "$_code" = 0 ]; then _b=1
            elif [ "$_code" = 3 ] && grep -q 'cannot judge the program' "$_t.err"; then echo declined >> "$_r"
            else echo "finding backend 'an accepted program did not compile (exit $_code)' $_f" >> "$_r"; fi
            rm -f "$_t.o"
        elif [ "$_code" = 1 ]; then
            grep -q '"rule": "[a-z]' "$_t.c" || echo "finding check 'a refusal that says no rule tag' $_f" >> "$_r"
            grep -q '"rule": "internal"' "$_t.c" && echo "finding check 'an internal refusal' $_f" >> "$_r"
        elif [ "$_code" = 3 ] && grep -q 'cannot judge the program' "$_t.err"; then
            # The compiler in cancho says it has no code for what is here (docs/gate.md G4): a declined program, not a crash.
            echo declined >> "$_r"
        else
            echo "finding check 'exit $_code' $_f" >> "$_r"
        fi
    fi
    echo "reach $_p $_c $_b" >> "$_r"
}

if [ -n "$CANCHO_FUZZ_REPLAY" ]; then
    rm -f "$work/replay.txt"
    exercise "$CANCHO_FUZZ_REPLAY" "$work/replay.txt"
    cat "$work/replay.txt"
    exit 0
fi

begin a_saved_finding_replays
end
begin mutants_never_crash_the_compiler
seeds=$work/seeds.txt
find . \( -name target -o -name '.?*' \) -prune -o -name '*.cho' -type f -print | LC_ALL=C sort > "$seeds"
nseeds=$(wc -l < "$seeds" | tr -d ' ')
[ "$nseeds" -ge 1000 ] || fail "only $nseeds seeds"
rm -rf "$work/m" "$work"/res.*
mkdir -p "$work/m"
# shellcheck disable=SC2046
awk -v seed="$seed" -v count="$iterations" -v dir="$work/m" -f tests/programs/fuzz_mutate.awk $(cat "$seeds") || fail "the mutator failed"
made=$(ls "$work/m" | wc -l | tr -d ' ')
[ "$made" = "$iterations" ] || fail "$made mutants made, $iterations wanted"
k=0
while [ "$k" -lt "$workers" ]; do
    (
        i=$((k + 1))
        while [ "$i" -le "$iterations" ]; do
            exercise "$work/m/m$(printf '%05d' "$i").cho" "$work/res.$k"
            i=$((i + workers))
        done
    ) &
    k=$((k + 1))
done
wait
cat "$work"/res.? > "$work/results.txt" 2> /dev/null
parsed=$(awk '$1 == "reach" { n += $2 } END { print n + 0 }' "$work/results.txt")
checked=$(awk '$1 == "reach" { n += $3 } END { print n + 0 }' "$work/results.txt")
built=$(awk '$1 == "reach" { n += $4 } END { print n + 0 }' "$work/results.txt")
total=$(grep -c '^reach' "$work/results.txt")
declined=$(grep -c '^declined' "$work/results.txt")
known=$(grep -c '^known' "$work/results.txt")
echo "mutants run: $total of $iterations (seed $seed), $nseeds seeds; parsed $parsed, checked $checked, built $built, declined (the compiler has no code for them) $declined, a large float printed in full $known" > "$work/summary.txt"
[ "$total" = "$iterations" ] || fail "$total mutants ran of $iterations"
[ "$declined" -le $((iterations / 20)) ] || fail "too many mutants the compiler declines to judge: $declined of $iterations"
[ "$parsed" -ge $((iterations / 10)) ] || fail "too few mutants parse: $parsed of $iterations"
[ "$checked" -ge $((iterations / 50)) ] || fail "too few mutants check: $checked of $iterations"
[ "$built" -ge $((iterations / 50)) ] || fail "too few mutants build: $built of $iterations"
# One report per distinct failure: the same crash from a thousand mutants is one bug.
grep '^finding' "$work/results.txt" | awk '{ f = $NF; $NF = ""; if (!seen[$0]++) print $0 "-- " f }' | sort > "$work/findings.txt"
nfind=$(wc -l < "$work/findings.txt" | tr -d ' ')
if [ "$nfind" != 0 ]; then
    out=${TMPDIR:-/tmp}/cancho-fuzz
    mkdir -p "$out"
    n=0
    while IFS= read -r line; do
        file=${line##*-- }
        cp "$file" "$out/finding-$n.cho"
        fail "$line (copied to $out/finding-$n.cho; seed $seed)"
        n=$((n + 1))
    done < "$work/findings.txt"
fi
# The gap the fuzzer found and the mutants are excused from, said deterministically (not by whether a mutant of this run hit it): the printer writes 1.0e200 as a 201-digit number, and the compiler in cancho's reader
# refuses a float literal of more than 120 significant digits, so the printed text does not read back (the Rust printer wrote the same text and its reader took it). Found by this script on 2026-11-17; a gap of
# examples/selfhost to close (docs/gate.md G4, wave 3 found the same for `ids`). When it closes, this line becomes `ok` and the transcript says so.
printf 'fn f() -> [] float {\n    return 1.0e200;\n}\n' > "$work/big.cho"
"$cancho" print "$work/big.cho" > "$work/big.printed.cho" 2> /dev/null
if "$cancho" print --output json "$work/big.printed.cho" > /dev/null 2>&1; then
    echo "ok a float of a large exponent is printed in a form that reads back"
else
    echo "GAP: print writes 1.0e200 as a number of 201 digits, which the reader of the compiler in cancho refuses (a float literal of more than 120 significant digits); the Rust printer wrote the same text and its reader took it"
fi
end
cat "$work/summary.txt" >&2
