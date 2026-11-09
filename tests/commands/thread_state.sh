# thread_state.sh -- the script of tests/commands/script_tests.cho for `conformance/thread_state.rs` (deleted): the scenarios are tests/commands/thread_state.py, driven by tests/support/sk.py. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
python3 tests/commands/thread_state.py "$1" "$2" 2>&1
