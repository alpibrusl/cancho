#!/bin/bash
# `docs/x25519.md` §3.1: the ctgrind check of `std.x25519` (and, for comparison, `std.ed25519.sign`).
#
#   scripts/curve25519_ctgrind.sh <lex-sys binary>
#
# Builds `tests/programs/ed25519_bench.ls` (which uses both functions) as an object, renames its `main`, links
# `tests/ct/ctgrind.c` against it, and runs each case under Valgrind's Memcheck with the secret marked undefined.
# Prints the number of reports for each; exit status 1 if X25519 has any.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
lex=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$lex" build --std "$here/tests/programs/ed25519_bench.ls" --emit obj -o "$work/curve.o"
objcopy --redefine-sym main=lexs_bench_main "$work/curve.o"
cc -O1 -g -o "$work/ctgrind" "$here/tests/ct/ctgrind.c" "$work/curve.o"
status=0
for case in x25519 ed25519-sign; do
    valgrind --tool=memcheck --error-exitcode=0 --log-file="$work/$case.log" "$work/ctgrind" "$case" > "$work/$case.out"
    reports=$(grep -c "depends on uninitialised value\|uninitialised value(s)" "$work/$case.log" || true)
    summary=$(grep "ERROR SUMMARY" "$work/$case.log" | sed 's/^==[0-9]*== //')
    echo "$case: $(cat "$work/$case.out") -- $summary"
    if [ "$case" = x25519 ] && [ "$reports" != 0 ]; then
        grep -A6 "uninitialised" "$work/$case.log" | head -40
        status=1
    fi
done
exit $status
