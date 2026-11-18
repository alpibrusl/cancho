# f32_text.sh -- the script of tests/commands/script_tests.cho for `conformance/f32_text.rs` (deleted): `f32` as text (docs/f32.md section 5.3), `std.fmt32` held against Rust's own `{:?}`,
# `{:.N}` and `str::parse::<f32>`, which the Rust test compiled from tests/programs/f32_oracle.rs with `rustc` and fed a million lines each. Their answers are recorded now, for a subset
# (see docs/gate.md, G4 wave 4, for the counts): tests/vectors/f32_text_debug, f32_text_fixed and f32_text_parse (`.txt` is what the program is fed, `.expected` Rust's answer line for line),
# and the digest of the shortest-round-trip sample. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/vector_lib.sh

build tests/programs/f32_text.cho f32_text
build tests/programs/f32_exhaustive.cho f32_exhaustive

begin "the shortest round-trip over a sample of the 2^32 patterns matches Rust's {:?}"
# `f32_exhaustive sample STRIDE PER` prints every pattern of a sample (about 10^7: 2 signs x 256 exponents x 19,500 and a stride of 429) as text and folds the lines into a digest, and checks in
# the program that each line reads back to its bits and that no decimal one digit shorter does. The digest of Rust's `{:?}` over the same patterns was recorded from the oracle: 9,954,694 lines,
# digest 1030297027630675948; equal digests mean every line, including which of several shortest strings was picked, is Rust's. (The sample of a million, 4093 and 1950, which the Cranelift
# leg ran, is 1,004,194 lines, digest 1606076375482270874.) The all-2^32-patterns run of the Rust test was opt in and is not here.
"$work/f32_exhaustive" sample 429 19500 > "$work/sample.txt" 2> "$work/sample.err"
same "$?" 0 "the program's exit status ($(head -c 200 "$work/sample.err"))"
# lines N digest D unread U longer L longest A widest B first F
set -- $(cat "$work/sample.txt")
same "$2 $3" "9954694 1030297027630675948" "the lines and the digest of Rust's {:?}"
same "$5" 0 "lines that did not read back to their bits"
same "$7" 0 "lines that were not the shortest"
same "${13}" -1 "the first failing pattern"
end

# filter_agrees MODE INPUT WANTED: the program f32_text, in MODE, fed INPUT, exits 0 and answers WANTED (line for line, byte for byte).
filter_agrees() {
    "$work/f32_text" "$1" < "$2" > "$work/filter.out" 2> "$work/filter.err"
    same "$?" 0 "$1: the program's exit status ($(head -c 200 "$work/filter.err"))"
    same "$(lines "$work/filter.out")" "$(lines "$3")" "$1: the number of answers"
    if ! cmp -s "$work/filter.out" "$3"; then
        fail "$1 differs from Rust, first three: $(paste -d' ' "$2" "$work/filter.out" "$3" | awk '$2 "" != $3 ""' | head -3 | tr '\n' '|')"
    fi
}

begin "{:?} matches Rust"
# 20,000 patterns: the specials (0, 1, 2, the largest subnormal, the smallest normal, 1, the largest, infinity, each with its sign), half uniform over all 32 bits and half with a random exponent
# and the fraction biased toward short ones (so the exact and the nearly exact turn up as often as the others), NaNs left out (their text is `NaN` whatever the payload).
same "$(lines tests/vectors/f32_text_debug.txt)" 20000 "the patterns recorded (a count, so the file cannot shrink silently)"
filter_agrees debug tests/vectors/f32_text_debug.txt tests/vectors/f32_text_debug.expected
end

begin "{:.N} matches Rust, ties included"
# 19,344 (pattern, N) pairs, 1,488 for each of N = 0 1 2 3 4 5 6 8 10 12 20 45 60: 400 random patterns, and the constructed ties: (2a+1) / 2^(N+1) and its negative (exactly halfway between two
# decimals of N places), k + 1/2 down to k + 1/32, and the f32s round the points where rounding carries through nines (9.5, 99.5, 0.9995, ...).
same "$(lines tests/vectors/f32_text_fixed.txt)" 19344 "the (N, pattern) pairs recorded (a count)"
paste -d' ' tests/vectors/f32_text_fixed.txt tests/vectors/f32_text_fixed.expected > "$work/fixed.both"
total=0
for prec in 0 1 2 3 4 5 6 8 10 12 20 45 60; do
    awk -v p="$prec" '$1 == p { print $2 }' "$work/fixed.both" > "$work/fixed.in"
    awk -v p="$prec" '$1 == p { print $3 }' "$work/fixed.both" > "$work/fixed.want"
    "$work/f32_text" fixed "$prec" < "$work/fixed.in" > "$work/fixed.out" 2> "$work/fixed.err"
    same "$?" 0 "{:.$prec}: the program's exit status ($(head -c 200 "$work/fixed.err"))"
    if ! cmp -s "$work/fixed.out" "$work/fixed.want"; then
        fail "{:.$prec} differs from Rust, first three: $(paste -d' ' "$work/fixed.in" "$work/fixed.out" "$work/fixed.want" | awk '$2 "" != $3 ""' | head -3 | tr '\n' '|')"
    fi
    total=$((total + $(lines "$work/fixed.in")))
done
same "$total" 19344 "the pairs run"
end

begin "parse matches Rust"
# 7,927 strings: the hard cases (what Rust accepts and refuses: spellings, nan and inf words, the edges of the range, exact halfway points between neighbouring f32s at up to 113 digits, 200
# digits, a tie followed by 300 zeros and a 1), 200 exact halfway points and 500 decimals a hair either side of one, 400 `{:?}` and `{:e}` texts of random patterns, and 4,000 random decimals
# (1 to 40 digits, a point or none, signs, exponents small and large). The answer is the bits of Rust's `str::parse::<f32>`, or what it refuses (a line of text); strings can hold a space, a tab, a
# NUL or a digit that is not ASCII.
same "$(lines tests/vectors/f32_text_parse.txt)" 7927 "the strings recorded (a count)"
filter_agrees parse tests/vectors/f32_text_parse.txt tests/vectors/f32_text_parse.expected
end

begin "parse does not round twice"
# The reason the reader exists: a decimal one digit above the tie 1 + 2^-24 is 0x3f800001 read once, and 0x3f800000 through binary64 (which takes the tie itself, to even, down). Rust agrees with the
# first, which is the pinned answer 1065353217.
same "$(printf '1.0000000596046447753906251\n' | "$work/f32_text" parse)" 1065353217 "the digit above the tie"
end
