# directory_nlink.sh -- the script of tests/commands/directory_nlink.cho. $1 is the probe (tests/probes/directory_nlink_probe.cho, built), $2 a scratch directory.
# The probe prints, for each name beneath the directory, `NAME COUNT` (how many names the file has, `st_nlink`) or `NAME err ERRNO`. A hard link is another name for the
# same file: `twin` and `twin2` are two names, `three` has three. A symbolic link answers its own count, 1, never its target's. `.`, `..` and a name with a `/` are refused
# before any call, as for `dir_mode`.
probe=$1
root=$2/links
mkdir "$root"
echo plain > "$root/plain"
echo twin > "$root/twin"
ln "$root/twin" "$root/twin2"
echo three > "$root/three"
ln "$root/three" "$root/three_b"
ln "$root/three" "$root/three_c"
ln -s plain "$root/link"

"$probe" "$root" || echo "probe exit $?"
