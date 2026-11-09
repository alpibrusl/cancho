# docs.sh -- the script of tests/commands/script_tests.cho for `conformance/docs.rs` (deleted): the documentation is checked -- links resolve, the README's commands exist, `AGENTS.md` compiles, the compiler carries
# the guidelines, and `docs/refusals-by-example.md` is what the compiler says. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# Every check prints `ok NAME` when it holds and a line with `FAILS` when it does not, so the transcript is the same on every machine.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
root=$(pwd)
work=$2

# docs/first-page.md section 5: every local documentation link resolves. Fenced blocks and inline code are stripped first, because `alloc_slice[a](5, 0)` is `](` followed by a path that is not one.
echo "== every documentation link resolves"
checked=0
broken=0
for file in README.md AGENTS.md docs/*.md; do
    awk -v file="$file" '
        /^[ \t]*```/ { fenced = !fenced; next }
        fenced { next }
        {
            line = $0
            gsub(/`[^`]*`/, "", line)
            while ((at = index(line, "](")) > 0) {
                line = substr(line, at + 2)
                close_at = index(line, ")")
                if (close_at == 0) break
                target = substr(line, 1, close_at - 1)
                line = substr(line, close_at + 1)
                if (target ~ /^http/ || target ~ /^mailto/ || target ~ /^#/) continue
                path = target
                sub(/#.*/, "", path)
                if (path == "" || path ~ /[ \t]/) continue
                print file "\t" target "\t" path
            }
        }' "$file"
done > "$work/links.txt"
while IFS="$(printf '\t')" read -r file target path; do
    checked=$((checked + 1))
    if [ ! -e "$(dirname "$file")/$path" ]; then
        broken=$((broken + 1))
        echo "FAILS broken link: $file -> $target"
    fi
done < "$work/links.txt"
[ $checked -gt 100 ] && echo "ok more than a hundred links were checked" || echo "FAILS only $checked links found; the scan is broken, not the links"
[ $broken = 0 ] && echo "ok no broken link"

# docs/first-page.md section 5: the commands the first page shows still exist, and the other way round.
echo "== the README commands still work"
"$cancho" --help > "$work/help.txt"
awk '/^### The compiler.s whole surface/ { on = 1; next } on && /^###/ { exit } on { sub(/^[ \t]+/, ""); if ($0 ~ /^cancho /) { split($0, w, " "); if (w[2] !~ /^-/) print w[2] } }' README.md | sort -u > "$work/shown.txt"
[ "$(wc -l < "$work/shown.txt")" -ge 6 ] && echo "ok the surface block lists the subcommands" || echo "FAILS the surface block lists too few: $(cat "$work/shown.txt")"
while read -r command; do
    grep -q "cancho $command" "$work/help.txt" || echo "FAILS the README shows 'cancho $command' and --help does not"
done < "$work/shown.txt"
sed -n '/^usage:/,/^options:/p' "$work/help.txt" | sed 's/^[ \t]*//' | awk '/^cancho / { if ($2 !~ /^-/) print $2 }' | sort -u > "$work/usage.txt"
while read -r command; do
    grep -qx "$command" "$work/shown.txt" || echo "FAILS 'cancho $command' exists and the README's surface block does not show it"
done < "$work/usage.txt"
echo "ok the README and --help agree"

# AGENTS.md's checked code blocks are fixtures: every block marked `cancho` must compile, and every block marked `cancho-refused` must be refused with the rule it names.
echo "== the agent guidelines compile as written"
mkdir "$work/blocks"
awk -v dir="$work/blocks" '
    /^```cancho-refused$/ { kind = "refused"; n++; file = dir "/block" n "." kind ".cho"; inblock = 1; next }
    /^```cancho$/ { kind = "accepted"; n++; file = dir "/block" n "." kind ".cho"; inblock = 1; next }
    /^```/ { inblock = 0; next }
    inblock { print > file }
' AGENTS.md
accepted=0
refused=0
for block in "$work"/blocks/*.accepted.cho; do
    [ -f "$block" ] || continue
    accepted=$((accepted + 1))
    if ! "$cancho" check "$block" --std --output json > "$work/check.json" 2>&1; then
        # A GAP is a block the Rust checker accepted and the one in cancho cannot judge yet ("has no code for what is here").
        if grep -q "cannot judge the program" "$work/check.json"; then echo "GAP: $(basename "$block" .accepted.cho) cannot be judged by the compiler in cancho"; else echo "FAILS an AGENTS.md block is shown as valid cancho and is not: $(basename "$block")"; fi
    fi
done
for block in "$work"/blocks/*.refused.cho; do
    [ -f "$block" ] || continue
    refused=$((refused + 1))
    declared=$(sed -n 's/^[ \t]*\/\/~ RULE \(.*\)$/\1/p' "$block" | head -1)
    "$cancho" check "$block" --std --output json > "$work/check.json" 2>&1
    reported=$(sed -n 's/^[ \t]*"rule": "\([^"]*\)".*/\1/p' "$work/check.json" | head -1)
    if [ -z "$declared" ]; then echo "FAILS $(basename "$block") is shown as refused and names no rule"; elif [ "$reported" != "$declared" ]; then echo "FAILS $(basename "$block") names '$declared' and the compiler says '$reported'"; fi
done
echo "checked blocks: $((accepted + refused)) ($accepted accepted, $refused refused)"
[ $((accepted + refused)) -ge 5 ] && echo "ok at least five checked blocks"

# The compiler carries the guidelines, so a reader with the binary needs no checkout.
echo "== agent-guidelines prints the file"
"$cancho" agent-guidelines > "$work/guidelines.txt"
cmp -s "$work/guidelines.txt" AGENTS.md && echo "ok the embedded guidelines are the file" || echo "FAILS the embedded guidelines have drifted from the file"

# docs/refusals-by-example.md is what the compiler says (#619): each refused block is the fixture it names, the compiler refuses that fixture once with the rule in the chapter's heading, the next `text` block is
# what `check --output json` prints, and every corrected block passes `check`.
echo "== refusals by example is what the compiler says"
mkdir "$work/chapters"
awk -v dir="$work/chapters" '
    /^### / { chapter = substr($0, 5) }
    /^```/ {
        if (inblock) { inblock = 0; next }
        inblock = 1; n++
        info = substr($0, 4)
        printf "%s", info > (dir "/" n ".info"); close(dir "/" n ".info")
        printf "%s", chapter > (dir "/" n ".chapter"); close(dir "/" n ".chapter")
        printf "" > (dir "/" n ".body")
        next
    }
    inblock { print >> (dir "/" n ".body") }
    END { print n > (dir "/count") }
