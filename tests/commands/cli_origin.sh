# cli_origin.sh -- the script of tests/commands/script_tests.cho for `conformance/selfhost/origin.rs` (deleted): the origin cache in cancho (`examples/selfhost/cliorigin.cho`,
# `docs/cli-in-cancho.md` section 22) makes the checkout that the Rust code (`vcs_origin.rs`, reached through `cancho add`) made. $1 is the compiler, $2 a scratch directory; run from the root of the
# repository. The oracle was the Rust `cancho add DEP URL --rev R --path P`; it was run once on the same repository (2026-11-17) and its answers are in the transcript: the tree of each checkout, the
# directories the cache holds, and for each refusal the sentence the Rust command said, which the command in cancho's first line of standard error must be part of (`rust said`). The two commits have
# fixed dates, so their hashes are the same on every machine and are written in the transcript.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_DATE=2026-01-01T00:00:00Z GIT_COMMITTER_DATE=2026-01-01T00:00:00Z

begin the_origin_cache_in_cancho_makes_the_checkout_the_rust_code_makes
git_() { git -c user.email=a@b -c user.name=n -c commit.gpgsign=false "$@"; }
# The tool: cliorigin.cho run on its own, built by the compiler under test with the files of the command line (cancho.files, less cli.cho).
files=$root/examples/selfhost/origincheck.cho
while read -r f; do [ -n "$f" ] && [ "$f" != cli.cho ] && files="$files $root/examples/selfhost/$f"; done < examples/selfhost/cancho.files
if ! "$cancho" build $files --std -o "$work/origincheck" > "$work/build.txt" 2>&1; then fail "origincheck did not build: $(head -c 300 "$work/build.txt")"; end; exit 0; fi
tool=$work/origincheck

repo=$work/repo
mkdir -p "$repo/.cancho-vcs/inner" "$repo/other"
( cd "$repo" || exit 1
  git_ init --quiet .
  echo '{}' > .cancho-vcs/manifest.json
  echo inner > .cancho-vcs/inner/a.txt
  echo other > other/b.txt
  git_ add .
  git_ commit --quiet -m one )
first=$(cd "$repo" && git_ rev-parse HEAD)
echo changed > "$repo/other/b.txt"
( cd "$repo" && git_ commit --quiet -am two && git_ tag v1 "$first" )
second=$(cd "$repo" && git_ rev-parse HEAD)
url=file://$repo
echo "commits: $first $second"
cache=$work/cache
# mine ARGS...: the tool's status, and its standard output and error in $work/o.txt and $work/e.txt.
mine() { "$tool" "$@" > "$work/o.txt" 2> "$work/e.txt"; s=$?; }
# tree DIR: every file beneath DIR with the checksum of its text, and every directory with a `/`, sorted, relative to DIR.
tree() { ( cd "$1" && find . -mindepth 1 | LC_ALL=C sort | while read -r p; do if [ -d "$p" ]; then echo "  $p/"; else echo "  $p $(ck "$p")"; fi; done ); }
listing() { ls "$1/git" 2> /dev/null | LC_ALL=C sort | tr '\n' ' '; }

for rev in "$first" "$second"; do
    mine ensure "$cache" "$url" "$rev" .cancho-vcs
    same "$s" 0 "ensure $rev: $(cat "$work/e.txt")"
    case $(cat "$work/o.txt") in *"git/$rev/.cancho-vcs") ;; *) fail "the store directory of $rev: $(cat "$work/o.txt")" ;; esac
    echo "checkout of $rev:"
    tree "$cache/git/$rev"
    tree "$cache/git/$rev" | grep -q '  \./\.git' && fail "a .git in the checkout of $rev"
done
# Only the commits asked for are there, and nothing half made.
echo "the cache holds: $(listing "$cache")"
# A second ask changes nothing (the cache is not asked again).
mine ensure "$cache" file:///nowhere "$first" .cancho-vcs
same "$s" 0 "a cached commit asks nothing of the repository"
echo "a second ask: exit $s"

# The refusals: a fresh cache each, and what the first line of standard error was, as the Rust command said it (on its standard error, which contains it).
refuse() { # NAME REV PATH RUST-SAID
    rm -rf "$cache"
    mine ensure "$cache" "$url" "$2" "$3"
    [ "$s" != 0 ] || fail "$1: was not refused"
    said=$(head -1 "$work/e.txt")
    [ -n "$said" ] || fail "$1: said nothing"
    case $4 in *"$said"*) ;; *) fail "$1: the Rust command said '$4', not '$said'" ;; esac
    echo "$1: exit $s, said: $(printf "%s" "$said" | sed "s|$url|URL|g; s|$work|WORK|g"), the cache holds: $(listing "$cache")"
}
refuse "no such path" "$first" nothere "commit $first of \`$url\` has no directory \`nothere\`"
refuse "not a hash" abc .cancho-vcs "origin \`rev\` must be a full commit hash (40 or 64 lowercase hex digits), found \`abc\`; a branch or tag name moves, so a lock never holds one"
refuse "no such commit" 0000000000000000000000000000000000000000 .cancho-vcs "\`git\` failed to check out the commit: fatal: unable to read tree (0000000000000000000000000000000000000000)"
refuse "an escaping path" "$first" ../x "origin \`path\` must be a relative path inside the repository, found \`../x\`"
refuse "an absolute path" "$first" /x "origin \`path\` must be a relative path inside the repository, found \`/x\`"

# A ref is resolved to the hash it names now (the Rust command said `HEAD is HASH` and `v1 is HASH`), and one that is not there is refused with the same words.
mine ref "$url" HEAD; echo "ref HEAD: exit $s $(cat "$work/o.txt")"
same "$(cat "$work/o.txt")" "$second" "ref HEAD"
mine ref "$url" v1; echo "ref v1: exit $s $(cat "$work/o.txt")"
same "$(cat "$work/o.txt")" "$first" "ref v1"
mine ref "$url" nope
said=$(head -1 "$work/e.txt")
same "$s" 3 "ref nope"
case "\`$url\` has no ref \`nope\`" in *"$said"*) echo "ref nope: exit $s, said: $(printf "%s" "$said" | sed "s|$url|URL|g")" ;; *) fail "ref nope said '$said'" ;; esac
for loc in -x ext::sh; do
    mine ref "$loc" HEAD
    same "$s" 3 "ref $loc"
    same "$(cat "$work/e.txt")" "not a repository location: \`$loc\`" "ref $loc says"
    echo "ref $loc: exit $s, said: $(cat "$work/e.txt")"
done
mine ref "$url" -x
same "$s $(cat "$work/e.txt")" "3 not a ref name: \`-x\`" "ref -x"
echo "ref -x: exit $s, said: $(cat "$work/e.txt")"
# With no `--cache`, the root is `.cache/cancho` in the home directory.
if [ -n "${HOME:-}" ]; then
    mine root
    same "$(cat "$work/o.txt")" "$HOME/.cache/cancho" "the cache root"
fi
end
