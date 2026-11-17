# cli_small.sh -- the script of tests/commands/script_tests.cho for five tests of `conformance/selfhost/cli.rs` (deleted): `the_command_line_in_cancho_writes_the_agent_guidelines`,
# `..._answers_help_and_the_command_tree_like_cancho`, `the_stamp_script_refuses_what_is_not_a_commit`, `..._reads_a_path_with_dot_dot_like_cancho` and `..._words_a_program_with_no_main_like_cancho`.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. The Rust `cancho` was the oracle of four of them; each row of the transcript that was compared with it was
# run with the Rust binary once (2026-11-17) and the rows are the same bytes as the compiler in cancho's (checked: the transcript of the two differs only in the lines marked `cancho only`).
# A row carries the checksums (`cksum`) of what was said, so the help, the command tree and the skill text are pinned: a change to them changes this transcript, and is then renewed by running
# this script with the compiler in cancho and reading the diff.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1

# row NAME STATUS OUT ERR [FILE]: one line of the transcript.
row() { echo "$1: exit $2 out $(ck "$3") err $(ck "$4") file ${5:+$(ck "$5")}"; }

# --- the_command_line_in_cancho_writes_the_agent_guidelines
begin the_command_line_in_cancho_writes_the_agent_guidelines
mkdir -p "$work/g"
( cd "$work/g" && "$cancho" agent-guidelines > o.txt 2> e.txt; echo $? > s.txt )
same "$(cat "$work/g/s.txt")" 0 "agent-guidelines status"
cmp -s "$work/g/o.txt" AGENTS.md || fail "agent-guidelines is not the text of AGENTS.md"
( cd "$work/g" && "$cancho" agent-guidelines x > o2.txt 2> e2.txt; echo $? > s2.txt )
same "$(cat "$work/g/s2.txt")" 2 "agent-guidelines with an argument"
[ ! -s "$work/g/o2.txt" ] || fail "agent-guidelines x wrote to the standard output"
end

# --- the_command_line_in_cancho_answers_help_and_the_command_tree_like_cancho
begin the_command_line_in_cancho_answers_help_and_the_command_tree_like_cancho
rm -rf "$work/tree"
mkdir -p "$work/tree"
tree_rows() {
    # the twenty command lines of the Rust test, each run in an empty directory with no out.md
    for line in '--help' '-h' 'help' 'introspect' 'introspect --output json' 'introspect --output=table' 'introspect --output text' 'introspect extra --output=json' \
        'skill' 'skill --output json' 'skill --output=table' 'skill out.md' 'skill --output json out.md' 'skill --output table out.md' 'skill out.md --output text' \
        'introspect --output' 'introspect --output bogus' 'skill --output=bogus' 'skill --output' 'skill --output json QUOTED'; do
        rm -f "$work/tree/out.md" "$work/tree/a\"b\\c.md"
        case $line in
            *QUOTED) set -- skill --output json 'a"b\c.md' ;;
            *) set -- $line ;;
        esac
        ( cd "$work/tree" && "$cancho" "$@" > o.txt 2> e.txt; s=$?
          f=out.md; [ -f "$f" ] || f='a"b\c.md'; [ -f "$f" ] || f=
          echo "$line: exit $s out $(ck o.txt) err $(ck e.txt) file $([ -n "$f" ] && ck "$f")" )
    done
}
tree_rows > "$work/tree-rows.txt"
cat "$work/tree-rows.txt"
# Two things the Rust command does that this one does not: it makes the directories above the file (the standard library has no `mkdir`), and it says a skill file was written when it could not write
# it. Here a file that cannot be written is a status 3 and nothing on the standard output.
: > "$work/tree/blocker"
( cd "$work/tree" && "$cancho" skill blocker/out.md > o.txt 2> e.txt; echo $? > s.txt )
same "$(cat "$work/tree/s.txt")" 3 "skill into a file that cannot be made (cancho only)"
[ ! -s "$work/tree/o.txt" ] || fail "skill into a file that cannot be made wrote to the standard output"
end

