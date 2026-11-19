# confined.sh -- the script of tests/commands/confined.cho. $1 is the probe (tests/probes/confined_probe.cho, built), $2 a scratch directory.
# The probe opens each name with `dirs.confined.open_file` and prints `NAME opened` or `NAME err ERRNO`. A file with one name opens; a file with two names (`twin`, made with `ln`)
# is refused with EMLINK (31), at the top and beneath a directory; a missing name, `..` and an empty name answer as `std.dirs.open_file` does.
probe=$1
root=$2/confined
mkdir "$root" "$root/sub"
echo plain > "$root/plain"
echo twin > "$root/twin"
ln "$root/twin" "$root/twin2"
echo plain > "$root/sub/plain"
echo twin > "$root/sub/twin"
ln "$root/sub/twin" "$root/sub/twin2"

"$probe" "$root" || echo "probe exit $?"
