#!/bin/sh
# Run the tests of the libraries of `cancho vcs` written in cancho (`docs/vcs-native.md`): each `*_test.cho` is run by `cancho test` with the
# module it tests and the modules that one imports. No Rust is involved but the compiler that is named. (Compression is `std.flate`, tested by
# `tests/lex/flate_test.cho`.)
#
# usage: examples/selfhost/test.sh [CANCHO]        (CANCHO defaults to `cancho`; CANCHO_COMPILER=auto uses the compiler in cancho alone)
set -e
C=${1:-cancho}
D=$(dirname "$0")
status=0
run() {
    name=$1
    shift
    files=""
    for m in "$name" "$@"; do
        files="$files $D/$m.cho"
    done
    # shellcheck disable=SC2086
    "$C" test "$D/${name}_test.cho" $files --std || status=1
}
run sha1 hex blake3
run vcsobj hex blake3
run vcsdiff
run vcschange vcsobj blake3 jsonread jsonwrite lexcore rules tables
exit $status
