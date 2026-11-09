# capture.sh -- the script of tests/commands/script_tests.cho for `conformance/capture.rs` (deleted): `std.process` (docs/processes.md 7.1, 7.2), an argument list, and a child's output captured with a bound and a deadline.
# One program, tests/programs/process_capture.cho, built here by the compiler ($1); each case is a mode on its command line, and what it prints ends in the milliseconds it took (`ms N`), which is checked here and left
# out of the comparison. $2 is a scratch directory; run from the root of the repository.
cancho=$1
work=$2
. tests/commands/script_lib.sh

build tests/programs/process_capture.cho capture

# case MODE: runs the driver on MODE; $text is what it printed less its `ms` lines (trailing newlines kept), $ms the largest of them.
case_of() {
    "$work/capture" "$1" > "$work/case.out" 2> /dev/null
    same "$?" 0 "the driver itself should not fail"
    text=$(grep -v '^ms ' "$work/case.out"; echo x); text=${text%x}
    ms=$(sed -n 's/^ms //p' "$work/case.out" | sort -n | tail -1)
    ms=${ms:-0}
}
# wanted FORMAT...: $_w is the text of printf with its trailing newlines kept.
wanted() { _w=$(printf "$@"; echo x); _w=${_w%x}; }
# is_text WHAT FORMAT...: $text is what printf says.
is_text() { _what=$1; shift; wanted "$@"; same "$text" "$_w" "$_what"; }
# checksum TEXT-WITH-PRINTF-ESCAPES: the driver's checksum (`sum` in process_capture.cho) of the bytes.
checksum() { printf "$1" | od -An -tu1 | tr -s ' ' '\n' | awk 'NF { s = (s * 31 + $1) % 1000000007 } END { print s + 0 }'; }
between() { [ "$1" -ge "$2" ] && [ "$1" -lt "$3" ]; }

begin "input and output are interleaved past both kernels' buffers"
# 7.1: 1 MiB through `cat`, written while it is read. A channel holds 8,192 bytes on macOS and 180,224 on Linux, and writing it all before reading any deadlocks from 32 KiB and 512 KiB; every byte comes back.
case_of cat
want_sum=$(printf '%s' "$text" | sed -n 's/^want sum //p')
[ -n "$want_sum" ] || fail "no 'want sum' line: $text"
is_text "what came back" 'want sum %s\ncode 0\nkept 1048576\nsum %s\n' "$want_sum" "$want_sum"
end

begin "output past the bound ends the child"
# 7.1: `most` bytes are kept and the capture is a normal one; the byte past `most` kills the child, and exactly `most` are kept. The child lingers after its output (`sleep 30`), so only the kill ends it.
case_of exact; is_text "exactly the bound" 'code 0\nkept 1000\nsum 0\n'
case_of most; is_text "past the bound" 'too much\nkept 1000\nsum 0\n'
[ "$ms" -lt 5000 ] || fail "the child outlived the bound: $ms ms"
end

begin "a child that cannot be watched is killed, not waited on"
# 7.1: with `pidfd_open` refused (ENOSYS, 38), `sleep 30` is not waited on blind. Linux only, and where strace can inject the error (it is installed on the CI machines); elsewhere this prints what a pass prints.
if [ "$(uname -s)" = Linux ] && command -v strace > /dev/null 2>&1 && strace -qq -f -o /dev/null -e trace=none true > /dev/null 2>&1; then
    strace -qq -f -o /dev/null -e trace=pidfd_open -e inject=pidfd_open:error=ENOSYS "$work/capture" failed > "$work/case.out" 2> /dev/null
    case $(cat "$work/case.out") in
        'failed 38
kept 0
sum 0
ms '*) ms=$(awk 'END { print $2 }' "$work/case.out"); [ "$ms" -lt 5000 ] || fail "the child was waited on: $ms ms" ;;
        *) fail "unexpected: $(cat "$work/case.out")" ;;
    esac
fi
end

# cpu_ms PID: the processor time the process has used so far, in ms (Linux: /proc; macOS: ps shows hundredths).
cpu_ms() {
    if [ -r "/proc/$1/stat" ]; then
        awk -v hz="$(getconf CLK_TCK)" '{ print int(($14 + $15) * 1000 / hz) }' "/proc/$1/stat"
    else
        ps -o time= -p "$1" | tr -d ' ' | awk -F'[:.]' '{ n = NF; cs = $n; s = $(n - 1); m = (n > 2) ? $(n - 2) : 0; h = (n > 3) ? $(n - 3) : 0; print int(((h * 60 + m) * 60 + s) * 1000 + cs * 10) }'
    fi
}
# quiet MODE: the driver on MODE, its processor time sampled 0.6 s into the one-second wait it spends (the Rust test read it from wait4 at the end); $text is what it printed.
quiet() {
    "$work/capture" "$1" > "$work/case.out" 2> /dev/null &
    qpid=$!
    sleep 0.6
    cpu=$(cpu_ms "$qpid")
    wait "$qpid"
    text=$(cat "$work/case.out"; echo x); text=${text%x}
    case $cpu in '' | *[!0-9]*) fail "no processor time for the driver: '$cpu'"; cpu=0 ;; esac
}

