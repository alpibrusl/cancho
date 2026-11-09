# processes.sh -- the script of tests/commands/script_tests.cho for `conformance/processes.rs` (deleted): `exec_spawn`, `child_wait`, `child_kill`, the pipe verbs, the poller and the working directory of a child
# (docs/processes.md), over real processes. The probes are programs under tests/programs/ (process_probe, process_waiter, process_traps, process_cwd), built here by the compiler ($1); $2 is a scratch directory;
# run from the root of the repository. Every check prints `ok NAME`, or `FAILS NAME` and what it got.
cancho=$1
work=$2

build() { "$cancho" build "tests/programs/$1.cho" -o "$work/$2" > "$work/build-$2.txt" 2>&1 || { echo "FAILS build $1"; cat "$work/build-$2.txt"; }; }
build process_probe probe
build process_waiter waiter
build process_traps traps
build process_cwd cwd

# check NAME GOT WANT: the two strings are the same.
check() {
    if [ "$2" = "$3" ]; then echo "ok $1"; else echo "FAILS $1"; printf '  got:  %s\n  want: %s\n' "$2" "$3"; fi
}
# want FORMAT [ARGS]: the text of printf, with its trailing newlines kept (a command substitution drops them).
want() { _w=$(printf "$@"; echo x); _w=${_w%x}; }
# got COMMAND [ARGS]: the standard output of the command, trailing newlines kept.
got() { _g=$("$@"; echo x); _g=${_g%x}; }
# probe NAME PRINTF-FORMAT PROGRAM ARGS ENV STDIN MODE
probe() {
    _n=$1; _f=$2; shift 2
    got "$work/probe" "$@"
    want "$_f"
    check "$_n" "$_g" "$_w"
}

# 4.2, 4.3: the child's arguments and environment are exactly what was passed -- an empty argument, a space inside one, nothing inherited.
probe "arguments" 'a\nb c\n\nend\n== code 0\n' /usr/bin/printf '%s\n|a|b c||end' - - plain
probe "environment" 'A=1\nB=two\n== code 0\n' /usr/bin/env - 'A=1|B=two' - plain
probe "nothing inherited" '== code 0\n' /usr/bin/env - - - plain

# 4.4: a channel carries the parent's bytes to the child and back.
probe "the child reads what the parent writes" 'fed through== code 0\n' /bin/cat - - 'fed through' plain

# 4.5: holding one descriptor it inherited without close-on-exec, the parent starts a child that holds its three streams and nothing else (`ls` opens one of its own for /dev/fd, on macOS two).
got sh -c 'exec 9</dev/null; exec "$0" "$@"' "$work/probe" /bin/sh '-c|ls /dev/fd' - - many
listing=${_g%%== *}
fds=$(printf '%s' "$listing" | tr -s ' \n\t' '\n\n\n' | grep -E '^[0-9]+$' | sort -n | tr '\n' ' ')
shape=$(echo "$fds" | awk '{ ok = ($1 == 0 && $2 == 1 && $3 == 2 && NF <= 5); for (i = 1; i <= NF; i++) if ($i > 4) ok = 0; print ok ? "streams only" : "other descriptors: " $0 }')
check "a child holds exactly its three streams" "$shape" "streams only"
case $_g in *"== code 0
") echo "ok the listing child ended with code 0" ;; *) echo "FAILS the listing child did not end with code 0: $_g" ;; esac

# 4.7: KILL by its bit ends a child as Signaled(256); a bit that names no signal is EINVAL with no call. 4.6: TERM ends a child although the parent holds TERM claimed.
probe "kill by its bit" '== kill 0\n== signal 256\n' /bin/sleep 30 - - kill
probe "term with TERM claimed" '== kill 0\n== signal 8\n' /bin/sleep 30 - - term
probe "a bit that names no signal" '== kill 22\n== code 0\n' /bin/sleep 0 - - bad

# 4.1, 4.7: a missing program is an outcome (ENOENT), an exit code is reported as itself.
probe "a missing program" '== spawn 2\n' /nonexistent/program - - - plain
probe "an exit code" '== code 3\n' /bin/sh '-c|exit 3' - - plain

# 4.4: writing to a child that has ended is Sent::Failed(EPIPE), and the parent lives to say so.
probe "writing to an ended child" '== code 0\n== write 32\n' /usr/bin/true - - 'never read' late

# waiter NAME PRINTF-FORMAT MODE SCRIPT: the waiter on one mode, with `/bin/sh -c SCRIPT`.
waiter() {
    _n=$1; _f=$2; shift 2
    got "$work/waiter" "$1" "-c|$2"
    want "$_f"
    check "$_n" "$_g" "$_w"
}
# 4.8: a channel and a child watched together.
waiter "a poller reports a child's output and its exit" 'add pipe 0\nadd child 0\nidle 0\nbytes 3\npipe ended 1\nchild reported 1\n== code 0\n' a 'sleep 1; echo hi'
waiter "a child that has already ended is still reported" 'nap 0\nadd child 0\nwoke 1\ntoken 2\nevents 1\n== code 7\n' b 'exit 7'
waiter "a deadline passes and a kill is reported" 'add child 0\nearly 0\nkill 0\nwoke 1\ntoken 2\nevents 1\n== signal 256\n' c 'sleep 30'

# 4.8: the pidfd of a reaped child is given back: under a limit of 24 descriptors, sixty children come and go and the next is still watchable.
got sh -c 'ulimit -n 24; exec "$0" "$@"' "$work/waiter" d '-c|exit 0'
want 'churned 60\nnap 0\nadd child 0\nwoke 1\ntoken 2\nevents 1\n== code 0\n'
check "reaping a child gives back its pidfd" "$_g" "$_w"

