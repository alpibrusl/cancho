# compile_cli.sh -- the script of tests/commands/script_tests.cho for four tests of conformance/selfhost/compile.rs (deleted; docs/gate.md, G4 wave 4): a function the compiler cannot write is a refusal
# when the program reaches it, builds that start together with an empty cache all succeed, build from the command line (a program with the standard library runs as the Rust build did, a
# refusal is exit status 1 that says where) and the refusal of a program the language refuses is in the words of its rule. $1 is the compiler, $2 a scratch directory; run from the root of the
# repository.
#
# tests/vectors/compile_cli.txt is what the Rust `cancho` said of the two programs below: `program-exit`, `program-out` (the first one's status and standard output, the Rust build run) and, from
# `check --output json` on the refused one, `refused-position`, `refused-rule` and `refused-sentence`. The recording is this script with the Rust `cancho`:
#     ORACLE=rust RECORD_TO=tests/vectors/compile_cli.txt sh tests/commands/compile_cli.sh target/release/cancho DIR
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
oracle=${ORACLE:-}
vector=$(pwd)/tests/vectors/compile_cli.txt
mkdir -p "$work"
d=$work/cli
rm -rf "$d"
mkdir -p "$d"
cd "$d" || exit 1
got=$d/got.txt
: > "$got"
fact() { printf '%s %s\n' "$1" "$2" >> "$got"; }
want() { grep "^$1 " "$vector" | head -1 | cut -d' ' -f2-; }
# build ARGS...: the compiler's build, which for the recording is the Rust compiler (`--compiler rust`).
b() { if [ -n "$oracle" ]; then "$cancho" build "$@" --compiler rust; else "$cancho" build "$@"; fi; }

# docs/self-hosting.md section 6: a function the compiler has no code for (here one that takes a function of four parameters) stops a build only when the program reaches it, and the answer says where.
begin a_function_the_compiler_cannot_write_is_a_refusal_when_the_program_reaches_it
apply='fn apply(f: fn(int, int, int, int) -> [] int, x: int) -> [] int { return f(x, x, x, x); }
fn sum(a: int, b: int, c: int, d: int) -> [] int { return a + b + c + d; }'
printf '%s\nfn main(world: World) -> [] int { release(world); return apply(sum, 1); }\n' "$apply" > reached.cho
printf '%s\nfn main(world: World) -> [] int { release(world); return 0; }\n' "$apply" > unreached.cho
if [ -z "$oracle" ]; then
    "$cancho" build reached.cho -o reached > reached.out 2> reached.err
    same "$?" 3 "a call to a function left out is refused: the exit status"
    # `apply(sum, 1)` starts at column 58 of line 3 (the program is `fn apply..`, `fn sum..` and then `main`).
    case $(head -1 reached.err) in
        "reached.cho:3:58: "*"no code for"*) ;;
        *) fail "a call to a function left out is refused at the call: $(head -1 reached.err)" ;;
    esac
    [ ! -e reached ] || fail "the refused build left an executable"
    "$cancho" build unreached.cho -o unreached > unreached.out 2> unreached.err
    same "$?" 0 "a function nobody calls is left out without a word"
    [ ! -s unreached.err ] || fail "a function nobody calls is left out without a word: $(cat unreached.err)"
    "./unreached"
    same "$?" 0 "and the program runs"
fi
end

# Builds that start together (people have two terminals) each build, and none of them sees another's half-written files. The Rust test raced eight `--compiler auto` builds on an empty
# compiler cache and counted what was left in it; the compiler in cancho is the binary that is running and has no cache, so what is left to race is the builds' own scratch files.
begin builds_that_start_together_with_an_empty_cache_all_succeed
printf 'fn main(world: World) -> [] int {\n    release(world);\n    return 5;\n}\n' > race.cho
if [ -z "$oracle" ]; then
    i=0
    while [ "$i" -lt 8 ]; do
        "$cancho" build race.cho -o "race$i" > "race$i.out" 2>&1 &
        i=$((i + 1))
    done
    wait
    i=0
    while [ "$i" -lt 8 ]; do
        [ -x "race$i" ] || fail "build $i: $(head -c 300 "race$i.out")"
        "./race$i"
        same "$?" 5 "build $i: the program's exit status"
        i=$((i + 1))
    done
fi
end

# Build from the command line: the program, with the standard library it imports, runs as the Rust build does, and a refusal is an exit status 1 that says where.
begin build_with_the_compiler_in_cancho_from_the_command_line
cat > program.cho <<'EOF'
import std.io;
fn say[&i](con: &!i Io) -> [io_write] int {
    return io.write_all(con, "from the compiler in cancho\n");
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    release(heap);
    release(args);
    borrow mut io as &!i in {
        say(i);
    }
    release(io);
    return 3;
}
EOF
printf 'fn main(world: World) -> [] int {\n    release(world);\n    return true;\n}\n' > refused.cho
b program.cho --std -o program > program.build 2>&1
same "$?" 0 "the program builds"
./program > program.out 2> /dev/null
fact program-exit "$?"
fact program-out "$(cat program.out)"
if [ -n "$oracle" ]; then
    "$cancho" check --output json refused.cho > refused.json 2> /dev/null
    fact refused-position "$(sed -n 's/.*"line": \([0-9]*\), "column": \([0-9]*\).*/refused.cho:\1:\2/p' refused.json)"
    fact refused-rule "$(sed -n 's/.*"rule": "\(.*\)",/\1/p' refused.json)"
    fact refused-sentence "$(sed -n 's/.*"explanation": "\(.*\)",/\1/p' refused.json)"
else
    same "$(cat program.out)" "$(want program-out)" "what the program prints"
    same "$(./program > /dev/null 2>&1; echo $?)" "$(want program-exit)" "the program's exit status"
    "$cancho" build refused.cho -o refused > refused.out 2> refused.err
    same "$?" 1 "a refused program: the exit status"
    said=$(cat refused.err)
    case $said in *"error: "*) ;; *) fail "'error: ' is not in '$said'" ;; esac
    case $said in *"$(want refused-sentence)"*) ;; *) fail "the sentence of the rule is not in '$said'" ;; esac
    case $said in *"^^^^"*) ;; *) fail "the line and what of it: '$said'" ;; esac
    case $said in "$(want refused-position): error: "*) ;; *) fail "where (the position $(want refused-position)): '$said'" ;; esac
    "$cancho" check --output json refused.cho > refused.json 2> /dev/null
    case $(cat refused.json) in *"\"rule\": \"$(want refused-rule)\""*) ;; *) fail "the rule $(want refused-rule) is not named by check --output json" ;; esac
    [ ! -e refused ] || fail "the refused build left an executable"
fi
end

if [ -n "$oracle" ]; then cp "$got" "${RECORD_TO:-/dev/null}"; fi
cd "$work" && rm -rf "$d"
