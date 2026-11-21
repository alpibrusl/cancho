# bootstrap.sh -- the script of tests/commands/script_tests.cho for the four tests of conformance/selfhost/bootstrap.rs (deleted; docs/gate.md, G4 wave 4): the compiler written in cancho compiles itself
# to a fixed point, the builder builds itself with no help, the file list of the standalone build is the import closure of the command line, and the standalone build reproduces itself.
# $1 is the compiler (it builds the first compiler and the first builder, and is the stage 0 of `scripts/build-standalone.sh`), $2 a scratch directory; run from the root of the repository.
# There was no oracle but the compiler itself: the Rust tests compared the compiler in cancho built by Rust with the compiler in cancho built by itself, and here the first one is built by `$1`.
# Needs `clang` ($CLANG names another). `BOOTSTRAP_REGENERATE=1` writes compiler/cancho.files instead of comparing with it.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
root=$(pwd)
mkdir -p "$work"
clang=${CLANG:-clang}
clang=$(command -v "$clang") || { echo "bootstrap: no clang"; exit 2; }
case $(uname -s) in
    Darwin) target=darwin ;;
    *) if [ "$(uname -m)" = aarch64 ] || [ "$(uname -m)" = arm64 ]; then target="linux aarch64"; else target=linux; fi ;;
esac

# The program the compiler compiles: the `std` modules it imports, then its own, then its root, as `FILE <length>` lines and the files (the stream `driver.cho` reads).
modules="lexcore tables runtime kinds rules ast pass1 foreign builtins signatures types prims scope effects emit consteval instances floats floatcode fold threadcode codes externcode netcode platform wordcode cryptocode ex lookup atoms filecode narrowing allocs binops exprs body listing checker floatprint sink filewrite formatting jsonprint jsonread ingest blake3 identity printing prune testscan layoutrep authwasi version authority satisfytext driver"
own=$root/compiler
# own_source ROOT-MODULE OUT
own_source() {
    {
        printf 'TARGET %s\n' "$target"
        for file in "$root/std/math.cho" "$root/std/bytes.cho" "$root/std/buffer.cho" "$root/std/io.cho"; do
            printf 'FILE %s\n' "$(wc -c < "$file" | tr -d ' ')"
            cat "$file"
        done
        for module in $modules $1; do
            printf 'FILE %s\n' "$(wc -c < "$own/$module.cho" | tr -d ' ')"
            cat "$own/$module.cho"
        done
    } > "$2"
}
# The files a program of `compiler.cho` or `builder.cho` is built from: the root and what it imports of the front end.
files_of() { for module in $modules; do printf '%s ' "$own/$module.cho"; done; }

# 1. The first compiler (built by $1) reads its own source and writes LLVM IR, which clang builds: the second compiler, which no other compiler had a hand in; the second reads the same source and
# writes IR again. The two IR texts are the same, byte for byte.
begin the_compiler_in_cancho_compiles_itself_to_a_fixed_point
"$cancho" build "$own/compiler.cho" $(files_of) --std -o "$work/first" > "$work/first.build" 2>&1 || fail "the first compiler did not build: $(head -c 300 "$work/first.build")"
own_source compiler "$work/compiler.stream"
"$work/first" < "$work/compiler.stream" > "$work/ir1.ll" 2> "$work/ir1.err"
case $(head -c 3 "$work/ir1.ll") in ERR*) fail "the first compiler should write code for its own source, but answered: $(head -c 300 "$work/ir1.ll")" ;; esac
# Not an empty module: the compiler's own functions are most of 600, and what is left out is `std.math`.
defined=$(grep -c '^define ' "$work/ir1.ll")
[ "$defined" -ge 600 ] || fail "only $defined functions were written"
"$clang" -O1 -Wno-override-module "$work/ir1.ll" -o "$work/second" > "$work/second.build" 2>&1 || fail "clang should build the compiler the first one wrote, but said: $(head -c 300 "$work/second.build")"
"$work/second" < "$work/compiler.stream" > "$work/ir2.ll" 2> "$work/ir2.err"
cmp -s "$work/ir1.ll" "$work/ir2.ll" || fail "the compiler built by itself writes different code from the one built by $(basename "$cancho"): $(wc -c < "$work/ir1.ll" | tr -d ' ') and $(wc -c < "$work/ir2.ll" | tr -d ' ') bytes, first difference $(cmp "$work/ir1.ll" "$work/ir2.ll" 2>&1 | head -1)"
end
rm -f "$work/ir1.ll" "$work/ir2.ll" "$work/first" "$work/second"

