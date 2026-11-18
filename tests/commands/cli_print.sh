# cli_print.sh -- the script of tests/commands/script_tests.cho for `the_command_line_in_cancho_prints_and_ingests_like_the_rust_front_end` of `conformance/selfhost/cli.rs` (deleted). $1 is the compiler,
# $2 a scratch directory; run from the root of the repository. The oracles were the printer, the JSON printer and the ingester of the Rust front end; the Rust `cancho print` and `cancho ingest`, which
# are those, were run once on the same programs (tests/commands/cli_lib.sh): tests/vectors/cli/print.txt holds what they did. A refused program is a status 1: the Rust command
# says where as `x.cho:LINE:COLUMN: error: ...` and the command in cancho as `x.cho: ERR RULE FROM TO` (byte offsets); turned into lines they agree in 219 of 233 refusals and in the other 14 name the
# next token's line against the end of the last one's (measured), so the place is not compared (the offsets are held by the parser tests and the reject walk), only that the refusal is made.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1
edge_files

begin the_command_line_in_cancho_prints_and_ingests_like_the_rust_front_end
mkdir -p "$work/p"
# one_run FILE ARGS...: the status, and the checksum of the standard output (a refusal is `1 refused`: the words and the place differ, see above).
one_run() {
    shift
    ( cd "$work/p" && "$cancho" "$@" > o.txt 2> e.txt; _s=$?
      if [ $_s = 0 ]; then echo "$_s $(ck o.txt)"; else echo "$_s refused"; fi )
}
pr_answer() {
    case $2 in
        t) cp "$(real "$1")" "$work/p/x.cho"; one_run "$work/p/x.cho" print x.cho ;;
        j) cp "$(real "$1")" "$work/p/x.cho"; one_run "$work/p/x.cho" print --output json x.cho ;;
        i)
            cp "$(real "$1")" "$work/p/x.cho"
            ( cd "$work/p" && "$cancho" print --output json x.cho > y.json 2> /dev/null )
            ingest_run ;;
        e) cp "$(real "$1")" "$work/p/y.json"; ingest_run ;;
    esac
}
# ingest_run: `ingest y.json`: the status, and the checksum of the text when it is made (the refusal's words differ).
ingest_run() {
    ( cd "$work/p" && "$cancho" ingest y.json > o.txt 2> e.txt; _s=$?
      if [ $_s = 0 ]; then echo "$_s $(ck o.txt)"; else echo "$_s refused"; fi )
}
if recording; then
    {
        { corpus; for f in "$work"/edge/*.cho; do echo "edge/$(basename "$f")"; done; } | while read -r case; do
            f=$(real "$case")
            s=$(ck "$f")
            echo "$case|$s|t"
            echo "$case|$s|j"
            "$cancho" print "$f" > /dev/null 2>&1 && echo "$case|$s|i"
        done
        for f in "$work"/edge/ingest*.json; do echo "edge/$(basename "$f")|$(ck "$f")|e"; done
    } > "$work/cases.txt"
else
    vector_cases print
fi
: > "$work/got.txt"
while IFS='|' read -r case sum kind; do
    echo "$case|$sum|$kind|$(pr_answer "$case" "$kind")" >> "$work/got.txt"
done < "$work/cases.txt"
settle print "$work/got.txt"
floor print "$compared" 2500
if ! recording; then
    refused=$(awk -F'|' '$3 != "e" { split($4, a, " "); if (a[1] == 1) n++ } END { print n + 0 }' "$work/got.txt")
    ingested=$(awk -F'|' '$3 == "i" || $3 == "e" { split($4, a, " "); if (a[1] == 0) n++ } END { print n + 0 }' "$work/got.txt")
    [ "$refused" -ge 20 ] || fail "only $refused programs were refused"
    [ "$ingested" -ge 400 ] || fail "only $ingested ingests succeeded"
fi
end
