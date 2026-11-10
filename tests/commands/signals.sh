# signals.sh -- the script of tests/commands/script_tests.cho for `conformance/signals.rs` (deleted): the signal capability (docs/signals.md), over real processes. The programs are tests/programs/signals_*.cho,
# built here by the compiler ($1); $2 is a scratch directory; run from the root of the repository. They speak on standard error (`ready`, `mask=8`, ...), which is unbuffered, and are driven by standard input,
# one byte a command, so that a signal sent before a `p` is queued before the poll that must see it: kill(2) returns once the signal is pending. The signals are sent from here, to the real process.
cancho=$1
work=$2
. tests/commands/script_lib.sh

CLAIMABLE="ALRM HUP INT QUIT TERM USR1 USR2 WINCH"
ALL_SET="ALRM,HUP,INT,QUIT,TERM,USR1,USR2,WINCH"
# cancho's own bit for a claimable signal (docs/signals.md section 2.2).
bit() { case "$1" in HUP) echo 1 ;; INT) echo 2 ;; QUIT) echo 4 ;; TERM) echo 8 ;; USR1) echo 16 ;; USR2) echo 32 ;; ALRM) echo 64 ;; WINCH) echo 128 ;; esac; }

# Every program is built once, by whichever check needs it first.
built() { # SOURCE-NAME [SED-EXPRESSION]
    if [ ! -x "$work/$1" ]; then
        if [ -n "$2" ]; then sed "$2" "tests/programs/signals_$1.cho" > "$work/$1.cho"; else cp "tests/programs/signals_$1.cho" "$work/$1.cho"; fi
        build "$work/$1.cho" "$1"
    fi
}

# ---- the signals themselves ----

begin "each claimed signal is delivered exactly once"
built every && {
    start "$work/every"
    expect ready
    for s in $CLAIMABLE; do
        sig "$s"; cmd p; expect "mask=$(bit "$s")"
        cmd p; expect "mask=0"
    done
    # And a poll when nothing was sent says nothing arrived.
    cmd p; expect "mask=0"
    cmd q; expect ended
    finish; same "$status" 0 "the exit status"
    reap
}
end

begin "signals between polls are all reported and a repeat is one"
built every && {
    start "$work/every"
    expect ready
    sig TERM; sig INT; sig HUP
    cmd p; expect "mask=$(($(bit TERM) + $(bit INT) + $(bit HUP)))"
    # The same signal three times, with another between: two bits, not four.
    sig USR1; sig USR1; sig WINCH; sig USR1
    cmd p; expect "mask=$(($(bit USR1) + $(bit WINCH)))"
    # All eight at once.
    for s in $CLAIMABLE; do sig "$s"; done
    cmd p; expect "mask=255"
    cmd p; expect "mask=0"
    cmd q; expect ended
    finish; same "$status" 0 "the exit status"
    reap
}
end

begin "a signal that was not claimed still takes its default action"
# The same program, claiming only TERM, is ended by USR1 as any program is.
if [ ! -x "$work/unclaimed" ]; then
    sed "s/$ALL_SET/TERM/" tests/programs/signals_every.cho > "$work/unclaimed.cho"
    build "$work/unclaimed.cho" unclaimed
fi
if [ -x "$work/unclaimed" ]; then
    start "$work/unclaimed"
    expect ready
    sig TERM; cmd p; expect "mask=8"
    sig USR1
    ended_by USR1
    reap
fi
end

begin "a signal wakes a poller wait at once"
built poller && {
    start "$work/poller"
    expect "added=0"
    expect ready
    # The wait is established by now: a signal sent later cannot have been picked up by a poll that ran before the wait began.
    sleep 0.3
    sent=$(now)
    sig TERM
    expect "woke=1"
    woke=$(($(now) - sent))
    expect "token=7"
    expect "events=1"
    next
    case $got in
        waited=*) waited=${got#waited=}
            [ "$waited" -ge 250 ] && [ "$waited" -lt 5000 ] || fail "the wait lasted $waited ms" ;;
        *) fail "waited= expected, the program said '$got'" ;;
    esac
    expect "mask=8"
    expect "again=0"
    # 20 s was the timeout; a wake that took the timeout out would not be this (the clock here is whole seconds).
    [ "$woke" -le 5 ] || fail "woke $woke s after the signal"
    finish; same "$status" 0 "the exit status"
    reap
}
end

