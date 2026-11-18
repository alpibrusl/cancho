# floats.sh -- the script of tests/commands/script_tests.cho for `conformance/floats.rs` (deleted): floating point printing, `sqrt`, NaN, and `exp`, `log` and `pow` (docs/float-printing.md
# section 5, docs/floating-point.md section 2, docs/float-math.md sections 2 and 6). Each check is a program the compiler under test builds from a recorded vector file, so the literals in it go
# through the lexer too: the oracle was Rust (`{:e}`, `f64::sqrt`, `exp`, `ln`, `powf`), and its answers are recorded in tests/vectors/floats_*. $1 is the compiler, $2 a scratch directory; run
# from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh

# program NAME: builds the source in $work/NAME.cho (with --std) and runs it; the output is $work/NAME.out, the exit status $run_status.
program() {
    build "$work/$1.cho" "$1" || { run_status=build; return 1; }
    "$work/$1" > "$work/$1.out" 2> "$work/$1.err"
    run_status=$?
}

# show_head: the program that prints floats with `fmt.float_into`, up to the opening of the `borrow` block (the `show` calls follow).
show_head() {
    cat << 'EOF'
module main;

import std.fmt;
import std.io;

fn show[&i](i: &!i Io, x: float) -> [io_write] int {
    region a {
        let buf = alloc_slice[a](24, byte_of(0));
        let n = fmt.float_into(buf, x);
        io.write_all(i, buf[0..n]);
        io.newline(i);
    }
    return 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi);
    borrow mut io as &!i in {
EOF
}
show_tail() { printf '    }\n    release(io);\n    return 0;\n}\n'; }

begin "shortest printing agrees with the oracle"
# 9,173 values: every normal power of two, the subnormal powers and all-ones mantissas, every power of ten in range, the ones with a reputation, numbers with a short decimal form and their ulp
# neighbours, and a splitmix64 spread of bit patterns. The expected output is the literal the program was written with (`{:e}`, the shortest decimal that reads back, which the lexer reads
# correctly rounded), except where the value is an exact tie between two shortest decimals: `{:e}` rounds it up and `float_into` to the even digit (docs/float-printing.md 3.4); those lines are
# recorded in tests/vectors/floats_corpus.expected as the printer prints them, and counted.
same "$(wc -l < tests/vectors/floats_corpus.txt | tr -d ' ')" 9173 "the values recorded (a count, so the file cannot shrink silently)"
{
    show_head
    awk '{ print "        show(i, " $0 ");" }' tests/vectors/floats_corpus.txt
    show_tail
} > "$work/corpus.cho"
program corpus
same "$run_status" 0 "the corpus program's exit status"
same "$(wc -l < "$work/corpus.out" | tr -d ' ')" 9173 "one line per value"
differing=$(diff "$work/corpus.out" tests/vectors/floats_corpus.expected | grep -c '^<')
same "$differing" 0 "lines that differ from the recorded answers ($(diff "$work/corpus.out" tests/vectors/floats_corpus.expected | head -3 | tr '\n' '|'))"
same "$(diff tests/vectors/floats_corpus.txt tests/vectors/floats_corpus.expected | grep -c '^>')" 2 "the exact ties, printed to the even digit and not as {:e} rounds them"
end

begin "the values with no literal are spelled"
# docs/floating-point.md section 2: infinity, minus infinity, two NaNs and the signed zero are spelled, not refused.
{
    show_head | sed '$d'
    cat << 'EOF'
    let huge = 1.0e308;
    let infinite = huge * 10.0;
    let nothing = 0.0;
    borrow mut io as &!i in {
        show(i, infinite);
        show(i, -infinite);
        show(i, infinite - infinite);
        show(i, nothing / nothing);
        show(i, -nothing);
    }
    release(io);
    return 0;
}
EOF
} > "$work/specials.cho"
program specials
same "$run_status" 0 "the program's exit status"
same "$(cat "$work/specials.out")" "inf
-inf
NaN
NaN
-0e0" "the spellings"
end

begin "a short buffer is refused rather than overrun"
# The caller chose the buffer, so the caller hears about it: -1 for 4 bytes (exit 7), the length 19 for 24 (an exit of 9 would be a wrong length).
cat > "$work/short.cho" << 'EOF'
module main;

import std.fmt;
import std.io;

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi);
    var code = 0;
    region a {
        let small = alloc_slice[a](4, byte_of(0));
        if fmt.float_into(small, 3.141592653589793) == 0 - 1 {
            code = 7;
        }
        let enough = alloc_slice[a](24, byte_of(0));
        if fmt.float_into(enough, 3.141592653589793) != 19 {
            code = 9;
        }
    }
    release(io);
    return code;
}
EOF
program short
same "$run_status" 7 "a short buffer answers -1, a long one the length"
end

