# cli_vcs_lock.sh -- the script of tests/commands/script_tests.cho that was `vcs_lock_in_cancho_pins_like_cancho_vcs_lock` of crates/cancho/tests/conformance/selfhost/vcs.rs (docs/gate.md G4, wave 4): `vcs lock` on the stores
# of packages/ pinned whole, by name (with escapes, a name given twice, one that is two stores' or no store's), an existing lock extended and refused, stores that are not there or not what they should be,
# a store in a git repository (by --rev and by --ref, by a path and by the default one), and every command line the Rust command refuses. NAME.expected is what the Rust command did, recorded on 2026-11-17
# (see tests/commands/vcs_diff_lib.sh). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_diff_lib.sh

# The manifest of six names, the lock a store has been pinned to before, and the ways a lock begins.
names_store() {
    {
        printf '%s\t%s\n' "$(sig 1)" '"alpha"'
        printf '%s\t%s\n' "$(sig 2)" '"beta"'
        printf '%s\t%s\n' "$(sig 3)" '"same"'
        printf '%s\t%s\n' "$(sig 4)" '"same"'
        printf '%s\t%s\n' "$(sig 5)" '"q\"uote\\back\u0001é"'
        printf '%s\t%s\n' "$(sig 6)" '"Zed"'
    } | manifest_lit "$d/st/manifest.json"
}
pinned() { printf '{\n    "old": {\n      "sig_id": "s",\n      "stage_id": "t",\n      "source_hash": "u"\n    },\n    "alpha": {\n      "sig_id": "stale",\n      "stage_id": "stale",\n      "source_hash": "stale"\n    }\n  }'; }
only_old() { printf '{\n    "old": {\n      "sig_id": "s",\n      "stage_id": "t",\n      "source_hash": "u"\n    }\n  }'; }
plain_lock() { printf '{\n  "entries": %s\n}' "$1"; }
origin_lock() { printf '{\n  "origin": {\n    "git": "%s",\n    "rev": "%s",\n    "path": "%s"\n  },\n  "entries": %s\n}' "$1" "$2" "$3" "$4"; }
good_rev=$(rep a 40)
# lk NAME ARGS...: a case on a store of names (a lock to extend in out.lock if LOCK is set); the steps are `lock ARGS`.
lk() { dcase "$1"; shift; names_store; [ -n "$LOCK" ] && put out.lock "$LOCK"; vstep lock "$@"; dend; }
# bare NAME ARGS...: a case with no files.
bare() { dcase "$1"; shift; vstep lock "$@"; dend; }
LOCK=

begin vcs_lock_in_cancho_pins_like_cancho_vcs_lock
# Every store of packages/, whole.
n=0
for rel in $(stores_of_packages); do
    n=$((n + 1))
    bare "a store of packages/, all: $rel" --store "$packages/$rel" -o out.lock --all
done
[ "$n" -ge 26 ] || fail "only $n stores in packages/"

