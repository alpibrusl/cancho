# foreign_authority.sh -- the script of tests/commands/script_tests.cho for `conformance/foreign_authority.rs` (deleted): docs/foreign-authority.md, what a program that calls foreign code can reach, said
# symbol by symbol. Real programs, built and run, and the authority report read as text and as JSON. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
# One backend (LLVM) and one compiler: the Cranelift leg of the Rust test went with D2. The statx reproducer is Linux only, as it was.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
root=$(pwd)
work=$2
cd "$work" || exit 1

has() { if grep -qF -- "$2" "$1"; then echo "ok $3"; else echo "FAILS $3"; fi; }
hasnt() { if grep -qF -- "$2" "$1"; then echo "FAILS $3"; else echo "ok $3"; fi; }
# The exit status of the program: a build that fails says so.
run() { "$cancho" run "$1" --std > run.out 2> run.err; echo "exit $?"; }
# The rule of the first refusal of $1, from `check --output json`. The second argument is what the Rust checker's message said besides the rule; the compiler in cancho says one line per rule, so
# it is not compared (docs/gate.md, wave 2). A program that is not refused at all is a GAP: the Rust checker refused it, the one in cancho accepts it.
refusal() { # FILE WHY
    "$cancho" check "$1" --std --output json > check.json 2> /dev/null
    rule=$(sed -n 's/.*"rule": "\([^"]*\)".*/\1/p' check.json | head -1)
    if [ -n "$rule" ]; then echo "rule $rule"; else echo "GAP: accepted, the Rust checker refused it ($2)"; fi
}

cat > one.cho << 'EOF'
extern fn getpid[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] c_int;

fn alive[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] bool {
    return getpid(ffi) > 0;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    let libc = narrow(ffi, "libc");
    var status = 1;
    borrow libc as &f in {
        if alive(f) { status = 0; }
    }
    release(libc);
    return status;
}
EOF

# Two libraries held by one capability: `labs` is libc's, `pthread_self` is declared under `libpthread` (glibc folded it into libc in 2.34, so the scope is the claim and the symbol is the fact).
cat > two.cho << 'EOF'
edition 5;
extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;
extern fn pthread_self[&f](ffi: &f Ffi("libpthread")) -> [ffi("libpthread")] int;

fn magnitude[&f](ffi: &f Ffi("libc,libpthread"), n: int) -> [ffi("libc")] int {
    return labs(ffi, n);
}

fn thread[&f](ffi: &f Ffi("libc,libpthread")) -> [ffi("libpthread")] int {
    return pthread_self(ffi);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(fs); release(heap); release(args); release(net); release(clock);
    let native = narrow(ffi, "libpthread,libc");
    var status = 0;
    borrow native as &f in {
        status = magnitude(f, 0 - 7);
        if thread(f) == 0 { status = 1; }
    }
    release(native);
    return status;
}
EOF

echo "== no foreign code: bounded, and unbounded by nothing"
"$cancho" authority "$root/examples/cut/cut.cho" --std --output json > cut.json
head -3 cut.json
has cut.json '"foreign_symbols": [],' "the symbol list is empty"
"$cancho" authority "$root/examples/cut/cut.cho" --std > cut.txt
hasnt cut.txt UNBOUNDED "no UNBOUNDED"
hasnt cut.txt "unbounded by" "no unbounded-by section"
has cut.txt "foreign code" "it says it never touches foreign code"

echo "== one libc symbol is named exactly"
"$cancho" authority one.cho --std --output json
"$cancho" authority one.cho --std
run one.cho

echo "== two libraries are attributed symbol by symbol"
"$cancho" authority two.cho --std --output json > two.json
head -6 two.json
has two.json '{ "name": "ffi", "argument": "libc", "bounded": false },' "libc label"
has two.json '{ "name": "ffi", "argument": "libpthread", "bounded": false }' "libpthread label"
has two.json '"foreign_symbols": ["labs", "pthread_self"],' "both symbols, sorted"
"$cancho" authority two.cho --std | sed -n '/^unbounded by/,$p'
run two.cho