begin "every NaN has one bit pattern"
# docs/differential.md section 4: x86-64 sets the sign of a generated NaN and aarch64 does not, so every way this language can make a NaN, at run time and folded, must read back as 0x7ff8000000000000.
cat > "$work/nan.cho" << 'EOF'
import std.io;
fn show[&i](i: &!i Io, x: float) -> [io_write] int {
    io.print_int(i, bits_of(x));
    io.newline(i);
    return 0;
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi);
    var zero = 0.0;
    var one = 1.0;
    var inf = 1.0 / 0.0;
    borrow mut io as &!i in {
        show(i, 0.0 / 0.0);
        show(i, -(0.0 / 0.0));
        show(i, zero / zero);
        show(i, -(zero / zero));
        show(i, inf - inf);
        show(i, inf * zero);
        show(i, sqrt(-one));
        show(i, (zero / zero) + one);
        show(i, -((zero / zero) * one));
    }
    release(io);
    return 0;
}
EOF
program nan
same "$run_status" 0 "the program's exit status"
same "$(wc -l < "$work/nan.out" | tr -d ' ')" 9 "the NaNs made"
same "$(grep -c '^9221120237041090560$' "$work/nan.out")" 9 "the NaNs that read as 0x7ff8000000000000"
end

begin "sqrt is correctly rounded"
# docs/float-math.md section 2: against the hardware's own `f64::sqrt`, recorded as bits (tests/vectors/floats_sqrt.txt: `literal bits`): the specials, a decade sweep and 2,000 values across
# the exponent range where the old twenty-step Newton loop was wrong by 10^43 and more; the bits are compared, so a last-bit disagreement is seen.
same "$(wc -l < tests/vectors/floats_sqrt.txt | tr -d ' ')" 2010 "the values recorded (a count)"
{
    printf 'import std.io;\n\nfn show[&i](i: &!i Io, x: float) -> [io_write] int {\n    io.print_int(i, bits_of(x));\n    io.newline(i);\n    return 0;\n}\n\n'
    printf 'fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(ffi); release(fs); release(heap); release(args);\n    borrow mut io as &!i in {\n'
    awk '{ print "        show(i, sqrt(" $1 "));" }' tests/vectors/floats_sqrt.txt
    printf '    }\n    release(io);\n    return 0;\n}\n'
} > "$work/sqrt.cho"
program sqrt
same "$run_status" 0 "the program's exit status"
awk '{ print $2 }' tests/vectors/floats_sqrt.txt > "$work/sqrt.want"
if ! cmp -s "$work/sqrt.out" "$work/sqrt.want"; then
    fail "sqrt differs from the hardware's, first three: $(diff "$work/sqrt.out" "$work/sqrt.want" | grep '^[<>]' | head -3 | tr '\n' '|')"
fi
end

begin "exp, log and pow agree with the hardware within the stated accuracy"
# docs/float-math.md section 6: library code with a stated accuracy, not correctly rounded and not asserted to be. tests/vectors/floats_math.txt has 2,008 `exp`, 2,007 `log` and 509 `pow` cases
# with Rust's answer; exp within 1e-9 relative error, log within 1e-9 relative (1e-9 absolute where the answer is under 0.01), pow within 1e-8 relative (1e-6 absolute under 0.01; a NaN where
# Rust has one; the infinity of 0^-1).
same "$(grep -c '^exp ' tests/vectors/floats_math.txt)" 2008 "exp cases"
same "$(grep -c '^log ' tests/vectors/floats_math.txt)" 2007 "log cases"
same "$(grep -c '^pow ' tests/vectors/floats_math.txt)" 509 "pow cases"
{
    cat << 'EOF'
import std.io;
import std.math;

fn near(got: float, want: float, rel: float, abs_below: float, abs_tol: float) -> [] bool {
    if want != want {
        return got != got;
    }
    if abs_below > 0.0 {
        var w = want;
        if w < 0.0 {
            w = 0.0 - w;
        }
        if w < abs_below {
            var d = got - want;
            if d < 0.0 {
                d = 0.0 - d;
            }
            return d < abs_tol;
        }
    }
    if want == 0.0 {
        return got == 0.0;
    }
    if want - want != 0.0 {
        return got == want;
    }
    var d = got - want;
    if d < 0.0 {
        d = 0.0 - d;
    }
    var m = want;
    if m < 0.0 {
        m = 0.0 - m;
    }
    return d / m < rel;
}

fn run[&i](i: &!i Io) -> [io_write] int {
    var bad = 0;
EOF
    awk '
        function spelled(w) { if (w == "nan") return "(0.0 / 0.0)"; if (w == "inf") return "(1.0e308 * 10.0)"; if (w == "-inf") return "(-1.0e308 * 10.0)"; return w }
        $1 == "exp" { call = "math.exp(" $2 ")"; want = spelled($3); tol = "1e-9, 0.0, 0.0" }
        $1 == "log" { call = "math.log(" $2 ")"; want = spelled($3); tol = "1e-9, 0.01, 1e-9" }
        $1 == "pow" { call = "math.pow(" $2 ", " $3 ")"; want = spelled($4); tol = "1e-8, 0.01, 1e-6" }
        { print "    if !near(" call ", " want ", " tol ") {\n        bad = bad + 1;\n        io.print_int(i, " NR ");\n        io.newline(i);\n    }" }
    ' tests/vectors/floats_math.txt
    cat << 'EOF'
    return bad;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi); release(fs); release(heap); release(args);
    var bad = 0;
    borrow mut io as &!i in { bad = run(i); }
    release(io);
    return bad;
}
EOF
} > "$work/math.cho"
program math
same "$run_status" 0 "the number of cases outside the stated accuracy (their line numbers in the vector file: $(head -c 100 "$work/math.out" | tr '\n' ' '))"
end
