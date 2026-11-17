# float_text.sh -- the script of tests/commands/script_tests.cho for `conformance/float_text.rs` (deleted): float text at the edges (docs/json.md section 4, docs/float-printing.md sections 3.4 and 10,
# docs/floating-point.md section 4.1): `std.json.to_float` on strings including the halfway cases among the subnormals, `std.fmt.float_into`'s rule for exact ties, and `std.fmt.float_fixed_into`.
# The oracles were Rust's `str::parse::<f64>`, `{:e}` and an exact-decimal-expansion rounding written in the test; their answers are recorded in tests/vectors/float_text_*, a subset of what the
# Rust test generated (see docs/gate.md, G4 wave 4, for the counts). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/vector_lib.sh

build tests/programs/json_floats.cho json_floats
build tests/programs/float_print.cho float_print
build tests/programs/float_fixed.cho float_fixed

# array FILE COLUMN: the lines of FILE (or only their COLUMN-th word) as a JSON array on one line.
array() { awk -v c="$2" 'BEGIN { printf "[" } { printf "%s%s", (NR > 1 ? "," : ""), (c > 0 ? $c : $0) } END { print "]" }' "$1"; }

begin "numbers are read as Rust reads them, halfway cases included"
# 15,937 cells (the Rust test read a million, 29,000 more of them halfway cases): the exact halfway point between two doubles written out in full (up to 767 digits), a hair above it and a hair
# below it, for the 64 smallest subnormals (every one: 256 cells), a stride of the largest subnormals and smallest normals, a stride of random ones (anywhere, subnormals as often as the rest),
# and the top of the range (DBL_MAX plus half an ulp, a tie that goes to infinity); then 15,000 of every shape a document holds (shortest forms, 17 digits, two decimals, integers, `NeK`, 17
# fractional digits). The answer for each is the bits Rust's correctly rounded parse gave (tests/vectors/float_text_read.expected), the first column of the driver's two.
same "$(lines tests/vectors/float_text_read.txt)" 15937 "the cells recorded (a count, so the file cannot shrink silently)"
same "$(lines tests/vectors/float_text_read.expected)" 15937 "the answers recorded (a count)"
array tests/vectors/float_text_read.txt 0 > "$work/read.json"
"$work/json_floats" < "$work/read.json" > "$work/read.out" 2> "$work/read.err"
same "$?" 0 "the driver's exit status ($(head -c 200 "$work/read.err"))"
awk '{ print $1 }' "$work/read.out" > "$work/read.bits"
same "$(lines "$work/read.bits")" 15937 "one answer per cell"
if ! cmp -s "$work/read.bits" tests/vectors/float_text_read.expected; then
    paste -d' ' tests/vectors/float_text_read.txt "$work/read.bits" tests/vectors/float_text_read.expected | awk '$2 "" != $3 "" { print substr($1, 1, 50) "(" length($1) " bytes): read " $2 ", Rust reads " $3 }' > "$work/read.bad"
    fail "$(lines "$work/read.bad") of 15937 cells read wrongly, first few: $(head -3 "$work/read.bad" | tr '\n' '|')"
fi
end

