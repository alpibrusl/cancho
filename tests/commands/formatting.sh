# formatting.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/formatting.rs (docs/gate.md G4, wave 4): `cancho fmt` (docs/formatting.md), the command around the formatter.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh
root=$(pwd)

# `every_tracked_program_formats_and_is_formatted` ran `fmt --check` over ten paths. Stage 1 of scripts/gate.sh (`fmt`) runs the same command over the same ten, so the test is that stage; what is checked here is that the
# stage still names them all, which is the one way the two could part.
begin every_tracked_program_formats_and_is_formatted
corpus="std examples compiler packages tests/accept tests/programs tests/run.cho tests/support tests/commands scripts benches"
n=0
for p in $corpus; do
    n=$((n + 1))
    [ -e "$root/$p" ] || fail "$p is not there"
done
same "$n" 11 "the corpus has eleven paths"
grep -q "fmt --check $corpus" scripts/gate.sh || fail "stage fmt of scripts/gate.sh does not run fmt --check over $corpus"
end

# `CLAUDE.md`: no input may reach a panic. Delete each line of a few real files, one at a time, and format what is left: most of those do not parse, some do and are a different program, and every one must come back with an
# answer, 0 or 1, never a crash.
begin formatting_never_panics_on_a_damaged_program
fresh damaged
total=0
for source in tests/accept/vec_shrink.cho std/option.cho examples/cut/cut.cho; do
    lines=$(wc -l < "$root/$source" | tr -d ' ')
    skip=1
    while [ "$skip" -le "$lines" ]; do
        sed "${skip}d" "$root/$source" > "$d/damaged.cho"
        "$cancho" fmt --check "$d/damaged.cho" > /dev/null 2> "$d/err.txt"
        c=$?
        case $c in 0 | 1) ;; *) fail "$source without line $skip: exit $c: $(head -c 200 "$d/err.txt")" ;; esac
        skip=$((skip + 1))
        total=$((total + 1))
    done
done
# The count is the number of lines of the three files; it is written down so that a file that shrank, or a loop that ran fewer times, is seen.
echo "damaged programs formatted: $total"
end

begin already_formatted_files_are_a_fixed_point_of_the_command
fresh fixed
printf 'fn f() -> [] int {\n    // c\n    return 0; // d\n}\n' > "$d/a.cho"
r fmt --check "$d/a.cho"; succeeds "fmt --check of a formatted file"
same "$out" "" "and says nothing"
end

begin check_reports_and_writes_nothing_and_fmt_rewrites
fresh checkwrite
printf 'fn f() -> [] int {   \n    return 0;// c\n}\n' > "$d/a.cho"
cp "$d/a.cho" "$d/messy.cho"
r fmt --check "$d/a.cho"; exits 1 "a messy file"
has "$out" "would reformat" "--check says so"
cmp -s "$d/a.cho" "$d/messy.cho" || fail "--check wrote"
r fmt "$d/a.cho"; succeeds fmt
same "$(cat "$d/a.cho")" "fn f() -> [] int {
    return 0; // c
}" "fmt rewrote it"
r fmt --check "$d/a.cho"; succeeds "the second check"
end

begin a_directory_is_walked_and_hidden_directories_are_not
fresh walk
mkdir -p "$d/sub" "$d/.cancho-vcs"
printf 'fn f() -> [] int { return 0; }\n' > "$d/sub/deep.cho"
printf 'fn f() -> [] int { return 0; }\n' > "$d/.cancho-vcs/blob.cho"
printf 'not a program' > "$d/notes.txt"
r fmt --check "$d"; exits 1 "a directory with a messy file"
has "$out" deep.cho "the file in a subdirectory is walked"
hasnt "$out" blob.cho "a hidden directory is not"
hasnt "$out" notes.txt "a file that is not a program is not"
end

begin a_file_it_cannot_format_is_left_untouched_and_the_rest_are_still_done
fresh refuse
late='fn f() -> [] int {
    return 0;
}
import std.io;'
printf '%s\n' "$late" > "$d/a_late.cho"
printf 'fn g() -> [] int { return 0; }\n' > "$d/b_fine.cho"
r fmt "$d"; exits 1 "a file it cannot format"
has "$err" "a_late.cho: not formatted" "the refusal names the file"
same "$(cat "$d/a_late.cho")" "$late" "the refused file is untouched"
same "$(cat "$d/b_fine.cho")" "fn g() -> [] int {
    return 0;
}" "the other is still done"
end

begin a_file_that_does_not_parse_is_reported_with_its_position
fresh parse
printf 'fn f( {\n' > "$d/bad.cho"
r fmt "$d/bad.cho"; exits 1 "a file that does not parse"
case $err in
    *"bad.cho:1:"*) ;;
    "ERR "*) gap "fmt reports where a file does not parse" "says \`$err\` (the rule, and two offsets)" "says bad.cho:LINE:COLUMN: error: and the sentence" ;;
    *) fail "the position is reported: '$err'" ;;
esac
end

# `print` is the layout; `fmt` is the layout plus what `print` cannot know. For a file with none of the latter -- no comments, no blank lines, no literal that `print` respells -- the two must be the same text, or one of
# them has a second opinion about layout.
begin printing_a_comment_free_program_and_formatting_it_agree
fresh agrees
printf 'fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else if a == 2 {\n        return 2;\n    } else {\n        return 3;\n    }\n}\n' > "$d/a.cho"
"$cancho" print "$d/a.cho" > "$d/printed.txt" 2> /dev/null
cp "$d/a.cho" "$d/formatted.cho"
r fmt "$d/formatted.cho"; succeeds fmt
cmp -s "$d/printed.txt" "$d/formatted.cho" || fail "print and fmt disagree about layout"
end
