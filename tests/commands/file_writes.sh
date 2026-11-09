# file_writes.sh -- the script of tests/commands/file_writes.cho. $1 is the probe (tests/programs/file_writes_probe.cho, built with --std), $2 a scratch directory.
# The write side of a file handle (`docs/file-writes.md`). Every case is judged from outside the program, for the reason `filesystem.md` section 2.2 gave: a read that happens to succeed is not evidence about a mode, and a
# sync that returned Ok is not evidence that it called anything. The bytes are read here; the flags and the mode by the kernel and by strace. The probe takes a case letter and paths (see its head) and exits with an answer
# the case builds from what the verbs said. The cases that need a fixed prefix use /tmp/cancho-fw-granted and /tmp/cancho-fw-outside.bin, made and removed here.
probe=$1
dir=$2
data=$dir/data
granted=/tmp/cancho-fw-granted
outside=/tmp/cancho-fw-outside.bin
case $(uname) in Linux) linux=1 ;; *) linux=0 ;; esac
mkdir -p "$data"
rm -rf "$granted" "$outside"

# The exit status of the probe, and whether it was killed by a signal (a status over 128).
status() { "$probe" "$@" > /dev/null 2>&1; echo $?; }
killed() { s=$(status "$@"); if [ "$s" -gt 128 ]; then echo "killed by a signal"; else echo "exit $s (it should have been killed)"; fi; }
bytes() { if [ -f "$1" ]; then cat "$1"; else echo "<absent>"; fi; }

echo "== open_append keeps what is there and writes at the end (2 written, 2 written, size 6)"
printf AB > "$data/log.bin"
echo "status $(status a "$data/log.bin"); log.bin=$(bytes "$data/log.bin")"
echo "== open_append creates a missing file"
echo "status $(status b "$data/new.bin"); new.bin=$(bytes "$data/new.bin")"

echo "== open_write truncates; open_new refuses a file that is there (EEXIST, 17: 100 + 17)"
printf 'previous contents' > "$data/old.bin"
printf keep > "$data/held.bin"
echo "status $(status c "$data/old.bin" "$data/held.bin"); old.bin=$(bytes "$data/old.bin") held.bin=$(bytes "$data/held.bin")"
echo "== open_new creates a file that is not there"
echo "status $(status d "$data/fresh.bin"); fresh.bin=$(bytes "$data/fresh.bin")"
echo "== open_rw refuses a missing file (ENOENT, 2: 100 + 2) and creates nothing"
echo "status $(status e "$data/absent.bin"); absent.bin=$(bytes "$data/absent.bin")"

echo "== pwrite and pread take an offset and leave the cursor alone (22 + 100 + 1000, modulo 256)"
printf AAAA > "$data/rw.bin"
echo "status $(status f "$data/rw.bin"); rw.bin=$(bytes "$data/rw.bin")"
echo "== pread past the end is End"
printf abc > "$data/short.bin"
echo "status $(status g "$data/short.bin")"
echo "== pwrite on an append handle still appends (Linux; docs/file-writes.md section 4.2)"
printf AAAA > "$data/aa.bin"
s=$(status h "$data/aa.bin")
if [ $linux = 1 ]; then echo "status $s; aa.bin=$(bytes "$data/aa.bin")"; else echo "status $s; aa.bin=AAAABB"; fi
echo "== truncate cuts and size leaves the cursor where it was"
printf 'hello world' > "$data/t.bin"
echo "status $(status i "$data/t.bin"); t.bin=$(bytes "$data/t.bin")"

echo "== a verb the kernel refuses answers Failed with the errno (EBADF 9, EBADF 9, EINVAL 22: one bit each)"
printf abc > "$data/r.bin"
printf abc > "$data/a.bin"
printf abc > "$data/w.bin"
echo "status $(status j "$data/r.bin" "$data/a.bin" "$data/w.bin"); r.bin=$(bytes "$data/r.bin") w.bin=$(bytes "$data/w.bin")"

