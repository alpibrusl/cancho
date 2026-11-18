# authority_reports.sh -- the script of tests/commands/script_tests.cho for `authority_in_cancho_reports_like_cancho_authority` and `authority_in_cancho_reports_each_family_of_labels` of
# conformance/selfhost/authority.rs (deleted; docs/gate.md, G4 wave 4): `cancho authority` of the compiler in cancho says what the Rust `cancho authority` said. $1 is the compiler, $2 a scratch
# directory; run from the root of the repository.
#
# tests/vectors/authority_programs.txt is a line a report: `FORM TARGET RESULT FILES...`, the programs of the repository (every program of examples/ and of tests/accept/ that has a `main`, and
# the programs of several files of examples/ with the packages they import) in the two forms of the report (prose and data), for the host and for a WebAssembly target of each spelling.
# RESULT is `CRC-SIZE` of the Rust report (what `cksum` says of it, taken after the counts of what compile-time evaluation folded are out: the compiler in cancho does not make them), or `refused`
# where the Rust command refused the program. tests/vectors/authority_probes.txt (`##@@ NAME`, then a program) and authority_probes_reports.txt are the programs of one call each, for the families of
# builtins whose labels carry an argument (paths, a listener and a dial, a program started, the generic functions). The recording is this script's helper with the Rust `cancho`:
#     RECORD=1 sh tests/commands/authority_reports.sh target/release/cancho DIR > /dev/null   (it rewrites the two vectors in place)
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
root=$(pwd)
vec=$root/tests/vectors
helper=$root/tests/commands/authority_report.sh

mkdir -p "$work/probes"
awk -v dir="$work/probes" '/^##@@ / { if (out != "") close(out); out = dir "/" $2 ".cho"; next } { print > out }' "$vec/authority_probes.txt"

# reports ROOT VECTOR OUT: every report the vector lists, made by the compiler under test, four at a time, sorted.
reports() {
    awk '{ printf "%s %s", $1, $2; for (i = 4; i <= NF; i++) printf " %s", $i; print "" }' "$2" | xargs -L 1 -P 4 sh "$helper" "$cancho" "$1" | sort > "$3"
}

if [ -n "${RECORD:-}" ]; then
    reports "$root" "$vec/authority_programs.txt" "$work/programs.new"
    reports "$work" "$vec/authority_probes_reports.txt" "$work/probes.new"
    cp "$work/programs.new" "$vec/authority_programs.txt"
    cp "$work/probes.new" "$vec/authority_probes_reports.txt"
    exit 0
fi

# Programs the compiler in cancho answers status 3 for, and why: it has no code for them, or one of them calls a function through a function value, whose labels it does not follow.
# `camera_hello.cho` and `audio_hello.cho` open a device, which it has no code for (#665).
status3='aliasing_in_a_slice_of_references.cho function_value.cho camera_hello.cho audio_hello.cho'

# compare WANTED GOT: the lines of GOT are the lines of WANTED (a program the Rust command refused is left out), but for the programs of $status3, which may be status 3. Prints
# `compared refused status3 differing` and the differing lines after them.
compare() {
    awk -v ok="$status3" '
        BEGIN { n = split(ok, allowed, " ") }
        function key(   k, i) { k = $1 " " $2; for (i = 4; i <= NF; i++) k = k " " $i; return k }
        NR == FNR { want[key()] = $3; next }
        {
            w = want[key()]
            if (w == "refused") { refused++; next }
            compared++
            if ($3 == "status3") {
                status3++
                base = $4; sub(/.*\//, "", base)
                good = 0
                for (i = 1; i <= n; i++) if (allowed[i] == base) good = 1
                if (!good) { bad++; lines = lines "\n" $4 " (" $1 " " $2 "): status 3, which is not expected of it" }
                next
            }
            if ($3 != w) { bad++; lines = lines "\n" $4 " (" $1 " " $2 "): the Rust report sums " w ", this one " $3 }
        }
        END { printf "%d %d %d %d%s\n", compared, refused, status3, bad, lines }' "$1" "$2"
}

begin authority_in_cancho_reports_like_cancho_authority
reports "$root" "$vec/authority_programs.txt" "$work/programs.got"
sort "$vec/authority_programs.txt" > "$work/programs.want"
result=$(compare "$work/programs.want" "$work/programs.got")
set -- $(printf '%s\n' "$result" | head -1)
compared=$1; refused=$2; three=$3; bad=$4
[ "$bad" = 0 ] || fail "$bad of $compared reports differ ($three answered status 3, $refused were refused by the Rust command):$(printf '%s\n' "$result" | sed 1d | head -6 | tr '\n' ' ')"
[ "$compared" -ge 300 ] || fail "only $compared reports were compared"
same "$(wc -l < "$work/programs.got" | tr -d ' ')" "$(wc -l < "$vec/authority_programs.txt" | tr -d ' ')" "the number of reports made"
end
echo "reports compared: $compared, answered status 3: $three, refused by the Rust command: $refused"

begin authority_in_cancho_reports_each_family_of_labels
reports "$work" "$vec/authority_probes_reports.txt" "$work/probes.got"
sort "$vec/authority_probes_reports.txt" > "$work/probes.want"
# Every probe is accepted by both, and the same report: no status 3 and no refusal here.
same "$(awk '$3 == "refused" || $3 == "status3"' "$work/probes.want" "$work/probes.got" | wc -l | tr -d ' ')" 0 "probes that were refused"
if ! cmp -s "$work/probes.want" "$work/probes.got"; then
    fail "the reports of the probes differ: $(diff "$work/probes.want" "$work/probes.got" | grep '^[<>]' | head -4 | tr '\n' '|')"
fi
same "$(wc -l < "$work/probes.got" | tr -d ' ')" "$(($(grep -c '^##@@ ' "$vec/authority_probes.txt") * 6))" "the number of probe reports"
end

begin a_target_that_is_not_wasi_is_a_usage_error
# A target that is not WASI is a usage error of the command line in cancho (the Rust command reports the host's report unchanged), and so is a triple that is none.
for triple in x86_64-unknown-linux-gnu "no such triple"; do
    "$cancho" authority --target "$triple" "$work/probes/generic_tuple.cho" > /dev/null 2> /dev/null
    same "$?" 2 "--target $triple: the exit status"
done
end