begin "closing the claim makes the next signal kill at once"
built second && {
    start "$work/second"
    expect ready
    sig TERM; cmd r
    expect "first=8"
    expect "closed=0"
    sent=$(now)
    sig TERM
    ended_by TERM
    [ "$(($(now) - sent))" -le 5 ] || fail "not at once"
    reap
}
end

begin "closing with a signal unread delivers it at once"
built second && {
    start "$work/second"
    expect ready
    sig TERM; cmd g
    ended_by TERM
    # It never got as far as saying it was closed.
    [ "$(grep -c '^closed=' "$sd/err")" = 0 ] || fail "the program ran on past the close"
    reap
}
end

begin "closing after the read leaves nothing to deliver"
built second && {
    start "$work/second"
    expect ready
    sig USR2; cmd r
    expect "first=32"
    expect "closed=0"
    # Alive: it is sleeping on an empty poller, and the signal it read is gone.
    sleep 0.3
    alive || fail "it ended on its own"
    # Not claimed any more, so USR2 is USR2 again.
    sig USR2
    ended_by USR2
    reap
}
end

begin "a signal ignored on entry can be claimed"
# What it is afterwards is the one place the two kernels differ (docs/signals.md section 3): Linux never touched it, so it is ignored again; macOS ignored it to watch it and puts back the default.
built second && {
    start "$work/second" USR2
    expect ready
    sig USR2; cmd r
    expect "first=32"
    expect "closed=0"
    if [ "$(uname -s)" = Linux ]; then
        # Ignored as it was entered: the signal does nothing.
        sig USR2
        sleep 0.3
        alive || fail "ignored, so it lives"
        # And an unrelated one that was never ignored still ends it.
        sig TERM
        ended_by TERM
    fi
    reap
}
end

begin "a thread spawned after the claim does not take the signal"
built threads_inherit && {
    start "$work/threads_inherit"
    expect ready
    # The thread is napping (2.5 s: a loaded host can be slow to send the signal); the signal must wait for the program.
    sleep 0.2
    sig TERM
    expect "mask=8"
    finish; same "$status" 0 "the exit status"
    reap
}
end

begin "a claim is refused while a thread runs and granted after the join"
built threads_busy && {
    start "$work/threads_busy"
    expect "during=16"
    expect ready
    expect "after=granted"
    finish; same "$status" 0 "the exit status"
    reap
}
end

begin "a signal another claim holds is refused until it is closed"
built overlap && {
    start "$work/overlap"
    expect "second=16"
    expect "third=granted"
    finish; same "$status" 0 "the exit status"
    reap
}
end

begin "std.signals names the bits"
built std && {
    "$work/std"
    same "$?" 8 "the checks that pass"
}
end

# ---- refusals, and the authority report ----

# with_signals BODY: `main` with the whole of `Split` taken apart, leaving `signals`.
with_signals() {
    printf 'edition 6;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n    release(io); release(ffi); release(fs); release(heap); release(args); release(net);\n    release(clock);\n    %s\n    return 0;\n}\n' "$1"
}
narrowing_to() { with_signals "let claimed = narrow(signals, \"$1\"); release(claimed);"; }
# main_with RELEASED BODY: `main` that splits the world and releases what RELEASED names, then runs BODY.
main_with() {
    _rel=
    for _n in $1; do _rel="$_rel release($_n);"; done
    printf 'edition 6;\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n   %s\n    %s\n    return 0;\n}\n' "$_rel" "$2"
}
ALL_BUT_SIGNALS="io ffi fs heap args net clock"
CLAIM_AND_CLOSE='match signals_watch(CAP) { Watching::Ok(w) => { signals_close(w); } Watching::Failed(e) => { } }'

