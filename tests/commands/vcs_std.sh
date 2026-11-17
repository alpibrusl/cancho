# vcs_std.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/vcs_std.rs (docs/gate.md G4, wave 4): a published package may import `std`
# (docs/package-system.md section 7): what `publish`, `resolve` and `fetch` do with the library, and what a consumer then builds. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

# Its own `abs` next to `std.math`'s: the one name both define, so a publish that attributed by name alone would skip `pkg.abs` or republish `std.math.abs` under the package's store.
pkg() {
    cat > "$1" <<'EOF'
module pkg;
import std.math;
pub fn abs(n: int) -> [] int { return math.abs(n) + 100; }
pub fn clamp3(n: int) -> [] int { return math.min(n, 3); }
EOF
}
consumer() {
    cat > "$1" <<'EOF'
import pkg;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return pkg.abs(0 - 5) + pkg.clamp3(9);
}
EOF
}
# publish_pkg: pkg.cho published with --std into $d/store.
publish_pkg() { pkg "$d/pkg.cho"; r vcs publish --std --store "$d/store" "$d/pkg.cho"; succeeds "publish --std"; }

begin importing_std_without_the_flag_is_refused_and_says_what_to_pass
fresh std1; pkg "$d/pkg.cho"
r vcs publish --store "$d/store" "$d/pkg.cho"; exits 1 "no --std"
has "$err" "--std" "the refusal names the flag"
[ -e "$d/store/manifest.json" ] && fail "a refused publish wrote a manifest"
end

begin publish_with_std_logs_only_the_packages_own_functions
fresh std2; publish_pkg
same "$(count_in "$d/store/ops")" 2 "pkg.abs and pkg.clamp3, none of std's own"
r vcs log --store "$d/store"; succeeds log
has "$out" abs "abs is logged"; has "$out" clamp3 "clamp3 is logged"
same "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" 2 "two rows"
rows_std=$(printf '%s\n' "$out" | grep -c 'std$')
same "$rows_std" 2 "each row is marked std"
end

begin a_std_package_resolves_locks_fetches_and_a_consumer_runs_it
fresh std3; publish_pkg
r vcs resolve "$d/store"; succeeds resolve
lock "$d/store" "$d/cancho.lock" abs clamp3
fetch "$d/cancho.lock" "$d/store" "$d/fetched"; succeeds fetch
has "$out" "--std" "fetch says the consumer needs it"
same "$(count_in "$d/fetched")" 1 "std is the compiler's, never fetched into the directory"
consumer "$d/consumer.cho"
r build --std "$d/consumer.cho" "$d"/fetched/*.cho -o "$d/consumer"; succeeds "the consumer builds with --std"
"$d/consumer" > /dev/null 2>&1
same "$?" 108 "pkg.abs(-5) = 5 + 100; pkg.clamp3(9) = 3"
# And without --std the same consumer is refused, rather than building against something that is not there.
r build "$d/consumer.cho" "$d"/fetched/*.cho -o "$d/bare"; [ "$code" != 0 ] || fail "the consumer built without --std"
end

begin resolve_detects_std_from_the_source_not_from_the_manifest_flag
fresh std4; publish_pkg
has "$(cat "$d/store/manifest.json")" '"uses_std": true' "the manifest says so"
sed 's/"uses_std": true/"uses_std": false/' "$d/store/manifest.json" > "$d/m.json" && mv "$d/m.json" "$d/store/manifest.json"
r vcs resolve "$d/store"; succeeds "a forged flag"
end

begin a_manifest_written_before_the_flag_existed_still_loads
fresh std5; printf 'fn add(a: int, b: int) -> [] int { return a + b; }\n' > "$d/f.cho"
publish "$d/store" "$d/f.cho"
grep -v uses_std "$d/store/manifest.json" > "$d/m.json"
# The line before `uses_std` ends in a comma; drop it so the JSON is valid.
sed -e ':a' -e 'N' -e '$!ba' -e 's/,\n    }/\n    }/g' "$d/m.json" > "$d/store/manifest.json"
hasnt "$(cat "$d/store/manifest.json")" uses_std "the flag is gone"
r vcs log --store "$d/store"; succeeds "log of an old manifest"
has "$out" add "the old manifest still loads"
end

begin a_package_requiring_a_std_package_must_be_published_with_std_too
fresh std6; publish_pkg
lock "$d/store" "$d/dep.lock" clamp3
# This file never mentions `std`; its dependency does.
printf 'module top;\nimport pkg;\npub fn capped(n: int) -> [] int { return pkg.clamp3(n); }\n' > "$d/top.cho"
r vcs publish --store "$d/top-store" --requires "$d/dep.lock:$d/store" "$d/top.cho"; exits 1 "without --std"
has "$err" "--std" "the refusal names the flag"
r vcs publish --std --store "$d/top-store" --requires "$d/dep.lock:$d/store" "$d/top.cho"; succeeds "with --std"
has "$out" "published capped" "the dependent is published"
r vcs resolve "$d/top-store"; succeeds "resolving the closure brings the library in on its own"
end
