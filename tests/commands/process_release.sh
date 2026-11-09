# process_release.sh -- the script of tests/commands/script_tests.cho for `conformance/process_release.rs` (deleted): letting go of a child (docs/processes.md 4.11), `exec_spawn_detached`, `child_pid` and `child_release`,
# over real processes. The driver is tests/programs/process_release.cho, built here by the compiler ($1); what is looked at is outside it: the state, process group and session of the children it left behind, read with `ps`.
# $2 is a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh

# col PID COLUMN: one `ps` column of a process, nothing once the process is gone.
col() { ps -o "$2=" -p "$1" 2> /dev/null | tr -d ' '; }
# leads_session PID: `s` in its `ps` state on both kernels (macOS's `ps` has no session id to compare; Linux's is compared as well).
leads_session() { case $(col "$1" stat) in *s*) return 0 ;; *) return 1 ;; esac; }
# running PID: there, and not a zombie waiting for a parent to reap it.
running() { case $(col "$1" stat) in '' | Z*) return 1 ;; *) return 0 ;; esac; }
# ends PID SECONDS: not running within SECONDS (gone, or a zombie).
ends() {
    _i=0
    while [ "$_i" -lt $(($2 * 20)) ]; do
        running "$1" || return 0
        _i=$((_i + 1)); sleep 0.05
    done
    return 1
}
# kill_group GROUP: a whole process group signalled, as a supervisor or a terminal does (the `kill` program: dash's builtin has no `--`).
kill_group() { /bin/kill -KILL -- "-$1" > /dev/null 2>&1 || fail "group $1 should be there to signal"; }
# pids_of FILE: every `pid N` line of the driver's output, in order, as N.
pids_of() { sed -n 's/^pid //p' "$1"; }

# startd ARGS...: the driver in a process group of its own with its standard input open (a fifo held on descriptor 3), its output in $sd/out, read to its `ready` line. Leaves $pid (also its group).
startd() {
    runs=$((runs + 1)); sd=$work/run$runs
    mkdir "$sd"; mkfifo "$sd/in"; : > "$sd/out"
    exec 3<> "$sd/in"
    set -m
    "$work/release" "$@" < "$sd/in" > "$sd/out" 3>&- &
    pid=$!
    set +m
    _i=0
    until grep -q '^ready$' "$sd/out"; do
        _i=$((_i + 1))
        if [ "$_i" -gt 400 ] || ! running "$pid"; then fail "the driver ended or said nothing without 'ready': $(cat "$sd/out")"; return 1; fi
        sleep 0.05
    done
}
# stopd: its standard input closed and the driver waited for; the status is in $status.
stopd() { exec 3>&-; wait "$pid"; status=$?; }

build tests/programs/process_release.cho release

begin "a detached child outlives its parent in a session of its own"
begun=$(now)
"$work/release" d > "$work/d.out" 2> /dev/null < /dev/null; st=$?
same "$st" 0 "the driver returns at once"
[ "$(($(now) - begun))" -le 20 ] || fail "it must not wait for the child"
grep -q '^released 0$' "$work/d.out" || fail "released 0 missing: $(cat "$work/d.out")"
child=$(pids_of "$work/d.out" | head -1)
running "$child" || fail "the child is still running after its parent ended"
same "$(col "$child" pgid)" "$child" "it leads its group"
leads_session "$child" || fail "it leads a session"
[ "$(uname -s)" != Linux ] || same "$(col "$child" sid)" "$child" "its session is its own pid"
kill_group "$child"
ends "$child" 10 || fail "the group signal reaches the child"
end

begin "only the detached child survives its parent's group"
startd b
detached=$(pids_of "$sd/out" | sed -n 1p)
plain=$(pids_of "$sd/out" | sed -n 2p)
group=$pid
leads_session "$detached" || fail "a session of its own"
! leads_session "$plain" || fail "the plain child is in its parent's"
if [ "$(uname -s)" = Linux ]; then
    [ "$(col "$detached" sid)" != "$(col "$group" sid)" ] || fail "the detached child has a session of its own"
    same "$(col "$plain" sid)" "$(col "$group" sid)" "the plain child's session"
fi
same "$(col "$detached" pgid)" "$detached" "the detached child's group"
same "$(col "$plain" pgid)" "$group" "the plain child's group"
kill_group "$group"
wait "$pid" > /dev/null 2>&1
exec 3>&-
ends "$plain" 10 || fail "the plain child went with its group"
running "$detached" || fail "the detached child outlived its parent's group"
kill_group "$detached"
ends "$detached" 10 || fail "the detached child ends with its own group"
end

begin "released children do not become zombies"
startd c 50
released=$(pids_of "$sd/out")
same "$(printf '%s\n' "$released" | wc -l | tr -d ' ')" 50 "the children released"
for p in $released; do
    same "$(col "$p" stat)" "" "child $p ended and was released, so nothing is left of it"
done
grep -q '^held 7$' "$sd/out" || fail "a held child is still waited for"
stopd; same "$status" 0 "the exit status"
end

begin "a child that has already ended is released cleanly"
startd e
p=$(pids_of "$sd/out" | head -1)
grep -q '^released 0$' "$sd/out" || fail "released 0 missing: $(cat "$sd/out")"
same "$(col "$p" stat)" "" "no zombie of the child"
stopd; same "$status" 0 "the exit status"
end

begin "a detached start stays inside the prefix it was narrowed to"
if build tests/programs/process_release_bound.cho bound; then
    "$work/bound" inside > /dev/null 2>&1; same "$?" 0 "a start under the prefix"
    { "$work/bound" outside > /dev/null 2>&1; } 2> /dev/null
    case $? in 132 | 133) ;; *) fail "a start outside the prefix traps (SIGILL or SIGTRAP), not status $?" ;; esac
fi
end

begin "the authority report says a process may outlive the program"
released=$("$cancho" authority tests/programs/process_release.cho 2>&1)
case $released in *'exec("/bin")'*) ;; *) fail "exec(\"/bin\") missing: $released" ;; esac
case $released in *exec_detached*) ;; *) fail "exec_detached missing: $released" ;; esac
waited=$("$cancho" authority tests/accept/process_spawn.cho 2>&1)
case $waited in *'exec("/bin")'*) ;; *) fail "exec(\"/bin\") missing: $waited" ;; esac
case $waited in *exec_detached*) fail "exec_detached in a program that waits: $waited" ;; esac
end

begin "releasing a child gives back its pidfd"
# The pidfd a Linux child carries is closed by the release as it is by the wait: after fifty releases the program holds as many descriptors as after none. macOS watches a pid and holds no descriptor, so there is
# nothing to count there, and this prints what a pass prints.
if [ "$(uname -s)" = Linux ]; then
    startd c 0; none=$(ls "/proc/$pid/fd" | wc -l | tr -d ' '); stopd
    startd c 50; fifty=$(ls "/proc/$pid/fd" | wc -l | tr -d ' '); stopd
    same "$fifty" "$none" "descriptors held after fifty releases and after none"
fi
end
