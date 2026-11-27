# native_a64.sh -- the script of tests/commands/script_tests.cho for the AArch64 encoder (docs/native-backend.md section 11, epic #840 N2).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# tests/native/a64.vectors is 6,144 instructions, each as the operands of a form of compiler/native/a64.cho and the word clang's assembler made of it
# (tests/native/a64_vectors.py wrote it, and is not run here: the gate needs no assembler). tests/native/a64_check.cho calls the encoder for each and compares.
cancho=$1
work=$2
. tests/commands/script_lib.sh

begin every_vector_of_clangs_assembler_is_the_word_the_encoder_gives
if "$cancho" build tests/native/a64_check.cho tests/native/a64_forms.cho compiler/native/a64.cho compiler/native/a64sys.cho --std -o "$work/check" > "$work/build.txt" 2>&1; then
    "$work/check" tests/native/a64.vectors > "$work/said.txt" 2> "$work/wrong.txt"
    same "$?" 0 "the status"
    same "$(cat "$work/said.txt")" "vectors 6144 forms 40 registers 284 of a table of 171 wrong 0" "what it says"
    same "$(head -3 "$work/wrong.txt")" "" "the vectors that are wrong"
else
    fail "building the check said: $(head -c 300 "$work/build.txt")"
fi
end

# The file has a vector for every form, and every register `Cpu` accepts has one for each way it may be used (171 names: 113 read and written, 50 read, 8 written).
begin the_vectors_cover_every_form_and_every_register
same "$(cut -d' ' -f1 tests/native/a64.vectors | LC_ALL=C sort -u | wc -l | tr -d ' ')" 40 "the forms in the file"
[ "$(cut -d' ' -f1 tests/native/a64.vectors | LC_ALL=C sort | uniq -c | awk '{ print $1 }' | LC_ALL=C sort -n | sed -n 2p)" -ge 150 ] || fail "a form other than eret has fewer than 150 vectors"
grep 'fn both\|fn read_only\|fn write_only' -A1 compiler/bare.cho | grep 'return "' | sed 's/^ *return "//; s/";$//' | tr ' ' '\n' | grep . | LC_ALL=C sort > "$work/names.txt"
sed -n 's/^system_register [01] 9 [0-9]* [0-9a-f]* ; m[rs][rs] \(x9, \)\{0,1\}\([a-z0-9_]*\).*$/\2/p' tests/native/a64.vectors | grep . | LC_ALL=C sort -u > "$work/vectored.txt"
same "$(wc -l < "$work/names.txt" | tr -d ' ')" 171 "the names Cpu accepts"
same "$(cmp -s "$work/names.txt" "$work/vectored.txt"; echo $?)" 0 "the names with a vector"
end

# The check can fail: one bit of one word changed, and one operand, are each said.
begin a_wrong_word_is_said
sed '1000s/\([0-9a-f]\{7\}\)\([0-9a-f]\) ; /\1X ; /' tests/native/a64.vectors | awk 'NR == 1000 { sub(/0X ; /, "1 ; "); sub(/[1-9a-f]X ; /, "0 ; ") } { print }' > "$work/one.vectors"
same "$(cmp -s tests/native/a64.vectors "$work/one.vectors"; echo $?)" 1 "the changed file differs"
"$work/check" "$work/one.vectors" > "$work/one.txt" 2> "$work/one.err"
same "$?" 1 "the status with one word changed"
same "$(cat "$work/one.txt")" "vectors 6144 forms 40 registers 284 of a table of 171 wrong 1" "what it says"
same "$(grep -c '^wrong: ' "$work/one.err")" 1 "the line is given"
"$work/check" "$work/missing.vectors" > /dev/null 2>&1
same "$?" 2 "the status for a file that is not there"
end
