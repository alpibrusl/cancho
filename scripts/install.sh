#!/bin/sh
# Install a prebuilt cancho compiler (docs/package-system.md §9).
#
#   install.sh [<commit> [prefix]]
#
#   <commit>  the full commit hash, the value of `cancho = "..."` in cancho.toml;
#             left out, the latest release (the tag GitHub redirects `releases/latest` to; a release's tag is its commit)
#   prefix    default $HOME/.local; the binary lands in <prefix>/bin/cancho
#
#   curl -fsSL https://raw.githubusercontent.com/alpibrusl/cancho/main/scripts/install.sh | sh
#
# Environment:
#   CANCHO_REPO_URL  where `releases/latest` is asked (default https://github.com/alpibrusl/cancho); only used with no <commit>
#   CANCHO_RELEASES  base URL of the release assets
#                     (default https://github.com/alpibrusl/cancho/releases/download/<commit>;
#                     a release's tag is the full commit hash)
#
# The tarball is checked against its published sha256 before it is unpacked,
# and the installed binary must then report the commit that was asked for.
# Anything else is refused: a compiler that is not the pinned one is worse than none.
set -eu

if [ "$#" -eq 0 ]; then
  # Latest: the redirect of `releases/latest` ends at `releases/tag/<commit>`. Whatever it names is then held to the same checks as a commit
  # given by hand (a full hash, the published sha256, the commit the binary reports).
  repo=${CANCHO_REPO_URL:-https://github.com/alpibrusl/cancho}
  final=$(curl --fail --silent --show-error --location --head --output /dev/null --write-out '%{url_effective}' "$repo/releases/latest") ||
    { echo "could not find the latest release at $repo/releases/latest" >&2; exit 5; }
  rev=${final##*/}
  case $final in */releases/tag/*) ;; *) echo "no release yet at $repo (it redirected to $final)" >&2; exit 5 ;; esac
else
  rev=$1
fi
[ -n "$rev" ] || { echo "usage: install.sh [<commit> [prefix]]" >&2; exit 2; }
prefix=${2:-$HOME/.local}
case $rev in
  *[!0-9a-f]*|'') echo "not a full commit hash: $rev" >&2; exit 2 ;;
esac
[ "${#rev}" -eq 40 ] || [ "${#rev}" -eq 64 ] || { echo "not a full commit hash: $rev" >&2; exit 2; }

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)  target=linux-x86_64 ;;
  Linux-aarch64|Linux-arm64) target=linux-aarch64 ;;
  Darwin-arm64)  target=darwin-aarch64 ;;
  *) echo "no prebuilt cancho for $(uname -s)-$(uname -m); build from source" >&2; exit 3 ;;
esac

name=cancho-$rev-$target
base=${CANCHO_RELEASES:-https://github.com/alpibrusl/cancho/releases/download/$rev}
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
reported=$("$work/$name/bin/cancho" --version | sed -n 's/.*(rev \([0-9a-f]*\),.*/\1/p')
[ "$reported" = "$rev" ] || { echo "the binary reports rev '$reported', not $rev" >&2; exit 4; }

mkdir -p "$prefix/bin"
cp "$work/$name/bin/cancho" "$prefix/bin/cancho.new"
mv "$prefix/bin/cancho.new" "$prefix/bin/cancho"
echo "installed cancho $rev to $prefix/bin/cancho"
# The LLVM backend emits opaque pointers (`ptr`), which clang 15 introduced; older ones refuse its output.
if command -v clang >/dev/null; then
  major=$(clang --version | sed -n 's/.*clang version \([0-9]*\).*/\1/p' | head -n 1)
  case $major in ''|*[!0-9]*) ;; *) [ "$major" -ge 15 ] || echo "note: clang $major is too old for cancho (it needs 15 or newer; set CLANG to another)" >&2 ;; esac
else
  echo "note: cancho needs clang 15 or newer (and cc) on PATH at build time" >&2
fi
