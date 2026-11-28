#!/bin/sh
# regalloc_random.sh -- one module of tests/native/regalloc_random.py built three ways and run (docs/native-backend.md section 17.9): by clang, and by the
# native backend's code generator with the variables made values and the values given registers and without (`--no-registers`), each linked and run
# under qemu-aarch64. Not a test of the gate: it needs what tests/native/gen_run.sh needs. Prints `SEED same N` (N lines printed, the same by all three) or
# `SEED DIFF ...`, and keeps the files of a difference in WORK.
#
#   tests/native/regalloc_random.sh GEN_DUMP SEED [FUNCTIONS [WORK]]
#   seq 1 60 | xargs -P 4 -I{} sh tests/native/regalloc_random.sh GEN_DUMP {} 50 /tmp/regalloc-random     # the run of section 17.9
#
# GEN_DUMP is tests/native/gen_dump.cho built with compiler/native/*.cho.
dump=$1; seed=$2; count=${3:-50}; top=${4:-${TMPDIR:-/tmp}/regalloc-random}
here=$(cd "$(dirname "$0")" && pwd)
work=$top/$seed
mkdir -p "$work"
cd "$work" || exit 1
python3 "$here/regalloc_random.py" "$seed" "$count" module.ll
python3 "$here/gen_data.py" module.ll data.ll
clang --target=aarch64-linux-gnu -O1 -Wno-override-module -c data.ll -o data.o 2> data.err || { echo "$seed DIFF data: $(head -c 200 data.err)"; exit 0; }
clang --target=aarch64-linux-gnu -O0 -Wno-override-module -c module.ll -o clang.o 2> clang.err || { echo "$seed DIFF clang: $(head -c 200 clang.err)"; exit 0; }
aarch64-linux-gnu-gcc clang.o -o clang.prog -lm
for mode in registers none; do
    "$dump" module.ll $mode counts > $mode.s 2> $mode.txt || { echo "$seed DIFF $mode: $(cat $mode.txt)"; exit 0; }
    clang --target=aarch64-linux-gnu -c $mode.s -o $mode.o
    aarch64-linux-gnu-gcc $mode.o data.o -o $mode.prog -lm
done
for prog in clang registers none; do
    timeout 60 qemu-aarch64 -L /usr/aarch64-linux-gnu ./$prog.prog > $prog.out 2>&1
    echo "[status $?]" >> $prog.out
done
rm -f qemu_*.core
if cmp -s clang.out registers.out && cmp -s clang.out none.out; then
    echo "$seed same $(($(wc -l < clang.out) - 1)) $(cat registers.txt)"
    rm -f ./*.o ./*.prog ./*.s
else
    echo "$seed DIFF registers=$(cmp -s clang.out registers.out && echo same || echo differs) none=$(cmp -s clang.out none.out && echo same || echo differs)"
fi
