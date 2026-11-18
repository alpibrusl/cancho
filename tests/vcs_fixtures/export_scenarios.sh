#!/bin/sh
# export_scenarios.sh -- the repositories `cancho vcs export-git` is checked on besides the ones `import-git` makes from tests/vcs_fixtures/git/ (sourced by make_export_golden.sh, which
# runs them with the Rust command, the oracle, and by tests/commands/vcs_repo.sh, which runs them with the command in cancho). Each function works in the current directory, which is empty,
# and uses `$cancho`; `$gitdir` is the directory of the git fixtures.

# Changes made here: three records (one with an intent that has a session, a model and an issue, quotes, a backslash and a non-ASCII word, one with no intent, one with an intent of one
# word), a file, an executable, a link and a directory, then a branch.
scn_made() {
    "$cancho" vcs init > /dev/null
    printf 'one\n' > a.txt; mkdir d; printf 'two\n' > d/b.txt; printf '#!/bin/sh\necho hi\n' > run.sh; chmod +x run.sh; ln -s a.txt link
    "$cancho" vcs record --all --author "Ada <ada@example.com>" --time 1700000000 --intent-prompt 'first "quoted" \ and ünïcode  ' --intent-model m1 --intent-session s1 --intent-issue 7 --output json > /dev/null
    printf 'three\n' >> d/b.txt
    "$cancho" vcs record --all --author "Bob" --time 0 --output json > /dev/null
    printf 'x\n' > e.txt
    "$cancho" vcs record --all --author "Cy <cy@example.com>" --time 1700000200 --intent-prompt third --output json > /dev/null
    "$cancho" vcs branch create feature --output json > /dev/null
}

# The history of plain.git imported, then one change made here on top of it (its parent is a change that came from git, and its tree has the files of git's) and a branch.
scn_mixed() {
    "$cancho" vcs init > /dev/null
    "$cancho" vcs import-git ../g/plain.git --no-checkout --output json > /dev/null 2>&1
    printf 'added here\n' > added.txt
    "$cancho" vcs record --all --author "Dee <dee@example.com>" --time 1700000300 --intent-prompt "on top of git" --intent-session s9 --output json > /dev/null
    "$cancho" vcs branch create here --output json > /dev/null
}

# A repository with no change: HEAD names a branch that has no ref.
scn_empty() {
    "$cancho" vcs init > /dev/null
}

# HEAD detached at the newest change and no branch left: the only way to that change is HEAD, which the export then takes as a tip of its own and writes as a git id, not as a branch.
scn_headonly() {
    "$cancho" vcs init > /dev/null
    printf 'a\n' > a.txt
    "$cancho" vcs record --all --author "A" --time 5 --intent-prompt x --output json > /dev/null
    printf 'b\n' >> a.txt
    "$cancho" vcs record --all --author "A" --time 6 --output json > /dev/null
    "$cancho" vcs branch create feature --output json > /dev/null
    id=$(cat .cancho/vcs/refs/heads/main)
    "$cancho" vcs switch --detach "$id" --output json > /dev/null
    "$cancho" vcs branch delete main --expect "$id" --output json > /dev/null
    "$cancho" vcs branch delete feature --expect "$id" --output json > /dev/null
}
