# api.sh -- the script of tests/commands/script_tests.cho for `conformance/api.rs` (deleted): the scenarios are tests/commands/api.py, driven by the scripted client of tests/support/sk.py. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
python3 tests/commands/api.py "$1" "$2" 2>&1