# A declaration alone is not reach (docs/foreign-authority.md section 6): the program does not even need the library to exist.
echo "== a declaration nobody calls is not reach"
cat > declared.cho << 'EOF'
extern fn getpid[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] c_int;
extern fn no_such_symbol_anywhere[&f](ffi: &f Ffi("libnowhere")) -> [ffi("libnowhere")] c_int;

fn unreached[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    return getpid(ffi);
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return 0;
}
EOF
"$cancho" authority declared.cho --std --output json > declared.json
head -3 declared.json
has declared.json '"foreign_symbols": [],' "no symbol is reported"
# Linked and run: an unused import is not an undefined reference.
run declared.cho

# A CI pin of the report shows a diff when a symbol is added, and only that.
echo "== adding a symbol changes the pin by exactly that symbol"
sed -e 's/^fn alive/extern fn getuid[\&f](ffi: \&f Ffi("libc")) -> [ffi("libc")] c_int;\
\
fn alive/' -e 's/return getpid(ffi) > 0;/return getpid(ffi) > 0 \&\& getuid(ffi) >= 0;/' one.cho > after.cho
"$cancho" authority one.cho --std --output json | sed -n '/^  "unbounded_by"/,/^  "effects"/p' | sed '$d'
"$cancho" authority after.cho --std --output json | sed -n '/^  "unbounded_by"/,/^  "effects"/p' | sed '$d'

# The scope is a claim and the symbol is the fact: a libc symbol declared under another library's scope is reported under that scope.
echo "== the scope is a claim and the symbol is the fact"
cat > claim.cho << 'EOF'
extern fn system[&f, &c](ffi: &f Ffi("openssl"), command: &c [byte]) -> [ffi("openssl")] c_int;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    let ssl = narrow(ffi, "openssl");
    var status = 1;
    borrow ssl as &f in { status = system(f, "exit 0\0"); }
    release(ssl);
    return status;
}
EOF
"$cancho" authority claim.cho --std --output json | sed -n '/"unbounded_by"/,/\],/p'
run claim.cho

# Measured before the fix: `system` declared with no capability and the row `[]` ran a shell while the report said bounded. A foreign function is reached through exactly one `Ffi`.
echo "== a foreign function without a capability is refused"
cat > main_tail.cho << 'EOF'
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return 0;
}
EOF
declared() { # DECLARATION WHY
    { printf '%s\n' "$1"; cat main_tail.cho; } > refused.cho
    refusal refused.cho "$2"
}
declared 'extern fn system[&c](command: &c [byte]) -> [] c_int;' 'borrows 0 `Ffi` capabilities'
declared 'extern fn getpid() -> [] c_int;' 'borrows 0 `Ffi` capabilities'
declared 'extern fn tick[&i](io: &!i Io) -> [err_write, io_read, io_write] c_int;' 'borrows 0 `Ffi` capabilities'
declared 'extern fn f[&a, &b](x: &a Ffi("libc"), y: &b Ffi("libm")) -> [ffi("libc"), ffi("libm")] int;' 'borrows 2 `Ffi` capabilities'
declared 'extern fn f[&a](x: &a Ffi("libc,libm")) -> [ffi("libc,libm")] int;' 'names several libraries'

# A `main` that narrows $1 and does nothing, around the declarations $2.
narrowing_to() { # SCOPE DECLARATIONS
    {
        printf '%s\n' "$2"
        cat << EOF
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    let held = narrow(ffi, "$1");
    release(held);
    return 0;
}
EOF
    } > refused.cho
}
echo "== a scope is narrowed as a set, and every refusal has its rule"
for row in "libc,|empty library name" "libc,,libm|empty library name" "libc,libc|named twice" "libc libm|not a library name" "libc:statx|not a library name" "lib/c|not a library name" "|to itself"; do
    narrowing_to "${row%%|*}" ""
    echo "scope '${row%%|*}':"
    refusal refused.cho "${row#*|}"
