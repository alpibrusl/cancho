# http.sh -- std.http and std.route (docs/http.md): what the library's own tests are run from, and the misuse that must end a program instead of letting it go on. Moved from
# crates/cancho/tests/conformance/http.rs (docs/gate.md, G4 wave 4): `the_library_tests_pass_on_both_backends` and `a_misused_router_or_response_traps_instead_of_going_on`.
# The other four tests of that file stay in Rust: they hold the parser, the router and the chunked decoder against `httparse` (a Rust crate) and against references written
# in Rust, over bytes from a Rust generator (docs/gate.md, wave 4). The checks are `begin NAME` ... `end` of tests/commands/script_lib.sh. $1 is the compiler, $2 a
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
