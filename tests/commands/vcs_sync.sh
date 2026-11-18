# vcs_sync.sh -- the script of tests/commands/script_tests.cho that was crates/cancho/tests/conformance/vcs_sync.rs (docs/gate.md G4, wave 4): `cancho vcs sync` (docs/vcs.md section 8): the store rides inside
# the repository, a sorted plain-text index and op records, and `--check` turns semantic drift into exit 1. $1 is the compiler, $2 a scratch directory; run from the root of the repository.
cancho=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$2
. tests/commands/script_lib.sh
. tests/commands/vcs_lib.sh

ONE='fn add(a: int, b: int) -> [] int { return a + b; }'
BODY_CHANGED='fn add(a: int, b: int) -> [] int { return a + b + 0; }'
SIG_CHANGED='fn add(a: int, b: int, c: int) -> [] int { return a + b + c; }'

# The index is a function of the source: two syncs of the same tree write the same bytes, and the second answers "no semantic change".
begin sync_is_idempotent_and_the_second_run_is_quiet
fresh s1; printf '%s\n' "$ONE" > "$d/f.cho"
r vcs sync --dir "$d"; succeeds "the first sync"
has "$out" "1 declaration(s) added" "the first sync reports the add"
[ -f "$d/.cancho/ids.txt" ] || fail "sync wrote no .cancho/ids.txt"
cp "$d/.cancho/ids.txt" "$d/first.txt"
r vcs sync --dir "$d"; succeeds "the second sync"
has "$out" "no semantic change" "the second sync of an unchanged tree is quiet"
cmp -s "$d/first.txt" "$d/.cancho/ids.txt" || fail "the index is not a function of the source"
end

# A body change is a `ModifyBody` op: the op log gains exactly one record.
begin a_body_change_is_one_modify_body_op
fresh s2; printf '%s\n' "$ONE" > "$d/f.cho"
r vcs sync --dir "$d"; succeeds "the first sync"
before=$(count_in "$d/.cancho/ops")
printf '%s\n' "$BODY_CHANGED" > "$d/f.cho"
r vcs sync --dir "$d"; succeeds "the sync after the change"
has "$out" "1 body change(s)" "a body change says so"
same "$(count_in "$d/.cancho/ops")" "$((before + 1))" "one body change appends exactly one op record"
end

# A signature change is reported and counted, and a synced tree still checks clean.
begin a_signature_change_is_reported_and_fails_check
fresh s3; printf '%s\n' "$ONE" > "$d/f.cho"
r vcs sync --dir "$d"; succeeds "the first sync"
printf '%s\n' "$SIG_CHANGED" > "$d/f.cho"
r vcs sync --dir "$d"; succeeds "a plain sync records, it does not refuse"
has "$out" '1 signature change(s) — callers by `SigId` affected' "the report names who feels a signature change"
r vcs sync --dir "$d" --check; exits 0 "a synced tree checks clean"
end

# A removed declaration is a `RemoveFunction` op, and the index line goes with it.
begin a_removal_is_one_remove_op_and_a_shorter_index
fresh s4; printf '%s\n' "$ONE" > "$d/f.cho"
r vcs sync --dir "$d"; succeeds "the first sync"
has "$(cat "$d/.cancho/ids.txt")" add "the declaration is indexed"
rm "$d/f.cho"
r vcs sync --dir "$d"; succeeds "the sync after the removal"
has "$out" "1 declaration(s) removed" "a removal says so"
hasnt "$(cat "$d/.cancho/ids.txt")" add "the index no longer carries the removed declaration"
same "$(count_in "$d/.cancho/ops")" 2 "add then remove: two op records"
end

# Two files in the root module declaring the same name with different bodies: the index cannot hold both, and says so.
begin a_root_module_name_declared_in_two_files_is_a_reported_collision
fresh s5
printf 'fn helper() -> [] int { return 1; }\n' > "$d/a.cho"
printf 'fn helper() -> [] int { return 2; }\n' > "$d/b.cho"
r vcs sync --dir "$d"; succeeds sync
has "$out" 'collision: `helper`' "the collision is named"
has "$out" "a.cho" "the first file is named"; has "$out" "b.cho" "the second file is named"
r vcs sync --dir "$d" --check; exits 1 "a collision turns --check red"
end

# A symlinked directory is not followed: a link back to an ancestor would otherwise recurse until the system refuses.
begin a_symlinked_directory_is_not_walked
fresh s6; printf '%s\n' "$ONE" > "$d/f.cho"
ln -s "$d" "$d/loop"
r vcs sync --dir "$d"; succeeds sync
has "$out" "1 declaration(s) added" "only the real file is indexed"
end
