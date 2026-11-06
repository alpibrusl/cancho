#!/bin/sh
# Write the seed for this machine's target (docs/seed.md §4).
#
#   scripts/reseed.sh [--cancho0 PATH] [--out DIR]
#
# Builds the chain from the committed seed (or from --cancho0): stage 1 is
# built by cancho0, stage 2 by stage 1, stage 3 by stage 2. The LLVM IR that
# clang is handed for stages 2 and 3 is captured; the two must be identical
# (the fixed point on IR, S5), and that IR becomes seed/<target>.ll.gz with its
# SHA-256 in seed/<target>.sha256. Run on each target; commit only seed/.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out=reseed-out
c0=
while [ $# -gt 0 ]; do
  case $1 in
    --cancho0) c0=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    *) echo "usage: reseed.sh [--cancho0 PATH] [--out DIR]" >&2; exit 2 ;;
  esac
done
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64|Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  *) echo "reseed: no seed target for $(uname -s)-$(uname -m)" >&2; exit 2 ;;
esac
clang=$(command -v "${CLANG:-clang}") || { echo "reseed: no ${CLANG:-clang} on PATH" >&2; exit 2; }
sha256() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
mkdir -p "$out/s1" "$out/s2" "$out/s3"
out=$(cd "$out" && pwd)
if [ -z "$c0" ]; then
  c0=$("$here/scripts/seed-build.sh" --out "$out/seed" --target "$target")
fi
cd "$here"
files=$(sed 's|^|examples/selfhost/|' examples/selfhost/cancho.files | tr '\n' ' ')
# cancho runs clang with an empty environment, so the wrapper names the capture
# after the directory of the -o argument.
cat > "$out/clangwrap.sh" <<W
#!/bin/sh
out=
prev=
for a in "\$@"; do case "\$prev" in -o) out=\$a;; esac; prev=\$a; done
tag=\$(basename "\$(dirname "\$out")")
for a in "\$@"; do case "\$a" in *.ll) cp "\$a" "$out/ir-\$tag.ll";; esac; done
exec $clang "\$@"
W
chmod +x "$out/clangwrap.sh"
# shellcheck disable=SC2086
CANCHO_COMPILER=auto "$c0" build $files --std -o "$out/s1/cancho"
# shellcheck disable=SC2086
"$out/s1/cancho" build $files --std --clang "$out/clangwrap.sh" -o "$out/s2/cancho"
# shellcheck disable=SC2086
"$out/s2/cancho" build $files --std --clang "$out/clangwrap.sh" -o "$out/s3/cancho"
if ! cmp -s "$out/ir-s2.ll" "$out/ir-s3.ll"; then
  echo "reseed: the IR is not a fixed point: stage 1 and stage 2 write different IR" >&2
  echo "reseed: $out/ir-s2.ll and $out/ir-s3.ll" >&2
  exit 1
fi
mkdir -p "$here/seed"
gzip -9 -n -c "$out/ir-s3.ll" > "$here/seed/$target.ll.gz"
echo "$(sha256 "$out/ir-s3.ll")  $target.ll" > "$here/seed/$target.sha256"
echo "reseed: wrote seed/$target.ll.gz ($(wc -c < "$here/seed/$target.ll.gz") bytes) and seed/$target.sha256"
