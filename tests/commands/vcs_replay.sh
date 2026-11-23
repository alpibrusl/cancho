# vcs_replay.sh -- the script of tests/commands/script_tests.cho for `cancho vcs replay` (docs/vcs-cli.md section 12; compiler/vcsreplay.cho): a change applied again to another branch by its
# operations. Every document pinned (ids made `<id>`), and every way it ends reached: `exact`, `moved`, `unchanged`, a declaration put into a file the branch changed too, an anchored edit, and the
# refusals `replay-conflict`, `anchor-not-found`, `anchor-ambiguous`, `replay-unsupported`, `dirty-working-copy`, `ref-moved`, `bad-revision`, `bad-argument`, `gate-failed`.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh

AUTHOR='Ada <ada@example.com>'
rv() {
    case $1 in
        record | revert) "$cancho" vcs "$@" --author "$AUTHOR" --output json > "$work/o.json" 2> "$work/e.txt" ;;
        *) "$cancho" vcs "$@" --output json > "$work/o.json" 2> "$work/e.txt" ;;
    esac
    code=$?
}
jf() { sed -n "s/^ *\"$1\": \"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\}\$/\1/p" "$work/o.json" | head -1; }
shown() { sed -e 's/[0-9a-f]\{64\}/<id>/g' -e 's/^/| /' "$work/o.json"; }
said() {
    [ "$code" = 1 ] || fail "$2: exit $code, wanted 1: $(head -c 200 "$work/o.json")"
    [ "$(jf tag)" = "$1" ] || fail "$2: the tag is '$(jf tag)', wanted '$1'"
}
ok() { [ "$code" = 0 ] || fail "$1: exit $code: $(head -c 300 "$work/o.json")"; }
# file BODY_OF_TWICE BODY_OF_THRICE: the `.cho` file of the repository.
file() { printf 'module a;\n\n// Twice a number.\npub fn twice(n: int) -> [] int {\n    return %s;\n}\n\npub fn thrice(n: int) -> [] int {\n    return %s;\n}\n' "$1" "$2" > a.cho; }
notes() { printf 'one\ntwo\nthree\nfour\nfive\n' > notes.md; }
# A repository whose first change ($c0, on `main`) has a.cho and notes.md, and a branch `work` at it, with HEAD on `work`.
repo() {
    rm -rf "$work/$1"; mkdir -p "$work/$1"; cd "$work/$1" || exit 1
    rv init; ok init
    file 'n + n' 'n + n + n'; notes
    rv record --all --time 100 --intent-prompt 'the base'; ok "record base"; c0=$(jf change)
    rv switch work --create; ok "switch --create work"
}
# on BRANCH: HEAD and the working copy at BRANCH.
on() { rv switch "$1"; ok "switch $1"; }
# change PROMPT: everything recorded on the branch HEAD is on; its id in $last.
change() { rv record --all --time "${2:-200}" --intent-prompt "$1" --intent-model m1 --intent-session s1; ok "record '$1'"; last=$(jf change); }

begin a_change_goes_onto_a_branch_that_did_not_touch_its_files
repo one
file 'n * 2' 'n + n + n'; change 'twice by multiplying'; x=$last
on main
printf 'one\ntwo\nthree\nfour\nfive\nsix\n' > notes.md; change 'a sixth note' 300; m1=$last
rv replay "$x" --onto main --time 400; ok replay; shown
new=$(jf change)
same "$(cat .cancho/vcs/refs/heads/main)" "$new" "main is at the new change"
grep -q 'return n \* 2;' a.cho || fail "the working copy of main does not have the change"
grep -q six notes.md || fail "the working copy lost what main had"
rv show "$new"; ok show
same "$(jf origin)" "$x" "the new change names the one it replays"
grep -q '"prompt": "twice by multiplying"' "$work/o.json" || fail "the intent is not the original one"
grep -q '"model": "m1"' "$work/o.json" || fail "the model of the intent is lost"
grep -q "\"$m1\"" "$work/o.json" || fail "the parent is not where main was"
grep -q "$AUTHOR" "$work/o.json" || fail "the author is not the original one"
rv status; ok status
[ "$(grep -c '"path"' "$work/o.json")" = 0 ] || fail "the working copy is not clean after the replay"
rv replay "$x" --onto main --time 401; ok "replay again"; shown
same "$(cat .cancho/vcs/refs/heads/main)" "$new" "a second replay records nothing"
rv verify; ok verify
end

begin a_replay_onto_the_change_s_own_parent_is_exact
repo two
file 'n * 2' 'n + n + n'; change 'twice by multiplying'; x=$last
rv branch create other --from "$c0"; ok "branch create"
rv replay "$x" --onto other --time 400; ok replay; shown
same "$(jf result)" exact "the result"
rv show other; ok "show other"
rv show "$x"; ok "show x"; tx=$(jf tree)
rv show other; same "$(jf tree)" "$tx" "the tree of the new change is the tree of the one replayed"
grep -q 'return n \* 2;' a.cho || fail "the working copy (on work) changed"
end

begin a_declaration_goes_into_a_file_the_branch_changed_too
repo three
file 'n * 2' 'n + n + n'; change 'twice by multiplying'; x=$last
on main
file 'n + n' 'n * 3'; change 'thrice by multiplying' 300
rv replay "$x" --onto main --time 400; ok replay; shown
same "$(jf result)" moved "the result"
grep -q 'return n \* 2;' a.cho || fail "twice is not the change's"
grep -q 'return n \* 3;' a.cho || fail "thrice is not the branch's"
grep -q '// Twice a number.' a.cho || fail "the comment before the declaration is lost"
rv show HEAD; ok show
grep -q '"name": "fn a.twice"' "$work/o.json" || fail "the new change does not record the declaration it changed"
end

