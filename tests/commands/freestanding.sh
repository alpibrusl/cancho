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
    same "$(awk '/^define void @cancho_dir_step\(/ { on = 1 } on && /call i1 @cancho_sys_component\(/ { print "held"; exit } on && /^}/ { exit }' "$work/files.ll")" "held" "and a name to being one component"
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

# And for what a directory holds (docs/bare-system.md, step 4): `dir_list`, `dir_next`, `dir_list_close` and `dir_stat` are the module's `list`, `next`,
# `close` and `stat`; a name given to `dir_stat` is held to being one component first.
begin a_lent_system_lists_a_directory
if "$cancho" build tests/probes/bare_system_list.cho tests/probes/bare_system_fs.cho --target aarch64-none-elf --emit ll -o "$work/list.ll" > "$work/list.txt" 2>&1; then
    same "$(calls_undefined "$work/list.ll")" "" "what the module calls and does not define"
    same "$(grep -c '^define void @cancho_dir_list(\|^define void @cancho_dir_next(\|^define i64 @cancho_dir_list_close(\|^define void @cancho_dir_stat(' "$work/list.ll")" 4 "the four functions the listing builtins call are the module's own"
    same "$(awk '/^define void @cancho_dir_stat\(/ { on = 1 } on && /call i1 @cancho_sys_component\(/ { print "held"; exit } on && /^}/ { exit }' "$work/list.ll")" "held" "a name is held to being one component before the module is asked about it"
    same "$(awk '/^define void @cancho_dir_stat\(/ { on = 1 } on && /call i64 @lexs_/ { n++ } on && /^}/ { print n; exit }' "$work/list.ll")" 3 "the kind, the size and the time are asked one after the other"
else
    fail "a program that lists a directory of a lent system, for no system, said: $(head -c 300 "$work/list.txt")"
fi
end

# And for what writes (docs/bare-system.md, step 4): `file_write`, `file_sync`, `file_lock`, `dir_rename` and `dir_rename_new`, `dir_remove` and `dir_sync` are
# the module's `write`, `sync`, `lock`, `rename`, `remove` and `sync` again; `dir_open_new` and `dir_open_append` are its `step`. Both names of a rename, and
# the name removed, are held to being one component first.
begin a_lent_system_writes_a_file
if "$cancho" build tests/probes/bare_system_write.cho tests/probes/bare_system_fs.cho --target aarch64-none-elf --emit ll -o "$work/write.ll" > "$work/write.txt" 2>&1; then
    same "$(calls_undefined "$work/write.ll")" "" "what the module calls and does not define"
    same "$(grep -c '^define i64 @cancho_fd_write(\|^define i64 @cancho_fd_sync(\|^define i64 @cancho_fd_lock(\|^define void @cancho_dir_rename(\|^define void @cancho_dir_remove(\|^define void @cancho_dir_sync(' "$work/write.ll")" 6 "the six functions the writing builtins call are the module's own"
    same "$(awk '/^define void @cancho_dir_rename\(/ { on = 1 } on && /call i1 @cancho_sys_component\(/ { n++ } on && /^}/ { print n; exit }' "$work/write.ll")" 2 "both names of a rename are held to being one component before the module is asked"
    same "$(awk '/^define void @cancho_dir_remove\(/ { on = 1 } on && /call i1 @cancho_sys_component\(/ { print "held"; exit } on && /^}/ { exit }' "$work/write.ll")" "held" "and so is a name that is removed"
else
    fail "a program that replaces a file of a lent system, for no system, said: $(head -c 300 "$work/write.txt")"
