# vcs_cycle.sh -- the script of tests/commands/script_tests.cho that was most of crates/cancho/tests/vcs_repo.rs (docs/gate.md G4, wave 4): `cancho vcs` as a repository (docs/vcs-cli.md sections 4, 5, 7 and 9, slice V1): every
# command's JSON pinned in tests/vcs_fixtures/pins/*.json (moved from crates/cancho/tests/vcs_pins/; the pins were made by the Rust command, and the command in cancho prints them byte for byte), every refusal tag V1 has
# reached, a binary file and a link, an anchored edit that applies once, two writers racing on a ref with one winner, and the publish log when no repository is near. What stays in Rust: the kill at every write
# (`CANCHO_VCS_CRASH_AT`), which the command in cancho has no failpoint for (issue 687). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
root=$(pwd)
pins=$root/tests/vcs_fixtures/pins
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

AUTHOR='Ada <ada@example.com>'
GOOD='module a;
pub fn twice(n: int) -> [] int { return n + n; }'
# `rv ARGS...`: `cancho vcs ARGS --output json` in the current directory: the document in $work/o.json, the exit status in $code. A change is written under an author (the Rust command read it from `CANCHO_AUTHOR`; both take `--author`).
rv() {
    case $1 in
        record | revert) "$cancho" vcs "$@" --author "$AUTHOR" --output json > "$work/o.json" 2> "$work/e.txt" ;;
        *) "$cancho" vcs "$@" --output json > "$work/o.json" 2> "$work/e.txt" ;;
    esac
    code=$?
}
# jf KEY [FILE]: the value of the first `"KEY": value` of a document, without quotes.
jf() { sed -n "s/^ *\"$1\": \"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\}\$/\1/p" "${2:-$work/o.json}" | head -1; }
# norm: the root of the sandbox is `<root>` (as written and as the system spells it).
norm() { sed -e 's#"root": "[^"]*"#"root": "<root>"#' -e "s#$canon#<root>#g" -e "s#$sbx#<root>#g"; }
# matches NAME: the document is the pin NAME, byte for byte.
matches() {
    norm < "$work/o.json" > "$work/n.json"
    if ! cmp -s "$work/n.json" "$pins/$1.json"; then fail "the JSON of $1 changed: $(diff "$pins/$1.json" "$work/n.json" | head -6 | tr '\n' '|')"; fi
}
# pin NAME ARGS...: the command succeeds (`ok`) and prints the pin.
pin() {
    _n=$1; shift
    rv "$@"
    [ "$code" = 0 ] || { fail "$_n: $* exited $code: $(head -c 200 "$work/o.json")"; return 1; }
    grep -q '"ok": true' "$work/o.json" || fail "$_n: ok is not true"
    matches "$_n"
}
# quiet ARGS...: the command succeeds, the document stays in $work/o.json.
quiet() { rv "$@"; [ "$code" = 0 ] || fail "$* exited $code: $(head -c 200 "$work/o.json")"; }
# refuse NAME TAG ARGS...: exit 1, `ok` false, and the tag; NAME (not `-`) is a pin.
refuse() {
    _n=$1; _t=$2; shift 2
    rv "$@"
    if [ "$code" != 1 ]; then fail "$* should be refused (exit $code)"; return 1; fi
    grep -q '"ok": false' "$work/o.json" || fail "$*: ok is not false"
    same "$(jf tag)" "$_t" "the tag of $*"
    [ "$_n" = - ] || matches "$_n"
}
# sandbox NAME: a fresh directory, entered; $sbx and $canon say where it is.
sandbox() {
    sbx=$work/$1; rm -rf "$sbx"; mkdir -p "$sbx"; cd "$sbx" || exit 1
    canon=$(pwd -P)
}
put() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" > "$1"; }
# A repository with two recorded changes' worth of history started: `a.cho` and `notes.md`, initialised and recorded once, then both edited.
two_changes() {
    sandbox "$1"
    put a.cho "$GOOD"; put notes.md "# notes
one
two
three"
    pin init init
    pin status_dirty status
    pin record_first record --all --intent-prompt first --intent-model m1 --intent-session s1 --time 1000
    pin status_clean status
    put a.cho 'module a;
pub fn twice(n: int) -> [] int { return n + n + 0; }
pub fn thrice(n: int) -> [] int { return n * 3; }'
    put notes.md "# notes
one
2
three"
}

