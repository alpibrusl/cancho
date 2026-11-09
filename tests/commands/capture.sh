# capture.sh -- the script of tests/commands/script_tests.cho for `conformance/capture.rs` (deleted): `std.process` (docs/processes.md 7.1, 7.2), a child's output captured with a bound and a deadline; the driver is
# tests/commands/capture.py (python3; it needs `wait4`'s account of processor time, which a shell cannot read). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
if ! command -v python3 > /dev/null 2>&1; then
    echo "FAILS python3 is not installed"
    exit 0
fi
python3 tests/commands/capture.py "$1" "$2"
