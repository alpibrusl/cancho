# native_reader.sh -- the script of tests/commands/script_tests.cho for the native backend's reader, on what the compiler really writes (docs/native-backend.md
# section 10, epic #840 N1). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# `--backend roundtrip` is the module read by the reader and written back by its printer, then given to clang as any module is. Here: the compiler's own module
# comes back with every form counted the same (scripts/ll-census.cho); smaller modules come back as text clang makes the same object of; programs built so run
# as they did; and `--backend native`, the reader with vectors refused, names the line. tests/native/reader_test.cho is the reader's own suite, with no clang.
cancho=$1
work=$2
. tests/commands/script_lib.sh
files=$(sed 's|^|compiler/|' compiler/cancho.files | tr '\n' ' ')
clang=$(command -v clang || echo /usr/bin/clang)

begin the_compilers_module_comes_back_with_every_form_counted_the_same
"$cancho" build scripts/ll-census.cho --std -o "$work/census" > "$work/census.txt" 2>&1 || fail "building the census said: $(head -c 300 "$work/census.txt")"
# shellcheck disable=SC2086
"$cancho" build $files --std --emit ll -o "$work/compiler.ll" > "$work/plain.txt" 2>&1 || fail "the compiler as a module said: $(head -c 300 "$work/plain.txt")"
# shellcheck disable=SC2086
"$cancho" build $files --std --emit ll --backend roundtrip -o "$work/compiler.rt.ll" > "$work/rt.txt" 2>&1 || fail "the compiler's module through the reader said: $(head -c 300 "$work/rt.txt")"
"$work/census" "$work/compiler.ll" > "$work/plain.census" 2> /dev/null
"$work/census" "$work/compiler.rt.ll" > "$work/rt.census" 2> /dev/null
same "$(cmp -s "$work/plain.census" "$work/rt.census"; echo $?)" 0 "the census of the module before and after"
[ "$(wc -l < "$work/plain.census" | tr -d ' ')" -gt 100 ] || fail "the census is not of a module: $(wc -l < "$work/plain.census") lines"
same "$(grep -c '^define' "$work/compiler.rt.ll")" "$(grep -c '^define' "$work/compiler.ll")" "the functions"
# what is written back is not the bytes that were read: the comments are gone
case "$(grep -c '^;' "$work/compiler.ll")" in 0) fail "the module the compiler wrote has no comment line: the next check shows nothing" ;; esac
same "$(grep -c '^ *;' "$work/compiler.rt.ll")" 0 "comment lines in what the printer wrote"
end

# object SOURCE NAME [FLAGS]: SOURCE built to a module plainly and through the reader, each made an object by clang; fails unless the two objects are the same bytes.
object() {
    _src=$1; _name=$2; shift 2
    # shellcheck disable=SC2068
    "$cancho" build "$_src" --std --emit ll $@ -o "$work/$_name.ll" > "$work/$_name.txt" 2>&1 || { fail "$_name as a module said: $(head -c 200 "$work/$_name.txt")"; return; }
    mkdir -p "$work/rt"
    # shellcheck disable=SC2068
    "$cancho" build "$_src" --std --emit ll $@ --backend roundtrip -o "$work/rt/$_name.ll" > "$work/$_name.rt.txt" 2>&1 || { fail "$_name through the reader said: $(head -c 200 "$work/$_name.rt.txt")"; return; }
    _target=
    case " $* " in *" aarch64-none-elf "*) _target=--target=aarch64-none-elf ;; esac
    # shellcheck disable=SC2086
    "$clang" -O1 -Wno-override-module $_target -c "$work/$_name.ll" -o "$work/$_name.o" > "$work/$_name.clang.txt" 2>&1 || { fail "clang on $_name said: $(head -c 200 "$work/$_name.clang.txt")"; return; }
    # shellcheck disable=SC2086
    (cd "$work/rt" && "$clang" -O1 -Wno-override-module $_target -c "$_name.ll" -o "$_name.o") > "$work/$_name.clang.txt" 2>&1 || { fail "clang on $_name as the printer wrote it said: $(head -c 300 "$work/$_name.clang.txt")"; return; }
    same "$(cmp -s "$work/$_name.o" "$work/rt/$_name.o"; echo $?)" 0 "the object of $_name before and after"
}

begin clang_makes_the_same_object_of_what_the_printer_wrote
object tests/probes/freestanding.cho bare --target aarch64-none-elf
object tests/probes/cpu.cho cpu --target aarch64-none-elf
object tests/probes/mmio.cho mmio --target aarch64-none-elf
object tests/probes/bare_heap.cho heap --target aarch64-none-elf
object tests/probes/freestanding.cho hosted
object examples/hello.cho hello
object examples/newton.cho newton
end

begin a_program_built_through_the_reader_runs_as_it_did
"$cancho" run examples/hello.cho --std > "$work/plain.out" 2>&1
"$cancho" run examples/hello.cho --std --backend roundtrip > "$work/rt.out" 2>&1
same "$?" 0 "the status of run"
same "$(cat "$work/rt.out")" "$(cat "$work/plain.out")" "what it printed"
case "$(wc -c < "$work/plain.out" | tr -d ' ')" in 0) fail "the program printed nothing: the comparison shows nothing" ;; esac
cat > "$work/sums_test.cho" << 'EOF'
fn test_adds() -> [] int {
    return 2 + 2 - 4;
}

fn test_fails() -> [] int {
    return 3;
}
EOF
"$cancho" test "$work/sums_test.cho" --std --backend roundtrip > "$work/test.out" 2>&1
same "$?" 4 "the status of test, one of two failing"
same "$(grep -c '^test result: FAILED. 1 passed; 1 failed' "$work/test.out")" 1 "what test says"
end

# `--backend native` reads with vectors refused. The front end, asked for that backend, writes none (`compiler/nativert.cho`: the byte search of std.bytes is a
# function of the module's own), so a program that searches bytes builds; a module that has one all the same is refused at its line (tests/commands/native_gen.sh).
begin a_program_that_searches_bytes_has_no_vector_for_the_native_backend
cat > "$work/finds.cho" << 'EOF'
import std.bytes;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    return bytes.find("hello, world", "world");
}
EOF
"$cancho" build "$work/finds.cho" --std --emit ll --host linux-aarch64 -o "$work/finds.ll" > /dev/null 2>&1
case "$(grep -c '<16 x i8>' "$work/finds.ll")" in 0) fail "the module for the LLVM backend has no vector: the next check shows nothing" ;; esac
"$cancho" build "$work/finds.cho" --std --emit ll --backend native --host linux-aarch64 -o "$work/finds.native.ll" > "$work/finds.txt" 2>&1
same "$?" 0 "the status for the native backend's module"
same "$(grep -c '<[0-9]* x ' "$work/finds.native.ll")" 0 "vectors in it"
same "$(grep -c '^define internal i64 @cancho_byte_mask64(ptr %p, i8 %b) {$' "$work/finds.native.ll")" 1 "the function that takes their place"
"$cancho" build "$work/finds.cho" --std --backend roundtrip -o "$work/finds" > /dev/null 2>&1 && "$work/finds"
same "$?" 7 "the same program through the round trip, which reads vectors, run"
end
