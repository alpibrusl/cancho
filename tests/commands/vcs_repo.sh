# vcs_repo.sh -- the script of tests/commands/script_tests.cho for slice R1 of docs/vcs-routing.md: `cancho vcs init`, `log` and `show` of the command line in cancho against what the Rust command printed:
# the pins of crates/cancho/tests/vcs_pins/ and the goldens of tests/vcs_fixtures/golden/, on copies of the repository of tests/vcs_fixtures/three_changes (made by tests/vcs_fixtures/make.sh).
# $1 is the compiler, $2 a scratch directory; run from the root of the repository. Standard output is compared byte for byte (the root, which differs by machine, made `<root>`).
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
root=$(pwd)
work=$2
fx=$root/tests/vcs_fixtures
pins=$root/crates/cancho/tests/vcs_pins
gold=$fx/golden

norm() { sed 's#"root": "[^"]*"#"root": "<root>"#'; }
same() { # NAME WANTED_FILE GOT_FILE
    if cmp -s "$2" "$3"; then echo "ok $1"; else echo "FAILS $1"; diff "$2" "$3" | head -8; fi
}
exit_is() { # NAME WANTED
    got=$(cat "$work/code.txt")
    if [ "$got" = "$2" ]; then echo "ok $1 (exit $2)"; else echo "FAILS $1: exit $got, wanted $2"; fi
}
repo() { rm -rf "$work/r"; mkdir -p "$work/r"; cp -R "$fx/three_changes/." "$work/r/"; cd "$work/r" || exit 1; }
empty() { rm -rf "$work/e"; mkdir -p "$work/e"; cd "$work/e" || exit 1; }
vcs() { "$cancho" vcs "$@" --output json > "$work/out.json" 2> "$work/err.txt"; echo $? > "$work/code.txt"; }

echo "== log"
repo; vcs log;                same "log is the pin" "$pins/log.json" "$work/out.json"; exit_is "log" 0
repo; vcs log HEAD~1;         same "log HEAD~1" "$gold/log_rev.json" "$work/out.json"
repo; vcs log --limit 1;      same "log --limit 1" "$gold/log_limit_1.json" "$work/out.json"
repo; vcs log --limit=1;      same "log --limit=1 is the same" "$gold/log_limit_1.json" "$work/out.json"
repo; vcs log --limit 0;      same "log --limit 0 lists one change, as the Rust command does" "$gold/log_limit_0.json" "$work/out.json"
repo; vcs log nosuchbranch;   same "log of a name that names nothing" "$gold/refuse_log_bad_revision.json" "$work/out.json"; exit_is "log refused" 1
repo; vcs log --limit x;      same "log --limit x is a bad argument" "$gold/refuse_log_bad_limit.json" "$work/out.json"; exit_is "log --limit x refused" 1

echo "== show"
repo; vcs show HEAD;          same "show HEAD is the pin" "$pins/show.json" "$work/out.json"; exit_is "show" 0
repo; vcs show;               same "show with no revision is HEAD" "$pins/show.json" "$work/out.json"
repo; vcs show HEAD~1;        same "show HEAD~1" "$gold/show_parent.json" "$work/out.json"
repo; vcs show nosuchbranch;  same "show of a name that names nothing" "$gold/refuse_show_bad_revision.json" "$work/out.json"; exit_is "show refused" 1
repo; vcs show HEAD~9;        same "show past the first change" "$gold/refuse_show_past_the_first.json" "$work/out.json"
empty; vcs show;              same "show with no repository is not-a-repository" "$gold/refuse_show_not_a_repository.json" "$work/out.json"; exit_is "show, no repository" 1

echo "== finding the repository"
repo; mkdir -p sub/deeper; cd sub/deeper || exit 1
vcs show;                     same "show from two directories down" "$pins/show.json" "$work/out.json"
vcs log;                      same "log from two directories down is the repository log" "$pins/log.json" "$work/out.json"
cd "$work" || exit 1; empty
vcs show --repo "$work/r";    same "show --repo PATH from another directory" "$pins/show.json" "$work/out.json"
vcs show --repo="$work/r/sub/deeper"; same "show --repo=PATH from a directory of the repository" "$pins/show.json" "$work/out.json"

echo "== init"
empty; vcs init;              norm < "$work/out.json" > "$work/norm.json"; same "init is the pin" "$pins/init.json" "$work/norm.json"; exit_is "init" 0
[ -f .cancho/vcs/format ] && [ "$(cat .cancho/vcs/HEAD)" = "ref: refs/heads/main" ] && echo "ok init made a store with HEAD on main" || echo "FAILS init made no store"
vcs init;                     norm < "$work/out.json" > "$work/norm.json"; same "init again is the pin" "$pins/init_again.json" "$work/norm.json"
vcs show;                     if grep -q '"tag": "bad-revision"' "$work/out.json" && grep -q 'HEAD has no change yet' "$work/out.json"; then echo "ok show in the new repository: HEAD has no change yet"; else echo "FAILS show in the new repository"; fi
empty; vcs init --dry-run;    same "init --dry-run" "$gold/init_dry.json" "$work/out.json"; [ -e .cancho ] && echo "FAILS --dry-run made a store" || echo "ok --dry-run made nothing"
repo; vcs init --dry-run;     same "init --dry-run in a repository" "$gold/init_dry_existing.json" "$work/out.json"
empty; vcs init --branch dev; norm < "$work/out.json" > "$work/norm.json"; same "init --branch dev" "$gold/init_branch_dev.json" "$work/norm.json"
[ "$(cat .cancho/vcs/HEAD)" = "ref: refs/heads/dev" ] && echo "ok HEAD is on dev" || echo "FAILS HEAD is not on dev"
empty; vcs init sub/new;      sed 's#"root": ".*/sub/new"#"root": "<root>/sub/new"#' < "$work/out.json" > "$work/norm.json"; same "init of a directory that is not there" "$gold/init_new_directory.json" "$work/norm.json"
[ -f sub/new/.cancho/vcs/format ] && echo "ok the directory was made, with its store" || echo "FAILS no store in sub/new"
empty; vcs init --branch 'bad name'; same "init with a bad branch name" "$gold/refuse_init_bad_branch.json" "$work/out.json"; exit_is "init, bad branch" 1

