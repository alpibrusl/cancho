# vcs_lib.sh -- what the four scripts of `cancho vcs` over a store (vcs_publish.sh, vcs_remote.sh, vcs_std.sh, vcs_sync.sh) share. Not a test of its own: a script sources it after
# tests/commands/script_lib.sh, with cancho=$1 (an absolute path) and work=$2 set:   . tests/commands/script_lib.sh; . tests/commands/vcs_lib.sh
#
# `r ARGS...` runs the compiler and leaves what it did in $out (standard output), $err (standard error) and $code (the exit status), the trailing newlines of the two texts taken off.

r() {
    "$cancho" "$@" > "$work/r.out" 2> "$work/r.err"
    code=$?
    out=$(cat "$work/r.out")
    err=$(cat "$work/r.err")
}
# succeeds WHAT: the last `r` exited 0.  exits N WHAT: it exited N.
succeeds() { [ "$code" = 0 ] || fail "$1: exit $code, said: $(printf '%s' "$err" | head -c 300)"; }
exits() { [ "$code" = "$1" ] || fail "$2: exit $code, wanted $1; said: $(printf '%s%s' "$out" "$err" | head -c 300)"; }
# has TEXT NEEDLE WHAT: TEXT holds NEEDLE; hasnt: it does not.
has() { case $1 in *"$2"*) ;; *) fail "$3: '$2' is not in '$(printf '%s' "$1" | head -c 300)'" ;; esac; }
hasnt() { case $1 in *"$2"*) fail "$3: '$2' is in '$(printf '%s' "$1" | head -c 300)'" ;; esac; }
# count_in DIR: the number of entries of a directory (0 if it is not there).
count_in() { if [ -d "$1" ]; then ls -A "$1" | wc -l | tr -d ' '; else echo 0; fi; }

# A clean directory for one check: $d.
fresh() { d=$work/$1; rm -rf "$d"; mkdir -p "$d"; }

# The two sources most checks publish: two functions in one file.
two_functions() { printf 'fn add(a: int, b: int) -> [] int { return a + b; }\nfn sub(a: int, b: int) -> [] int { return a - b; }\n' > "$1"; }

# publish STORE FILE [FLAGS...]: a publish that must succeed.
publish() { _s=$1; _f=$2; shift 2; r vcs publish --store "$_s" "$@" "$_f"; succeeds "publish $_f"; }
# lock STORE OUT NAMES...: a lock that must succeed.
lock() { _s=$1; _o=$2; shift 2; r vcs lock --store "$_s" -o "$_o" "$@"; succeeds "lock $*"; }
# fetch LOCK STORE OUT
fetch() { r vcs fetch --lock "$1" --store "$2" -o "$3"; }

# editjson FILE NAME KEY NEW: in a manifest, after the first line that names `"name": "NAME"`, the first `"KEY": "<hex>"` becomes `"KEY": "NEW"`, and nothing else changes.
editjson() {
    awk -v name="$2" -v key="$3" -v new="$4" '
        !armed && index($0, "\"name\": \"" name "\"") { armed = 1 }
        armed && !done && match($0, "\"" key "\": \"[0-9a-f]+\"") {
            $0 = substr($0, 1, RSTART - 1) "\"" key "\": \"" new "\"" substr($0, RSTART + RLENGTH); done = 1
        }
        { print }' "$1" > "$1.new" && mv "$1.new" "$1"
}
# field FILE NAME KEY: the hex value of `"KEY"` after the line that names `"name": "NAME"`.
field() {
    awk -v name="$2" -v key="$3" '
        !armed && index($0, "\"name\": \"" name "\"") { armed = 1 }
        armed && match($0, "\"" key "\": \"[0-9a-f]+\"") { s = substr($0, RSTART, RLENGTH); sub(/^[^:]*: "/, "", s); sub(/"$/, "", s); print s; exit }' "$1"
}

# Build and run a consumer: consumer SOURCE EXE FILES...: builds SOURCE and the files, runs the program, and leaves its exit status in $code.
consume() {
    _src=$1; _exe=$2; shift 2
    r build "$_src" "$@" -o "$_exe"
    if [ "$code" != 0 ]; then fail "the consumer did not build: $(printf '%s' "$err" | head -c 300)"; return 1; fi
    "$_exe" > /dev/null 2>&1
    code=$?
}

# either TEXT A B WHAT: TEXT holds A or B. The compiler written in Rust said a refusal in its own words and the one in cancho says the sentence of the rule (docs/gate.md G3), so a check that
# looked for the Rust words accepts both.
either() { case $1 in *"$2"*) ;; *"$3"*) ;; *) fail "$4: neither '$2' nor '$3' is in '$(printf '%s' "$1" | head -c 300)'" ;; esac; }
