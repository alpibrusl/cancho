#!/bin/sh
# Check `vcsscan.cho` and the `status` document of `vcsrepo.cho` against the Rust `cancho vcs` (`docs/vcs-native.md` section 6, slice b4c): for each state of a
# working copy, standard output and the exit status of `vcs status` must be equal, byte for byte.
#
# usage: examples/selfhost/vcsstatus_check.sh [CANCHO]
set -u
C=${1:-cancho}
case $C in */*) C=$(cd "$(dirname "$C")" && pwd)/$(basename "$C") ;; esac
D=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
"$C" build "$D/vcsstorectl.cho" "$D/vcsstore.cho" "$D/vcsrepo.cho" "$D/vcsscan.cho" "$D/vcschange.cho" "$D/jsonread.cho" "$D/jsonwrite.cho" "$D/rules.cho" "$D/tables.cho" "$D/vcsobj.cho" "$D/blake3.cho" "$D/filewrite.cho" "$D/lexcore.cho" --std -o "$W/ctl" >/dev/null || { echo "FAIL build"; exit 1; }
export CANCHO_AUTHOR="Ada <ada@example.com>"
R="$W/r"; mkdir "$R"; cd "$R" || exit 1
"$C" vcs init --output json > /dev/null

same() { # name
    ( cd "$R" && "$C" vcs status --output json > "$W/rust.out" 2> /dev/null; echo $? > "$W/rust.code" )
    "$W/ctl" status "$R" > "$W/cho.out" 2> /dev/null; echo $? > "$W/cho.code"
    if cmp -s "$W/rust.out" "$W/cho.out" && cmp -s "$W/rust.code" "$W/cho.code"; then echo "ok   $1"; else echo "FAIL $1 (rust exit $(cat "$W/rust.code"), cancho exit $(cat "$W/cho.code"))"; diff "$W/rust.out" "$W/cho.out" | head -8; fail=1; fi
}
record() { ( cd "$R" && "$C" vcs record --all --no-gate --time "$1" --intent-prompt "$2" --output json > /dev/null ) || { echo "FAIL record"; exit 1; }; }

same "an empty repository is clean"
printf 'one\ntwo\n' > notes.md; : > empty.txt; printf 'x' > one.txt
same "new files are added (no change recorded yet: head is null)"
record 1 first
same "after a record it is clean"
printf 'one\nTWO\n' > notes.md; rm one.txt; printf 'new\n' > fresh.txt
same "modified, deleted and added"
chmod +x empty.txt
same "a mode change"
chmod 645 empty.txt
same "a mode change from the execute bit of others only (0645)"
chmod -x empty.txt; chmod 644 empty.txt; printf 'one\nTWO\n' > notes.md; rm fresh.txt; printf 'x' > one.txt
same "back to the recorded state (a file deleted and made again): clean again"

# names that sort differently as paths than as names: a.b < a/b < a0, a directory and files around it
mkdir -p a sub/deep; printf 1 > a/b; printf 2 > a.b; printf 3 > a0; printf 4 > "a b"; printf 5 > sub/deep/f.txt; printf 6 > 'ünï.txt'; printf 7 > sub/deep.txt; printf 8 > 'a-b'
same "paths that sort differently from names (a.b, a/b, a0, a b, a-b, sub/deep.txt, sub/deep/f.txt)"
record 2 second
same "...recorded, clean"
head -c 100000 /dev/urandom > blob.bin; printf '\0\1\2' > bin.dat
same "binary files and a 100 KB one"
mkdir empty_dir
same "an empty directory is not versioned"

# .canchoignore: every kind of rule, comments, blank lines
printf '# comment\n\n*.log\nbuild/\ndocs/gen\nscratch.txt\n  spaced.txt  \nsub/deep/\n' > .canchoignore
mkdir -p build docs/gen docs/keep sub/deep; printf 1 > x.log; printf 1 > sub/y.log; printf 1 > build/out; printf 1 > docs/gen/g; printf 1 > docs/keep/k; printf 1 > scratch.txt; printf 1 > sub/scratch.txt; printf 1 > spaced.txt; printf 1 > sub/deep/ignored.txt; printf 1 > build.txt
same ".canchoignore: *.log, build/, docs/gen, scratch.txt (anywhere), a trimmed rule, sub/deep/ (and the old sub/deep/f.txt is then deleted)"
mkdir -p .git && printf 1 > .git/config; mkdir -p sub/.git && printf 1 > sub/.git/config
same ".git at the top is skipped, sub/.git is not"
rm -rf .git sub/.git

# a detached HEAD
record 3 third
( cd "$R" && "$C" vcs switch HEAD~1 --detach --output json > /dev/null 2>&1 )
same "a detached HEAD (branch is null)"
( cd "$R" && "$C" vcs switch main --output json > /dev/null 2>&1 )
same "back on the branch"

# the whole tree of this repository's std and docs: about 400 files, clean, then one changed and one added
B="$W/big"; mkdir "$B"; ( cd "$B" && "$C" vcs init --output json > /dev/null ); cp -RL "$D/../../std" "$B/std"; cp -RL "$D/../../docs" "$B/docs"; head -c 70000 /dev/urandom > "$B/blob.bin"
( cd "$B" && "$C" vcs record --all --no-gate --time 9 --intent-prompt big --output json > /dev/null )
R="$B"; same "about 400 files, clean"
printf 'x' >> "$B/std/math.cho"; printf 'new' > "$B/std/zzz.cho"; rm "$B/docs/README.md"
same "about 400 files, one modified, one added, one deleted"

# refusals: the same tag from both (the documents differ: the cancho command prints `refused TAG`)
tag_same() { # name
    want=$(cd "$R" && "$C" vcs status --output json 2> /dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["refused"]["tag"])')
    got=$("$W/ctl" status "$R" 2> /dev/null | sed 's/^refused //')
    if [ -n "$want" ] && [ "$want" = "$got" ]; then echo "ok   $1 ($want)"; else echo "FAIL $1 (rust [$want], cancho [$got])"; fail=1; fi
}
F="$W/fail"; mkdir "$F"; ( cd "$F" && "$C" vcs init --output json > /dev/null && head -c 17000000 /dev/zero > huge.bin ); R="$F"
tag_same "a file over 16 MiB is file-too-large"
rm -f "$F/huge.bin"; printf 1 > "$F/$(printf 'a\nb')"
tag_same "a name with a newline is path-unsupported"

# the difference that is said: a symbolic link (no readlink builtin, #685)
L="$W/link"; mkdir "$L"; ( cd "$L" && "$C" vcs init --output json > /dev/null && printf a > f && ln -s f lnk )
if [ "$("$W/ctl" status "$L" 2>&1)" = "refused symlink" ]; then echo "ok   a symbolic link is refused as symlink (the Rust command makes a tree of it: #685)"; else echo "FAIL symlink"; fail=1; fi

[ $fail -eq 0 ] && echo "all checks passed" || echo "FAILED"
exit $fail