echo "== branch"
repo; vcs branch create feature --from HEAD~1;  same "branch create is the pin" "$pins/branch_create.json" "$work/out.json"; exit_is "branch create" 0
[ "$(cat .cancho/vcs/refs/heads/feature)" = "9e58c8a764b7d26837b806d291d8f7e50838343c3bd37076d1bc4bb5b1aaf442" ] && echo "ok the ref file holds the id and a newline" || echo "FAILS the ref file"
[ -e .cancho/vcs/refs/heads/feature.lock ] && echo "FAILS the lock was left" || echo "ok the lock is gone"
sed 's/ [0-9]* / T /' .cancho/vcs/refs/heads/feature.log > "$work/log.txt"; printf -- '- 9e58c8a764b7d26837b806d291d8f7e50838343c3bd37076d1bc4bb5b1aaf442 T branch create\n' > "$work/want.txt"; same "the ref log has the line the Rust command writes (time made T)" "$work/want.txt" "$work/log.txt"
vcs branch list;                  same "branch list is the pin" "$pins/branch_list.json" "$work/out.json"
vcs branch;                       same "branch with no word is branch list" "$pins/branch_list.json" "$work/out.json"
vcs branch delete feature --expect HEAD~1; same "branch delete is the pin" "$pins/branch_delete.json" "$work/out.json"; exit_is "branch delete" 0
[ -e .cancho/vcs/refs/heads/feature ] && echo "FAILS the ref is still there" || echo "ok the ref file is gone"
[ -e .cancho/vcs/refs/heads/feature.lock ] && echo "FAILS the lock was left" || echo "ok the lock is gone"
sed 's/ [0-9]* / T /' .cancho/vcs/refs/heads/feature.log > "$work/log.txt"; printf -- '- 9e58c8a764b7d26837b806d291d8f7e50838343c3bd37076d1bc4bb5b1aaf442 T branch create\n9e58c8a764b7d26837b806d291d8f7e50838343c3bd37076d1bc4bb5b1aaf442 - T deleted\n' > "$work/want.txt"; same "the ref log has the deletion too" "$work/want.txt" "$work/log.txt"
vcs branch list;                  same "branch list after the delete" "$gold/branch_list_after_delete.json" "$work/out.json"
repo; vcs branch list;            same "branch list of the repository" "$gold/branch_list_initial.json" "$work/out.json"
empty; "$cancho" vcs init --output json > /dev/null; vcs branch list; same "branch list of an empty repository" "$gold/branch_list_empty.json" "$work/out.json"
vcs branch create x;              same "branch create when HEAD has no change yet" "$gold/refuse_branch_create_no_change.json" "$work/out.json"; exit_is "branch create, no change" 1
repo; vcs branch create topic;    same "branch create from HEAD" "$gold/branch_create_head.json" "$work/out.json"
repo; vcs branch create feature --from HEAD~1 > /dev/null; vcs branch create feature --from HEAD~1; same "branch create again is created: false" "$gold/branch_create_again.json" "$work/out.json"
repo; vcs branch create feature --from HEAD~1 > /dev/null; vcs branch create feature --from HEAD; same "branch create where it is elsewhere is ref-moved" "$gold/refuse_branch_create_moved.json" "$work/out.json"; exit_is "ref-moved" 1
repo; vcs branch create feature --dry-run; same "branch create --dry-run" "$gold/branch_create_dry.json" "$work/out.json"; [ -e .cancho/vcs/refs/heads/feature ] && echo "FAILS --dry-run made the ref" || echo "ok --dry-run made nothing"
repo; vcs branch create 'bad name'; same "branch create with a bad name" "$gold/refuse_branch_name.json" "$work/out.json"
repo; vcs branch create;          same "branch create with no name" "$gold/refuse_branch_no_name.json" "$work/out.json"
repo; vcs branch create x --from nosuch; same "branch create --from a name that names nothing" "$gold/refuse_branch_create_bad_from.json" "$work/out.json"
repo; vcs branch frobnicate;      same "branch with a word it does not know" "$gold/refuse_branch_unknown.json" "$work/out.json"
repo; vcs branch create feature --from HEAD~1 > /dev/null; vcs branch delete feature --expect HEAD~1 --dry-run; same "branch delete --dry-run" "$gold/branch_delete_dry.json" "$work/out.json"; [ -e .cancho/vcs/refs/heads/feature ] && echo "ok --dry-run kept the ref" || echo "FAILS --dry-run deleted the ref"
repo; vcs branch create feature --from HEAD~1 > /dev/null; vcs branch delete feature; same "branch delete needs --expect" "$gold/refuse_branch_delete_no_expect.json" "$work/out.json"
repo; vcs branch create feature --from HEAD~1 > /dev/null; vcs branch delete feature --expect HEAD; same "branch delete --expect the wrong change is ref-moved" "$gold/refuse_branch_delete_moved.json" "$work/out.json"; [ -e .cancho/vcs/refs/heads/feature ] && echo "ok the ref was kept" || echo "FAILS the ref was deleted"
repo; vcs branch delete main --expect HEAD; same "the checked-out branch cannot be deleted" "$gold/refuse_branch_delete_current.json" "$work/out.json"
repo; vcs branch delete nosuch;   same "deleting a branch that is not there" "$gold/branch_delete_missing.json" "$work/out.json"
repo; vcs branch create 'a.b' > /dev/null; vcs branch create 'a/b' --from HEAD~1 > /dev/null; vcs branch create a0 --from HEAD~2 > /dev/null; vcs branch create b > /dev/null; vcs branch list; same "branch list: a.b, a/b, a0 in the order of the bytes of their paths" "$gold/branch_list_sorted.json" "$work/out.json"

echo "== status"
empty; mkdir x && cd x; "$cancho" vcs init --output json > /dev/null; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n; }\n' > a.cho; printf '# notes\none\ntwo\nthree\n' > notes.md
vcs status;                   same "status of a new repository is the pin (head null)" "$pins/status_dirty.json" "$work/out.json"; exit_is "status" 0
repo; echo 960fd3a813e8e05e68c304a1ea92bf374b61dd07399c70166822fd6e6c7b716f > .cancho/vcs/refs/heads/main; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n; }\n' > a.cho; printf '# notes\none\ntwo\nthree\n' > notes.md
vcs status;                   same "status clean at the first change is the pin" "$pins/status_clean.json" "$work/out.json"
repo; echo 9e58c8a764b7d26837b806d291d8f7e50838343c3bd37076d1bc4bb5b1aaf442 > .cancho/vcs/refs/heads/main
vcs status;                   same "status with one file left is the pin" "$pins/status_one_left.json" "$work/out.json"
repo; vcs status;             same "status of the clean repository" "$gold/status_clean_fixture.json" "$work/out.json"
repo; rm a.cho; vcs status;   same "status: a deleted file" "$gold/status_deleted.json" "$work/out.json"
repo; mkdir -p sub/deep; printf 'x\n' > sub/deep/new.txt; printf 'y\n' > top.txt; vcs status; same "status: added files, one in a new directory" "$gold/status_added.json" "$work/out.json"
repo; chmod +x notes.md; vcs status; same "status: the execute bit" "$gold/status_mode_changed.json" "$work/out.json"
repo; ln -s a.cho lnk; vcs status; same "status: a link added" "$gold/status_link_added.json" "$work/out.json"
repo; ln -s a.cho lnk; ln -s notes.md lnk2; ln -s nowhere dangling; mkdir sub; ln -s ../a.cho sub/up; ln -s sub link_to_dir; vcs status; same "status: links to a file, a name that is not there, a directory" "$gold/status_links.json" "$work/out.json"
repo; rm notes.md; ln -s a.cho notes.md; vcs status; same "status: a file that became a link" "$gold/status_file_became_link.json" "$work/out.json"
repo; ln -s "$(printf 'x%.0s' $(seq 1 500))" long; vcs status; same "status: a link target of 500 bytes" "$gold/status_long_link.json" "$work/out.json"
repo; printf '*.log\nsub/up\n' > .canchoignore; ln -s x1 a.log; mkdir sub; ln -s ../a.cho sub/up; vcs status; same "status: ignored links are not listed" "$gold/status_ignored.json" "$work/out.json"
repo; rm a.cho; mkdir a.cho; printf z > a.cho/inner; vcs status; same "status: a file that became a directory" "$gold/status_file_became_dir.json" "$work/out.json"
repo; ln -s "$(printf '\377\376')" bad; vcs status; if grep -q '"tag": "path-unsupported"' "$work/out.json"; then echo "ok status: a link target that is not UTF-8 is path-unsupported"; else echo "FAILS bad link target"; fi; exit_is "status, bad link target" 1
empty; "$cancho" vcs init --output json > /dev/null; vcs status; same "status of a repository with nothing in it" "$gold/status_empty_repository.json" "$work/out.json"
empty; vcs status;            same "status with no repository is not-a-repository" "$gold/refuse_status_not_a_repository.json" "$work/out.json"; exit_is "status, no repository" 1
repo; mkdir -p sub/deeper; cd sub/deeper || exit 1; vcs status; same "status from two directories down is the clean status" "$gold/status_clean_fixture.json" "$work/out.json"
cd "$work" || exit 1; empty; vcs status --repo "$work/r"; same "status --repo PATH from another directory" "$gold/status_clean_fixture.json" "$work/out.json"

