#!/bin/sh
# make_git_more.sh GITDIR [RUST_CANCHO] -- three more repositories for tests/vcs_fixtures/git/ (R15, `docs/vcs-routing.md` section 18), made as make_git.sh made the others (the `git` binary, as a
# measuring tool only), added to the ones already there so the others stay as they were:
#   many     205 commits in a line and a 206th that is refused (a submodule): the gitmap is written at the 200th and the refusal comes after it
#   trunk    HEAD is `refs/heads/t`, a branch of one letter that is not `main`
#   oddref   a branch `a+b` and a tag `v+1`, which git accepts and the store does not (`check_ref_name`)
set -eu
G=$(cd "$1" && pwd)
RUST=
if [ $# -ge 2 ]; then RUST=$(cd "$(dirname "$2")" && pwd)/$(basename "$2"); fi
W=$(mktemp -d)
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null HOME=$W
export GIT_AUTHOR_NAME="Ada" GIT_AUTHOR_EMAIL="ada@example.com" GIT_COMMITTER_NAME="Ada" GIT_COMMITTER_EMAIL="ada@example.com"
tick=1700100000
stamp() { tick=$((tick + 60)); export GIT_AUTHOR_DATE="$tick +0000" GIT_COMMITTER_DATE="$tick +0000"; }
bare() { # SOURCE NAME
  git clone -q --bare --template= "$1" "$G/$2.git"
  rm -f "$G/$2.git/config" "$G/$2.git/description"; rm -rf "$G/$2.git/hooks" "$G/$2.git/info/exclude"
  mkdir -p "$G/$2.git/refs/heads" "$G/$2.git/refs/tags"
  chmod -R u+w "$G/$2.git"
  ( cd "$G/$2.git" && git for-each-ref --format='%(refname) %(objectname)' | while read -r r o; do mkdir -p "$(dirname "$r")"; printf '%s\n' "$o" > "$r"; done; rm -f packed-refs )
}
mkdir "$W/many" && cd "$W/many"
git init -q -b main .
i=0
while [ $i -lt 205 ]; do i=$((i + 1)); stamp; printf 'file %s\n' "$i" > "f$((i % 7)).txt"; printf 'line %s\n' "$i" >> log.txt; git add -A; git commit -q -m "commit $i"; done
stamp; git update-index --add --cacheinfo 160000,0123456789abcdef0123456789abcdef01234567,sub; git commit -q -m "a submodule"
bare "$W/many" many
git -C "$G/many.git" repack -adq && git -C "$G/many.git" prune-packed
mkdir "$W/trunk" && cd "$W/trunk"
git init -q -b t .
stamp; printf 'one\n' > a.txt; git add -A; git commit -q -m "first"
stamp; printf 'two\n' > a.txt; git commit -q -am "second"
bare "$W/trunk" trunk
mkdir "$W/oddref" && cd "$W/oddref"
git init -q -b main .
stamp; printf 'one\n' > a.txt; git add -A; git commit -q -m "first"
git branch a+b; git tag v+1; git tag -a v2 -m "two"
bare "$W/oddref" oddref
if [ -n "$RUST" ]; then # the dry-run documents of the Rust command, in tests/vcs_fixtures/golden_git/
  R=$RUST
  g=$(dirname "$G")/golden_git
  mkdir "$W/r" && ( cd "$W/r" && "$R" vcs init --output json > /dev/null )
  for n in many trunk oddref; do ( cd "$G" && "$R" vcs import-git "$n.git" --dry-run --repo "$W/r" --output json > "$g/$n.json"; echo $? > "$g/$n.code" ) || true; done
fi
cd /; rm -rf "$W"
echo "made many trunk oddref in $G"
