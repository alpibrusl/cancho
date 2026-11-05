#!/bin/sh
# R1 and R4 of docs/vcs-cli.md: import a git repository's whole history,
# pack it (`vcs gc`), export it, and compare every commit id; print the
# store's size (after the import, loose and compressed; after `gc`, packed)
# against git's, and the wall time of each step. Measurement tooling: it
# runs git to read the original's refs and to check the export is a valid
# repository; the import, gc, export and verify are `cancho vcs` and run no
# program.
# Usage: scripts/vcs_r1.sh <cancho binary> <git repo> <scratch dir>
set -eu
B=$1; SRC=$2; W=$3
case "$B" in /*) ;; *) B="$(pwd)/$B" ;; esac
rm -rf "$W"; mkdir -p "$W/work"
cd "$W/work"
now() { python3 -I -c 'import time; print(f"{time.time():.2f}")'; }
size() {
python3 -I - "$1" "$2" <<'PY'
import os,sys
n=b=0
for r,_,fs in os.walk(sys.argv[2]):
    for f in fs:
        n+=1; b+=os.path.getsize(os.path.join(r,f))
print(f'{sys.argv[1]}: {n} files, {b/1048576:.1f} MiB apparent')
PY
}
t0=$(now)
"$B" vcs import-git "$SRC" --output json > "$W/import.json"
t1=$(now)
size "cancho objects after import (loose, compressed)" "$W/work/.cancho/vcs/objects"
"$B" vcs gc --output json > "$W/gc.json"
t2=$(now)
size "cancho objects after gc (packed)" "$W/work/.cancho/vcs/objects"
"$B" vcs export-git "$W/exported.git" --output json > "$W/export.json"
t3=$(now)
"$B" vcs verify --output json > "$W/verify.json"
t4=$(now)
"$B" vcs gc --output json > "$W/gc2.json"
t5=$(now)
python3 -I - "$t0" "$t1" "$t2" "$t3" "$t4" "$t5" <<'PY'
import sys
t=[float(x) for x in sys.argv[1:]]
print(f'time: import {t[1]-t[0]:.1f}s, gc {t[2]-t[1]:.1f}s, export {t[3]-t[2]:.1f}s, verify {t[4]-t[3]:.1f}s, second gc {t[5]-t[4]:.1f}s')
PY
python3 -I - "$W" <<'PY'
import json,sys
w=sys.argv[1]
i=json.load(open(w+'/import.json')); e=json.load(open(w+'/export.json')); v=json.load(open(w+'/verify.json'))
g=json.load(open(w+'/gc.json')); g2=json.load(open(w+'/gc2.json'))
print('imported commits', i['commits'], 'self-check mismatches', len(i['self_check_mismatches']), 'refs kept', i['refs_kept'])
print('gc objects', g['objects'], 'deltas', g['deltas'], 'removed loose', g['removed_loose'], 'pack', g['pack'])
print('second gc: removed loose', g2['removed_loose'], 'removed packs', g2['removed_packs'], 'same pack', g2['pack'] == g['pack'])
print('exported commits', e['commits'], 'compared', e['compared_with_import'], 'equal', e['equal'], 'mismatches', len(e['mismatches']))
print('verify sound', v['sound'], v['objects'])
PY
echo "git rev-list --all (original):  $(git -C "$SRC" rev-list --all | sort | wc -l)"
echo "git rev-list --all (exported):  $(git -C "$W/exported.git" rev-list --all | sort | wc -l)"
if diff -q <(git -C "$SRC" rev-list --all | sort) <(git -C "$W/exported.git" rev-list --all | sort) >/dev/null 2>&1; then echo "commit id sets: IDENTICAL"; else echo "commit id sets: DIFFER"; fi
git -C "$W/exported.git" fsck --strict 2>&1 | tail -3 | sed 's/^/fsck: /'
echo "(refs/remotes/*/HEAD are symbolic refs and are not imported)"
echo "refs: original $(git -C "$SRC" for-each-ref refs/heads refs/tags refs/remotes | grep -v '/HEAD$' | wc -l) exported $(git -C "$W/exported.git" for-each-ref | wc -l)"
echo "ref ids equal: $(diff <(git -C "$SRC" for-each-ref refs/heads refs/tags refs/remotes --format='%(refname) %(objectname)' | grep -v '/HEAD ' | sort) <(git -C "$W/exported.git" for-each-ref --format='%(refname) %(objectname)' | sort) >/dev/null && echo yes || echo NO)"
size "git .git" "$SRC/.git"
size "git .git/objects" "$SRC/.git/objects"
# git's own split (KiB): loose objects git has not packed yet vary from run to run.
git -C "$SRC" count-objects -v | grep -E '^(count|size|in-pack|packs|size-pack):' | tr '\n' ' ' | sed 's/^/git count-objects: /'; echo
