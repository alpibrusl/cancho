# cli_project.sh -- the script of tests/commands/script_tests.cho for seven tests of `conformance/selfhost/cli.rs` (deleted): `the_command_line_in_cancho_tests_like_cancho_test`,
# `..._satisfies_like_cancho_satisfy`, `..._works_in_a_project_like_cancho`, `..._says_its_version_and_checks_the_pin_like_cancho`, `..._carries_the_standard_library`,
# `..._says_what_this_build_does_not_carry`. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# Their oracle was the Rust `cancho`. A row of the transcript is what the command said in the Rust test's own normal form (see `plain`), and the rows are the Rust binary's: the transcript was made
# once with it (2026-11-17) and the compiler in cancho's is the same except where a line says `cancho only` (an assertion the Rust test made of the command in cancho alone). The compiler's own
# commit and host are written REV and HOST, so that the transcript is the same for every build.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/cli_lib.sh
cd "$root" || exit 1

version_line=$("$cancho" --version)
rev=$(printf '%s' "$version_line" | sed 's/^cancho [^ ]* (rev \([^,]*\), host .*/\1/')
host=$(printf '%s' "$version_line" | sed 's/.*, host \([^)]*\))$/\1/')
# plain FILE [DIR]: the Rust tests' normal form of what a command said: without the line that says the compiler is being built and the linker's warnings; the place of an error is kept and its
# words are not (the command in cancho says the sentence of the rule, `docs/cli-in-cancho.md` section 12); a line of a source excerpt (`|`, `12 |`) is dropped.
plain() {
    # the directory is spelled both as the script has it and as the command says it (its working directory is the real path: /var is /private/var on macOS)
    _phys=$(cd "${2:-.}" 2> /dev/null && pwd -P)
    grep -v -e '^building the compiler in cancho' -e '^ld: warning:' "$1" | sed 's/: error: .*$//' | awk '{ t = $0; sub(/^[ \t]+/, "", t); if (t ~ /^\|/) next; u = t; sub(/^[0-9]+/, "", u); if (u != t && u ~ /^ \|/) next; printf "%s ", $0 }' | tr -s ' \t' ' ' | sed "s| \$||; s|${_phys:-NO_SUCH_DIR_}|DIR|g; s|${2:-NO_SUCH_DIR_}|DIR|g; s|$rev|REV|g; s|$host|HOST|g"
    echo
}
# say NAME STATUS OUT ERR: the row of a command.
say() { echo "$1: exit $2"; echo "  out: $(plain "$3" "$5")"; echo "  err: $(plain "$4" "$5")"; }

