# vcs_crash.sh -- the last test of crates/cancho/tests/vcs_repo.rs, `a_kill_at_every_write_leaves_a_sound_store` (docs/gate.md G4, wave 4; docs/vcs-cli.md section 5): kill `cancho vcs record` and
# `cancho vcs switch` part-way and check the store is sound, the ref is at the old change or the new one, and running the command again finishes with the change a run that was never killed made.
# The Rust command had a failpoint (`CANCHO_VCS_CRASH_AT=n`: exit 137 at the n-th write) and the command in cancho has none (issue 687; docs/vcs-native.md section 3 said a trap would answer it, and
# a trap needs the counter threaded through every write of the store). So the kill comes from outside: the command runs in the background and `kill -9` reaches it after a delay, swept from 0 to 12 ms in
# steps of a quarter of a millisecond, five times each (a whole `record` takes about 6 ms). That is not "at every write": which write it stops at is the scheduler's, so what the transcript checks is
# that the sweep reached many different states of the store (`distinct` below, counted as the listing of the store with the size of each file), and a floor on that count stands where the Rust test had
# `killed >= 10`. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

AUTHOR='Ada <ada@example.com>'
GOOD='module a;
pub fn twice(n: int) -> [] int { return n + n; }'
# v ARGS...: `cancho vcs ARGS --output json` here; the document in $work/o.json, the status in $code.
v() {
    case $1 in
        record | revert) "$cancho" vcs "$@" --author "$AUTHOR" --output json > "$work/o.json" 2> "$work/e.txt" ;;
        *) "$cancho" vcs "$@" --output json > "$work/o.json" 2> "$work/e.txt" ;;
    esac
    code=$?
}
# jf KEY: the value of the first `"KEY": value`, without quotes.
jf() { sed -n "s/^ *\"$1\": \"\{0,1\}\([^\",]*\)\"\{0,1\},\{0,1\}\$/\1/p" "$work/o.json" | head -1; }
# shape DIR: the listing of a store with the size of each file, one line each.
shape() { (cd "$1" && find .cancho -type f -exec wc -c {} + 2> /dev/null | sort | sed 's/^ *//' | tr '\n' ';'); }
# killed_after SECONDS ARGS...: run the command, kill -9 it after SECONDS, and leave its status in $code (137 if it was killed).
killed_after() {
    _d=$1; shift
    case $1 in
        record) "$cancho" vcs "$@" --author "$AUTHOR" --output json > "$work/k.out" 2> /dev/null & ;;
        *) "$cancho" vcs "$@" --output json > "$work/k.out" 2> /dev/null & ;;
    esac
    _p=$!
    sleep "$_d"
    kill -9 "$_p" 2> /dev/null
    wait "$_p" 2> /dev/null
    code=$?
}
# The delays, in seconds.
delays() { awk 'BEGIN { for (i = 0; i <= 48; i++) printf "%.4f\n", i * 0.00025 }'; }

mk() { rm -rf "$work/$1"; mkdir -p "$work/$1"; cd "$work/$1" || exit 1; }

begin a_kill_during_record_leaves_a_sound_store
mk base
v init
printf '%s\n' "$GOOD" > a.cho
printf 'one\n' > notes.md
v record --all --intent-prompt first --time 1
first=$(jf change)
printf 'two\n' > notes.md
mkdir -p new/dir
printf 'hello\n' > new/dir/file.txt
rm -rf "$work/ref"; cp -R "$work/base" "$work/ref"
cd "$work/ref" || exit 1
v record --all --intent-prompt second --time 2
want=$(jf change)
[ "$want" != "$first" ] || fail "the reference record made no new change"
killed=0
states=
for round in 1 2 3 4 5; do
    for d in $(delays); do
        rm -rf "$work/t"; cp -R "$work/base" "$work/t"; cd "$work/t" || exit 1
        killed_after "$d" record --all --intent-prompt second --time 2
        [ "$code" = 137 ] || continue
        killed=$((killed + 1))
        states="$states$(shape "$work/t")
"
        v verify
        [ "$code" = 0 ] && [ "$(jf sound)" = true ] || { fail "verify after a kill at ${d}s: exit $code, $(head -c 200 "$work/o.json")"; continue; }
        locked=1
        grep -q '"locks": \[\]' "$work/o.json" && locked=0
        v status
        head=$(jf head)
        [ "$head" = "$first" ] || [ "$head" = "$want" ] || fail "kill at ${d}s: HEAD is $head, neither old nor new"
        if [ "$locked" != 0 ]; then
            v record --all --time 2
            tag=$(jf tag)
            [ "$tag" = lock-held ] || [ "$(jf change)" = "$want" ] || fail "kill at ${d}s: a held lock was passed over: $(head -c 200 "$work/o.json")"
            v unlock
        fi
        v record --all --intent-prompt second --time 2
        same "$(jf change)" "$want" "kill at ${d}s: running again finishes with the same change ($(tr -d '\n ' < "$work/o.json" | head -c 300))"
        v verify
        same "$(jf sound)" true "kill at ${d}s: sound after the second run"
        v status
        same "$(jf clean)" true "kill at ${d}s: clean after the second run"
    done
done
distinct=$(printf '%s' "$states" | sort -u | grep -c .)
[ "$killed" -ge 10 ] && echo "record killed at least 10 times: yes" || echo "record killed at least 10 times: no ($killed)"
[ "$distinct" -ge 4 ] && echo "record: at least 4 distinct states of the store seen after a kill: yes" || echo "record: at least 4 distinct states seen: no ($distinct)"
end

begin a_kill_during_switch_leaves_a_sound_store
rm -rf "$work/sw"; cp -R "$work/ref" "$work/sw"; cd "$work/sw" || exit 1
v branch create old --from HEAD~1
killed=0
states=
for round in 1 2 3 4 5; do
    for d in $(delays); do
        rm -rf "$work/t"; cp -R "$work/sw" "$work/t"; cd "$work/t" || exit 1
        killed_after "$d" switch old
        [ "$code" = 137 ] || continue
        killed=$((killed + 1))
        states="$states$(shape "$work/t")
"
        v unlock
        v verify
        [ "$(jf sound)" = true ] || { fail "switch killed at ${d}s: the store is not sound: $(head -c 200 "$work/o.json")"; continue; }
        v switch old --discard
        same "$code" 0 "switch killed at ${d}s: switching again"
        same "$(cat notes.md)" one "switch killed at ${d}s: the working copy is the old one"
        v status
        same "$(jf clean)" true "switch killed at ${d}s: clean"
    done
done
distinct=$(printf '%s' "$states" | sort -u | grep -c .)
[ "$killed" -ge 5 ] && echo "switch killed at least 5 times: yes" || echo "switch killed at least 5 times: no ($killed)"
[ "$distinct" -ge 2 ] && echo "switch: at least 2 distinct states of the store seen after a kill: yes" || echo "switch: at least 2 distinct states seen: no ($distinct)"
end
