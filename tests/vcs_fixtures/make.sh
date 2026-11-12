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
rm -rf "$out/three_changes" "$out/golden"
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
rm -rf "$out/w" "$out/e"
echo "made $out/three_changes and $out/golden"
