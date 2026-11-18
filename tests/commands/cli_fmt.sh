# cli_fmt.sh -- the script of tests/commands/script_tests.cho for two tests of `conformance/selfhost/cli.rs` (deleted): `the_command_line_in_cancho_formats_like_cancho_fmt` and
# `the_command_line_in_cancho_walks_directories_like_cancho_fmt`. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# Their oracle was the Rust `cancho fmt`; its answers are recorded in tests/vectors/cli/fmt.txt (see tests/commands/cli_lib.sh), and the walks, which are small, are the transcript itself.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1
edge_files

# --- the_command_line_in_cancho_formats_like_cancho_fmt
begin the_command_line_in_cancho_formats_like_cancho_fmt
mkdir -p "$work/f"
# fmt_run ARGS...: the status and the checksums of what `fmt ARGS x.cho` said and of the file afterwards.
fmt_run() {
    ( cd "$work/f" && "$cancho" fmt "$@" x.cho > o.txt 2> e.txt; s=$?; case $(head -1 e.txt) in "x.cho: not formatted:"*) w=$(ck e.txt) ;; *) if [ $s != 0 ]; then w=refused; else w=$(ck e.txt); fi ;; esac; echo "$s $(ck o.txt) $w $(ck x.cho)" )
}
# fmt_answer CASE KIND: what `fmt` and `fmt --check` do to the program (its text is put back before the second run).
fmt_answer() {
    case $2 in
        o) cp "$(real "$1")" "$work/f/x.cho" ;;
        s*) spoil "${2#s}" "$(real "$1")" "$work/f/x.cho" ;;
    esac
    cp "$work/f/x.cho" "$work/f/orig.cho"
    a=$(fmt_run)
    cp "$work/f/orig.cho" "$work/f/x.cho"
    b=$(fmt_run --check)
    echo "$a|$b"
}
if recording; then
    # The cases: every program that the printer accepts, and two of the seven spoilings of it, chosen by a checksum of its name.
    : > "$work/cases.txt"
    { corpus; for f in "$work"/edge/edge*.cho "$work"/edge/format*.cho; do echo "edge/$(basename "$f")"; done; } | while read -r case; do
        f=$(real "$case")
        "$cancho" print "$f" > /dev/null 2>&1 || continue
        h=$(printf '%s' "$case" | cks)
        echo "$case|$(ck "$f")|o"
        echo "$case|$(ck "$f")|s$((h % 7))"
        echo "$case|$(ck "$f")|s$(((h + 3) % 7))"
    done > "$work/cases.txt"
else
    vector_cases fmt
fi
: > "$work/got.txt"
while IFS='|' read -r case sum kind; do
    echo "$case|$sum|$kind|$(fmt_answer "$case" "$kind")" >> "$work/got.txt"
done < "$work/cases.txt"
settle fmt "$work/got.txt"
floor fmt "$compared" 2500
# The cases are only worth something if they cover files that change, files that do not, and files that are refused.
if ! recording; then
    changed=$(awk -F'|' '{ split($4, a, " "); if (a[1] == 0 && a[4] != "") print }' "$work/got.txt" | wc -l | tr -d ' ')
    refused=$(awk -F'|' '{ split($4, a, " "); if (a[1] == 1) n++ } END { print n + 0 }' "$work/got.txt")
    [ "$refused" -ge 10 ] || fail "only $refused runs were refused by the formatter"
fi
echo "fmt cases: $compared" >&2
end

# --- the_command_line_in_cancho_walks_directories_like_cancho_fmt
begin the_command_line_in_cancho_walks_directories_like_cancho_fmt
loose='fn   main() -> [] int {
return 0;
}
'
mktree() {
    rm -rf "$1"
    mkdir -p "$1"
    ( cd "$1" || exit 1
      mkdir -p A/sub A/.hidden B/x.cho C D
      for f in A/z.cho A/a.cho A/sub/m.cho A/.hidden/h.cho A/.dot.cho B/x.cho/in.cho B/notes.txt C/Q.CHO top.cho A/Zed.cho A/b-1.cho A/b_1.cho; do printf '%s' "$loose" > "$f"; done
      printf 'fn broken(' > A/bad.cho
      printf 'fn f() -> [] int {\n    return 0;\n}\n' > A/ok.cho
      ln -s A link )
}
# walk ARGS...: `cancho fmt ARGS` in a fresh tree: the status, what it printed, and the files under the tree afterwards with their checksums.
walk() {
    mktree "$work/walk"
    ( cd "$work/walk" && "$cancho" fmt "$@" > ../walk-out.txt 2> ../walk-err.txt; echo "fmt $*: exit $?" )
    sed 's/^/  /' "$work/walk-out.txt"
    [ -s "$work/walk-err.txt" ] && echo "  stderr: a refusal"
    ( cd "$work/walk" && find . -type f | LC_ALL=C sort | while read -r f; do echo "  file $f $(ck "$f")"; done )
}
walk --check A B C D top.cho .
walk --check A/ ./B
walk D
walk A B C top.cho link
end
