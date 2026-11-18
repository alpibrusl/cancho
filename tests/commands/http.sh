# http.sh -- std.http and std.route (docs/http.md): what the library's own tests are run from, and the misuse that must end a program instead of letting it go on. Moved from
# crates/cancho/tests/conformance/http.rs (docs/gate.md, G4 waves 4 and 5; the file is gone). The parser and the chunked decoder are held to the recorded answers of `httparse` and of a
# second decoder (tests/vectors/http_parse.*, tests/vectors/http_dechunk.*; docs/gate.md, wave 5). The checks are `begin NAME` ... `end` of tests/commands/script_lib.sh. $1 is the compiler, $2 a
# scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/support/net.sh

begin the_library_tests_pass_on_both_backends
# tests/lex/*.cho are `cancho test` files run by the conformance suite; this is the check that the two files this module owns are among them.
for file in http_test.cho route_test.cho; do
    [ -f "tests/lex/$file" ] || fail "tests/lex/$file is missing"
done
end

# misuse NAME KIND BODY: a program that does BODY with a router (route) or a response buffer (http) and then returns 0; it must be built, and it must be ended by a signal.
misuse() {
    _tag=$1; _kind=$2; _body=$3
    if [ "$_kind" = route ]; then
        _head='import std.route;

fn run[&r](heap: &!r Heap) -> [heap] int {
    var r = route.empty(heap);'
        _tail='    route.drop(heap, r);'
    else
        _head='import std.buffer;
import std.http;

fn run[&r](heap: &!r Heap) -> [heap] int {
    var out = buffer.empty(heap, 16);'
        _tail='    buffer.drop(heap, out);'
    fi
    {
        printf '%s\n' "$_head"
        printf '    %s\n' "$_body"
        printf '%s\n' "$_tail"
        printf '    return 0;\n}\n\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args); release(fs); release(ffi); release(io);\n    var status = 0;\n    borrow mut heap as &!h in { status = run(h); }\n    release(heap);\n    return status;\n}\n'
    } > "$work/misuse-$_tag.cho"
    build "$work/misuse-$_tag.cho" "misuse-$_tag" || return 1
    traps "$work/misuse-$_tag" || fail "$_tag should be ended by a signal, not exit"
    misused=$((misused + 1))
}

begin a_misused_router_or_response_traps_instead_of_going_on
misused=0
misuse no-leading-slash route 'r = route.add(heap, r, "GET", "users", 1);'
misuse empty-pattern route 'r = route.add(heap, r, "GET", "", 1);'
misuse trailing-slash route 'r = route.add(heap, r, "GET", "/a/", 1);'
misuse empty-segment route 'r = route.add(heap, r, "GET", "/a//b", 1);'
misuse nameless-param route 'r = route.add(heap, r, "GET", "/a/:", 1);'
misuse rest-not-last route 'r = route.add(heap, r, "GET", "/a/*r/b", 1);'
misuse negative-id route 'r = route.add(heap, r, "GET", "/a", 0 - 1);'
misuse empty-method route 'r = route.add(heap, r, "", "/a", 1);'
misuse duplicate-static route 'r = route.add(heap, r, "GET", "/a", 1); r = route.add(heap, r, "GET", "/a", 2);'
misuse header-injection http 'out = http.respond_head(heap, out, 200, "text/plain\r\nSet-Cookie: x=1", 0, true);'
misuse status-out-of-range http 'out = http.respond_head(heap, out, 42, "text/plain", 0, true);'
misuse no-content-200 http 'out = http.respond_no_content(heap, out, 200, true, "");'
misuse no-content-404 http 'out = http.respond_no_content(heap, out, 404, true, "");'
misuse no-content-extra-injection http 'out = http.respond_no_content(heap, out, 204, true, "X: a\nSet-Cookie: b\r\n");'
misuse negative-length http 'out = http.respond_head(heap, out, 200, "text/plain", 0 - 1, true);'
misuse extra-bare-line-feed http 'out = http.respond_head_with(heap, out, 200, "text/plain", 0, true, "X: a\nSet-Cookie: b\r\n");'
misuse extra-bare-carriage-return http 'out = http.respond_head_with(heap, out, 200, "text/plain", 0, true, "X: a\rY: b\r\n");'
misuse extra-without-a-colon http 'out = http.respond_head_with(heap, out, 200, "text/plain", 0, true, "not a header\r\n");'
misuse extra-ends-in-a-bare-line-feed http 'out = http.respond_head_with(heap, out, 200, "text/plain", 0, true, "X: a\n");'
misuse extra-ends-in-a-bare-carriage-return http 'out = http.respond_head_with(heap, out, 200, "text/plain", 0, true, "X: a\r");'
misuse no-content-extra-ends-in-a-bare-line-feed http 'out = http.respond_no_content(heap, out, 204, true, "X: a\n");'
misuse extra-a-blank-line http 'out = http.respond_head_with(heap, out, 200, "text/plain", 0, true, "X: a\r\n\r\nbody");'
# The count of programs built and ended by a signal: a misuse dropped from this list cannot go quietly.
same "$misused" 22 "the misuses that were built and trapped"
end

begin the_router_agrees_with_a_reference_matcher
# The script (14 random routes, 4,000 queries, the methods listed for 1,200 paths) is made by tests/programs/route_reference.awk, which also holds every line the driver
# printed to a matcher written from the rules in docs/http.md section 5. Three seeds: 12,000 queries.
build_exe route_driver tests/programs/route_driver.cho || exit 1
for seed in 37006 12345 777; do
    awk -v mode=gen -v seed=$seed -f tests/programs/route_reference.awk > "$work/route.script"
    "$work/route_driver" < "$work/route.script" > "$work/route.out" 2> "$work/route.err"
    same "$?" 0 "the driver's exit status (seed $seed)"
    awk -v mode=check -v seed=$seed -v out="$work/route.out" -f tests/programs/route_reference.awk > "$work/route.said"
    [ "$?" = 0 ] || fail "seed $seed: $(head -5 "$work/route.said")"
    grep -q '^ok queries=4000 ' "$work/route.said" || fail "seed $seed: the corpus did not run: $(head -2 "$work/route.said")"
done
end

begin no_byte_string_makes_the_parser_trap
# 1,500 random byte strings of random length, biased towards the bytes the grammar is made of (tests/programs/http_fuzz_gen.cho); a trap anywhere makes the driver die and
# its status non-zero. Three seeds: 4,500 strings.
build_exe http_driver tests/programs/http_driver.cho || exit 1
build_exe http_fuzz_gen tests/programs/http_fuzz_gen.cho || exit 1
for seed in 2989 1 2; do
    "$work/http_fuzz_gen" $seed 1500 > "$work/fuzz.in"
    "$work/http_driver" < "$work/fuzz.in" > "$work/fuzz.out" 2> "$work/fuzz.err"
    same "$?" 0 "the driver's exit status (seed $seed): $(head -c 200 "$work/fuzz.err")"
    same "$(wc -l < "$work/fuzz.out" | tr -d ' ')" 1500 "one line per string (seed $seed)"
done
end

begin the_parser_agrees_with_httparse_and_is_never_looser
# tests/vectors/http_parse.bin is 1,500 cases framed as the driver reads them (a decimal length, a newline, the bytes): 250 generated requests, each with four mutations (a byte
# changed, deleted, inserted, a delimiter inserted, or a cut) and one cut inside the head. tests/vectors/http_parse.expected is what the Rust crate `httparse` 1.10.1 said of each,
# recorded once (2026-11-18, G4 wave 5) while crates/cancho/tests/conformance/http.rs was still in the tree: `C BARE BODY METHOD-END TARGET-START TARGET-END MINOR NHEADERS` and four
# offsets a header (name start and end, value start and end) for a complete request, `P BARE` for one it finds partial, `R BARE` for one it refuses; BARE is 1 when the bytes hold a
# line feed that no carriage return precedes. The parser must give the same offsets wherever it accepts; it may refuse (code 2, 4, 6, 8, 9, 11, 12: the reasons docs/http.md section 3
# names) what httparse accepts, never for another reason; and it may call a request incomplete where httparse has it complete only when the bytes hold a bare line feed.
build_exe http_driver tests/programs/http_driver.cho || exit 1
"$work/http_driver" < tests/vectors/http_parse.bin > "$work/parse.out" 2> "$work/parse.err"
same "$?" 0 "the driver's exit status: $(head -c 200 "$work/parse.err")"
same "$(wc -l < "$work/parse.out" | tr -d ' ')" "$(wc -l < tests/vectors/http_parse.expected | tr -d ' ')" "one line per case"
paste -d'|' "$work/parse.out" tests/vectors/http_parse.expected | awk -F'|' '
function bad(why) { if (++problems <= 5) print "case " NR ": " why ": " $1 " | " $2 }
{
    ng = split($1, g, " "); split($2, e, " ")
    if (g[1] == "OK") {
        accepted++
        if (e[1] != "C") { bad("accepted what httparse refuses or finds partial"); next }
        nh = e[8]
        if (ng != 11 + 4 * nh) { bad("the number of fields"); next }
        if (g[2] != e[3] || g[3] != e[4] || g[4] != e[5] || g[5] != e[6] || g[7] != 10 + e[7] || g[8] != nh) { bad("the head"); next }
        for (k = 0; k < 4 * nh; k++) if (g[12 + k] != e[9 + k]) { bad("a header offset"); break }
    } else if (g[2] == 1) {
        incomplete++
        if (e[1] == "C" && e[2] != 1) bad("incomplete where httparse is complete and no bare line feed is in the bytes")
    } else if (e[1] == "C") {
        stricter++
        if (g[2] !~ /^(2|4|6|8|9|11|12)$/) bad("refused (code " g[2] ") what httparse accepts, for no declared reason")
    }
}
END {
    if (accepted < 200) { print "the vectors must hold plenty of valid requests: " accepted; problems++ }
    if (incomplete < 100) { print "too few incomplete: " incomplete; problems++ }
    if (stricter < 10) { print "too few cases where the parser is stricter: " stricter; problems++ }
    exit problems ? 1 : 0
}' > "$work/parse.said"
[ "$?" = 0 ] || fail "$(head -c 600 "$work/parse.said")"
end

begin the_chunked_decoder_agrees_with_a_second_decoder_on_valid_and_mutated_bodies
# tests/vectors/http_dechunk.bin is 1,504 chunked bodies (250 generated, each mutated four ways and cut once, and four sizes that overflow) and tests/vectors/http_dechunk.expected
# what a second decoder, written independently in Rust from the rules in the header of std.http.dechunk, said of each with 256 bytes of room (recorded 2026-11-18, G4 wave 5, from
# crates/cancho/tests/conformance/http.rs): `CONSUMED` or, when CONSUMED is positive, `CONSUMED HEX-OF-THE-DECODED-BYTES`. CONSUMED is -1 for "not all here", -2 a bad size, -3 bad
# framing, -4 too large for the room, -5 an extension or a trailer. Every outcome must be reached by some case, or the check proves less than it says.
build_exe dechunk_driver tests/programs/dechunk_driver.cho || exit 1
"$work/dechunk_driver" < tests/vectors/http_dechunk.bin > "$work/dechunk.out" 2> "$work/dechunk.err"
same "$?" 0 "the driver's exit status: $(head -c 200 "$work/dechunk.err")"
same "$(wc -l < "$work/dechunk.out" | tr -d ' ')" "$(wc -l < tests/vectors/http_dechunk.expected | tr -d ' ')" "one line per case"
paste -d'|' "$work/dechunk.out" tests/vectors/http_dechunk.expected | awk -F'|' '
function bad(why) { if (++problems <= 5) print "case " NR ": " why ": " $1 " | " $2 }
{
    split($1, g, " "); split($2, e, " ")
    if (g[1] != e[1]) { bad("consumed"); next }
    code = e[1] > 0 ? 1 : e[1]
    seen[code]++
    if (e[1] > 0 && (g[3] != e[2] || g[2] * 2 != length(e[2]))) bad("the decoded bytes")
}
END {
    for (c = -5; c <= 1; c++) if (c != 0 && seen[c] == 0) { print "no case reached " c; problems++ }
    exit problems ? 1 : 0
}' > "$work/dechunk.said"
[ "$?" = 0 ] || fail "$(head -c 600 "$work/dechunk.said")"
end