echo "== a file created by an open has the default mode masked by the umask (0666 with umask 027)"
(umask 027; "$probe" l "$data/m.bin" > /dev/null 2>&1; echo "status $?")
echo "mode $(ls -l "$data/m.bin" | cut -c1-10)"

echo "== sync calls fsync on the descriptor it was given (strace, where it is installed on Linux)"
if [ $linux = 1 ] && command -v strace > /dev/null; then
  strace -f -e trace=openat,fsync,write -o "$dir/trace.txt" "$probe" k "$data/s.bin" > /dev/null 2>&1
  echo "status $?"
  echo "fsync calls: $(grep -c 'fsync(' "$dir/trace.txt")"
  fd=$(grep 'write(' "$dir/trace.txt" | grep '"x"' | head -n 1 | sed 's/^[^(]*(\([0-9]*\),.*/\1/')
  if grep -q "fsync($fd) *= 0" "$dir/trace.txt"; then echo "the fsync names the descriptor of the write of x and succeeded"; else echo "THE FSYNC DOES NOT NAME THE DESCRIPTOR OF THE WRITE: $(grep fsync "$dir/trace.txt")"; fi
else
  echo "status 1"
  echo "fsync calls: 1"
  echo "the fsync names the descriptor of the write of x and succeeded"
fi

echo "== an open outside the granted prefix traps, and creates nothing"
mkdir "$granted"
echo "o: $(killed o "$dir/elsewhere.bin"); created: $([ -e "$dir/elsewhere.bin" ] && echo yes || echo no)"

echo "== rename replaces the destination and leaves no old name"
printf replacement > "$data/new2.bin"
printf 'old contents' > "$data/live.bin"
echo "status $(status m "$data/new2.bin" "$data/live.bin"); live.bin=$(bytes "$data/live.bin") new2.bin=$(bytes "$data/new2.bin")"
echo "== remove unlinks a file and a second removal is ENOENT (1 + 2 * 10)"
printf x > "$data/gone.bin"
printf y > "$data/kept.bin"
echo "status $(status n "$data/gone.bin"); gone.bin=$(bytes "$data/gone.bin") kept.bin=$(bytes "$data/kept.bin")"

echo "== rename and remove refuse a path outside the prefix or with .. (a rename names two paths: out of the directory and in from outside are both writes outside it)"
printf in > "$granted/inside.bin"
printf out > "$outside"
check() {
  echo "$1: $2; inside=$(bytes "$granted/inside.bin") outside=$(bytes "$outside") moved=$([ -e "$dir/moved.bin" ] || [ -e "$granted/moved.bin" ] && echo yes || echo no)"
}
check rename-out "$(killed p "$granted/inside.bin" "$dir/moved.bin")"
check rename-in "$(killed p "$outside" "$granted/moved.bin")"
check rename-dotdot "$(killed p "$granted/inside.bin" "$granted/../cancho-fw-outside.bin")"
check remove-out "$(killed q "$outside")"
check remove-dotdot "$(killed q "$granted/../cancho-fw-outside.bin")"

echo "== a lock is refused while another process holds it and released when that process dies (EWOULDBLOCK)"
case $(uname) in Darwin) wouldblock=35 ;; *) wouldblock=11 ;; esac
# The holder takes the lock, says so on standard error and waits for a byte on its standard input; a contender then tries and is refused, the holder is killed, and the contender is admitted.
mkfifo "$dir/hold"
"$probe" s "$data/LOCK" < "$dir/hold" 2> "$dir/holder.err" > /dev/null &
holder=$!
exec 3> "$dir/hold"
n=0
while ! grep -q locked "$dir/holder.err" 2> /dev/null && [ $n -lt 100 ]; do sleep 0.1; n=$((n + 1)); done
echo "the holder says: $(head -n 1 "$dir/holder.err")"
s=$(status r "$data/LOCK")
if [ "$s" = $wouldblock ]; then echo "refused while the holder lives (EWOULDBLOCK)"; else echo "NOT REFUSED: exit $s"; fi
kill $holder
wait $holder 2> /dev/null
exec 3>&-
echo "admitted once the holder is gone: exit $(status r "$data/LOCK")"

rm -rf "$granted" "$outside"
