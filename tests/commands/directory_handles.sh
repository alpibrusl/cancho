# directory_handles.sh -- the script of tests/commands/directory_handles.cho. $1 is the probe (tests/programs/directory_handles_probe.cho, built), $2 a scratch directory.
# The probe opens a directory (`argv[1]`) and a path beneath it (`argv[2]`) in a mode (`argv[3]`: `walk` is `dirs.open_file`, `one` hands the path to `dir_open_read` as one name, `enter` walks to a
# directory with `dirs.enter`, `n` hands `dir_open_read` a name holding a NUL) and prints `ok BYTES-READ` or `err ERRNO`; it exits 2 when `open_dir` itself failed. The tree: `root/a/b/f.txt`
# (seven bytes), `root/link.txt` to a file outside, `root/dirlink` to the directory outside, and `outside/s.txt` (14 bytes), which no probe may read.
probe=$1
dir=$2
root=$dir/root
mkdir -p "$root/a/b" "$dir/outside"
echo "inside" > "$root/a/b/f.txt"
echo "SECRET-SECRET" > "$dir/outside/s.txt"
ln -s ../outside/s.txt "$root/link.txt"
ln -s ../outside "$root/dirlink"

# ELOOP is 62 on macOS and 40 on Linux, ENOTDIR 20: the same names in the transcript. A probe that read the outside file says `ok 14`.
case $(uname) in Darwin) eloop=62 ;; *) eloop=40 ;; esac
spell() { sed -e "s/^err $eloop\$/err ELOOP/" -e 's/^err 20$/err ENOTDIR/' -e 's/^err 22$/err EINVAL/' -e 's/^err 2$/err ENOENT/' -e 's/^ok 14$/READ THE OUTSIDE FILE/'; }
run() {
  label=$1
  [ "${#label}" -gt 40 ] && label="${#label} x"
  echo "[$label] $2 -> $("$probe" "$root" "$1" "$2" | spell)"
}
# The outcomes a platform may choose between (a link entered on the way is ENOTDIR or ELOOP): both are refusals, neither a read.
either() {
  said=$("$probe" "$root" "$1" "$2" | spell)
  case $said in "err ENOTDIR" | "err ELOOP") said="refused (ENOTDIR or ELOOP)" ;; esac
  echo "[$1] $2 -> $said"
}

echo "== the file two directories down, through the walk, and the directory above it"
run a/b/f.txt walk
run a/b enter
echo "== a link to a file outside, last component: refused, not read"
run link.txt walk
run link.txt one
echo "== a link to a directory outside, entered on the way: refused"
either dirlink/s.txt walk
either dirlink enter
echo "== what O_NOFOLLOW does not cover, refused by the component check"
run ../outside/s.txt walk
run a/../a/b/f.txt walk
run ./a/b/f.txt walk
run a//b/f.txt walk
run /a/b/f.txt walk
run a/b/ walk
run "" walk
run .. enter
echo "== the builtin alone: one name, so a / . and .. are refused without a call"
run a/b/f.txt one
run .. one
run . one
run "" one
echo "== ordinary failures are values"
run a/b/missing walk
run a/b/f.txt/x walk
echo "== a name longer than NAME_MAX is refused before the copy; 255 bytes reach the kernel"
long=$(head -c 256 /dev/zero | tr '\0' x)
run "$long" one
long=$(head -c 255 /dev/zero | tr '\0' x)
run "$long" one
echo "== a NUL inside a name, which argv cannot carry"
run x n
echo "== open_dir of a file, and of nothing: open_dir itself answers (exit 2), not a step after it"
echo "file -> $("$probe" "$root/a/b/f.txt" x walk | spell)"
"$probe" "$root/a/b/f.txt" x walk > /dev/null
echo "file exit $?"
echo "absent -> $("$probe" "$dir/absent" x walk | spell)"
"$probe" "$dir/absent" x walk > /dev/null
echo "absent exit $?"
