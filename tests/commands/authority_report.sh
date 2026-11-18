# authority_report.sh CANCHO ROOT FORM TARGET FILES... -- one report of authority_reports.sh: `CANCHO authority [--output json] [--target T] FILES... [--std]`, run in ROOT, and the line
# `FORM TARGET RESULT FILES...` where RESULT is `CRC-SIZE` of the report (what `cksum` says of it) or `refused` if the command failed. FORM is `prose` or `json`; TARGET is `-` for the host.
# The compiler written in Rust counted what compile-time evaluation folded and the compiler in cancho does not (`folded_operators` and `folded_calls` of the data form, the lines of
# "evaluated at compile time" and what is indented under them in the prose), so those are taken out of both before the sum. `--std` is given when a file imports `std`.
# Not a test of its own: authority_reports.sh runs it four at a time, and with the Rust `cancho` as CANCHO it is the recording of tests/vectors/authority_*.txt.
cancho=$1
root=$2
form=$3
target=$4
shift 4
cd "$root" || exit 1
set -- "$@"
files="$*"
std=
if grep -qs '^import std' "$@"; then std=--std; fi
fmt=
[ "$form" = json ] && fmt="--output json"
tgt=
[ "$target" = - ] || tgt="--target $target"
# shellcheck disable=SC2086
if "$cancho" authority $fmt $tgt "$@" $std > "${TMPDIR:-/tmp}/authority-report.$$" 2> /dev/null; then
    sum=$(awk -v json="$([ "$form" = json ] && echo 1 || echo 0)" '
        json == 1 { t = $0; sub(/^[ \t]+/, "", t); if (t ~ /^"folded_operators":/ || t ~ /^"folded_calls":/) next }
        json == 0 && /^evaluated at compile time/ { skipping = 1; next }
        json == 0 && skipping && /^    / { next }
        { skipping = 0; print }' "${TMPDIR:-/tmp}/authority-report.$$" | cksum)
    echo "$form $target $(echo $sum | tr " " -) $files"
else
    status=$?
    if [ "$status" = 3 ]; then echo "$form $target status3 $files"; else echo "$form $target refused $files"; fi
fi
rm -f "${TMPDIR:-/tmp}/authority-report.$$"