# want_rule CASE WANTED: the source in $work/src.cho is refused under the rule WANTED (`accepted`: it is not; `refused`: under any); a pinned gap (gap_CASE below) is reported, not failed.
want_rule() {
    _got=$(rule_of)
    if [ "$_got" = "$2" ]; then return 0; fi
    if [ "$2" = refused ] && [ "$_got" != accepted ]; then return 0; fi
    _key=$(printf '%s' "$1" | tr -c 'a-zA-Z0-9' '_')
    _known=$(eval "echo \"\$gap_$_key\"")
    if [ -n "$_known" ] && [ "$_known" = "$_got" ]; then gap "$1" "says $_got" "answers $2"; return 0; fi
    fail "$1: got '$_got', wanted '$2'"
}
# The pinned differences: where the compiler in cancho answers differently from the Rust compiler the test was written against (the transcript says GAP for each, so it changes when one closes).
gap_root=accepted
gap_not_signals=accepted
gap_by_value=linear-use-after-move
gap_owned_root=accepted
gap_unordered=accepted
gap_row_exact=effect-declared-not-performed

begin "every unclaimable signal is refused with its rule"
# The Rust test also read the reason out of the message ("cannot be caught", "a fault, not a request", "not claimable (yet)"); the compiler in cancho says the sentence of the rule, so only the tag is compared.
for s in KILL STOP SEGV ILL BUS FPE ABRT TRAP SYS PIPE CHLD CONT TSTP TTIN TTOU URG IO XCPU XFSZ VTALRM PROF STKFLT PWR KILL,TERM TERM,KILL INT,SEGV,TERM; do
    narrowing_to "$s" > "$work/src.cho"; want_rule "$s" signal-not-claimable
done
end

begin "a malformed set is refused and any order is accepted"
for s in "" "TERM," ",TERM" "TERM,TERM" "INT, TERM" SIGTERM term FOO "TERM;INT"; do
    narrowing_to "$s" > "$work/src.cho"; want_rule "set '$s'" signal-not-claimable
done
for s in $CLAIMABLE TERM,INT "$ALL_SET"; do
    narrowing_to "$s" > "$work/src.cho"; want_rule "set '$s'" accepted
done
# The type is the canonical one whichever order the set was written in.
{ with_signals 'let claimed = narrow(signals, "TERM,INT"); borrow claimed as &s in { f(s); } release(claimed);'; echo 'fn f[&s](sig: &s Signals("INT,TERM")) -> [] int { return 0; }'; } > "$work/src.cho"; want_rule "the canonical type" accepted
end

begin "narrowing a signal set never widens it"
with_signals 'let one = narrow(signals, "INT"); let two = narrow(one, "INT,TERM"); release(two);' > "$work/src.cho"; want_rule widen capability-not-narrowable
with_signals 'let one = narrow(signals, "INT"); let two = narrow(one, "TERM"); release(two);' > "$work/src.cho"; want_rule sideways capability-not-narrowable
with_signals 'let one = narrow(signals, "INT,TERM"); let two = narrow(one, "TERM,INT"); release(two);' > "$work/src.cho"; want_rule same capability-not-narrowable
# Narrowing to a subset, and again to a smaller one, is the point.
with_signals 'let a = narrow(signals, "HUP,INT,TERM"); let b = narrow(a, "INT,TERM"); let c = narrow(b, "TERM"); release(c);' > "$work/src.cho"; want_rule subset accepted
end

