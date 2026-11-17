# benchmarks.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/benchmarks.rs (docs/gate.md G4, wave 4): the benchmark programs of benches/: every variant of each computes the
# same answer. Not a timing gate (wall-clock in CI is noise; scripts/bench.py is where the numbers come from): a refactor must not quietly make the two halves of a comparison disagree. The C halves are built with `cc`
# and the Rust halves with `rustc` when that is on the path, as the Rust test did. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

# `benches/`: both halves of every pair compute the same answer. The benchmarks are how docs/overflow-cost.md knows what the overflow trap costs, and the measurement is only meaningful while the two halves are the same
# program. Each returns `result - expected`, so a non-zero exit is a wrong answer.
begin every_benchmark_pair_agrees
fresh pairs
pairs=0
for bench in sum sieve scan fib reduce; do
    for half in checked wrapping; do
        r build "benches/${bench}_$half.cho" -o "$d/${bench}_$half"; succeeds "${bench}_$half builds"
        "$d/${bench}_$half" > /dev/null 2>&1
        same "$?" 0 "${bench}_$half computed the right answer (it exits with its error)"
    done
    pairs=$((pairs + 1))
done
# Counted from the directory rather than written down twice: a pair added to `benches/` and forgotten here would never run.
same "$pairs" "$(ls benches/*_checked.cho | wc -l | tr -d ' ')" "every pair in benches/ is covered here"
echo "benchmark pairs run: $pairs"
end

# `benches/guards.c`: all three modes of every kernel compute the same answer (docs/check-cost.md, docs/poison.md). If a check ever *fires*, the number measured is the trap rather than the check. Built with a small `ROUNDS`.
begin every_guard_kernel_agrees_across_its_three_modes
fresh guards
kernels=0
for kernel in 0 1 2 3 4 5 6 7 8 9 10 11; do
    for mode in 0 1 2; do
        cc -O2 -DROUNDS=2 -DKERNEL=$kernel -DMODE=$mode benches/guards.c -o "$d/k${kernel}m$mode" > "$d/cc.txt" 2>&1 || fail "kernel $kernel mode $mode should compile: $(head -c 200 "$d/cc.txt")"
        "$d/k${kernel}m$mode" > "$d/k${kernel}m$mode.out" 2> /dev/null
        [ "$?" = 0 ] || fail "kernel $kernel mode $mode should exit 0; a check that fires means the data trips it"
    done
    cmp -s "$d/k${kernel}m0.out" "$d/k${kernel}m1.out" && cmp -s "$d/k${kernel}m1.out" "$d/k${kernel}m2.out" || fail "kernel $kernel: unchecked, trapping and poison must compute the same thing"
    kernels=$((kernels + 1))
done
# One `noinline run` per kernel.
same "$kernels" "$(grep -c '__attribute__((noinline))' benches/guards.c)" "every kernel in guards.c is covered here"
echo "guard kernels run: $kernels"
end

# `benches/three/`: every build still computes the same answer (docs/against-c-and-rust.md compares cancho to C and Rust on the same algorithm). Each prints a checksum. The C and Rust halves are skipped when their compilers
# are not on the path. The cancho halves always run.
begin the_three_language_benchmarks_agree
fresh three
cc=
for c in cc clang gcc; do if command -v $c > /dev/null 2>&1; then cc=$c; break; fi; done
have_rustc=0; command -v rustc > /dev/null 2>&1 && have_rustc=1
index=0
for group in "mandelbrot -DCHECKED=1" "sieve" "purity -DPROMISED=0"; do
    set -- $group
    stem=$1; shift
    flags=$*
    r build "benches/three/$stem.cho" --std -o "$d/ls$index"; succeeds "$stem.cho builds"
    "$d/ls$index" > "$d/ls$index.out" 2> /dev/null
    [ "$?" = 0 ] || fail "$stem.cho exited badly"
    expected=$(cat "$d/ls$index.out")
    [ -n "$expected" ] || fail "$stem.cho printed no checksum"
    # A benchmark split across a compilation boundary has its other half beside it, named `<stem>_lib.<extension>`.
    if [ -n "$cc" ]; then
        lib=; [ -f "benches/three/${stem}_lib.c" ] && lib="benches/three/${stem}_lib.c"
        # shellcheck disable=SC2086
        $cc -O2 $flags "benches/three/$stem.c" $lib -o "$d/c$index" > "$d/cc.txt" 2>&1 || fail "$stem.c should compile: $(head -c 200 "$d/cc.txt")"
        same "$("$d/c$index" 2> /dev/null)" "$expected" "$stem.c agrees with $stem.cho"
    fi
    if [ "$have_rustc" = 1 ]; then
        link=
        if [ -f "benches/three/${stem}_lib.rs" ]; then
            rustc -O --crate-type=staticlib "benches/three/${stem}_lib.rs" -o "$d/librs$index.a" > "$d/rs.txt" 2>&1 || fail "${stem}_lib.rs should compile: $(head -c 200 "$d/rs.txt")"
            link="-L $d -l static=rs$index"
        fi
        # shellcheck disable=SC2086
        rustc -O -Coverflow-checks=on "benches/three/$stem.rs" $link -o "$d/rs$index" > "$d/rs.txt" 2>&1 || fail "$stem.rs should compile: $(head -c 200 "$d/rs.txt")"
        same "$("$d/rs$index" 2> /dev/null)" "$expected" "$stem.rs agrees with $stem.cho"
    fi
    index=$((index + 1))
