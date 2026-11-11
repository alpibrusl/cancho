# directory_rmdir.sh -- the script of tests/commands/directory_rmdir.cho. $1 is the probe (tests/programs/directory_rmdir_probe.cho, built), $2 a scratch directory.
# The probe removes each name it is given and prints `ok` or `err ERRNO`; this prints what it said and what is on disk. ENOTEMPTY is 39 on Linux and 66 on Darwin
# (not measured there), so it is printed by name.
probe=$1
root=$2/root
mkdir "$root"

case $(uname -s) in Darwin) notempty=66 ;; *) notempty=39 ;; esac
run() { "$probe" "$root" "$@" | sed "s/^err $notempty\$/err ENOTEMPTY/" || echo "probe exit $?"; }

# Only an empty directory goes, and it is really gone.
echo "== empty"
mkdir "$root/empty"
run empty
[ -e "$root/empty" ] && echo "empty is still there"

# A directory with something in it is ENOTEMPTY, and it and what it holds are untouched. Removing what it holds first makes it go.
echo "== not empty"
mkdir "$root/full"
echo KEEP > "$root/full/file"
run full
echo "full holds $(cat "$root/full/file")"
rm "$root/full/file"
run full
[ -e "$root/full" ] && echo "full is still there"

# A missing name is ENOENT (2). A file, a link to a directory and a dangling link are ENOTDIR (20): the call never follows a link and never removes one.
echo "== not a directory"
mkdir "$root/target"
echo FILE > "$root/file"
ln -s target "$root/link"
ln -s elsewhere "$root/dangling"
run missing file link dangling
echo "file holds $(cat "$root/file")"
[ -d "$root/target" ] && echo "target is still a directory"
[ -L "$root/link" ] && echo "link is still a link"
[ -L "$root/dangling" ] && echo "dangling is still a link"

# Not one component: EINVAL (22), and no call was made (a directory two levels down is still there).
echo "== invalid"
mkdir -p "$root/a/b"
long=$(printf 'x%.0s' $(seq 1 256))
fits=$(printf 'x%.0s' $(seq 1 255))
run .. . "" a/b "$long" "$fits"
[ -d "$root/a/b" ] && echo "a/b is still there"
echo done
