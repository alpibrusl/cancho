# cpu.sh -- the script of tests/commands/script_tests.cho for `Cpu` (docs/cpu.md): the processor's own registers, as a capability.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# There is no bare AArch64 machine to run the probe on here, so what is read is the module (one instruction a builtin, the register in it) and the authority report.
# That the registers are the right ones is seen on a kernel: alpibrusl/cancho-os boots in QEMU with its timer, MMU and fault report written on these builtins.
cancho=$1
work=$2
. tests/commands/script_lib.sh

begin a_register_is_read_and_written_by_one_instruction_that_names_it
if "$cancho" build tests/probes/cpu.cho --target aarch64-none-elf --emit ll -o "$work/cpu.ll" > "$work/ll.txt" 2>&1; then
    same "$(grep -c 'call i64 asm sideeffect "mrs $0, cntvct_el0", "=r"()$' "$work/cpu.ll")" 1 "the read of the counter"
    same "$(grep -c 'call i64 asm sideeffect "mrs $0, cntfrq_el0", "=r"()$' "$work/cpu.ll")" 1 "the read of its frequency"
    same "$(grep -c 'call void asm sideeffect "msr cntv_tval_el0, $0\\0Aisb", "r"(i64 %t[0-9]*)$' "$work/cpu.ll")" 1 "a write, and the isb that makes it hold"
    same "$(grep -c 'call void asm sideeffect "wfi", ""()$' "$work/cpu.ll")" 1 "the wait"
    same "$(grep -c 'call void asm sideeffect "tlbi vmalle1\\0Adsb nsh\\0Aisb", ""()$' "$work/cpu.ll")" 1 "the flush"
    same "$(grep -c 'asm sideeffect "m[rs][rs] ' "$work/cpu.ll")" 4 "and no other register"
else
    fail "--emit ll said: $(head -c 300 "$work/ll.txt")"
fi
end

begin the_report_names_each_register_and_no_foreign_code
"$cancho" authority tests/probes/cpu.cho > "$work/auth.txt" 2>&1
same "$?" 0 "the status"
same "$(grep '^    cpu(' "$work/auth.txt" | tr -d ' \n')" 'cpu("cntfrq_el0")cpu("cntv_ctl_el0")cpu("cntv_tval_el0")cpu("cntvct_el0")cpu("tlbi_vmalle1")cpu("wfi")' "the registers and the instructions"
same "$(grep -c '^    foreign code$' "$work/auth.txt")" 1 "foreign code is never touched"
same "$(grep -c 'UNBOUNDED' "$work/auth.txt")" 0 "and the program is bounded"
end

# Another machine has other registers, and a program under a system is not let near them: the access is a trap, and none of the assembly is in the module.
begin on_another_target_an_access_traps
if "$cancho" build tests/probes/cpu.cho --emit ll -o "$work/host.ll" > "$work/hostll.txt" 2>&1; then
    same "$(grep -c 'mrs \|msr \|wfi\|tlbi' "$work/host.ll")" 0 "no AArch64 instruction in the module for this machine"
else
    fail "--emit ll for this machine said: $(head -c 300 "$work/hostll.txt")"
fi
if "$cancho" build tests/probes/cpu.cho -o "$work/hosted" > "$work/hosted.txt" 2>&1; then
    # (In a shell of its own, which is the one that says how the program died.)
    sh -c '"$1"; echo $? > "$2"' sh "$work/hosted" "$work/hosted.status" > /dev/null 2>&1
    case $(cat "$work/hosted.status") in
        0) fail "the hosted probe ran to the end" ;;
    esac
else
    fail "the build for this machine said: $(head -c 300 "$work/hosted.txt")"
fi
end