fi
# without the functions that write, the same program's calls are a trap, not a call of something missing
sed '/^\/\/ What is written: /,$d' tests/probes/bare_system_fs.cho > "$work/readonly_fs.cho"
if "$cancho" build tests/probes/bare_system_write.cho "$work/readonly_fs.cho" --target aarch64-none-elf --emit ll -o "$work/readonly.ll" > "$work/readonly.txt" 2>&1; then
    same "$(calls_undefined "$work/readonly.ll")" "" "what the module calls and does not define, when the system has nothing that writes"
    same "$(awk '/^define i64 @cancho_fd_write\(/ { on = 1 } on && /call void @cancho_trap\(/ { print "trap"; exit } on && /^}/ { exit }' "$work/readonly.ll")" "trap" "writing to a system that has no function for it is a trap"
else
    fail "with a system that has nothing that writes, the build said: $(head -c 300 "$work/readonly.txt")"
fi
end

# What a `system` block discharges (docs/bare-system.md 12). rule_for FILES...: the rule `check` refuses the program by, for no system; nothing if it is accepted.
rule_for() { "$cancho" check "$@" --target aarch64-none-elf --output json 2> /dev/null | grep -o '"rule": "[a-z-]*"' | head -n 1 | sed 's/.*: "\(.*\)"/\1/'; }
# rule_here FILES...: the same, for the machine this runs on.
rule_here() { "$cancho" check "$@" --output json 2> /dev/null | grep -o '"rule": "[a-z-]*"' | head -n 1 | sed 's/.*: "\(.*\)"/\1/'; }
# performs FILE: the labels of the report FILE, on one line.
performs() { awk '/^performs/ { on = 1; next } /^[a-z]/ { on = 0 } on { sub(/^ */, ""); printf "%s ", $0 }' "$1"; }
never() { awk '/^never touches/ { on = 1; next } /^[a-z]/ { on = 0 } on { sub(/^ */, ""); printf "%s, ", $0 }' "$1"; }

# The function that contains the block performs, for no system, none of the labels the module answers every function of: its row is empty, and one that still lists
# them says more than the body does. For this machine the block is only its body and the same function performs them: the empty row says less.
begin a_lent_system_discharges_the_labels_it_answers
same "$(rule_for tests/probes/bare_discharge.cho tests/probes/bare_system_io.cho)" "" "the row with nothing in it, for no system"
sed 's/^\(fn lend\[.*\) -> \[\] int {$/\1 -> [io_write, args] int {/' tests/probes/bare_discharge.cho > "$work/hosted_row.cho"
same "$(grep -c '^fn lend.* -> \[io_write, args\] int {$' "$work/hosted_row.cho")" 1 "the row for a system was written in"
same "$(rule_for "$work/hosted_row.cho" tests/probes/bare_system_io.cho)" "effect-declared-not-performed" "a row that still lists what is discharged, for no system"
sed 's/^\(fn lend\[.*\) -> \[\] int {$/\1 -> [args] int {/' tests/probes/bare_discharge.cho > "$work/one_left.cho"
same "$(rule_for "$work/one_left.cho" tests/probes/bare_system_io.cho)" "effect-declared-not-performed" "a row that still lists one of them"
same "$(rule_here tests/probes/bare_discharge.cho tests/probes/bare_system_io.cho)" "effect-not-declared" "the row with nothing in it, for this machine"
same "$(rule_here "$work/one_left.cho" tests/probes/bare_system_io.cho)" "effect-not-declared" "half of the row, for this machine"
if "$cancho" build "$work/hosted_row.cho" tests/probes/bare_system_io.cho -o "$work/discharge-host" > "$work/discharge-host.txt" 2>&1; then
    same "$("$work/discharge-host" one two | tail -n 2 | tr '\n' ' ')" "one two " "what it writes on this machine, with the row for a system"
else
    fail "with the row for a system, for this machine, the build said: $(head -c 300 "$work/discharge-host.txt")"