# 2. The same, with no help but a clang to be run: the builder (`builder.cho`) reads a program, writes its IR to a file and starts clang on it through its `Exec`. Built by $1 it is the first builder; it
# builds the second from its own source, and the second builds the third. All three write the same IR.
begin the_builder_in_cancho_builds_itself_with_no_help
"$cancho" build "$own/builder.cho" $(files_of) --std -o "$work/b1" > "$work/b1.build" 2>&1 || fail "the first builder did not build: $(head -c 300 "$work/b1.build")"
own_source builder "$work/builder.stream"
"$work/b1" "$work/b2" "$clang" < "$work/builder.stream" > "$work/b1.out" 2>&1 || fail "the builder should build its source, but said: $(head -c 300 "$work/b1.out")"
"$work/b2" "$work/b3" "$clang" < "$work/builder.stream" > "$work/b2.out" 2>&1 || fail "the second builder should build its source, but said: $(head -c 300 "$work/b2.out")"
size=$(wc -c < "$work/b2.ll" | tr -d ' ')
[ "$size" -gt 100000 ] || fail "the IR is only $size bytes"
cmp -s "$work/b2.ll" "$work/b3.ll" || fail "the builder built by itself writes different code from the first: $size and $(wc -c < "$work/b3.ll" | tr -d ' ') bytes"
# And what it builds runs: a program of its own.
program='fn main(world: World) -> [] int { release(world); return 42; }'
{ printf 'TARGET %s\n' "$target"; printf 'FILE %s\n' "${#program}"; printf '%s' "$program"; } > "$work/made.stream"
"$work/b3" "$work/made" "$clang" < "$work/made.stream" > /dev/null 2>&1 || fail "the third builder did not build a program"
"$work/made" > /dev/null 2>&1
same "$?" 42 "what the third builder built exits 42"
end
rm -f "$work/b1" "$work/b2" "$work/b3" "$work/b2.ll" "$work/b3.ll" "$work/made"

