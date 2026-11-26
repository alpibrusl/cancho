# cli_messages.sh -- the script of tests/commands/script_tests.cho for `conformance/selfhost/messages.rs` (deleted): the ratchet on the refusal message (D4 of `docs/diagnostic-messages.md`). The
# `message` of `check --output json` in the command line in cancho is held to the text each fixture of tests/reject/ expects (the first `//~ ERROR` line). $1 is the compiler, $2 a scratch directory;
# run from the root of the repository. No oracle was involved, so nothing is recorded but the list: tests/vectors/cli/message_gaps.txt names the fixtures whose message does not contain that text, each
# of which must still differ, so the list only shrinks: a fixture that matches and is listed fails (take it out), and so does one that differs and is not listed.
# CLI_RECORD=DIR makes this script write the list it measures to DIR/message_gaps.txt (for the first writing, or to check a shrink; read the diff).
# A fixture with a `//~ TARGET` line is the Rust compiler's, and one with no `//~ ERROR` line has nothing to hold the message to.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1

begin the_message_of_a_refusal_holds_the_text_its_fixture_expects
: > "$work/results.txt"
# One awk pass over the reply of `check --output json` for each fixture: the first `"message": "..."` unescaped, and whether it contains the expected text.
n=0
for f in tests/reject/*.cho; do
    grep -q '^//~ TARGET \|^//~ BACKEND ' "$f" && continue
    want=$(sed -n 's|^//~ ERROR ||p' "$f" | head -1 | sed 's/^[ \t]*//; s/[ \t]*$//')
    [ -n "$want" ] || continue
    n=$((n + 1))
    "$cancho" check --output json "$f" > "$work/m.json" 2> /dev/null
    got=$(sed -n 's/^ *"message": "\(.*\)",\{0,1\}$/\1/p' "$work/m.json" | head -1)
    # the JSON string unescaped (a backslash makes the next character itself, `\n` and `\t` a space and a tab)
    got=$(printf '%s' "$got" | awk '{ s = $0; out = ""; i = 1; while (i <= length(s)) { c = substr(s, i, 1); if (c == "\\") { d = substr(s, i + 1, 1); if (d == "n") out = out " "; else if (d == "t") out = out "\t"; else out = out d; i += 2 } else { out = out c; i++ } } printf "%s", out }')
    if [ -n "$got" ] && awk -v g="$got" -v w="$want" 'BEGIN { exit !(index(g, w) > 0) }'; then echo "$(basename "$f")|match" >> "$work/results.txt"; else echo "$(basename "$f")|differ" >> "$work/results.txt"; fi
done
if recording; then
    mkdir -p "$CLI_RECORD"
    { sed -n '/^#/p' "$vec/message_gaps.txt"; sed -n 's/|differ$//p' "$work/results.txt"; } > "$CLI_RECORD/message_gaps.txt"
else
    sed -n '/^#/!{/./p;}' "$vec/message_gaps.txt" | LC_ALL=C sort > "$work/gaps.txt"
    [ "$n" -gt 300 ] || fail "tests/reject/ is read: $n fixtures"
    # a fixture that matches and is listed, or differs and is not
    sed -n 's/|differ$//p' "$work/results.txt" | LC_ALL=C sort > "$work/differ.txt"
    wrong=$(comm -3 "$work/gaps.txt" "$work/differ.txt")
    [ -z "$wrong" ] || fail "$(printf '%s\n' "$wrong" | wc -l | tr -d ' ') fixtures disagree with message_gaps.txt (a name at the left margin is listed and now matches, one indented is not listed and differs): $(printf '%s' "$wrong" | head -4 | tr '\n' ' ')"
    sed 's/|.*//' "$work/results.txt" | LC_ALL=C sort > "$work/names.txt"
    stale=$(comm -23 "$work/gaps.txt" "$work/names.txt")
    [ -z "$stale" ] || fail "message_gaps.txt names no fixture: $stale"
    echo "fixtures held to their message: $n, listed as gaps: $(wc -l < "$work/gaps.txt" | tr -d ' ')"
fi
end
