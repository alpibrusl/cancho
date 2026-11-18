#!/bin/sh
# make_import_golden.sh RUST_CANCHO -- the goldens of `cancho vcs import-git` (not `--dry-run`) in tests/vcs_fixtures/golden_import/, made by the Rust command, which is the oracle
# (used only to measure, as `git` is in make_git.sh). For each repository of tests/vcs_fixtures/git/: NAME.json and NAME.code (the document and the exit status of an import into
# a repository made by `vcs init`, `--no-checkout`), NAME.store (the files of `.cancho/vcs` but the logs and `tmp`, each with its `cksum`; a refused import is listed without its
# `objects`, because the Rust command leaves the blobs it read before it refused and the command in cancho writes nothing for a commit it refuses), NAME.wc (the working copy after an
# import that checks out), and for a few: the second import (`again`) and an import into a repository whose `heads/main` is another change (`kept`).

cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
here=$(cd "$(dirname "$0")" && pwd)
out=$here/golden_import
work=$(mktemp -d)
rm -rf "$out"; mkdir -p "$out"
store_list() { ( cd "$1" && find .cancho -type f ! -name '*.log' ! -path '*/tmp/*' | sort | while read -r f; do echo "$f $(cksum < "$f" | tr ' ' ':')"; done ); }
wc_list() { ( cd "$1" && find . -path ./.cancho -prune -o -type f -print -o -type l -print | sort | while read -r f; do if [ -L "$f" ]; then echo "$f -> $(readlink "$f")"; else echo "$f $(cksum < "$f" | tr ' ' ':') $([ -x "$f" ] && echo x)"; fi; done ); }
mkdir -p "$work/g"; cp -R "$here/git/." "$work/g/"
for g in "$work"/g/*.git; do
    n=$(basename "$g" .git)
    rm -rf "$work/r"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null )
    ( cd "$work/g" && "$cancho" vcs import-git "$n.git" --no-checkout --repo "$work/r" --output json > "$out/$n.json" 2> /dev/null; echo $? > "$out/$n.code" )
    if [ "$(cat "$out/$n.code")" = 0 ]; then store_list "$work/r" > "$out/$n.store"; else store_list "$work/r" | grep -v '^\.cancho/vcs/objects/' > "$out/$n.store"; fi
    rm -rf "$work/r"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null )
    ( cd "$work/g" && "$cancho" vcs import-git "$n.git" --repo "$work/r" --output json > /dev/null 2>&1 ) || true
    wc_list "$work/r" > "$out/$n.wc"
done
rm -rf "$work/r"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null )
( cd "$work/g" && "$cancho" vcs import-git plain.git --no-checkout --repo "$work/r" --output json > /dev/null 2>&1; "$cancho" vcs import-git plain.git --no-checkout --repo "$work/r" --output json > "$out/again.json" 2> /dev/null; echo $? > "$out/again.code" )
store_list "$work/r" > "$out/again.store"
rm -rf "$work/r"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null )
( cd "$work/g" && "$cancho" vcs import-git plain.git --no-checkout --repo "$work/r" --output json > /dev/null 2>&1; "$cancho" vcs import-git headers.git --no-checkout --repo "$work/r" --output json > "$out/kept.json" 2> /dev/null; echo $? > "$out/kept.code" )
store_list "$work/r" > "$out/kept.store"
rm -rf "$work/r"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null ); cp "$here/golden_git/plain_known.gitmap" "$work/r/.cancho/vcs/gitmap"
( cd "$work/g" && "$cancho" vcs import-git plain.git --no-checkout --repo "$work/r" --output json > "$out/known.json" 2> /dev/null; echo $? > "$out/known.code" )
store_list "$work/r" > "$out/known.store"
rm -rf "$work"
echo "made $(ls "$out" | wc -l) files in $out"