# 3. The files of the command line in cancho, `compiler/cancho.files`: `cli.cho` and everything it imports, by the `module` line of each file (a file without one is named by its stem).
# `scripts/build-standalone.sh` reads the list, so it must be the closure: a module added to the command line and not to the list would be built by nobody.
begin the_file_list_of_the_standalone_build_is_the_import_closure_of_cli
(cd "$own" && awk '
    FNR == 1 { file = FILENAME; module[file] = file; sub(/\.cho$/, "", module[file]) }
    /^module .*;$/ && !named[file] { m = $0; sub(/^module /, "", m); sub(/;$/, "", m); module[file] = m; named[file] = 1 }
    /^import / { n = $2; sub(/[; ].*/, "", n); imports[file] = imports[file] " " n }
    END {
        for (f in module) byname[module[f]] = f
        todo[1] = "cli"; count = 1
        while (count > 0) {
            m = todo[count]; count--
            f = byname[m]
            if (f == "" && substr(m, 1, 9) == "selfhost.") f = byname[substr(m, 10)]
            if (f == "" || seen[f]) continue
            seen[f] = 1
            k = split(imports[f], parts, " ")
            for (i = 1; i <= k; i++) { count++; todo[count] = parts[i] }
        }
        for (f in seen) print f
    }' *.cho | LC_ALL=C sort) > "$work/closure.txt"
if [ -n "${BOOTSTRAP_REGENERATE:-}" ]; then cp "$work/closure.txt" "$own/cancho.files"; fi
cmp -s "$work/closure.txt" "$own/cancho.files" || fail "compiler/cancho.files is not the import closure of cli.cho: run this script with BOOTSTRAP_REGENERATE=1 and commit the result ($(diff "$work/closure.txt" "$own/cancho.files" | head -4 | tr '\n' '|'))"
[ "$(wc -l < "$work/closure.txt" | tr -d ' ')" -gt 70 ] || fail "only $(wc -l < "$work/closure.txt" | tr -d ' ') files in the closure"
end
echo "files in the closure of cli: $(wc -l < "$work/closure.txt" | tr -d ' ')"

# 4. `scripts/build-standalone.sh` (`docs/standalone-cancho.md` D5): the command line in cancho, stamped, is built by $1 (stage 1), by itself (stage 2) and by that (stage 3), and stage 2 and stage 3 are
# the same bytes. The binary it leaves reports the stamp, builds a program, and is the second stage's. The gate can fail: with a byte appended to stage 3 the script refuses and leaves no `cancho`; and the good
# build writes stage 3 under another name, which changes nothing (the symbol table of an executable once held the name of the file the module was written to).
begin the_standalone_build_reproduces_itself
rev=0123456789abcdef0123456789abcdef01234567
good=$work/good
rm -rf "$good" "$work/bad"
STANDALONE_STAGE3_NAME=another-name sh scripts/build-standalone.sh --out "$good" --rev "$rev" --cancho0 "$cancho" > "$work/good.out" 2>&1 || fail "the build did not succeed: $(tail -c 400 "$work/good.out")"
built=$good/cancho
if [ -x "$built" ]; then
    said=$("$built" --version)
    # The version is the Rust command line's (`cancho --version` of the Rust binary said `cancho 0.0.0 (rev ..., host ...)`, tests/vectors/standalone_version.txt).
    case $said in
        "cancho $(cat "$root/tests/vectors/standalone_version.txt") (rev $rev, host "*")") ;;
        *) fail "--version said '$said'" ;;
    esac
    # It builds with its own embedded library, with nothing but the system around it, and the program runs.
    printf 'edition 7;\nimport std.math;\nfn main(world: World) -> [] int { release(world); return math.abs(0 - 4); }\n' > "$work/p.cho"
    env -i PATH=/usr/bin:/bin "$built" build "$work/p.cho" --std -o "$work/p" > "$work/p.build" 2>&1 || fail "the binary did not build a program: $(head -c 300 "$work/p.build")"
    "$work/p" > /dev/null 2>&1
    same "$?" 4 "the program the standalone binary built exits 4"
    # The script has compared them: byte for byte, except on macOS, where the linker's UUID, sizes and signature depend on the file's name and the code, symbols and strings are compared instead.
    if [ "$(uname -s)" != Darwin ]; then
        cmp -s "$good/stages/2/cancho" "$good/stages/3/another-name" || fail "stages 2 and 3 differ (stage 3 was written under another name)"
    fi
    cmp -s "$built" "$good/stages/2/cancho" || fail "the result is not stage 2"
fi
# The gate can fail.
STANDALONE_STAGE3_CORRUPT=1 sh scripts/build-standalone.sh --out "$work/bad" --rev "$rev" --cancho0 "$cancho" > "$work/bad.out" 2>&1
same "$?" 1 "with a byte appended to stage 3 the script refuses"
case $(cat "$work/bad.out") in *"does not reproduce itself"*) ;; *) fail "the refusal does not say that the command line does not reproduce itself: $(tail -c 300 "$work/bad.out")" ;; esac
[ ! -e "$work/bad/cancho" ] || fail "a failed build left a cancho"
end
rm -rf "$good" "$work/bad"