begin "only a narrowed borrowed Signals can claim"
claim=$(echo "$CLAIM_AND_CLOSE" | sed 's/CAP/s/')
main_with "$ALL_BUT_SIGNALS" "borrow signals as &s in { $claim } release(signals);" > "$work/src.cho"; want_rule root capability-misused
main_with "io ffi fs heap args net signals" "borrow clock as &c in { $(echo "$CLAIM_AND_CLOSE" | sed 's/CAP/c/') } release(clock);" > "$work/src.cho"; want_rule "not signals" capability-misused
main_with "$ALL_BUT_SIGNALS" "let claim = narrow(signals, \"TERM\"); $(echo "$CLAIM_AND_CLOSE" | sed 's/CAP/claim/') release(claim);" > "$work/src.cho"; want_rule "by value" capability-misused
main_with "$ALL_BUT_SIGNALS" "let claim = narrow(signals, \"TERM\"); borrow claim as &s in { $claim } release(claim);" > "$work/src.cho"; want_rule good accepted
end

begin "the signal names are edition six"
five='edition 5;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args, net, clock } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    release(net); release(clock);
    return 0;
}'
printf '%s\n' "$five" > "$work/src.cho"; want_rule "edition 5 keeps its Split" accepted
printf '%s\n' "$five" | sed 's/return 0;/return signals_close(3);/' > "$work/src.cho"; want_rule "a name below edition 6" not-a-function
# A seven-field pattern at edition 6 names too few.
printf '%s\n' "$five" | sed 's/edition 5;/edition 6;/' > "$work/src.cho"; want_rule "seven fields at edition 6" arity-mismatch
# Edition 6 is edition 5 and more: the socket names are still there.
printf 'edition 6;\nfn f() -> [] int { match poller_new() { Polling::Ok(p) => { poller_close(p); } Polling::Failed(e) => { } } return 0; }\nfn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args, net, clock, signals } = split(world);\n    release(io); release(ffi); release(fs); release(heap); release(args);\n    release(net); release(clock); release(signals);\n    return f();\n}\n' > "$work/src.cho"; want_rule "the socket names at edition 6" accepted
end

begin "the claim is linear and its rows are exact"
claim_with() { main_with "$ALL_BUT_SIGNALS" "let c = narrow(signals, \"TERM\"); borrow c as &s in { match signals_watch(s) { Watching::Ok(w) => { $1 } Watching::Failed(e) => { } } } release(c);"; }
main_only='fn main(world: World) -> [] int { release(world); return 0; }'
claim_with "" > "$work/src.cho"; want_rule unclosed linear-value-unconsumed
claim_with "signals_close(w); signals_close(w);" > "$work/src.cho"; want_rule twice linear-use-after-move
claim_with "let SignalWatch { } = w;" > "$work/src.cho"; want_rule apart linear-value-taken-apart
claim_with "signals_close(w);" > "$work/src.cho"; want_rule closed accepted
printf 'edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [] int { return signals_pending(watch); }\n%s\n' "$main_only" > "$work/src.cho"; want_rule undeclared effect-not-declared
printf 'edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [signals_read] int { return 0; }\n%s\n' "$main_only" > "$work/src.cho"; want_rule unperformed effect-declared-not-performed
printf 'edition 6;\nfn look[&w](watch: &!w SignalWatch) -> [signals_read] int { return signals_pending(watch); }\n%s\n' "$main_only" > "$work/src.cho"; want_rule declared accepted
# Owning one is stronger than borrowing it, and says so by having an empty row.
printf 'edition 6;\nfn drain(watch: SignalWatch) -> [] int {\n    var held = watch;\n    borrow mut held as &!w in { signals_pending(w); }\n    return signals_close(held);\n}\n%s\n' "$main_only" > "$work/src.cho"; want_rule "owned watch" accepted
owned_caps='edition 6;
fn claim(sig: Signals("TERM")) -> [] int {
    var status = 1;
    borrow sig as &s in {
        match signals_watch(s) {
            Watching::Ok(w) => { var held = w; borrow mut held as &!h in { signals_pending(h); } status = signals_close(held); }
            Watching::Failed(e) => { status = e; }
        }
    }
    release(sig);
    return status;
}'
{ echo "$owned_caps"; echo "$main_only"; } > "$work/src.cho"; want_rule "owned cap" accepted
# The root, owned, cannot claim; a set written in a type is the canonical one, or it is refused.
{ echo "$owned_caps" | sed 's/Signals("TERM")/Signals("")/'; echo "$main_only"; } > "$work/src.cho"; want_rule "owned root" refused
{ echo "$owned_caps" | sed 's/Signals("TERM")/Signals("INT,TERM")/'; echo "$main_only"; } > "$work/src.cho"; want_rule wider accepted
{ echo "$owned_caps" | sed 's/Signals("TERM")/Signals("TERM,INT")/'; echo "$main_only"; } > "$work/src.cho"; want_rule unordered signal-not-claimable
# The set in a row is the set in the capability's type, exactly.
row() { printf 'edition 6;\nfn claim[&s](sig: &s Signals("INT,TERM")) -> [signals("%s")] int { match signals_watch(sig) { Watching::Ok(w) => { return signals_close(w); } Watching::Failed(e) => { return e; } } }\n%s\n' "$1" "$main_only"; }
row "INT,TERM" > "$work/src.cho"; want_rule "row exact" accepted
for wrong in INT TERM HUP,INT,TERM TERM,INT; do
    row "$wrong" > "$work/src.cho"; _got=$(rule_of)
    case $_got in effect-not-declared | effect-declared-not-performed) ;; *) fail "row $wrong: $_got" ;; esac
