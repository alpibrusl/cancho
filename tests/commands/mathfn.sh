# mathfn.sh -- the script of tests/commands/script_tests.cho for `conformance/mathfn.rs` (deleted): `std.math`'s transcendental functions against the C library (docs/float-math.md section 9).
# A foreign signature cannot carry a float (docs/opaque-pointers.md), so the C library's answers are recorded: tests/vectors/mathfn.txt has a line `FUNCTION RANGE BOUND ANSWER...` for each of the
# 130 pairs of a function and a range of arguments that tests/programs/math_samples.cho knows, with libm's answer (as bits) for every 50th of the 10,000 arguments (200 each), and BOUND, the most
# the library may differ in units in the last place (the worst measured against glibc plus one). Each pair is run through math_samples and tests/programs/math_ulps.cho measures the distance.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh

build tests/programs/math_samples.cho math_samples
build tests/programs/math_ulps.cho math_ulps

begin "the transcendental functions agree with libm within the stated accuracy"
same "$(wc -l < tests/vectors/mathfn.txt | tr -d ' ')" 130 "the pairs recorded (a count, so the file cannot shrink silently)"
same "$(awk '{ n += NF - 3 } END { print n }' tests/vectors/mathfn.txt)" 26000 "the answers recorded (a count)"
while read -r id range limit answers; do
    "$work/math_samples" "$id" "$range" > "$work/samples.txt" 2> /dev/null
    same "$?" 0 "math_samples $id $range exit status"
    same "$(wc -l < "$work/samples.txt" | tr -d ' ')" 10000 "math_samples $id $range answers"
    awk 'NR % 50 == 1' "$work/samples.txt" > "$work/ours.txt"
    printf '%s\n' $answers > "$work/theirs.txt"
    paste -d, "$work/ours.txt" "$work/theirs.txt" | tr '\n' ',' | sed -e 's/,$//' -e 's/^/[/' -e 's/$/]/' > "$work/pairs.json"
    got=$("$work/math_ulps" < "$work/pairs.json")
    worst=${got%% *}
    [ "$worst" -le "$limit" ] 2> /dev/null || { fail "function $id over range $range is $worst ulp from libm's, over the bound of $limit"; break; }
done < tests/vectors/mathfn.txt
end
