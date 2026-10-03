#!/bin/sh
# Install a prebuilt lex-sys compiler (docs/package-system.md §9).
#
#   install.sh <commit> [prefix]
#
#   <commit>  the full commit hash, the value of `lex-sys = "..."` in lex-sys.toml
#   prefix    default $HOME/.local; the binary lands in <prefix>/bin/lex-sys
#
# Environment:
#   LEX_SYS_RELEASES  base URL of the release assets
#                     (default https://github.com/alpibrusl/lex-sys/releases/download/<commit>;
#                     a release's tag is the full commit hash)
#
# The tarball is checked against its published sha256 before it is unpacked,
# and the installed binary must then report the commit that was asked for.
# Anything else is refused: a compiler that is not the pinned one is worse than none.
set -eu

rev=${1:?usage: install.sh <commit> [prefix]}
prefix=${2:-$HOME/.local}
case $rev in
  *[!0-9a-f]*|'') echo "not a full commit hash: $rev" >&2; exit 2 ;;
esac
[ "${#rev}" -eq 40 ] || [ "${#rev}" -eq 64 ] || { echo "not a full commit hash: $rev" >&2; exit 2; }

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)  target=linux-x86_64 ;;
  Darwin-arm64)  target=darwin-aarch64 ;;
  *) echo "no prebuilt lex-sys for $(uname -s)-$(uname -m); build from source" >&2; exit 3 ;;
esac

name=lex-sys-$rev-$target
base=${LEX_SYS_RELEASES:-https://github.com/alpibrusl/lex-sys/releases/download/$rev}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fetch() { curl --fail --silent --show-error --location --output "$2" "$1"; }
fetch "$base/$name.tar.gz" "$work/$name.tar.gz"
fetch "$base/$name.tar.gz.sha256" "$work/$name.tar.gz.sha256"

want=$(cut -d' ' -f1 "$work/$name.tar.gz.sha256")
if command -v sha256sum >/dev/null; then got=$(sha256sum "$work/$name.tar.gz" | cut -d' ' -f1)
else got=$(shasum -a 256 "$work/$name.tar.gz" | cut -d' ' -f1); fi
[ "$want" = "$got" ] || { echo "sha256 mismatch for $name.tar.gz (want $want, got $got)" >&2; exit 4; }

tar -C "$work" -xzf "$work/$name.tar.gz"
reported=$("$work/$name/bin/lex-sys" --version | sed -n 's/.*(rev \([0-9a-f]*\),.*/\1/p')
[ "$reported" = "$rev" ] || { echo "the binary reports rev '$reported', not $rev" >&2; exit 4; }

mkdir -p "$prefix/bin"
cp "$work/$name/bin/lex-sys" "$prefix/bin/lex-sys.new"
mv "$prefix/bin/lex-sys.new" "$prefix/bin/lex-sys"
echo "installed lex-sys $rev to $prefix/bin/lex-sys"
command -v clang >/dev/null || echo "note: lex-sys needs clang (and cc) on PATH at build time" >&2
