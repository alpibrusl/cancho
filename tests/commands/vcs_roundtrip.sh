# vcs_roundtrip.sh -- the script of tests/commands/script_tests.cho that was the git half of crates/cancho/tests/vcs_repo.rs (docs/gate.md G4, wave 4; docs/vcs-git.md): a git history round-trips to the same commit ids. The fixture is
# built with git (a test may; the product may not): loose objects, a merge, an executable, a symlink, a non-UTF-8 message, an annotated tag, and then `git gc` so the history is in a pack with deltas. `cancho vcs import-git`
# reads it, `export-git` writes it again, and git itself says the two histories are the same. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh
AUTHOR='Ada <ada@example.com>'

gt() { _g=$1; shift; GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_AUTHOR_NAME=Gita GIT_AUTHOR_EMAIL=g@example.com GIT_COMMITTER_NAME=Gita GIT_COMMITTER_EMAIL=g@example.com git -C "$_g" "$@"; }
# commit DIR MESSAGE WHEN: add everything and commit at WHEN.
commit() { gt "$1" add -A; GIT_AUTHOR_DATE=$3 GIT_COMMITTER_DATE=$3 gt "$1" commit -q --allow-empty -m "$2"; }
fixture() { # DIR GC
    g=$1
    mkdir -p "$g/src"
    gt "$g" init -q -b main
    : > "$g/big.txt"
    i=0; while [ $i -lt 400 ]; do echo "line $i" >> "$g/big.txt"; i=$((i + 1)); done
    printf 'module a;\npub fn twice(n: int) -> [] int { return n + n; }\n' > "$g/src/a.cho"
    printf '#!/bin/sh\necho hi\n' > "$g/run.sh"; chmod 755 "$g/run.sh"
    ln -s big.txt "$g/link"
    cp "$g/big.txt" "$g/../big.orig"
    commit "$g" one "1700000000 +0100"
    i=0
    while [ $i -lt 6 ]; do
        sed "s/^line 7\$/line seven $i/" "$g/../big.orig" > "$g/big.txt"
        commit "$g" "edit $i" "$((1700000100 + i)) +0000"
        i=$((i + 1))
    done
    gt "$g" checkout -q -b side HEAD~3
    printf 'side\n' > "$g/side.txt"
    commit "$g" "side work" "1700000500 -0500"
    gt "$g" checkout -q main
    printf 'm\n' > "$g/main-only.txt"
    commit "$g" "main work" "1700000600 +0000"
    gt "$g" merge -q --no-ff -m "merge side" side
    gt "$g" tag -a -m release v1
    gt "$g" tag light
    # A message that is not UTF-8 survives byte for byte.
    printf 'caf\303\251\n' > "$g/latin.txt"
    gt "$g" add -A
    printf 'caf\351 message\n' | GIT_AUTHOR_DATE="1700000700 +0000" GIT_COMMITTER_DATE="1700000700 +0000" gt "$g" -c i18n.commitEncoding=latin1 commit -q -F -
    if [ "$2" = gc ]; then gt "$g" gc -q --aggressive; fi
}
ids() { gt "$1" rev-list --all | sort; }
# jf KEY: the value of the first `"KEY": value` of the last document.
jf() { sed -n "s/^ *\"$1\": \"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\}\$/\1/p" "$work/o.json" | head -1; }
rv() { "$cancho" vcs "$@" --output json > "$work/o.json" 2> "$work/e.txt"; code=$?; }

round_trip() { # TAG GC
    if ! command -v git > /dev/null 2>&1; then echo "git is not installed; the round-trip fixture is skipped"; return 0; fi
    fresh "$1"
    fixture "$d/git" "$2"
    mkdir "$d/work" "$d/out"
    # A path through a symbolic link (`/tmp` on macOS) is refused by the command in cancho as not a git repository, and read by the one in Rust: the repository is given by its real path.
    real=$(cd "$d" && pwd -P)
    cd "$d/work" || exit 1
    ln -s "$real" "$d/linked"
    rv import-git "$d/linked/git"
    if [ "$code" != 0 ] && [ "$(jf tag)" = git-unreadable ]; then gap "import-git of a path through a symbolic link" "says git-unreadable, not a git repository" "reads the repository"; fi
    rm -rf "$d/work/.cancho"
    rv import-git "$real/git"
    same "$code" 0 "import-git exits 0"
    same "$(jf ok)" true "import-git is ok"
    same "$(jf self_check_mismatches)" "[]" "the import checked every id it computed against its own"
    same "$(jf commits)" "$(gt "$d/git" rev-list --all --count)" "every commit is imported"
    # The working copy is the checked-out tree.
    same "$(cat src/a.cho)" "$(cat "$d/git/src/a.cho")" "src/a.cho is the checked-out file"
    rv status
    same "$(jf clean)" true "the working copy is clean"
    # Idempotent: a second import adds nothing.
    rv import-git "$real/git"
    same "$(jf commits)" 0 "a second import adds nothing"
    rv export-git "$real/out"
    same "$code" 0 "export-git exits 0"
    same "$(jf ok)" true "export-git is ok"
    same "$(jf mismatches)" "[]" "nothing differs"
    same "$(jf equal)" "$(jf compared_with_import)" "every commit compared is equal"
    # The independent check: git's own view of both histories.
    [ "$(ids "$d/git")" = "$(ids "$d/out")" ] || fail "the two histories do not have the same commits"
    gt "$d/out" fsck --strict > "$d/fsck.txt" 2>&1 || fail "git fsck --strict: $(head -c 300 "$d/fsck.txt")"
    same "$(gt "$d/out" rev-parse main side 'v1^{commit}')" "$(gt "$d/git" rev-parse main side 'v1^{commit}')" "the branches and the tag point at the same commits"
    # log and blame open the imported history.
    rv log
    same "$(grep -c '^      "change": ' "$work/o.json")" 11 "the log has every commit"
    rv blame big.txt
    same "$(grep -c '^      "line": ' "$work/o.json")" 400 "blame has every line"
    rv verify
    same "$(jf sound)" true "the store is sound"
    cd "$work" || exit 1
}

begin import_then_export_reproduces_every_commit_id_from_loose_objects
round_trip rt-loose loose
end

begin import_then_export_reproduces_every_commit_id_from_a_pack
round_trip rt-pack gc
end