# --- the_stamp_script_refuses_what_is_not_a_commit
begin the_stamp_script_refuses_what_is_not_a_commit
mkdir -p "$work/stamp"
out=$work/stamp/version.cho
good=0123456789abcdef0123456789abcdef01234567
stamp() { VERSION_CHO=$out sh scripts/stamp-version.sh "$@" > /dev/null 2> "$work/stamp/err.txt"; }
for bad in '' abc 0123456789ABCDEF0123456789abcdef01234567 0123456789abcdef0123456789abcdef0123456 0123456789abcdef0123456789abcdef012345678 xyz23456789abcdef0123456789abcdef01234567; do
    rm -f "$out"
    stamp "$bad" x86_64-unknown-linux-gnu
    same "$?" 2 "'$bad' is refused"
    [ ! -e "$out" ] || fail "'$bad' wrote a stamp"
done
stamp "$good" "x86 64"
same "$?" 2 "a host with a space"
stamp "$good-dirty" aarch64-apple-darwin
same "$?" 0 "a dirty commit and a host"
grep -qF "return \"$good-dirty\";" "$out" && grep -qF 'return "aarch64-apple-darwin";' "$out" || fail "the stamp does not hold the commit and the host: $(cat "$out")"
end

# --- the_command_line_in_cancho_reads_a_path_with_dot_dot_like_cancho
begin the_command_line_in_cancho_reads_a_path_with_dot_dot_like_cancho
rm -rf "$work/dd"
mkdir -p "$work/dd/sub/deeper"
d=$work/dd
printf 'edition 7;\n\nfn main(world: World) -> [] int {\n    release(world);\n    return 3;\n}\n' > "$d/p.cho"
for spelled in "$d/sub/../p.cho" "$d/sub/deeper/../../p.cho" "$d/sub/../sub/../p.cho" "$d/sub/deeper/../../sub/../p.cho"; do
    for how in check fmt-check build; do
        case $how in
            check) set -- check "$spelled" ;;
            fmt-check) set -- fmt --check "$spelled" ;;
            build) set -- build "$spelled" -o "$d/out" ;;
        esac
        "$cancho" "$@" > "$d/o.txt" 2> "$d/e.txt"
        s=$?
        [ "$s" = 0 ] && [ ! -s "$d/o.txt" ] && [ ! -s "$d/e.txt" ] || fail "cancho $how ${spelled#"$d"}: exit $s $(head -c 200 "$d/e.txt")"
    done
    "$d/out"
    same "$?" 3 "the program built from ${spelled#"$d"} answers 3"
done
# Nothing in front of the `..` of a path made absolute: read; a bare `../p.cho` has no directory to collapse it against and is refused.
"$cancho" check "$d/../$(basename "$d")/p.cho" > /dev/null 2>&1
same "$?" 0 "a leading .. of an absolute path"
( cd "$d" && "$cancho" check ../p.cho > o2.txt 2> e2.txt; echo $? > s2.txt )
same "$(cat "$d/s2.txt")" 2 "a .. with nothing in front of it"
grep -qF "cannot read the file" "$d/e2.txt" || fail "a .. with nothing in front of it did not say it cannot read the file: $(cat "$d/e2.txt")"
end

# --- the_command_line_in_cancho_words_a_program_with_no_main_like_cancho
begin the_command_line_in_cancho_words_a_program_with_no_main_like_cancho
rm -rf "$work/nm"
mkdir -p "$work/nm"
printf 'edition 5;\nmodule t;\n\npub fn f() -> [] int {\n    return 1;\n}\n' > "$work/nm/t.cho"
( cd "$work/nm" && "$cancho" check t.cho > o.txt 2> e.txt; echo "exit $?"; sed 's/^/  err: /' e.txt; sed 's/^/  out: /' o.txt )
grep -qF 'error: no `main` function' "$work/nm/e.txt" || fail 'check says another thing than "no `main` function"'
( cd "$work/nm" && "$cancho" check t.cho --output json > o.txt 2> e.txt; echo "exit $?"; sed 's/^/  err: /' e.txt; sed 's/^/  out: /' o.txt )
end
