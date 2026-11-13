# The builtin tables, owned by `examples/selfhost`

> **Status: built** (cancho#487, third box). The three builtin tables of the compiler in cancho are checked-in data that `scripts/builtin_tables.py` checks and extends, and
> `runtime.cho` is made from the `.ll` files by `scripts/embed_runtime.py`. Neither script needs Rust. The first builtin added this way is `dir_nlink`
> ([`directory-nlink.md`](directory-nlink.md)).

## 1. What changed

`builtins.cho`, `signatures.cho` and `tables.cho` were written by `crates/cancho-ir/src/tests/selfhost_tables.rs` from the Rust IR (`Builtin::ALL`, `Builtin::signature`,
`Builtin::effects`, `prelude_types`, `Rule::ALL`), and the same test failed when a file differed. So a builtin could only start in Rust. `runtime.cho` was made by the same
test from `runtime.ll` and `runtime/*.ll`. Now:

- the three tables are the source of truth. Their headers say so. The generator test is `#[ignore]`d (it would overwrite a builtin that only the compiler in cancho has) and
  goes with the crate (`removing-rust.md` phase 5);
- `scripts/embed_runtime.py` is that test's `render_runtime`, in Python: on the inputs of the day it wrote the same bytes (all of `runtime.cho` but its three header lines);
- `tests/builtins.golden` is the list of every builtin, in order. `rules_and_builtins_are_the_goldens` now requires only that it **begins with** the Rust crates' list, so a builtin
  that only the compiler in cancho has can follow.

## 2. The format, and why a script and not a generator

Every per-builtin array is indexed by the builtin's position (`Builtin::ALL` order): `builtin_starts`, `builtin_lens`, `builtin_sinces` (`tables.cho`);
`builtin_region_counts`, `builtin_effect_starts`, `builtin_effect_counts` (`builtins.cho`); `builtin_sig_starts` (`signatures.cho`); and two streams the starts index into,
`builtin_effect_stream` and `builtin_sigs`. The names are one string, `builtin_chars`. Inserting a builtin in the middle would renumber everything after it, so the script **appends**.

    scripts/builtin_tables.py check
    scripts/builtin_tables.py show dir_mode
    scripts/builtin_tables.py append NAME --like OTHER [--since N]
    scripts/builtin_tables.py append NAME --since N --regions R --effects "dir_read" --sig "0 2 10 0 13 31 0 10 1 12 2 13 26 0"

`check` verifies: the count agrees with every per-builtin array; the names string and the starts and lengths agree; no two builtins share a name; `tests/builtins.golden` is the
names in order; the signature stream decodes builtin by builtin with no gap; the effect stream is the sum of the counts; no effect refers past `effect_count`.
`show` decodes a signature (`(&r0 Dir, &r1 [byte]) -> Done`) so a person can read what they are adding. A signature is the prefix code the header of the script documents.

## 3. Adding a builtin

1. `scripts/builtin_tables.py append NAME …` (the three tables and the golden).
2. The code the compiler writes for it: a number in `codes.cho` (`lc.spells`), and its handling in `netcode.cho` (arguments, result, which runtime function).
3. The runtime function, in `runtime/*.c`; `runtime/make.sh` makes the `.ll` files (it also rewrites `net_*.ll` with this machine's clang: restore those if the diff is only noise);
   per-target constants go in `platform.cho` (appended to the end of `cancho_plat`, and to the `enum` in `dirs.c`).
4. `scripts/embed_runtime.py`, so `runtime.cho` follows the `.ll` files.
5. A command test in `tests/commands` and its probe in `tests/probes` (not `tests/programs`: the Rust parity tests walk that and cannot parse the builtin), a reject fixture for the edition gate
   (one the Rust front end refuses with the same rule) and its lines in `tests/reject.golden` and, while the message differs from the Rust wording, `selfhost/message_gaps.txt`.
6. `scripts/build-standalone.sh`: the compiler must reproduce itself (stage 2 equals stage 3).

## 4. What this does not do

- The prelude types, the rules and their explanations (also in `tables.cho`) are hand-owned too now, with no generator: a rule added in Rust is a change to the cancho file as well
  until the Rust front end goes.
- The Rust compiler does not know a builtin added this way. `cancho check` and `cancho authority` from a Rust build refuse it; the compiler in cancho (`scripts/build-standalone.sh`)
  does not. That is the direction of `removing-rust.md`; it is also why a second reader of the tables would need the same script.
- Builtins are only appended. A reordering is a different, larger change.
