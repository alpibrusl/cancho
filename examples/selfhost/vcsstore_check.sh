#!/bin/sh
# Check `vcsstore.cho` against the Rust `cancho vcs` (`docs/vcs-native.md` section 6, slice b3): the Rust command is the oracle and the writer of the
# store the cancho code has to read; the cancho code writes a store the Rust command has to read. The tests of `cancho test` cannot do this (a test is
# given a heap and an io, not a file system), so it is a script.
#
# usage: examples/selfhost/vcsstore_check.sh [CANCHO]     (CANCHO: the compiler to build with, and the Rust `cancho vcs` that is the oracle)
set -u
C=${1:-cancho}
case $C in */*) C=$(cd "$(dirname "$C")" && pwd)/$(basename "$C") ;; esac
D=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
check() { # name, command...
    name=$1
    shift
    if "$@"; then echo "ok   $name"; else echo "FAIL $name"; fail=1; fi
}

"$C" build "$D/vcsstorectl.cho" "$D/vcsworktree.cho" "$D/vcsstore.cho" "$D/vcsrepo.cho" "$D/vcsstatus.cho" "$D/vcsscan.cho" "$D/vcschange.cho" "$D/jsonread.cho" "$D/jsonwrite.cho" "$D/rules.cho" "$D/tables.cho" "$D/vcsobj.cho" "$D/blake3.cho" "$D/filewrite.cho" "$D/lexcore.cho" --std -o "$W/ctl" >/dev/null || { echo "FAIL build"; exit 1; }
CTL="$W/ctl"
export CANCHO_AUTHOR="Ada <ada@example.com>"

# --- the oracle's store: a text tree with a large file, an empty one and a one-byte one, recorded by the Rust command
mkdir "$W/rust" && cd "$W/rust" || exit 1
cp -RL "$D/../../std" std && cp -RL "$D/../../docs" docs
head -c 70000 /dev/urandom > blob.bin && : > empty.txt && printf a > one.txt
"$C" vcs init --output json > /dev/null
"$C" vcs record --all --no-gate --time 1000 --intent-prompt "first" --output json > "$W/record.json" || exit 1
RV="$W/rust/.cancho/vcs"
n=0
for f in $(cd "$RV/objects" && find . -type f | sed 's|^\./||'); do n=$((n + 1)); done
echo "the Rust store has $n objects"

# 1. every object the Rust command wrote is read back by the cancho code, byte for byte, after being hashed again
readback() {
    bad=0
    for f in $(cd "$RV/objects" && find . -type f | sed 's|^\./||'); do
        kind=${f%%/*}
        rest=${f#*/}
        id=$(echo "$rest" | tr -d /)
        "$CTL" get "$RV" "$kind" "$id" > "$W/got" || bad=$((bad + 1))
        cmp -s "$W/got" "$RV/objects/$f" || bad=$((bad + 1))
    done
    [ $bad -eq 0 ]
}
check "get: every object of the Rust store, byte for byte" readback

# 2. the cancho code writes the same store: the same bytes in the same names with the same mode
mkdir -p "$W/native" && cd "$W/native" || exit 1
check "init makes a store" sh -c "[ \"\$($CTL init $W/native)\" = created ]"
check "init again says it exists" sh -c "[ \"\$($CTL init $W/native)\" = exists ]"
writeback() {
    bad=0
    for f in $(cd "$RV/objects" && find . -type f | sed 's|^\./||'); do
        kind=${f%%/*}
        rest=${f#*/}
        id=$(echo "$rest" | tr -d /)
        got=$("$CTL" put "$W/native/.cancho/vcs" "$kind" < "$RV/objects/$f") || bad=$((bad + 1))
        [ "$got" = "$id" ] || bad=$((bad + 1))
    done
    [ $bad -eq 0 ]
}
check "put: every object of the Rust store is stored under its own name" writeback
check "the two object directories are the same, file for file" diff -r "$RV/objects" "$W/native/.cancho/vcs/objects"
modes() { [ "$(cd "$RV/objects" && find . -type f -perm 644 | wc -l)" = "$(cd "$W/native/.cancho/vcs/objects" && find . -type f -perm 644 | wc -l)" ]; }
check "the same file modes (644)" modes
check "no temporary file is left" sh -c "[ -z \"\$(find $W/native/.cancho/vcs -name '*.tmp-*')\" ]"
check "the layout of init is the Rust one" sh -c "cd $RV && find . -not -path './objects/*' -not -path './refs/heads/*' | sort > $W/l1; cd $W/native/.cancho/vcs && find . -not -path './objects/*' -not -path './refs/heads/*' | sort > $W/l2; diff $W/l1 $W/l2"
check "HEAD and format are the Rust bytes" sh -c "cmp $RV/HEAD $W/native/.cancho/vcs/HEAD && cmp $RV/format $W/native/.cancho/vcs/format"