done
# The f64 pair has no cancho half (that is the point of section 4), so the two of them are checked against each other.
if [ -n "$cc" ] && [ "$have_rustc" = 1 ]; then
    $cc -O2 benches/three/mandelbrot_f64.c -o "$d/cf64" > /dev/null 2>&1 || fail "mandelbrot_f64.c should compile"
    rustc -O benches/three/mandelbrot_f64.rs -o "$d/rsf64" > /dev/null 2>&1 || fail "mandelbrot_f64.rs should compile"
    same "$("$d/cf64" 2> /dev/null)" "$("$d/rsf64" 2> /dev/null)" "the two f64 builds agree, so section 4's precision figure is what it says"
fi
same "$index" 3 "three groups"
end

# `docs/benchmarks-game.md` section 2: every ported program prints the answer the Benchmarks Game publishes. Two programs written here agreeing with each other proves only that the same mistake was made twice; agreeing with a
# number someone else published is evidence. Each is run at the size the Game states an answer for.
begin benchmark_game_programs_print_the_published_answer
fresh game
printf '228\nPfannkuchen(7) = 16\n' > "$d/fannkuch.want"
printf '1.274219991\n' > "$d/spectral.want"
printf 'stretch tree of depth 11\t check: 4095\n1024\t trees of depth 4\t check: 31744\n256\t trees of depth 6\t check: 32512\n64\t trees of depth 8\t check: 32704\n16\t trees of depth 10\t check: 32752\nlong lived tree of depth 10\t check: 2047\n' > "$d/binarytrees.want"
for pair in fannkuch:7 spectral:100 binarytrees:10; do
    bench=${pair%%:*}; size=${pair#*:}
    r build --std "benches/game/$bench.cho" -o "$d/$bench"; succeeds "$bench builds"
    "$d/$bench" "$size" > "$d/$bench.out" 2> /dev/null
    same "$?" 0 "$bench exits 0"
    cmp -s "$d/$bench.out" "$d/$bench.want" || fail "$bench at N=$size should print what the Benchmarks Game publishes"
    # And the C counterpart, which is only a fair comparison if it computes the same thing.
    cc -O2 "benches/game/$bench.c" -o "$d/${bench}_c" > "$d/cc.txt" 2>&1 || fail "$bench.c should compile: $(head -c 200 "$d/cc.txt")"
    "$d/${bench}_c" "$size" > "$d/${bench}_c.out" 2> /dev/null
    cmp -s "$d/${bench}_c.out" "$d/$bench.want" || fail "$bench.c must compute the same thing, or the timing means nothing"
done
end

# Section 2.1: fasta and reverse-complement, the two programs that were waiting on docs/bulk-io.md. Both are checked against benches/game/fasta-1000.txt, which is what `fasta` must print at N=1000 and is also the Game's own
# N=1000 input for `revcomp`, whose answer is benches/game/revcomp-1000.txt.
begin fasta_and_reverse_complement_print_the_published_answer
fresh gameio
for bench in fasta revcomp; do
    r build --std "benches/game/$bench.cho" -o "$d/$bench"; succeeds "$bench builds"
    cc -O2 "benches/game/$bench.c" -o "$d/${bench}_c" > "$d/cc.txt" 2>&1 || fail "$bench.c should compile: $(head -c 200 "$d/cc.txt")"
done
"$d/fasta" 1000 < /dev/null > "$d/fasta.out" 2> /dev/null
same "$?" 0 "fasta exits 0"
cmp -s "$d/fasta.out" benches/game/fasta-1000.txt || fail "fasta should print what the Benchmarks Game publishes"
"$d/fasta_c" 1000 < /dev/null > "$d/fasta_c.out" 2> /dev/null
cmp -s "$d/fasta_c.out" benches/game/fasta-1000.txt || fail "fasta.c must compute the same thing"
"$d/revcomp" < benches/game/fasta-1000.txt > "$d/revcomp.out" 2> /dev/null
same "$?" 0 "revcomp exits 0"
cmp -s "$d/revcomp.out" benches/game/revcomp-1000.txt || fail "revcomp should print what the Benchmarks Game publishes"
"$d/revcomp_c" < benches/game/fasta-1000.txt > "$d/revcomp_c.out" 2> /dev/null
cmp -s "$d/revcomp_c.out" benches/game/revcomp-1000.txt || fail "revcomp.c must compute the same thing"
end