begin the_whole_cycle_is_pinned
two_changes cycle
pin diff diff
pin record_second record a.cho --intent-prompt second --intent-issue 7 --time 1001
pin status_one_left status
pin record_third record --all --intent-prompt "third: notes" --time 1002
pin log log
pin log_intent log --intent notes
pin log_path log --path notes.md
pin show show HEAD
pin show_patch show HEAD~1 --patch
pin blame blame notes.md
pin blame_decl blame a.cho --decl "fn a.twice"
pin diff_revisions diff HEAD~2 HEAD
# Branches, switching, and the working copy following.
pin branch_create branch create feature --from HEAD~1
pin branch_list branch list
pin switch switch feature
same "$(cat notes.md)" "# notes
one
two
three" "the working copy follows the switch"
pin switch_again switch feature
put extra.txt x
refuse refuse_dirty_switch dirty-working-copy switch main
pin switch_keep switch main --keep
same "$(cat extra.txt)" x "a kept file stays"
rm extra.txt
pin branch_delete branch delete feature --expect HEAD~1
# Undo: revert a change, restore a file, both idempotent.
pin revert revert HEAD --time 1003
same "$(cat notes.md)" "# notes
one
two
three" "the revert undid the change"
pin revert_again revert HEAD~1 --time 1004
put notes.md scribble
pin restore_dry restore notes.md --dry-run
pin restore restore notes.md
same "$(cat notes.md)" "# notes
one
two
three" "restore gave the file back"
pin restore_again restore notes.md
pin verify verify
pin unlock unlock
# Idempotent record: nothing changed, nothing recorded.
pin record_unchanged record --all --time 1005
pin init_again init
end

begin every_refusal_has_its_tag
two_changes refusals
quiet record --all --intent-prompt base --time 1001
# not-a-repository
here=$(pwd); sandbox refusals-empty
refuse refuse_not_a_repository not-a-repository status
cd "$here" || exit 1; sbx=$here; canon=$(pwd -P)
# nothing-selected, bad-ref-name, bad-revision
refuse - nothing-selected record
refuse - bad-ref-name branch create bad..name
refuse - bad-revision show nonesuch
# path-escapes
refuse refuse_path_escapes path-escapes restore ../elsewhere
# gate-failed: the rule tag is the refusal (the sentence of a diagnostic is the compiler's own, so the pin is read for its facts)
put a.cho 'module a;
pub fn twice(n: int) -> [] int { return true; }'
rv record a.cho --intent-prompt bad --time 1002
same "$code" 1 "a type error is refused"
same "$(jf tag)" gate-failed "the tag"; same "$(jf path)" a.cho "the path"; same "$(jf rule)" type-mismatch "the rule"
if [ "$(grep -E '"(command|ok|path|rule|tag)":' "$work/o.json")" != "$(grep -E '"(command|ok|path|rule|tag)":' "$pins/refuse_gate_failed.json")" ]; then fail "the facts of refuse_gate_failed changed"; fi
put a.cho 'module a;
pub fn twice(n: int) -> [] int { return n +; }'
rv record a.cho --time 1002
same "$(jf tag)" gate-failed "a parse error is a gate refusal"; same "$(jf path)" a.cho "and names the path"
first=$(grep -E '"(rule|tag|path)":' "$work/o.json")
rv record a.cho --time 1002
same "$(grep -E '"(rule|tag|path)":' "$work/o.json")" "$first" "the same refusal on the next run"
quiet restore a.cho
# file-too-large
dd if=/dev/zero bs=1048576 count=16 2> /dev/null > big.bin; printf 'x' >> big.bin
refuse - file-too-large record big.bin
rm big.bin
# ref-moved: a stale --expect
put notes.md changed
quiet record --all --intent-prompt n --time 1003
head_id=$(jf change)
put notes.md "changed again"
rv record --all --expect HEAD~1 --time 1004
same "$(jf tag)" ref-moved "a stale --expect"
if [ -z "$(jf current)" ]; then
    gap "ref-moved says where the winner left the ref" "says the tag and a sentence only" "also says the ref, the head it is at and the one expected"
else
    same "$(jf current)" "$head_id" "the loser is told where the winner left the ref"
