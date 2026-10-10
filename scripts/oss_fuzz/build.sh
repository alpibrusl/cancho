#!/bin/bash -eu
# OSS-Fuzz's build script for the cancho fuzzing harnesses
# (scripts/oss_fuzz/project.yaml explains the submission; this file and
# that one are copied into a `projects/cancho` directory of google/oss-fuzz).
#
# Builds the Rust compiler with the same pinned toolchain the repo's CI
# does, then one libFuzzer binary a harness: the harness program built
# as an object, its `main` renamed, linked against the shim whose
# `LLVMFuzzerTestOneInput` feeds the input over stdin. The harnesses
# and their committed corpora are `tests/programs/fuzz_<name>.cho` and
# `tests/vectors/fuzz/<name>/` (`docs/tls-assurance.md` \u00a73): the DER
# reader, the chain builder, the ClientHello, the record layer's
# messages, the client's flight, the server's hello, the engine's
# serve path.

cd "$SRC/cancho"

# The Rust toolchain the repo pins (rust-toolchain.toml), and a clang
# for the LLVM backend, which OSS-Fuzz's image provides as $CC/$CXX.
rustup default "$(grep channel rust-toolchain.toml | sed 's/channel = "//;s/"//')"

cargo build --release -p cancho
LEX="$SRC/cancho/target/release/cancho"

cp "$SRC/cancho/scripts/oss_fuzz/shim.c" "$OUT/"

for harness in der chain messages client flight hello server; do
  name="fuzz_$harness"
  work="$(mktemp -d)"
  "$LEX" build --std --emit obj "tests/programs/$name.cho" \
    packages/tls/{tls,record,message,slot,client12,client,hello,identity,server}.cho \
    packages/x509/{verify,names,x509,key}.cho \
    -o "$work/$name.o"

  # A cancho program's `main` reads standard input; the shim redirects
  # the libFuzzer input there and calls it under its renamed symbol.
  objcopy --redefine-sym main=cancho_fuzz_main "$work/$name.o"

  $CC $CFLAGS -c "$SRC/cancho/scripts/oss_fuzz/shim.c" -o "$work/shim.o" \
    -DCANCHO_FUZZ_MAIN=cancho_fuzz_main

  $CXX $CXXFLAGS "$work/shim.o" "$work/$name.o" -o "$OUT/${name}_fuzzer" \
    $LIB_FUZZING_ENGINE

  # The committed corpus seeds the fuzzer on OSS-Fuzz's side.
  cp -r "tests/vectors/fuzz/$harness" "$OUT/${name}_seed_corpus.zip" 2>/dev/null \
    || zip -r "$OUT/${name}_seed_corpus.zip" "tests/vectors/fuzz/$harness"

  rm -rf "$work"
done
