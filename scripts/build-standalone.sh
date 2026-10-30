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
# STANDALONE_STAGE3_NAME makes stage 3 be built under another name, which must change nothing: the module the compiler writes names itself `cancho`, not the file it was
# written to (it did the second, and the executables differed in one byte of their symbol table). STANDALONE_STAGE3_CORRUPT=1 appends a byte to stage 3, and the
# comparison must refuse (the gate's own test).
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
# The command line in cancho has no environment to read (a cancho program has no `getenv`), so `$CLANG` reaches it as `--clang PATH`, as an absolute path
# (it runs `/usr/bin/clang` otherwise). The first release run found this: on ubuntu-22.04 stage 2 used clang 14, which cannot read the IR.
clang=$(command -v "${CLANG:-clang}") || { echo "build-standalone: no ${CLANG:-clang} on PATH" >&2; exit 2; }
echo "stage 2: built by stage 1" >&2
"$out/stages/1/cancho" build $files --std --clang "$clang" -o "$out/stages/2/cancho"
echo "stage 3: built by stage 2" >&2
name3=${STANDALONE_STAGE3_NAME:-cancho}
"$out/stages/2/cancho" build $files --std --clang "$clang" -o "$out/stages/3/$name3"
[ -z "${STANDALONE_STAGE3_CORRUPT:-}" ] || printf 'x' >> "$out/stages/3/$name3"
# On macOS the linker gives an executable a UUID, a size and an ad-hoc signature that depend on the name of the file it writes (the first run of the name
# check there found 26,000 differing bytes, and none in the strings or the symbols). A stage 3 under another name is therefore compared by what the file
# says (its code, its symbols and its strings); the build under the name `cancho`, which is what is shipped, is compared whole.
same() { cmp -s "$1" "$2"; }
if [ "$name3" != cancho ] && [ "$(uname -s)" = Darwin ]; then
  same() {
    otool -tv "$1" 2>&1 | sed '1d' > "$out/stamped/cmp.a"   # the first line names the file
    otool -tv "$2" 2>&1 | sed '1d' > "$out/stamped/cmp.b"
    cmp -s "$out/stamped/cmp.a" "$out/stamped/cmp.b" || return 1
    for what in "nm -a" "strings -a"; do
      $what "$1" > "$out/stamped/cmp.a" 2>&1
      $what "$2" > "$out/stamped/cmp.b" 2>&1
      cmp -s "$out/stamped/cmp.a" "$out/stamped/cmp.b" || return 1
    done
  }
fi
if ! same "$out/stages/2/cancho" "$out/stages/3/$name3"; then
  echo "build-standalone: stage 2 and stage 3 differ: the command line in cancho does not reproduce itself" >&2
  # Where they differ goes to the log, so that a difference found on a machine nobody can reach is read from it rather than guessed at.
  echo "build-standalone: $(cmp -l "$out/stages/2/cancho" "$out/stages/3/$name3" 2>&1 | wc -l | tr -d ' ') differing bytes; first offsets (decimal, then the two values in octal):" >&2
  cmp -l "$out/stages/2/cancho" "$out/stages/3/$name3" 2>&1 | head -n 24 >&2 || true
  if command -v otool >/dev/null 2>&1; then
    otool -l "$out/stages/2/cancho" > "$out/stamped/l2.txt" 2>&1 || true
    otool -l "$out/stages/3/$name3" > "$out/stamped/l3.txt" 2>&1 || true
    echo "build-standalone: load commands that differ (otool -l):" >&2
    diff "$out/stamped/l2.txt" "$out/stamped/l3.txt" 2>&1 | head -n 30 >&2 || true
  fi
  if command -v strings >/dev/null 2>&1; then
    strings -a "$out/stages/2/cancho" > "$out/stamped/s2.txt" 2>&1 || true
    strings -a "$out/stages/3/$name3" > "$out/stamped/s3.txt" 2>&1 || true
    echo "build-standalone: strings that differ (strings -a):" >&2
    diff "$out/stamped/s2.txt" "$out/stamped/s3.txt" 2>&1 | head -n 30 >&2 || true
  fi
  if command -v nm >/dev/null 2>&1; then
    nm -a "$out/stages/2/cancho" > "$out/stamped/n2.txt" 2>&1 || true
    nm -a "$out/stages/3/$name3" > "$out/stamped/n3.txt" 2>&1 || true
    echo "build-standalone: symbols that differ (nm -a):" >&2
    diff "$out/stamped/n2.txt" "$out/stamped/n3.txt" 2>&1 | head -n 30 >&2 || true
  fi
  exit 1
fi
cp "$out/stages/2/cancho" "$out/cancho"
echo "ok: $(sha256sum "$out/cancho" | cut -d' ' -f1)  $out/cancho" >&2
