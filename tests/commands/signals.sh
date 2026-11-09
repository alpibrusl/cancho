# signals.sh -- the script of tests/commands/script_tests.cho for `conformance/signals.rs` (deleted): the signal capability (docs/signals.md), over real processes. The programs are tests/programs/signals_*.cho; the driver, which
# builds them with the compiler ($1), sends the signals and reads what they say, is tests/commands/signals.py (python3, as `refusals` and `release` already need it). $2 is a scratch directory; run from the root of the repository.
if ! command -v python3 > /dev/null 2>&1; then
    echo "FAILS python3 is not installed"
    exit 0
fi
PYTHONPATH=tests/commands python3 tests/commands/signals.py "$1" "$2"
