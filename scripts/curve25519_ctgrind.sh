#!/bin/bash
# `docs/x25519.md` §3.1: the ctgrind check of `std.x25519` (and, for comparison, `std.ed25519.sign`).
#
#   scripts/curve25519_ctgrind.sh <cancho binary>
#
# Builds `tests/programs/ed25519_bench.cho` (which uses both functions) as an object, renames its `main`, links
# `tests/ct/ctgrind.c` against it, and runs each case under Valgrind's Memcheck with the secret marked undefined.
# Prints the number of reports for each; exit status 1 if X25519 has any.
#
# Then the control (docs/wide-multiply.md §10.3): the same `std/x25519.cho` with an `if` on the secret bit planted in the
# ladder (around a call too large to become a select: an inversion of `a`, which the ladder overwrites next), built as a local module whose symbol is renamed to the one the harness calls. It must be reported, or the check
# is not seeing anything; exit status 1 if it is not.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
lex=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$lex" build --std "$here/tests/programs/ed25519_bench.cho" --emit obj -o "$work/curve.o"
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
sed -e 's/^module std.x25519;/module x25519;/' \
    -e 's/^            swap = bit;$/            swap = bit;\n            if bit == 1 {\n                field25519_51.invert(a, a);\n            }/' \
    "$here/std/x25519.cho" > "$work/x25519.cho"
grep -q "if bit == 1" "$work/x25519.cho" || { echo "the control's \`if\` was not planted"; exit 1; }
sed -e 's/^import std.x25519;/import x25519;/' "$here/tests/programs/ed25519_bench.cho" > "$work/bench.cho"
"$lex" build --std "$work/bench.cho" "$work/x25519.cho" --emit obj -o "$work/planted.o"
objcopy --redefine-sym main=lexs_bench_main --redefine-sym lexs_x25519.scalarmult=lexs_std.x25519.scalarmult "$work/planted.o"
cc -O1 -g -o "$work/ctgrind-planted" "$here/tests/ct/ctgrind.c" "$work/planted.o"
valgrind --tool=memcheck --error-exitcode=0 --log-file="$work/planted.log" "$work/ctgrind-planted" x25519 > "$work/planted.out"
planted=$(grep -c "depends on uninitialised value\|uninitialised value(s)" "$work/planted.log" || true)
echo "x25519 with an if on the secret bit planted (the control): $planted reports -- $(grep "ERROR SUMMARY" "$work/planted.log" | sed 's/^==[0-9]*== //')"
if [ "$planted" = 0 ]; then
    echo "the control was not reported: the check sees nothing"
    status=1
fi
exit $status