echo "== restore"
tree_is_head() { if diff -r --exclude=.cancho "$fx/three_changes" "$work/r" > /dev/null; then echo "ok $1: the working copy is HEAD's"; else echo "FAILS $1: the working copy differs from HEAD's"; diff -r --exclude=.cancho "$fx/three_changes" "$work/r" | head -5; fi; }
no_lock() { [ -e "$work/r/.cancho/vcs/lock" ] && echo "FAILS $1: the lock was left" || echo "ok $1: the lock is gone"; }
repo; printf 'scribble\n' > notes.md; vcs restore notes.md; same "restore one file" "$gold/restore_one.json" "$work/out.json"; exit_is "restore" 0; tree_is_head "restore one file"; no_lock "restore"
repo; printf 'scribble\n' > notes.md; vcs restore ./notes.md; same "restore ./notes.md is notes.md (a dot component was once left in the path, and nothing was restored)" "$gold/restore_one.json" "$work/out.json"; tree_is_head "restore ./notes.md"
repo; printf 'scribble\n' > notes.md; vcs restore s/../notes.md; same "restore s/../notes.md is notes.md" "$gold/restore_one.json" "$work/out.json"; tree_is_head "restore s/../notes.md"
repo; printf 'scribble\n' > notes.md; vcs restore notes.md --dry-run; same "restore --dry-run" "$gold/restore_dry.json" "$work/out.json"; [ "$(cat notes.md)" = "scribble" ] && echo "ok --dry-run wrote nothing" || echo "FAILS --dry-run wrote"
repo; vcs restore notes.md;   same "restore of a file that is already HEAD's" "$gold/restore_clean.json" "$work/out.json"
repo; rm a.cho; chmod +x notes.md; ln -s a.cho lnk; mkdir -p sub/deep; printf x > sub/new; printf y > sub/deep/new2; printf z > top.txt; vcs restore .; same "restore . after every kind of change" "$gold/restore_all.json" "$work/out.json"; tree_is_head "restore ."; no_lock "restore ."
vcs restore .;                same "restore . again restores nothing" "$gold/restore_clean.json" "$work/out.json"
repo; rm a.cho; mkdir a.cho; printf z > a.cho/inner; vcs restore .; if grep -q '"tag": "io-error"' "$work/out.json"; then echo "ok restore . where a file became a directory: io-error, as the Rust command stops"; else echo "FAILS file became a directory"; cat "$work/out.json"; fi; exit_is "restore, file became a directory" 3; no_lock "restore, io-error"
repo; mkdir -p sub/deep; printf x > sub/new; printf y > sub/deep/new2; printf z > top.txt; vcs restore sub; same "restore a directory" "$gold/restore_beneath.json" "$work/out.json"; [ -e top.txt ] && echo "ok a path outside the one named is left" || echo "FAILS top.txt was removed"
repo; vcs restore . --from HEAD~1; same "restore . --from HEAD~1" "$gold/restore_from_parent.json" "$work/out.json"; cmp -s notes.md "$gold/restore_from_parent.notes.md" && cmp -s a.cho "$gold/restore_from_parent.a.cho" && echo "ok the files are the parent's" || echo "FAILS the files are not the parent's"
repo; vcs restore --from HEAD~1; same "restore with no path" "$gold/refuse_restore_no_path.json" "$work/out.json"; exit_is "restore, no path" 1
repo; printf 'scribble\n' > notes.md; vcs restore nosuchfile; same "restore of a name nothing has" "$gold/restore_no_match.json" "$work/out.json"; [ "$(cat notes.md)" = "scribble" ] && echo "ok nothing else was restored" || echo "FAILS another file was restored"
repo; vcs restore ../elsewhere; same "restore of a path outside the repository" "$gold/refuse_restore_escapes.json" "$work/out.json"; exit_is "restore, path escapes" 1
repo; vcs restore notes.md --from nosuch; same "restore --from a name that names nothing" "$gold/refuse_restore_bad_from.json" "$work/out.json"
repo; mkdir -p sub; printf x > sub/new; printf 'scribble\n' > notes.md; cd sub || exit 1; vcs restore ../notes.md; same "restore ../notes.md from a subdirectory" "$gold/restore_from_subdir.json" "$work/out.json"
cd "$work/r" || exit 1; printf 'scribble\n' > notes.md; cd sub || exit 1; vcs restore .; same "restore . in a subdirectory covers only it" "$gold/restore_in_subdir.json" "$work/out.json"; [ "$(cat ../notes.md)" = "scribble" ] && echo "ok the rest of the working copy is left" || echo "FAILS the rest was restored"
cd "$work" || exit 1; repo; rm a.cho; vcs restore a.cho notes.md; same "restore with two paths" "$gold/restore_two_paths.json" "$work/out.json"
repo; printf 'scribble\n' > notes.md; printf '4242 1000' > .cancho/vcs/lock; vcs restore notes.md --dry-run; same "restore --dry-run takes no lock" "$gold/restore_dry_locked.json" "$work/out.json"
vcs restore notes.md; if grep -q '"tag": "lock-held"' "$work/out.json" && grep -q '"pid": 4242' "$work/out.json" && grep -q '"age_seconds": [0-9]' "$work/out.json" && grep -q "\"lock\": \"$(cd "$work/r" && pwd -P)/.cancho/vcs/lock\"" "$work/out.json" && grep -q 'is locked by pid 4242 for [0-9]*s; if that process is gone, run `cancho vcs unlock`' "$work/out.json"; then echo "ok restore with the lock held: lock-held with its pid, age and path"; else echo "FAILS lock-held"; cat "$work/out.json"; fi; exit_is "restore, lock held" 1
[ "$(cat notes.md)" = "scribble" ] && [ "$(cat .cancho/vcs/lock)" = "4242 1000" ] && echo "ok a held lock changes nothing and is left" || echo "FAILS the held lock was disturbed"
printf '0 5' > .cancho/vcs/lock; vcs restore notes.md; if grep -q '"pid": null' "$work/out.json" && grep -q '"tag": "lock-held"' "$work/out.json"; then echo "ok a lock stamped with pid 0 has a null pid"; else echo "FAILS pid 0"; fi

