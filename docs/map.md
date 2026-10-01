# `std.map`: a hash map from byte strings to copyable values

> **Status: built.** `std/map.ls`; tests `tests/lex/map_test.ls` (5, run by
> `lex-sys test` on both backends); drivers `tests/programs/map_bench.ls`
> and `map_lookup_bench.ls`. Insert, overwrite, lookup, remove, and
> iteration in insertion order. §5 has the numbers, §7 the compiler bug a
> benchmark found.

## 1. Why

`json.md` §8 listed it: an object with forty members is walked linearly by
`json.get`, which is what a flat tape costs and is fine until it is not.
The same shape comes up everywhere a program keys on text it was handed --
an HTTP route table, a header set, a symbol table, a cache -- and until now
a program wrote a `Vec` of pairs and scanned it.

## 2. The decision: a key is bytes

A map generic over its key needs a hash and an equality for that key. This
language has no traits, so the choice was between passing both as function
values on every call (`function-values.md`), and fixing the key type.
Everything keyed on today is bytes: member names, headers, routes, file
names, identifiers. So **the key is `&[byte]`**, and the value is any `val`
type, the same bound `std.vec` has and for the same two reasons
(`collections.md` §2).

The map **copies the key** into storage of its own on `put`, so the slice a
caller passes need not outlive the call. `key_at` hands back a view into
that storage, valid until the map is next written to.

A map keyed on integers is not this module. It would be a different hash
(an integer needs no byte loop) and no copy; nothing has asked for it, and
`map.put(h, m, key_bytes_of(n), v)` works in the meantime.

## 3. The structure

```
slots   [0 | -1 | entry+1 ...]   power-of-two length, at most half full
meta    [start, length, hash, live] per entry
vals    [V] per entry
keys    one Buffer, every key's bytes back to back
```

Open addressing, linear probing, and the entries themselves in the order
they were first put. Three consequences are the interface:

* **Iteration is deterministic.** `0..entries(m)` skipping `!is_live` is the
  insertion order, whatever the hash did, so a program that prints a map
  prints the same bytes on every run and every machine. A key that is
  overwritten keeps its place; a key removed and put again goes to the end.
* **Removal marks, it does not move.** `remove` makes the entry dead and its
  slot a tombstone (`-1`). Entry numbers held by a caller stay valid until
  the next `put` that finds the entry arrays full, which compacts: live
  entries are copied in order into arrays sized for twice their number and
  the dead ones' key bytes are dropped.
* **The table is never more than half full**, tombstones included. It is
  sized from the entry capacity and every tombstone is an entry not yet
  compacted away, so a probe always reaches an empty slot and the lookup
  loop needs no bound.

`put` and `remove` have the shapes `std.vec` gave `push` and `pop`: `put`
takes the map by value and returns it because growth replaces the storage;
`remove` takes it by unique reference because nothing is replaced.

## 4. The hash

FNV-1a over the bytes, seeded, then murmur3's `fmix64` finalizer. FNV-1a is
one xor and one multiply a byte and is not strong; the finalizer is what
makes the low bits -- the ones that pick a slot -- depend on every byte, so
sequential keys (`key-0`, `key-1`, ...) do not cluster. The 64-bit offset
basis has its top bit set and does not parse as a literal (`sha512.md`), so
it is built from halves; `wrapping_mul` is the multiply and `shr33` masks
the arithmetic shift into a logical one.

**It is not a defence against a hostile key set.** An attacker who knows
the seed -- or the hash, with a seed of zero -- can choose keys that all
land in one probe chain and make a lookup linear. If keys arrive from
outside, pass a per-process seed from a source of randomness. That raises
the cost of the attack from "send keys" to "learn the seed", which is what a
seeded non-cryptographic hash can do and what Python and Rust's defaults
also stop at (theirs are stronger hashes; the shape of the defence is the
same). A keyed hash such as SipHash is the upgrade if a caller needs more,
and it is a change to `hash` alone.

## 5. Evidence

**Correctness**

| Check | Result |
|---|---|
| put / overwrite / get / missing / has | pass, both backends |
| remove, remove of an absent key, put again after remove | pass |
| the empty key | pass |
| insertion order across growth and a compaction that drops two removed entries | pass |
| **30,000 random puts, removes and gets over 300 keys against a plain array** | every `get`, every `remove` answer, and the final size agree |
| mutation: `remove` no longer decrements the count | 3 of the 5 tests fail |