begin "exact ties between two shortest decimals print the even one and everything reads back"
# tests/vectors/float_text_ties.txt: `BITS RUST PRINTED` for 12,002 doubles: 2,000 whose exact decimal is dead between two shorter decimals (17 or 18 digits ending in 5, j / 2^k), the two
# documented ones (2^-25 and 86.125 * 2^40) and 10,000 random bit patterns. RUST is `{:e}` (a tie rounded up), PRINTED what `float_into` printed when the Rust test accepted it as Rust's or the
# even tie. The count of lines where they differ is pinned: a corpus with no difference tests nothing (the Rust test asked for more than 1,000 of 222,000; here 294 of 12,002 differ).
same "$(lines tests/vectors/float_text_ties.txt)" 12002 "the values recorded (a count)"
same "$(awk '$2 "" != $3 ""' tests/vectors/float_text_ties.txt | wc -l | tr -d ' ')" 294 "values printed differently from Rust's {:e} (the ties)"
array tests/vectors/float_text_ties.txt 1 > "$work/ties.json"
"$work/float_print" < "$work/ties.json" > "$work/ties.out" 2> "$work/ties.err"
same "$?" 0 "the driver's exit status ($(head -c 200 "$work/ties.err"))"
awk '{ print $3 }' tests/vectors/float_text_ties.txt > "$work/ties.want"
if ! cmp -s "$work/ties.out" "$work/ties.want"; then
    fail "the printer differs from the recorded text, first three: $(diff "$work/ties.out" "$work/ties.want" | grep '^[<>]' | head -3 | tr '\n' '|')"
fi
# and every printed text reads back to its bits
array "$work/ties.out" 0 > "$work/back.json"
"$work/json_floats" < "$work/back.json" | awk '{ print $1 }' > "$work/back.bits"
awk '{ print $1 }' tests/vectors/float_text_ties.txt > "$work/ties.bits"
if ! cmp -s "$work/back.bits" "$work/ties.bits"; then
    fail "a printed text does not read back to its bits, first three: $(diff "$work/back.bits" "$work/ties.bits" | grep '^[<>]' | head -3 | tr '\n' '|')"
fi
# the documented example, by name: 2^-25
same "$(printf '[%s]' "$((0x3e60000000000000))" | "$work/float_print")" "2.9802322387695312e-8" "the documented example"
end

begin "fixed printing rounds the exact value half to even and refuses what does not fit"
# docs/float-printing.md section 10, tests/vectors/float_text_fixed.txt: `BITS PLACES EXPECTED` for 8,726 cases: a tenth of 40,000 random doubles at 0 to 25 places, a fifth of the exact ties at the
# precision (t / 2^(p+1)), a fourteenth of the neighbours of the powers of two and ten at 0, 1, 2, 9, 17, 340 and 1,100 places, then every carry through nines, every extreme and every value
# with no literal (0, -0, 0.5, 1.5, 2.5, 1e22, 1e23, 5e-324, MAX, MIN_POSITIVE, NaN, both infinities) at 0, 1, 2, 20, 400 and 1,074 places, five that the 1,500-byte buffer must refuse
# (`short`: not trapped, not cut), two that fill it exactly, and the twelve known values of the oracle's own test (0.125 with 2 places is 0.12, 2.5 with 0 is 2, 99.5 with 0 is 100, ...). The
# expected text is rounded half to even from the exact decimal expansion, by hand, in the Rust test; recorded here.
same "$(lines tests/vectors/float_text_fixed.txt)" 8726 "the cases recorded (a count)"
same "$(grep -c ' short$' tests/vectors/float_text_fixed.txt)" 5 "the cases the buffer must refuse"
awk 'BEGIN { printf "[" } { printf "%s%s,%s", (NR > 1 ? "," : ""), $1, $2 } END { print "]" }' tests/vectors/float_text_fixed.txt > "$work/fixed.json"
"$work/float_fixed" < "$work/fixed.json" > "$work/fixed.out" 2> "$work/fixed.err"
same "$?" 0 "the driver's exit status ($(head -c 200 "$work/fixed.err"))"
awk '{ print $3 }' tests/vectors/float_text_fixed.txt > "$work/fixed.want"
same "$(lines "$work/fixed.out")" 8726 "one answer per case"
if ! cmp -s "$work/fixed.out" "$work/fixed.want"; then
    paste -d' ' "$work/fixed.out" "$work/fixed.want" | awk '$1 "" != $2 ""' | head -3 | cut -c1-120 > "$work/fixed.bad"
    fail "fixed printing differs from the recorded answers, first three: $(tr '\n' '|' < "$work/fixed.bad")"
fi
end