done
# Widening, in a program that holds a set and asks for more, for a prefix of what it holds, and for the root.
held() { # FIRST THEN
    cat > refused.cho << EOF
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    let one = narrow(ffi, "$1");
    let two = narrow(one, "$2");
    release(two);
    return 0;
}
EOF
}
for row in "libc|libc,libm|never widened" "libc,libm|libssl|never widened" "libc|libcrypto|never widened" "libcrypto|libc|never widened" "libc||never widened" "libc,libm|libm,libc|to itself"; do
    first=${row%%|*}
    rest=${row#*|}
    held "$first" "${rest%%|*}"
    echo "$first -> ${rest%%|*}:"
    refusal refused.cho "${rest#*|}"
done
# A scope written in a type is checked where it is written.
narrowing_to libc 'fn f[&a](x: &a Ffi("libc,,libm")) -> [] int { return 0; }'
echo "a scope in a type:"
refusal refused.cho ""

# Lending a capability narrower is accepted; lending what it lacks, or the unnarrowed root, is a type error.
echo "== a capability is lent narrower and never wider"
program() { # LEND WANTS
    cat << EOF
extern fn labs[&f](ffi: &f Ffi("libc"), n: int) -> [ffi("libc")] int;
fn helper[&f](ffi: &f Ffi("$2")) -> [] int { return 0; }
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    $1
    return 0;
}
EOF
}
for row in "libc,libm|libc|accepted" "libm,libc|libc,libm|accepted" "libc|libm|refused" "libc|libc,libm|refused" "libc,libm|libssl|refused" "libc,libm,libssl|libm,libssl|accepted"; do
    held=${row%%|*}
    rest=${row#*|}
    program "let c = narrow(ffi, \"$held\"); borrow c as &f in { helper(f); } release(c);" "${rest%%|*}" > lend.cho
    if "$cancho" check lend.cho --std > /dev/null 2>&1; then got=accepted; else got=refused; fi
    if [ "$got" = "${rest#*|}" ]; then echo "$held lent to ${rest%%|*}: $got"; else echo "$held lent to ${rest%%|*}: GAP: $got, the Rust checker said ${rest#*|}"; fi
done
# The root is lent only after it is narrowed.
program "borrow ffi as &f in { helper(f); }" libc > root.cho
"$cancho" check root.cho --std --output json > root.json 2> /dev/null
rule=$(sed -n 's/.*"rule": "\([^"]*\)".*/\1/p' root.json | head -1)
if [ "$rule" = type-mismatch ]; then echo "rule type-mismatch"; else echo "GAP: the root is lent and the rule is '$rule', the Rust checker said type-mismatch"; fi

# Owning a set discharges each of its libraries, and a row that names a library the function does not call is still refused.
echo "== rows stay exact per library"
sed 's/-> \[ffi("libc")\] int {$/-> [ffi("libc"), ffi("libpthread")] int {/' two.cho > wrong.cho
cmp -s two.cho wrong.cho && echo "FAILS the edit changed nothing"
refusal wrong.cho ""
sed 's/^fn thread\(.*\)-> \[ffi("libpthread")\] int {$/fn thread\1-> [ffi("libc")] int {/' two.cho > swapped.cho
cmp -s two.cho swapped.cho && echo "FAILS the edit changed nothing"
refusal swapped.cho ""

# A slice crosses as a pointer and a length, so statx(AT_FDCWD, path, flags, mask, buf) cannot be declared as C has it. Linux only: macOS's libc has no such symbol.
echo "== a path cannot be passed to statx as C has it"
if [ "$(uname -s)" = Linux ]; then
    cat > statx.cho << 'EOF'
extern fn statx[&f, &p, &b](ffi: &f Ffi("libc"), dirfd: int, path: &p [byte], flags: int, mask: int, buf: &b [byte]) -> [ffi("libc")] c_int;
fn mode[&f](ffi: &f Ffi("libc")) -> [ffi("libc")] int {
    region a {
        let buf = alloc_slice[a](256, byte_of(0));
        return statx(ffi, 0 - 100, "/etc/passwd\0", 0, 4, buf);
    }
}
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(fs); release(heap); release(args);
    let libc = narrow(ffi, "libc");
    var r = 0;
    borrow libc as &f in { r = mode(f); }
    release(libc);
    if r == 0 { return 0; }
    return 1;
}
EOF
    run statx.cho
else
    echo "exit 1"
fi

# A string literal has six escapes and no `\x`, so a byte such as 0xFF cannot be written in one.
echo "== a string literal has no hex escape"
cat > hex.cho << 'EOF'
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    let text = "a\x41";
    return len(text);
}
EOF
refusal hex.cho ""
exit 0
