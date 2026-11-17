# vcs_remote.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/vcs_remote.rs (docs/gate.md G4, wave 4): `cancho vcs` with a store in another repository (an origin on a lock,
# fetched through a cache of git checkouts) and a library of several files published as a directory (docs/package-system.md section 7). Every repository is a local one made with `git`, so nothing touches a
# network. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh
root=$(pwd)

# Where the cache of checkouts is said: the compiler in cancho reads no environment and takes `--cache DIR`; the one in Rust read `CANCHO_CACHE` and refuses the flag.
if "$cancho" vcs fetch --cache /nonexistent 2>&1 | grep -q 'unknown option'; then flag_cache=env; else flag_cache=flag; fi
# vcsc ARGS...: `cancho vcs ARGS` with the cache of this check ($cache), like `r`.
vcsc() {
    if [ "$flag_cache" = env ]; then CANCHO_CACHE=$cache r vcs "$@"; else r vcs "$@" --cache "$cache"; fi
}
gitq() { _g=$1; shift; GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$_g" -c user.email=t@example.com -c user.name=t -c commit.gpgsign=false "$@"; }

# A library of two modules: `libx.base` has a `static` (a table, the thing `vcs publish` could not publish before section 7.4) and a function that reads it; `libx.top` imports it.
base() { # FILE ZLIB-NAME T2 T3: libx.base with t[2] = $3 and t[3] = $4
    cat > "$1" <<EOF
module ${2:-libx.base};

static table: [int] {
    let t = alloc_slice[static](4, 0);
    t[0] = 10;
    t[1] = 20;
    t[2] = ${3:-30};
    t[3] = ${4:-40};
    return t;
}

pub fn pick(i: int) -> [] int {
    return table[i];
}
EOF
}
top() { # FILE TOP-NAME BASE-NAME
    cat > "$1" <<EOF
module ${2:-libx.top};

import ${3:-libx.base};

pub fn twice(i: int) -> [] int {
    return base.pick(i) * 2;
}
EOF
}
# library_repo TAG: $w (the check's directory), $repo (a repository holding libx: the sources in src/, the published stores in .cancho-vcs/, committed), $cache and $rev.
library_repo() {
    fresh "$1"; w=$d; repo=$w/libx; cache=$w/cache
    mkdir -p "$repo/src"
    base "$repo/src/base.cho"; top "$repo/src/top.cho"
    gitq "$repo" init --quiet
    r vcs publish --dir "$repo/src" --store "$repo/.cancho-vcs"; succeeds "publishing libx"
    gitq "$repo" add -A
    gitq "$repo" commit --quiet -m libx
    rev=$(gitq "$repo" rev-parse HEAD)
}
lock_top() { # OUT [REV]: a lock of libx.top at REV (the head by default)
    vcsc lock --git "$repo" --rev "${2:-$rev}" --path .cancho-vcs/libx.top -o "$1" --all
}
# The consumer of libx.top: twice(2) is pick(2) * 2 = 60.
app() {
    cat > "$1" <<'EOF'
import libx.top;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return top.twice(2);
}
EOF
}

# ---- section 7.4: publishing a directory ----

begin a_directory_with_a_static_and_an_import_publishes_in_dependency_order
fresh dir1; mkdir "$d/src"
# Module names and file names that sort the wrong way round: `alib.top` comes first alphabetically and imports `zlib.base`.
top "$d/src/a_top.cho" alib.top zlib.base; base "$d/src/b_base.cho" zlib.base
r vcs publish --dir "$d/src" --store "$d/stores"; succeeds "publish --dir"
base_at=$(printf '%s\n' "$out" | grep -n '== zlib.base' | head -1 | cut -d: -f1)
top_at=$(printf '%s\n' "$out" | grep -n '== alib.top' | head -1 | cut -d: -f1)
if [ -z "$base_at" ] || [ -z "$top_at" ]; then fail "both modules are published"; elif [ "$base_at" -gt "$top_at" ]; then fail "a module is published after what it imports"; fi
has "$out" "published table" "a static is published like any declaration"; has "$out" "published twice" "and the function that reads it"
[ -d "$d/stores/alib.top/requires" ] || fail "the import did not become a requirement"
r vcs resolve "$d/stores/alib.top"; succeeds resolve
has "$out" "verified transitively" "the closure holds together"
end

