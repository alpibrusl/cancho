# directory_modes.sh -- the script of tests/commands/directory_modes.cho. $1 is the probe (tests/programs/directory_modes_probe.cho, built), $2 a scratch directory.
# The probe prints the directory's own bits (`dir_own_mode`), then each name's (`dir_mode`): `NAME BITS` or `NAME err ERRNO`, bits in octal. The tree gets its bits after it is made, so
# the umask does not decide them. A symbolic link answers its own bits, never its target's: they are the ones `stat` reports for the link (they differ between systems), so that line
# says whether they are.
probe=$1
root=$2/modes
mkdir "$root"

# The permission bits of a path, as `chmod` writes them (the link itself, not what it points to).
bits() {
  stat -c %a "$1" 2>/dev/null || stat -f %Mp%Lp "$1" | sed 's/^0*//; s/^$/0/'
}

for pair in f600:600 f644:644 f400:400 f4755:4755 f000:0; do
  name=${pair%%:*}
  echo "$name" > "$root/$name"
  chmod "${pair#*:}" "$root/$name"
done
mkdir "$root/d0750" "$root/d1777"
chmod 750 "$root/d0750"
chmod 1777 "$root/d1777"
ln -s f600 "$root/link"
chmod 710 "$root"

"$probe" "$root" > "$2/said" || echo "probe exit $?"
while read -r name rest; do
  if [ "$name" = link ]; then
    if [ "$rest" = "$(bits "$root/link")" ]; then
      echo "link answers its own bits"
    else
      echo "link answers $rest, stat says $(bits "$root/link")"
    fi
  else
    echo "$name $rest"
  fi
done < "$2/said"
echo "on disk: root $(bits "$root"), f4755 $(bits "$root/f4755"), d1777 $(bits "$root/d1777")"
