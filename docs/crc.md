# std.crc: the checksums, and the 64-bit ones that needed a design

> **Status: built.** `std.crc` ships CRC-16/XMODEM, CRC-32/IEEE,
> CRC-64/XZ and CRC-64/REDIS, each with the incremental `start`/`update`/`finish` a
> snapshot writer needs and a one-call `of`, gated against the standard
> catalogue's published check values for `"123456789"`
> (`tests/accept/crc_check_values.cho`).

---

## 1. Why

Two askers cleared `CONTRIBUTING.md`'s bar (#352): cancho-cache's
cluster-lite epic needs **CRC-16/XMODEM** (`crc16(key) mod 16384` is a
Redis Cluster hash slot, and a cluster-aware client must agree with the
server byte for byte), and a snapshot/replication format needs
**CRC-64** (Redis's, so an RDB or a payload checks byte for byte;
`std.crc` also carries the catalogue's CRC-64/XZ); cancho-log's private `crc.cho` (a CRC-32C — a different
polynomial, which stays where it is) is the second asker for the
module. `std.crc` adds the **CRC-32/IEEE** a record check wants.

## 2. The design decision: a checksum's width is a mask — until it isn't

`int` is 64-bit and **signed**, and `>>` is arithmetic
(`docs/bitwise.md` §2). A reflected CRC keeps its state in the register
and shifts it right every byte, so a state whose top bit is set would
sign-extend under `>>` and read back wrong.

* **CRC-16 and CRC-32** stay under 2^31 by masking after every shift
  (`& 0xffff`, `& 0xffffffff`): their `>>` is logical for free.
* **CRC-64 cannot.** Its state genuinely uses the top bit, and the mask
  `0xffffffffffffffff` is not a literal a 64-bit signed language can
  write — `i64::MAX` is the limit, `1 << 64` traps, and the all-ones
  word is negative.

The answer is not an emulation of unsigned shifts (a logical-shift
helper tried against the reference produced wrong values before the
real cause was found). The answer is **splitting the state into two
32-bit halves**, `hi` and `lo`, each always in `[0, 2^32)`: no
intermediate reaches a sign bit, every `>>` is logical by construction,
and no 64-bit mask is ever needed. The update, derived from the
one-word form `crc = table[(crc ^ b) & 0xff] ^ (crc >> 8)`:

```
idx = (lo ^ b) & 0xff                    the byte meets the low half
shr = (lo >> 8) | (hi << 24)             the logical `crc >> 8`, under 2^56
lo  = table_lo[idx] ^ (shr & 0xffffffff)
hi  = table_hi[idx] ^ (shr >> 32)
```

The table is stored the same way — two 256-entry statics, built in
halves through all eight steps of each entry (the shift moves a bit
from `hi` into `lo`'s top; the halves xor their own half of the
polynomial, `0xd7870f42` below and `0xc96c5795` above, for XZ). Splitting is
the design, not an optimisation: the one-word table cannot be written
in this language at all.

## 3. Which CRC-64: two, and an earlier claim here was false

The filing issue (#352) cited poly `0xad93d23594c935a9` with check
value `0xE9C6D914C4B8D9CA` for `"123456789"`. An earlier version of this
section said that value "matches no init/xorout combination" of that
polynomial, and that Redis's `crc64` is the XZ polynomial. **Both were
wrong, and the error is corrected here, where it was made.** Computed
independently (a bitwise reflected CRC in Python, using the reflected
polynomial `0x95ac9329ac4bc9b5`):

| Variant | Polynomial (normal / reflected) | init | xorout | `"123456789"` |
|---|---|---|---|---|
| CRC-64/REDIS ("Jones") | `0xad93d23594c935a9` / `0x95ac9329ac4bc9b5` | 0 | 0 | `0xE9C6D914C4B8D9CA` |
| CRC-64/XZ (ECMA-182) | `0x42f0e1eba9ea3693` / `0xc96c5795d7870f42` | all ones | all ones | `0x995DC9BBDF1939FA` |

The first row is the value in Redis's own `crc64.c` test
(`crc64(0, "123456789", 9) == 0xe9c6d914c4b8d9ca`), so the issue's pair
was right all along; the earlier check had tried the wrong polynomial.
The "Jones" label belongs to the Redis polynomial, **not** to XZ, which
is the ECMA-182 polynomial. `std.crc` ships both (`crc64_*` is XZ,
`crc64_redis_*` is Redis), over one machinery: they differ in the
polynomial halves of the table and in the two constants. Both are gated
by `tests/accept/crc_check_values.cho`, one-call and incremental.

## 4. The interface

```cho
crc16_start() -> [] int                // 0
crc16_update[&b](state, data) -> [] int
crc16_finish(state) -> [] int
crc16_of[&b](data) -> [] int          // 0x31C3 for "123456789"

crc32_start() -> [] int                // all ones
crc32_update[&b](state, data) -> [] int
crc32_finish(state) -> [] int
crc32_of[&b](data) -> [] int          // 0xCBF43926

crc64_start() -> [] (int, int)         // two all-ones halves
crc64_update[&b](state, data) -> [] (int, int)
crc64_finish(state) -> [] (int, int)   // each half complemented
crc64_of[&b](data) -> [] (int, int)   // (0x995DC9BB, 0xDF1939FA): CRC-64/XZ

crc64_redis_start() -> [] (int, int)   // (0, 0)
crc64_redis_update[&b](state, data) -> [] (int, int)
crc64_redis_finish(state) -> [] (int, int)   // the state: no final xor
crc64_redis_of[&b](data) -> [] (int, int)    // (0xE9C6D914, 0xC4B8D9CA): Redis's
```

CRC-64's state and answer are the `(hi, lo)` pair rather than one
number, for the reason §2 gives: the one-number form is not writable. A
caller comparing answers compares the halves; one that must combine
them (`hi * 4294967296 + lo`) is outside the checked range and should
not be — the halves *are* the value.

CRC-16 is deliberately table-free: sixteen bits make the byte loop
eight conditional shifts, cheaper than 512 bytes of table, and it is
the exact loop Redis's own `crc16` reference implements.

## 5. The gate

`tests/accept/crc_check_values.cho` checks the one-call forms against
the catalogue, and the incremental forms two ways: one byte at a time,
and CRC-64 split across two updates of different lengths — the
snapshot case, where a record is checksummed in slices between turns of
an event loop. All eight answers (four checksums, each one-call and incremental) must agree with the published values. The issue's suggested
differential against a live Redis (`CLUSTER KEYSLOT`, `redis-check-rdb`)
is noted as a follow-up where Redis is available; the catalogue's
published vectors are the gate here, and they are checked, not assumed.
