#!/bin/sh
# Build the `cancho` that is written in cancho and built by cancho (docs/standalone-cancho.md D5).
#
#   scripts/build-standalone.sh --out DIR --rev COMMIT [--host TRIPLE] [--cancho0 PATH]
#
#   --out DIR      where the result goes: DIR/cancho (stage 2) and DIR/stages/ (the three stages, for inspection)
#   --rev COMMIT   the commit the binary reports and a pinned project is compared with (scripts/stamp-version.sh)
#   --host TRIPLE  the host triple `--version` reports (default: from uname)
#   --cancho0      stage 0, the Rust build (default: cancho on PATH)
#
# Stage 0 is the Rust `cancho`. Stage 1 is the command line in cancho (examples/selfhost/cancho.files, stamped) built by stage 0 with the compiler in cancho.
# Stage 2 is the same files built by stage 1, and stage 3 by stage 2, each under the name `cancho` in a directory of its own (the object file's name is in the
# executable). Stage 2 must equal stage 3 byte for byte, or this fails: a compiler that does not reproduce itself is not shipped. Stage 2 is what is
# installed. Stage 1 differs from stage 2 (it was linked by stage 0's driver) and is not compared.
#
# STANDALONE_STAGE3_NAME makes stage 3 be built under another name: it changes one byte, and the comparison must refuse (the gate's own test).
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out=; rev=; host=; cancho0=cancho
while [ $# -gt 0 ]; do
  case $1 in
    --out) out=$2; shift 2 ;;
    --rev) rev=$2; shift 2 ;;
    --host) host=$2; shift 2 ;;
    --cancho0) cancho0=$2; shift 2 ;;
    *) echo "usage: build-standalone.sh --out DIR --rev COMMIT [--host TRIPLE] [--cancho0 PATH]" >&2; exit 2 ;;
  esac
done
[ -n "$out" ] && [ -n "$rev" ] || { echo "usage: build-standalone.sh --out DIR --rev COMMIT [--host TRIPLE] [--cancho0 PATH]" >&2; exit 2; }
mkdir -p "$out/stages/1" "$out/stages/2" "$out/stages/3" "$out/stamped"
out=$(cd "$out" && pwd)

# The files of the command line, with version.cho replaced by the stamped copy.
VERSION_CHO="$out/stamped/version.cho" sh "$here/scripts/stamp-version.sh" "$rev" ${host:+"$host"}
files=
while IFS= read -r name; do
  [ -n "$name" ] || continue
  if [ "$name" = version.cho ]; then files="$files $out/stamped/version.cho"; else files="$files $here/examples/selfhost/$name"; fi
done < "$here/examples/selfhost/cancho.files"

echo "stage 1: built by $cancho0 with the compiler in cancho" >&2
CANCHO_COMPILER=auto "$cancho0" build $files --std -o "$out/stages/1/cancho"
echo "stage 2: built by stage 1" >&2
"$out/stages/1/cancho" build $files --std -o "$out/stages/2/cancho"
echo "stage 3: built by stage 2" >&2
name3=${STANDALONE_STAGE3_NAME:-cancho}
"$out/stages/2/cancho" build $files --std -o "$out/stages/3/$name3"
if ! cmp -s "$out/stages/2/cancho" "$out/stages/3/$name3"; then
  echo "build-standalone: stage 2 and stage 3 differ: the command line in cancho does not reproduce itself" >&2
  exit 1
fi
cp "$out/stages/2/cancho" "$out/cancho"
echo "ok: $(sha256sum "$out/cancho" | cut -d' ' -f1)  $out/cancho" >&2