fi
# The module without `flush`: `io_write` has a function missing and is not discharged; `args` still is.
grep -v '^$' tests/probes/bare_system_io.cho | sed 's/^pub fn flush\[/pub fn flushed[/' > "$work/noflush.cho"
same "$(rule_for tests/probes/bare_discharge.cho "$work/noflush.cho")" "effect-not-declared" "with no flush in the module, the empty row"
sed 's/^\(fn lend\[.*\) -> \[\] int {$/\1 -> [io_write] int {/' tests/probes/bare_discharge.cho > "$work/write_left.cho"
same "$(rule_for "$work/write_left.cho" "$work/noflush.cho")" "" "with no flush in the module, the row with io_write"
# The report: for no system the discharged labels are not in it, whoever performed them; for this machine they are.
"$cancho" authority tests/probes/bare_discharge.cho tests/probes/bare_system_io.cho --target aarch64-none-elf > "$work/discharge-auth.txt" 2>&1
same "$(head -n 1 "$work/discharge-auth.txt")" "performs nothing" "the report for no system"
same "$(never "$work/discharge-auth.txt")" "the console, the filesystem, the network, the heap, the command line, signals, other programs, foreign code, " "what it never touches"
"$cancho" authority "$work/hosted_row.cho" tests/probes/bare_system_io.cho > "$work/discharge-auth-host.txt" 2>&1
same "$(performs "$work/discharge-auth-host.txt")" "args io_write " "the report for this machine"
"$cancho" authority "$work/write_left.cho" "$work/noflush.cho" --target aarch64-none-elf > "$work/noflush-auth.txt" 2>&1
same "$(performs "$work/noflush-auth.txt")" "io_write " "the report with no flush in the module"
end

# What the block does not discharge stays in the row, exactly: the addresses the module's own functions write to, the addresses the body writes to itself inside the block,
# and a hosted label the module has no function for.
begin what_a_lent_system_does_not_answer_stays_in_the_row
if "$cancho" build tests/probes/bare_discharge_own.cho tests/probes/bare_discharge_dev.cho --target aarch64-none-elf --emit ll -o "$work/own.ll" > "$work/own.txt" 2>&1; then
    same "$(calls_undefined "$work/own.ll")" "" "what the module calls and does not define"
    same "$(awk '/^define i32 @getchar\(/ { on = 1 } on && /call void @cancho_trap\(/ { print "trap"; exit } on && /^}/ { exit }' "$work/own.ll")" "trap" "reading with no key in the module is a trap"
else
    fail "a block with hardware of its own, for no system, said: $(head -c 300 "$work/own.txt")"
