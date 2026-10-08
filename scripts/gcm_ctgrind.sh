#!/bin/bash
# `docs/gcm-wide.md` §5: the ctgrind check of `std.gcm`'s hardware path.
#
#   scripts/gcm_ctgrind.sh <cancho binary>
#
# Builds `tests/programs/gcm_driver.cho` as an object, renames its `main`, links `tests/ct/gcm_ctgrind.c` against it, and
# runs a seal and an open under Valgrind's Memcheck with the key and the message marked undefined, for AES-128 and
# AES-256 and for messages of 0, 64, 1,000 and 16,384 bytes (the last two end inside a block and inside a group of
# eight). Memcheck reports a conditional jump, or an address, that depends on an undefined value. Prints the number of
# reports for each case: the seal and the preparation must have none; the open has exactly one, the branch on whether
# the tag matched. Exit status 1 otherwise. aarch64 Linux (Valgrind emulates AES and PMULL there).
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
lex=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
"$lex" build --std "$here/tests/programs/gcm_driver.cho" --emit obj -o "$work/gcm.o"
objcopy --redefine-sym main=lexs_driver_main "$work/gcm.o"
cc -O1 -g -o "$work/ctgrind" "$here/tests/ct/gcm_ctgrind.c" "$work/gcm.o"
status=0
for klen in 16 32; do
    for n in 0 64 1000 16384; do
        valgrind --tool=memcheck --error-exitcode=0 --log-file="$work/log" "$work/ctgrind" $klen $n > "$work/out" 2> "$work/err"
        # Reports before "seal done" are the seal's and the preparation's; the rest are the open's.
        total=$(grep -c "depends on uninitialised value" "$work/log" || true)
        in_open=$(grep -B1 -A12 "depends on uninitialised value" "$work/log" | grep -c "open_hardware" || true)
        in_seal=$((total - in_open))
        echo "AES-$((klen * 8)), $n bytes: $(cat "$work/out") -- $total reports, $in_open in open_hardware, $in_seal elsewhere"
        if [ "$in_seal" != 0 ] || [ "$in_open" -gt 1 ] || ! grep -q "roundtrip ok" "$work/out"; then
            grep -A8 "depends on uninitialised value" "$work/log" | head -50
            status=1
        fi
    done
done
exit $status
