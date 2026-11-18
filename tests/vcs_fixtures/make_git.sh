# make_git.sh -- how tests/vcs_fixtures/git/ was made: small bare git repositories for `cancho vcs import-git` (`docs/vcs-routing.md` section 16, R14), and the documents the Rust command prints
# for `--dry-run` on each. Written once with the `git` binary (as a measuring tool only: the commands never run it, and the gate does not, the repositories are committed files) and the Rust command.
#
#   sh tests/vcs_fixtures/make_git.sh RUST_CANCHO OUTDIR      writes OUTDIR/git/NAME.git and OUTDIR/golden_git/NAME.json (and NAME.code, the exit status)
#
# Objects that git would not write (a tree with a mode git refuses, a name that is not UTF-8, two entries of one name) are made with `git hash-object --literally`, from bytes printed here.
set -eu
R=$1
out=$2
mkdir -p "$out"
out=$(cd "$out" && pwd)
case $R in */*) R=$(cd "$(dirname "$R")" && pwd)/$(basename "$R") ;; esac
rm -rf "$out/git" "$out/golden_git" "$out/work"
mkdir -p "$out/git" "$out/golden_git" "$out/work"
G=$out/git
W=$out/work
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null HOME=$W
export GIT_AUTHOR_NAME="Ada" GIT_AUTHOR_EMAIL="ada@example.com" GIT_COMMITTER_NAME="Ada" GIT_COMMITTER_EMAIL="ada@example.com"
tick=1700000000
stamp() { tick=$((tick + 60)); export GIT_AUTHOR_DATE="$tick +0000" GIT_COMMITTER_DATE="$tick +0000"; }
bare() { # SOURCE NAME: a bare clone with nothing but objects and refs
  git clone -q --bare --template= "$1" "$G/$2.git"
  rm -f "$G/$2.git/config" "$G/$2.git/description"; rm -rf "$G/$2.git/hooks" "$G/$2.git/info/exclude"
  mkdir -p "$G/$2.git/refs/heads" "$G/$2.git/refs/tags"
  chmod -R u+w "$G/$2.git"
  # every ref loose (a clone packs them), so each fixture says which of the two a ref is in
  ( cd "$G/$2.git" && git for-each-ref --format='%(refname) %(objectname)' | while read -r r o; do mkdir -p "$(dirname "$r")"; printf '%s\n' "$o" > "$r"; done; rm -f packed-refs )
}

# ---- plain: a merge, an executable, an empty file, a link, an annotated and a lightweight tag, a commit with a signature header
mkdir "$W/plain" && cd "$W/plain"
git init -q -b main .
stamp; printf 'one\n' > a.txt; mkdir dir; printf 'bee\n' > dir/b.txt; : > empty; printf '#!/bin/sh\necho hi\n' > run.sh; chmod +x run.sh; ln -s a.txt link
git add -A; git commit -q -m "first"
git tag light
git checkout -q -b topic
stamp; printf 'bee two\n' > dir/b.txt; git commit -q -am "topic: change b"
git checkout -q main
stamp; printf 'one two\n' > a.txt; git commit -q -am "main: change a"
stamp; git merge -q --no-ff topic -m "merge topic"
stamp; git tag -a v1 -m "release 1"
git cat-file commit HEAD | awk 'BEGIN{done=0} {print} /^committer /&&!done{print "gpgsig -----BEGIN PGP SIGNATURE-----"; print " "; print " iQEzBAABCAAdFiEEfake"; print " -----END PGP SIGNATURE-----"; done=1}' > "$W/signed.commit"
sid=$(git hash-object -t commit -w --stdin < "$W/signed.commit")
git update-ref refs/heads/signed "$sid"
bare "$W/plain" plain

# ---- packed: a history that deltifies, then `git gc` (ofs-delta)
mkdir "$W/packed" && cd "$W/packed"
git init -q -b main .
i=0
while [ $i -lt 14 ]; do
  stamp; i=$((i + 1))
  { n=0; while [ $n -lt 60 ]; do echo "line $n of the long file, revision $((i / 3))"; n=$((n + 1)); done; echo "tail $i"; } > long.txt
  printf 'v%s\n' "$i" > small.txt
  git add -A; git commit -q -m "revision $i"
done
bare "$W/packed" packed
git -C "$G/packed.git" gc -q --prune=now
chmod -R u+w "$G/packed.git"

# ---- refdelta: the same objects in a pack whose deltas name their base by id (`pack-objects` without --delta-base-offset)
mkdir -p "$G/refdelta.git/objects/pack" "$G/refdelta.git/refs/heads" "$G/refdelta.git/refs/tags"
cp "$G/packed.git/HEAD" "$G/refdelta.git/HEAD"
git -C "$G/packed.git" rev-parse main > "$G/refdelta.git/refs/heads/main"
git -C "$G/packed.git" rev-list --objects --all | cut -d' ' -f1 | git -C "$G/packed.git" pack-objects -q --no-reuse-delta "$G/refdelta.git/objects/pack/pack" > /dev/null
git -C "$G/refdelta.git" index-pack "$G"/refdelta.git/objects/pack/pack-*.pack > /dev/null

# ---- packedrefs: the refs of plain only in packed-refs (the annotated tag peeled)
cp -R "$G/plain.git" "$G/packedrefs.git"; chmod -R u+w "$G/packedrefs.git"
git -C "$G/packedrefs.git" pack-refs --all --prune

# ---- alternate: no objects of its own; they are plain's, by a relative path
mkdir -p "$G/alternate.git/objects/info" "$G/alternate.git/refs/heads"
printf '../../plain.git/objects\n' > "$G/alternate.git/objects/info/alternates"
printf 'ref: refs/heads/main\n' > "$G/alternate.git/HEAD"
git -C "$G/plain.git" rev-parse main > "$G/alternate.git/refs/heads/main"

# ---- odd commits: an octopus merge, a commit that is not UTF-8, a commit with no blank line (it does not render as itself), a tree id written in capitals
mkdir "$W/odd" && cd "$W/odd"
git init -q -b main .
stamp; printf 'x\n' > x; git add -A; git commit -q -m base; b=$(git rev-parse HEAD)
for n in 1 2 3; do git checkout -q -b "br$n" "$b"; stamp; printf 'n%s\n' "$n" > "f$n"; git add -A; git commit -q -m "branch $n"; done
git checkout -q main
stamp
oct=$(printf 'merge three\n' | git commit-tree "$(git write-tree)" -p "$b" -p "$(git rev-parse br1)" -p "$(git rev-parse br2)" -p "$(git rev-parse br3)")
git update-ref refs/heads/octopus "$oct"
tree=$(git rev-parse "main^{tree}")
nonutf=$(printf 'tree %s\nauthor Ada <ada@example.com> 1700009000 +0000\ncommitter Ada <ada@example.com> 1700009000 +0000\n\ncaf\351 not utf-8\n' "$tree" | git hash-object -t commit -w --literally --stdin)
git update-ref refs/heads/nonutf8 "$nonutf"
nobreak=$(printf 'tree %s\nauthor Ada <ada@example.com> 1700009001 +0000\ncommitter Ada <ada@example.com> 1700009001 +0000\n' "$tree" | git hash-object -t commit -w --literally --stdin)
git update-ref refs/heads/nobreak "$nobreak"
upper=$(printf 'tree %s\nauthor Ada <ada@example.com> 1700009002 +0000\ncommitter Ada <ada@example.com> 1700009002 +0000\n\nupper tree id\n' "$(echo "$tree" | tr a-f A-F)" | git hash-object -t commit -w --literally --stdin)
git update-ref refs/heads/upper "$upper"
bare "$W/odd" odd

# ---- trees git would not write, one repository each: plain's objects, one more tree and a commit of it, and `main` there and nowhere else
bl=$(git -C "$G/plain.git" rev-parse main:a.txt)
tr0=$(git -C "$G/plain.git" rev-parse "main^{tree}")
entry() { # MODE NAME ID: the bytes of one tree entry (the name as printf reads it)
  printf '%s ' "$1"; printf "$2"; printf '\0'; printf '%s' "$3" | xxd -r -p
}
bad() { # NAME TREEFILE: the repository NAME, its main a commit of the tree in TREEFILE
  rm -rf "$G/$1.git"; cp -R "$G/plain.git" "$G/$1.git"; chmod -R u+w "$G/$1.git"
  rm -f "$G/$1.git/refs/heads/"* "$G/$1.git/packed-refs"; rm -rf "$G/$1.git/refs/tags/"*
  stamp
  t=$(git --git-dir="$G/$1.git" hash-object -t tree -w --literally --stdin < "$2")
  c=$(printf 'tree %s\nauthor Ada <ada@example.com> %s +0000\ncommitter Ada <ada@example.com> %s +0000\n\nbad tree\n' "$t" "$tick" "$tick" | git --git-dir="$G/$1.git" hash-object -t commit -w --literally --stdin)
  printf '%s\n' "$c" > "$G/$1.git/refs/heads/main"
}
mkdir "$W/t"
entry 160000 sub "$bl" > "$W/t/submodule"
entry 100664 f "$bl" > "$W/t/badmode"
entry 100644 '\377x' "$bl" > "$W/t/nonutf8name"
entry 100644 .git "$bl" > "$W/t/gitdir"
entry 100644 'a\nb' "$bl" > "$W/t/newline"
{ entry 100644 a "$bl"; entry 100644 a "$bl"; } > "$W/t/dupname"
{ entry 100644 a "$bl"; entry 40000 a "$tr0"; } > "$W/t/filedir"
{ entry 100644 a "$bl"; entry 100644 a-b "$bl"; entry 40000 a "$tr0"; } > "$W/t/filedir_apart"
entry 100644 .cancho "$bl" > "$W/t/canchodir"
{ entry 100644 'zz\nz' "$bl"; entry 100644 .git "$bl"; } > "$W/t/twobad"
printf '100689 f\0' > "$W/t/octal89"; printf '%s' "$bl" | xxd -r -p >> "$W/t/octal89"
entry 100644 .. "$bl" > "$W/t/dotdot"
entry 100644 '' "$bl" > "$W/t/emptyname"
entry 100644 f "$tr0" > "$W/t/blobkind"
entry 40000 d "$bl" > "$W/t/treekind"
printf '100644 noterminator' > "$W/t/badtree"
printf '100644 shortid\0abc' > "$W/t/truncatedtree"
printf 'zz f\0' > "$W/t/badmodetext"
for n in submodule badmode nonutf8name gitdir newline dupname filedir filedir_apart canchodir twobad octal89 dotdot emptyname blobkind treekind badtree truncatedtree badmodetext; do bad "$n" "$W/t/$n"; done

# ---- refusals of the objects themselves: a loose object with a damaged zlib stream, a truncated pack, a ref to an object that is not there, a ref to a blob
cp -R "$G/plain.git" "$G/corrupt.git"; chmod -R u+w "$G/corrupt.git"
f=$(git -C "$G/plain.git" rev-parse main:dir/b.txt)
printf 'not a zlib stream at all' > "$G/corrupt.git/objects/$(printf %s "$f" | cut -c1-2)/$(printf %s "$f" | cut -c3-)"
cp -R "$G/packed.git" "$G/truncated.git"; chmod -R u+w "$G/truncated.git"
p=$(ls "$G"/truncated.git/objects/pack/*.pack)
size=$(wc -c < "$p"); head -c $((size / 2)) "$p" > "$p.cut"; mv "$p.cut" "$p"
cp -R "$G/plain.git" "$G/missing.git"; chmod -R u+w "$G/missing.git"
printf '0123456789012345678901234567890123456789\n' > "$G/missing.git/refs/heads/ghost"
cp -R "$G/plain.git" "$G/blobref.git"; chmod -R u+w "$G/blobref.git"
printf '%s\n' "$bl" > "$G/blobref.git/refs/heads/blobref"
cp -R "$G/plain.git" "$G/detached.git"; chmod -R u+w "$G/detached.git"
rm -f "$G/detached.git/refs/heads/signed"
printf '%s\n' "$sid" > "$G/detached.git/HEAD"
cp -R "$G/plain.git" "$G/remotes.git"; chmod -R u+w "$G/remotes.git"
rm -f "$G/remotes.git/refs/heads/signed"
mkdir -p "$G/remotes.git/refs/remotes/origin"
printf '%s\n' "$sid" > "$G/remotes.git/refs/remotes/origin/signed"
printf '# pack-refs with: peeled fully-peeled sorted \n%s refs/remotes/up/topic\n' "$(git -C "$G/plain.git" rev-parse topic)" > "$G/remotes.git/packed-refs"
cp -R "$G/plain.git" "$G/tags.git"; chmod -R u+w "$G/tags.git"
rm -f "$G/tags.git/refs/heads/signed" "$G/tags.git/refs/heads/topic" "$G/tags.git/refs/heads/main"
stamp
orphan=$(printf 'orphan\n' | git --git-dir="$G/tags.git" commit-tree "$tr0")
git --git-dir="$G/tags.git" tag -a orphan_tag -m "tag of an orphan" "$orphan"
git --git-dir="$G/tags.git" tag -a tag_of_tag -m "tag of a tag" orphan_tag
printf 'ref: refs/heads/nothing\n' > "$G/tags.git/HEAD"
mkdir -p "$G/notgit.git"

# ---- odd objects: commits whose headers are not in git's order, a tag with no object line, a remote only in packed-refs, an alternate that names an alternate
cp -R "$G/plain.git" "$G/headers.git"; chmod -R u+w "$G/headers.git"
rm -f "$G/headers.git/refs/heads/"*
h1=$(printf 'author Ada <ada@example.com> 1700009100 +0000\ntree %s\ntree %s\nparent %s\ncommitter Ada <ada@example.com> 1700009100 +0000\n\nodd order\n' "$tr0" "$tr0" "$(git -C "$G/plain.git" rev-parse main)" | git --git-dir="$G/headers.git" hash-object -t commit -w --literally --stdin)
printf '%s\n' "$h1" > "$G/headers.git/refs/heads/main"
cp -R "$G/plain.git" "$G/badtag.git"; chmod -R u+w "$G/badtag.git"
bt=$(printf 'tagger Ada <ada@example.com> 1700009200 +0000\n\nno object line\n' | git --git-dir="$G/badtag.git" hash-object -t tag -w --literally --stdin)
printf '%s\n' "$bt" > "$G/badtag.git/refs/tags/badtag"
cp -R "$G/plain.git" "$G/packedremote.git"; chmod -R u+w "$G/packedremote.git"
rm -f "$G/packedremote.git/refs/heads/"*
printf '# pack-refs with: peeled fully-peeled sorted \n%s refs/remotes/up/signed\n' "$sid" > "$G/packedremote.git/packed-refs"
mkdir -p "$G/alternate2.git/objects/info" "$G/alternate2.git/refs/heads"
printf '../../alternate.git/objects\n' > "$G/alternate2.git/objects/info/alternates"
printf 'ref: refs/heads/main\n' > "$G/alternate2.git/HEAD"
git -C "$G/plain.git" rev-parse main > "$G/alternate2.git/refs/heads/main"

# ---- the documents: the Rust command's, each run from the directory of the repositories with the repository of `r` (no change yet) named by `--repo`; the git directory is given as typed here (`NAME.git`)
mkdir "$W/r" && ( cd "$W/r" && "$R" vcs init --output json > /dev/null )
g=$out/golden_git
cd "$G"
for d in *.git; do
  n=$(basename "$d" .git)
  set +e; "$R" vcs import-git "$d" --dry-run --repo "$W/r" --output json > "$g/$n.json" 2> /dev/null; echo $? > "$g/$n.code"; set -e
done
# a gitmap that knows two of plain's commits (the repository `k`), the arguments that are not one directory, a directory with no repository, `--no-checkout`
mkdir "$W/k" && ( cd "$W/k" && "$R" vcs init --output json > /dev/null )
ids=$(git -C "$G/plain.git" rev-list --all | head -2 | tr '\n' ' ')
: > "$W/k/.cancho/vcs/gitmap"
for i in $ids; do printf '%s %s\n' "$i" "0000000000000000000000000000000000000000000000000000000000000000" >> "$W/k/.cancho/vcs/gitmap"; done
third=$(git -C "$G/plain.git" rev-list --all | sed -n 3p | tr a-f A-F)
printf '%s %s\n' "$third" "0000000000000000000000000000000000000000000000000000000000000000" >> "$W/k/.cancho/vcs/gitmap"
cp "$W/k/.cancho/vcs/gitmap" "$g/plain_known.gitmap"
set +e
"$R" vcs import-git plain.git --dry-run --repo "$W/k" --output json > "$g/plain_known.json"; echo $? > "$g/plain_known.code"
"$R" vcs import-git --dry-run --repo "$W/r" --output json > "$g/noarg.json"; echo $? > "$g/noarg.code"
"$R" vcs import-git plain.git packed.git --dry-run --repo "$W/r" --output json > "$g/twoargs.json"; echo $? > "$g/twoargs.code"
"$R" vcs import-git plain.git --dry-run --no-checkout --repo "$W/r" --output json > "$g/nocheckout.json"; echo $? > "$g/nocheckout.code"
mkdir "$W/none"; ( cd "$W/none" && "$R" vcs import-git "$G/plain.git" --dry-run --output json > "$g/norepo.json"; echo $? > "$g/norepo.code" )
sed -i.bak "s#$G#<git>#g" "$g/norepo.json" && rm -f "$g/norepo.json.bak"
set -e
rmdir "$G/notgit.git"
cd "$out" && rm -rf "$W"
echo "made $(ls "$G" | wc -l) repositories"
