#!/usr/bin/env bash
# Package a built compiler as a release tarball (docs/package-system.md §9).
#
#   scripts/package-release.sh <target> [out-dir]
#
# Expects CANCHO_BIN to name the binary, e.g. the one scripts/build-standalone.sh wrote (default `target/release/cancho` of the checkout).
# The release ships it as `cancho-<commit>-<target>` (the `cancho-rust-*` backup asset ended with the Rust crates, docs/removing-rust.md Phase 5).
# Writes, into <out-dir> (default `dist`):
#
#   cancho-<commit>-<target>.tar.gz         bin/cancho, LICENSE, README.md
#   cancho-<commit>-<target>.tar.gz.sha256  one line, `sha256sum` format
#
# The name carries the **full commit**, the same string a project's
# `cancho.toml` pins, so an installer can build the asset's name from the pin.
set -euo pipefail

target=${1:?usage: package-release.sh <target> [out-dir]}
out=${2:-dist}
root=$(cd "$(dirname "$0")/.." && pwd)
bin=${CANCHO_BIN:-$root/target/release/cancho}
[ -x "$bin" ] || { echo "no $bin: run scripts/build-standalone.sh first, or set CANCHO_BIN" >&2; exit 1; }

# The binary says which commit it was built from; trust that, not the checkout.
rev=$("$bin" --version | sed -n 's/.*(rev \([0-9a-f]\{40,64\}\),.*/\1/p')
if [ -z "$rev" ]; then
  echo "the binary does not report a clean commit ($("$bin" --version)); refusing to package it" >&2
  exit 1
fi

name="cancho-$rev-$target"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name/bin" "$out"
cp "$bin" "$stage/$name/bin/cancho"
cp "$root/LICENSE" "$root/README.md" "$stage/$name/"

# Reproducible archive on GNU tar (the Linux release build): sorted names, no
# owners, a fixed mtime. bsdtar has none of those flags and gets a plain archive.
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='@0' \
  -C "$stage" -czf "$out/$name.tar.gz" "$name" 2>/dev/null ||
  tar -C "$stage" -czf "$out/$name.tar.gz" "$name"   # bsdtar (macOS) has no --sort

(cd "$out" && if command -v sha256sum >/dev/null; then sha256sum "$name.tar.gz"; else shasum -a 256 "$name.tar.gz"; fi > "$name.tar.gz.sha256")
echo "$out/$name.tar.gz"
