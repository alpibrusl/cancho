# binary32.sh -- the script of tests/commands/script_tests.cho for `conformance/binary32.rs` (deleted): `f32` (docs/f32.md), F1's and F2's gates, the layout report and `int_of_f32`'s traps.
# The gates are programs of tests/programs (f32_gate.cho, f32_convert_gate.cho) that hold every operation against two oracles of their own (binary64 rounded once, and integer arithmetic) and
# report counts; this script reads the counts. $1 is the compiler, $2 a scratch directory; run from the root of the repository. One backend (LLVM): the Cranelift legs went with D2, and with
# them "the gate reports the same on both backends"; `tests/accept/f32.cho` and `tests/accept/f32_text.cho` are run by the accept walk of tests/run.cho, which holds them to their own STDOUT lines.
cancho=$1
work=$2
. tests/commands/script_lib.sh

pairs=1000000
# 255 exponents and both signs, 46 subnormal powers of two, 29 more for each sign: every special meets every special.
specials=$((255 * 2 + 23 * 2 + 2 * 29))
expected=$((pairs + specials * specials))

build tests/programs/f32_gate.cho f32_gate
build tests/programs/f32_convert_gate.cho f32_convert_gate

begin "f32 operations match binary64 and integer arithmetic"
# Each of the ten rows (+ - * / == != < <= > >=) saw `expected` pairs and has no result that differs from either oracle; "no mismatch" means nothing if the operands never made rounding hard, so
# every arithmetic operation must have rounded (>10,000), overflowed and made a zero (>100), + - * must have met exact ties (>1,000), and * / reached the subnormal range with a rounding.
"$work/f32_gate" "$pairs" > "$work/gate.txt" 2> "$work/gate.err"
same "$?" 0 "the gate's exit status ($(head -c 200 "$work/gate.err"))"
same "$(grep -c '^[0-9]' "$work/gate.txt")" 10 "the rows of the gate (one per operation)"
same "$(grep -c '^cover ' "$work/gate.txt")" 4 "the coverage rows (one per arithmetic operation)"
awk -v want="$expected" '/^[0-9]/ { if ($2 != want) print "operation " $1 " saw " $2 " pairs, wanted " want; if ($3 != 0 || $4 != 0) print "operation " $1 ": " $3 " results differ from the f32_of oracle and " $4 " from the integer one; first a=" $5 " b=" $6 " got=" $7 " expected=" $8 } /^cover / { op = $2; if ($3 <= 10000) print "op " op ": only " $3 " rounded results"; if ($6 <= 100) print "op " op ": only " $6 " overflows"; if ($7 <= 100) print "op " op ": only " $7 " zero results"; if (op != 3 && $4 <= 1000) print "op " op ": only " $4 " exact ties"; if (op >= 2 && $5 <= 1000) print "op " op ": only " $5 " subnormal roundings" }' "$work/gate.txt" > "$work/gate.bad"
if [ -s "$work/gate.bad" ]; then fail "$(head -3 "$work/gate.bad" | tr '\n' '|')"; fi
end

