# http_server_upload.sh -- the script of tests/commands/script_tests.cho for `conformance/http_server_upload.rs` (deleted): the scenarios are tests/commands/http_server_upload.py, driven by the scripted client of tests/support/sk.py. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
python3 tests/commands/http_server_upload.py "$1" "$2" 2>&1