echo "== switch"
head_is() { if cmp -s "$work/r/.cancho/vcs/HEAD" "$2"; then echo "ok $1: HEAD is as the Rust command left it"; else echo "FAILS $1: HEAD differs"; fi; }
listing() { find . -path ./.cancho -prune -o -print | sort | while read -r f; do if [ -L "$f" ]; then echo "l $f -> $(readlink "$f")"; elif [ -d "$f" ]; then echo "d $f"; elif [ -x "$f" ]; then echo "x $f $(cksum < "$f")"; else echo "f $f $(cksum < "$f")"; fi; done; }
wide() { rm -rf "$work/r"; mkdir -p "$work/r"; cp -R "$fx/four_changes/." "$work/r/"; cd "$work/r" || exit 1; }
parent_files() { cmp -s notes.md "$gold/restore_from_parent.notes.md" && cmp -s a.cho "$gold/restore_from_parent.a.cho" && echo "ok $1: the files are the parent's" || echo "FAILS $1: the files are not the parent's"; }
branch_old() { "$cancho" vcs branch create old --from HEAD~1 --output json > /dev/null; }
repo; vcs switch main;        same "switch to the branch HEAD is on" "$gold/switch_noop.json" "$work/out.json"; exit_is "switch, nowhere" 0; no_lock "switch, nowhere"
repo; vcs switch topic --create; same "switch --create" "$gold/switch_create.json" "$work/out.json"; head_is "switch --create" "$gold/switch_create.HEAD"; [ "$(cat .cancho/vcs/refs/heads/topic)" = "$(cat .cancho/vcs/refs/heads/main)" ] && echo "ok the new branch is at HEAD's change" || echo "FAILS the new branch is elsewhere"; no_lock "switch --create"
repo; vcs switch topic --create --dry-run; same "switch --create --dry-run" "$gold/switch_create_dry.json" "$work/out.json"; [ -e .cancho/vcs/refs/heads/topic ] && echo "FAILS --dry-run made the branch" || echo "ok --dry-run made no branch"
repo; vcs switch main --create; same "switch --create over a branch that exists" "$gold/refuse_switch_create_exists.json" "$work/out.json"; exit_is "switch --create, exists" 1; no_lock "switch --create, exists"
repo; vcs switch 'bad name' --create; same "switch --create with a name that is not a ref name" "$gold/refuse_switch_create_bad_name.json" "$work/out.json"; exit_is "switch --create, bad name" 1
empty; "$cancho" vcs init --output json > /dev/null; vcs switch x --create; same "switch --create where HEAD has no change" "$gold/refuse_switch_create_no_change.json" "$work/out.json"
empty; "$cancho" vcs init --output json > /dev/null; vcs switch main; same "switch in a repository with no change" "$gold/refuse_switch_empty_repository.json" "$work/out.json"
repo; branch_old; vcs switch old; same "switch to a branch" "$gold/switch_branch.json" "$work/out.json"; exit_is "switch to a branch" 0; head_is "switch to a branch" "$gold/switch_branch.HEAD"; parent_files "switch to a branch"; no_lock "switch to a branch"
repo; branch_old; vcs switch old --dry-run; same "switch --dry-run" "$gold/switch_branch_dry.json" "$work/out.json"; [ "$(cat .cancho/vcs/HEAD)" = "ref: refs/heads/main" ] && [ "$(cat notes.md)" = "$(cd "$fx/three_changes" && cat notes.md)" ] && echo "ok --dry-run moved nothing" || echo "FAILS --dry-run moved something"
repo; vcs switch HEAD~1;      same "switch to a change detaches" "$gold/switch_detach_parent.json" "$work/out.json"; head_is "switch to a change" "$gold/switch_detach_parent.HEAD"; parent_files "switch to a change"
repo; vcs switch main --detach; same "switch --detach reads a branch name as a change" "$gold/switch_detach_branch.json" "$work/out.json"; head_is "switch --detach" "$gold/switch_detach_branch.HEAD"
repo; vcs switch nosuch;      same "switch to a name that names nothing" "$gold/refuse_switch_bad_revision.json" "$work/out.json"; exit_is "switch, bad revision" 1
repo; vcs switch;             same "switch with no argument" "$gold/refuse_switch_no_argument.json" "$work/out.json"; exit_is "switch, no argument" 1
repo; vcs switch a b;         same "switch with two arguments" "$gold/refuse_switch_two_arguments.json" "$work/out.json"; exit_is "switch, two arguments" 1
repo; branch_old; printf 'scribble\n' > notes.md; vcs switch old; same "switch over a dirty file the move touches" "$gold/refuse_switch_dirty.json" "$work/out.json"; exit_is "switch, dirty" 1; [ "$(cat notes.md)" = "scribble" ] && [ "$(cat .cancho/vcs/HEAD)" = "ref: refs/heads/main" ] && echo "ok a refused switch changed nothing" || echo "FAILS a refused switch changed something"; no_lock "switch, dirty"
repo; branch_old; printf 'z\n' > zzz.txt; vcs switch old; same "switch over a dirty file the move does not touch (no flag)" "$gold/refuse_switch_dirty_other.json" "$work/out.json"
repo; branch_old; printf 'z\n' > zzz.txt; vcs switch old --keep; same "switch --keep carries a dirty file the move does not touch" "$gold/switch_keep_carries.json" "$work/out.json"; [ "$(cat zzz.txt)" = "z" ] && parent_files "switch --keep" || echo "FAILS --keep lost the file"
repo; branch_old; printf 'scribble\n' > notes.md; vcs switch old --keep; same "switch --keep still refuses a dirty file in the way" "$gold/refuse_switch_keep_in_the_way.json" "$work/out.json"
repo; branch_old; printf 'scribble\n' > notes.md; printf 'z\n' > zzz.txt; vcs switch old --discard; same "switch --discard lists what it overwrote" "$gold/switch_discard.json" "$work/out.json"; parent_files "switch --discard"; [ "$(cat zzz.txt)" = "z" ] && echo "ok --discard leaves a file the move does not touch" || echo "FAILS --discard removed zzz.txt"
repo; branch_old; printf 'scribble\n' > notes.md; vcs switch old --dry-run; same "switch --dry-run still refuses a dirty working copy" "$gold/refuse_switch_dry_dirty.json" "$work/out.json"
repo; branch_old; vcs switch old; vcs switch main; same "switch back" "$gold/switch_back.json" "$work/out.json"; tree_is_head "switch back"; no_lock "switch back"
wide; vcs switch wide;        same "switch to a branch that adds, removes, links and changes a mode" "$gold/switch_wide.json" "$work/out.json"; head_is "switch wide" "$gold/switch_wide.HEAD"; [ "$(listing)" = "$(cat "$gold/switch_wide.tree")" ] && echo "ok switch wide: the working copy is what the Rust command made" || { echo "FAILS switch wide: the working copy differs"; listing | diff "$gold/switch_wide.tree" - | head -8; }
vcs switch main;              same "switch back from it" "$gold/switch_wide_back.json" "$work/out.json"; [ "$(listing)" = "$(cat "$gold/switch_wide_back.tree")" ] && echo "ok switch back: files removed, directories with them" || { echo "FAILS switch back: the working copy differs"; listing | diff "$gold/switch_wide_back.tree" - | head -8; }; tree_is_head "switch back from wide"
wide; printf 'q\n' > unrelated.txt; vcs switch wide --keep; same "switch --keep carries a file the move does not name" "$gold/switch_wide_keep.json" "$work/out.json"; [ "$(listing)" = "$(cat "$gold/switch_wide_keep.tree")" ] && echo "ok switch --keep: the same tree" || echo "FAILS switch --keep: the tree differs"
wide; vcs switch wide --dry-run; same "switch --dry-run to a wider branch" "$gold/switch_wide_dry.json" "$work/out.json"; tree_is_head "switch --dry-run, wide"
wide; mkdir -p sub; vcs switch wide; cd sub || exit 1; vcs switch main; same "switch from a subdirectory" "$gold/switch_from_subdir.json" "$work/out.json"
cd "$work/r" 2> /dev/null || cd "$work" || exit 1
repo; branch_old; printf '4242 1000' > .cancho/vcs/lock; vcs switch old --dry-run; same "switch --dry-run takes no lock" "$gold/switch_dry_locked.json" "$work/out.json"
vcs switch old; if grep -q '"tag": "lock-held"' "$work/out.json" && grep -q '"command": "switch"' "$work/out.json" && grep -q '"pid": 4242' "$work/out.json" && grep -q "\"lock\": \"$(cd "$work/r" && pwd -P)/.cancho/vcs/lock\"" "$work/out.json"; then echo "ok switch with the lock held: lock-held with its pid and path"; else echo "FAILS switch lock-held"; cat "$work/out.json"; fi; exit_is "switch, lock held" 1
[ "$(cat .cancho/vcs/HEAD)" = "ref: refs/heads/main" ] && [ "$(cat .cancho/vcs/lock)" = "4242 1000" ] && echo "ok a held lock changes nothing and is left" || echo "FAILS the held lock was disturbed"

