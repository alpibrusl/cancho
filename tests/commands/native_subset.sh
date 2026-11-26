# native_subset.sh -- the script of tests/commands/script_tests.cho for the native backend's contract and its flag (docs/native-backend.md, epic #840 N0).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# The contract is tests/native/subset.txt: the forms of LLVM text a module of the compiler may hold, by name. scripts/ll-census.cho counts the forms of a module, and
# here the compiler's own module (for this machine) and the probe for no system must hold none that is outside the file. The three files of hardware ciphers are
# counted apart: the native backend leaves their functions out.
cancho=$1
work=$2
. tests/commands/script_lib.sh
apart="--apart compiler/runtime/crypto_x86_64.ll --apart compiler/runtime/crypto_aarch64_darwin.ll --apart compiler/runtime/crypto_aarch64_linux.ll"
files=$(sed 's|^|compiler/|' compiler/cancho.files | tr '\n' ' ')

begin what_the_compiler_writes_is_inside_the_subset
if ! "$cancho" build scripts/ll-census.cho --std -o "$work/census" > "$work/census.txt" 2>&1; then
    fail "building the census said: $(head -c 300 "$work/census.txt")"
fi
# shellcheck disable=SC2086
if ! "$cancho" build $files --std --emit ll -o "$work/compiler.ll" > "$work/compiler.txt" 2>&1; then
    fail "the compiler as a module said: $(head -c 300 "$work/compiler.txt")"
fi
if ! "$cancho" build tests/probes/freestanding.cho --std --target aarch64-none-elf --emit ll -o "$work/bare.ll" > "$work/bare.txt" 2>&1; then
    fail "the probe for no system said: $(head -c 300 "$work/bare.txt")"
fi
# shellcheck disable=SC2086
"$work/census" "$work/compiler.ll" "$work/bare.ll" $apart --subset tests/native/subset.txt > "$work/outside.txt" 2>&1
same "$?" 0 "the status of the census against the subset"
same "$(head -3 "$work/outside.txt")" "" "the forms outside the subset"
end

# The census is read: it finds the instructions the compiler's module is mostly made of, and says the same thing twice.
begin the_census_counts_and_is_the_same_each_time
# shellcheck disable=SC2086
"$work/census" "$work/compiler.ll" $apart > "$work/one.txt" 2> /dev/null
# shellcheck disable=SC2086
"$work/census" "$work/compiler.ll" $apart > "$work/two.txt" 2> /dev/null
same "$(cmp -s "$work/one.txt" "$work/two.txt"; echo $?)" 0 "two runs"
same "$(LC_ALL=C sort -c "$work/one.txt" 2>&1 | head -1)" "" "the lines are sorted by their bytes"
for form in "instruction load" "instruction call" "instruction br" "instruction store" "instruction alloca" "instruction extractvalue" "intrinsic llvm.sadd.with.overflow.i64" "type i64" "type ptr" "top define"; do
    n=$(grep "^$form	" "$work/one.txt" | cut -f2)
    [ "${n:-0}" -gt 100 ] || fail "the census of the compiler counts $form ${n:-0} times"
done
same "$(grep '^top define	\|^apart top define	' "$work/one.txt" | awk -F'\t' '{ s += $2 } END { print s }')" "$(grep -c '^define' "$work/compiler.ll")" "a function a define line, the ciphers' apart"
end

# The test can fail: one call of an intrinsic that is not in the subset, written into the probe's module, is named. So is a type, and a word.
begin a_form_outside_the_subset_is_named
sed 's/^entry:$/entry:\
  %outside = call i64 @llvm.bitreverse.i64(i64 0)/' "$work/bare.ll" > "$work/widened.ll"
case "$(grep -c 'llvm.bitreverse.i64' "$work/widened.ll")" in
    0) fail "the call was not written in" ;;
esac
"$work/census" "$work/widened.ll" --subset tests/native/subset.txt > "$work/widened.txt" 2>&1
same "$?" 1 "the status with the intrinsic"
same "$(cat "$work/widened.txt")" "not in the subset: intrinsic llvm.bitreverse.i64" "what it says"
sed 's/^entry:$/entry:\
  %outside = urem i64 7, 2\
  %wider = fpext half 0xH0000 to double\
  %taken = add nuw i64 1, 2/' "$work/bare.ll" > "$work/words.ll"
"$work/census" "$work/words.ll" --subset tests/native/subset.txt > "$work/words.txt" 2>&1
same "$?" 1 "the status with an instruction, a type and a flag"
same "$(tr '\n' ';' < "$work/words.txt")" "not in the subset: instruction urem;not in the subset: type half;not in the subset: word add nuw;" "each is named"
# and a vector in a function that is not one of the hardware ciphers is not excused by them
sed 's/^entry:$/entry:\
  %lanes = extractelement <2 x i64> zeroinitializer, i64 0/' "$work/bare.ll" > "$work/lanes.ll"
