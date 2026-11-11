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
rm -rf "$out/w" "$out/e"
echo "made $out/three_changes and $out/golden"