begin "f32 sqrt and the integer conversions match integer oracles"
# Three rows (sqrt32, f32_of_int, int_of_f32): no result differs from the integer-arithmetic oracle (column 3), nor, for sqrt32 and int_of_f32, from the binary64 route (column 4: for f32_of_int
# it is the route that rounds twice, which is counted and must be non-zero: it is the reason the builtin exists). Coverage: sqrt32 had more than 10,000 inexact results, odd exponents and
# round-ups and 1,000 subnormal inputs; f32_of_int more than 10,000 inexact results and exact ties, 100 carries and -2^63 at least once.
"$work/f32_convert_gate" "$pairs" > "$work/convert.txt" 2> "$work/convert.err"
same "$?" 0 "the gate's exit status ($(head -c 200 "$work/convert.err"))"
same "$(grep -c . "$work/convert.txt")" 3 "the rows of the gate (one per builtin)"
awk -v n="$pairs" '
    { name = $1; cases = $2; bad_int = $3; bad_second = $4; first = $6 " " $7 " " $8; c1 = $10; c2 = $11; c3 = $12; c4 = $13; c5 = $14 }
    NR == 1 && name != "sqrt32" { print "row 1 is " name }
    NR == 2 && name != "f32_of_int" { print "row 2 is " name }
    NR == 3 && name != "int_of_f32" { print "row 3 is " name }
    bad_int != 0 { print name ": " bad_int " results differ from the integer oracle; first " first }
    name != "f32_of_int" && bad_second != 0 { print name ": " bad_second " results differ from the binary64 route; first " first }
    cases <= n { print name ": only " cases " cases" }
    NR == 1 { if (c1 <= 10000) print "sqrt32: only " c1 " inexact"; if (c2 <= 10000) print "sqrt32: only " c2 " odd exponents"; if (c4 <= 10000) print "sqrt32: only " c4 " round ups"; if (c3 <= 1000) print "sqrt32: only " c3 " subnormal inputs" }
    NR == 2 { if (c1 <= 10000) print "f32_of_int: only " c1 " inexact"; if (c2 <= 10000) print "f32_of_int: only " c2 " exact ties"; if (c3 <= 100) print "f32_of_int: only " c3 " carries"; if (c4 < 1) print "f32_of_int never saw -2^63"; if (bad_second <= 0) print "binary64 would have been enough: the oracle routes agree" }
' "$work/convert.txt" > "$work/convert.bad"
if [ -s "$work/convert.bad" ]; then fail "$(head -3 "$work/convert.bad" | tr '\n' '|')"; fi
end

begin "the layout report says what an f32 costs"
# docs/f32.md section 2: a slice of f32 is four bytes an element, and a struct is still eight per leaf (layout.md section 2's packing, not built here).
cat > "$work/layout.cho" << 'EOF'
edition 6;
struct Sample { value: f32 }
struct Pair { a: f32, b: f32 }
fn main(world: World) -> [] int { release(world); return 0; }
EOF
"$cancho" layout "$work/layout.cho" > "$work/layout.txt" 2> "$work/layout.err"
same "$?" 0 "layout's exit status ($(head -c 200 "$work/layout.err"))"
same "$(grep Sample "$work/layout.txt" | head -1 | awk '{ $1 = ""; print }' | tr -s ' ' | sed 's/^ //')" "1 8 4 8" "the row for Sample"
same "$(grep Pair "$work/layout.txt" | head -1 | awk '{ $1 = ""; print }' | tr -s ' ' | sed 's/^ //')" "2 16 8 16" "the row for Pair"
end

begin "int_of_f32 traps where truncate does"
# `int_of_f32` stops the process on NaN, either infinity and any magnitude at or beyond 2^63 (exactly -2^63 included, as `truncate`'s does), and answers toward zero everywhere else. Each case is
# a program of its own: BITS then the value it must answer, or `trap`.
n=0
while read -r bits want; do
    n=$((n + 1))
    if [ "$want" = trap ]; then compare=0; else compare=$want; fi
    cat > "$work/t.cho" << EOF
edition 6;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);
    release(args); release(heap); release(fs); release(ffi); release(io);
    release(net); release(clock); release(signals);
    let seen = int_of_f32(f32_of_bits($bits));
    if seen != $compare { return 1; }
    return 0;
}
EOF
    if "$cancho" build "$work/t.cho" -o "$work/t" > "$work/t.build" 2>&1; then
        { "$work/t" > /dev/null 2>&1; } 2> /dev/null
        status=$?
        if [ "$want" = trap ]; then
            [ "$status" -ge 128 ] || fail "int_of_f32($bits) should be killed by a signal, not exit $status"
        else
            [ "$status" = 0 ] || fail "int_of_f32($bits) should answer $want without trapping, exit $status"
        fi
    else
        fail "int_of_f32($bits) does not build: $(head -c 300 "$work/t.build")"
    fi
done << 'EOF'
2143289344 trap
4290772993 trap
2139095040 trap
4286578688 trap
1593835520 trap
3741319168 trap
2139095039 trap
1593835519 9223371487098961920
3741319167 -9223371487098961920
0 0
2147483648 0
1065353215 0
3212836863 0
3224161485 -2
1266679809 16777218
EOF
same "$n" 15 "the cases (a count)"
end
