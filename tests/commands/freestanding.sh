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

# The heap (docs/freestanding.md 8): a program that boxes a value and grows a buffer builds for no system, and what it calls for memory is the module's own.
begin a_program_for_no_system_has_a_heap_of_its_own
if "$cancho" build tests/probes/bare_heap.cho --std --target aarch64-none-elf --emit ll -o "$work/heap.ll" > "$work/heap.txt" 2>&1; then
    same "$(calls_undefined "$work/heap.ll")" "" "what the module calls and does not define"
    same "$(grep -c '^define ptr @malloc(i64' "$work/heap.ll")" 1 "malloc is the module's own"
    same "$(grep -c '^define void @free(ptr' "$work/heap.ll")" 1 "free is the module's own"
    same "$(grep -c '^define ptr @calloc(i64' "$work/heap.ll")" 1 "calloc is the module's own"
    same "$(grep -c '^declare ptr @malloc\|^declare void @free\|^declare ptr @calloc' "$work/heap.ll")" 0 "none of the three is declared as somebody else's"
    case "$(grep -c 'call ptr @malloc\|call void @free' "$work/heap.ll")" in
        0) fail "the module never calls malloc or free: the check above shows nothing" ;;
    esac
    same "$(grep -c 'needs-system' "$work/heap.ll")" 0 "the heap needs no system"
else
    fail "a program that uses the heap, for no system, said: $(head -c 300 "$work/heap.txt")"
fi
# and it is a program: for the machine this runs on it answers the length of the object it made, {"sum":7}
if "$cancho" build tests/probes/bare_heap.cho --std -o "$work/heap-host" > "$work/heap-host.txt" 2>&1; then
    "$work/heap-host"
    same "$?" 9 "what the same program answers on this machine"
else
    fail "the same program for this machine said: $(head -c 300 "$work/heap-host.txt")"
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
