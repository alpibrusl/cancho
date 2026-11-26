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
if "$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --emit ll -o "$work/bare.ll" > "$work/ll.txt" 2>&1; then
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
if "$cancho" build tests/probes/freestanding.cho --std --emit ll -o "$work/host.ll" > "$work/hostll.txt" 2>&1; then
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
if "$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --emit obj -o "$work/bare.o" > "$work/obj.txt" 2>&1; then
    # The four bytes of the magic, then (at 18) the machine: 183, AArch64, little-endian.
    same "$(od -An -tx1 -N4 "$work/bare.o" | tr -d ' \n')" "7f454c46" "the magic"
    same "$(od -An -tx1 -j18 -N2 "$work/bare.o" | tr -d ' \n')" "b700" "the machine"
else
    fail "--emit obj said: $(head -c 300 "$work/obj.txt")"
fi
end

begin a_program_for_no_system_is_not_linked_by_the_compiler
"$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf -o "$work/bare.out" > "$work/exe.txt" 2>&1
same "$?" 2 "the status without --emit"
same "$(head -1 "$work/exe.txt")" '`--target aarch64-none-elf` makes an object file or a module, which the program'"'"'s own link script places: add `--emit obj` or `--emit ll`' "what it says"
"$cancho" run tests/probes/freestanding.cho --std --target aarch64-none-elf > "$work/run.txt" 2>&1
same "$?" 2 "the status of run"
end

# A builtin that needs a system (docs/freestanding.md 5). In a function nothing calls it is left out with the function; reached, it is refused by a rule, where it is written,
# and not by clang at a name the program never wrote.
begin a_builtin_that_needs_a_system_is_refused_only_if_the_program_reaches_it
if "$cancho" build tests/probes/bare_hosted.cho --target aarch64-none-elf --emit ll -o "$work/unreached.ll" > "$work/unreached.txt" 2>&1; then
    same "$(calls_undefined "$work/unreached.ll")" "" "what the module calls and does not define"
    same "$(grep -c 'needs-system' "$work/unreached.ll")" 0 "the note went with the function"
else
    fail "with the call not reached, the build said: $(head -c 300 "$work/unreached.txt")"
fi
sed 's/^    release(io);$/    borrow mut io as \&!i in { say(i, 65); }\
    release(io);/' tests/probes/bare_hosted.cho > "$work/reached.cho"
same "$(grep -c 'say(i, 65)' "$work/reached.cho")" 1 "the call was written in"
"$cancho" build "$work/reached.cho" --target aarch64-none-elf --emit obj -o "$work/reached.o" > "$work/reached.txt" 2>&1
same "$?" 1 "the status with the call reached"
same "$(head -1 "$work/reached.txt" | sed 's/^[^:]*:\([0-9]*:[0-9]*\): error: \(The program reaches something this target cannot do\).*/\1 \2/')" "7:12 The program reaches something this target cannot do" "where and what"
same "$(ls "$work" | grep -c '^reached.o$')" 0 "no object is written"
"$cancho" check "$work/reached.cho" --target aarch64-none-elf --output json 2> /dev/null | grep -c '"rule": "unsupported-on-target"' > "$work/tagged.txt"
same "$(cat "$work/tagged.txt")" 1 "check says it with the rule's tag"
if ! "$cancho" build "$work/reached.cho" -o "$work/reached" > "$work/reachedhost.txt" 2>&1; then
    fail "for this machine the same program is refused: $(head -c 300 "$work/reachedhost.txt")"
fi
end

# SHA-256 and HMAC-SHA-256 (docs/freestanding.md 8). `std.crypto` asks `hw_sha256()` before it compresses; with no system the answer is no and the software path is taken. The
# hardware functions are in the module all the same, since the branch is written: there they are a trap, and nothing with the processor's SHA instructions.
begin sha256_and_hmac_for_no_system_take_the_software_path_and_call_nothing
if "$cancho" build tests/probes/bare_sha256.cho --std --target aarch64-none-elf --emit ll -o "$work/sha.ll" > "$work/sha.txt" 2>&1; then
    same "$(calls_undefined "$work/sha.ll")" "probe_out " "what the module calls and does not define"
    same "$(grep -c 'needs-system' "$work/sha.ll")" 0 "nothing in it needs a system"
    same "$(grep -A2 '^define internal i8 @cancho_hw_sha256()' "$work/sha.ll" | grep -c '^  ret i8 0$')" 1 "hw_sha256() answers no"
    same "$(grep -A2 '^define internal void @cancho_sha256_blocks(' "$work/sha.ll" | grep -c 'call void @cancho_trap()')" 1 "the hardware path, reached anyway, traps"
    same "$(grep -c 'target-features' "$work/sha.ll")" 0 "no function asks for an instruction set"
else
    fail "--emit ll said: $(head -c 300 "$work/sha.txt")"
fi
if "$cancho" build tests/probes/bare_sha256.cho --std --target aarch64-none-elf --emit obj -o "$work/sha.o" > "$work/shaobj.txt" 2>&1; then
    same "$(od -An -tx1 -j18 -N2 "$work/sha.o" | tr -d ' \n')" "b700" "the machine of the object"
else
    fail "--emit obj said: $(head -c 300 "$work/shaobj.txt")"
fi
# The same program for this machine has the hardware path, so the two checks above that read the functions can fail.
if "$cancho" build tests/probes/bare_sha256.cho --std --emit ll -o "$work/shahost.ll" > "$work/shahost.txt" 2>&1; then
    same "$(grep -A2 '^define internal i8 @cancho_hw_sha256()' "$work/shahost.ll" | grep -c '^  ret i8 0$')" 0 "for this machine hw_sha256() asks the processor"
    case "$(grep -c 'target-features' "$work/shahost.ll")" in
        0) fail "the hosted module has no function with an instruction set" ;;
    esac
else
    fail "--emit ll for this machine said: $(head -c 300 "$work/shahost.txt")"
fi
end
