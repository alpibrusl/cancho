#!/bin/sh
# R1 of docs/vcs-cli.md: import a git repository's whole history, export it,
# and compare every commit id. Measurement tooling: it runs git to read the
# original's refs and to check the export is a valid repository; the import
# and export themselves are `cancho vcs` and run no program.
# Usage: scripts/vcs_r1.sh <cancho binary> <git repo> <scratch dir>
set -eu
B=$1; SRC=$2; W=$3
rm -rf "$W"; mkdir -p "$W/work"
cd "$W/work"
start=$(date +%s)
"$B" vcs import-git "$SRC" --output json > "$W/import.json"
mid=$(date +%s)
"$B" vcs export-git "$W/exported.git" --output json > "$W/export.json"
end=$(date +%s)
"$B" vcs verify --output json > "$W/verify.json"
echo "import ${mid}-${start}=$((mid-start))s export $((end-mid))s"
python3 -I - "$W" <<'PY'
import json,sys
w=sys.argv[1]
i=json.load(open(w+'/import.json')); e=json.load(open(w+'/export.json')); v=json.load(open(w+'/verify.json'))
print('imported commits', i['commits'], 'self-check mismatches', len(i['self_check_mismatches']), 'refs kept', i['refs_kept'])
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
python3 -I - "$W/work/.cancho/vcs/objects" "$SRC/.git" <<'PY'
import os,sys
def tot(p):
    n=b=0
    for r,_,fs in os.walk(p):
        for f in fs:
            n+=1; b+=os.path.getsize(os.path.join(r,f))
    return n,b
for name,p in (('cancho objects',sys.argv[1]),('git .git',sys.argv[2])):
    n,b=tot(p); print(f'{name}: {n} files, {b/1048576:.1f} MiB apparent')
PY