begin publishing_a_directory_twice_writes_the_same_bytes_and_survives_an_edit
fresh dir2; mkdir "$d/src"; base "$d/src/base.cho"; top "$d/src/top.cho"
r vcs publish --dir "$d/src" --store "$d/a"; succeeds "the first publish"
r vcs publish --dir "$d/src" --store "$d/b"; succeeds "the second publish"
diff -r "$d/a" "$d/b" > /dev/null 2>&1 || fail "the same source did not publish to the same bytes"
# An edit to a declaration is refused by a plain `publish` (incremental publish is not built) and must not be by a directory publish.
base "$d/src/base.cho" libx.base 31 40
r vcs publish --dir "$d/src" --store "$d/a"; succeeds "a directory publish after an edit"
r vcs resolve "$d/a/libx.top"; succeeds resolve
has "$out" "still resolve" "the edited library resolves"
end

begin a_directory_refuses_an_import_it_cannot_place_and_a_cycle
fresh dir3; mkdir "$d/src" "$d/cyc"
top "$d/src/top.cho"   # imports libx.base, which is not here
r vcs publish --dir "$d/src" --store "$d/s"; [ "$code" != 0 ] || fail "an import with no module was published"
has "$err" libx.base "the module is named"
either "$err" 'neither `std` nor a module' "is not in scope" "and what is wrong"
printf 'module p;\nimport q;\npub fn f() -> [] int { return 1; }\n' > "$d/cyc/p.cho"
printf 'module q;\nimport p;\npub fn g() -> [] int { return 2; }\n' > "$d/cyc/q.cho"
r vcs publish --dir "$d/cyc" --store "$d/s2"; [ "$code" != 0 ] || fail "a cycle was published"
has "$err" "import each other" "a cycle is named"
end

begin a_program_in_the_directory_is_skipped_not_published
fresh dir4; mkdir "$d/src"; base "$d/src/base.cho"
printf 'fn main(world: World) -> [] int { return 0; }\n' > "$d/src/prog.cho"
r vcs publish --dir "$d/src" --store "$d/s"; succeeds "publish --dir"
has "$out" skipped "a program is skipped"; has "$out" prog.cho "and named"
[ -e "$d/s/main" ] && fail "the program was published"
end

# ---- section 7.3: an origin on the lock ----

