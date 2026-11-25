# mmio.sh -- the script of tests/commands/script_tests.cho for `Mmio` (docs/mmio.md): the memory of the machine by address, as a capability.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# There is no machine with registers to run the probe on here, so what is read is the module (the loads and stores, and the checks ahead of them) and the authority
# report. That the checks let the right addresses through is seen on a kernel: alpibrusl/cancho-os boots in QEMU with its drivers written on these builtins.
cancho=$1
work=$2
. tests/commands/script_lib.sh

begin a_read_or_a_write_is_one_volatile_access_of_its_width
if "$cancho" build tests/probes/mmio.cho --target aarch64-none-elf --emit ll -o "$work/mmio.ll" > "$work/ll.txt" 2>&1; then
    same "$(grep -c 'load volatile i32, ptr %t[0-9]*, align 4$' "$work/mmio.ll")" 1 "the read of 32 bits"
    same "$(grep -c 'load volatile i64, ptr %t[0-9]*, align 8$' "$work/mmio.ll")" 1 "the read of 64 bits"
    same "$(grep -c 'load volatile i8, ptr %t[0-9]*, align 1$' "$work/mmio.ll")" 1 "the read of 8 bits"
    same "$(grep -c 'store volatile i32 %t[0-9]*, ptr %t[0-9]*, align 4$' "$work/mmio.ll")" 1 "the write of 32 bits"
    same "$(grep -c 'volatile' "$work/mmio.ll")" 4 "and no other"
else
    fail "--emit ll said: $(head -c 300 "$work/ll.txt")"
fi
end

# The UART's bound is two ranges of 0x90 bytes: a 32-bit access is inside one when its distance from the start is at most 0x90 - 4 = 140.
begin an_access_is_checked_against_each_range_of_the_bound
same "$(grep -c 'sub i64 %t[0-9]*, 1059065856$' "$work/mmio.ll")" 2 "the first range, 0x3f201000, for the read and the write"
same "$(grep -c 'sub i64 %t[0-9]*, 4263514112$' "$work/mmio.ll")" 2 "the second range, 0xfe201000"
same "$(grep -c 'icmp ule i64 %t[0-9]*, 140$' "$work/mmio.ll")" 4 "the room of each"
same "$(grep -c 'icmp ule i64 %t[0-9]*, 8$' "$work/mmio.ll")" 1 "the timer's sixteen bytes hold a 64-bit access at 0 to 8"
same "$(grep -c 'icmp ule i64 %t[0-9]*, 15$' "$work/mmio.ll")" 1 "and a byte at 0 to 15"
same "$(grep -c 'icmp ugt i64 %t[0-9]*, 4294967295$' "$work/mmio.ll")" 1 "a value written is one the width holds"
end

begin the_report_names_the_addresses_and_nothing_else
"$cancho" authority tests/probes/mmio.cho > "$work/auth.txt" 2>&1
same "$?" 0 "the status"
same "$(grep -c '^    mmio("0x3f201000..0x3f201090,0xfe201000..0xfe201090")$' "$work/auth.txt")" 1 "the UART of either board"
same "$(grep -c '^    mmio("0x40000040..0x40000050")$' "$work/auth.txt")" 1 "the timer"
same "$(grep -c '^    foreign code$' "$work/auth.txt")" 1 "foreign code is never touched"
same "$(grep -c 'UNBOUNDED' "$work/auth.txt")" 0 "and the program is bounded"
end

# With a system under the program there is no address to reach: the access is a trap, not a fault of the machine's choosing.
begin under_a_system_an_access_traps
if "$cancho" build tests/probes/mmio.cho -o "$work/hosted" > "$work/hosted.txt" 2>&1; then
    # (In a shell of its own, which is the one that says how the program died.)
    sh -c '"$1"; echo $? > "$2"' sh "$work/hosted" "$work/hosted.status" > /dev/null 2>&1
    case $(cat "$work/hosted.status") in
        0) fail "the hosted probe ran to the end" ;;
    esac
else
    fail "the build for this machine said: $(head -c 300 "$work/hosted.txt")"
fi
end
