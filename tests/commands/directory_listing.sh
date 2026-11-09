# directory_listing.sh -- the script of tests/commands/directory_listing.cho. $1 is the probe (tests/programs/directory_listing_probe.cho, built), $2 a scratch directory.
# The probe (`docs/directory-listing.md`, issue #222) takes a directory (`argv[1]`) and a mode (`argv[2]`): `sorted` prints `std.dirs.list`'s answer, `raw` every `dir_next` answer in the kernel's order, `short` the first
# answer into a three-byte buffer, `cap` `std.dirs.list` with at most two names, `twice` two listings of one directory read in turns, `statall` `dir_stat` on every sorted name and then on five names that are not one component.
# A name is a line `<hex> <kind>` (1 file, 2 directory, 3 link, 4 other), so that a newline or a byte that is not UTF-8 survives the trip. The oracle of a listing is `find` over the same directory, in bulk, one pass per kind.
probe=$1
dir=$2
cd "$dir" || exit 2
case $(uname) in Darwin) toolong=63 ;; *) toolong=36 ;; esac

# The names of a directory, one `<hex> <kind>` line each, sorted as bytes: from `find` (the names NUL-separated, so a newline is safe) and `od`, a pass for each kind.
oracle() {
  (
    cd "$1" || exit 2
    pass() {
      find . -mindepth 1 -maxdepth 1 "$@" -print0 | od -An -v -tx1 | awk -v kind="$kind" '{ for (i = 1; i <= NF; i++) { if ($i == "00") { print substr(cur, 5) " " kind; cur = "" } else cur = cur $i } }'
    }
    kind=1 pass -type f
    kind=2 pass -type d
    kind=3 pass -type l
    kind=4 pass ! -type f ! -type d ! -type l
  ) | LC_ALL=C sort
}

# The hostile directory of section 6: a newline, a byte that is not UTF-8 (APFS refuses to create one, so on macOS it is left out), a 255-byte name, a dangling link, a link to `..`, a FIFO and a subdirectory, among $2 plain files.
hostile() {
  mkdir -p "$1/sub"
  (
    cd "$1" || exit 2
    seq -f 'f%06.0f' 0 $(($2 - 1)) | xargs touch
    touch "$(printf 'new\nline')" cafe Z -dash
    if [ "$(uname)" != Darwin ]; then : > "$(printf 'caf\351')"; fi
    touch "$(head -c 255 /dev/zero | tr '\0' x)"
    ln -s nowhere dangling
    ln -s .. up
    mkfifo fifo
  )
}

echo "== a hostile directory of 100,000 plain files lists sorted, and dir_next alone sees the same entries"
hostile "$dir/hostile" 100000
oracle "$dir/hostile" > "$dir/want"
"$probe" "$dir/hostile" sorted > "$dir/sorted"
if cmp -s "$dir/sorted" "$dir/want"; then echo "std.dirs.list: the entries of find, sorted as bytes"; else echo "std.dirs.list: DIFFERS from find"; fi
"$probe" "$dir/hostile" raw | LC_ALL=C sort > "$dir/raw"
if cmp -s "$dir/raw" "$dir/want"; then echo "dir_next: the same entries"; else echo "dir_next: DIFFERS from find"; fi
if [ "$(wc -l < "$dir/want")" -ge 100008 ]; then echo "the tree: 100,000 plain files and the hostile ones"; else echo "the tree is too small: $(wc -l < "$dir/want")"; fi

echo "== a short buffer, a cap and two listings"
mkdir small only
touch small/longname small/b small/a only/longname
echo "short: $("$probe" "$dir/only" short | sed "s/^err $toolong\$/err ENAMETOOLONG/")"
"$probe" "$dir/small" cap > "$dir/cap"
echo "cap: $(wc -l < "$dir/cap" | tr -d ' ') lines, the last is $(tail -n 1 "$dir/cap")"
"$probe" "$dir/small" twice | LC_ALL=C sort > "$dir/twice"
oracle "$dir/small" > "$dir/once"
LC_ALL=C sort "$dir/once" "$dir/once" > "$dir/doubled"
if cmp -s "$dir/twice" "$dir/doubled"; then echo "twice: every name twice"; else echo "twice: DIFFERS"; fi

echo "== status is lstat and never follows a link (the FIFO is among the entries: a status that opened it would block here)"
# Every entry has the same whole-second modification time, links included; a directory's size is the filesystem's own and is left out.
mkdir stat
(
  cd stat || exit 2
  seq -f 'f%06.0f' 0 19 | xargs touch
  touch "$(printf 'new\nline')" cafe Z -dash
  printf 'twelve bytes' > sized.txt
  touch "$(head -c 255 /dev/zero | tr '\0' x)"
  mkdir sub
  ln -s nowhere dangling
  ln -s .. up
  mkfifo fifo
  TZ=UTC touch -h -t 202001020304.05 ./* ./-dash
)
"$probe" "$dir/stat" statall | awk 'NR % 2 == 1 && $2 == 2 { dir = 1 } { if (!(NR % 2) && dir) { $1 = "SIZE"; dir = 0 } print }'
