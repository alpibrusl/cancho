# vcs_publish.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/vcs.rs (docs/gate.md G4, wave 4): `cancho vcs publish`, `log`, `resolve`, `lock` and `fetch` against
# a store on disk, and a package that requires a package (docs/package-system.md section 4.6). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh
root=$(pwd)

# The hash a blob is named by is BLAKE3 of its text: a cancho program computes it (compiler/blake3sum.cho).
"$cancho" build compiler/blake3sum.cho compiler/blake3.cho compiler/lexcore.cho --std -o "$work/b3sum" > "$work/b3build.txt" 2>&1 || { echo "FAILS the BLAKE3 program did not build"; cat "$work/b3build.txt"; }

# The one-module libraries of the dependency checks: leaf_fn returns $2.
leaf() { printf 'module %s.leaf;\npub fn leaf_fn() -> [] int { return %s; }\n' "$1" "$2" > "$3"; }
dependent() { printf 'module closure.dependent;\nimport closure.leaf;\npub fn dependent_fn() -> [] int { return leaf.leaf_fn() + 1; }\n' > "$1"; }
requires() { # STORE FILE LOCK:STORE... -> a publish with one --requires each
    _store=$1; _file=$2; shift 2
    _flags=
    for _p in "$@"; do _flags="$_flags --requires $_p"; done
    # shellcheck disable=SC2086
    r vcs publish --store "$_store" $_flags "$_file"
}

# A consumer that returns what dep.CALL returns.
consumer() { # FILE MODULE CALL
    printf 'import %s;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io); release(ffi); release(fs); release(heap); release(args);\n    return %s;\n}\n' "$2" "$3" > "$1"
}

begin publish_logs_every_declaration_once
fresh pub1; two_functions "$d/f.cho"
r vcs publish --store "$d/store" "$d/f.cho"; succeeds publish
has "$out" "published add" "the first is said"; has "$out" "published sub" "the second is said"
[ -f "$d/store/manifest.json" ] || fail "no manifest"
same "$(count_in "$d/store/ops")" 2 "one AddFunction per declaration"
end

begin publishing_the_same_declarations_again_is_a_no_op
fresh pub2; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
r vcs publish --store "$d/store" "$d/f.cho"; succeeds "the second publish"
has "$out" "nothing new" "a second publish says so"
same "$(count_in "$d/store/ops")" 2 "no new op file"
end

begin changing_an_already_published_body_is_refused_not_silently_logged
fresh pub3; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
printf 'fn add(a: int, b: int) -> [] int { return a + b + 1; }\nfn sub(a: int, b: int) -> [] int { return a - b; }\n' > "$d/f.cho"
r vcs publish --store "$d/store" "$d/f.cho"; exits 1 "a changed body"
has "$err" "already published" "the refusal says why"
same "$(count_in "$d/store/ops")" 2 "a refused publish writes nothing"
end

begin log_lists_what_a_store_already_has
fresh log1; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
r vcs log --store "$d/store"; succeeds log
has "$out" add "log names add"; has "$out" sub "log names sub"
end

begin log_against_an_empty_store_says_so_rather_than_erroring
fresh log2
r vcs log --store "$d/store"; succeeds log
has "$out" "nothing published yet" "an empty store"
end

begin publishing_a_type_error_is_refused_with_its_rule_not_a_panic
fresh pub4
printf 'fn broken(a: int) -> [] int { return a + "nope"; }\n' > "$d/f.cho"
r vcs publish --store "$d/store" "$d/f.cho"; exits 1 "a type error"
either "$err" 'expected `int`' "does not match what the context requires" "the refusal is the checker's"
has "$err" "f.cho:1:" "and it has a position"
end

begin resolve_confirms_a_clean_store_matches_exactly
fresh res1; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
r vcs resolve "$d/store"; succeeds resolve
has "$out" "2 declaration(s)" "both are counted"; has "$out" "still resolve exactly as published" "the verdict"
end

begin resolve_against_an_empty_store_says_so_rather_than_erroring
fresh res2
r vcs resolve "$d/store"; succeeds resolve
has "$out" "nothing published" "an empty store"
end

begin resolve_refuses_a_pin_that_no_longer_matches_its_own_source
fresh res3; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
editjson "$d/store/manifest.json" sub stage_id bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
r vcs resolve "$d/store"; exits 1 "a drifted pin"
has "$err" sub "the pin is named"; has "$err" "no longer matches" "and what is wrong with it"
end

