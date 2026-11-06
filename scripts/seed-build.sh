#!/bin/sh
# Build `cancho0`, the seed compiler, from the committed LLVM IR (docs/seed.md §4).
#
#   scripts/seed-build.sh [--out DIR] [--target TRIPLE]
#
# Checks seed/<target>.sha256 against the uncompressed IR, then hands the IR to
# clang. No Rust and no `cancho` is involved. Writes DIR/cancho0 (default
# seed-out) and prints its path on stdout.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out=seed-out
target=
while [ $# -gt 0 ]; do
  case $1 in
    --out) out=$2; shift 2 ;;
    --target) target=$2; shift 2 ;;
    *) echo "usage: seed-build.sh [--out DIR] [--target TRIPLE]" >&2; exit 2 ;;
  esac
done
if [ -z "$target" ]; then
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    Linux-aarch64|Linux-arm64) target=aarch64-unknown-linux-gnu ;;
    *) echo "seed-build: no seed for $(uname -s)-$(uname -m)" >&2; exit 2 ;;
  esac
fi
gz=$here/seed/$target.ll.gz
want=$here/seed/$target.sha256
[ -f "$gz" ] && [ -f "$want" ] || { echo "seed-build: no seed/$target.ll.gz and seed/$target.sha256 (docs/seed.md S1)" >&2; exit 2; }
clang=$(command -v "${CLANG:-clang}") || { echo "seed-build: no ${CLANG:-clang} on PATH" >&2; exit 2; }
sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
mkdir -p "$out"
gunzip -c "$gz" > "$out/cancho0.ll"
got=$(sha256 "$out/cancho0.ll")
expect=$(cut -d' ' -f1 < "$want")
if [ "$got" != "$expect" ]; then
  echo "seed-build: seed/$target.ll.gz does not match seed/$target.sha256" >&2
  echo "seed-build: expected $expect" >&2
  echo "seed-build: got      $got" >&2
  exit 1
fi
"$clang" -O2 -Wno-override-module "$out/cancho0.ll" -o "$out/cancho0"
echo "$out/cancho0"
