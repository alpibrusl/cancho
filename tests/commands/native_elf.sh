# native_elf.sh -- the script of tests/commands/script_tests.cho for the native backend's objects (docs/native-backend.md section 13, epic #840 N4).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# The object of tests/native/gen/tricky.ll is kept, as the bytes `od` prints (tests/native/gen/tricky.o.hex): it is the object that was linked by the system's
# linker and run under qemu, where it printed what clang's build of the module prints, and that `llvm-readelf -a` (kept beside it, tricky.readelf) and `llvm-objdump -dr`
# read with no warning. The gate has no AArch64 machine but one (stage `native`, on Linux on AArch64, is where programs built so are run), so here the
# writer must write those bytes again; and the compiler must write an object for a machine with no system from any host.
cancho=$1
work=$2
. tests/commands/script_lib.sh
# The bytes of a file, sixteen to a line, written the same way whatever `od` this machine has (they space their columns differently).
hex() { od -An -v -tx1 "$1" | tr -s ' \n' '\n\n' | grep . | awk '{ printf "%s%s", $0, (NR % 16 == 0 ? "\n" : " ") } END { if (NR % 16) print "" }'; }
native="compiler/native/llmod.cho compiler/native/llread.cho compiler/native/llbody.cho compiler/native/lltop.cho compiler/native/lllayout.cho compiler/native/llout.cho compiler/native/a64.cho compiler/native/a64sys.cho compiler/native/a64frame.cho compiler/native/a64call.cho compiler/native/a64gen.cho compiler/native/a64fn.cho compiler/native/elf.cho"

begin the_writer_writes_the_object_that_was_run
# shellcheck disable=SC2086
if "$cancho" build tests/native/gen_object.cho $native --std -o "$work/object" > "$work/build.txt" 2>&1; then
    "$work/object" tests/native/gen/tricky.ll > "$work/tricky.o" 2> "$work/tricky.txt"
    same "$?" 0 "the status"
    hex "$work/tricky.o" > "$work/tricky.o.hex"
    same "$(cmp -s "$work/tricky.o.hex" tests/native/gen/tricky.o.hex; echo $?)" 0 "the bytes against tests/native/gen/tricky.o.hex"
    same "$(od -An -tx1 -N4 "$work/tricky.o" | tr -d ' \n')" "7f454c46" "the magic"
    same "$(od -An -tx1 -j16 -N4 "$work/tricky.o" | tr -d ' \n')" "0100b700" "a relocatable file for AArch64"
else
    fail "building the object writer said: $(head -c 300 "$work/build.txt")"
fi
end

# One instruction changed, and one value, are other bytes: the comparison above can fail.
begin a_changed_module_is_another_object
sed 's/^  %d = sub i64 %b, %m$/  %d = add i64 %b, %m/' tests/native/gen/tricky.ll > "$work/code.ll"
"$work/object" "$work/code.ll" > "$work/code.o" 2> /dev/null
hex "$work/code.o" > "$work/code.hex"
same "$(diff "$work/code.hex" tests/native/gen/tricky.o.hex | grep -c '^<')" 1 "lines of bytes that differ for the instruction"
sed 's/^@cell = global i64 5$/@cell = global i64 6/' tests/native/gen/tricky.ll > "$work/value.ll"
"$work/object" "$work/value.ll" > "$work/value.o" 2> /dev/null
hex "$work/value.o" > "$work/value.hex"
same "$(diff "$work/value.hex" tests/native/gen/tricky.o.hex | grep -c '^<')" 1 "lines of bytes that differ for the value"
end

# From any machine, a program for a machine with no system is an object the native backend writes itself: no clang is started (the one named does not exist).
begin the_compiler_writes_an_object_for_no_system_with_no_clang
for probe in freestanding cpu mmio bare_heap; do
    "$cancho" build tests/probes/$probe.cho --std --target aarch64-none-elf --emit obj --backend native --clang /nonexistent/clang -o "$work/$probe.o" > "$work/$probe.txt" 2>&1
    same "$?" 0 "the status for $probe"
    same "$(od -An -tx1 -N4 "$work/$probe.o" 2> /dev/null | tr -d ' \n')" "7f454c46" "the magic of $probe"
    same "$(od -An -tx1 -j16 -N4 "$work/$probe.o" 2> /dev/null | tr -d ' \n')" "0100b700" "what $probe is for"