begin a_lock_with_an_origin_fetches_the_closure_from_a_git_repository_and_it_builds
library_repo orig1
lock_top "$w/top.lock"; succeeds "lock --git"
has "$out" "locked twice" "the lock says what it pinned"
text=$(cat "$w/top.lock")
has "$text" '"origin"' "the lock records where"; has "$text" "$rev" "and at which commit"
# No `--store`: the origin says where. The closure comes with it: `top` requires `base`, in the same repository.
vcsc fetch --lock "$w/top.lock" -o "$w/fetched"; succeeds fetch
same "$(count_in "$w/fetched")" 2 "the top and, through its requirement, the base"
if [ -d "$cache/git/$rev/.cancho-vcs" ] && [ ! -e "$cache/git/$rev/.git" ]; then :; else fail "the cache holds the tree, not a repository"; fi
app "$w/app.cho"
consume "$w/app.cho" "$w/app" "$w"/fetched/*.cho
same "$code" 60 "twice(2) is pick(2) * 2 = 60"
end

begin a_second_fetch_is_served_from_the_cache_without_git
library_repo orig2
lock_top "$w/top.lock"; succeeds "lock --git"
# Delete the repository and take `git` off the path: only the cache is left.
rm -rf "$repo"
if [ "$flag_cache" = env ]; then
    PATH=$w/no-such-dir CANCHO_CACHE=$cache "$cancho" vcs fetch --lock "$w/top.lock" -o "$w/fetched" > /dev/null 2>&1
else
    PATH=$w/no-such-dir "$cancho" vcs fetch --lock "$w/top.lock" -o "$w/fetched" --cache "$cache" > /dev/null 2>&1
fi
same "$?" 0 "a fetch from the cache alone"
same "$(count_in "$w/fetched")" 2 "both files"
end

begin a_cached_checkout_that_was_changed_is_refused_and_nothing_is_written
library_repo orig3
lock_top "$w/top.lock"; succeeds "lock --git"
blob=$(ls "$cache/git/$rev/.cancho-vcs/libx.top/sources"/*.cho | head -1)
sed 's/\* 2/* 3/' "$blob" > "$blob.edit" && mv "$blob.edit" "$blob"
vcsc fetch --lock "$w/top.lock" -o "$w/fetched"; [ "$code" != 0 ] || fail "a changed source was accepted"
same "$(count_in "$w/fetched")" 0 "nothing is written when a pin fails"
end

begin a_lock_holds_a_commit_and_never_a_name
library_repo orig4
branch=$(gitq "$repo" rev-parse --abbrev-ref HEAD)
vcsc lock --git "$repo" --rev "$branch" -o "$w/top.lock" --all; [ "$code" != 0 ] || fail "a branch name was a pin"
has "$err" "full commit hash" "the refusal says why"
# `--ref` is looked up once, and the hash is what is written.
vcsc lock --git "$repo" --ref "$branch" --path .cancho-vcs/libx.top -o "$w/top.lock" --all; succeeds "lock --ref"
text=$(cat "$w/top.lock")
has "$text" "$rev" "the hash is what is written"; hasnt "$text" "\"rev\": \"$branch\"" "and not the name"
# A path that leaves the repository is not a store.
for nopath in ../elsewhere /etc a/../../b ""; do
    vcsc lock --git "$repo" --rev "$rev" --path "$nopath" -o "$w/p.lock" --all
    [ "$code" != 0 ] || fail "--path '$nopath' was accepted"
done
# A hand-edited lock that holds a name is refused where it is read.
sed "s/$rev/$branch/" "$w/top.lock" > "$w/edited.lock"
vcsc fetch --lock "$w/edited.lock" -o "$w/o"; [ "$code" != 0 ] || fail "a lock holding a name was read"
either "$err" "bad origin" "full commit hash" "the refusal says why"
end

begin moving_the_branch_changes_nothing_a_lock_already_pins
library_repo orig5
lock_top "$w/top.lock"; succeeds "lock --git"
# The library moves on: an edit, republished and committed.
base "$repo/src/base.cho" libx.base 99 40
r vcs publish --dir "$repo/src" --store "$repo/.cancho-vcs"; succeeds "republishing libx"
gitq "$repo" add -A
gitq "$repo" commit --quiet -m moved
vcsc fetch --lock "$w/top.lock" -o "$w/fetched"; succeeds fetch
all=$(cat "$w"/fetched/*.cho)
has "$all" "t[2] = 30;" "the pinned commit"; hasnt "$all" "t[2] = 99;" "not the head"
end

begin an_explicit_store_wins_over_the_origin
library_repo orig6
lock_top "$w/top.lock"; succeeds "lock --git"
# The same store, named: works with no cache and no git.
if [ "$flag_cache" = env ]; then
    PATH=$w/no-such-dir CANCHO_CACHE=$w/empty-cache "$cancho" vcs fetch --lock "$w/top.lock" --store "$repo/.cancho-vcs/libx.top" -o "$w/fetched" > /dev/null 2>&1
else
    PATH=$w/no-such-dir "$cancho" vcs fetch --lock "$w/top.lock" --store "$repo/.cancho-vcs/libx.top" -o "$w/fetched" --cache "$w/empty-cache" > /dev/null 2>&1
fi
same "$?" 0 "a fetch from a named store"
same "$(count_in "$w/fetched")" 2 "both files"
end

begin a_package_in_another_repository_requires_through_the_origin
# `libx` is one repository; `liby` is another and requires `libx.top` by a lock that has an origin, with no store path that would mean anything inside `liby`.
library_repo orig7
lock_top "$w/libx.lock"; succeeds "lock --git"
mkdir "$w/liby"
printf 'module liby;\nimport libx.top;\npub fn thrice(i: int) -> [] int { return top.twice(i) + top.twice(i) / 2; }\n' > "$w/liby/y.cho"
ystore=$w/liby/.cancho-vcs
# `--requires <lock>` alone: the lock says where the store is.
vcsc publish --store "$ystore" --requires "$w/libx.lock" "$w/liby/y.cho"; succeeds "publish --requires <lock>"
recorded=$(cat "$ystore/requires/0.json")
has "$recorded" '"origin"' "the requirement records the origin"; has "$recorded" "$rev" "and the commit"
has "$recorded" '"store": ""' "no machine-specific path is recorded"
gitq "$w/liby" init --quiet
gitq "$w/liby" add -A
gitq "$w/liby" commit --quiet -m liby
yrev=$(gitq "$w/liby" rev-parse HEAD)
# A consumer on a machine with an empty cache: everything comes from the two origins.
cache=$w/fresh-cache
vcsc lock --git "$w/liby" --rev "$yrev" -o "$w/liby.lock" --all; succeeds "lock liby"
vcsc fetch --lock "$w/liby.lock" -o "$w/fetched"; succeeds "fetch liby"
same "$(count_in "$w/fetched")" 3 "liby, and libx's top and base from the other repository"
end

begin a_fetch_that_fails_leaves_no_half_made_checkout
library_repo orig8
nowhere=0123456789abcdef0123456789abcdef01234567
lock_top "$w/x.lock" "$nowhere"; [ "$code" != 0 ] || fail "a commit the repository does not have was fetched"
[ -e "$cache/git/$nowhere" ] && fail "a directory in the cache is a complete checkout or it is not there"
left=$(ls -A "$cache/git" 2> /dev/null | grep -c '\.tmp\.')
same "$left" 0 "no temporary directory is left behind"
end

begin a_lock_pins_one_store_and_will_not_be_extended_with_another
library_repo orig9
lock_top "$w/top.lock"; succeeds "lock --git"
# The same library, one commit later: a different origin.
base "$repo/src/base.cho" libx.base 30 41
r vcs publish --dir "$repo/src" --store "$repo/.cancho-vcs"; succeeds "republishing libx"
gitq "$repo" add -A
gitq "$repo" commit --quiet -m later
later=$(gitq "$repo" rev-parse HEAD)
cp "$w/top.lock" "$w/before.lock"
lock_top "$w/top.lock" "$later"; [ "$code" != 0 ] || fail "a lock that pins one commit was quietly moved to another"
has "$err" "already pins a different store" "the refusal says why"
cmp -s "$w/top.lock" "$w/before.lock" || fail "the lock was not left as it was"
end

begin a_hook_in_the_template_directory_does_not_run
library_repo orig10
mkdir -p "$w/template/hooks"
printf '#!/bin/sh\ntouch "%s"\n' "$w/hook-ran" > "$w/template/hooks/post-checkout"
chmod 755 "$w/template/hooks/post-checkout"
GIT_TEMPLATE_DIR=$w/template lock_top "$w/h.lock"; succeeds "lock --git with a template"
[ -e "$w/hook-ran" ] && fail "git ran a hook from the dependency's checkout"
end

begin only_honest_transports_are_used
fresh trans; w=$d; cache=$w/cache
# A listener standing in for a cleartext server: a connection to it is the failure. It hangs up at once (tests/programs/accept_witness.cho).
"$cancho" build tests/programs/accept_witness.cho --std -o "$w/witness" > "$w/witness-build.txt" 2>&1 || fail "the listener did not build: $(head -c 300 "$w/witness-build.txt")"
port=0
tries=0
while [ $tries -lt 5 ]; do
    port=$(awk 'BEGIN { srand(); print 30000 + int(rand() * 20000) }')
    "$w/witness" "$port" > /dev/null 2> "$w/witness.out" &
    witness=$!
    sleep 0.4
    if kill -0 $witness 2> /dev/null && grep -q ready "$w/witness.out"; then break; fi
    tries=$((tries + 1))
done
for url in "ext::sh -c 'touch $w/ran'" "http://127.0.0.1:$port/r"; do
    vcsc lock --git "$url" --rev 0123456789abcdef0123456789abcdef01234567 -o "$w/t.lock" --all
    [ "$code" != 0 ] || fail "'$url' was accepted"
done
sleep 0.3
kill -9 $witness 2> /dev/null; wait $witness 2> /dev/null
[ -e "$w/ran" ] && fail "an ext:: url ran its command"
has "$(cat "$w/witness.out")" ready "the listener was up"
hasnt "$(cat "$w/witness.out")" connected "git connected over cleartext http"
end