echo "== apply"
apx() { printf '%s' "$1" > "$work/edits.json"; shift; "$cancho" vcs apply "$work/edits.json" "$@" --output json > "$work/out.json" 2> "$work/err.txt"; echo $? > "$work/code.txt"; }
tree_was() { if [ "$(listing)" = "$(cat "$gold/$2")" ]; then echo "ok $1: the working copy is as the Rust command left it"; else echo "FAILS $1: the working copy differs"; fi; }
no_lock_after() { [ -e .cancho/vcs/lock ] && echo "FAILS $1: the lock was left" || echo "ok $1: no lock left"; }
repo; apx '[{"path":"notes.md","old":"2","new":"two"}]' ; same "apply: basic" "$gold/apply_basic.json" "$work/out.json"; tree_was "apply: basic" apply_basic.tree; no_lock_after "apply: basic"
repo; apx '[{"path":"notes.md","old":"one","new":"uno"},{"path":"notes.md","old":"uno\n","new":"UNO\n"}]' ; same "apply: seq" "$gold/apply_seq.json" "$work/out.json"; tree_was "apply: seq" apply_seq.tree; no_lock_after "apply: seq"
repo; apx '[{"path":"notes.md","old":"one","new":"1"},{"path":"a.cho","old":"twice","new":"TWICE"}]' ; same "apply: two files" "$gold/apply_two_files.json" "$work/out.json"; tree_was "apply: two files" apply_two_files.tree; no_lock_after "apply: two files"
repo; apx '[{"path":"a.cho","old":"twice","new":"TWICE"},{"path":"notes.md","old":"one","new":"1"}]' ; same "apply: two files reversed" "$gold/apply_two_files_reversed.json" "$work/out.json"; tree_was "apply: two files reversed" apply_two_files_reversed.tree; no_lock_after "apply: two files reversed"
repo; printf 'aaa\n' > notes.md; apx '[{"path":"notes.md","old":"aa","new":"b"}]' ; same "apply: overlap" "$gold/apply_overlap.json" "$work/out.json"; tree_was "apply: overlap" apply_overlap.tree; no_lock_after "apply: overlap"
repo; apx '[{"path":"notes.md","old":"2","new":"two"}]' --dry-run; same "apply: dry" "$gold/apply_dry.json" "$work/out.json"; tree_was "apply: dry" apply_dry.tree; no_lock_after "apply: dry"
repo; apx '[{"path":"a.cho","old":"twice","new":"TWICE"},{"path":"notes.md","old":"zzz","new":"b"}]' ; same "apply: atomic" "$gold/apply_atomic.json" "$work/out.json"; tree_was "apply: atomic" apply_atomic.tree; no_lock_after "apply: atomic"
repo; apx '[{"path":"notes.md","old":"zzz","new":"b"},{"path":"a.cho","old":"twice","new":"TWICE"}]' ; same "apply: atomic reversed" "$gold/apply_atomic_reversed.json" "$work/out.json"; tree_was "apply: atomic reversed" apply_atomic_reversed.tree; no_lock_after "apply: atomic reversed"
repo; apx '[{"path":"notes.md","old":"zzz","new":"b"}]' ; same "apply: not found" "$gold/apply_not_found.json" "$work/out.json"; tree_was "apply: not found" apply_not_found.tree; no_lock_after "apply: not found"
repo; printf 'x x\n' > notes.md; apx '[{"path":"notes.md","old":"x","new":"b"}]' ; same "apply: ambiguous" "$gold/apply_ambiguous.json" "$work/out.json"; tree_was "apply: ambiguous" apply_ambiguous.tree; no_lock_after "apply: ambiguous"
repo; apx '[{"path":"a.cho","old":"fn","new":"FN"}]' ; same "apply: ambiguous code" "$gold/apply_ambiguous_code.json" "$work/out.json"; tree_was "apply: ambiguous code" apply_ambiguous_code.tree; no_lock_after "apply: ambiguous code"
repo; apx '[{"path":"notes.md","old":"","new":"b"}]' ; same "apply: empty anchor" "$gold/apply_empty_anchor.json" "$work/out.json"; tree_was "apply: empty anchor" apply_empty_anchor.tree; no_lock_after "apply: empty anchor"
repo; apx '[{"path":"../x","old":"a","new":"b"}]' ; same "apply: escapes" "$gold/apply_escapes.json" "$work/out.json"; tree_was "apply: escapes" apply_escapes.tree; no_lock_after "apply: escapes"
repo; apx '[]' ; same "apply: empty list" "$gold/apply_empty_list.json" "$work/out.json"; tree_was "apply: empty list" apply_empty_list.tree; no_lock_after "apply: empty list"
repo; chmod +x notes.md; apx '[{"path":"notes.md","old":"2","new":"two"}]' ; same "apply: exec" "$gold/apply_exec.json" "$work/out.json"; tree_was "apply: exec" apply_exec.tree; no_lock_after "apply: exec"
repo; printf 'caf\303\251 ok\n' > notes.md; apx '[{"path":"notes.md","old":"caf\u00e9","new":"tea \ud83d\ude00"}]' ; same "apply: unicode" "$gold/apply_unicode.json" "$work/out.json"; tree_was "apply: unicode" apply_unicode.tree; no_lock_after "apply: unicode"
repo; apx '[{"path":"./sub/../notes.md","old":"2","new":"two"}]' ; same "apply: dotted" "$gold/apply_dotted.json" "$work/out.json"; tree_was "apply: dotted" apply_dotted.tree; no_lock_after "apply: dotted"
repo; apx '[{"path":"./sub/../notes.md","old":"zz","new":"two"}]' ; same "apply: dotted refused" "$gold/apply_dotted_refused.json" "$work/out.json"; tree_was "apply: dotted refused" apply_dotted_refused.tree; no_lock_after "apply: dotted refused"
repo; apx '[{"path":"notes.md","old":"one","new":"1"},{"path":"./notes.md","old":"2","new":"two"}]' ; same "apply: spellings" "$gold/apply_spellings.json" "$work/out.json"; tree_was "apply: spellings" apply_spellings.tree; no_lock_after "apply: spellings"
repo; apx '[{"path":"notes.md","old":"one","new":"1"},{"path":"./notes.md","old":"qq","new":"two"}]' ; same "apply: spellings refused" "$gold/apply_spellings_refused.json" "$work/out.json"; tree_was "apply: spellings refused" apply_spellings_refused.tree; no_lock_after "apply: spellings refused"
repo; mkdir -p s/t; printf 'x y\n' > s/t/f.txt; apx '[{"path":"s/t/f.txt","old":"y","new":"z"}]' ; same "apply: subdirectory" "$gold/apply_subdirectory.json" "$work/out.json"; tree_was "apply: subdirectory" apply_subdirectory.tree; no_lock_after "apply: subdirectory"
repo; printf 'a\r\nb\r\n' > notes.md; apx '[{"path":"notes.md","old":"a\r\nb","new":"c"}]' ; same "apply: crlf" "$gold/apply_crlf.json" "$work/out.json"; tree_was "apply: crlf" apply_crlf.tree; no_lock_after "apply: crlf"
repo; apx '[{"path":"notes.md","old":"one\n","new":""}]' ; same "apply: deleting" "$gold/apply_deleting.json" "$work/out.json"; tree_was "apply: deleting" apply_deleting.tree; no_lock_after "apply: deleting"
repo; seq 1 5000 > notes.md; apx '[{"path":"notes.md","old":"\n2500\n","new":"\nX\n"}]' ; same "apply: large" "$gold/apply_large.json" "$work/out.json"; tree_was "apply: large" apply_large.tree; no_lock_after "apply: large"
repo; apx '  [ { "new" : "ONE" , "old" : "one" , "path" : "notes.md" } ]  ' ; same "apply: spaced json" "$gold/apply_spaced_json.json" "$work/out.json"; tree_was "apply: spaced json" apply_spaced_json.tree; no_lock_after "apply: spaced json"
repo; apx '[{"path":"notes.md","old":"2","new":"two","note":1}]' ; same "apply: extra key" "$gold/apply_extra_key.json" "$work/out.json"; tree_was "apply: extra key" apply_extra_key.tree; no_lock_after "apply: extra key"
repo; "$cancho" vcs apply --output json > "$work/out.json" 2> /dev/null; echo $? > "$work/code.txt"; same "apply with no argument" "$gold/refuse_apply_no_argument.json" "$work/out.json"; exit_is "apply, no argument" 1
repo; "$cancho" vcs apply a b --output json > "$work/out.json" 2> /dev/null; echo $? > "$work/code.txt"; same "apply with two arguments" "$gold/refuse_apply_two_arguments.json" "$work/out.json"; exit_is "apply, two arguments" 1
repo; mkdir -p s; apx '[{"path":"nope.txt","old":"a","new":"b"}]'; if grep -q '"tag": "io-error"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 3 ]; then echo "ok apply, a file that is not there: io-error, exit 3"; else echo "FAILS apply, a file that is not there"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '[{"path":"s","old":"a","new":"b"}]'; if grep -q '"tag": "io-error"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 3 ]; then echo "ok apply, a directory: io-error, exit 3"; else echo "FAILS apply, a directory"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '[{"path":".","old":"a","new":"b"}]'; if grep -q '"tag": "io-error"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 3 ]; then echo "ok apply, the root: io-error, exit 3"; else echo "FAILS apply, the root"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '[{"path":"","old":"a","new":"b"}]'; if grep -q '"tag": "io-error"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 3 ]; then echo "ok apply, an empty path: io-error, exit 3"; else echo "FAILS apply, an empty path"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '{"a":1}'; if grep -q '"tag": "bad-argument"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 1 ]; then echo "ok apply, an object, not an array: bad-argument, exit 1"; else echo "FAILS apply, an object, not an array"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '[{"path":"notes.md","old":"a"}]'; if grep -q '"tag": "bad-argument"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 1 ]; then echo "ok apply, an edit with no new: bad-argument, exit 1"; else echo "FAILS apply, an edit with no new"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '[{"path":"notes.md","old":1,"new":"a"}]'; if grep -q '"tag": "bad-argument"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 1 ]; then echo "ok apply, a number as old: bad-argument, exit 1"; else echo "FAILS apply, a number as old"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '[{"path":"notes.md","old":'; if grep -q '"tag": "bad-argument"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 1 ]; then echo "ok apply, JSON that stops: bad-argument, exit 1"; else echo "FAILS apply, JSON that stops"; cat "$work/out.json"; fi
repo; mkdir -p s; apx '[{"path":"notes.md","old":"1","new":"2"},]'; if grep -q '"tag": "bad-argument"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 1 ]; then echo "ok apply, a trailing comma: bad-argument, exit 1"; else echo "FAILS apply, a trailing comma"; cat "$work/out.json"; fi
repo; "$cancho" vcs apply nosuch.json --output json > "$work/out.json" 2> /dev/null; echo $? > "$work/code.txt"; if grep -q '"tag": "io-error"' "$work/out.json" && [ "$(cat "$work/code.txt")" = 3 ]; then echo "ok apply, an edits file that is not there: io-error, exit 3"; else echo "FAILS apply, no such file"; fi
repo; printf '4242 1000' > .cancho/vcs/lock; apx '[{"path":"notes.md","old":"2","new":"two"}]' --dry-run; if grep -q '"tag": "lock-held"' "$work/out.json" && grep -q '"command": "apply"' "$work/out.json" && grep -q '"pid": 4242' "$work/out.json"; then echo "ok apply --dry-run takes the lock, unlike restore and switch: lock-held"; else echo "FAILS apply --dry-run, held lock"; fi; [ "$(cat .cancho/vcs/lock)" = "4242 1000" ] && echo "ok a held lock is left" || echo "FAILS the held lock was disturbed"
repo; mkdir -p s; cd s || exit 1; apx '[{"path":"../notes.md","old":"2","new":"two"}]'; [ "$(cat ../notes.md | sed -n 3p)" = two ] && grep -q '"applied"' "$work/out.json" && grep -q '"notes.md"' "$work/out.json" && echo "ok apply from a subdirectory: paths are the current directory's" || echo "FAILS apply from a subdirectory"; cd "$work/r" || exit 1

echo "== unlock"
ulx() { r=$(pwd -P); "$cancho" vcs unlock "$@" --output json 2> "$work/err.txt" | sed "s#$r#<root>#g" > "$work/out.json"; }
left() { find .cancho/vcs -name '*lock*' | sort; }
repo; ulx ; same "unlock: none" "$gold/unlock_none.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_none.left")" ]; then echo "ok unlock: none: the locks left are the Rust command's"; else echo "FAILS unlock: none: locks left differ"; fi
repo; ulx --dry-run; same "unlock: none dry" "$gold/unlock_none_dry.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_none_dry.left")" ]; then echo "ok unlock: none dry: the locks left are the Rust command's"; else echo "FAILS unlock: none dry: locks left differ"; fi
repo; echo '4242 99999999999' > .cancho/vcs/lock; ulx ; same "unlock: top" "$gold/unlock_top.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_top.left")" ]; then echo "ok unlock: top: the locks left are the Rust command's"; else echo "FAILS unlock: top: locks left differ"; fi
repo; echo '4242 99999999999' > .cancho/vcs/lock; ulx --dry-run; same "unlock: top dry" "$gold/unlock_top_dry.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_top_dry.left")" ]; then echo "ok unlock: top dry: the locks left are the Rust command's"; else echo "FAILS unlock: top dry: locks left differ"; fi
repo; echo '7 99999999999' > .cancho/vcs/refs/heads/main.lock; ulx ; same "unlock: ref" "$gold/unlock_ref.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_ref.left")" ]; then echo "ok unlock: ref: the locks left are the Rust command's"; else echo "FAILS unlock: ref: locks left differ"; fi
repo; echo '7 99999999999' > .cancho/vcs/refs/heads/main.lock; echo '9 99999999999' > .cancho/vcs/lock; ulx ; same "unlock: both" "$gold/unlock_both.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_both.left")" ]; then echo "ok unlock: both: the locks left are the Rust command's"; else echo "FAILS unlock: both: locks left differ"; fi
repo; echo xyz > .cancho/vcs/lock; ulx ; same "unlock: garbage" "$gold/unlock_garbage.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_garbage.left")" ]; then echo "ok unlock: garbage: the locks left are the Rust command's"; else echo "FAILS unlock: garbage: locks left differ"; fi
repo; : > .cancho/vcs/lock; ulx ; same "unlock: empty" "$gold/unlock_empty.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_empty.left")" ]; then echo "ok unlock: empty: the locks left are the Rust command's"; else echo "FAILS unlock: empty: locks left differ"; fi
repo; echo 12 > .cancho/vcs/lock; ulx ; same "unlock: pid only" "$gold/unlock_pid_only.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_pid_only.left")" ]; then echo "ok unlock: pid only: the locks left are the Rust command's"; else echo "FAILS unlock: pid only: locks left differ"; fi
repo; echo '99999999999 99999999999' > .cancho/vcs/lock; ulx ; same "unlock: big pid" "$gold/unlock_big_pid.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_big_pid.left")" ]; then echo "ok unlock: big pid: the locks left are the Rust command's"; else echo "FAILS unlock: big pid: locks left differ"; fi
repo; echo '+12 +99999999999' > .cancho/vcs/lock; ulx ; same "unlock: plus" "$gold/unlock_plus.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_plus.left")" ]; then echo "ok unlock: plus: the locks left are the Rust command's"; else echo "FAILS unlock: plus: locks left differ"; fi
repo; echo '0 99999999999' > .cancho/vcs/lock; ulx ; same "unlock: zero" "$gold/unlock_zero.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_zero.left")" ]; then echo "ok unlock: zero: the locks left are the Rust command's"; else echo "FAILS unlock: zero: locks left differ"; fi
repo; mkdir -p .cancho/vcs/refs/heads/a/b; echo '1 99999999999' > .cancho/vcs/refs/heads/a/b/x.lock; ulx ; same "unlock: nested" "$gold/unlock_nested.json" "$work/out.json"; if [ "$(left)" = "$(cat "$gold/unlock_nested.left")" ]; then echo "ok unlock: nested: the locks left are the Rust command's"; else echo "FAILS unlock: nested: locks left differ"; fi

