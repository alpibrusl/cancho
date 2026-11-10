# directory_links.sh -- the script of tests/commands/directory_links.cho. $1 is the probe (tests/programs/directory_links_probe.cho, built), $2 a scratch directory.
# The probe makes (`s NAME TARGET`) and reads (`r NAME ROOM`) links beneath a directory and prints `ok`, `ok N TEXT` or `err ERRNO`; this prints what it said and what is on
# disk. ENAMETOOLONG is 36 on Linux and 63 on Darwin (not measured there), so it is printed by name. Lines are cut at 40 columns: the long targets are checked by length.
probe=$1
root=$2/root
mkdir "$root"

case $(uname -s) in Darwin) toolong=63 ;; *) toolong=36 ;; esac
run() { "$probe" "$root" "$@" | sed "s/^err $toolong\$/err ENAMETOOLONG/" | cut -c1-40 || echo "probe exit $?"; }
x255=$(printf 'x%.0s' $(seq 1 255))
x4095=$(printf 'x%.0s' $(seq 1 4095))
x4096=$(printf 'x%.0s' $(seq 1 4096))

# What is made is a link, with the text given; what is read is that text.
echo "== make and read"
run s one x s seven 1234567 s l255 "$x255" s l4095 "$x4095"
run r one 2 r seven 8 r l255 256 r l4095 4096
for n in one seven l255 l4095; do [ -L "$root/$n" ] && echo "$n is a link of $(printf %s "$(readlink "$root/$n")" | wc -c | tr -d ' ') bytes"; done
[ "$(readlink "$root/seven")" = 1234567 ] && echo "seven is what the shell reads too"

# The kernel cuts a long target without saying so; a buffer that fills is ENAMETOOLONG, so the buffer must be one byte longer than the target.
echo "== the buffer"
run r seven 7 r seven 8 r seven 4 r seven 1 r seven 0

# A name that is taken is EEXIST (17), whatever holds it, a dangling link included, and what held it is untouched.
echo "== taken"
echo KEEP > "$root/file"
mkdir "$root/dir"
ln -s nowhere "$root/dangling"
run s seven y s file y s dir y s dangling y
echo "file holds $(cat "$root/file")"
echo "dangling is still $(readlink "$root/dangling")"
[ -e "$root/nowhere" ] && echo "a dangling link was followed"

# A target that is empty, or 4096 bytes or longer, is EINVAL and no link is made (the kernel would say ENOENT and ENAMETOOLONG: the copy is 4096 bytes).
echo "== invalid targets"
run s empty "" s long "$x4096"
for n in empty long; do [ -L "$root/$n" ] && echo "$n was made"; done

# Not one component: EINVAL (22), and no call was made.
echo "== invalid names"
run s .. x s . x s "" x s a/b x r .. 8 r . 8 r "" 8 r a/b 8
[ -e "$root/a" ] && echo "a was made"

# A file and a directory are not links: EINVAL (22). A missing name is ENOENT (2).
echo "== not a link"
run r file 8 r dir 8 r missing 8
echo done
