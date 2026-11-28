# native_gen.sh -- the script of tests/commands/script_tests.cho for the first AArch64 code generator (docs/native-backend.md section 12, epic #840 N3).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# tests/native/gen/tricky.ll is a module of the cases that are easy to get wrong (two `phi`s that read each other, the six overflow checks at their edges, a
# result too large for registers, thirteen arguments, frames of five thousand bytes and of two megabytes, integers of 8 to 128 bits, atomics, floating point,
# a `switch`), with every instruction the generator lowers in it. tests/native/gen/tricky.s is what the generator writes for it, as assembly: the very text
# that was assembled, linked and run under qemu, where it printed tests/native/gen/tricky.out, which is what clang's build of the module prints
# (tests/native/gen_run.sh does that, where there is an AArch64 toolchain). The gate has none, so here the generator must write that text again.
cancho=$1
work=$2
. tests/commands/script_lib.sh
native="compiler/native/llmod.cho compiler/native/llread.cho compiler/native/llbody.cho compiler/native/lltop.cho compiler/native/lllayout.cho compiler/native/llout.cho compiler/native/a64.cho compiler/native/a64sys.cho compiler/native/a64frame.cho compiler/native/a64call.cho compiler/native/a64gen.cho compiler/native/a64fn.cho compiler/native/mem2reg.cho"

begin the_generator_writes_the_code_that_was_run
# shellcheck disable=SC2086
if "$cancho" build tests/native/gen_dump.cho $native --std -o "$work/dump" > "$work/build.txt" 2>&1; then
    "$work/dump" tests/native/gen/tricky.ll > "$work/tricky.s" 2> "$work/tricky.txt"
    same "$?" 0 "the status"
    same "$(cat "$work/tricky.txt")" "ok: words 1581 functions 15 relocations 73" "what it says"
    same "$(cmp -s "$work/tricky.s" tests/native/gen/tricky.s; echo $?)" 0 "the assembly against tests/native/gen/tricky.s"
else
    fail "building the dump said: $(head -c 300 "$work/build.txt")"
fi
end

# The variables of a module made values (docs/native-backend.md section 17.2): tests/native/gen/vars.ll is written as the front end writes, every local an
# `alloca` (a swap through a third variable in a loop, a diamond, nested loops, `double` and `float`, `i1`, `i8` and `i16` through a `switch`, an alloca that
# escapes beside one that does not, a pointer variable, a block no jump reaches). tests/native/gen/vars.s is the code with the pass and
# tests/native/gen/vars-no-registers.s without it; both were assembled, linked and run under qemu by tests/native/gen_run.sh, where each printed
# tests/native/gen/vars.printed, which is what clang's build of the module prints. tricky.ll's two allocas are aggregates, so the pass leaves its code as it was.
begin the_variables_made_values_are_the_code_that_was_run
"$work/dump" tests/native/gen/vars.ll registers > "$work/vars.s" 2> "$work/vars.txt"
same "$?" 0 "the status"
same "$(cat "$work/vars.txt")" "ok: words 703 functions 10 relocations 32" "what it says with the pass"
same "$(cmp -s "$work/vars.s" tests/native/gen/vars.s; echo $?)" 0 "the assembly against tests/native/gen/vars.s"
"$work/dump" tests/native/gen/vars.ll > "$work/vars0.s" 2> "$work/vars0.txt"
same "$(cat "$work/vars0.txt")" "ok: words 722 functions 10 relocations 32" "what it says without"
same "$(cmp -s "$work/vars0.s" tests/native/gen/vars-no-registers.s; echo $?)" 0 "the assembly against tests/native/gen/vars-no-registers.s"
"$work/dump" tests/native/gen/tricky.ll registers > "$work/tricky-registers.s" 2> /dev/null
same "$(cmp -s "$work/tricky-registers.s" tests/native/gen/tricky.s; echo $?)" 0 "tricky.ll with the pass against tests/native/gen/tricky.s"
end

# One instruction changed in the module is a change in what is written: the comparison above can fail.
begin a_changed_instruction_is_other_code
sed 's/^  %d = sub i64 %b, %m$/  %d = add i64 %b, %m/' tests/native/gen/tricky.ll > "$work/changed.ll"
same "$(cmp -s tests/native/gen/tricky.ll "$work/changed.ll"; echo $?)" 1 "the module was changed"
"$work/dump" "$work/changed.ll" > "$work/changed.s" 2> /dev/null
same "$(diff "$work/changed.s" tests/native/gen/tricky.s | grep -c '^[<>]')" 2 "one word differs"
end