echo "== routing"
empty; "$cancho" vcs log > "$work/out.txt" 2>&1; echo "exit $?"; grep -c "nothing published yet" "$work/out.txt" | sed 's/^/manifest log lines: /'
repo; "$cancho" vcs log --store elsewhere > "$work/out.txt" 2>&1; echo "exit $?"; grep -o "nothing published yet at .elsewhere." "$work/out.txt"

echo "== what is not in this build, and what is not an option"
repo; vcs log --path notes.md;  exit_is "log --path" 2; grep -q "not in this build" "$work/err.txt" && echo "ok log --path says it is not in this build" || echo "FAILS log --path said nothing of the kind"
repo; vcs show --patch;         exit_is "show --patch" 2; grep -q "not in this build" "$work/err.txt" && echo "ok show --patch says it is not in this build" || echo "FAILS show --patch said nothing of the kind"
repo; vcs log --bogus;          exit_is "log --bogus" 2; grep -q "unknown option" "$work/err.txt" && echo "ok log --bogus is an unknown option" || echo "FAILS log --bogus"
repo; "$cancho" vcs show --output text > "$work/out.json" 2> "$work/err.txt"; echo $? > "$work/code.txt"; exit_is "show --output text" 2
# `--limit` as the last word has no value (a helper that added `--output json` after it would hand `--output` to it, as the Rust parser does).
repo; "$cancho" vcs log --limit > "$work/out.json" 2> "$work/err.txt"; echo $? > "$work/code.txt"; exit_is "log --limit with no value" 2; grep -q "needs a value" "$work/err.txt" && echo "ok it says --limit needs a value" || echo "FAILS --limit with no value said nothing of the kind"
cd "$root" || exit 1
echo done

