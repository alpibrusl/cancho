#!/bin/sh
# make_export_golden.sh RUST_CANCHO -- the goldens of `cancho vcs export-git` in tests/vcs_fixtures/golden_export/, made by the Rust command, which is the oracle (used only to measure, as `git` is in make_git.sh).
# For each repository of tests/vcs_fixtures/git/ that the Rust import takes (exit 0): NAME.json and NAME.code (the document and the exit status of an export of what was imported) and NAME.git (every file and
# directory of the git directory the export made, each file with its `cksum`). `odd` has its octopus branch taken out first (an octopus change is refused by the Rust command on load). The scenarios of
# export_scenarios.sh are exported the same way, and `dry_plain.json` is the dry run of plain.git. `refuse_*` are refusals.

cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
here=$(cd "$(dirname "$0")" && pwd)
out=$here/golden_export
work=$(mktemp -d)
rm -rf "$out"; mkdir -p "$out"
gitdir=$work/g
. "$here/export_scenarios.sh"
git_list() { ( cd "$1" && find . | sort | while read -r f; do if [ -d "$f" ]; then echo "$f/"; else echo "$f $(cksum < "$f" | tr ' ' ':')"; fi; done ); }
export_to() { ( cd "$work" && "$cancho" vcs export-git x.git --repo r --output json "$@" > "$out/$name.json" 2> /dev/null; echo $? > "$out/$name.code" ); }
mkdir -p "$work/g"; cp -R "$here/git/." "$work/g/"
for g in "$work"/g/*.git; do
    n=$(basename "$g" .git)
    rm -rf "$work/r" "$work/x.git"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null )
    ( cd "$work/g" && "$cancho" vcs import-git "$n.git" --no-checkout --repo "$work/r" --output json > /dev/null 2>&1 ) || continue
    [ "$n" = odd ] && rm -f "$work"/r/.cancho/vcs/refs/heads/octopus*
    name=$n; export_to
    git_list "$work/x.git" > "$out/$n.git"
done
for s in made mixed empty headonly; do
    rm -rf "$work/r" "$work/x.git"; mkdir "$work/r"; ( cd "$work/r" && "scn_$s" )
    name=$s; export_to
    git_list "$work/x.git" > "$out/$s.git"
done
rm -rf "$work/r" "$work/x.git"; mkdir "$work/r"; ( cd "$work/r" && scn_made )
name=dry_made; export_to --dry-run
rm -rf "$work/r" "$work/x.git"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null && "$cancho" vcs import-git ../g/plain.git --no-checkout --output json > /dev/null 2>&1 )
name=dry_plain; export_to --dry-run
# Refusals: no directory, two directories, no repository, a directory that cannot be made, a change that is not in the store, a change with three parents.
( cd "$work/r" && "$cancho" vcs export-git --output json > "$out/refuse_no_dir.json" 2> /dev/null; echo $? > "$out/refuse_no_dir.code" )
( cd "$work/r" && "$cancho" vcs export-git a b --output json > "$out/refuse_two_dirs.json" 2> /dev/null; echo $? > "$out/refuse_two_dirs.code" )
mkdir -p "$work/nr"; ( cd "$work/nr" && "$cancho" vcs export-git x.git --output json > "$out/refuse_no_repository.json" 2> /dev/null; echo $? > "$out/refuse_no_repository.code" )
: > "$work/afile"; ( cd "$work/r" && "$cancho" vcs export-git ../afile/sub.git --output json > "$out/refuse_unwritable.json" 2> /dev/null; echo $? > "$out/refuse_unwritable.code" )
rm -rf "$work/r" "$work/x.git"; mkdir "$work/r"; ( cd "$work/r" && scn_made )
c=$(cat "$work/r/.cancho/vcs/refs/heads/main"); rm -f "$work/r/.cancho/vcs/objects/change/$(echo "$c" | cut -c1-2)/$(echo "$c" | cut -c3-)"
name=refuse_missing_change; export_to
rm -rf "$work/r" "$work/x.git"; mkdir "$work/r"; ( cd "$work/r" && "$cancho" vcs init > /dev/null ); ( cd "$work/g" && "$cancho" vcs import-git odd.git --no-checkout --repo "$work/r" --output json > /dev/null 2>&1 )
name=refuse_octopus; export_to
rm -rf "$work"
echo "made $(ls "$out" | wc -l) files in $out"
