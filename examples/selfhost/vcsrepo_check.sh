#!/bin/sh
# Check `vcsrepo.cho` against the Rust `cancho vcs` (`docs/vcs-native.md` section 6, slice b4a): the Rust command records a history with a branch and a tag
# and answers `show REV` for a list of revisions; the cancho code must resolve each to the same change, or refuse with the same tag.
#
# usage: examples/selfhost/vcsrepo_check.sh [CANCHO]
set -u
C=${1:-cancho}
case $C in */*) C=$(cd "$(dirname "$C")" && pwd)/$(basename "$C") ;; esac
D=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
check() { name=$1; shift; if "$@"; then echo "ok   $name"; else echo "FAIL $name"; fail=1; fi; }

"$C" build "$D/vcsstorectl.cho" "$D/vcsstore.cho" "$D/vcsrepo.cho" "$D/vcsstatus.cho" "$D/vcsscan.cho" "$D/vcschange.cho" "$D/jsonread.cho" "$D/jsonwrite.cho" "$D/rules.cho" "$D/tables.cho" "$D/vcsobj.cho" "$D/blake3.cho" "$D/filewrite.cho" "$D/lexcore.cho" --std -o "$W/ctl" >/dev/null || { echo "FAIL build"; exit 1; }
export CANCHO_AUTHOR="Ada <ada@example.com>"
mkdir "$W/r" && cd "$W/r" || exit 1
"$C" vcs init --output json >/dev/null
record() {
    echo "line $1" >> notes.md
    "$C" vcs record --all --no-gate --time $((1000 + $1)) --intent-prompt "change $1" --output json >/dev/null || exit 1
}
for i in 1 2 3; do record $i; done
# a branch and a tag at the third change, then three more on main (V1 has no `tag` command: a tag is a ref file)
"$C" vcs branch create side --output json >/dev/null || exit 1
mkdir -p .cancho/vcs/refs/tags && cp .cancho/vcs/refs/heads/main .cancho/vcs/refs/tags/v1
for i in 4 5 6; do record $i; done
VCS="$W/r/.cancho/vcs"
SIDE=$("$C" vcs show side --output json | sed -n 's/.*"change": "\([0-9a-f]*\)".*/\1/p')
TOP=$("$C" vcs show --output json | sed -n 's/.*"change": "\([0-9a-f]*\)".*/\1/p')
check "the branch and the tag are not at HEAD (so the comparison means something)" sh -c "[ -n \"$SIDE\" ] && [ \"$SIDE\" != \"$TOP\" ] && [ \"\$(cat $W/r/.cancho/vcs/refs/tags/v1)\" = \"$SIDE\" ]"

# Compare: for each revision, the Rust `show` change (or refusal tag) against the cancho `resolve`.
cmp_rev() {
    spec=$1
    want=$("$C" vcs show "$spec" --output json | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("change") or ("refused "+d["refused"]["tag"]))')
    got=$("$W/ctl" resolve "$VCS" "$spec")
    if [ "$want" = "$got" ]; then return 0; fi
    echo "  $spec: rust says [$want], cancho says [$got]" >&2
    return 1
}
HEADID=$("$W/ctl" head "$VCS")
for spec in HEAD @ HEAD~1 HEAD~3 HEAD~5 HEAD~6 HEAD~99 main heads/main main~2 side tags/v1 v1 v1~1 "$HEADID" "$(echo "$HEADID" | cut -c1-8)" "$(echo "$HEADID" | cut -c1-6)" "$(echo "$HEADID" | cut -c1-5)" nonexistent "" "HEAD~" "HEAD~x" "HEAD~-1" ".." "heads/.." ; do
    check "resolve [$spec]" cmp_rev "$spec"
done
# two change names that share a prefix (fabricated: only their names matter to a prefix), so a prefix of them is ambiguous
mkdir -p "$VCS/objects/change/de"
: > "$VCS/objects/change/de/adbeef00000000000000000000000000000000000000000000000000000a"
: > "$VCS/objects/change/de/adbeef00000000000000000000000000000000000000000000000000000b"
check "resolve [deadbeef] (ambiguous)" cmp_rev deadbeef
check "an ambiguous prefix is refused, not guessed" sh -c "[ \"\$($W/ctl resolve $VCS deadbeef)\" = 'refused bad-revision' ]"
check "head is what the Rust show says" sh -c "[ \"\$($W/ctl head $VCS)\" = \"$HEADID\" ] && [ -n \"$HEADID\" ]"

# an empty repository: HEAD has no change yet
mkdir "$W/e" && cd "$W/e" || exit 1
"$C" vcs init --output json >/dev/null
check "head of an empty repository is absent" sh -c "[ \"\$($W/ctl head $W/e/.cancho/vcs)\" = absent ]"
check "HEAD of an empty repository is bad-revision" sh -c "[ \"\$($W/ctl resolve $W/e/.cancho/vcs HEAD)\" = 'refused bad-revision' ]"

[ $fail -eq 0 ] && echo "all checks passed" || echo "FAILED"
exit $fail
