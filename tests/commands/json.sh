# json.sh -- the script of tests/commands/script_tests.cho for `conformance/json.rs` (deleted): `std.json` (docs/json.md), the parser and writer against `serde_json` and Rust's own correctly
# rounded `f64` parse, which were the oracles and whose answers are recorded now, for a subset of the Rust test's corpora (tests/vectors/json_*; docs/gate.md, G4 wave 4 has the counts). The
# fifth test of the Rust file, `the_library_tests_pass_on_both_backends` (`cancho test` of tests/lex/*.cho), is nine lines of tests/cancho-tests.txt. $1 is the compiler, $2 a scratch directory;
# run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/vector_lib.sh

build tests/programs/json_floats.cho json_floats
build tests/programs/json_docs.cho json_docs
build tests/programs/json_with_diff.cho json_with_diff

begin "numbers are read exactly as Rust reads them"
# tests/vectors/json_numbers.txt: 18,428 numbers: shortest forms of random doubles of every exponent and their negatives, 17 significant digits and 20 decimals of values in [0, 1), the halfway
# point between two adjacent doubles with a hair either side (and the same above 2^53 as integers), integers at and past the ends of `int`, and 38 named ones (1e22, 5e-324, the subnormal
# edge, DBL_MAX, 1e309, -0.0, 1e999999999999, ...). `.expected` has the bits of Rust's `str::parse::<f64>` (correctly rounded: not "close", the same bits) and `to_int`'s answer, which is the
# `i128` parse saturated to `i64` for an integer, and Rust's truncating `as i64` otherwise.
same "$(lines tests/vectors/json_numbers.txt)" 18428 "the numbers recorded (a count, so the file cannot shrink silently)"
awk 'BEGIN { printf "[" } { printf "%s%s", (NR > 1 ? "," : ""), $0 } END { print "]" }' tests/vectors/json_numbers.txt > "$work/numbers.json"
"$work/json_floats" < "$work/numbers.json" > "$work/numbers.out" 2> "$work/numbers.err"
same "$?" 0 "the driver's exit status ($(head -c 200 "$work/numbers.err"))"
same "$(lines "$work/numbers.out")" 18428 "one answer per number"
if ! cmp -s "$work/numbers.out" tests/vectors/json_numbers.expected; then
    paste -d' ' tests/vectors/json_numbers.txt "$work/numbers.out" tests/vectors/json_numbers.expected | awk '$2 "" != $4 "" || $3 "" != $5 "" { print $1 ": read " $2 " " $3 ", Rust " $4 " " $5 }' > "$work/numbers.bad"
    fail "$(lines "$work/numbers.bad") numbers read wrongly, first few: $(head -3 "$work/numbers.bad" | cut -c1-100 | tr '\n' '|')"
fi
end

# A corpus run: json_docs, fed the framed documents of FILE, answers a line each; $work/docs.out has the lines with `E code position` cut to `E`, and $work/docs.class with `A text` cut to `A`.
docs_run() {
    "$work/json_docs" < "$1" > "$work/docs.raw" 2> "$work/docs.err"
    same "$?" 0 "the driver's exit status ($(head -c 200 "$work/docs.err"))"
    sed -e 's/^E .*/E/' "$work/docs.raw" > "$work/docs.out"
    sed -e 's/^A .*/A/' "$work/docs.out" > "$work/docs.class"
}

begin "the parser accepts what serde_json accepts and writes it back whole"
# tests/vectors/json_docs.txt: 1,508 documents, framed (length, newline, bytes): 10 valid seeds that exercise every production and 1,500 of them with one or two damaged bytes (replaced, removed,
# inserted, truncated or swapped), those on which serde_json had an opinion (the library accepts a number past the f64 range as an infinity, which serde_json refuses: no comparison there).
# `.expected` has `A <the document written back>` for the 291 both accept (the Rust test held the writeback to serde_json's reading of the input, as JSON values: here it is the recorded text, which
# serde_json accepted as the same value) and `E` for the 1,217 both refuse. The class (accepted or refused) is compared for every document, the text for the accepted.
same "$(lines tests/vectors/json_docs.expected)" 1508 "the documents recorded (a count)"
same "$(grep -c '^A' tests/vectors/json_docs.expected)" 291 "documents both accept (the Rust test asked for more than 200)"
same "$(grep -c '^E' tests/vectors/json_docs.expected)" 1217 "documents both refuse (the Rust test asked for more than 200)"
docs_run tests/vectors/json_docs.txt
if ! cmp -s "$work/docs.out" tests/vectors/json_docs.expected; then
    fail "the answers differ from the recorded ones, first three: $(diff "$work/docs.out" tests/vectors/json_docs.expected | grep '^[<>]' | head -3 | cut -c1-120 | tr '\n' '|')"
