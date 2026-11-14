# make.sh -- how tests/vcs_fixtures/ was made, and a check that it can be made again: the repository of `crates/cancho/tests/vcs_repo.rs`'s `two_changes` and its third record (three changes,
# fixed times and author, so every object, change and ref is the same each time; only the wall-clock seconds in `refs/heads/main.log`, which nothing reads, differ), and the documents the Rust `cancho vcs` prints for the commands of `docs/vcs-routing.md`
# slice R1 that `crates/cancho/tests/vcs_pins/` does not pin. Written once by the Rust command; the tests of the command line in cancho compare with them, so they outlive the Rust code (D6).
#
#   sh tests/vcs_fixtures/make.sh RUST_CANCHO OUTDIR      writes OUTDIR/three_changes and OUTDIR/golden; `diff -r` them with the committed ones (only that one log differs)
set -eu
R=$1
out=$2
mkdir -p "$out"
out=$(cd "$out" && pwd)
case $R in */*) R=$(cd "$(dirname "$R")" && pwd)/$(basename "$R") ;; esac
rm -rf "$out/three_changes" "$out/four_changes" "$out/golden"
mkdir -p "$out/three_changes" "$out/golden"
export CANCHO_AUTHOR="Ada <ada@example.com>"
cd "$out/three_changes"
printf 'module a;\npub fn twice(n: int) -> [] int { return n + n; }\n' > a.cho
printf '# notes\none\ntwo\nthree\n' > notes.md
"$R" vcs init --output json > /dev/null
"$R" vcs record --all --intent-prompt first --intent-model m1 --intent-session s1 --time 1000 --output json > /dev/null
printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 0; }\npub fn thrice(n: int) -> [] int { return n * 3; }\n' > a.cho
printf '# notes\none\n2\nthree\n' > notes.md
"$R" vcs record a.cho --intent-prompt second --intent-issue 7 --time 1001 --output json > /dev/null
"$R" vcs record --all --intent-prompt "third: notes" --time 1002 --output json > /dev/null

# four_changes (R6, `switch`): three_changes and a branch `wide` one change ahead of main that removes a file, adds files (one in a new directory), changes a mode and adds a link; HEAD is back on main.
cp -R "$out/three_changes" "$out/four_changes"
cd "$out/four_changes"
"$R" vcs switch --create wide --output json > /dev/null
rm a.cho; chmod +x notes.md; mkdir -p sub/deep; printf 'x\n' > sub/deep/x.txt; printf 'y\n' > top.txt; ln -s notes.md lnk
"$R" vcs record --all --intent-prompt "fourth: wide" --time 1003 --output json > /dev/null
"$R" vcs switch main --output json > /dev/null

# The documents, each from a copy of the repository (or an empty directory), the root made `<root>`.
g=$out/golden
norm() { sed 's#"root": "[^"]*"#"root": "<root>"#'; }
here() { rm -rf "$out/w"; mkdir -p "$out/w"; cp -R "$out/three_changes/." "$out/w/"; cd "$out/w"; }
empty() { rm -rf "$out/e"; mkdir -p "$out/e"; cd "$out/e"; }
here; "$R" vcs log HEAD~1 --output json > "$g/log_rev.json"
here; "$R" vcs log --limit 1 --output json > "$g/log_limit_1.json"
here; "$R" vcs log --limit 0 --output json > "$g/log_limit_0.json"
here; "$R" vcs log nosuchbranch --output json > "$g/refuse_log_bad_revision.json" || true
here; "$R" vcs log --limit x --output json > "$g/refuse_log_bad_limit.json" || true
here; "$R" vcs show HEAD~1 --output json > "$g/show_parent.json"
here; "$R" vcs show nosuchbranch --output json > "$g/refuse_show_bad_revision.json" || true
here; "$R" vcs show HEAD~9 --output json > "$g/refuse_show_past_the_first.json" || true
empty; "$R" vcs show --output json > "$g/refuse_show_not_a_repository.json" || true
empty; "$R" vcs init --dry-run --output json > "$g/init_dry.json"
here; "$R" vcs init --dry-run --output json > "$g/init_dry_existing.json"
empty; "$R" vcs init --branch dev --output json | norm > "$g/init_branch_dev.json"
empty; "$R" vcs init sub/new --output json | sed 's#"root": ".*/sub/new"#"root": "<root>/sub/new"#' > "$g/init_new_directory.json"
empty; "$R" vcs init --branch 'bad name' --output json > "$g/refuse_init_bad_branch.json" || true
# branch (R2): each from a copy of the repository; `$R vcs branch ...` is run as the commands before it were.
br() { "$R" vcs branch "$@" --output json; }
here; br list > "$g/branch_list_initial.json"
empty; "$R" vcs init --output json > /dev/null; br list > "$g/branch_list_empty.json"
empty; "$R" vcs init --output json > /dev/null; br create x > "$g/refuse_branch_create_no_change.json" || true
here; br create topic > "$g/branch_create_head.json"
here; br create feature --from HEAD~1 > /dev/null; br create feature --from HEAD~1 > "$g/branch_create_again.json"
here; br create feature --from HEAD~1 > /dev/null; br create feature --from HEAD > "$g/refuse_branch_create_moved.json" || true
here; br create feature --dry-run > "$g/branch_create_dry.json"
here; br create 'bad name' > "$g/refuse_branch_name.json" || true
here; br create > "$g/refuse_branch_no_name.json" || true
here; br create x --from nosuch > "$g/refuse_branch_create_bad_from.json" || true
here; br frobnicate > "$g/refuse_branch_unknown.json" || true
here; br create feature --from HEAD~1 > /dev/null; br delete feature --expect HEAD~1 --dry-run > "$g/branch_delete_dry.json"
here; br create feature --from HEAD~1 > /dev/null; br delete feature > "$g/refuse_branch_delete_no_expect.json" || true
here; br create feature --from HEAD~1 > /dev/null; br delete feature --expect HEAD > "$g/refuse_branch_delete_moved.json" || true
here; br delete main --expect HEAD > "$g/refuse_branch_delete_current.json" || true
here; br delete nosuch > "$g/branch_delete_missing.json"
here; br create 'a.b' > /dev/null; br create 'a/b' --from HEAD~1 > /dev/null; br create a0 --from HEAD~2 > /dev/null; br create b > /dev/null; br list > "$g/branch_list_sorted.json"
here; br create feature --from HEAD~1 > /dev/null; br delete feature --expect HEAD~1 > /dev/null; br list > "$g/branch_list_after_delete.json"
# status (R4): each from a copy of the repository, which is clean at HEAD; `st` is the command, then one thing done to the working copy.
st() { "$R" vcs status --output json; }
here; st > "$g/status_clean_fixture.json"
here; rm a.cho; st > "$g/status_deleted.json"
here; mkdir -p sub/deep; printf 'x\n' > sub/deep/new.txt; printf 'y\n' > top.txt; st > "$g/status_added.json"
here; chmod +x notes.md; st > "$g/status_mode_changed.json"
here; ln -s a.cho lnk; st > "$g/status_link_added.json"
here; ln -s a.cho lnk; ln -s notes.md lnk2; ln -s nowhere dangling; mkdir sub; ln -s ../a.cho sub/up; ln -s sub link_to_dir; st > "$g/status_links.json"
here; rm notes.md; ln -s a.cho notes.md; st > "$g/status_file_became_link.json"
here; ln -s "$(printf 'x%.0s' $(seq 1 500))" long; st > "$g/status_long_link.json"
here; printf '*.log\nsub/up\n' > .canchoignore; ln -s x1 a.log; mkdir sub; ln -s ../a.cho sub/up; st > "$g/status_ignored.json"
here; rm a.cho; mkdir a.cho; printf z > a.cho/inner; st > "$g/status_file_became_dir.json"
empty; st > "$g/refuse_status_not_a_repository.json" || true
empty; "$R" vcs init --output json > /dev/null; st > "$g/status_empty_repository.json"
# restore (R5): each from a copy of the repository (clean at HEAD), something done to the working copy, then `rs`. `restore_from_parent_*` keep the files the Rust command wrote.
rs() { "$R" vcs restore "$@" --output json; }
here; printf 'scribble\n' > notes.md; rs notes.md > "$g/restore_one.json"
here; printf 'scribble\n' > notes.md; rs notes.md --dry-run > "$g/restore_dry.json"
here; rs notes.md > "$g/restore_clean.json"
here; rm a.cho; chmod +x notes.md; ln -s a.cho lnk; mkdir -p sub/deep; printf x > sub/new; printf y > sub/deep/new2; printf z > top.txt; rs . > "$g/restore_all.json"
here; rm a.cho; mkdir a.cho; printf z > a.cho/inner; rs . > "$g/refuse_restore_file_became_dir.json" || true
here; mkdir -p sub/deep; printf x > sub/new; printf y > sub/deep/new2; printf z > top.txt; rs sub > "$g/restore_beneath.json"
here; rs . --from HEAD~1 > "$g/restore_from_parent.json"; cp notes.md "$g/restore_from_parent.notes.md"; cp a.cho "$g/restore_from_parent.a.cho"
here; rs --from HEAD~1 > "$g/refuse_restore_no_path.json" || true
here; printf 'scribble\n' > notes.md; rs nosuchfile > "$g/restore_no_match.json"
here; rs ../elsewhere > "$g/refuse_restore_escapes.json" || true
here; printf 'scribble\n' > notes.md; rs notes.md --from nosuch > "$g/refuse_restore_bad_from.json" || true
here; mkdir -p sub; printf x > sub/new; printf 'scribble\n' > notes.md; ( cd sub && rs ../notes.md > "$g/restore_from_subdir.json" )
here; mkdir -p sub; printf x > sub/new; printf 'scribble\n' > notes.md; ( cd sub && rs . > "$g/restore_in_subdir.json" )
here; printf 'scribble\n' > notes.md; printf '4242 1000' > .cancho/vcs/lock; rs notes.md --dry-run > "$g/restore_dry_locked.json"
here; rm a.cho; rs a.cho notes.md > "$g/restore_two_paths.json"
# switch (R6): each from a copy of the repository (`here`, three changes, HEAD on main) or of four_changes (`wide`), then `sw`. `listing` is the working copy as the tests list it: kind, path, content sum, execute bit, link target.
sw() { "$R" vcs switch "$@" --output json; }
wide() { rm -rf "$out/w"; mkdir -p "$out/w"; cp -R "$out/four_changes/." "$out/w/"; cd "$out/w"; }
listing() { find . -path ./.cancho -prune -o -print | sort | while read -r f; do if [ -L "$f" ]; then echo "l $f -> $(readlink "$f")"; elif [ -d "$f" ]; then echo "d $f"; elif [ -x "$f" ]; then echo "x $f $(cksum < "$f")"; else echo "f $f $(cksum < "$f")"; fi; done; }
br() { "$R" vcs branch "$@" --output json; }
here; sw main > "$g/switch_noop.json"
here; sw topic --create > "$g/switch_create.json"; cp .cancho/vcs/HEAD "$g/switch_create.HEAD"
here; sw topic --create --dry-run > "$g/switch_create_dry.json"
here; sw main --create > "$g/refuse_switch_create_exists.json" || true
here; sw 'bad name' --create > "$g/refuse_switch_create_bad_name.json" || true
empty; "$R" vcs init --output json > /dev/null; sw x --create > "$g/refuse_switch_create_no_change.json" || true
empty; "$R" vcs init --output json > /dev/null; sw main > "$g/refuse_switch_empty_repository.json" || true
here; br create old --from HEAD~1 > /dev/null; sw old > "$g/switch_branch.json"; cp .cancho/vcs/HEAD "$g/switch_branch.HEAD"
here; br create old --from HEAD~1 > /dev/null; sw old --dry-run > "$g/switch_branch_dry.json"
here; sw HEAD~1 > "$g/switch_detach_parent.json"; cp .cancho/vcs/HEAD "$g/switch_detach_parent.HEAD"
here; sw main --detach > "$g/switch_detach_branch.json"; cp .cancho/vcs/HEAD "$g/switch_detach_branch.HEAD"
here; sw nosuch > "$g/refuse_switch_bad_revision.json" || true
here; sw > "$g/refuse_switch_no_argument.json" || true
here; sw a b > "$g/refuse_switch_two_arguments.json" || true
here; br create old --from HEAD~1 > /dev/null; printf 'scribble\n' > notes.md; sw old > "$g/refuse_switch_dirty.json" || true
here; br create old --from HEAD~1 > /dev/null; printf 'z\n' > zzz.txt; sw old > "$g/refuse_switch_dirty_other.json" || true
here; br create old --from HEAD~1 > /dev/null; printf 'z\n' > zzz.txt; sw old --keep > "$g/switch_keep_carries.json"
here; br create old --from HEAD~1 > /dev/null; printf 'scribble\n' > notes.md; sw old --keep > "$g/refuse_switch_keep_in_the_way.json" || true
here; br create old --from HEAD~1 > /dev/null; printf 'scribble\n' > notes.md; printf 'z\n' > zzz.txt; sw old --discard > "$g/switch_discard.json"
here; br create old --from HEAD~1 > /dev/null; printf 'scribble\n' > notes.md; sw old --dry-run > "$g/refuse_switch_dry_dirty.json" || true
here; br create old --from HEAD~1 > /dev/null; sw old > /dev/null; sw main > "$g/switch_back.json"
here; br create old --from HEAD~1 > /dev/null; printf '4242 1000' > .cancho/vcs/lock; sw old --dry-run > "$g/switch_dry_locked.json"
wide; sw wide > "$g/switch_wide.json"; cp .cancho/vcs/HEAD "$g/switch_wide.HEAD"; listing > "$g/switch_wide.tree"
wide; sw wide > /dev/null; sw main > "$g/switch_wide_back.json"; listing > "$g/switch_wide_back.tree"
wide; printf 'q\n' > unrelated.txt; sw wide --keep > "$g/switch_wide_keep.json"; listing > "$g/switch_wide_keep.tree"
wide; sw wide --dry-run > "$g/switch_wide_dry.json"
wide; mkdir -p sub; sw wide > /dev/null; cd sub || exit 1; sw main > "$g/switch_from_subdir.json"
# apply (R7): the documents of `apply FILE`, and the working copy each leaves (`.tree`); the edits are written to OUT/edits.json and named by their absolute path.
ap() { printf '%s' "$1" > "$out/edits.json"; shift; "$R" vcs apply "$out/edits.json" "$@" --output json; }
here; ap '[{"path":"notes.md","old":"2","new":"two"}]'  > "$g/apply_basic.json" || true; listing > "$g/apply_basic.tree"
here; ap '[{"path":"notes.md","old":"one","new":"uno"},{"path":"notes.md","old":"uno\n","new":"UNO\n"}]'  > "$g/apply_seq.json" || true; listing > "$g/apply_seq.tree"
here; ap '[{"path":"notes.md","old":"one","new":"1"},{"path":"a.cho","old":"twice","new":"TWICE"}]'  > "$g/apply_two_files.json" || true; listing > "$g/apply_two_files.tree"
here; ap '[{"path":"a.cho","old":"twice","new":"TWICE"},{"path":"notes.md","old":"one","new":"1"}]'  > "$g/apply_two_files_reversed.json" || true; listing > "$g/apply_two_files_reversed.tree"
here; printf 'aaa\n' > notes.md; ap '[{"path":"notes.md","old":"aa","new":"b"}]'  > "$g/apply_overlap.json" || true; listing > "$g/apply_overlap.tree"
here; ap '[{"path":"notes.md","old":"2","new":"two"}]' --dry-run > "$g/apply_dry.json" || true; listing > "$g/apply_dry.tree"
here; ap '[{"path":"a.cho","old":"twice","new":"TWICE"},{"path":"notes.md","old":"zzz","new":"b"}]'  > "$g/apply_atomic.json" || true; listing > "$g/apply_atomic.tree"
here; ap '[{"path":"notes.md","old":"zzz","new":"b"},{"path":"a.cho","old":"twice","new":"TWICE"}]'  > "$g/apply_atomic_reversed.json" || true; listing > "$g/apply_atomic_reversed.tree"
here; ap '[{"path":"notes.md","old":"zzz","new":"b"}]'  > "$g/apply_not_found.json" || true; listing > "$g/apply_not_found.tree"
here; printf 'x x\n' > notes.md; ap '[{"path":"notes.md","old":"x","new":"b"}]'  > "$g/apply_ambiguous.json" || true; listing > "$g/apply_ambiguous.tree"
here; ap '[{"path":"a.cho","old":"fn","new":"FN"}]'  > "$g/apply_ambiguous_code.json" || true; listing > "$g/apply_ambiguous_code.tree"
here; ap '[{"path":"notes.md","old":"","new":"b"}]'  > "$g/apply_empty_anchor.json" || true; listing > "$g/apply_empty_anchor.tree"
here; ap '[{"path":"../x","old":"a","new":"b"}]'  > "$g/apply_escapes.json" || true; listing > "$g/apply_escapes.tree"
here; ap '[]'  > "$g/apply_empty_list.json" || true; listing > "$g/apply_empty_list.tree"
here; chmod +x notes.md; ap '[{"path":"notes.md","old":"2","new":"two"}]'  > "$g/apply_exec.json" || true; listing > "$g/apply_exec.tree"
here; printf 'caf\303\251 ok\n' > notes.md; ap '[{"path":"notes.md","old":"caf\u00e9","new":"tea \ud83d\ude00"}]'  > "$g/apply_unicode.json" || true; listing > "$g/apply_unicode.tree"
here; ap '[{"path":"./sub/../notes.md","old":"2","new":"two"}]'  > "$g/apply_dotted.json" || true; listing > "$g/apply_dotted.tree"
here; ap '[{"path":"./sub/../notes.md","old":"zz","new":"two"}]'  > "$g/apply_dotted_refused.json" || true; listing > "$g/apply_dotted_refused.tree"
here; ap '[{"path":"notes.md","old":"one","new":"1"},{"path":"./notes.md","old":"2","new":"two"}]'  > "$g/apply_spellings.json" || true; listing > "$g/apply_spellings.tree"
here; ap '[{"path":"notes.md","old":"one","new":"1"},{"path":"./notes.md","old":"qq","new":"two"}]'  > "$g/apply_spellings_refused.json" || true; listing > "$g/apply_spellings_refused.tree"
here; mkdir -p s/t; printf 'x y\n' > s/t/f.txt; ap '[{"path":"s/t/f.txt","old":"y","new":"z"}]'  > "$g/apply_subdirectory.json" || true; listing > "$g/apply_subdirectory.tree"
here; printf 'a\r\nb\r\n' > notes.md; ap '[{"path":"notes.md","old":"a\r\nb","new":"c"}]'  > "$g/apply_crlf.json" || true; listing > "$g/apply_crlf.tree"
here; ap '[{"path":"notes.md","old":"one\n","new":""}]'  > "$g/apply_deleting.json" || true; listing > "$g/apply_deleting.tree"
here; seq 1 5000 > notes.md; ap '[{"path":"notes.md","old":"\n2500\n","new":"\nX\n"}]'  > "$g/apply_large.json" || true; listing > "$g/apply_large.tree"
here; ap '  [ { "new" : "ONE" , "old" : "one" , "path" : "notes.md" } ]  '  > "$g/apply_spaced_json.json" || true; listing > "$g/apply_spaced_json.tree"
here; ap '[{"path":"notes.md","old":"2","new":"two","note":1}]'  > "$g/apply_extra_key.json" || true; listing > "$g/apply_extra_key.tree"
here; "$R" vcs apply --output json > "$g/refuse_apply_no_argument.json" || true
here; "$R" vcs apply a b --output json > "$g/refuse_apply_two_arguments.json" || true
# unlock (R7): the documents of `unlock` (the root made <root>, every SINCE in the future so that the age is 0) and the locks left (`.left`).
ul() { r=$(pwd -P); "$R" vcs unlock "$@" --output json | sed "s#$r#<root>#g"; }
left() { find .cancho/vcs -name '*lock*' | sort; }
here; ul  > "$g/unlock_none.json"; left > "$g/unlock_none.left"
here; ul --dry-run > "$g/unlock_none_dry.json"; left > "$g/unlock_none_dry.left"
here; echo '4242 99999999999' > .cancho/vcs/lock; ul  > "$g/unlock_top.json"; left > "$g/unlock_top.left"
here; echo '4242 99999999999' > .cancho/vcs/lock; ul --dry-run > "$g/unlock_top_dry.json"; left > "$g/unlock_top_dry.left"
here; echo '7 99999999999' > .cancho/vcs/refs/heads/main.lock; ul  > "$g/unlock_ref.json"; left > "$g/unlock_ref.left"
here; echo '7 99999999999' > .cancho/vcs/refs/heads/main.lock; echo '9 99999999999' > .cancho/vcs/lock; ul  > "$g/unlock_both.json"; left > "$g/unlock_both.left"
here; echo xyz > .cancho/vcs/lock; ul  > "$g/unlock_garbage.json"; left > "$g/unlock_garbage.left"
here; : > .cancho/vcs/lock; ul  > "$g/unlock_empty.json"; left > "$g/unlock_empty.left"
here; echo 12 > .cancho/vcs/lock; ul  > "$g/unlock_pid_only.json"; left > "$g/unlock_pid_only.left"
here; echo '99999999999 99999999999' > .cancho/vcs/lock; ul  > "$g/unlock_big_pid.json"; left > "$g/unlock_big_pid.left"
here; echo '+12 +99999999999' > .cancho/vcs/lock; ul  > "$g/unlock_plus.json"; left > "$g/unlock_plus.left"
here; echo '0 99999999999' > .cancho/vcs/lock; ul  > "$g/unlock_zero.json"; left > "$g/unlock_zero.left"
here; mkdir -p .cancho/vcs/refs/heads/a/b; echo '1 99999999999' > .cancho/vcs/refs/heads/a/b/x.lock; ul  > "$g/unlock_nested.json"; left > "$g/unlock_nested.left"
# record (R8): the documents of `record` and the store each leaves (`.store`: the objects and the refs by name and checksum). The author and the time are fixed, so every id is the same each time.
rc() { "$R" vcs record "$@" --author "Ada <ada@example.com>" --time 2000 --output json; }
store() { find .cancho/vcs -type f -not -name '*.log' -not -name lock | sort | while read -r f; do echo "$f $(cksum < "$f")"; done; }
here; rc --all > "$g/record_clean.json" || true; store > "$g/record_clean.store"
here; printf 'four\n' >> notes.md; rc --all > "$g/record_notes.json" || true; store > "$g/record_notes.store"
here; printf 'four\n' >> notes.md; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 1; }\n' > a.cho; rc notes.md > "$g/record_notes_path.json" || true; store > "$g/record_notes_path.store"
here; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 1; }\npub fn thrice(n: int) -> [] int { return n * 3; }\n' > a.cho; rc a.cho > "$g/record_cho_body.json" || true; store > "$g/record_cho_body.store"
here; printf 'module a;\npub fn twice(n: int, m: int) -> [] int { return n + m; }\npub fn thrice(n: int) -> [] int { return n * 3; }\n' > a.cho; rc a.cho > "$g/record_cho_sig.json" || true; store > "$g/record_cho_sig.store"
here; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 0; }\npub fn thrice(n: int) -> [] int { return n * 3; }\npub struct P { x: int }\n' > a.cho; rc a.cho > "$g/record_cho_add.json" || true; store > "$g/record_cho_add.store"
here; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 0; }\n' > a.cho; rc a.cho > "$g/record_cho_remove.json" || true; store > "$g/record_cho_remove.store"
here; mkdir -p d/e; printf 'x\n' > d/e/x.txt; printf 'y\n' > y.txt; ln -s notes.md lnk; rc --all > "$g/record_new_files.json" || true; store > "$g/record_new_files.store"
here; rm notes.md; rc --all > "$g/record_delete.json" || true; store > "$g/record_delete.store"
here; chmod +x notes.md; rc --all > "$g/record_mode.json" || true; store > "$g/record_mode.store"
here; printf 'four\n' >> notes.md; rc --all --dry-run > "$g/record_dry.json" || true; store > "$g/record_dry.store"
here; printf 'module a;\npub fn twice(n: int) -> [] int { return true; }\n' > a.cho; rc --all --no-gate > "$g/record_no_gate.json" || true; store > "$g/record_no_gate.store"
here; printf 'four\n' >> notes.md; rc --all --intent-prompt 'why: four' --intent-model m --intent-session s --intent-issue 9 > "$g/record_intent.json" || true; store > "$g/record_intent.store"
here; printf 'four\n' >> notes.md; rc --all --expect main > "$g/record_expect_ok.json" || true; store > "$g/record_expect_ok.store"
here; printf 'four\n' >> notes.md; rc --all --expect HEAD~1 > "$g/record_expect_wrong.json" || true; store > "$g/record_expect_wrong.store"
here; rc  > "$g/record_nothing_selected.json" || true; store > "$g/record_nothing_selected.store"
here; "$R" vcs record --all --time abc --output json > "$g/record_bad_time_clean.json" || true; store > "$g/record_bad_time_clean.store"
here; printf 'four\n' >> notes.md; "$R" vcs record --all --time abc --output json > "$g/record_bad_time.json" || true; store > "$g/record_bad_time.store"
here; rc ../x > "$g/record_escapes.json" || true; store > "$g/record_escapes.store"
here; printf 'module b;\nimport a;\npub fn h() -> [] int { return a.twice(1); }\n' > b.cho; rc --all > "$g/record_imports.json" || true; store > "$g/record_imports.store"
here; printf 'edition 12;\nmodule s;\nimport std.bytes;\npub fn h() -> [] bool { return bytes.equal("a", "b"); }\n' > s.cho; rc --all > "$g/record_std.json" || true; store > "$g/record_std.store"
here; printf '\377\376abc' > bin.dat; rc --all > "$g/record_binary.json" || true; store > "$g/record_binary.store"
here; seq 1 40 > notes.md; rc --all > "$g/record_multi_hunk.json" || true; store > "$g/record_multi_hunk.store"
here; printf 'module a;\npub fn twice(n: int) -> [] int { return true; }\n' > a.cho; rc --all > "$g/record_gate_fail.json" || true; store > "$g/record_gate_fail.store"
here; printf 'module b;\nimport a;\npub fn h() -> [] int { return nope(1); }\n' > b.cho; rc --all > "$g/record_gate_fail_import.json" || true; store > "$g/record_gate_fail_import.store"
here; printf 'module a;\npub fn twice( {\n' > a.cho; rc --all > "$g/record_parse_fail.json" || true; store > "$g/record_parse_fail.store"
here; rm notes.md; printf 'module a;\npub fn twice(n: int) -> [] int { return n + n + 1; }\n' > a.cho; rc a.cho > "$g/record_one_of_two.json" || true; store > "$g/record_one_of_two.store"
here; rm a.cho; rc --all > "$g/record_delete_cho.json" || true; store > "$g/record_delete_cho.store"
here; printf '\377\376abc' > bin.dat; "$R" vcs record --all --author "Ada <ada@example.com>" --time 1999 --output json > /dev/null; printf '\377\376abd' > bin.dat; rc --all > "$g/record_binary_change.json" || true; store > "$g/record_binary_change.store"
here; printf 'a.cho' > t.txt; ln -s notes.md lnk; "$R" vcs record --all --author "Ada <ada@example.com>" --time 1999 --output json > /dev/null; rm lnk; ln -s a.cho lnk; rc --all > "$g/record_link_change.json" || true; store > "$g/record_link_change.store"
here; "$R" vcs switch --detach HEAD~1 --output json > /dev/null; printf 'four\n' >> notes.md; rc --all > "$g/record_detached.json" || true; store > "$g/record_detached.store"; cat .cancho/vcs/HEAD >> "$g/record_detached.store"
here; mkdir -p d; printf 'q\n' > d/y.txt; (cd d && rc y.txt ../notes.md) > "$g/record_subdir.json" || true; store > "$g/record_subdir.store"
empty; "$R" vcs init --output json > /dev/null; rc --all > "$g/record_empty_nothing.json" || true; store > "$g/record_empty_nothing.store"
empty; "$R" vcs init --output json > /dev/null; printf 'one\n' > f.txt; rc --all > "$g/record_empty_first.json" || true; store > "$g/record_empty_first.store"
rm -rf "$out/w" "$out/e"
echo "made $out/three_changes and $out/golden"
