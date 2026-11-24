# freestanding.sh -- the script of tests/commands/script_tests.cho for `--target aarch64-none-elf` and `--emit ll` (docs/freestanding.md).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# There is no machine to run the program on here, so what is checked is what the module asks of whoever links it: the functions it calls and does not define. For a
# hosted target that list is a libc; for this one it must be the program's own foreign function and nothing else (the two symbols of the heap are data, and are
# looked for by name). An operating system that boots from such an object is the test that it runs: alpibrusl/cancho-os builds with this target in its CI.
cancho=$1
work=$2
. tests/commands/script_lib.sh

# calls_undefined FILE: the names the module FILE calls and does not define, apart from LLVM's own intrinsics, sorted, on one line.
calls_undefined() {
    grep '^define' "$1" | sed 's/^[^@]*@\([A-Za-z0-9_.]*\).*/\1/' | LC_ALL=C sort -u > "$work/defined.txt"
    grep -o 'call [^@]*@[A-Za-z0-9_.]*' "$1" | sed 's/.*@//' | grep -v '^llvm\.' | LC_ALL=C sort -u > "$work/called.txt"
    LC_ALL=C comm -23 "$work/called.txt" "$work/defined.txt" | tr '\n' ' '
}

begin a_module_for_no_system_calls_nothing_but_the_programs_own_foreign_function
if "$cancho" build tests/probes/freestanding.cho --target aarch64-none-elf --emit ll -o "$work/bare.ll" > "$work/ll.txt" 2>&1; then
    same "$(calls_undefined "$work/bare.ll")" "probe_out " "what the module calls and does not define"
    same "$(grep -c 'udf #0xc11f' "$work/bare.ll")" 1 "the trap is aarch64's"
    same "$(grep -c '^@__cancho_heap_start = external global' "$work/bare.ll")" 1 "the heap is the link's"
    same "$(grep -c '^define ptr @memset(' "$work/bare.ll")" 1 "memset is the module's own"
else
    fail "--emit ll said: $(head -c 300 "$work/ll.txt")"
fi
end

# The same program for the machine this runs on calls a libc: the check above can fail.
begin the_same_module_for_this_machine_calls_a_libc
if "$cancho" build tests/probes/freestanding.cho --emit ll -o "$work/host.ll" > "$work/hostll.txt" 2>&1; then
    case " $(calls_undefined "$work/host.ll")" in
        *" malloc "*) ;;
        *) fail "the hosted module does not call malloc: $(calls_undefined "$work/host.ll")" ;;
    esac
    same "$(ls "$work" | grep -c 'host.ll.clang.log\|host.ll.ll')" 0 "nothing is left beside the module"
else
    fail "--emit ll for this machine said: $(head -c 300 "$work/hostll.txt")"
fi
end

begin an_object_for_no_system_is_an_elf_for_aarch64
if "$cancho" build tests/probes/freestanding.cho --target aarch64-none-elf --emit obj -o "$work/bare.o" > "$work/obj.txt" 2>&1; then
    # The four bytes of the magic, then (at 18) the machine: 183, AArch64, little-endian.
    same "$(od -An -tx1 -N4 "$work/bare.o" | tr -d ' \n')" "7f454c46" "the magic"
    same "$(od -An -tx1 -j18 -N2 "$work/bare.o" | tr -d ' \n')" "b700" "the machine"
else
    fail "--emit obj said: $(head -c 300 "$work/obj.txt")"
fi
end

begin a_program_for_no_system_is_not_linked_by_the_compiler
"$cancho" build tests/probes/freestanding.cho --target aarch64-none-elf -o "$work/bare.out" > "$work/exe.txt" 2>&1
same "$?" 2 "the status without --emit"
same "$(head -1 "$work/exe.txt")" '`--target aarch64-none-elf` makes an object file or a module, which the program'"'"'s own link script places: add `--emit obj` or `--emit ll`' "what it says"
"$cancho" run tests/probes/freestanding.cho --target aarch64-none-elf > "$work/run.txt" 2>&1
same "$?" 2 "the status of run"
end
