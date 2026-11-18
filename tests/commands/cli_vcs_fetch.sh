# cli_vcs_fetch.sh -- the script of tests/commands/script_tests.cho that was `vcs_fetch_and_resolve_in_cancho_check_a_store_like_cancho_vcs` of crates/cancho/tests/conformance/selfhost/vcs.rs (docs/gate.md G4, wave 4):
# `vcs resolve` and `vcs fetch` on four stores of packages/ (no dependency, one dependency, a chain that imports `std`, four sources), whole and by lock, a lock that names a store by its origin,
# locks and stores that are not what they should be (an empty lock, no lock, a lock that is not JSON, a pin that is not published, a source changed, a source gone), and every command line the Rust command
# refuses. The locks are made by the command under test (`vcs lock`, which cli_vcs_lock.sh holds to the Rust command's). NAME.expected is what the Rust command did, recorded on 2026-11-17
# (see tests/commands/vcs_diff_lib.sh). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_diff_lib.sh

# rf NAME ARGS...: a case with no files that runs `vcs ARGS`; rl NAME LOCKFILE ARGS...: a case with the lock `a.lock`.
rf() { dcase "$1"; shift; vstep "$@"; dend; }
rl() { dcase "$1"; _l=$2; shift 2; putf a.lock "$_l"; vstep "$@"; dend; }

begin vcs_fetch_and_resolve_in_cancho_check_a_store_like_cancho_vcs
printf '{\n  "entries": {}\n}' > "$work/empty.lock"
printf '{\n  "entries": {\n    "gone": {\n      "sig_id": "%s",\n      "stage_id": "t",\n      "source_hash": "u"\n    }\n  }\n}' "$(sig 9)" > "$work/stale.lock"
printf '{' > "$work/bad.lock"
# The sampled stores.
for rel in net-sockets/.cancho-vcs http-response/.cancho-vcs x509/.cancho-vcs/x509_verify tls/.cancho-vcs/tls_hello; do
    store=$packages/$rel
    mklock whole.lock --store "$store" --all
    rl "$rel: resolve, the store" "$work/whole.lock" resolve "$store"
    rl "$rel: resolve, its lock" "$work/whole.lock" resolve --lock a.lock "$store"
    rl "$rel: fetch" "$work/whole.lock" fetch --lock a.lock --store "$store" -o out
    dcase "$rel: fetch into a directory that is there"; putf a.lock "$work/whole.lock"; put out/old kept; vstep fetch --lock a.lock --store "$store" -o out/; dend
done
store=$packages/net-sockets/.cancho-vcs
mklock two.lock --store "$store" bind listen
# Subsets of the pins.
rl "resolve, two pins" "$work/two.lock" resolve --lock a.lock "$store"
rl "fetch, two pins" "$work/two.lock" fetch --lock a.lock --store "$store" -o deeper/out
# Locks and stores that are not there, or not what they should be.
rl "resolve, an empty lock" "$work/empty.lock" resolve --lock a.lock "$store"
rl "fetch, an empty lock" "$work/empty.lock" fetch --lock a.lock --store "$store" -o out
rf "resolve, no lock file" resolve --lock absent.lock "$store"
rf "fetch, no lock file" fetch --lock absent.lock --store "$store" -o out
rl "resolve, a lock that is not JSON" "$work/bad.lock" resolve --lock a.lock "$store"
rl "fetch, a lock that is not JSON" "$work/bad.lock" fetch --lock a.lock --store "$store" -o out
rl "resolve, a pin that is not published" "$work/stale.lock" resolve --lock a.lock "$store"
rl "fetch, a pin that is not published" "$work/stale.lock" fetch --lock a.lock --store "$store" -o out
dcase "resolve, a store with nothing in it"; store_with "$d/st" < /dev/null; vstep resolve st; dend
rf "resolve, no store directory" resolve absent
rl "resolve, a lock with no origin and no store" "$work/two.lock" resolve --lock a.lock
rl "fetch, a lock with no origin and no store" "$work/two.lock" fetch --lock a.lock -o out
# The command lines.
rf "resolve, nothing" resolve
rf "resolve, two stores" resolve a b
rf "resolve, an option" resolve --bogus
rf "resolve, --lock last" resolve --lock
rf "resolve, --store" resolve --store x
rf "fetch, nothing" fetch
rl "fetch, no -o" "$work/empty.lock" fetch --lock a.lock
rf "fetch, no --lock" fetch -o out
rf "fetch, a word" fetch word
rf "fetch, an option" fetch --bogus
rf "fetch, --lock last" fetch --lock
rf "fetch, --store last" fetch --store
rf "fetch, -o last" fetch -o
# A store changed after it was published: a source that is not what its name says, one that is gone, a pin that moved.
copytree "$store" "$work/tampered"
mklock tampered.lock --store "$work/tampered" --all
first=$(ls "$work/tampered/sources" | LC_ALL=C sort | head -n 1)
printf 'module x;\n' > "$work/tampered/sources/$first"
addnorm "$work/tampered" '<tampered>'
rf "resolve, a source changed" resolve "$work/tampered"
rl "fetch, a source changed" "$work/tampered.lock" fetch --lock a.lock --store "$work/tampered" -o out
rm "$work/tampered/sources/$first"
rf "resolve, a source gone" resolve "$work/tampered"
# A lock that says where its store is: a commit of a git repository.
make_repo repository "$packages" packages
mklock origin.lock --git "$repo_url" --rev "$repo_rev" --path packages/net-sockets/.cancho-vcs --all
rl "resolve, by the origin" "$work/origin.lock" resolve --lock a.lock
rl "fetch, by the origin" "$work/origin.lock" fetch --lock a.lock -o out
rl "fetch, by the origin, the store named" "$work/origin.lock" fetch --lock a.lock --store "$store" -o out
end
