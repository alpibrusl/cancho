#!/bin/sh
# gen_run.sh -- a module of LLVM text run with its code from the native backend's code generator (docs/native-backend.md section 12.6). Not a test of the gate:
# it needs clang with the AArch64 target, an AArch64 C library to link with and qemu-aarch64 to run on, and is how the code generator was run before the
# object writer (N4) existed. On Debian or Ubuntu: apt-get install qemu-user gcc-aarch64-linux-gnu libc6-dev-arm64-cross.
#
#   tests/native/gen_run.sh GEN_DUMP MODULE.ll [WORK]
#
# GEN_DUMP is tests/native/gen_dump.cho, built. The module's functions are written as assembly by it; the module's data (its globals, with every name made
# visible to the linker, and its functions as declarations) is compiled by clang (tests/native/gen_data.py makes that module); the two are linked and run.
# Prints what the program printed, then `[status N]`. A module with a vector in it is refused: tests/native/gen/tricky.ll has none.
set -e
dump=$1; ll=$2; work=${3:-${TMPDIR:-/tmp}/gen-run.$$}
mkdir -p "$work"
here=$(cd "$(dirname "$0")" && pwd)
"$dump" "$ll" > "$work/code.s"
python3 "$here/gen_data.py" "$ll" "$work/data.ll"
clang --target=aarch64-linux-gnu -O1 -Wno-override-module -c "$work/data.ll" -o "$work/data.o"
clang --target=aarch64-linux-gnu -c "$work/code.s" -o "$work/code.o"
aarch64-linux-gnu-gcc "$work/code.o" "$work/data.o" -o "$work/program" -lpthread -lm
set +e
qemu-aarch64 -L /usr/aarch64-linux-gnu "$work/program"
echo "[status $?]"
