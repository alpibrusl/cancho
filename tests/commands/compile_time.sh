# compile_time.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/compile_time.rs (docs/gate.md G4, wave 4): compile-time evaluation (docs/compile-time.md): what folds, what stays a
# call, and what still traps. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

# number FILE KEY: the integer after `"KEY": ` in a report.
number() { sed -n "s/.*\"$2\": \([0-9][0-9]*\).*/\1/p" "$1" | head -1; }
# report SOURCE-FILE: `authority --output json` of the file, in $work/report.json.
report() { "$cancho" authority "$1" --std --output json > "$work/report.json" 2> "$work/report.err"; }

# docs/emitted-checks.md section 1: folding a `byte`-returning call. A pure call on constant arguments is folded to a literal, and `Expr` has no `byte` literal, so the fold used to put an `int`-shaped node where the
# backend expects one machine byte. Each of the three expressions failed the verifier with no span and no rule tag.
begin folding_a_byte_returning_call
fresh byte
cat > "$d/fold.cho" <<'EOF'
fn g(n: int) -> [] byte { return byte_of(n); }
fn h() -> [] byte { return g(65); }
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi); release(io);
    var same = 0;
    if g(65) == byte_of(66) { same = 1; }
    return int_of(g(65)) + int_of(h()) - 130 + same;
}
EOF
r build "$d/fold.cho" -o "$d/fold"; succeeds "a folded byte keeps its width"
"$d/fold" > /dev/null 2>&1
same "$?" 0 "'A' twice is 130, and 65 is not 66"
end

# docs/compile-time.md sections 2 and 3: the pass does what it says. Checked through the report rather than a disassembler, for the reason section 9 gives for having a report at all.
begin constants_are_evaluated_at_compile_time
fresh constants
cat > "$d/fold.cho" <<'EOF'
fn factorial(n: int) -> [] int {
    if n < 2 { return 1; }
    return n * factorial(n - 1);
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi); release(io);
    let arithmetic = 2 + 3 * 4 - 14;
    return factorial(5) - 120 + arithmetic;
}
EOF
report "$d/fold.cho"
ops=$(number "$work/report.json" folded_operators)
if [ -z "$ops" ]; then
    gap "the report counts what folded" "has no folded_operators and no folded_calls" "says folded_operators > 0 and folded_calls 1"
else
    [ "$ops" -gt 0 ] || fail "the operators should fold: folded_operators is '$ops'"
    same "$(number "$work/report.json" folded_calls)" 1 "factorial(5) should fold"
fi
# What does not depend on the count: the program builds, and its answer is the arithmetic's.
r build --std "$d/fold.cho" -o "$d/fold"; succeeds build
"$d/fold" > /dev/null 2>&1
same "$?" 0 "factorial(5) - 120 + (2 + 3 * 4 - 14) is 0"
end

# Section 5: running out of fuel is not an error, and not visible in the answer. `fib(24)` needs about 150 000 calls, which is past the budget, so the call survives into the binary; the program still prints 46368.
begin running_out_of_fuel_leaves_a_working_program
fresh fuel
cat > "$d/fuel.cho" <<'EOF'
import std.io;
fn fib(n: int) -> [] int {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(args); release(heap); release(fs); release(ffi);
    borrow mut io as &!i in {
        io.print_int(i, fib(23));
        io.newline(i);
        io.print_int(i, fib(24));
        io.newline(i);
    }
    release(io);
    return 0;
}
EOF
report "$d/fuel.cho"
if [ -z "$(number "$work/report.json" folded_calls)" ]; then
    gap "the report counts what folded" "has no folded_calls" "says folded_calls 1: fib(23) is inside the budget and fib(24) is not"
else
    same "$(number "$work/report.json" folded_calls)" 1 "fib(23) is inside the budget and fib(24) is not"
fi
r build --std "$d/fuel.cho" -o "$d/fuel"; succeeds build
same "$("$d/fuel" 2> /dev/null)" "28657
46368" "the folded call and the one that ran agree"
end

# Section 4: a trap that is merely *possible* is untouched. The same operators with a value the compiler cannot know still emit their check and still stop the process, which is the guarantee docs/defined-behaviour.md
# section 2.1 makes.
begin a_possible_trap_still_traps
fresh trap
index=0
for pair in "9223372036854775807:n + 1" "0:1 / n" "64:1 << n"; do
    setup=${pair%%:*}; expression=${pair#*:}
    printf 'fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(args); release(heap); release(fs); release(ffi); release(io);\n    var n = %s;\n    return %s;\n}\n' "$setup" "$expression" > "$d/trap$index.cho"
    r build "$d/trap$index.cho" -o "$d/trap$index"
    if [ "$code" != 0 ]; then fail "'$expression' should compile, n is a runtime value: $(printf '%s' "$err" | head -c 200)"; else
        { "$d/trap$index" > /dev/null 2>&1; c=$?; } 2> /dev/null
        # A process ended by a signal is 128 and the signal's number in a shell; a status the program chose is below.
        if [ "$c" -lt 129 ]; then fail "'$expression' should be killed by a signal, and the status is $c"; fi
    fi
    index=$((index + 1))
done
same "$index" 3 "three possible traps"
end

# Section 6: folding is a fact about the *host* arithmetic agreeing with the target's, and the float row is the one worth checking. Both are computed and compared, which cannot pass by both sides being wrong in the same way.
begin a_folded_float_is_the_same_float
fresh float
cat > "$d/float.cho" <<'EOF'
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
    // The same expression twice: once from literals, which folds, and once through `var`s, which does not.
    var a = 0.1;
    var b = 0.2;
    var c = 3.0;
    borrow mut io as &!i in {
        show(i, 0.1 + 0.2 * 3.0);
        show(i, a + b * c);
        show(i, 1.0 / 3.0);
        var one = 1.0;
        var three = 3.0;
        show(i, one / three);
    }
    release(io);
    return 0;
}
EOF
r build --std "$d/float.cho" -o "$d/float"; succeeds build
"$d/float" > "$d/out.txt" 2> /dev/null
same "$(wc -l < "$d/out.txt" | tr -d ' ')" 4 "four lines"
l1=$(sed -n 1p "$d/out.txt"); l2=$(sed -n 2p "$d/out.txt"); l3=$(sed -n 3p "$d/out.txt"); l4=$(sed -n 4p "$d/out.txt")
same "$l1" "$l2" "0.1 + 0.2 * 3.0 folded and unfolded must agree"
same "$l3" "$l4" "1.0 / 3.0 folded and unfolded must agree"
end