begin a_declaration_the_branch_changed_too_is_a_conflict_with_both_bodies
repo four
file 'n * 2' 'n + n + n'; change 'twice by multiplying'; x=$last
on main
file 'n << 1' 'n + n + n'; change 'twice by shifting' 300; m1=$last
rv replay "$x" --onto main --time 400; said replay-conflict "a declaration changed on both sides"; shown
same "$(cat .cancho/vcs/refs/heads/main)" "$m1" "main did not move"
grep -q 'n << 1' a.cho || fail "the working copy changed"
end

begin a_declaration_is_added_and_one_is_removed
repo five
printf 'module a;\n\n// Twice a number.\npub fn twice(n: int) -> [] int {\n    return n + n;\n}\n\npub fn half(n: int) -> [] int {\n    return n / 2;\n}\n' > a.cho
change 'half comes, thrice goes'; x=$last
on main
file 'n * 2' 'n + n + n'; change 'twice by multiplying' 300
rv replay "$x" --onto main --time 400; ok replay; shown
grep -q 'pub fn half' a.cho || fail "the added declaration is not there"
grep -q 'pub fn thrice' a.cho && fail "the removed declaration is still there"
same "$(tail -5 a.cho | head -2 | tr '\n' '|')" "}||" "one empty line before the added declaration"
grep -q 'return n \* 2;' a.cho || fail "what the branch did to twice is lost"
end

begin an_anchored_edit_goes_into_a_text_the_branch_changed_too
repo six
printf 'one\nTWO\nthree\nfour\nfive\n' > notes.md; change 'two, louder'; x=$last
on main
printf 'one\ntwo\nthree\nfour\nFIVE\n' > notes.md; change 'five, louder' 300
rv replay "$x" --onto main --time 400; ok replay; shown
same "$(tr '\n' ' ' < notes.md)" "one TWO three four FIVE " "both edits are in the file"
end

begin an_anchor_that_is_gone_or_twice_there_is_refused
repo seven
printf 'one\nTWO\nthree\nfour\nfive\n' > notes.md; change 'two, louder'; x=$last
on main
printf 'one\nthree\nfour\nfive\n' > notes.md; change 'no two' 300
rv replay "$x" --onto main --time 400; said anchor-not-found "the line the edit replaces is gone"; shown
printf 'one\ntwo\nthree\nfour\nfive\none\ntwo\nthree\n' > notes.md; change 'two, twice' 301
rv replay "$x" --onto main --time 400; said anchor-ambiguous "the line the edit replaces is there twice"; shown
end

begin what_replay_refuses
repo eight
file 'n * 2' 'n + n + n'; change 'twice by multiplying'; x=$last
rv replay "$x"; said bad-argument "no --onto"
rv replay --onto main; said bad-argument "no change"
rv replay "$x" --onto nosuchbranch; said bad-revision "a branch that is not there"
rv replay nosuchchange --onto main; said bad-revision "a change that is not there"
rv replay "$c0" --onto main; said replay-unsupported "a change with no parent"
rv replay "$x" --onto main --time soon; said bad-argument "a time that is not a number"
rv replay "$x" --onto main --expect "$x" --time 400; said ref-moved "an --expect that main is not at"
same "$(cat .cancho/vcs/refs/heads/main)" "$c0" "main did not move"
on main
echo dirt >> notes.md
rv replay "$x" --onto main --time 400; said dirty-working-copy "a working copy with changes, on the branch replayed onto"; shown
notes
rv replay "$x" --onto main --dry-run --time 400; ok "replay --dry-run"; shown
same "$(cat .cancho/vcs/refs/heads/main)" "$c0" "a dry run moves nothing"
grep -q 'return n + n;' a.cho || fail "a dry run changed the working copy"
end

begin a_replay_that_does_not_check_is_refused_by_the_gate
repo nine
printf 'module a;\n\n// Twice a number.\npub fn twice(n: int) -> [] int {\n    return helper(n);\n}\n\npub fn thrice(n: int) -> [] int {\n    return n + n + n;\n}\n\nfn helper(n: int) -> [] int {\n    return n + n;\n}\n' > a.cho
change 'twice through a helper'; x=$last
printf 'module a;\n\n// Twice a number.\npub fn twice(n: int) -> [] int {\n    return helper(n);\n}\n\npub fn thrice(n: int) -> [] int {\n    return helper(n) + n;\n}\n\nfn helper(n: int) -> [] int {\n    return n + n;\n}\n' > a.cho
change 'thrice through the helper' 201; y=$last
on main
file 'n * 2' 'n + n + n'; change 'twice by multiplying' 300; m1=$last
rv replay "$y" --onto main --time 400; said gate-failed "a change that needs a declaration the branch does not have"; shown
same "$(cat .cancho/vcs/refs/heads/main)" "$m1" "main did not move"
end

begin the_same_replay_is_the_same_bytes
repo ten
file 'n * 2' 'n + n + n'; change 'twice by multiplying'; x=$last
on main
file 'n + n' 'n * 3'; change 'thrice by multiplying' 300
cd "$work" || exit 1; rm -rf ten-copy; cp -R ten ten-copy
cd "$work/ten" || exit 1; rv replay "$x" --onto main --time 400; ok "replay"; first=$(jf change)
cd "$work/ten-copy" || exit 1; rv replay "$x" --onto main --time 400; ok "replay in the copy"
same "$(jf change)" "$first" "the change of the second replay"
cmp -s "$work/ten/a.cho" "$work/ten-copy/a.cho" || fail "the files differ"
end