done
end

# ---- the authority report ----

report_of() { # SET [FLAGS]: the authority of tests/programs/signals_report.cho claiming SET
    sed "s/@SET@/$1/" tests/programs/signals_report.cho > "$work/report.cho"
    shift
    "$cancho" authority "$work/report.cho" --std "$@" 2>&1
}

begin "the authority report names the set and is bounded"
out=$(report_of "TERM,INT" --output json)
case $out in *'"bounded": true'*) ;; *) fail "not bounded: $out" ;; esac
case $out in *'{ "name": "signals", "argument": "INT,TERM", "bounded": true }'*) ;; *) fail "the set is not named: $out" ;; esac
case $out in *'{ "name": "signals_read", "argument": null, "bounded": true }'*) ;; *) fail "signals_read is not named: $out" ;; esac
case $out in *'"ffi"'*) fail "foreign code in the report: $out" ;; esac
case $out in *'"foreign_symbols": []'*) ;; *) fail "foreign symbols: $out" ;; esac
# The set is what was claimed: a different set is a different report.
every=$(report_of "$ALL_SET" --output json)
case $every in *"\"argument\": \"$ALL_SET\""*) ;; *) fail "the full set is not named: $every" ;; esac
one=$(report_of HUP --output json)
case $one in *'"argument": "HUP"'*) ;; *) fail "HUP is not named: $one" ;; esac
case $one in *TERM*) fail "TERM is named in a report of HUP: $one" ;; esac
end

begin "the text report says what is claimed and what is not"
claims=$(report_of "INT,TERM")
case $claims in *'signals("INT,TERM")'*) ;; *) fail "the set is not named: $claims" ;; esac
case $claims in *UNBOUNDED*) fail "UNBOUNDED: $claims" ;; esac
# "never touches" lists `signals` among the capabilities a program does not use; a program that claims it must not be listed so.
printf '%s\n' "$claims" | grep -q '^    signals$' && fail "signals are claimed, so not 'never touches': $claims"
main_with "$ALL_BUT_SIGNALS signals" "" > "$work/silent.cho"
silent=$("$cancho" authority "$work/silent.cho" --std 2>&1)
case $silent in *"never touches"*) ;; *) fail "never touches: $silent" ;; esac
printf '%s\n' "$silent" | grep -q '^    signals$' || fail "signals not listed as untouched: $silent"
end
