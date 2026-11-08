# directory_open_new_mode.sh -- the script of tests/commands/directory_open_new_mode.cho. $1 is the probe (tests/programs/directory_open_new_mode_probe.cho, built), $2 a scratch directory.
# The probe makes each (name, mode in decimal) it is given, writes `hello` to it and prints `ok` or `err ERRNO`; this prints what it said and what is on disk, under umask 022.
umask 022
probe=$1
root=$2/root
mkdir "$root"

bits() { stat -c %a "$1" 2>/dev/null || stat -f %Lp "$1"; }
run() { "$probe" "$root" "$@" || echo "probe exit $?"; }

# The mode is the program's, less the umask: 0o777 under 0o022 is 0o755. The executable bit is the point.
echo "== modes"
run x755 493 x700 448 x777 511 r644 420 z000 0
for n in x755 x700 x777 r644 z000; do echo "$n $(bits "$root/$n")"; done
echo "x755 holds $(cat "$root/x755")"

# A name that is taken is EEXIST (17), whatever holds it: nothing is replaced or written through.
echo "== taken"
echo KEEP > "$root/file"
mkdir "$root/dir"
ln -s elsewhere "$root/dangling"
run file 493 dir 493 dangling 493
echo "file holds $(cat "$root/file")"
echo "file $(bits "$root/file")"
[ -e "$root/elsewhere" ] && echo "a dangling link was followed"

# Not one component, or a mode that is not permission bits: EINVAL (22), and no file was made.
echo "== invalid"
run .. 493 "" 493 a/b 493 big 512 suid 2541 sticky 1023 negative -1
for n in big suid sticky negative a; do [ -e "$root/$n" ] && echo "$n was made"; done
echo done
