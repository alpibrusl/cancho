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

# A system lent for a block (docs/bare-system.md): a program that writes to its `Io` and reads its `Args` builds for no system when a module of its own answers
# them, and the functions the builtins call are then the module's own. Without the block the same builtins are refused, as before.
begin a_program_for_no_system_lends_one
if "$cancho" build tests/probes/bare_system.cho tests/probes/bare_system_io.cho --target aarch64-none-elf --emit ll -o "$work/system.ll" > "$work/system.txt" 2>&1; then
    same "$(calls_undefined "$work/system.ll")" "" "what the module calls and does not define"
    same "$(grep -c '^define i64 @cancho_write_bytes(ptr %p, i64 %n)\|^define i32 @putchar(i32 %c)\|^define i64 @write(i32 %fd, ptr %p, i64 %n)\|^define i32 @getchar()\|^define void @cancho_flush_out(ptr %out)\|^define ptr @cancho_arg(i64 %n)\|^define i64 @strlen(ptr %s)' "$work/system.ll")" 7 "the seven functions the builtins call are the module's own"
    same "$(grep -c '^@cancho_sys_on = global i64 0' "$work/system.ll")" 1 "whether something is lent"
    same "$(grep -c 'store i64 1, ptr @cancho_sys_on' "$work/system.ll")" 1 "it is lent at the block"
    same "$(grep -c 'store i64 0, ptr @cancho_sys_on' "$work/system.ll")" 1 "and not after it"
    same "$(grep -c '^declare i32 @putchar\|^declare i32 @getchar\|^declare i64 @write' "$work/system.ll")" 0 "none is declared as somebody else's"
else
    fail "a program that lends a system, for no system, said: $(head -c 300 "$work/system.txt")"
fi
# the same program for the machine this runs on: the block is only its body, and the arguments are the command line's
if "$cancho" build tests/probes/bare_system.cho tests/probes/bare_system_io.cho -o "$work/system-host" > "$work/system-host.txt" 2>&1; then
    "$work/system-host" one two > "$work/system-out.txt"
    same "$?" 3 "what it answers on this machine"
    same "$(tail -n 2 "$work/system-out.txt" | tr '\n' ' ')" "one two " "what it writes on this machine"
else
    fail "the same program for this machine said: $(head -c 300 "$work/system-host.txt")"
fi
# without the block, the builtin is refused where it is written
sed 's/^                system probe_io(mem, used) in {$/                region q {/' tests/probes/bare_system.cho > "$work/nosystem.cho"
same "$(grep -c 'system probe_io' "$work/nosystem.cho")" 0 "the block was taken out"
"$cancho" build "$work/nosystem.cho" tests/probes/bare_system_io.cho --target aarch64-none-elf --emit ll -o "$work/nosystem.ll" > "$work/nosystem.txt" 2>&1
same "$?" 1 "the status without the block"
case "$(cat "$work/nosystem.txt")" in
    *"this target cannot do"*) ;;
    *) fail "without the block the build said: $(head -c 300 "$work/nosystem.txt")" ;;
esac
end

# The same for files that are read (docs/bare-system.md, step 3): a program that opens a directory, opens a file under it and reads it builds for no system when
# the module answers `folder`, `step`, `open`, `read`, `read_at`, `size` and `close`; a path is held to its capability, and a name to being one component, by
# functions of the runtime before the module is asked.
begin a_lent_system_reads_files
if "$cancho" build tests/probes/bare_system_files.cho tests/probes/bare_system_fs.cho --target aarch64-none-elf --emit ll -o "$work/files.ll" > "$work/files.txt" 2>&1; then
    same "$(calls_undefined "$work/files.ll")" "" "what the module calls and does not define"
    same "$(grep -c '^define void @cancho_dir_open(\|^define void @cancho_dir_step(\|^define i64 @cancho_open(\|^define i64 @cancho_fd_read(\|^define i64 @cancho_fd_pread(\|^define i64 @cancho_fd_size(\|^define i64 @cancho_fd_close(' "$work/files.ll")" 7 "the seven functions the file builtins call are the module's own"
    same "$(grep -c 'call i1 @cancho_path_covers(' "$work/files.ll")" 2 "a path is held to its capability before the module is asked, for a directory and for a file"
    same "$(grep -c 'call i1 @cancho_sys_component(' "$work/files.ll")" 1 "and a name to being one component"
else
    fail "a program that reads files from a lent system, for no system, said: $(head -c 300 "$work/files.txt")"
fi
# a module with no `step`: the function is there and traps, so the program still builds
grep -v '^$' tests/probes/bare_system_fs.cho | sed 's/^pub fn step\[/pub fn stepped[/' > "$work/nostep.cho"
if "$cancho" build tests/probes/bare_system_files.cho "$work/nostep.cho" --target aarch64-none-elf --emit ll -o "$work/nostep.ll" > "$work/nostep.txt" 2>&1; then
    same "$(awk '/^define void @cancho_dir_step\(/ { on = 1 } on && /cancho_trap/ { print "traps"; exit } on && /^}/ { exit }' "$work/nostep.ll")" "traps" "an operation the module does not answer"
else
    fail "with no \`step\` the build said: $(head -c 300 "$work/nostep.txt")"
fi
# a function that does not fit its place is refused where the block is written
sed 's/^pub fn size\[&u\](at: &!u \[int\], file: int)/pub fn size[\&u, \&p](at: \&!u [int], file: \&p [byte])/' tests/probes/bare_system_fs.cho > "$work/badsize.cho"
same "$(grep -c 'file: &p \[byte\]' "$work/badsize.cho")" 1 "the function was changed"
"$cancho" build tests/probes/bare_system_files.cho "$work/badsize.cho" --target aarch64-none-elf --emit ll -o "$work/badsize.ll" > "$work/badsize.txt" 2>&1
same "$?" 1 "the status with a function that does not fit"
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