# 3. a second put of an object that is there is a no-op, and a damaged or missing one is refused
check "put of a stored object is the same id" sh -c "[ \"\$(printf hello | $CTL put $W/native/.cancho/vcs blob)\" = \"\$(printf hello | $CTL put $W/native/.cancho/vcs blob)\" ]"
id=$(printf 'damage me' | "$CTL" put "$W/native/.cancho/vcs" blob)
f="$W/native/.cancho/vcs/objects/blob/$(echo "$id" | cut -c1-2)/$(echo "$id" | cut -c3-)"
printf 'damage you' > "$f"
check "get of a damaged object is object-corrupt" sh -c "[ \"\$($CTL get $W/native/.cancho/vcs blob $id)\" = 'refused object-corrupt' ]"
check "get of an absent object is object-missing" sh -c "[ \"\$($CTL get $W/native/.cancho/vcs blob 0000000000000000000000000000000000000000000000000000000000000000)\" = 'refused object-missing' ]"
check "get of a name that is not an id is bad-id" sh -c "[ \"\$($CTL get $W/native/.cancho/vcs blob xyz)\" = 'refused bad-id' ]"

# 4. refs: the Rust store has main at the change it recorded; compare-and-swap
CH=$(sed -n 's/.*"change": "\([0-9a-f]*\)".*/\1/p' "$W/record.json")
check "ref-read of the Rust ref" sh -c "[ \"\$($CTL ref-read $RV heads/main)\" = $CH ]"
check "ref-read of a ref that is not there is absent" sh -c "[ \"\$($CTL ref-read $RV heads/none)\" = absent ]"
check "a bad ref name is refused" sh -c "[ \"\$($CTL ref-read $RV heads/..)\" = 'refused bad-ref-name' ]"
NEW=$(printf 'a second change' | "$CTL" put "$RV" change)
check "ref-cas with the right expectation moves the ref" sh -c "[ \"\$($CTL ref-cas $RV heads/main $CH $NEW moved 1001)\" = '' ] && [ \"\$($CTL ref-read $RV heads/main)\" = $NEW ]"
check "the log has the move" sh -c "tail -1 $RV/refs/heads/main.log | grep -q \"^$CH $NEW 1001 moved\$\""
check "ref-cas with the old expectation is ref-moved, naming the winner" sh -c "[ \"\$($CTL ref-cas $RV heads/main $CH $CH again 1002)\" = 'refused ref-moved' ]"
check "ref-cas absent on an existing ref is ref-moved" sh -c "[ \"\$($CTL ref-cas $RV heads/main absent $CH x 1)\" = 'refused ref-moved' ]"
check "ref-cas absent makes a new ref (and its directories)" sh -c "[ \"\$($CTL ref-cas $RV heads/new/deep absent $CH made 1003)\" = '' ] && [ \"\$($CTL ref-read $RV heads/new/deep)\" = $CH ]"
check "the log of a new ref starts with a dash" sh -c "[ \"\$(cat $RV/refs/heads/new/deep.log)\" = \"- $CH 1003 made\" ]"
check "ref-cas any moves it anyway" sh -c "[ \"\$($CTL ref-cas $RV heads/main any $CH back 1004)\" = '' ] && [ \"\$($CTL ref-read $RV heads/main)\" = $CH ]"
check "the Rust command still reads the store (log)" sh -c "cd $W/rust && $C vcs log --output json | grep -q $CH"
check "a held lock is lock-held and is not broken" sh -c "echo '4242 1' > $RV/refs/heads/main.lock && [ \"\$($CTL ref-cas $RV heads/main any $NEW x 5)\" = 'refused lock-held' ] && [ -f $RV/refs/heads/main.lock ] && rm $RV/refs/heads/main.lock"

# 5. a race: ten writers move one ref from the same old value; exactly one wins and the others are told so
race() {
    rm -f "$W/r.*"
    i=0
    while [ $i -lt 10 ]; do
        nid=$(printf "racer $i" | "$CTL" put "$RV" change)
        ( "$CTL" ref-cas "$RV" heads/main "$CH" "$nid" "racer $i" 2000 > "$W/r.$i" 2>&1 ) &
        i=$((i + 1))
    done
    wait
    wins=$(cat "$W"/r.* | grep -c '^$\|^[0-9a-f]\{0\}$')
    refused=$(cat "$W"/r.* | grep -c '^refused \(ref-moved\|lock-held\)$')
    echo "  race: $((10 - refused)) won, $refused refused" >&2
    [ "$refused" -eq 9 ]
}
check "ten racing writers: one wins, nine are refused" race

[ $fail -eq 0 ] && echo "all checks passed" || echo "FAILED"
exit $fail
