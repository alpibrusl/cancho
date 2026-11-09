# directory_rename_new.sh -- the script of tests/commands/directory_rename_new.cho. $1 is the probe (tests/programs/directory_rename_new_probe.cho, built), $2 a scratch directory.
# The rename that never replaces (`docs/directory-handles.md` section 3, slice 4): `dir_rename_new` is renameat2(RENAME_NOREPLACE) on Linux and renameatx_np(RENAME_EXCL) on Darwin. The probe takes a directory (`argv[1]`) and
# pairs of names (`argv[2..]`), renames each pair in order and prints `ok` or `err ERRNO` for each. Two checks: a table (a free name, a name that holds a file, a link, a directory, a missing source, names that are not one
# component), each followed by what is on disk; and a race, in which this script creates the destination, exclusively, at the moment the program renames onto it, and one of the two must lose.
probe=$1
dir=$2
root=$dir/root
outside=$dir/outside
mkdir -p "$root/subdir" "$outside"
printf OUTSIDE > "$outside/target.txt"

# 17 is EEXIST, 2 ENOENT, 22 EINVAL on both systems.
spell() { sed -e 's/^err 17$/EEXIST/' -e 's/^err 2$/ENOENT/' -e 's/^err 22$/EINVAL/'; }
rn() { "$probe" "$root" "$@" | spell | tr "\n" " "; echo; }
# What a name holds, or `<absent>`, and whether two names are one file.
read_of() { if [ -f "$1" ]; then cat "$1"; else echo "<absent>"; fi; }
inode() { ls -i "$1" | awk '{ print $1 }'; }

echo "== a free destination: the source moves, byte for byte, and the inode is the same file's"
printf AAAA > "$root/a"
before=$(inode "$root/a")
rn a free
echo "a=$(read_of "$root/a") free=$(read_of "$root/free")"
if [ "$(inode "$root/free")" = "$before" ]; then echo "same inode"; else echo "DIFFERENT inode"; fi

echo "== an existing destination: refused with EEXIST, its bytes and the source's untouched"
printf 'NEW!' > "$root/src"
printf KEEP > "$root/held"
held=$(inode "$root/held")
rn src held
echo "src=$(read_of "$root/src") held=$(read_of "$root/held")"
if [ "$(inode "$root/held")" = "$held" ]; then echo "same inode"; else echo "DIFFERENT inode"; fi

echo "== a destination that is a link, dangling or not, is a name that is taken: the link stays and nothing is written through it"
ln -s ../outside/target.txt "$root/link"
ln -s ../outside/absent.txt "$root/dangling"
rn src link src dangling
echo "src=$(read_of "$root/src") target=$(read_of "$outside/target.txt") absent=$(read_of "$outside/absent.txt")"
[ -L "$root/link" ] && [ -L "$root/dangling" ] && echo "both links stayed"

echo "== a destination that is a directory (empty, which a plain rename of a directory would replace)"
mkdir "$root/emptydir" "$root/srcdir"
rn src subdir srcdir emptydir srcdir held
[ -d "$root/srcdir" ] && [ -d "$root/emptydir" ] && echo "both directories stayed"

echo "== a directory onto a free name moves; a missing source is ENOENT, even onto a taken name"
rn srcdir movedir
if [ -d "$root/movedir" ] && [ ! -e "$root/srcdir" ]; then echo "moved"; fi
rn missing held missing nowhere
echo "held=$(read_of "$root/held")"

echo "== names that are not one component are refused before any call (EINVAL, never what a filesystem without the flag answers)"
rn src ../escaped .. x src "" src a/b src .
echo "escaped=$(read_of "$dir/escaped") src=$(read_of "$root/src")"

echo "== a rival creating the destination loses or wins but is never overwritten"
trials=300
# One round: the program renames s<i> to d<i> for each i, while this script, in the background, creates d<i> exclusively with RIVAL, with a head start of a few trials on two trials in three. Every third trial is a control the
# rival leaves alone, so that s<i> always goes and the program's progress can be read from the directory. Exactly one of the two may win a name: if the rename said ok, the rival's create was refused and the file holds the
# source's bytes; if it said EEXIST, the rival's create succeeded, the file holds the rival's bytes and the source is still there.
program_won=0
rival_won=0
bad=0
round=0
while [ $round -lt 20 ] && { [ $rival_won -eq 0 ] || [ $program_won -eq 0 ]; }; do
  r=$dir/race$round
  mkdir "$r"
  pairs=
  i=0
  while [ $i -lt $trials ]; do
    printf SOURCE > "$r/s$i"
    pairs="$pairs s$i d$i"
    i=$((i + 1))
  done
  (
    set -C
    i=0
    while [ $i -lt $trials ]; do
      if [ $((i % 3)) -ne 0 ]; then
        if [ $((i % 3)) -eq 1 ]; then b=$((i - 4)); else b=$((i - 8)); fi
        [ $b -lt 0 ] && b=0
        b=$((b - b % 3))
        n=0
        while [ -e "$r/s$b" ] && [ $n -lt 3000000 ]; do n=$((n + 1)); done
        if { printf RIVAL > "$r/d$i"; } 2> /dev/null; then echo "$i made"; else echo "$i refused"; fi
      fi
      i=$((i + 1))
    done > "$r.rival"
  ) &
  rival=$!
  # shellcheck disable=SC2086
  "$probe" "$r" $pairs > "$r.answers"
  wait $rival
  exec 4< "$r.answers" 5< "$r.rival"
  i=0
  while [ $i -lt $trials ]; do
    read -r said <&4
    made=
    if [ $((i % 3)) -ne 0 ]; then read -r _ made <&5; fi
    held="<absent>"
    [ -f "$r/d$i" ] && read -r held < "$r/d$i"
    source="<absent>"
    [ -f "$r/s$i" ] && read -r source < "$r/s$i"
    if [ "$said" = ok ] && [ "$made" != made ] && [ "$held" = SOURCE ] && [ "$source" = "<absent>" ]; then
      program_won=$((program_won + 1))
    elif [ "$said" = "err 17" ] && [ "$made" = made ] && [ "$held" = RIVAL ] && [ "$source" = SOURCE ]; then
      rival_won=$((rival_won + 1))
    else
      bad=$((bad + 1))
      echo "trial $i of round $round: the rename said '$said' and the rival '$made'; d$i holds '$held', s$i holds '$source'" >&2
    fi
    i=$((i + 1))
  done
  exec 4<&- 5<&-
  round=$((round + 1))
done
echo "trials that broke the rule: $bad"
if [ $rival_won -gt 0 ]; then echo "the rival won some names"; else echo "the rival never created a name first in 20 rounds"; fi
if [ $program_won -gt 0 ]; then echo "the program won some names"; else echo "the program never won a name"; fi