fi
sed 's/^\(fn lend\[.*\), mmio("0x2000\.\.0x2008")\] int {$/\1] int {/' tests/probes/bare_discharge_own.cho > "$work/nolamp.cho"
same "$(grep -c '^fn lend.*0x2000\.\.0x2008")\] int {$' "$work/nolamp.cho")" 0 "the lamp was taken out of the row"
same "$(rule_for "$work/nolamp.cho" tests/probes/bare_discharge_dev.cho)" "effect-not-declared" "without the addresses the body writes to itself"
sed 's/^\(fn lend\[.*\) -> \[io_read, mmio("0x1000\.\.0x1010"), /\1 -> [io_read, /' tests/probes/bare_discharge_own.cho > "$work/nocon.cho"
same "$(rule_for "$work/nocon.cho" tests/probes/bare_discharge_dev.cho)" "effect-not-declared" "without the addresses the module writes to"
sed 's/^\(fn lend\[.*\)\] int {$/\1, mmio("0x3000..0x3008")] int {/' tests/probes/bare_discharge_own.cho > "$work/extra.cho"
same "$(rule_for "$work/extra.cho" tests/probes/bare_discharge_dev.cho)" "effect-declared-not-performed" "with addresses nothing writes to"
sed 's/^\(fn lend\[.*\) -> \[io_read, /\1 -> [/' tests/probes/bare_discharge_own.cho > "$work/noread.cho"
same "$(grep -c '^fn lend.* -> \[mmio' "$work/noread.cho")" 1 "io_read was taken out of the row"
same "$(rule_for "$work/noread.cho" tests/probes/bare_discharge_dev.cho)" "effect-not-declared" "without io_read, which the module has no key for"
# A row that still lists what the block discharged is refused in a function that has addresses in its row too, where `check` alone would not say so.
sed 's/^\(fn lend\[.*\) -> \[io_read, /\1 -> [io_read, io_write, /' tests/probes/bare_discharge_own.cho > "$work/stale.cho"
same "$(grep -c '^fn lend.* -> \[io_read, io_write, mmio' "$work/stale.cho")" 1 "io_write was written into the row"
same "$(rule_for "$work/stale.cho" tests/probes/bare_discharge_dev.cho)" "effect-declared-not-performed" "a row with addresses that still lists what is discharged"
# The same for a path, with a module that answers paths: the row lists `fs_read("")` and nothing reads a file.
{ cat tests/probes/bare_discharge_dev.cho; printf '\npub fn open[&c, &u, &p](con: &!c Mmio("0x1000..0x1010"), used: &!u [int], path: &p [byte], mode: int) -> [] int {\n    return 0 - 2;\n}\n\npub fn folder[&c, &u, &p](con: &!c Mmio("0x1000..0x1010"), used: &!u [int], path: &p [byte]) -> [] int {\n    return 0 - 2;\n}\n'; } > "$work/withpaths.cho"
sed 's/^\(fn lend\[.*\) -> \[io_read, /\1 -> [io_read, fs_read(""), /' tests/probes/bare_discharge_own.cho > "$work/stale_path.cho"
same "$(rule_for tests/probes/bare_discharge_own.cho "$work/withpaths.cho")" "" "the row as it is, with a module that answers paths"
same "$(rule_for "$work/stale_path.cho" "$work/withpaths.cho")" "effect-declared-not-performed" "a row with addresses that lists a path nothing reads"
# With a `key` in the module the same row is the right one.
{ cat tests/probes/bare_discharge_dev.cho; printf '\npub fn key[&c, &u](con: &!c Mmio("0x1000..0x1010"), used: &!u [int]) -> [] int {\n    return 0 - 1;\n}\n'; } > "$work/withkey.cho"
same "$(rule_for "$work/noread.cho" "$work/withkey.cho")" "" "without io_read, with a key in the module"
"$cancho" authority tests/probes/bare_discharge_own.cho tests/probes/bare_discharge_dev.cho --target aarch64-none-elf > "$work/own-auth.txt" 2>&1
same "$(performs "$work/own-auth.txt")" 'io_read mmio("0x1000..0x1010") mmio("0x2000..0x2008") ' "the report: what was not discharged, the module's own and the body's own"
"$cancho" authority "$work/noread.cho" "$work/withkey.cho" --target aarch64-none-elf > "$work/key-auth.txt" 2>&1
same "$(performs "$work/key-auth.txt")" 'mmio("0x1000..0x1010") mmio("0x2000..0x2008") ' "the report with a key in the module"
end

# And a path: `fs_read("")` and `file_read` are discharged when the module has `open` and `folder`, and `read`, `read_at` and `size`. A file read outside the block is
# the function's own, before the block or after it.
begin a_lent_system_discharges_the_paths_it_answers
same "$(rule_for tests/probes/bare_discharge_files.cho tests/probes/bare_system_fs.cho)" "" "the row with nothing in it, for no system"
same "$(rule_here tests/probes/bare_discharge_files.cho tests/probes/bare_system_fs.cho)" "effect-not-declared" "the same row for this machine"
grep -v '^$' tests/probes/bare_system_fs.cho | sed 's/^pub fn open\[/pub fn opened[/' > "$work/noopen.cho"
same "$(rule_for tests/probes/bare_discharge_files.cho "$work/noopen.cho")" "effect-not-declared" "with no open in the module, the empty row"
sed 's/^\(fn lend\[.*\) -> \[\] int {$/\1 -> [fs_read("")] int {/' tests/probes/bare_discharge_files.cho > "$work/path_left.cho"
same "$(rule_for "$work/path_left.cho" "$work/noopen.cho")" "" "with no open in the module, the row with the path"
sed 's/^    var c = 0;$/    var c = first(fs);/' tests/probes/bare_discharge_files.cho > "$work/before.cho"
same "$(grep -c '^    var c = first(fs);$' "$work/before.cho")" 1 "a file is read before the block"
same "$(rule_for "$work/before.cho" tests/probes/bare_system_fs.cho)" "effect-not-declared" "a file read before the block, the empty row"
sed 's/^\(fn lend\[.*\) -> \[\] int {$/\1 -> [fs_read(""), file_read] int {/' "$work/before.cho" > "$work/before_row.cho"
same "$(rule_for "$work/before_row.cho" tests/probes/bare_system_fs.cho)" "" "a file read before the block, with its row"
sed 's/^    return c;$/    return c + first(fs);/' tests/probes/bare_discharge_files.cho > "$work/after.cho"
same "$(grep -c '^    return c + first(fs);$' "$work/after.cho")" 1 "a file is read after the block"
same "$(rule_for "$work/after.cho" tests/probes/bare_system_fs.cho)" "effect-not-declared" "a file read after the block, the empty row"
"$cancho" authority tests/probes/bare_discharge_files.cho tests/probes/bare_system_fs.cho --target aarch64-none-elf > "$work/files-auth.txt" 2>&1
same "$(head -n 1 "$work/files-auth.txt")" "performs nothing" "the report for no system"
"$cancho" authority "$work/path_left.cho" "$work/noopen.cho" --target aarch64-none-elf > "$work/noopen-auth.txt" 2>&1
same "$(performs "$work/noopen-auth.txt")" 'fs_read("") ' "the report with no open in the module"
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

# The same object from the native backend (docs/native-backend.md section 13): no clang is started, the allocator in it is code the native backend made, and
# the only names it leaves to the link are the heap's two ends, as for the object clang makes. `nm` is the judge: it is not the compiler's.
begin a_native_object_for_no_system_leaves_the_heaps_two_ends_to_the_link
if "$cancho" build tests/probes/bare_heap.cho --std --target aarch64-none-elf --emit obj --backend native --clang /nonexistent/clang -o "$work/heap.native.o" > "$work/heap.native.txt" 2>&1; then
    same "$(nm -u "$work/heap.native.o" | awk '{ print $NF }' | LC_ALL=C sort | tr '\n' ' ')" "__cancho_heap_end __cancho_heap_start " "what the native object does not define"
    same "$(nm "$work/heap.native.o" | awk '$2 == "T" && ($3 == "malloc" || $3 == "free" || $3 == "main") { print $3 }' | LC_ALL=C sort | tr '\n' ' ')" "free main malloc " "the allocator and the program, defined in it"
    if "$cancho" build tests/probes/bare_heap.cho --std --target aarch64-none-elf --emit obj -o "$work/heap.llvm.o" > /dev/null 2>&1; then
        same "$(nm -u "$work/heap.llvm.o" | awk '{ print $NF }' | LC_ALL=C sort | tr '\n' ' ')" "__cancho_heap_end __cancho_heap_start " "what clang's object does not define"
    else
        fail "the LLVM backend did not make the object to compare with"
    fi
else
    fail "--emit obj --backend native said: $(head -c 300 "$work/heap.native.txt")"
fi
# A program for Linux leaves only functions of the C library: no name of the compiler's runtime, and no function of LLVM's.
"$cancho" build tests/probes/bare_heap.cho --std --host linux-aarch64 --emit obj --backend native -o "$work/hosted.native.o" > "$work/hosted.native.txt" 2>&1
same "$?" 0 "the status for Linux"
same "$(nm -u "$work/hosted.native.o" | grep -c 'cancho\|llvm')" 0 "names of the runtime or of LLVM left to the link"
case "$(nm -u "$work/hosted.native.o" | grep -c ' malloc$')" in 1) ;; *) fail "the object for Linux does not ask the C library for malloc" ;; esac
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
