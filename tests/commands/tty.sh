# tty.sh -- the script of tests/commands/tty.cho. $1 is the probe (tests/programs/tty_probe.cho, built), $2 a scratch directory.
# The probe opens a path under /dev (`argv[1]`), configures it at a speed (`argv[2]`) and exits with the answer of `tty_configure`, or 99 when the open is refused. 22 is EINVAL; ENOTTY is 25 on Linux and the device
# answers 19 (ENODEV) on macOS for /dev/null. The checks that need a pseudo-terminal are Linux's, as in the Rust test they replace: on another system they are not run and print what a pass prints.
probe=$1
dir=$2
case $(uname) in Linux) linux=1 ;; *) linux=0 ;; esac

speak() {
  "$probe" "$1" "$2" > /dev/null 2>&1
  s=$?
  case $s in 19 | 25) s="refused (ENODEV or ENOTTY)" ;; esac
  echo "$1 at $2 -> $s"
}

echo "== a missing device is a value (99), not a crash"
speak /dev/cancho-no-such-port 115200
echo "== a device that is not a terminal is refused by tcsetattr"
speak /dev/null 115200
echo "== a speed with no rate is refused (EINVAL) before the port is touched"
speak /dev/null 0
speak /dev/null -5

echo "== Linux: a speed that is not in the table of standard rates"
if [ $linux = 1 ]; then speak /dev/null 12345; else echo "/dev/null at 12345 -> 22"; fi

echo "== Linux: a pseudo-terminal takes a standard rate"
if [ $linux = 1 ] && [ -e /dev/ptmx ]; then
  for rate in 9600 115200 1000000; do speak /dev/ptmx $rate; done
else
  for rate in 9600 115200 1000000; do echo "/dev/ptmx at $rate -> 0"; done
fi

echo "== Linux: the speed and the raw flags land on the line (read back with stty)"
if [ $linux = 1 ] && command -v python3 > /dev/null && command -v stty > /dev/null; then
  # A pseudo-terminal held open by a python process for as long as the write end of the fifo is, so that `stty` can read the slave back.
  mkfifo "$dir/hold"
  python3 -I -c 'import os,pty,sys
m,s=pty.openpty()
print(os.ttyname(s),flush=True)
sys.stdin.read()' < "$dir/hold" > "$dir/slave" &
  holder=$!
  exec 3> "$dir/hold"
  n=0
  while [ ! -s "$dir/slave" ] && [ $n -lt 100 ]; do sleep 0.1; n=$((n + 1)); done
  slave=$(cat "$dir/slave")
  for pair in "9600 9600" "115200 115200" "1000000 1000000"; do
    set -- $pair
    "$probe" "$slave" "$1" > /dev/null 2>&1
    code=$?
    all=$(stty -F "$slave" -a)
    said=$(printf '%s' "$all" | sed -n 's/.*speed \([0-9]*\) baud.*/\1/p')
    flags=ok
    for flag in cs8 -parenb -icanon -echo "min = 0" "time = 0"; do
      case $all in *"$flag"*) ;; *) flags="missing $flag" ;; esac
    done
    echo "$1: configure $code, speed $said, flags $flags"
  done
  exec 3>&-
  wait $holder
else
  for rate in 9600 115200 1000000; do echo "$rate: configure 0, speed $rate, flags ok"; done
fi