begin "a quiet child is waited for, not spun on"
# 7.1: a capture with nothing to do waits; it does not spin. The child closes both its streams and sleeps for a second: a channel that has ended stays ready for ever unless it is closed, and a capture that kept one on its
# poller would spend that time on the processor.
quiet quiet
case $text in 'code 0
kept 0
sum 0
'*) ;; *) fail "unexpected: $text" ;; esac
[ "$cpu" -lt 300 ] || fail "a one-second wait took $cpu ms of processor time in 0.6 s"
end

begin "a quiet child's errors are waited for, not spun on"
# 7.2: the same with the errors channel: `capture_both` that forgot the errors would spin on that one.
quiet r
case $text in 'code 0
out kept 0
out sum 0
err kept 0
err sum 0
'*) ;; *) fail "unexpected: $text" ;; esac
[ "$cpu" -lt 300 ] || fail "a one-second wait took $cpu ms of processor time in 0.6 s"
end

begin "a deadline ends a child that outlives it"
# 7.1: the deadline is the `Clock`'s, not a timeout per wait. `sleep 30` under 200 ms is killed, reaped and reported `TimedOut`, well before 30 s.
case_of time; is_text "the answer" 'timed out\nkept 0\nsum 0\n'
between "$ms" 200 5000 || fail "200 ms asked for, $ms taken"
end

begin "a process left behind does not hold the capture"
# 7.1: the child's exit ends the capture, not the end of the stream. A background `sleep 2` holds the output end for two seconds after the shell has exited.
case_of behind; is_text "the answer" 'code 0\nkept 3\nsum 103209\n'
[ "$ms" -lt 1500 ] || fail "the capture waited for the grandchild: $ms ms"
end

begin "a child that does not read its input still exits"
# 7.1: the write fails, the input end is closed, and the child's own exit status is the answer. No input at all is the same.
case_of deaf; is_text "the answers" 'code 0\nkept 0\nsum 0\ncode 4\nkept 0\nsum 0\n'
end

begin "an argument with a NUL is refused and left out"
# 7.1: an argument containing a `\0` would arrive as two; `add` refuses it with EINVAL (22) and leaves it out, and the list without it runs.
case_of nul; is_text "the answers" '0 22 0 2\ncode 6\nkept 0\nsum 0\n'
end

begin "a caller of capture declares the clock"
# 7.1: `capture`'s row is [heap, clock, poll], and a caller that lends it the `Clock` performs `clock` through it: a row that leaves it out is refused. A check here rather than a reject fixture, because fixtures are checked
# without --std. The Rust compiler's message also said "performs `clock`"; the compiler in cancho says the sentence of the rule, so only the tag is compared.
printf 'edition 7;\nimport std.buffer;\nimport std.process;\n\nfn wait[&h, &c](heap: &!h Heap, clock: &c Clock, child: Child, to: Pipe, from: Pipe)\n    -> [heap, poll] int {\n    let (out, ran) = process.capture(heap, clock, child, to, from, "", 10, 10);\n    return buffer.drop(heap, out);\n}\n\nfn main(world: World) -> [] int {\n    release(world);\n    return 0;\n}\n' > "$work/src.cho"
same "$(rule_of)" effect-not-declared "the rule"
end

begin "a capture that times out gives back all three channels"
# 7.2: under a limit of 40 descriptors sixty captures come and go, each with three channels; the errors end that was not closed would leak one per capture, and `channels_with_errors` would answer EMFILE (24) well before the sixtieth.
text=$(sh -c 'ulimit -n 40; exec "$0" "$@"' "$work/capture" l 2> /dev/null; echo x); text=${text%x}
case $text in 'timed out 60
'*) ;; *) fail "unexpected: $text" ;; esac
end

begin "standard error is captured beside standard output"
case_of x; is_text "the answer" 'code 3\nout kept 4\nout sum %s\nerr kept 4\nerr sum %s\n' "$(checksum 'out\n')" "$(checksum 'err\n')"
end

begin "a flood of errors does not stall the output"
# 7.2: a megabyte of errors before any output: a capture that read the output to its end before the errors would leave the child blocked on the errors for ever; both are on the poller.
case_of y; is_text "the answer" 'code 0\nout kept 4\nout sum %s\nerr kept 1048576\nerr sum 0\n' "$(checksum 'out\n')"
[ "$ms" -lt 5000 ] || fail "the capture waited on the child: $ms ms"
end

begin "errors past their bound end the child"
# 7.2: `most_errors` is the errors' own bound. Errors past it end the child (`sleep 30` follows them, so only the kill ends it) and exactly the bound is kept; and the bounds are separate.
case_of z; is_text "the answer" 'too much\nout kept 0\nout sum 0\nerr kept 1000\nerr sum 0\n'
[ "$ms" -lt 5000 ] || fail "the child outlived the bound: $ms ms"
case_of w; is_text "separate bounds" 'too much\nout kept 4\nout sum %s\nerr kept 3\nerr sum %s\n' "$(checksum 'out\n')" "$(checksum 'err')"
end

begin "a deadline returns both channels so far"
case_of v; is_text "the answer" 'timed out\nout kept 2\nout sum %s\nerr kept 2\nerr sum %s\n' "$(checksum 'a\n')" "$(checksum 'b\n')"
between "$ms" 300 5000 || fail "300 ms asked for, $ms taken"
end
