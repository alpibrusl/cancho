# script_lib.sh -- what the longer shell scripts of tests/commands share (signals.sh, process_release.sh, capture.sh). Not a test of its own: a script sources it after setting
#     cancho=$1 (the compiler under test)  work=$2 (a scratch directory)
# and run from the root of the repository:   . tests/commands/script_lib.sh
#
# A check is `begin NAME`, the steps, `end`: `end` prints `ok NAME`, preceded by a `GAP:` line for each pinned difference from the Rust compiler the check was written against; a step that
# does not hold calls `fail WHY`, which prints `FAILS NAME: WHY` (and `end` then prints nothing). The transcript is what tests/commands/script_tests.cho compares with NAME.expected.

bad=0
gaps=
begin() { name=$1; bad=0; gaps=; }
fail() { bad=1; echo "FAILS $name: $*"; }
end() { if [ "$bad" = 0 ]; then printf '%s' "$gaps"; echo "ok $name"; fi; }
# same GOT WANT WHAT: the two strings are the same.
same() { [ "$1" = "$2" ] || fail "$3: got '$1', wanted '$2'"; }
# gap CASE GOT WANTED: a pinned difference, reported as a GAP line.
gap() { gaps="${gaps}GAP: $1: the Rust compiler $3, the compiler in cancho $2
"; }

# build SOURCE NAME: the program SOURCE built (with --std) by the compiler under test, as $work/NAME; fails the check if it does not build.
build() {
    if ! "$cancho" build "$1" --std -o "$work/$2" > "$work/build-$2.txt" 2>&1; then
        fail "building $1 said: $(head -c 300 "$work/build-$2.txt")"
        return 1
    fi
}

# sleep_ms is `sleep` with a fraction of a second, which both GNU and BSD `sleep` take.
now() { date +%s; }

# A running program: its standard error in $sd/err, its standard input a fifo held open on descriptor 3 (one byte a command), its process id in $pid.
runs=0
# start EXE [SIGNAL-IGNORED-ON-ENTRY]
start() {
    runs=$((runs + 1))
    sd=$work/run$runs
    mkdir "$sd"
    mkfifo "$sd/in"
    : > "$sd/err"
    exec 3<> "$sd/in"
    if [ -n "$2" ]; then
        sh -c "trap '' $2; exec \"\$0\"" "$1" < "$sd/in" 2> "$sd/err" 3>&- &
    else
        "$1" < "$sd/in" 2> "$sd/err" 3>&- &
    fi
    pid=$!
    nline=0
}
# next: waits for the next line the program wrote (20 seconds at most) and leaves it in $got.
next() {
    nline=$((nline + 1))
    tries=0
    while [ "$(wc -l < "$sd/err" | tr -d ' ')" -lt "$nline" ]; do
        tries=$((tries + 1))
        if [ "$tries" -gt 400 ]; then got="(nothing for 20 seconds)"; return 1; fi
        sleep 0.05
    done
    got=$(sed -n "${nline}p" "$sd/err")
}
# expect LINE: the next line is LINE.
expect() { next; same "$got" "$1" "the program said"; }
# cmd BYTE: one byte on the program's standard input.
cmd() { printf '%s' "$1" >&3; }
# sig NAME: a signal to the program.
sig() { kill -"$1" "$pid"; }
alive() { _s=$(ps -o stat= -p "$pid" 2> /dev/null); case $_s in '' | Z*) return 1 ;; *) return 0 ;; esac; }
# finish: close its standard input and wait for the end, however it comes (20 seconds at most); the status is in $status (128 + n if a signal n ended it).
finish() {
    exec 3>&-
    ( sleep 20; kill -KILL "$pid" ) > /dev/null 2>&1 &
    _wd=$!
    wait "$pid"
    status=$?
    kill "$_wd" > /dev/null 2>&1
    wait "$_wd" > /dev/null 2>&1
    return 0
}
# reap: whatever happened, the program is not left behind.
reap() { exec 3>&-; kill -KILL "$pid" > /dev/null 2>&1; wait "$pid" > /dev/null 2>&1; return 0; }
# ended_by NAME: finish, and the program was ended by that signal.
ended_by() { finish; same "$status" "$((128 + $(signal_number "$1")))" "the program was ended by $1"; }

# The signal numbers of this machine (macOS and Linux differ for USR1 and USR2).
signal_number() {
    case "$1" in
        HUP) echo 1 ;; INT) echo 2 ;; QUIT) echo 3 ;; ALRM) echo 14 ;; TERM) echo 15 ;; WINCH) echo 28 ;;
        USR1) if [ "$(uname -s)" = Darwin ]; then echo 30; else echo 10; fi ;;
        USR2) if [ "$(uname -s)" = Darwin ]; then echo 31; else echo 12; fi ;;
    esac
}

# rule_of: the source in $work/src.cho is checked (--std); prints the rule the first refusal is under, `accepted` if there is none.
rule_of() {
    _out=$("$cancho" check "$work/src.cho" --std --output json 2> /dev/null)
    if [ $? = 0 ]; then echo accepted; return 0; fi
    _r=$(printf '%s\n' "$_out" | sed -n 's/^ *"rule": "\([^"]*\)".*/\1/p' | head -1)
    echo "${_r:-refused}"
}
