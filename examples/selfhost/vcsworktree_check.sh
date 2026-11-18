#!/bin/sh
# Check `vcsworktree.cho` (`materialize` and `remove_path`, `docs/vcs-worktree.md`) against the Rust `cancho vcs restore`, which calls the Rust functions of the
# same names: for each state of a working copy, two copies of it are put back to HEAD's tree by `cancho vcs restore PATH...` and by `vcsstorectl restore`, and the
# two trees must be equal entry by entry (kind, execute bit, link target, content checksum). The `.cancho` directory is left out; a temporary file left behind
# by the cancho writer is in the listing and so a difference.
#
# usage: examples/selfhost/vcsworktree_check.sh [CANCHO]
set -u
C=${1:-cancho}
case $C in */*) C=$(cd "$(dirname "$C")" && pwd)/$(basename "$C") ;; esac
D=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
"$C" build "$D/vcsstorectl.cho" "$D/vcsstore.cho" "$D/vcsrepo.cho" "$D/vcsstatus.cho" "$D/vcsscan.cho" "$D/vcsworktree.cho" "$D/vcschange.cho" "$D/jsonread.cho" "$D/jsonwrite.cho" "$D/rules.cho" "$D/tables.cho" "$D/vcsobj.cho" "$D/blake3.cho" "$D/filewrite.cho" "$D/lexcore.cho" --std -o "$W/ctl" >/dev/null || { echo "FAIL build"; exit 1; }
export CANCHO_AUTHOR="Ada <ada@example.com>"
umask 022

BASE="$W/base"; mkdir "$BASE"; cd "$BASE" || exit 1
"$C" vcs init --output json > /dev/null
printf 'one\ntwo\n' > notes.md; printf 'plain\n' > plain.txt; printf '#!/bin/sh\necho hi\n' > run.sh; chmod +x run.sh
printf 'f1\n' > f1; : > empty.txt; ln -s notes.md link1; ln -s plain.txt link2
mkdir -p d/g deep/a/b/c; printf 'df\n' > d/f; printf 'dh\n' > d/g/h; printf 'deep\n' > deep/a/b/c/file; printf 'sib\n' > deep/a/sibling
"$C" vcs record --all --no-gate --time 1 --intent-prompt base --output json > /dev/null || { echo "FAIL record"; exit 1; }

snap() { # DIR: one line to an entry, the store left out
    ( cd "$1" && find . -path ./.cancho -prune -o -print | LC_ALL=C sort | while IFS= read -r f; do
        if [ -L "$f" ]; then echo "L $f -> $(readlink "$f")"
        elif [ -d "$f" ]; then echo "D $f"
        elif [ -x "$f" ]; then echo "X $f $(cksum < "$f")"
        else echo "F $f $(cksum < "$f")"; fi
    done )
}

# case NAME 'SHELL TO MAKE THE STATE' PATH...   (the paths in the order the Rust command visits them: sorted)
case_() {
    name=$1; make=$2; shift 2
    rm -rf "$W/a" "$W/b"; cp -a "$BASE" "$W/a"; cp -a "$BASE" "$W/b"
    ( cd "$W/a" && eval "$make" ) ; ( cd "$W/b" && eval "$make" )
    ( cd "$W/a" && "$C" vcs restore "$@" --output json > "$W/rust.out" 2> /dev/null ); rcode=$?
    ccode=0
    for p in "$@"; do "$W/ctl" restore "$W/b" "$p" > "$W/cho.out" 2> /dev/null || { ccode=$?; break; }; done
    snap "$W/a" > "$W/a.snap"; snap "$W/b" > "$W/b.snap"
    agree=no
    if [ "$rcode" = 0 ] && [ "$ccode" = 0 ]; then agree=yes; fi
    if [ "$rcode" != 0 ] && [ "$ccode" != 0 ]; then agree=yes; fi
    if cmp -s "$W/a.snap" "$W/b.snap" && [ "$agree" = yes ]; then echo "ok   $name"
    else echo "FAIL $name (rust exit $rcode, cancho exit $ccode)"; diff "$W/a.snap" "$W/b.snap" | head -8; fail=1; fi
}

# refused NAME 'SHELL' PATH: the controller refuses (exit 1) and the working copy is as it was
refused() {
    name=$1; make=$2; p=$3
    rm -rf "$W/b"; cp -a "$BASE" "$W/b"; ( cd "$W/b" && eval "$make" ); snap "$W/b" > "$W/before.snap"
    "$W/ctl" restore "$W/b" "$p" > "$W/cho.out" 2> /dev/null; ccode=$?
    snap "$W/b" > "$W/after.snap"
    if [ "$ccode" = 1 ] && cmp -s "$W/before.snap" "$W/after.snap"; then echo "ok   $name"; else echo "FAIL $name (exit $ccode, expected 1 and nothing touched)"; diff "$W/before.snap" "$W/after.snap" | head -5; fail=1; fi
}

case_ "a changed file is written back"            'printf x > notes.md' notes.md
case_ "a deleted file is made again"              'rm notes.md' notes.md
case_ "an empty file"                             'printf junk > empty.txt' empty.txt
case_ "an executable that lost its bit"           'chmod -x run.sh' run.sh
case_ "a deleted executable is made executable"   'rm run.sh' run.sh
case_ "a file that gained the bit"                'chmod +x plain.txt' plain.txt
case_ "a file where a directory was"              'rm -r d; printf z > d' d d/f d/g/h
case_ "a directory (with contents) where a file was" 'rm f1; mkdir -p f1/deep/er; printf q > f1/inner; printf w > f1/deep/er/x' f1 f1/deep/er/x f1/inner
case_ "a link where a file was"                   'rm plain.txt; ln -s notes.md plain.txt' plain.txt
case_ "a file where a link was"                   'rm link1; printf file > link1' link1
case_ "a link with another target"                'rm link1; ln -s elsewhere link1' link1
case_ "a link to a directory where a file was"    'rm f1; mkdir other; ln -s other f1' f1
case_ "a deleted link is made again"              'rm link2' link2
case_ "a dangling link in the way of a file"      'rm notes.md; ln -s nowhere notes.md' notes.md
case_ "a link as a directory of the path"         'rm -r d; mkdir other; ln -s other d' d/f d/g/h
case_ "every directory of a deep path is made"    'rm -r deep' deep/a/b/c/file deep/a/sibling
case_ "an added file is removed, its empty directories with it" 'mkdir -p newdir/x/y; printf 1 > newdir/x/y/f' newdir/x/y/f
case_ "...but not a directory that holds something else" 'mkdir -p nd2/x; printf 1 > nd2/x/f; printf 2 > nd2/keep' nd2/x/f
case_ "...and not a directory that is not empty"   'printf n > deep/a/b/c/new' deep/a/b/c/new
case_ "an added file is removed"                  'printf n > added.txt' added.txt
case_ "a file with every byte value"              'head -c 70000 /dev/urandom > notes.md' notes.md

# what the Rust command has no case for: a path nothing names, and the paths `check_path` refuses
case_ "a path nothing names is left alone"        'true' nothing.txt
refused "a path with .. is refused"               'true' '../x'
refused "an absolute path is refused"             'true' '/etc/passwd'
refused "a path inside .cancho is refused"        'true' '.cancho/HEAD'
refused "a path with an empty component"          'true' 'a//b'

# a temporary file left by a crash is not a change
mkdir -p "$W/t"; rm -rf "$W/t"; cp -a "$BASE" "$W/t"; : > "$W/t/notes.md.cancho-tmp-0"
if "$W/ctl" status "$W/t" | grep -q 'notes.md.cancho-tmp-0'; then echo "FAIL the scan sees a left-over temporary file"; fail=1; else echo "ok   the scan skips a left-over temporary file"; fi

[ "$fail" = 0 ] && echo "all passed"
exit $fail