# shellcheck disable=SC2086
"$work/census" "$work/lanes.ll" $apart --subset tests/native/subset.txt > "$work/lanes.txt" 2>&1
same "$(head -1 "$work/lanes.txt")" "not in the subset: instruction extractelement" "a vector instruction of the program's own"
"$work/census" "$work/missing.ll" > /dev/null 2>&1
same "$?" 2 "the status for a file that is not there"
end

# The flag (docs/native-backend.md section 8). `native` is refused by a rule, for a program the front end accepts, and nothing is written. (What the reader
# behind it reads and refuses is native_reader.sh's.)
begin the_native_backend_is_refused_by_a_rule_and_writes_nothing
cat > "$work/small.cho" << 'EOF'
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    return 0;
}
EOF
mkdir "$work/out"
"$cancho" build "$work/small.cho" --backend native -o "$work/out/small" > "$work/native.txt" 2>&1
same "$?" 1 "the status of build"
same "$(head -1 "$work/native.txt" | sed 's/^.*small\.cho: error: \(The backend that was asked for cannot build this program\).*/\1/')" "The backend that was asked for cannot build this program" "what build says"
same "$(sed -n 2p "$work/native.txt")" '    the native backend reads this program'"'"'s module and has no code generator yet: nothing was written, and `--backend llvm` builds this program' "and why, today"
same "$(ls "$work/out" | wc -l | tr -d ' ')" 0 "what build wrote"
"$cancho" build "$work/small.cho" --backend native --emit ll -o "$work/out/small.ll" > /dev/null 2>&1
same "$?" 1 "the status of build --emit ll"
"$cancho" build "$work/small.cho" --backend native --target aarch64-none-elf --emit obj -o "$work/out/small.o" > /dev/null 2>&1
same "$?" 1 "the status of build for no system"
same "$(ls "$work/out" | wc -l | tr -d ' ')" 0 "what they wrote"
"$cancho" check "$work/small.cho" --backend native --output json > "$work/native.json" 2> /dev/null
same "$?" 1 "the status of check"
same "$(grep -c '"rule": "unsupported-by-backend"' "$work/native.json")" 1 "check says it with the rule's tag"
same "$(grep -c '"rule":' "$work/native.json")" 1 "and with no other"
python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$work/native.json" 2> /dev/null || fail "the answer of check is not JSON"
"$cancho" run "$work/small.cho" --backend native > /dev/null 2>&1
same "$?" 1 "the status of run"
printf 'fn test_nothing() -> [] int {\n    return 0;\n}\n' > "$work/small_test.cho"
"$cancho" test "$work/small_test.cho" --backend native > "$work/test.txt" 2>&1
same "$?" 1 "the status of test"
same "$(grep -c 'The backend that was asked for cannot build this program' "$work/test.txt")" 1 "what test says"
# A program the front end refuses is refused for its own reason: the backend is not reached.
printf 'fn main() -> [] int { return true; }\n' > "$work/wrong.cho"
"$cancho" check "$work/wrong.cho" --backend native --output json 2> /dev/null | grep -c '"rule": "type-mismatch"' > "$work/wrong.txt"
same "$(cat "$work/wrong.txt")" 1 "a program that is wrong is refused by its own rule"
end

begin the_llvm_backend_named_is_the_llvm_backend
"$cancho" build "$work/small.cho" --emit ll -o "$work/plain.ll" > /dev/null 2>&1
"$cancho" build "$work/small.cho" --backend llvm --emit ll -o "$work/named.ll" > /dev/null 2>&1
same "$?" 0 "the status with --backend llvm"
same "$(cmp -s "$work/plain.ll" "$work/named.ll"; echo $?)" 0 "the module with and without the flag"
"$cancho" build "$work/small.cho" --backend llvm -o "$work/small" > /dev/null 2>&1 && "$work/small"
same "$?" 0 "the program it builds, run"
"$cancho" build "$work/small.cho" --backend gcc -o "$work/small2" > "$work/gcc.txt" 2>&1
same "$?" 2 "the status for a backend there is not"
same "$(head -1 "$work/gcc.txt")" '`--backend` needs llvm or native, and is of `check`, `build`, `run` and `test`' "what it says"
"$cancho" ids "$work/small.cho" --backend native > /dev/null 2>&1
same "$?" 2 "a command that builds nothing takes no backend"
end