fi
end

begin "parse_with agrees with serde_json on what is a document"
# The edge documents the JSON test suites are made of (`n_*`, `y_*`, `i_*`: truncations, bytes a string may not hold, a BOM, numbers at the edge of the grammar, trailing bytes), the ones under
# 2,000 bytes and 100 levels of nesting (the depth limit, 128, is policy and pinned by std.json's own tests): 102 of them, each classified by serde_json when recorded; 16 are accepted.
same "$(lines tests/vectors/json_edge.expected)" 102 "the edge documents recorded (a count; the Rust test asked for more than 100)"
same "$(grep -c '^A' tests/vectors/json_edge.expected)" 16 "edge documents accepted"
docs_run tests/vectors/json_edge.txt
if ! cmp -s "$work/docs.class" tests/vectors/json_edge.expected; then
    fail "classified differently from serde_json, first three (line, ours, recorded): $(paste -d' ' "$work/docs.class" tests/vectors/json_edge.expected | awk '$1 != $2 { print NR ": " $1 " " $2 }' | head -3 | tr '\n' '|')"
fi
end

begin "parse_with answers what parse answers and writes the same tape"
# `parse_with` takes its two ints of state from the caller instead of making a region (docs/json.md section 3.1); the claim is that nothing else differs. The driver runs both on every document
# (into a tape of the right size and into one too short, with a state slice that holds the previous document's garbage) and compares the answers and every int of the tapes. The Rust test ran
# 58,733 documents (11,444 accepted); tests/vectors/json_with.txt has 8,865: every edge document and seed and every prefix of each (all of them), every eighth of the seeds with any one byte of the
# pool in any one position, and 6,000 of the 40,000 with one or two random damages. The driver's report, `ok DOCUMENTS ACCEPTED REFUSED`, is pinned.
"$work/json_with_diff" < tests/vectors/json_with.txt > "$work/with.out" 2> "$work/with.err"
same "$?" 0 "the driver's exit status ($(head -c 200 "$work/with.err"))"
same "$(cat "$work/with.out")" "ok 8865 1661 7204" "the driver's report"
end

begin "a misused writer traps instead of writing bad json"
# Each of these is a bug in the caller, and the alternative to stopping is a service that sends `{"a":}`; `put_fragment` checks what it splices: a fragment that is not exactly one value would make
# the whole document invalid from a distance.
n=0
misuse() {
    n=$((n + 1))
    cat > "$work/misuse.cho" << EOF
import std.json;
import std.buffer;

fn run[&r](heap: &!r Heap) -> [heap] int {
    var w = json.writer(heap, 4);
    $2
    json.drop(heap, w);
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(fs); release(ffi); release(io);
    var status = 0;
    borrow mut heap as &!h in { status = run(h); }
    release(heap);
    return status;
}
EOF
    if "$cancho" build "$work/misuse.cho" --std -o "$work/misuse" > "$work/misuse.build" 2>&1; then
        { "$work/misuse" > /dev/null 2>&1; } 2> /dev/null
        [ "$?" -ge 128 ] || fail "$1 should be killed by a signal, not exit"
    else
        fail "$1 does not build: $(head -c 300 "$work/misuse.build")"
    fi
}
misuse keyless 'w = json.begin_object(heap, w); w = json.put_int(heap, w, 1);'
misuse key-outside 'w = json.put_key(heap, w, "k");'
misuse key-in-array 'w = json.begin_array(heap, w); w = json.put_key(heap, w, "k");'
misuse two-keys 'w = json.begin_object(heap, w); w = json.put_key(heap, w, "a"); w = json.put_key(heap, w, "b");'
misuse unbalanced-end 'w = json.end_array(heap, w);'
misuse mismatched-end 'w = json.begin_array(heap, w); w = json.end_object(heap, w);'
misuse end-after-key 'w = json.begin_object(heap, w); w = json.put_key(heap, w, "a"); w = json.end_object(heap, w);'
misuse unfinished 'w = json.begin_array(heap, w); let kept = json.finish(w); buffer.drop(heap, kept); w = json.writer(heap, 4);'
misuse fragment-empty 'w = json.put_fragment(heap, w, "");'
misuse fragment-whitespace-only 'w = json.put_fragment(heap, w, "  ");'
misuse fragment-two-values 'w = json.put_fragment(heap, w, "1 2");'
misuse fragment-trailing-comma 'w = json.put_fragment(heap, w, "[1,]");'
misuse fragment-unterminated 'w = json.put_fragment(heap, w, "{\"a\":");'
misuse fragment-keyless-in-object 'w = json.begin_object(heap, w); w = json.put_fragment(heap, w, "1");'
same "$n" 14 "the misuses (a count)"
end
