# vector_lib.sh -- what the scripts that replay a recorded vector file through a driver share (wide_multiply.sh, sha256_builtins.sh, crypto_builtins.sh, floats.sh, ...). Not a test of its own: a
# script sources it after tests/commands/script_lib.sh, which gives `begin`, `fail`, `end`, `same` and `build`:   . tests/commands/vector_lib.sh
#
# The recorded files live in tests/vectors/: NAME.txt is what the driver is fed (a line a case), NAME.expected the answer line of each case, written once by the Rust test the script replaced
# (docs/gate.md, G4 wave 4), whose oracle (Rust's u128, a reference AES, f64::exp, ...) is not in cancho.

# lines FILE: the number of lines of FILE.
lines() { wc -l < "$1" | tr -d ' '; }

# agrees DRIVER INPUT WANTED WHAT: DRIVER, fed INPUT, exits 0 and answers WANTED, line for line; otherwise fails the check with the first three differing lines.
agrees() {
    "$1" < "$2" > "$work/answers.txt" 2> "$work/answers.err"
    _status=$?
    same "$_status" 0 "$4: the driver's exit status ($(head -c 200 "$work/answers.err"))"
    same "$(lines "$work/answers.txt")" "$(lines "$3")" "$4: the number of answers"
    if ! cmp -s "$work/answers.txt" "$3"; then
        fail "$4: the answers differ from $3, first three lines: $(diff "$work/answers.txt" "$3" | grep '^[<>]' | head -3 | cut -c1-150 | tr '\n' '|')"
    fi
}

# traps DRIVER INPUT WHAT: DRIVER, fed the one line INPUT, is ended abnormally (not exit 0) and answers nothing.
traps() {
    printf '%s\n' "$2" > "$work/trap.in"
    { "$1" < "$work/trap.in" > "$work/trap.out" 2> /dev/null; } 2> /dev/null
    _status=$?
    [ "$_status" != 0 ] || fail "$3: '$(printf '%s' "$2" | cut -c1-60)' did not trap"
    [ ! -s "$work/trap.out" ] || fail "$3: '$(printf '%s' "$2" | cut -c1-60)' answered before it trapped"
}

# answers DRIVER INPUT: what the driver, fed the one line INPUT, answers.
answers() {
    printf '%s\n' "$2" > "$work/ask.in"
    { "$1" < "$work/ask.in" 2> /dev/null; } 2> /dev/null
}
