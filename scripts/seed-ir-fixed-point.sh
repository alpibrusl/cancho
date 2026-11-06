#!/bin/sh
# Measure the seed's IR fixed point (docs/seed.md §3): the LLVM IR the
# compiler emits for its own sources, built by stage 1 and by stage 2.
#
#   scripts/seed-ir-fixed-point.sh CANCHO0 [OUTDIR]
#
# CANCHO0 is a Rust-built `cancho` (stage 0). Stage 1 is built by it, stage 2
# by stage 1, stage 3 by stage 2; the IR clang is handed is captured for the
# last two. Exits 0 only if IR(stage 1 builds) == IR(stage 2 builds), and a
# binary built from that IR alone equals stage 3. Prints sha256 and sizes.
set -eu
C0=$1
OUT=${2:-seed-ir-out}
mkdir -p "$OUT/s1" "$OUT/s2" "$OUT/s3" "$OUT/r"
OUT=$(cd "$OUT" && pwd)
CLANG=$(command -v clang)
files=$(sed 's|^|examples/selfhost/|' examples/selfhost/cancho.files | tr '\n' ' ')
# cancho runs clang with an empty environment, so the tag is the -o directory.
cat > "$OUT/clangwrap.sh" <<W
#!/bin/sh
out=
prev=
for a in "\$@"; do case "\$prev" in -o) out=\$a;; esac; prev=\$a; done
tag=\$(basename "\$(dirname "\$out")")
for a in "\$@"; do case "\$a" in *.ll) cp "\$a" "$OUT/ir-\$tag.ll";; esac; done
exec $CLANG "\$@"
W
chmod +x "$OUT/clangwrap.sh"
# shellcheck disable=SC2086
CANCHO_COMPILER=auto "$C0" build $files --std -o "$OUT/s1/cancho"
# shellcheck disable=SC2086
"$OUT/s1/cancho" build $files --std --clang "$OUT/clangwrap.sh" -o "$OUT/s2/cancho"
# shellcheck disable=SC2086
"$OUT/s2/cancho" build $files --std --clang "$OUT/clangwrap.sh" -o "$OUT/s3/cancho"
echo "arch: $(uname -m)  $($CLANG --version | head -1)"
ok=0
cmp "$OUT/s2/cancho" "$OUT/s3/cancho" && echo "stage2 == stage3 (bytes)" || { echo "stage2 != stage3"; ok=1; }
sha256sum "$OUT/ir-s2.ll" "$OUT/ir-s3.ll"
wc -c "$OUT/ir-s2.ll" "$OUT/ir-s3.ll"
cmp "$OUT/ir-s2.ll" "$OUT/ir-s3.ll" && echo "IR(stage 1) == IR(stage 2)" || { echo "IR differs"; ok=1; }
cp "$OUT/ir-s3.ll" "$OUT/r/cancho.ll"
"$CLANG" -O2 -Wno-override-module "$OUT/r/cancho.ll" -o "$OUT/r/cancho"
cmp "$OUT/r/cancho" "$OUT/s3/cancho" && echo "binary from IR alone == stage 3 (bytes)" || { echo "binary from IR differs from stage 3"; ok=1; }
echo "gzip -9 size: $(gzip -9 -c "$OUT/ir-s3.ll" | wc -c)"
exit $ok
