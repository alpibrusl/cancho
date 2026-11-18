# front_end_names.sh -- the script of tests/commands/script_tests.cho for three tests of `conformance/selfhost.rs` (deleted): the identities (`ids.cho`, against cancho-id), BLAKE3
# (`blake3.cho`, against the blake3 crate) and the ingester (`ingester.cho`, against `cancho ingest`). The oracle ran once; its answers are in tests/vectors/frontend/ (ids.vec,
# blake3.txt, ingest.vec, ingest_roundtrip.vec; tests/commands/frontend_lib.sh says how they are read). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/frontend_lib.sh

begin the_identities_in_cancho_agree_with_the_rust_ones
port ids ids && {
    same "$(lines_of $V/ids.vec)" 1263 "the vector of the identities has 1263 programs"
    # Programs alone (1,070), of several files (41, where a name is reached through an import), and with the library after them (152, a sample, as each parses half a megabyte).
    same "$(awk '{ n[$1]++ } END { print n["w"] " " n["f"] " " n["l"] }' $V/ids.vec)" "1070 41 152" "the modes of the vector"
    [ "$(tags ids '^ERR')" -gt 5 ] || fail "fewer than 6 programs refused by the parser"
    replay ids "$work/ids"
}
end

# BLAKE3 of messages of every length around the sizes at which the tree of chunks changes shape (a block is 64 bytes, a chunk 1,024, and the stack of chaining values is joined at every
# power of two chunks). The messages are tests/programs/xorshift_bytes.cho's, the hashes the blake3 crate's.
begin blake3_in_cancho_agrees_with_the_blake3_crate
port blake3sum blake3sum blake3sum.cho blake3.cho lexcore.cho && build tests/programs/xorshift_bytes.cho xorshift_bytes && {
    same "$(lines_of $V/blake3.txt)" 271 "the vector has 271 lengths"
    : > "$work/b3.got"
    while read -r _len _hex; do
        _got=$("$work/xorshift_bytes" "$_len" | "$work/blake3sum")
        echo "$_len $_got" >> "$work/b3.got"
    done < "$V/blake3.txt"
    if ! cmp -s "$work/b3.got" "$V/blake3.txt"; then
        fail "BLAKE3 in cancho and the blake3 crate disagree about messages of these lengths: $(diff "$work/b3.got" "$V/blake3.txt" | grep '^<' | cut -d' ' -f2 | head -10 | tr '\n' ' ')"
    fi
}
end

begin the_ingester_in_cancho_agrees_with_the_rust_one
port ingester ingester && port jsonprinter jsonprinter && {
    # The JSON of every program that parses (1,046 of the corpus and the edge cases) must give the text `print` gives.
    same "$(lines_of $V/ingest_roundtrip.vec)" 1046 "the round-trip vector has 1046 programs"
    replay ingest_roundtrip "$work/ingester" j
    _roundtrip=$ran
    # JSON that is wrong in each way the refusals list: damaged trees of small programs, the edge cases of the reader, nesting at the limit and past it, spaces. The Rust test damaged 8,245
    # trees of every size; 728 of those that are 700 bytes or less are recorded (their inputs are in tests/vectors/frontend/cases/).
    same "$(lines_of $V/ingest.vec)" 849 "the vector of damaged JSON has 849 inputs"
    # The comparison is only worth something if it covers every rule and the programs that are let through.
    [ "$(tags ingest '^ERR:ingest-json$')" -ge 20 ] || fail "fewer than 20 inputs refused as not JSON"
    [ "$(tags ingest '^ERR:ingest-node$')" -ge 20 ] || fail "fewer than 20 inputs refused for a wrong node"
    [ "$(tags ingest '^ERR:ingest-arity$')" -ge 20 ] || fail "fewer than 20 inputs refused for a wrong arity"
    [ $(($(tags ingest '^ok$') + $(tags ingest_roundtrip '^ok$'))) -gt 300 ] || fail "fewer than 300 inputs accepted"
    replay ingest "$work/ingester"
}
end
