#!/bin/sh
# Check the `log` and `show` documents of `vcsrepo.cho` against the Rust `cancho vcs` (`docs/vcs-native.md` section 6, slice b4b): the same repository is read by
# both, and standard output and the exit status of each command must be equal, byte for byte.
#
# usage: examples/selfhost/vcslog_check.sh [CANCHO]
set -u
C=${1:-cancho}
case $C in */*) C=$(cd "$(dirname "$C")" && pwd)/$(basename "$C") ;; esac
D=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
"$C" build "$D/vcsstorectl.cho" "$D/vcsworktree.cho" "$D/vcsstore.cho" "$D/vcsrepo.cho" "$D/vcsstatus.cho" "$D/vcsscan.cho" "$D/vcschange.cho" "$D/jsonread.cho" "$D/jsonwrite.cho" "$D/rules.cho" "$D/tables.cho" "$D/vcsobj.cho" "$D/blake3.cho" "$D/filewrite.cho" "$D/lexcore.cho" --std -o "$W/ctl" >/dev/null || { echo "FAIL build"; exit 1; }
export CANCHO_AUTHOR="Ada <ada@example.com>"

# same REPO NAME ARGS...: the Rust command in REPO and the cancho one on REPO/.cancho/vcs give the same bytes and status
same() {
    repo=$1; name=$2; shift 2
    verb=$1; shift
    ( cd "$repo" && "$C" vcs "$verb" "$@" --output json > "$W/rust.out" 2>/dev/null; echo $? > "$W/rust.code" )
    "$W/ctl" "$verb" "$repo/.cancho/vcs" "$@" > "$W/cho.out" 2>/dev/null; echo $? > "$W/cho.code"
    if cmp -s "$W/rust.out" "$W/cho.out" && cmp -s "$W/rust.code" "$W/cho.code"; then echo "ok   $name"; else echo "FAIL $name (rust exit $(cat $W/rust.code), cancho exit $(cat $W/cho.code))"; diff "$W/rust.out" "$W/cho.out" | head -6; fail=1; fi
}
rec() { # repo time prompt [flags]
    ( cd "$1" && shift && t=$1 && p=$2 && shift 2 && "$C" vcs record --all --time "$t" --intent-prompt "$p" "$@" --output json > /dev/null ) || { echo "FAIL record"; exit 1; }
}

# A: declarations, edits, an attestation, a model, a session, an issue, a prompt with escapes, two authors (the gate runs: the Rust command checks the `.cho` file)
A="$W/a"; mkdir "$A"; ( cd "$A" && "$C" vcs init --output json > /dev/null )
printf 'module a;\npub fn twice(n: int) -> [] int { return n + n; }\n' > "$A/a.cho"; printf 'one\ntwo\n' > "$A/notes.md"
rec "$A" 1000 first --intent-model m1 --intent-session s1
printf 'module a;\npub fn twice(n: int) -> [] int { return n * 2; }\npub fn thrice(n: int) -> [] int { return n * 3; }\n' > "$A/a.cho"
rec "$A" 1001 "$(printf 'second: ünï "q" \\ tab\tx\nline two')" --intent-issue 7
printf 'one\n2\n' > "$A/notes.md"; mkdir -p "$A/sub/deep"; printf 'x' > "$A/sub/deep/f.txt"; printf '#!/bin/sh\n' > "$A/run.sh"; chmod +x "$A/run.sh"
CANCHO_AUTHOR="Zoë <z@x.org>" rec "$A" 1002 "third"
chmod -x "$A/run.sh"; rm "$A/notes.md"; printf 'y' >> "$A/sub/deep/f.txt"
rec "$A" 1003 "fourth: a mode, a deletion, a change" --intent-model ""
for spec in "" HEAD HEAD~1 HEAD~2 HEAD~3 HEAD~4 main "--limit 2" "--limit 0" "--limit 100" "HEAD~1 --limit 1" nonexistent; do
    same "$A" "A: log $spec" log $spec
done
for spec in "" HEAD HEAD~1 HEAD~2 HEAD~3 HEAD~4 main nonexistent; do
    same "$A" "A: show $spec" show $spec
done

# B: a history that forks (a branch and more changes on both)
B="$W/b"; mkdir "$B"; ( cd "$B" && "$C" vcs init --output json > /dev/null )
for i in 1 2 3; do echo "l$i" >> "$B/f.txt"; rec "$B" $((2000 + i)) "main $i"; done
( cd "$B" && "$C" vcs branch create side --output json > /dev/null && "$C" vcs switch side --output json > /dev/null ) || { echo "FAIL switch"; exit 1; }
for i in 4 5; do echo "s$i" >> "$B/f.txt"; rec "$B" $((2000 + i)) "side $i"; done
( cd "$B" && "$C" vcs switch main --output json > /dev/null )
for i in 6 7; do echo "m$i" >> "$B/g.txt"; rec "$B" $((2000 + i)) "main $i"; done
for spec in "" side main "side~1" "HEAD~2" "--limit 3" "side --limit 2"; do same "$B" "B: log $spec" log $spec; done
for spec in side HEAD "side~2" "HEAD~4"; do same "$B" "B: show $spec" show $spec; done

# C: one change with many files (the std and docs of this tree), a large one, an empty one
Cc="$W/c"; mkdir "$Cc"; ( cd "$Cc" && "$C" vcs init --output json > /dev/null ); cp -RL "$D/../../std" "$Cc/std"; cp -RL "$D/../../docs" "$Cc/docs"
head -c 70000 /dev/urandom > "$Cc/blob.bin"; : > "$Cc/empty.txt"
rec "$Cc" 3000 big --no-gate
same "$Cc" "C: show (about 400 files added)" show
same "$Cc" "C: log" log

# an ambiguous prefix, with its candidates (two empty files named like changes)
mkdir -p "$A/.cancho/vcs/objects/change/de"
: > "$A/.cancho/vcs/objects/change/de/adbeef0000000000000000000000000000000000000000000000000000000a"
: > "$A/.cancho/vcs/objects/change/de/adbeef0000000000000000000000000000000000000000000000000000000b"
same "$A" "A: show deadbeef (ambiguous, with candidates)" show deadbeef
same "$A" "A: log deadbeef (ambiguous, with candidates)" log deadbeef
same "$A" "A: show HEAD~99 (back past the first change)" show HEAD~99
same "$A" "A: show HEAD~x (a ~ that is not a count)" show HEAD~x

# E: a history with a merge, imported by the Rust command from a git repository made for the purpose (git is only the fixture's maker here; the
# import and everything compared reads no program). Times 1, 3, 5, 9, 10: the merge has two parents, so the order of the history is decided by (time, id).
if command -v git > /dev/null 2>&1; then
    G="$W/g"; mkdir "$G"; ( cd "$G" && git init -q -b main . 2> /dev/null && git config user.email a@x && git config user.name Ada ) || G=""
fi
if [ -n "${G:-}" ]; then
    gc() { t=$1; shift; ( cd "$G" && GIT_AUTHOR_DATE="2020-01-01T00:00:${t}Z" GIT_COMMITTER_DATE="2020-01-01T00:00:${t}Z" "$@" > /dev/null ) 2> /dev/null; }
    ( cd "$G" && echo 1 > f && git add f ) && gc 01 git commit -q -m c1
    ( cd "$G" && git checkout -q -b side && echo s > s && git add s ) && gc 05 git commit -q -m s1
    ( cd "$G" && git checkout -q main && echo 2 > m && git add m ) && gc 03 git commit -q -m m2
    gc 09 git merge -q --no-ff side -m merge
    ( cd "$G" && echo 3 > z && git add z ) && gc 10 git commit -q -m c4
    M="$W/m"; mkdir "$M"; ( cd "$M" && "$C" vcs init --output json > /dev/null && "$C" vcs import-git "$G" --output json > /dev/null )
    for spec in "" HEAD HEAD~1 HEAD~2 side main "--limit 3"; do same "$M" "E: log $spec" log $spec; done
    for spec in HEAD HEAD~1 HEAD~2 side; do same "$M" "E: show $spec" show $spec; done
else
    echo "skip E: git is not installed (it only makes the fixture)"
fi

# D: an empty repository
E="$W/e"; mkdir "$E"; ( cd "$E" && "$C" vcs init --output json > /dev/null )
same "$E" "D: log of an empty repository" log
same "$E" "D: show of an empty repository" show
same "$E" "D: log HEAD of an empty repository" log HEAD

[ $fail -eq 0 ] && echo "all checks passed" || echo "FAILED"
exit $fail