The last row is how the random test was checked to be able to fail at all.
The starting capacity is 4, so the run grows the table many times and
compacts it with tombstones in the probe chains.

**Speed.** One shared, noisy machine; each figure is the spread of three
runs, taken as a difference between a short and a long run so the build of
the table cancels.

| Lookup, 12-byte keys | lex-sys | Rust `HashMap<Vec<u8>, _>` | Python `dict` |
|---|---|---|---|
| 100,000 keys (in cache) | 51-72 ns | 52-81 ns | ~190 ns |
| 2,000,000 keys (past cache) | 171-196 ns | 208-251 ns | ~455 ns |

The honest reading is **the same ballpark as Rust's standard map**, not
faster. Rust's default hasher is SipHash, built for the hostile-key case
§4 declines, so a Rust map with a cheap hasher would be quicker; and its keys
are separate allocations where these are one contiguous buffer, which is a
layout difference as much as a table one. The Python column is one run.

Put 1M keys, get 1M, remove 500k, get 1M (`map_bench.ls`), building every
key as a string as it goes: lex-sys 1.2-1.4 s, Rust 1.6-2.2 s, Python
2.4 s. That figure is mostly key construction and allocation, which all
three do.

## 6. What it does not do

| | |
|---|---|
| Integer or other keys | §2. A different hash and no copy; nothing has asked |
| Values that are resources | `V: val`, the `std.vec` bound. A map holding a `res` value is the `std.list` shape, and no collection of that shape exists yet |
| Shrinking | The arrays shrink only on a compaction, which happens only on a `put` that fills them. A map that grew to a million and was then emptied keeps its storage until the next growth |
| A hostile-key defence | §4 |
| Iteration while writing | `key_at` is a view into storage a `put` may replace; the checker enforces it, because the view borrows the map |

## 7. Found along the way: the LLVM backend grew the stack inside loops

The first benchmark died with SIGSEGV at 30,000 keys. Cranelift ran it
correctly; so did the LLVM backend at 20,000.

`gdb` showed `lexs_run`'s frame pointer near the top of the stack and its
stack pointer exactly 8 MiB lower. The disassembly had
`lea -0x70(%rax),%rdi; mov %rdi,%rsp` inside the loop: a **dynamic
`alloca`**. The backend emitted an `alloca` where it was used for the
buffer every `borrow` writes its referent into (a `Map` is nine words, so
0x70 bytes) and for the one-byte temporary every `&&` and `||` merges
through. An `alloca` outside the entry block takes stack each time it runs
and returns it only when the function does, and `mem2reg` promotes only
entry-block ones. About 30,000 iterations was the 8 MiB.

Every `alloca` outside the slot prologue is now collected and spliced into
the entry block (`FuncEmitter::hoist`); the cell is the same one each time
the code runs, which is all any of these uses wanted. A `region` inside a
loop is covered too: its base and bump cells were the same pattern.

**Why nothing caught it.** The conformance suite's loops are short, and
where they were long the reference never reached a function the optimiser
could not see through, so the `alloca` was deleted as dead. The regression
test (`a_loop_body_does_not_grow_the_stack_on_either_backend`) took three
attempts for exactly that reason: the first two passed on the broken
compiler. The version that fails there passes the borrowed value to a
non-tail recursive function with a depth known only at run time, so the
reference escapes, and runs 20 million iterations of a `borrow` and a
`&&` / `||`. It was run against the unfixed backend and fails with SIGSEGV
there.

That is the second bug in a row that a library found and a compiler test
did not (`json.md` §6), and the pattern is the same: the compiler was
tested on programs written to test the compiler.

## 8. Open

| Question | Why it waits |
|---|---|
| A router over this | The next piece: a path is bytes, the `:id` segments are a parse, the table is this map plus a segment list |
| Integer keys | §2 |
| A keyed hash for hostile input | §4 |
| Wiring it into `std.json` | `json.get` is a linear scan and is right for the objects it sees; an `index_object` that builds a map is a caller's few lines, and nothing yet has a document wide enough to justify owning it |
