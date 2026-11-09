# process_release.sh -- the script of tests/commands/script_tests.cho for `conformance/process_release.rs` (deleted): letting go of a child (docs/processes.md 4.11), over real processes; the driver is
# tests/commands/process_release.py (python3; it reads the children it left behind with `ps`). $1 is the compiler, $2 a scratch directory; run from the root of the repository.
if ! command -v python3 > /dev/null 2>&1; then
    echo "FAILS python3 is not installed"
    exit 0
fi
python3 tests/commands/process_release.py "$1" "$2"