' docs/refusals-by-example.md
total=$(cat "$work/chapters/count")
refused=0
fixed=0
said=0
unjson() { sed -e 's/\\"/"/g' -e 's/\\\\/\\/g'; }
n=1
while [ $n -le "$total" ]; do
    info=$(cat "$work/chapters/$n.info")
    chapter=$(cat "$work/chapters/$n.chapter")
    tag=$(printf '%s' "$chapter" | awk -F'`' '{ print $2 }')
    case $info in
        "cancho refused "*)
            refused=$((refused + 1))
            path=${info#cancho refused }
            # The fixture less its `//~` header lines and the blank line after them.
            awk 'started || !(/^\/\/~/ || /^$/) { started = 1; print }' "$path" > "$work/program.cho"
            cmp -s "$work/chapters/$n.body" "$work/program.cho" || echo "FAILS chapter '$tag' shows a program that is not $path"
            grep -qx "//~ RULE $tag" "$path" || echo "FAILS $path does not expect the rule '$tag' that its chapter is headed with"
            "$cancho" check --output json "$path" > "$work/check.json" 2> /dev/null
            code=$?
            [ $code = 1 ] || echo "FAILS $path should be refused (exit $code)"
            [ "$(grep -c '"rule":' "$work/check.json")" = 1 ] || echo "FAILS $path should be refused once, for the rule of its chapter"
            rule=$(sed -n 's/^[ \t]*"rule": "\([^"]*\)".*/\1/p' "$work/check.json" | head -1)
            [ "$rule" = "$tag" ] || echo "FAILS $path is refused with another rule: $rule"
            message=$(sed -n 's/^[ \t]*"message": "\(.*\)",$/\1/p' "$work/check.json" | head -1 | unjson)
            explanation=$(sed -n 's/^[ \t]*"explanation": "\(.*\)",$/\1/p' "$work/check.json" | head -1 | unjson)
            # The `message:` line is the Rust checker's own sentence ("expected `int`, found `bool`"); the compiler in cancho says the sentence of the rule, so only `rule:` and `explanation:` are compared.
            printf 'rule: %s\nexplanation: %s\n' "$rule" "$explanation" > "$work/want.txt"
            # The block that follows says what the compiler printed.
            m=$((n + 1))
            while [ $m -le "$total" ] && [ "$(cat "$work/chapters/$m.info")" != text ]; do m=$((m + 1)); done
            if [ $m -gt "$total" ]; then echo "FAILS no 'text' block after the refused program of '$tag'"; elif grep -v '^message: ' "$work/chapters/$m.body" | cmp -s - "$work/want.txt"; then said=$((said + 1)); else echo "FAILS chapter '$tag' quotes what the compiler no longer says"; fi
            ;;
        "cancho fixed")
            fixed=$((fixed + 1))
            cp "$work/chapters/$n.body" "$work/fixed-$fixed.cho"
            "$cancho" check "$work/fixed-$fixed.cho" > /dev/null 2>&1 || echo "FAILS the corrected program of chapter '$tag' is refused"
            ;;
    esac
    n=$((n + 1))
done
echo "chapters: $refused refused, $fixed fixed, $said said"
[ $refused -ge 13 ] && echo "ok at least thirteen rules are walked through"
[ "$refused" = "$fixed" ] && [ "$refused" = "$said" ] && echo "ok every chapter has a refusal, what is said, and a correction"
exit 0