# 4.8: EMFILE reaches the pidfd when the spawn gave no descriptor back and none was free (Linux); Darwin watches the pid and needs no descriptor.
got sh -c 'ulimit -n 8; exec 4</dev/null 5</dev/null 6</dev/null 7</dev/null; exec "$0" f' "$work/waiter"
if [ "$(uname -s)" = Linux ]; then want 'add child 24\n== code 5\n'; else want 'add child 0\n== code 5\n'; fi
check "a child with no descriptor to spare says EMFILE" "$_g" "$_w"

# 4.8: a kernel without pidfd_open (before Linux 5.3), stood in for by strace answering that system call with ENOSYS (38) (the Rust test installed a seccomp filter). Linux only, and where strace can inject the error (it is
# installed on the CI machines); elsewhere this prints what a pass prints.
if [ "$(uname -s)" = Linux ] && command -v strace > /dev/null 2>&1 && strace -qq -f -o /dev/null -e trace=none true > /dev/null 2>&1; then
    got strace -qq -f -o /dev/null -e trace=pidfd_open -e inject=pidfd_open:error=ENOSYS "$work/waiter" e '-c|exit 5'
    want 'add child 38\n== code 5\n'
    check "a child without a pidfd says why it cannot be watched" "$_g" "$_w"
else
    echo "ok a child without a pidfd says why it cannot be watched"
fi

# 4.1 to 4.3: a path outside the bound, `..`, a sibling of the bound, a relative path, a loader variable and a list that does not end in NUL each trap (SIGILL, or SIGTRAP: `brk` on aarch64); an ordinary call under the bound does not.
# 4.4: a child writes to Stdio::Null as to any stream.
"$work/traps" ok > /dev/null 2>&1; check "an ordinary spawn under /bin" "$?" 0
"$work/traps" null > /dev/null 2>&1; check "a child writing to Stdio::Null" "$?" 0
for mode in usr dotdot sibling relative ld yld args env; do
    { sh -c '"$0" "$1"' "$work/traps" "$mode" > /dev/null 2>&1; } 2> /dev/null
    status=$?
    case $status in 132 | 133) echo "ok $mode traps" ;; *) echo "FAILS $mode does not trap: status $status" ;; esac
done

# 2: the report names the bound, and the program stays bounded.
report=$("$cancho" authority tests/programs/process_traps.cho 2>&1)
case $report in *'exec("/bin")'*) echo "ok the report names the bound" ;; *) echo "FAILS the report does not name exec(\"/bin\"): $report" ;; esac
case $report in *UNBOUNDED*) echo "FAILS the report says UNBOUNDED" ;; *) echo "ok the program stays bounded" ;; esac

# ---- 4.10: the working directory ----
# A tree for the driver: work/ holds an executable script `tool`, elsewhere/ is the directory the driver itself starts in (canonical paths: macOS's /var is /private/var).
mkdir -p "$work/tree/work" "$work/tree/elsewhere"
printf '#!/bin/sh\npwd\n' > "$work/tree/work/tool"
chmod 755 "$work/tree/work/tool"
w=$(cd "$work/tree/work" && pwd -P)
e=$(cd "$work/tree/elsewhere" && pwd -P)
# cwd FROM DIR PROGRAM ARG MODE: the driver, started in FROM.
cwd() { _from=$1; shift; got sh -c 'cd "$0" && exec "$@"' "$_from" "$work/cwd" "$@"; }

cwd "$e" "$w" /bin/pwd - d
want '%s\n== code 0\n== dirclose 0\n' "$w"
check "a child runs in the directory it is given" "$_g" "$_w"
cwd "$e" "$w" /bin/pwd - n
want '%s\n== code 0\n== dirclose 0\n' "$e"
check "a child without one runs where its parent does" "$_g" "$_w"
cwd "$e" "$w" /bin/pwd - b
want '%s\n== code 0\n%s\n== code 0\n== dirclose 0\n' "$w" "$e"
check "the directory is lent and the parent has not moved" "$_g" "$_w"

cwd "$e" "$w" ./tool - d
want '%s\n== code 0\n== dirclose 0\n' "$w"
check "a relative program is found beneath the directory" "$_g" "$_w"
cwd "$e" "$w" ./tool - n
want '== spawn 2\n== dirclose 0\n'
check "and without a directory it is ENOENT" "$_g" "$_w"

if [ "$(uname -s)" = Darwin ]; then listing=/dev/fd; else listing=/proc/self/fd; fi
cwd "$e" "$w" /bin/ls "$listing" d; with=$_g
cwd "$e" "$w" /bin/ls "$listing" n; without=$_g
check "the directory is not a descriptor the child holds" "$with" "$without"

cwd "$e" "$work/tree/nothing" /bin/pwd - d
want '== opendir 2\n'
check "a directory that is not there starts nothing" "$_g" "$_w"

# A directory that opens for reading but has no search permission cannot be a working directory: EACCES (13). Not run as root, which searches anything.
if [ "$(id -u)" = 0 ]; then
    echo "ok a directory that cannot be searched fails the spawn"
else
    mkdir "$w/closed"
    chmod 444 "$w/closed"
    cwd "$e" "$w/closed" /bin/pwd - d
    want '== spawn 13\n== dirclose 0\n'
    check "a directory that cannot be searched fails the spawn" "$_g" "$_w"
    chmod 755 "$w/closed"
fi
