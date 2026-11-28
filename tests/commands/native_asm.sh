# native_asm.sh -- the script of tests/commands/script_tests.cho for the assembler of the native backend (docs/native-backend.md, epic #840 N7).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# tests/native/asm/NAME.expected is what clang's assembler made of tests/native/asm/NAME.S: each section's alignment and bytes, the labels, the relocations
# (tests/native/asm_vectors.py wrote them, and is not run here: the gate needs no assembler). tests/native/asm_check.cho assembles the same source with
# compiler/native/a64asm.cho and prints the same lines.
cancho=$1
work=$2
. tests/commands/script_lib.sh

# refused LINES WANT WHAT: the source of those lines (printf's format) is refused with exactly WANT.
refused() {
    # shellcheck disable=SC2059
    printf "$1" > "$work/bad.S"
    "$work/check" "$work/bad.S" > "$work/bad.txt" 2>&1
    same "$?" 1 "the status for $3"
    same "$(cat "$work/bad.txt")" "$2" "$3"
}

begin what_clangs_assembler_made_of_a_source_is_what_the_assembler_makes
if "$cancho" build tests/native/asm_check.cho compiler/native/a64asm.cho compiler/native/a64asmops.cho compiler/native/a64.cho compiler/native/a64sys.cho compiler/native/llmod.cho --std -o "$work/check" > "$work/build.txt" 2>&1; then
    for source in forms virt; do
        "$work/check" "tests/native/asm/$source.S" > "$work/$source.txt" 2> "$work/$source.err"
        same "$?" 0 "the status for $source.S"
        same "$(cmp "$work/$source.txt" "tests/native/asm/$source.expected" 2>&1)" "" "$source.S"
    done
    same "$(grep -c '^section ' tests/native/asm/forms.expected)" 3 "the sections of forms.S"
    same "$(cut -d' ' -f2 tests/native/asm/forms.expected | grep -cE '^(abs64|call26|jump26|adr21|cond19|literal19)$')" "$(grep -c '^relocation ' tests/native/asm/forms.expected)" "the relocations of forms.S whose kind is one of the six"
    same "$(grep '^relocation ' tests/native/asm/forms.expected | cut -d' ' -f2 | LC_ALL=C sort -u | wc -l | tr -d ' ')" 6 "the kinds of relocation forms.S has"
else
    fail "building the check said: $(head -c 300 "$work/build.txt")"
fi
end

# A source the assembler does not understand is refused, with the line and the reason, and nothing is guessed.
begin a_source_outside_the_subset_is_refused_with_its_line
refused 'entry:\n    wfe\n    nop\n' "refused line 3: this is not an instruction of the subset" "an instruction that is not of the subset"
refused '    mov x0, #1\n    mov x31, #1\n' "refused line 2: a register was expected" "a register there is not"
refused '    mov x0, #1\n\n\n    b 3f\n1:  wfe\n' "refused line 4: no numbered label of this section is there in that direction" "a numbered label that is not there"
refused '3:  wfe\n.section .other, "ax"\n    b 3b\n' "refused line 3: no numbered label of this section is there in that direction" "a numbered label of another section"
refused '    and x0, x0, #3\n    and x0, x0, #5\n' "refused line 2: the instruction has no encoding for this operand" "a number and cannot take"
refused '    add x0, x0, #0x1001\n' "refused line 1: the instruction has no encoding for this operand" "a number add cannot take"
refused '    wfe\n.macro one a\n    mov x0, #\\a\n' "refused line 2: the macro has no .endm" "a macro without its end"
refused '.macro one a\n    mov x0, #\\b\n.endm\n    wfe\n    one 1\n' "refused line 5: a macro is used or written in a way the subset does not have" "a parameter the macro does not have"
refused '    ldr w0, =0x12345\n    ldr x0, =0x12345\n' "refused line 2: one number is loaded both as 32 bits and as 64, which clang makes one literal of" "a literal of two sizes"
refused '    msr midr_el1, x0\n' "refused line 1: this is not a system register or operation of the subset" "a write of a register that is only read"
refused 'a:  wfe\na:  wfe\n' "refused line 2: the name is defined twice" "a label defined twice"
refused '    wfe\n    .word 1\n' "refused line 2: this is not a directive of the subset" "a directive that is not of the subset"
# A conditional branch reaches a megabyte: seventeen times 64 KB is further.
{
    echo 'far:'
    i=0
    while [ "$i" -lt 17 ]; do echo '    wfe'; echo '    .balign 0x10000'; i=$((i + 1)); done
    echo '    b far'
    echo '    b.eq far'
} > "$work/far.S"
"$work/check" "$work/far.S" > "$work/far.txt" 2>&1
same "$?" 1 "the status for a branch too far"
same "$(cat "$work/far.txt")" "refused line 37: the label is too far from the instruction" "a branch too far"
end

# The registers that are only read and only written are the lists `Cpu` has (compiler/bare.cho).
begin the_registers_of_one_direction_are_those_of_cpu
for list in read_only write_only; do
    same "$(grep -A1 "^fn $list()" compiler/native/a64asmops.cho | grep -c 'return " ')" 1 "the list $list of the assembler"
    same "$(grep -A1 "^fn $list()" compiler/native/a64asmops.cho | grep 'return " ')" "$(grep -A1 "^fn $list()" compiler/bare.cho | grep 'return " ')" "the list $list"
done
end

# No source traps: every beginning of each source, and bytes of it replaced one at a time, is accepted or refused, and none for want of room in a table of
# the size `room` answers. A trap would end the check with a signal and no line; the line it prints has every run in it.
begin a_damaged_source_is_accepted_or_refused_and_never_traps
for source in forms virt; do
    "$work/check" --damage "tests/native/asm/$source.S" > "$work/damage.txt" 2>&1
    same "$?" 0 "the status for $source.S damaged"
    # beginnings B replacements R refused X accepted Y without room 0
    set -- $(cat "$work/damage.txt")
    same "$1 $3 $5 $7 $9 ${10} ${11}" "beginnings replacements refused accepted without room 0" "what is said of $source.S damaged"
    same "$2" "$(($(wc -c < "tests/native/asm/$source.S") + 1))" "the beginnings of $source.S"
    same "$(($6 + $8))" "$(($2 + $4))" "the runs of $source.S that were refused or accepted"
    [ "${4:-0}" -ge 4000 ] || fail "$source.S had ${4:-0} bytes replaced, fewer than 4000"
    [ "${6:-0}" -ge 1000 ] || fail "only ${6:-0} of the damaged $source.S were refused"
done
end

# The check can fail: one number of one instruction changed, and what the assembler makes is no longer what clang made.
begin a_changed_source_is_not_what_clang_made
sed 's/str     xzr, \[x0\], #8/str     xzr, [x0], #16/' tests/native/asm/virt.S > "$work/changed.S"
same "$(cmp -s tests/native/asm/virt.S "$work/changed.S"; echo $?)" 1 "the changed source differs"
"$work/check" "$work/changed.S" > "$work/changed.txt" 2>&1
same "$?" 0 "the status for the changed source"
same "$(cmp -s "$work/changed.txt" tests/native/asm/virt.expected; echo $?)" 1 "what it assembles to differs"
same "$(diff "$work/changed.txt" tests/native/asm/virt.expected | grep -c '^<')" 1 "the lines that differ"
"$work/check" "$work/missing.S" > /dev/null 2>&1
same "$?" 2 "the status for a file that is not there"
end