# Every instruction and every function of LLVM's own the compiler may write (tests/native/subset.txt) is lowered or refused on purpose
# (tests/native/lowering.txt), and every one that is lowered is in the module above, so it was run.
begin every_form_is_lowered_or_refused_and_the_lowered_ones_were_run
grep '^instruction \|^intrinsic ' tests/native/subset.txt | LC_ALL=C sort > "$work/may.txt"
grep '^instruction \|^intrinsic ' tests/native/lowering.txt | sed 's/ lowered$//; s/ refused .*$//' | LC_ALL=C sort > "$work/decided.txt"
same "$(diff "$work/may.txt" "$work/decided.txt" | head -3)" "" "tests/native/subset.txt against tests/native/lowering.txt"
[ "$(wc -l < "$work/may.txt" | tr -d ' ')" -gt 50 ] || fail "the list is short: $(wc -l < "$work/may.txt") forms"
"$cancho" build scripts/ll-census.cho --std -o "$work/census" > /dev/null 2>&1
"$work/census" tests/native/gen/tricky.ll 2> /dev/null | cut -f1 | grep '^instruction \|^intrinsic ' | LC_ALL=C sort > "$work/run.txt"
grep ' lowered$' tests/native/lowering.txt | sed 's/ lowered$//' | LC_ALL=C sort > "$work/lowered.txt"
same "$(LC_ALL=C comm -23 "$work/lowered.txt" "$work/run.txt" | head -3)" "" "lowered forms that are not in the module that was run"
end

# refused NAME WANT: the module in $work/NAME.ll is refused with WANT (the line and the reason).
refused() {
    "$work/dump" "$work/$1.ll" > "$work/$1.s" 2> "$work/$1.txt"
    same "$?" 1 "the status for $1"
    same "$(cat "$work/$1.txt")" "$2" "what is said of $1"
    same "$(wc -c < "$work/$1.s" | tr -d ' ')" 0 "what is written for $1"
}

begin what_the_generator_has_no_code_for_is_refused_with_its_line
printf 'define i64 @f(i64 %%a) {\nentry:\n  %%v = insertelement <2 x i64> zeroinitializer, i64 %%a, i64 0\n  ret i64 %%a\n}\n' > "$work/vector.ll"
refused vector "generate: line 3: a vector, which the native backend has no code for"
printf 'define ptr @f(i64 %%n) {\nentry:\n  %%p = alloca i8, i64 %%n\n  ret ptr %%p\n}\n' > "$work/alloca.ll"
refused alloca "generate: line 3: an \`alloca\` whose size is not known when the program is built"
printf 'define void @f() {\nentry:\n  call void asm sideeffect "ud2", ""()\n  ret void\n}\n' > "$work/asm.ll"
refused asm "generate: line 3: inline assembly the native backend has no instruction for"
printf 'define void @f() {\nentry:\n  %%r = call i64 asm sideeffect "mrs $0, midr_el9", "=r"()\n  ret void\n}\n' > "$work/register.ll"
refused register "generate: line 3: inline assembly the native backend has no instruction for"
printf 'define i128 @f(i128 %%a, i128 %%b) {\nentry:\n  %%c = add i128 %%a, %%b\n  ret i128 %%c\n}\n' > "$work/wide.ll"
refused wide "generate: line 3: an integer wider than 64 bits used in a way the native backend has no code for"
printf 'define i64 @f(i64 %%a) {\nentry:\n  %%c = call i64 @llvm.bitreverse.i64(i64 %%a)\n  ret i64 %%c\n}\n' > "$work/intrinsic.ll"
refused intrinsic "generate: line 3: a function of LLVM's own the native backend has no code for"
printf 'define i64 @f(i64 %%a) {\nentry:\n  br label %%next\nnext:\n  %%p = phi i64 [ %%a, %%other ]\n  ret i64 %%p\nother:\n  br label %%next\n}\n' > "$work/phi.ll"
refused phi "generate: line 3: an instruction whose operands are not of a type the native backend has code for"
printf 'define void @f() {\nentry:\n  call void @llvm.nothing()\n  %%u = frobnicate i64 1\n  ret void\n}\n' > "$work/unread.ll"
refused unread "read: line 4: an instruction the native backend does not read"
end

# The modules the compiler writes for a machine with no system go through whole: the probes of the tests of that target, registers and volatile accesses in them.
begin the_probes_for_no_system_have_code
for probe in freestanding cpu cpu_registers mmio bare_heap; do
    if "$cancho" build tests/probes/$probe.cho --std --target aarch64-none-elf --emit ll -o "$work/$probe.ll" > "$work/$probe.build.txt" 2>&1; then
        "$work/dump" "$work/$probe.ll" > "$work/$probe.s" 2> "$work/$probe.txt"
        same "$?" 0 "the status for $probe"
        case "$(cat "$work/$probe.txt")" in "ok: words "*) ;; *) fail "$probe: $(cat "$work/$probe.txt")" ;; esac
    else
        fail "$probe as a module said: $(head -c 200 "$work/$probe.build.txt")"
    fi
done
# every register `Cpu` accepts is read or written in one of them: 284 instructions that name one
same "$(grep -c '^	\.inst 0xd5[13]' "$work/cpu_registers.s")" 284 "the reads and writes of a system register in cpu_registers"
end