done
same "$(ls "$work" | grep -c 'native.o$\|link.log$')" 0 "nothing is left beside the objects"
# The LLVM backend's object for the same program is another file, and that one needs clang.
"$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --emit obj --clang /nonexistent/clang -o "$work/llvm.o" > /dev/null 2>&1
same "$?" 3 "the LLVM backend with no clang"
end

# The hashes and the ciphers (docs/native-backend.md sections 13.2 and 14): for the native backend the module has no vector and none of the processor's
# instructions, and the question "has this processor SHA-256?" is a function that answers no, so the library's own code is what runs. That the answers are
# then the published ones is tests/accept/sha256.cho and its kin, which stage `native` of the gate builds and runs.
begin a_program_that_hashes_asks_no_processor_for_the_native_backend
"$cancho" build tests/accept/sha256.cho --std --emit ll --host linux-aarch64 -o "$work/sha.ll" > /dev/null 2>&1
case "$(grep -c 'sha256h\|<4 x i32>' "$work/sha.ll")" in 0) fail "the module for the LLVM backend has none of the instructions: the next check shows nothing" ;; esac
"$cancho" build tests/accept/sha256.cho --std --emit ll --host linux-aarch64 --backend native -o "$work/sha.native.ll" > "$work/sha.txt" 2>&1
same "$?" 0 "the status for the native backend's module"
same "$(grep -c 'sha256h\|<[0-9]* x ' "$work/sha.native.ll")" 0 "vectors and hash instructions in it"
same "$(awk '/^define internal i8 @cancho_hw_sha256\(\) \{$/ { on = 1; next } on && /^}/ { exit } on { printf "%s;", $0 }' "$work/sha.native.ll")" "entry:;  ret i8 0;" "what the question answers"
"$cancho" build tests/accept/sha256.cho --std --emit obj --host linux-aarch64 --backend native -o "$work/sha.o" > /dev/null 2>&1
same "$?" 0 "the status for its object"
same "$(od -An -tx1 -j16 -N4 "$work/sha.o" 2> /dev/null | tr -d ' \n')" "0100b700" "what the object is for"
end

# A program the front end refuses is refused the same way whichever backend is asked for: the backend is not reached. So tests/reject needs no native run.
begin a_refused_program_is_refused_the_same_with_either_backend
n=0
for f in tests/reject/assignment_type_mismatch.cho tests/reject/box_leaked.cho tests/reject/effect_not_propagated.cho; do
    [ -f "$f" ] || continue
    n=$((n + 1))
    "$cancho" check "$f" --output json > "$work/llvm.json" 2> /dev/null
    one=$?
    "$cancho" check "$f" --output json --backend native --host linux-aarch64 > "$work/native.json" 2> /dev/null
    same "$?" "$one" "the status for $f"
    same "$(cmp -s "$work/llvm.json" "$work/native.json"; echo $?)" 0 "what is said of $f"
done
[ "$n" -ge 1 ] || fail "none of the fixtures named here exists"
end

# For a machine the native backend writes no code for, it says so, and writes nothing.
begin another_machine_is_refused_by_the_rule
printf 'fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io);\n    release(ffi);\n    release(fs);\n    release(heap);\n    release(args);\n    return 0;\n}\n' > "$work/plain.cho"
mkdir "$work/out"
for host in linux darwin; do
    "$cancho" build "$work/plain.cho" --backend native --host $host -o "$work/out/plain" > "$work/$host.txt" 2>&1
    same "$?" 1 "the status for $host"
    same "$(sed -n 2p "$work/$host.txt")" "    a machine the native backend writes no code for yet: it has AArch64 on Linux and with no system (x86-64 and macOS are later)" "what is said for $host"
done
same "$(ls "$work/out" | wc -l | tr -d ' ')" 0 "what was written"
end
