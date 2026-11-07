#!/bin/sh
# Does the compiler in cancho agree with the Rust one about the identities of packages/secp256k1's files (docs/secp256k1.md section 2)?
#
#   scripts/secp256k1_ids_parity.sh RUST_CANCHO SELFHOST_CANCHO
#
# The conformance test `selfhost::cli::the_command_line_in_cancho_writes_the_identities_like_cancho_ids` runs `ids` from both on every file in the
# repository, alone and, for a tenth of them, with the standard library, and takes 28 minutes. This is the same comparison for this package's files, in
# seconds: the same status and the same lines, or it says which differs. The first version of `comb.cho` decoded 64-bit words in its `static`, which the
# Rust evaluator refuses and the compiler in cancho does not, and failed that test in CI.
set -eu
rust=$1; self=$2
here=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
bad=0
for f in packages/secp256k1/comb.cho packages/secp256k1/field.cho packages/secp256k1/scalar.cho packages/secp256k1/secp256k1.cho tests/programs/secp256k1_driver.cho; do
  cp "$here/$f" "$tmp/x.cho"
  for mode in plain std; do
    if [ $mode = std ]; then ra="--std"; sa="--std-dir $here/std"; else ra=""; sa=""; fi
    (cd "$tmp" && "$rust" ids $ra x.cho > r.out 2>/dev/null) && rs=0 || rs=$?
    # shellcheck disable=SC2086
    (cd "$tmp" && "$self" ids --host linux $sa x.cho > s.out 2>/dev/null) && ss=0 || ss=$?
    if [ "$rs" = "$ss" ] && cmp -s "$tmp/r.out" "$tmp/s.out"; then
      echo "same  $mode $f (status $rs, $(wc -l < "$tmp/r.out") lines)"
    else
      echo "DIFF  $mode $f: rust status $rs, self-hosted status $ss"; bad=1
    fi
  done
done
exit $bad
