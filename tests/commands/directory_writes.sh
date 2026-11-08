# directory_writes.sh -- the script of tests/commands/directory_writes.cho. $1 is the probe (tests/programs/directory_writes_probe.cho, built), $2 a scratch directory.
# The probe opens `root` and does one thing beneath it: `new NAME` and `append NAME` write `hello` to what they open, `move FROM TO`, `remove NAME` and `sync` answer a `Done`; it prints
# `ok N` or `err ERRNO`. This runs each against a tree with a link to a file outside and a dangling link whose target is outside, and prints what the probe said and what is on disk.
# Nothing outside `root` may be created, changed or removed.
umask 022
probe=$1
dir=$2
root=$dir/root
outside=$dir/outside
mkdir "$root" "$outside"
printf KEPT > "$outside/kept.txt"
ln -s ../outside/kept.txt "$root/link.txt"
ln -s ../outside/new.txt "$root/dangling.txt"

# ELOOP is 62 on macOS and 40 on Linux: the same name in the transcript.
case $(uname) in Darwin) eloop=62 ;; *) eloop=40 ;; esac
run() { echo "$* -> $("$probe" "$root" "$@" | sed "s/^err $eloop\$/err ELOOP/")"; }
# What a file holds, or `<absent>`.
holds() { if [ -e "$1" ]; then echo "$1 holds $(cat "$1")" | sed "s#$dir/##"; else echo "$1 <absent>" | sed "s#$dir/##"; fi; }

echo "== create: once, then refused (EEXIST 17); through either link, refused, and nothing appears outside"
run new f
holds "$root/f"
run new f
run new link.txt
run new dangling.txt
holds "$outside/new.txt"

echo "== append: created if missing, then added to; through a link, refused"
run append log
run append log
holds "$root/log"
run append link.txt
run append dangling.txt
holds "$outside/kept.txt"
holds "$outside/new.txt"

echo "== rename within the directory; every name is one component (EINVAL 22)"
run move f g
holds "$root/f"
holds "$root/g"
run move g ../escaped
run move .. x
run move g ""
holds "$dir/escaped"
run move missing x

echo "== remove: a file, a link (the link goes, its target stays), nothing, and not a component"
run remove g
holds "$root/g"
run remove link.txt
if [ -L "$root/link.txt" ]; then echo "the link stayed"; else echo "the link is gone"; fi
holds "$outside/kept.txt"
run remove missing
run remove ../outside/kept.txt
holds "$outside/kept.txt"

echo "== sync the directory itself"
run sync

# A file made by `dir_open_new` or `dir_open_append` gets mode 0644 (under umask 022), as one made by `open_new` does: on Apple AArch64 the mode is a variadic argument on the stack, so a call
# shaped wrongly there gives junk bits.
echo "== the mode of a created file"
mkdir "$dir/modes"
"$probe" "$dir/modes" new made > /dev/null
"$probe" "$dir/modes" append added > /dev/null
for n in made added; do echo "$n $(stat -c %a "$dir/modes/$n" 2>/dev/null || stat -f %Lp "$dir/modes/$n")"; done
