# regen.sh -- the script of tests/commands/script_tests.cho for `conformance/selfhost/statics.rs` (deleted). The modules of the command line that hold text are written by
# compiler/regen.cho (docs/removing-rust.md phase 2): `guidelines.cho` is `AGENTS.md`, `stdorder.cho` is `std/std.files`, `stdsource.cho` is `std/*.cho`. Built by the compiler under
# test and run from the root of the repository with `--check`, it writes nothing and exits 1 for a module that is not what the sources make it. The four Rust tests of statics.rs
# (`guidelines_module_is_the_text_of_agents_md`, `std_order_module_is_the_std_table_of_cancho`, `std_source_module_is_the_std_directory`, `command_tree_module_is_what_cancho_says`) generated
# the modules in Rust and compared; the first three are this check, and the fourth (the texts the Rust binary said for `--help`, `introspect` and `skill`) has no oracle left: `commandtree.cho`
# is the record of what it said, and the compiler in cancho's own answers are compared with the Rust binary's by the command-line tests (cli.rs) while there is one.
# $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh

begin regen_in_cancho_makes_the_modules_the_rust_generators_make
if "$cancho" build compiler/regen.cho compiler/filewrite.cho --std -o "$work/regen" > "$work/build.txt" 2>&1; then
    "$work/regen" --check > "$work/check.txt" 2>&1
    same "$?" 0 "regen --check ($(head -c 300 "$work/check.txt"))"
else
    fail "regen.cho does not build: $(head -c 300 "$work/build.txt")"
fi
end
