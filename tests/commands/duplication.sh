# duplication.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/duplication.rs (docs/gate.md G4, wave 4; docs/next-phase.md section 4): no function body is duplicated, verbatim, in a
# different file. Each file is parsed and identified on its own, no `--std` and no import resolution, which is what makes the check correct: docs/canonical-ast.md section 4.2 encodes a call to a name declared in the same
# file as the hash of that declaration and a call to anything else as the literal name, so two files that call the same builtins the same way still collide, and two files whose functions read alike but reach two different
# local declarations hash apart. The identities are those of `compiler/ids.cho` (the identity module of the compiler in cancho; `cancho ids` checks the program first, so it refuses a file that imports `std`), built here
# with the compiler under test. The floor of two statements (docs/next-phase.md section 4) is counted on the canonical form of `cancho print`: a line that ends in `;`, and a line that opens an `if`, `while`, `match`, `borrow`
# or `region`. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

# A `(file, function)` pair known to duplicate another file's, and kept anyway. Every entry must still name a real cross-file match, or the check below fails and says which entry is stale.
cat > "$work/allowed.txt" <<'EOF'
examples/hello.cho write_all
examples/modular/text.cho write_all
examples/modular/text.cho print_nat
std/io.cho print_nat
examples/serve/serve.cho port_of
examples/collect/collect.cho nat_of
examples/agent_supervisor/agent_supervisor.cho nat_of
examples/results_stub/results_stub.cho nat_of
examples/sort/sort.cho read_stdin
examples/sort/sort.cho read_file
examples/seek/seek.cho read_stdin
examples/seek/seek.cho read_file
examples/buffer/buffer.cho release_buffer
examples/buffer/buffer.cho push_byte
std/buffer.cho drop
std/buffer.cho push
examples/lines.cho append
examples/ocpp_ws/ocpp.cho put
packages/net-sockets/sockets.cho put
EOF
sort "$work/allowed.txt" > "$work/allowed.sorted"

# statements FILE: `NAME COUNT` for each function and static of a file in canonical form (see the head of this file).
statements() {
    "$cancho" print "$1" 2> /dev/null | awk '
        /^(pub )?(fn|static) / {
            line = $0; sub(/^pub /, "", line); sub(/^(fn|static) /, "", line)
            n = line; sub(/[\[(:{ ].*$/, "", n); name = n; count = 0; inside = 1; next
        }
        inside && /^}/ { print name, count; inside = 0; next }
        inside {
            if ($0 ~ /;[ ]*$/) count++
            else if ($0 ~ /^[ ]*(if|while|match|borrow|region) /) count++
        }'
}

begin no_function_body_is_duplicated_across_files
fresh dup
# The program that says the identities, built once.
files=$(grep -v '^$' compiler/cancho.files | grep -v '^cli.cho$' | grep -v '^ids.cho$' | sed 's#^#compiler/#')
# shellcheck disable=SC2086
"$cancho" build compiler/ids.cho $files --std -o "$d/ids" > "$d/build.txt" 2>&1 || fail "the identity program did not build: $(head -c 300 "$d/build.txt")"
find examples compiler std packages -name '*.cho' -not -path '*/.cancho-vcs/*' | LC_ALL=C sort > "$d/files.txt"
total=$(wc -l < "$d/files.txt" | tr -d ' ')
[ "$total" -gt 50 ] || fail "the walk should have found the whole suite, found $total"
: > "$d/entries.txt"
parsed=0
while read -r f; do
    size=$(wc -c < "$f" | tr -d ' ')
    { printf 'FILE %s\n' "$size"; cat "$f"; } | "$d/ids" > "$d/ids.out" 2> /dev/null
    # A file that does not parse is skipped (the sources under tests/ that are meant not to parse are not in these three directories, but a few examples are fragments).
    if [ "$(head -c 3 "$d/ids.out")" = ERR ] || [ ! -s "$d/ids.out" ]; then continue; fi
    parsed=$((parsed + 1))
    statements "$f" > "$d/counts.txt"
    # body NAME HASH, with a count of two or more statements: `HASH FILE NAME`.
    awk -v file="$f" 'NR == FNR { count[$1] = $2; next } $1 == "body" && ($2 in count) && count[$2] >= 2 { print $3, file, $2 }' "$d/counts.txt" "$d/ids.out" >> "$d/entries.txt"
done < "$d/files.txt"
# The hashes that more than one file has, and each file and name that has one.
awk '{ files[$1] = files[$1] " " $2; seen[$1, $2] = 1; if (!(($1, $2) in counted)) { counted[$1, $2] = 1; nfiles[$1]++ } entry[NR] = $0 }
     END { for (i = 1; i <= NR; i++) { split(entry[i], p, " "); if (nfiles[p[1]] >= 2) print p[2], p[3] } }' "$d/entries.txt" | LC_ALL=C sort -u > "$d/duplicated.txt"
unexpected=$(LC_ALL=C comm -23 "$d/duplicated.txt" "$work/allowed.sorted")
stale=$(LC_ALL=C comm -13 "$d/duplicated.txt" "$work/allowed.sorted")
if [ -n "$unexpected" ]; then fail "function bodies duplicated across files and not in the allowed list (migrate the copy onto the shared declaration, or add it with a reason): $(printf '%s' "$unexpected" | tr '\n' ';')"; fi
if [ -n "$stale" ]; then fail "these allowed entries no longer match a real cross-file duplicate, remove them: $(printf '%s' "$stale" | tr '\n' ';')"; fi
# The counts are written down: a walk that found fewer files, or fewer that parse, or fewer duplicates, is seen.
echo "files walked: $total, parsed: $parsed, duplicated entries: $(wc -l < "$d/duplicated.txt" | tr -d ' ') (allowed: $(wc -l < "$work/allowed.sorted" | tr -d ' '))"
end
