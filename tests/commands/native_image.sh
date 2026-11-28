# native_image.sh -- the script of tests/commands/script_tests.cho for the image of a program with no system (docs/native-backend.md section 16, epic #840 N7).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# The probe for no system (tests/probes/freestanding.cho), its boot page (tests/native/asm/virt.S) and a layout (virt.layout) are made into the file that
# boots, by the compiler alone: every build below runs with a PATH that has no program in it, so no clang, no linker and no objcopy can have been started.
# Where there is a qemu-system-aarch64 the image is booted, and must print the line the probe's foreign function prints for the number it is given.
cancho=$1
work=$2
. tests/commands/script_lib.sh
layout=tests/native/asm/virt.layout
mkdir -p "$work/empty"
# The layout with the line $2 added after its line $1 (awk: a `sed` does not everywhere take a new line in what it puts).
after() { awk -v at="$1" -v more="$2" '{ print } $0 == at { print more }' $layout; }
build() { PATH="$work/empty" "$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --backend native "$@"; }

begin the_compiler_alone_writes_the_image_and_the_executable_that_holds_it
build --layout $layout -o "$work/probe.img" > "$work/img.txt" 2>&1
same "$?" 0 "the status for the flat image ($(head -c 200 "$work/img.txt"))"
build --layout $layout --emit image -o "$work/probe2.img" > /dev/null 2>&1
same "$(cmp -s "$work/probe.img" "$work/probe2.img"; echo $?)" 0 "--emit image is what --layout alone means"
build --layout $layout --emit exe -o "$work/probe.elf" > "$work/elf.txt" 2>&1
same "$?" 0 "the status for the executable ($(head -c 200 "$work/elf.txt"))"
same "$(od -An -tx1 -N4 "$work/probe.elf" | tr -d ' \n')" "7f454c46" "the executable's magic"
same "$(od -An -tx1 -j16 -N4 "$work/probe.elf" | tr -d ' \n')" "0200b700" "an executable for AArch64"
same "$(od -An -tx1 -j24 -N8 "$work/probe.elf" | tr -d ' \n')" "0000084000000000" "where it starts (0x40080000, the low byte first)"
# The executable's one loaded part is the flat image, byte for byte, 4096 bytes into the file.
tail -c +4097 "$work/probe.elf" > "$work/part.img"
same "$(cmp -s "$work/part.img" "$work/probe.img"; echo $?)" 0 "the executable's part against the flat image"
# The image starts with the boot page's first instruction, and holds no more than it reaches: the object of the same program, which holds all of the
# module, is several times its size.
build --emit obj -o "$work/probe.o" > /dev/null 2>&1
size=$(wc -c < "$work/probe.img" | tr -d ' ')
whole=$(wc -c < "$work/probe.o" | tr -d ' ')
[ "$size" -gt 4096 ] || fail "the image is $size bytes"
[ $((size * 3)) -lt "$whole" ] || fail "the image is $size bytes and the object $whole: what is not reached was not dropped"
same "$(ls "$work" | grep -c 'native.o$\|link.log$')" 0 "nothing is left beside the files"
end

begin the_image_boots_and_prints_the_probes_line
if command -v qemu-system-aarch64 > /dev/null 2>&1; then
    for file in probe.img probe.elf; do
        rm -f "$work/serial.txt"
        # The probe ends by waiting for ever: the machine is stopped after the time it needs many times over.
        (qemu-system-aarch64 -M virt -cpu cortex-a72 -display none -nic none -serial "file:$work/serial.txt" -kernel "$work/$file" > /dev/null 2>&1 &
         pid=$!; n=0; while [ $n -lt 100 ] && ! grep -q 'probe' "$work/serial.txt" 2> /dev/null; do sleep 0.1; n=$((n + 1)); done; kill $pid 2> /dev/null; wait $pid 2> /dev/null)
        same "$(cat "$work/serial.txt" 2> /dev/null)" "probe be" "what $file prints on the serial line"
    done
else
    echo "native_image: no qemu-system-aarch64 here: the image was made and not booted" >&2
fi
end

# What cannot be placed is refused by the rule, with the name or the number, and nothing is written.
begin an_image_that_cannot_be_made_is_refused_with_the_reason
mkdir "$work/out"
try() {
    # $1 the name of the case, $2 what the refusal must say; the layout is on the standard input, beside a copy of the boot page.
    mkdir -p "$work/$1"
    cp tests/native/asm/virt.S "$work/$1/virt.S"
    cat > "$work/$1/case.layout"
    build --layout "$work/$1/case.layout" -o "$work/out/$1.img" > "$work/$1.txt" 2>&1
    same "$?" 1 "the status for $1"
    same "$(head -1 "$work/$1.txt" | grep -c 'error: The backend that was asked for cannot build this program')" 1 "the rule for $1"
    same "$(sed -n 2p "$work/$1.txt" | sed 's/^ *//')" "$2" "what is said for $1"
}
grep -v '__stack_top' $layout | try undefined 'the image: a name the image uses is defined by nothing: not by the program, the boot page or a `symbol` of the layout: `__stack_top`'
after 'symbol __bss_end' 'symbol main' | try twice 'the image: a name is defined twice, among the program, the boot page and the layout'"'"'s `symbol` lines: `main`'
# (By how much depends on the code, so the number is not held to one value: it is a number, and not zero.)
after 'text' 'limit 0x80100' > "$work/limit.layout.txt"
mkdir -p "$work/limit" && cp tests/native/asm/virt.S "$work/limit/virt.S" && cp "$work/limit.layout.txt" "$work/limit/case.layout"
build --layout "$work/limit/case.layout" -o "$work/out/limit.img" > "$work/limit.txt" 2>&1
same "$?" 1 "the status for limit"
same "$(sed -n 2p "$work/limit.txt" | sed 's/^ *//; s/([1-9][0-9]*)$/(N)/')" 'the image: the image has grown past a `limit` of the layout, by this many bytes (N)' "what is said for limit"
grep -v '^rodata$' $layout | try unplaced 'the image: the image has something the layout has no line for (1 the boot page, 2 `text`, 3 `rodata`, 4 `data`, 5 `bss`) (3)'
sed 's/^entry _start$/entry main/' $layout | try entry 'the image: a flat image starts at its first byte, and `entry` names another place: `main`'
grep -v '^entry' $layout | try noentry 'the image: the layout has no `entry` line'
after 'at 0x80000' 'at 0x1000' | try back 'the image: an `at` of the layout is before the place: the place only goes forward (520192)'
sed 's/^align 4096$/align 4000/' $layout | try layout 'line 6 of the layout: `align` takes a power of two'
sed 's/^boot virt.S$/boot none.S/' $layout > "$work/missing.layout"
build --layout "$work/missing.layout" -o "$work/out/missing.img" > "$work/missing.txt" 2>&1
same "$?" 3 "the status when the boot page cannot be read"
mkdir -p "$work/page"
sed 's/^    wfe$/    wfx/' tests/native/asm/virt.S > "$work/page/virt.S"
cp $layout "$work/page/case.layout"
build --layout "$work/page/case.layout" -o "$work/out/page.img" > "$work/page.txt" 2>&1
same "$?" 1 "the status for a boot page with an instruction the assembler has not"
same "$(sed -n 2p "$work/page.txt" | sed 's/^ *//; s/: .*//')" "line $(grep -n '^    wfe$' tests/native/asm/virt.S | cut -d: -f1) of the boot page" "where it is refused"
same "$(ls "$work/out" | wc -l | tr -d ' ')" 0 "what was written"
end

# The flag goes with a program for no system and the native backend, and with nothing else.
begin the_layout_flag_is_refused_where_it_means_nothing
"$cancho" build tests/probes/freestanding.cho --std --layout $layout -o "$work/out/a" > "$work/usage1.txt" 2>&1
same "$?" 2 "--layout for this machine"
"$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --layout $layout -o "$work/out/a" > /dev/null 2>&1
same "$?" 2 "--layout with the LLVM backend"
"$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --backend native --layout $layout --emit obj -o "$work/out/a" > /dev/null 2>&1
same "$?" 2 "--layout with --emit obj"
"$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --backend native --emit image -o "$work/out/a" > /dev/null 2>&1
same "$?" 2 "--emit image with no --layout"
"$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --backend native --layout > /dev/null 2>&1
same "$?" 2 "--layout with no file"
same "$(head -1 "$work/usage1.txt" | cut -c1-40)" '`--layout` places a program for no syste' "what is said"
same "$(ls "$work/out" | wc -l | tr -d ' ')" 0 "what was written"
end
