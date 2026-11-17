# wide_multiply.sh -- the script of tests/commands/script_tests.cho for `conformance/wide_multiply.rs` (deleted): `mul_wide`, `add_carry` and `sub_borrow` (docs/wide-multiply.md section 6)
# through tests/programs/wide_multiply_driver.cho, against Rust's u128, now recorded: tests/vectors/wide_multiply.txt (2,048 cases) and .expected. $1 is the compiler, $2 a scratch directory;
# run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh
. tests/commands/vector_lib.sh

build tests/programs/wide_multiply_driver.cho driver

begin "the wide builtins agree with u128"
# Every pair of 15 operand edges with each kind of carry-in (M, 4 A and 4 S lines per pair: 2,025), a ripple of carries through eight words each way (16), four known answers, and three
# seeds of two million random cases each, whose answers the driver folds into one checksum.
same "$(lines tests/vectors/wide_multiply.txt)" 2048 "the cases recorded (a count, so the file cannot shrink silently)"
agrees "$work/driver" tests/vectors/wide_multiply.txt tests/vectors/wide_multiply.expected "the wide builtins"
end

begin "on wasm32 the wide builtins link and give the native checksum"
# Needs the toolchain docs/wasm.md names; where it is not configured (CI) nothing runs, as the ignored Rust test did not. The checksum is the native driver's for 300,000 cases of seed
# 9e3779b97f4a7c15, which the oracle agreed with when the 2,000,000-case checksum above was recorded.
if [ -n "$CLANG" ] && [ -n "$WASM_LD" ] && [ -n "$WASI_SYSROOT" ] && [ -n "$WASMTIME" ]; then
    if "$cancho" build tests/programs/wide_multiply_driver.cho --std --target wasm32-wasip1 -o "$work/driver.wasm" > "$work/wasm-build.txt" 2>&1; then
        got=$(printf 'R 9e3779b97f4a7c15 300000\nM ffffffffffffffff ffffffffffffffff\n' | "$WASMTIME" "$work/driver.wasm")
        same "$got" "b55222b094bdf512
fffffffffffffffe 0000000000000001" "the wasm32 answers"
    else
        fail "the wasm32 build said: $(head -c 300 "$work/wasm-build.txt")"
    fi
fi
end
