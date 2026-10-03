#!/usr/bin/env bash
# Package a built compiler as a release tarball (docs/package-system.md §9).
#
#   scripts/package-release.sh <target> [out-dir]
#
# Expects `cargo build --release -p lex-sys` to have been run from a checkout
# (or LEX_SYS_BIN to name the binary).
# Writes, into <out-dir> (default `dist`):
#
#   lex-sys-<commit>-<target>.tar.gz         bin/lex-sys, LICENSE, README.md
#   lex-sys-<commit>-<target>.tar.gz.sha256  one line, `sha256sum` format
#
# The name carries the **full commit**, the same string a project's
# `lex-sys.toml` pins, so an installer can build the asset's name from the pin.
set -euo pipefail

target=${1:?usage: package-release.sh <target> [out-dir]}
out=${2:-dist}
root=$(cd "$(dirname "$0")/.." && pwd)
bin=${LEX_SYS_BIN:-${CARGO_TARGET_DIR:-$root/target}/release/lex-sys}
[ -x "$bin" ] || { echo "no $bin: run cargo build --release -p lex-sys first" >&2; exit 1; }

# The binary says which commit it was built from; trust that, not the checkout.
rev=$("$bin" --version | sed -n 's/.*(rev \([0-9a-f]\{40,64\}\),.*/\1/p')
if [ -z "$rev" ]; then
  echo "the binary does not report a clean commit ($("$bin" --version)); refusing to package it" >&2
  exit 1
fi

name="lex-sys-$rev-$target"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name/bin" "$out"
cp "$bin" "$stage/$name/bin/lex-sys"
cp "$root/LICENSE" "$root/README.md" "$stage/$name/"

# Reproducible archive on GNU tar (the Linux release build): sorted names, no
# owners, a fixed mtime. bsdtar has none of those flags and gets a plain archive.
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='@0' \
  -C "$stage" -czf "$out/$name.tar.gz" "$name" 2>/dev/null ||
  tar -C "$stage" -czf "$out/$name.tar.gz" "$name"   # bsdtar (macOS) has no --sort

(cd "$out" && if command -v sha256sum >/dev/null; then sha256sum "$name.tar.gz"; else shasum -a 256 "$name.tar.gz"; fi > "$name.tar.gz.sha256")
echo "$out/$name.tar.gz"