echo "== record"
rcx() { "$cancho" vcs record "$@" --author "Ada <ada@example.com>" --time 2000 --output json > "$work/out.json" 2> "$work/err.txt"; echo $? > "$work/code.txt"; }
store() { find .cancho/vcs -type f -not -name '*.log' -not -name lock | sort | while read -r f; do echo "$f $(cksum < "$f")"; done; }
store_was() { if [ "$(store)" = "$(cat "$gold/$2")" ]; then echo "ok $1: the store is as the Rust command left it"; else echo "FAILS $1: the store differs"; fi; }
facts() { grep -E '"(path|rule|tag|ok)":' "$1"; }
same_facts() { if [ "$(facts "$2")" = "$(facts "$3")" ]; then echo "ok $1: the tag, the rule and the path are the Rust command's"; else echo "FAILS $1"; diff "$2" "$3" | head -8; fi; }
repo; rcx --all; same "record: clean" "$gold/record_clean.json" "$work/out.json"; store_was "record: clean" record_clean.store; no_lock_after "record: clean"
repo; printf 'four\n' >> notes.md; rcx --all; same "record: notes" "$gold/record_notes.json" "$work/out.json"; store_was "record: notes" record_notes.store; no_lock_after "record: notes"
repo; printf 'four\n' >> notes.md; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 1; }\n' > a.cho; rcx notes.md; same "record: notes path" "$gold/record_notes_path.json" "$work/out.json"; store_was "record: notes path" record_notes_path.store; no_lock_after "record: notes path"
repo; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 1; }\npub fn thrice(n: int) -> [] int { return n * 3; }\n' > a.cho; rcx a.cho; same "record: cho body" "$gold/record_cho_body.json" "$work/out.json"; store_was "record: cho body" record_cho_body.store; no_lock_after "record: cho body"
repo; printf 'module a;\npub fn twice(n: int, m: int) -> [] int { return n + m; }\npub fn thrice(n: int) -> [] int { return n * 3; }\n' > a.cho; rcx a.cho; same "record: cho sig" "$gold/record_cho_sig.json" "$work/out.json"; store_was "record: cho sig" record_cho_sig.store; no_lock_after "record: cho sig"
repo; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 0; }\npub fn thrice(n: int) -> [] int { return n * 3; }\npub struct P { x: int }\n' > a.cho; rcx a.cho; same "record: cho add" "$gold/record_cho_add.json" "$work/out.json"; store_was "record: cho add" record_cho_add.store; no_lock_after "record: cho add"
repo; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 0; }\n' > a.cho; rcx a.cho; same "record: cho remove" "$gold/record_cho_remove.json" "$work/out.json"; store_was "record: cho remove" record_cho_remove.store; no_lock_after "record: cho remove"
repo; mkdir -p d/e; printf 'x\n' > d/e/x.txt; printf 'y\n' > y.txt; ln -s notes.md lnk; rcx --all; same "record: new files" "$gold/record_new_files.json" "$work/out.json"; store_was "record: new files" record_new_files.store; no_lock_after "record: new files"
repo; rm notes.md; rcx --all; same "record: delete" "$gold/record_delete.json" "$work/out.json"; store_was "record: delete" record_delete.store; no_lock_after "record: delete"
repo; chmod +x notes.md; rcx --all; same "record: mode" "$gold/record_mode.json" "$work/out.json"; store_was "record: mode" record_mode.store; no_lock_after "record: mode"
repo; printf 'four\n' >> notes.md; rcx --all --dry-run; same "record: dry" "$gold/record_dry.json" "$work/out.json"; store_was "record: dry" record_dry.store; no_lock_after "record: dry"
repo; printf 'module a;\npub fn twice(n: int) -> [] int { return true; }\n' > a.cho; rcx --all --no-gate; same "record: no gate" "$gold/record_no_gate.json" "$work/out.json"; store_was "record: no gate" record_no_gate.store; no_lock_after "record: no gate"
repo; printf 'four\n' >> notes.md; rcx --all --intent-prompt 'why: four' --intent-model m --intent-session s --intent-issue 9; same "record: intent" "$gold/record_intent.json" "$work/out.json"; store_was "record: intent" record_intent.store; no_lock_after "record: intent"
repo; printf 'four\n' >> notes.md; rcx --all --expect main; same "record: expect ok" "$gold/record_expect_ok.json" "$work/out.json"; store_was "record: expect ok" record_expect_ok.store; no_lock_after "record: expect ok"
repo; printf 'four\n' >> notes.md; rcx --all --expect HEAD~1; same_facts "record: expect wrong" "$gold/record_expect_wrong.json" "$work/out.json"; store_was "record: expect wrong" record_expect_wrong.store; no_lock_after "record: expect wrong"
repo; rcx ; same "record: nothing selected" "$gold/record_nothing_selected.json" "$work/out.json"; store_was "record: nothing selected" record_nothing_selected.store; no_lock_after "record: nothing selected"
repo; vcs record --all --time abc; same "record: bad time on a clean tree is unchanged, as in Rust" "$gold/record_bad_time_clean.json" "$work/out.json"; store_was "record: bad time clean" record_bad_time_clean.store
repo; printf 'four\n' >> notes.md; vcs record --all --time abc; same "record: bad time" "$gold/record_bad_time.json" "$work/out.json"; store_was "record: bad time" record_bad_time.store; no_lock_after "record: bad time"
repo; rcx ../x; same "record: escapes" "$gold/record_escapes.json" "$work/out.json"; store_was "record: escapes" record_escapes.store; no_lock_after "record: escapes"
repo; printf 'module b;\nimport a;\npub fn h() -> [] int { return a.twice(1); }\n' > b.cho; rcx --all; same "record: imports" "$gold/record_imports.json" "$work/out.json"; store_was "record: imports" record_imports.store; no_lock_after "record: imports"
repo; printf 'edition 12;\nmodule s;\nimport std.bytes;\npub fn h() -> [] bool { return bytes.equal("a", "b"); }\n' > s.cho; rcx --all; same "record: std" "$gold/record_std.json" "$work/out.json"; store_was "record: std" record_std.store; no_lock_after "record: std"
repo; printf '\377\376abc' > bin.dat; rcx --all; same "record: binary" "$gold/record_binary.json" "$work/out.json"; store_was "record: binary" record_binary.store; no_lock_after "record: binary"
repo; seq 1 40 > notes.md; rcx --all; same "record: multi hunk" "$gold/record_multi_hunk.json" "$work/out.json"; store_was "record: multi hunk" record_multi_hunk.store; no_lock_after "record: multi hunk"
repo; printf 'module a;\npub fn twice(n: int) -> [] int { return true; }\n' > a.cho; rcx --all; same_facts "record: gate fail" "$gold/record_gate_fail.json" "$work/out.json"; store_was "record: gate fail" record_gate_fail.store; no_lock_after "record: gate fail"
repo; printf 'module b;\nimport a;\npub fn h() -> [] int { return nope(1); }\n' > b.cho; rcx --all; same_facts "record: gate fail import" "$gold/record_gate_fail_import.json" "$work/out.json"; store_was "record: gate fail import" record_gate_fail_import.store; no_lock_after "record: gate fail import"
repo; printf 'module a;\npub fn twice( {\n' > a.cho; rcx --all; same_facts "record: parse fail" "$gold/record_parse_fail.json" "$work/out.json"; store_was "record: parse fail" record_parse_fail.store; no_lock_after "record: parse fail"
repo; "$cancho" vcs switch --detach HEAD~1 --output json > /dev/null; printf 'four\n' >> notes.md; rcx --all; same "record: detached" "$gold/record_detached.json" "$work/out.json"; { store; cat .cancho/vcs/HEAD; } > "$work/store.txt"; if cmp -s "$work/store.txt" "$gold/record_detached.store"; then echo "ok record: detached: HEAD moved to the change, as the Rust command moves it"; else echo "FAILS record: detached: the store or HEAD differs"; fi
repo; mkdir -p d; printf 'q\n' > d/y.txt; cd d || exit 1; rcx y.txt ../notes.md; cd "$work/r" || exit 1; same "record: from a subdirectory" "$gold/record_subdir.json" "$work/out.json"; store_was "record: from a subdirectory" record_subdir.store
empty; "$cancho" vcs init --output json > /dev/null; rcx --all; same "record: nothing in an empty repository" "$gold/record_empty_nothing.json" "$work/out.json"; exit_is "record, empty and nothing" 1; store_was "record: nothing in an empty repository" record_empty_nothing.store
empty; "$cancho" vcs init --output json > /dev/null; printf 'one\n' > f.txt; rcx --all; same "record: first change of an empty repository" "$gold/record_empty_first.json" "$work/out.json"; store_was "record: first change" record_empty_first.store
repo; printf 'four\n' >> notes.md; printf '4242 1000' > .cancho/vcs/lock; rcx --all; if grep -q '"tag": "lock-held"' "$work/out.json" && grep -q '"command": "record"' "$work/out.json" && grep -q '"pid": 4242' "$work/out.json"; then echo "ok record with the lock held: lock-held with its pid"; else echo "FAILS record, lock held"; cat "$work/out.json"; fi; exit_is "record, lock held" 1; [ "$(cat .cancho/vcs/lock)" = "4242 1000" ] && echo "ok a held lock is left" || echo "FAILS the held lock was disturbed"
repo; printf 'four\n' >> notes.md; printf '4242 1000' > .cancho/vcs/lock; rcx --all --dry-run; if grep -q '"dry_run": true' "$work/out.json"; then echo "ok record --dry-run does not take the lock"; else echo "FAILS record --dry-run with a held lock"; fi
repo; rcx --all; exit_is "record, nothing changed" 0
repo; printf 'module a;\npub fn twice( {\n' > a.cho; rcx --all; exit_is "record, a gate refusal" 1
repo; rcx; exit_is "record, nothing selected" 1
repo; printf 'four\n' >> notes.md; vcs record --all --time abc; exit_is "record, a bad time" 1
repo; printf 'four\n' >> notes.md; rcx --all --expect HEAD~1; exit_is "record, a wrong expect" 1
