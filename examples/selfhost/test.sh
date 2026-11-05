#!/bin/sh
# Run the tests of the libraries of `cancho vcs` written in cancho (`docs/vcs-native.md`): each `*_test.cho` is run by `cancho test` with
# the module it tests. No Rust is involved but the compiler that is named.
#
# usage: examples/selfhost/test.sh [CANCHO]        (CANCHO defaults to `cancho`; CANCHO_COMPILER=auto uses the compiler in cancho alone)
set -e
C=${1:-cancho}
D=$(dirname "$0")
status=0
for t in sha1:sha1 zlib:zlib; do
    name=${t%%:*}
    "$C" test "$D/${name}_test.cho" "$D/${name}.cho" "$D/hex.cho" --std || status=1
done
exit $status
