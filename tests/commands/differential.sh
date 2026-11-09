# differential.sh -- the script of tests/commands/script_tests.cho for `conformance/differential.rs` (deleted): the constant folder against the compiled program (docs/differential.md). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
python3 tests/commands/differential.py "$1" "$2"
