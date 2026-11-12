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