# --- the_command_line_in_cancho_tests_like_cancho_test
begin the_command_line_in_cancho_tests_like_cancho_test
rm -rf "$work/t"
mkdir -p "$work/t"
tcase() { # NAME FILES...: `cancho test FILES` in $work/t, whose files are there
    _name=$1
    shift
    ( cd "$work/t" && "$cancho" test "$@" > o.txt 2> e.txt < /dev/null; _s=$? ; say "$_name" "$_s" o.txt e.txt )
    ls "$work/t" | grep -q cancho-test && fail "$_name: left over: $(ls "$work/t")"
    rm -rf "$work/t"/*
}
module_file() { # PATH NAME PUB ANSWER
    printf 'module %s;\n\n%sfn %s() -> [] int {\n    return %s;\n}\n' "$1" "${3:+pub }" "$2" "$4"
}
cat > "$work/t/x.cho" <<'EOF'
fn test_a() -> [] int {
    return 0;
}

fn test_b[&h](heap: &!h Heap) -> [] int {
    return 0;
}
EOF
tcase "all pass" x.cho
cat > "$work/t/x.cho" <<'EOF'
fn test_ok() -> [] int {
    return 0;
}

fn test_noisy[&i](io: &!i Io) -> [io_write] int {
    putchar(io, 'x');
    putchar(io, '\n');
    return 7;
}

fn test_both[&h, &i](heap: &!h Heap, io: &!i Io) -> [io_write] int {
    putchar(io, 'y');
    return 256;
}
EOF
tcase "some fail" x.cho
cat > "$work/t/x.cho" <<'EOF'
fn test_fine() -> [] int {
    return 0;
}

fn test_trap() -> [] int {
    return 1 / zero();
}

fn zero() -> [] int {
    return 0;
}
EOF
tcase "a trap" x.cho
printf 'fn test_root() -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
module_file lib.core test_core pub 0 > "$work/t/a.cho"
module_file lib.more test_more pub 3 > "$work/t/b.cho"
tcase "tests in modules" x.cho a.cho b.cho
module_file lib.core test_core pub 0 > "$work/t/a.cho"
printf 'module lib.more;\n\npub fn test_one() -> [] int {\n    return 0;\n}\n\npub fn test_two() -> [] int {\n    return 3;\n}\n' > "$work/t/b.cho"
tcase "two tests in the second module" a.cho b.cho
printf 'fn test_root() -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
module_file one.core test_one pub 0 > "$work/t/a.cho"
module_file two.core test_two pub 0 > "$work/t/b.cho"
tcase "two modules end the same" x.cho a.cho b.cho
printf 'fn test_a() -> [] int {\n    return 0;\n}\n\nfn test_b[&h](heap: &!h Heap) -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
module_file lib.core test_core '' 0 > "$work/t/a.cho"
tcase "a test that is not pub" x.cho a.cho
printf 'fn helper() -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
tcase "no tests" x.cho
printf 'fn main(world: World) -> [] int {\n    release(world);\n    return 0;\n}\n\nfn test_a() -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
tcase "a main" x.cho
printf 'fn test_a[&h](heap: &!h Heap) -> [] int {\n    return 0;\n}\n\nfn test_b[T](x: T) -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
tcase "a type parameter" x.cho
printf 'fn test_a() -> [] bool {\n    return true;\n}\n' > "$work/t/x.cho"
tcase "not an int" x.cho
printf 'fn test_a(n: int) -> [] int {\n    return n;\n}\n' > "$work/t/x.cho"
tcase "a parameter of another kind" x.cho
printf 'fn test_a[&h](a: &!h Heap, b: &!h Heap) -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
tcase "a capability twice" x.cho
printf 'fn test_a( -> [] int {\n    return 0;\n}\n' > "$work/t/x.cho"
tcase "a file that does not parse" x.cho
printf 'fn test_a() -> [] int {\n    return missing;\n}\n' > "$work/t/x.cho"
tcase "a program that is refused" x.cho
end

# --- the_command_line_in_cancho_satisfies_like_cancho_satisfy
begin the_command_line_in_cancho_satisfies_like_cancho_satisfy
rm -rf "$work/s"
mkdir -p "$work/s"
candidate='module lib.calc;

pub fn double(n: int) -> [] int {
    return n + n;
}

pub fn triple(n: int) -> [] int {
    return n + n + n;
}

fn test_inner() -> [] int {
    return 0;
}
'
contract='import lib.calc;

fn test_double() -> [] int {
    if calc.double(4) == 8 {
        return 0;
    }
    return 1;
}

fn test_triple() -> [] int {
    if calc.triple(2) == 6 {
        return 0;
    }
    return 1;
}
'
scase() { # NAME OPTIONS FILES... (the contract is contract.cho and the candidate cand.cho; the files are in $work/s)
    _name=$1
    _opts=$2
    shift 2
    ( cd "$work/s" && "$cancho" satisfy $_opts "$@" > o.txt 2> e.txt < /dev/null; _s=$?; say "$_name" "$_s" o.txt e.txt )
    ls "$work/s" | grep -q cancho-test && fail "$_name: left over"
    rm -rf "$work/s"/*
}
both() { printf '%s' "$contract" > "$work/s/contract.cho"; printf '%s' "$candidate" > "$work/s/cand.cho"; }
both; scase "satisfied" '' contract.cho cand.cho
both; printf '%s' "$candidate" | sed 's/n + n + n/n + n + n + 1/' > "$work/s/cand.cho"; scase "a test that fails" '' contract.cho cand.cho
printf 'fn test_inc() -> [] int {\n    if inc(1) == 2 {\n        return 0;\n    }\n    return 1;\n}\n' > "$work/s/contract.cho"
printf 'struct Pair { a: int, b: int }\n\nfn inc(n: int) -> [] int {\n    return n + 1;\n}\n' > "$work/s/cand.cho"; scase "a root candidate" '' contract.cho cand.cho
printf 'import lib.calc;\n\nfn helper() -> [] int {\n    return calc.double(1);\n}\n' > "$work/s/contract.cho"; printf '%s' "$candidate" > "$work/s/cand.cho"; scase "a contract with no tests" '' contract.cho cand.cho
printf 'fn test_nothing() -> [] int {\n    return 0;\n}\n' > "$work/s/contract.cho"; printf '%s' "$candidate" > "$work/s/cand.cho"; scase "a contract that calls nothing" '' contract.cho cand.cho
printf '// calc.double(1) triple(2)\nfn test_text() -> [] int {\n    let s = "double(1)";\n    return 0;\n}\n' > "$work/s/contract.cho"; printf '%s' "$candidate" > "$work/s/cand.cho"; scase "a contract that only names a function in a comment and a string" '' contract.cho cand.cho
printf '%s' "$contract" > "$work/s/contract.cho"
printf 'module lib.calc;\n\npub fn double(n: int) -> [] int {\n    return missing;\n}\n\npub fn triple(n: int) -> [] int {\n    return n;\n}\n' > "$work/s/cand.cho"; scase "a candidate that is refused" '' contract.cho cand.cho
printf 'import lib.calc;\n\nfn test_x() -> [] int {\n    return calc.quadruple(1);\n}\n' > "$work/s/contract.cho"; printf '%s' "$candidate" > "$work/s/cand.cho"; scase "a contract that calls what is not there" '' contract.cho cand.cho
printf 'import lib.calc;\n\nfn test_x(n: int) -> [] int {\n    return calc.double(n) - 2 * n;\n}\n' > "$work/s/contract.cho"; printf '%s' "$candidate" > "$work/s/cand.cho"; scase "a test of the wrong shape" '' contract.cho cand.cho
both; scase "an option that is not one" --bogus contract.cho cand.cho
both; rm "$work/s/cand.cho"; scase "one file" '' contract.cho
both; printf 'fn extra() -> [] int {\n    return 1;\n}\n' > "$work/s/more.cho"; scase "three files" '' contract.cho cand.cho more.cho
end

# --- the_command_line_in_cancho_works_in_a_project_like_cancho
begin the_command_line_in_cancho_works_in_a_project_like_cancho
dir=$work/proj
two='[package]
name = "demo"

[[bin]]
name = "hello"
sources = ["src"]

[[bin]]
name = "other"
sources = ["src/main.cho"]
out = "dist/x/other"

[[test]]
name = "unit"
sources = ["tests"]

[[test]]
name = "more"
sources = ["tests/t.cho", "tests"]
'
main='edition 7;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock, signals, exec } = split(world);
    release(ffi);
    release(net);
    release(signals);
    release(clock);
    release(exec);
    release(fs);
    release(args);
    release(heap);
    release(io);
    return 0;
}
'
tests='edition 7;
fn test_one() -> [] int { return 0; }
fn test_two() -> [] int { return 1; }
'
pcase() { # NAME TOML BELOW ARGS...
    _name=$1
    _toml=$2
    _below=$3
    shift 3
    rm -rf "$dir"
    mkdir -p "$dir/src" "$dir/tests" "$dir/empty"
    printf '%s' "$_toml" > "$dir/cancho.toml"
    printf '%s' "$main" > "$dir/src/main.cho"
    printf '%s' "$tests" > "$dir/tests/t.cho"
    ( cd "$dir/$_below" && "$cancho" "$@" > "$work/p-out.txt" 2> "$work/p-err.txt" < /dev/null; echo $? > "$work/p-status.txt" )
    # The text of the TOML library's complaint is not something the command in cancho has (`docs/cli-in-cancho.md` section 20).
    [ "$_name" = "not toml" ] && : > "$work/p-err.txt"
    say "$_name" "$(cat "$work/p-status.txt")" "$work/p-out.txt" "$work/p-err.txt" "$dir"
    echo "  files: $(cd "$dir" && find . -type f | LC_ALL=C sort | tr '\n' ' ')"
}
pcase "build all" "$two" . build
pcase "build one" "$two" . build --bin other
pcase "build none such" "$two" . build --bin nope
pcase "build from below" "$two" src build
pcase "build bad flag" "$two" . build --nope
pcase "test bad flag" "$two" . test --nope
pcase "test all" "$two" . test
pcase "test one" "$two" . test --test more
pcase "test none such" "$two" . test --test nope
pcase "test from below" "$two" tests test
pcase "install" "$two" . install
pcase "install from below" "$two" src install
pcase "install with an argument" "$two" . install x
pcase "no program" '[package]
name = "demo"
' . build
pcase "no tests" '[package]
name = "demo"
' . test
pcase "unknown field" '[package]
name = "demo"
version = "1"
' . install
pcase "not toml" '[package
' . install
pcase "bad name" '[package]
name = "De Mo"
' . install
pcase "missing sources" '[package]
name = "demo"

[[bin]]
name = "a"
sources = ["nowhere"]
' . build
pcase "empty directory" '[package]
name = "demo"

[[bin]]
name = "a"
sources = ["empty"]
' . build
pcase "same name twice" '[package]
name = "demo"

[[bin]]
name = "a"
sources = ["src"]

[[bin]]
name = "a"
sources = ["src"]
' . build
pcase "compiler pin" '[package]
name = "demo"
cancho = "0000000000000000000000000000000000000000"

[[bin]]
name = "a"
sources = ["src"]
' . build
pcase "compiler pin ignored" '[package]
name = "demo"
cancho = "0000000000000000000000000000000000000000"

[[bin]]
name = "a"
sources = ["src"]
' . build --ignore-compiler-rev
# A dependency that cannot be fetched (`.invalid` is not a host) is a status 3, said, and no program made (cancho only).
rm -rf "$dir"
mkdir -p "$dir/src"
printf '%s' "$main" > "$dir/src/main.cho"
printf '[package]\nname = "demo"\n\n[dependencies.lib]\ngit = "https://example.invalid/lib"\nrev = "0123456789abcdef0123456789abcdef01234567"\n\n[[bin]]\nname = "a"\nsources = ["src"]\n\n[[test]]\nname = "t"\nsources = ["src"]\n' > "$dir/cancho.toml"
for command in build install test; do
    ( cd "$dir" && "$cancho" $command > /dev/null 2>&1; echo $? > s.txt )
    same "$(cat "$dir/s.txt")" 3 "$command with a dependency that cannot be fetched (cancho only)"
    [ ! -e "$dir/build/a" ] || fail "$command made a program"
done
# A path with `..` in it is refused by an `Fs` with a trap; the command line says it cannot read the file and answers 2 (cancho only).
for command in check build fmt test; do
    ( cd "$dir/src" && "$cancho" $command ../src/main.cho > o.txt 2> e.txt; echo $? > s.txt )
    same "$(cat "$dir/src/s.txt")" 2 "$command ../src/main.cho (cancho only)"
    grep -qF '../src/main.cho: cannot read the file' "$dir/src/e.txt" || fail "$command ../src/main.cho did not say it cannot read the file: $(cat "$dir/src/e.txt")"
done
( cd "$dir" && "$cancho" add x > /dev/null 2>&1; echo $? > s.txt )
same "$(cat "$dir/s.txt")" 2 "add with a name and no url (cancho only)"
end

# --- the_command_line_in_cancho_says_its_version_and_checks_the_pin_like_cancho
begin the_command_line_in_cancho_says_its_version_and_checks_the_pin_like_cancho
# A command line in cancho built from these sources with a stamp (`scripts/stamp-version.sh`) says `cancho VERSION (rev REV, host HOST)`: this one is stamped, so its line is its own.
for flag in --version -V; do
    "$cancho" $flag > "$work/v.txt" 2> "$work/v-err.txt"
    same "$?" 0 "$flag status"
    [ ! -s "$work/v-err.txt" ] || fail "$flag wrote to the standard error"
    echo "$flag: $(sed "s|$rev|REV|; s|$host|HOST|" "$work/v.txt")"
done
# build_stamped REV HOST NAME: the compiler in cancho built from the sources with that stamp (`version.cho` replaced) as $work/NAME.
build_stamped() {
    rm -rf "$work/stamped-$3"
    mkdir -p "$work/stamped-$3"
    if [ "$1" = unknown ]; then _vc=compiler/version.cho; else VERSION_CHO=$work/stamped-$3/version.cho sh scripts/stamp-version.sh "$1" "$2" || return 1; _vc=$work/stamped-$3/version.cho; fi
    if [ -n "${CRATE_VERSION:-}" ]; then # a release: the crate version without `-dev`, the first `return` of version.cho
        awk -v v="$CRATE_VERSION" 'done == 0 && /return "/ { sub(/"[^"]*"/, "\"" v "\""); done = 1 } { print }' "$_vc" > "$work/stamped-$3/release.cho" && _vc=$work/stamped-$3/release.cho
    fi
    _files=
    while read -r f; do [ -n "$f" ] || continue; if [ "$f" = version.cho ]; then _files="$_files $_vc"; else _files="$_files compiler/$f"; fi; done < compiler/cancho.files
    "$cancho" build $_files --std -o "$work/$3" > "$work/build-$3.txt" 2>&1 || { fail "building the compiler stamped $1: $(head -c 300 "$work/build-$3.txt")"; return 1; }
}
pin_project() { # PIN
    rm -rf "$work/pin"
    mkdir -p "$work/pin/src"
    printf 'edition 7;\nfn main(world: World) -> [] int { release(world); return 0; }\n' > "$work/pin/src/main.cho"
    printf '[package]\nname = "demo"\ncancho = "%s"\n\n[[bin]]\nname = "a"\nsources = ["src"]\n' "$1" > "$work/pin/cancho.toml"
}
asked() { # COMPILER ARGS...: the status, and the standard error in the Rust test's normal form
    _c=$1
    shift
    ( cd "$work/pin" && "$_c" "$@" > o.txt 2> e.txt; echo $? > s.txt )
    echo "$(cat "$work/pin/s.txt") $(plain "$work/pin/e.txt" "$work/pin")"
}
other=0123456789abcdef0123456789abcdef01234567
pin_project "$other"; echo "pin other: $(asked "$cancho" install)"
pin_project "$other"; echo "pin other, ignored: $(asked "$cancho" install --ignore-compiler-rev)"
# the pin that is this compiler's own commit (the Rust test did not run it for a tree with changes, `-dirty`, and neither does this)
case $rev in
    *-dirty | unknown) echo "pin own: 0 " ;;
    *) pin_project "$rev"; echo "pin own: $(asked "$cancho" install)" ;;
esac
# A pin that is a release version (docs/compiler-pin.md) other than this compiler's own is refused, and `--ignore-compiler-rev` lets it through; a `-dev` version is not a pin at all, whatever tree this is.
pin_project "0.1.0"; echo "pin release, another version: $(asked "$cancho" install)"
pin_project "0.1.0"; echo "pin release, another version, ignored: $(asked "$cancho" install --ignore-compiler-rev)"
pin_project "0.1.1-dev"; echo "pin a dev version: $(asked "$cancho" install)"
for bad in 1.2 1.2.3.4 1..2 .1.2 1.2. 1.2.x v1.2.3 ""; do
    pin_project "$bad"; echo "pin '$bad': $(asked "$cancho" install)"
done
pin_project "$other"
asked "$cancho" install > "$work/pin-refused.txt"
grep -qF "and this compiler is $rev" "$work/pin/e.txt" || fail "the refusal does not say which commit this compiler is: $(cat "$work/pin/e.txt")"
# Stamped with another commit, it is not this one: the line differs, and a project that names this one is refused.
case $rev in
    0*) wrong=1${rev#0} ;;
    *) wrong=0${rev#?} ;;
esac
if build_stamped "$wrong" "$host" wrong; then
    "$work/wrong" --version > "$work/v2.txt"
    [ "$(cat "$work/v2.txt")" != "$version_line" ] || fail "a compiler stamped with another commit says the same line"
    case $rev in
        *-dirty | unknown) ;;
        *) pin_project "$rev"; same "$(asked "$work/wrong" install | sed 's/ .*//')" 1 "a compiler stamped with another commit must refuse a project pinned to this one" ;;
    esac
fi
# A release build (its version has no `-dev`) accepts the pin that names it, and refuses another version.
CRATE_VERSION=0.1.1; built=0; build_stamped "$rev" "$host" released && built=1; CRATE_VERSION=
if [ "$built" = 1 ]; then
    "$work/released" --version | grep -qF 'cancho 0.1.1 (rev' || fail "a release build's line: $("$work/released" --version)"
    pin_project "0.1.1"; same "$(asked "$work/released" install | sed 's/ .*//')" 0 "a release build must accept the pin that names its version"
    pin_project "0.1.2"; r=$(asked "$work/released" install)
    same "${r%% *}" 1 "a release build must refuse another version"
    grep -qF 'and this compiler is 0.1.1' "$work/pin/e.txt" || fail "the refusal for a version does not say this compiler's version: $(cat "$work/pin/e.txt")"
fi
# Unstamped, as the repository carries it: the line says `unknown`, and a pinned project cannot be checked.
if build_stamped unknown unknown plain; then
    version_plain=$("$work/plain" --version)
    same "$version_plain" "$(printf '%s' "$version_line" | sed 's/(rev .*/(rev unknown, host unknown)/')" "an unstamped compiler's line"
    pin_project "$other"
    r=$(asked "$work/plain" install)
    same "${r%% *}" 1 "an unstamped compiler refuses a pinned project"
    grep -qF 'does not know which commit it was built from' "$work/pin/e.txt" || fail "an unstamped compiler's refusal: $(cat "$work/pin/e.txt")"
    r=$(asked "$work/plain" install --ignore-compiler-rev)
    same "${r%% *}" 0 "an unstamped compiler with --ignore-compiler-rev"
    grep -qF 'this is unknown (--ignore-compiler-rev)' "$work/pin/e.txt" || fail "an unstamped compiler's note: $(cat "$work/pin/e.txt")"
fi
end

# --- the_command_line_in_cancho_carries_the_standard_library
begin the_command_line_in_cancho_carries_the_standard_library
rm -rf "$work/std"
mkdir -p "$work/std"
for prog in assert bitwise bytes_library collections crc_check_values flags floating_point math_floats linalg_hand_checked ed25519; do
    file=$root/tests/accept/$prog.cho
    for command in check ids; do
        ( cd "$work/std" && "$cancho" $command "$file" --std > o.txt 2> e.txt; echo $? > s.txt )
        echo "$prog $command --std: exit $(cat "$work/std/s.txt") out $(ck "$work/std/o.txt") err $(ck "$work/std/e.txt")"
    done
    # The directory gives what the binary carries.
    ( cd "$work/std" && "$cancho" ids "$file" --std-dir "$root/std" > o1.txt 2> /dev/null; "$cancho" ids "$file" --std > o2.txt 2> /dev/null )
    cmp -s "$work/std/o1.txt" "$work/std/o2.txt" || fail "$prog: --std-dir std and --std differ"
    # And it builds and does what the Rust build does.
    ( cd "$work/std" && "$cancho" build "$file" --std -o prog > o.txt 2> e.txt; s=$?; echo "$prog build --std: exit $s"; if [ $s = 0 ]; then ./prog > po.txt 2> /dev/null; echo "$prog run: exit $? out $(ck po.txt)"; fi )
done
# A program that imports the library, with no `--std`: refused (status 1).
( cd "$work/std" && "$cancho" check "$root/tests/accept/assert.cho" > /dev/null 2>&1; echo "assert check without --std: exit $?" )
end

# --- the_command_line_in_cancho_says_what_this_build_does_not_carry
begin the_command_line_in_cancho_says_what_this_build_does_not_carry
rm -rf "$work/nc"
mkdir -p "$work/nc"
printf 'edition 7;\nfn main(world: World) -> [] int { release(world); return 3; }\n' > "$work/nc/p.cho"
# `--backend cranelift` and a `--target` that is not the machine's own are said, with where the rest is, and are status 2 (cancho only: the Rust command has them).
for command in build check run test; do
    for flagvalue in '--backend cranelift' '--target wasm32-wasip1' '--target aarch64-unknown-freebsd'; do
        if [ $command = build ]; then extra="-o $work/nc/out"; else extra=; fi
        ( cd "$work/nc" && "$cancho" $command p.cho $flagvalue $extra > o.txt 2> e.txt; echo $? > s.txt )
        same "$(cat "$work/nc/s.txt")" 2 "$command $flagvalue (cancho only)"
        grep -qF "\`$flagvalue\` is not in this build of cancho" "$work/nc/e.txt" && ! grep -qiw 'rust' "$work/nc/e.txt" || fail "$command $flagvalue says: $(head -c 200 "$work/nc/e.txt")"
    done
done
# What it does carry: the LLVM backend, and the machine's own triple (as the Rust command takes it).
for args in "--backend llvm" "--target $host"; do
    rm -f "$work/nc/out"
    ( cd "$work/nc" && "$cancho" build p.cho $args -o out > o.txt 2> e.txt; echo $? > s.txt )
    same "$(cat "$work/nc/s.txt")" 0 "build $args"
    "$work/nc/out"
    same "$?" 3 "the program built with $args answers 3"
done
( cd "$work/nc" && "$cancho" check p.cho --target "$host" > o.txt 2> e.txt; echo $? > s.txt )
same "$(cat "$work/nc/s.txt")" 0 "check --target $host"
end