lk "names" --store st -o out.lock alpha beta Zed
lk "escapes" --store st -o out.lock "$(printf 'q"uote\\back\001é')"
lk "ambiguous" --store st -o out.lock alpha same
lk "ambiguous all" --store st -o out.lock --all
lk "not published" --store st -o out.lock alpha nothing
lk "a name twice" --store st -o out.lock alpha alpha beta
dcase "no manifest"; put st/other x; vstep lock --store st -o out.lock --all; dend
bare "no store" --store absent -o out.lock alpha
dcase "empty store, name"; store_with "$d/st" < /dev/null; vstep lock --store st -o out.lock alpha; dend
dcase "bad manifest"; put st/manifest.json '{'; vstep lock --store st -o out.lock alpha; dend
LOCK=$(plain_lock "$(pinned)")
lk "extend a lock" --store st -o out.lock beta alpha
lk "extend, all" --store st -o out.lock beta
LOCK=$(origin_lock https://x/y "$good_rev" .cancho-vcs "$(pinned)")
lk "a lock with an origin, a store" --store st -o out.lock beta
LOCK=$(origin_lock https://x/y "$good_rev" .cancho-vcs '{}')
lk "an empty lock with an origin" --store st -o out.lock beta
LOCK=$(origin_lock https://x/y main .cancho-vcs "$(pinned)")
lk "a lock with a bad origin" --store st -o out.lock beta
LOCK='{'
lk "a lock that is not JSON" --store st -o out.lock beta
LOCK='{}'
lk "a lock without entries" --store st -o out.lock beta
LOCK=
lk "-o in a new directory" --store st -o a/b/out.lock beta
dcase "-o a directory"; names_store; put out.lock/x y; vstep lock --store st -o out.lock beta; dend
# The command lines.
lk "no -o" --store st beta
lk "no name" --store st -o out.lock
lk "--all and a name" --store st -o out.lock --all beta
lk "--git and --store" --store st --git x -o out.lock beta
lk "neither" -o out.lock beta
lk "--rev with --store" --store st --rev a -o out.lock beta
lk "--ref with --store" --store st --ref a -o out.lock beta
lk "--path with --store" --store st --path a -o out.lock beta
bare "--git alone" --git x -o out.lock beta
bare "--rev and --ref" --git x --rev a --ref b -o out.lock beta
bare "-o last" --store st -o
bare "--store last" -o x --store
bare "--git last" --git
bare "--rev last" --rev
bare "--ref last" --ref
bare "--path last" --path
bare "an option" --bogus
bare "nothing"
bare "a bad rev" --git https://x/y --rev main -o out.lock beta
bare "a bad git" --git --bogus --rev "$good_rev" -o out.lock beta
bare "a bad path" --git https://x/y --rev "$good_rev" --path /abs -o out.lock beta

# A store in a git repository: by commit, by tag and by head, the lock's origin written, an existing lock of the same origin extended and of another refused.
make_repo repository "$packages" packages
url=$repo_url; rev=$repo_rev
path=packages/net-sockets/.cancho-vcs
other=packages/http-response/.cancho-vcs
bare "git, rev, all" --git "$url" --rev "$rev" --path "$path" -o out.lock --all
bare "git, ref HEAD" --git "$url" --ref HEAD --path "$path" -o out.lock bind listen
dcase "git, same origin extended"; put out.lock "$(origin_lock "$url" "$rev" "$path" '{}')"; vstep lock --git "$url" --rev "$rev" --path "$path" -o out.lock bind; dend
dcase "git, another path"; put out.lock "$(origin_lock "$url" "$rev" "$path" "$(only_old)")"; vstep lock --git "$url" --rev "$rev" --path "$other" -o out.lock --all; dend
bare "git, a path that is not there" --git "$url" --rev "$rev" --path packages/nope -o out.lock --all
bare "git, a ref that is not there" --git "$url" --ref no-such-branch --path "$path" -o out.lock --all
bare "git, a commit that is not there" --git "$url" --rev "$(rep 1 40)" --path "$path" -o out.lock --all
# A repository whose store is at the default place, `.cancho-vcs` at its root: `--git` with no `--path`, and a lock whose origin does not say `path` (read as the default).
make_repo plain "$packages/net-sockets/.cancho-vcs" .cancho-vcs
purl=$repo_url; prev=$repo_rev
bare "git, the default path" --git "$purl" --rev "$prev" -o out.lock --all
bare "git, the default path, ref" --git "$purl" --ref HEAD -o out.lock bind
nopath=$(printf '{\n  "origin": {\n    "git": "%s",\n    "rev": "%s"\n  },\n  "entries": {\n    "old": {\n      "sig_id": "s",\n      "stage_id": "t",\n      "source_hash": "u"\n    }\n  }\n}' "$purl" "$prev")
dcase "an origin with no path, the same"; put out.lock "$nopath"; vstep lock --git "$purl" --rev "$prev" -o out.lock bind; dend
dcase "an origin with no path, another path"; put out.lock "$nopath"; vstep lock --git "$purl" --rev "$prev" --path other -o out.lock bind; dend
end
