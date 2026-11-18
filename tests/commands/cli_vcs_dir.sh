# cli_vcs_dir.sh -- the script of tests/commands/script_tests.cho that was the two `vcs_publish_dir_in_cancho_*` tests of crates/cancho/tests/conformance/selfhost/vcs.rs (docs/gate.md G4, wave 4): `vcs publish --dir` on a
# chain and a diamond of modules published in the order of their imports (with a program, a text file and a directory beside them that are not), the same published again over the stores it made, after a body
# changed, and over a store with a stranger in it; a module that imports the library; modules that import each other, themselves, or a module that is not there; two files that declare one module, one that
# declares two, one that does not parse, none that declares a module, no `.cho` file, a directory that is not there and one that is a file; then the real libraries `packages/x509` and `packages/http-client`,
# each with the library. NAME.expected is what the Rust command did, recorded on 2026-11-17 (see tests/commands/vcs_diff_lib.sh). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_diff_lib.sh

# dirmod PATH NAME IMPORTS BODY: a module file: its name, an import line each, a blank line and the body.
dirmod() {
    mkdir -p "$(dirname "$d/$1")"
    { printf 'module %s;\n\n' "$2"; for _i in $3; do printf 'import %s;\n' "$_i"; done; printf '\n%s\n' "$4"; } > "$d/$1"
}
ONE=$(printf 'pub fn one(n: int) -> [] int {\n    return n;\n}')
TWO=$(printf 'pub fn two(n: int) -> [] int {\n    return base.one(n) + 1;\n}')
THREE=$(printf 'pub fn three(n: int) -> [] int {\n    return mid.two(n) + base.one(n);\n}')
chain() {
    dirmod lib/base.cho lib.base '' "$ONE"
    dirmod lib/mid.cho lib.mid lib.base "$TWO"
    dirmod lib/top.cho lib.top 'lib.mid lib.base' "$THREE"
    put lib/prog.cho "$(printf 'fn main() -> [] int {\n    return 0;\n}')
"
    put lib/notes.txt 'not a source'
    dirmod lib/sub/deep.cho lib.deep '' "$ONE"
}
dir() { vstep publish --dir lib --store st "$@"; }
main_only="$(printf 'fn main() -> [] int {\n    return 0;\n}')
"

begin vcs_publish_dir_in_cancho_publishes_a_library_like_cancho_vcs_publish
dcase "a chain"; chain; dir; dend
dcase "a chain, again, and a body changed"; chain; dir; dir; dirmod lib/base.cho lib.base '' "$(printf '%s' "$ONE" | sed 's/return n;/return n + 5;/')"; dir; dend
dcase "a store with a stranger in it"; chain; dir; put st/lib.base/stranger.txt left; dir; dend
dcase "a store that is a file"; chain; put st/lib.base 'a file'; dir; dend
dcase "a diamond"
dirmod lib/a.cho lib.a '' "$ONE"
dirmod lib/b.cho lib.b lib.a "$(printf '%s' "$TWO" | sed 's/base\./a./g')"
dirmod lib/c.cho lib.c lib.a "$(printf '%s' "$THREE" | sed -e 's/mid\.two/a.one/g' -e 's/base\./a./g')"
dirmod lib/d.cho lib.d 'lib.b lib.c' "$(printf 'pub fn four(n: int) -> [] int {\n    return b.two(n) + c.three(n);\n}')"
dir; dend
dcase "the library"
dirmod lib/s.cho lib.s std.bytes "$(printf 'pub fn same[&a, &b](x: &a [byte], y: &b [byte]) -> [] bool {\n    return bytes.equal(x, y);\n}')"
dir; dir --std; dir --std; dend
dcase "modules that import each other"; dirmod lib/a.cho lib.a lib.b "$ONE"; dirmod lib/b.cho lib.b lib.c "$ONE"; dirmod lib/c.cho lib.c lib.a "$ONE"; dir; dend
dcase "a module that imports itself"; dirmod lib/a.cho lib.a lib.a "$ONE"; dir; dend
dcase "a module that sorts before what it imports"; dirmod lib/a.cho lib.aaa 'lib.mmm lib.zzz' "$ONE"; dirmod lib/m.cho lib.mmm '' "$ONE"; dirmod lib/z.cho lib.zzz '' "$ONE"; dir; dend
dcase "a module that is not there"; dirmod lib/a.cho lib.a lib.gone "$ONE"; dir; dend
dcase "one module in two files"; dirmod lib/a.cho lib.a '' "$ONE"; dirmod lib/b.cho lib.a '' "$ONE"; dir; dend
dcase "two modules in a file"; dirmod lib/a.cho lib.a '' "$ONE"; dirmod lib/b.cho lib.b '' "$ONE"; cat "$d/lib/a.cho" "$d/lib/b.cho" > "$work/two-modules.cho"; putf lib/a.cho "$work/two-modules.cho"; rm "$d/lib/b.cho"; dir; dend
dcase "a file that does not parse"; putp lib/a.cho 'module lib.a;\nfn broken('; dir; dend
dcase "only programs"; put lib/a.cho "$main_only"; dir; dend
dcase "no source"; put lib/notes.txt x; dir; dend
dcase "a directory that is not there"; dir; dend
dcase "a directory that is a file"; put lib 'a file'; dir; dend
dcase "the default store"; chain; vstep publish --dir lib; vstep publish --dir lib; dend
end

begin vcs_publish_dir_in_cancho_publishes_the_libraries_of_packages
for library in x509 http-client; do
    dcase "$library"
    copytree "$packages/$library" "$d/lib"
    vstep publish --dir lib --store st --std
    dend lib
done
end