fi
quiet restore notes.md
# ref-protected
refuse - ref-protected branch delete main --expect HEAD
# lock-held, and unlock clears it
printf '99999 1' > .cancho/vcs/lock
put notes.md "locked out"
rv record --all --time 1005
same "$(jf tag)" lock-held "a held lock"; same "$(jf pid)" 99999 "and whose it is"
same "$code" 1 "is a refusal"
quiet unlock
quiet restore notes.md
# revert-conflict and revert-unsupported
refuse - revert-conflict revert HEAD~1 --time 1006
refuse - revert-unsupported revert HEAD~2 --time 1006
# object-corrupt, found by verify
blobdir=$(ls -d .cancho/vcs/objects/blob/* | head -1)
blob=$(ls "$blobdir"/* | head -1)
printf 'tampered' > "$blob"
refuse - object-corrupt verify
end

begin an_anchored_edit_applies_once_or_not_at_all
sandbox anchored
quiet init
put f.txt "a
b
c
b
d"
quiet record --all --time 1
# Recording a modification derives the edit list.
put f.txt "a
b
c
B
d"
quiet record --all --time 2
same "$(jf edits)" 1 "one edit is derived"
pin show_edits show HEAD
# the edit list as the pin has it, the array of "edits"
sed -n '/^  "edits": \[/,/^  \],\{0,1\}$/p' "$work/o.json" | sed '1s/.*/[/; $s/.*/]/' > "$work/edits.json"
same "$(grep -c '"old"' "$work/edits.json")" 1 "one edit in the list"
# Replaying it onto the older text lands exactly where it was made.
quiet restore f.txt --from HEAD~1
same "$(cat f.txt)" "a
b
c
b
d" "the older text is back"
cp "$work/edits.json" edits.json
pin apply apply edits.json
same "$(cat f.txt)" "a
b
c
B
d" "the edit landed where it was made"
# The second time the old text is gone, and nothing is written.
refuse refuse_anchor_not_found anchor-not-found apply edits.json
# An anchor that occurs twice is ambiguous.
printf 'x\nx\n' > g.txt
printf '[{"path": "g.txt", "old": "x\\n", "new": "y\\n"}]' > amb.json
refuse - anchor-ambiguous apply amb.json
same "$(cat g.txt)" "x
x" "nothing was written"
end

begin a_binary_file_is_a_snapshot_and_a_symlink_is_a_link
sandbox binary
quiet init
printf '\000\237\222\226\000\377' > blob.bin
ln -s blob.bin link
printf '#!/bin/sh\n' > run.sh; chmod 755 run.sh
quiet record --all --time 1
printf '\001\002\003\000' > blob.bin
quiet diff
same "$(grep -c '"binary": true' "$work/o.json")" 1 "the changed file is a binary one"
quiet restore .
same "$(od -An -tu1 blob.bin | tr -s ' ' | sed 's/^ //; s/ $//')" "0 159 146 150 0 255" "the binary file is back, byte for byte"
same "$(readlink link)" blob.bin "the link is a link"
[ -x run.sh ] || fail "the mode of run.sh is gone"
quiet status
same "$(jf clean)" true "the tree is clean"
end

# Two agents move one ref from the same base at the same moment.
begin two_writers_racing_one_ref_have_one_winner
round=0
while [ $round -lt 8 ]; do
    sandbox "race$round"
    quiet init
    put f.txt base
    quiet record --all --time 1
    base=$(jf change)
    put f.txt edit
    i=0
    while [ $i -lt 4 ]; do
        ( "$cancho" vcs record --all --expect "$base" --intent-prompt "writer $i" --time "$((10 + i))" --author "$AUTHOR" --output json > "$work/race$round.w$i.json" 2> /dev/null; echo $? > "$work/race$round.w$i.code" ) &
        i=$((i + 1))
    done
    wait
    winners=0; recorded=0; i=0
    while [ $i -lt 4 ]; do
        c=$(cat "$work/race$round.w$i.code")
        if [ "$c" = 0 ]; then
            winners=$((winners + 1))
            if grep -q '"recorded": true' "$work/race$round.w$i.json"; then recorded=$((recorded + 1)); fi
        else
            # The loser's reply is either the ref check or the lock; never success, never silence.
            t=$(jf tag "$work/race$round.w$i.json")
            case $t in ref-moved | lock-held) ;; *) fail "round $round: writer $i exited $c with tag '$t'" ;; esac
        fi
        i=$((i + 1))
    done
    [ "$winners" -ge 1 ] || fail "round $round: nobody won"
    same "$recorded" 1 "round $round: exactly one writer moves the ref from the base"
    quiet verify
    same "$(jf sound)" true "round $round: the store is sound"
    quiet log
    same "$(grep -c '^      "change": ' "$work/o.json")" 2 "round $round: base and the winner"
    round=$((round + 1))
done
end

begin a_directory_that_is_not_git_is_refused_with_its_tag
sandbox notgit-work
mkdir "$work/notgit-dir"
refuse - git-unreadable import-git "$work/notgit-dir"
end

begin the_old_publish_log_is_still_the_log_when_no_repository_is_near
sandbox oldlog
"$cancho" vcs log > "$work/o.txt" 2> /dev/null
same "$?" 0 "the old log succeeds"
has "$(cat "$work/o.txt")" "nothing published yet" "and says so"
end
