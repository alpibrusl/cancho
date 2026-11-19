# declared_intrinsics.sh -- every `@llvm.*` intrinsic the IR calls is declared in it. Apple's clang declares an undeclared intrinsic itself and LLVM on Linux does not, so a missing
# `declare` (the unsigned add and subtract with overflow, called by `add_carry` and `sub_borrow`) built on macOS and failed on Linux (the fuzz build of PR #800). The IR clang is
# handed is captured through `--clang`. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh

cat > "$work/capture.sh" <<W
#!/bin/sh
for a in "\$@"; do case "\$a" in *.ll) cp "\$a" "$work/seen.ll";; esac; done
exec ${CLANG:-clang} "\$@"
W
chmod +x "$work/capture.sh"

begin every_intrinsic_the_ir_calls_is_declared
if ! "$cancho" build tests/programs/wide_multiply_driver.cho --std --clang "$work/capture.sh" -o "$work/wide" > "$work/build.txt" 2>&1; then
    fail "building said: $(head -c 300 "$work/build.txt")"
else
    grep -o 'call [^@]*@llvm\.[a-z0-9.]*' "$work/seen.ll" | sed 's/.*@//' | sort -u > "$work/called.txt"
    grep -o '^declare [^@]*@llvm\.[a-z0-9.]*' "$work/seen.ll" | sed 's/.*@//' | sort -u > "$work/declared.txt"
    same "$(comm -23 "$work/called.txt" "$work/declared.txt" | tr '\n' ' ')" "" "intrinsics called and not declared"
    same "$(grep -c 'llvm.uadd.with.overflow' "$work/called.txt")" 1 "the program calls the unsigned add with overflow (else this checks nothing)"
fi
end
