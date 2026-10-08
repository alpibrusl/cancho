# Stability exemptions

`docs/hash-stability.md` §7's plateau rule, as a checked property: every
historical revision of every `.cho` file under `std/` and `examples/`
must type-check under today's compiler, **or** its failing rule appears
here with a reason. `scripts/history.py --explain` enforces it — an
unlisted failing rule is a red CI run, and a listed rule that stops
failing is reported as stale so the list only ever shrinks.

The failing total stays honest either way: a rule exempted here still
counts toward "do not read"; what the exemption buys is that every
entry of that number has a line in the repository saying why.

Baseline at introduction: **203 revisions, 133 do not read** (65%).
The three rules below account for all 133.

| rule | reason |
|---|---|
| duplicate-declaration | 98 revisions: pre-module-era files that declared the same helper (`sha1`, `hex`) as a sibling; under today's one-module-per-file rule the harness lays them beside each other and the names collide. The files were legal when written; the fix is per-revision (`as` renames), not a language change |
| unknown-name | 34 revisions: examples importing module paths that later moved or were folded into packages (`net.sockets`, `net.connect`, `http.server`, `http.request`). Their imports were legal at their revision; the harness checks them beside today's `--std`, where those modules no longer exist under those names |
| not-a-function | 1 revision: a pre-enum-era file using a name that is a value, not a call, today |