begin resolve_refuses_source_that_no_longer_typechecks
fresh res4; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
# A blob that is self-consistent (its name is its own hash) but does not type-check.
printf 'fn add(a: int, b: int) -> [] int { return a + unknown_name; }\nfn sub(a: int, b: int) -> [] int { return a - b; }\n' > "$d/broken.cho"
h=$("$work/b3sum" < "$d/broken.cho")
cp "$d/broken.cho" "$d/store/sources/$h.cho"
editjson "$d/store/manifest.json" add source_hash "$h"
r vcs resolve "$d/store"; exits 1 "a source that does not type-check"
either "$err" "no longer type-checks" "is not in scope" "the refusal says why"
end

begin resolve_refuses_a_pin_whose_source_blob_is_missing
fresh res5; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
rm -f "$d"/store/sources/*
r vcs resolve "$d/store"; exits 1 "a missing blob"
has "$err" "no source blob recorded" "the refusal says why"
end

begin lock_pins_a_name_to_its_current_hashes
fresh lock1; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
lock "$d/store" "$d/cancho.lock" add
has "$out" "locked add" "the lock says what it pinned"
text=$(cat "$d/cancho.lock")
has "$text" '"add"' "add is in the lock"; hasnt "$text" '"sub"' "only the locked name"
end

begin locking_an_unpublished_name_is_refused
fresh lock2; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
r vcs lock --store "$d/store" -o "$d/cancho.lock" nonexistent; exits 1 "an unpublished name"
has "$err" "is not published" "the refusal says why"
end

begin resolve_with_lock_scopes_to_just_the_locked_names
fresh lock3; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
lock "$d/store" "$d/cancho.lock" add
r vcs resolve --lock "$d/cancho.lock" "$d/store"; succeeds "resolve --lock"
has "$out" "1 declaration(s)" "only add was locked"
end

begin resolve_with_lock_against_a_store_that_never_had_it_is_refused
fresh lock4; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
lock "$d/store" "$d/cancho.lock" add
printf 'fn nine() -> [] int { return 9; }\n' > "$d/other.cho"
publish "$d/other-store" "$d/other.cho"
r vcs resolve --lock "$d/cancho.lock" "$d/other-store"; exits 1 "a store that never had it"
has "$err" add "the name"; has "$err" "no longer published" "the refusal says why"
end

begin resolve_with_an_unwritten_lock_file_says_so_rather_than_erroring
fresh lock5; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
r vcs resolve --lock "$d/never-written.lock" "$d/store"; succeeds "an unwritten lock"
has "$out" "nothing locked" "an unwritten lock file"
end

begin fetch_materializes_a_verified_file_a_real_program_can_import
fresh fetch1
printf 'module dep;\npub fn seven() -> [] int { return 7; }\n' > "$d/f.cho"
publish "$d/store" "$d/f.cho"
lock "$d/store" "$d/cancho.lock" seven
fetch "$d/cancho.lock" "$d/store" "$d/fetched"; succeeds fetch
has "$out" fetched "fetch says so"
same "$(count_in "$d/fetched")" 1 "one distinct source file was locked"
consumer "$d/consumer.cho" dep "dep.seven()"
consume "$d/consumer.cho" "$d/consumer" "$d"/fetched/*.cho
same "$code" 7 "main returns dep.seven()'s own 7"
end

begin fetch_deduplicates_a_file_shared_by_two_locked_names
fresh fetch2; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
lock "$d/store" "$d/cancho.lock" add sub
fetch "$d/cancho.lock" "$d/store" "$d/fetched"; succeeds fetch
same "$(count_in "$d/fetched")" 1 "add and sub came from one file, written once"
end

begin fetch_writes_nothing_when_a_pin_no_longer_matches
fresh fetch3; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
lock "$d/store" "$d/cancho.lock" add
editjson "$d/store/manifest.json" add stage_id bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
fetch "$d/cancho.lock" "$d/store" "$d/fetched"; exits 1 "a drifted pin"
has "$err" "no longer matches" "the refusal says why"
[ -e "$d/fetched" ] && fail "a refused fetch wrote something"
end

begin fetch_with_an_unwritten_lock_file_says_so_rather_than_erroring
fresh fetch4; two_functions "$d/f.cho"
publish "$d/store" "$d/f.cho"
fetch "$d/never-written.lock" "$d/store" "$d/fetched"; succeeds "an unwritten lock"
has "$out" "nothing locked" "an unwritten lock file"
end

begin publishing_an_unresolved_import_is_refused_with_the_missing_module_named
fresh req1; dependent "$d/dependent.cho"
requires "$d/store" "$d/dependent.cho"; exits 1 "an import with no --requires"
either "$err" 'no module `closure.leaf`' "import closure.leaf" "the missing module is named"
end

begin publish_with_requires_resolves_a_real_dependency_and_records_it
fresh req2; leaf closure 1 "$d/leaf.cho"
publish "$d/leaf-store" "$d/leaf.cho"
lock "$d/leaf-store" "$d/leaf.lock" leaf_fn
dependent "$d/dependent.cho"
requires "$d/dependent-store" "$d/dependent.cho" "$d/leaf.lock:$d/leaf-store"; succeeds "publish --requires"
has "$out" "published dependent_fn" "the dependent is published"
same "$(count_in "$d/dependent-store/requires")" 1 "one file per distinct dependency store"
has "$(cat "$d/dependent-store/requires/0.json")" leaf_fn "the requirement names what it pins"
end

begin resolve_walks_a_transitively_required_store
fresh req3; leaf closure 1 "$d/leaf.cho"
publish "$d/leaf-store" "$d/leaf.cho"
lock "$d/leaf-store" "$d/leaf.lock" leaf_fn
dependent "$d/dependent.cho"
requires "$d/dependent-store" "$d/dependent.cho" "$d/leaf.lock:$d/leaf-store"; succeeds "publish --requires"
r vcs resolve "$d/dependent-store"; succeeds resolve
has "$out" "1 declaration(s)" "the dependent's own declaration"; has "$out" "verified transitively" "the closure is walked"
end

begin fetch_materializes_the_whole_closure_and_a_consumer_builds_against_it
fresh req4; leaf closure 41 "$d/leaf.cho"
publish "$d/leaf-store" "$d/leaf.cho"
lock "$d/leaf-store" "$d/leaf.lock" leaf_fn
dependent "$d/dependent.cho"
requires "$d/dependent-store" "$d/dependent.cho" "$d/leaf.lock:$d/leaf-store"; succeeds "publish --requires"
lock "$d/dependent-store" "$d/consumer.lock" dependent_fn
fetch "$d/consumer.lock" "$d/dependent-store" "$d/fetched"; succeeds fetch
same "$(count_in "$d/fetched")" 2 "the dependent's own file and the leaf's"
consumer "$d/main.cho" closure.dependent "dependent.dependent_fn()"
consume "$d/main.cho" "$d/consumer" "$d"/fetched/*.cho
same "$code" 42 "leaf_fn() (41) + 1, through two fetched files"
end

begin resolve_refuses_a_dependency_cycle
fresh req5
printf 'module cycle.a;\npub fn a_fn() -> [] int { return 1; }\n' > "$d/a.cho"
printf 'module cycle.b;\npub fn b_fn() -> [] int { return 2; }\n' > "$d/b.cho"
publish "$d/a-store" "$d/a.cho"; publish "$d/b-store" "$d/b.cho"
lock "$d/b-store" "$d/a-needs-b.lock" b_fn; lock "$d/a-store" "$d/b-needs-a.lock" a_fn
# The cycle is wired on disk, the shape a real one would leave: each store requires the other.
mkdir -p "$d/a-store/requires" "$d/b-store/requires"
printf '{"store": "../b-store", "lock": %s}' "$(cat "$d/a-needs-b.lock")" > "$d/a-store/requires/0.json"
printf '{"store": "../a-store", "lock": %s}' "$(cat "$d/b-needs-a.lock")" > "$d/b-store/requires/0.json"
r vcs resolve "$d/a-store"; exits 1 "a cycle"
has "$err" "dependency cycle" "the cycle is named"
end

begin resolve_refuses_a_diamond_conflict
fresh req6
printf 'module diamond.leaf;\npub fn leaf_fn() -> [] int { return 1; }\npub fn leaf_fn2() -> [] int { return 2; }\n' > "$d/leaf.cho"
publish "$d/leaf-store" "$d/leaf.cho"
lock "$d/leaf-store" "$d/mid1-leaf.lock" leaf_fn
printf 'module diamond.mid1;\npub fn mid1_fn() -> [] int { return 10; }\n' > "$d/mid1.cho"
requires "$d/mid1-store" "$d/mid1.cho" "$d/mid1-leaf.lock:$d/leaf-store"; succeeds "mid1"
# mid2's lock is hand-edited: leaf_fn2's identity under the name leaf_fn, so two paths reach one (store, name) with two heads.
lock "$d/leaf-store" "$d/mid2-leaf.lock" leaf_fn2
sed 's/"leaf_fn2"/"leaf_fn"/' "$d/mid2-leaf.lock" > "$d/mid2-leaf.edited" && mv "$d/mid2-leaf.edited" "$d/mid2-leaf.lock"
printf 'module diamond.mid2;\npub fn mid2_fn() -> [] int { return 20; }\n' > "$d/mid2.cho"
requires "$d/mid2-store" "$d/mid2.cho" "$d/mid2-leaf.lock:$d/leaf-store"; succeeds "mid2"
lock "$d/mid1-store" "$d/top-mid1.lock" mid1_fn; lock "$d/mid2-store" "$d/top-mid2.lock" mid2_fn
printf 'module diamond.top;\npub fn top_fn() -> [] int { return 100; }\n' > "$d/top.cho"
requires "$d/top-store" "$d/top.cho" "$d/top-mid1.lock:$d/mid1-store" "$d/top-mid2.lock:$d/mid2-store"; exits 1 "a diamond"
has "$err" "diamond dependency" "the diamond is named"
end

begin two_paths_agreeing_on_the_same_pin_is_not_a_diamond
fresh req7
printf 'module agree.leaf;\npub fn leaf_fn() -> [] int { return 1; }\n' > "$d/leaf.cho"
publish "$d/leaf-store" "$d/leaf.cho"
lock "$d/leaf-store" "$d/mid1-leaf.lock" leaf_fn; lock "$d/leaf-store" "$d/mid2-leaf.lock" leaf_fn
printf 'module agree.mid1;\npub fn mid1_fn() -> [] int { return 10; }\n' > "$d/mid1.cho"
printf 'module agree.mid2;\npub fn mid2_fn() -> [] int { return 20; }\n' > "$d/mid2.cho"
requires "$d/mid1-store" "$d/mid1.cho" "$d/mid1-leaf.lock:$d/leaf-store"; succeeds "mid1"
requires "$d/mid2-store" "$d/mid2.cho" "$d/mid2-leaf.lock:$d/leaf-store"; succeeds "mid2"
lock "$d/mid1-store" "$d/top-mid1.lock" mid1_fn; lock "$d/mid2-store" "$d/top-mid2.lock" mid2_fn
printf 'module agree.top;\npub fn top_fn() -> [] int { return 100; }\n' > "$d/top.cho"
requires "$d/top-store" "$d/top.cho" "$d/top-mid1.lock:$d/mid1-store" "$d/top-mid2.lock:$d/mid2-store"; succeeds "two paths pinning one head compose"
end

# Four tables of one type and a function that reads two of them; `unread` is read by nothing.
tables() {
    cat > "$1" <<'EOF'
module tables;
pub static p: [int] { let h = alloc_slice[static](2, 0); h[0] = 1; return h; }
pub static q: [int] { let h = alloc_slice[static](2, 0); h[0] = 2; return h; }
pub static unread: [int] { let h = alloc_slice[static](2, 0); h[0] = 3; return h; }
pub fn sum() -> [] int { return p[0] + q[0]; }
EOF
}

begin two_statics_of_one_type_publish_resolve_fetch_and_republish_unchanged
fresh stat1; tables "$d/f.cho"
r vcs publish --store "$d/store" "$d/f.cho"; succeeds publish
for n in p q unread sum; do has "$out" "published $n" "$n is published"; done
r vcs publish --store "$d/store" "$d/f.cho"; succeeds "the second publish"
has "$out" "4 declaration(s) unchanged" "nothing changed"; hasnt "$out" "published " "nothing is published again"
r vcs resolve "$d/store"; succeeds resolve
has "$out" "4 declaration(s)" "all four resolve"
lock "$d/store" "$d/cancho.lock" p q unread sum
fetch "$d/cancho.lock" "$d/store" "$d/fetched"; succeeds fetch
same "$(count_in "$d/fetched")" 1 "one source file holds all four"
end

begin a_static_and_a_function_of_one_name_are_refused_rather_than_published
fresh stat2
printf 'static p: [int] { let h = alloc_slice[static](2, 0); return h; }\nfn p() -> [] int { return 1; }\n' > "$d/f.cho"
r vcs publish --store "$d/store" "$d/f.cho"; [ "$code" != 0 ] || fail "it was published"
either "$err" "has the name of a function" "is introduced once" "the refusal says why"
end
