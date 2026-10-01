module std.map;

import std.buffer;
import std.bytes;

// `std.map` — a hash map from byte strings to copyable values.
//
// `docs/map.md` is the design. The decision that shapes everything here
// is the key: **a key is a run of bytes.** This language has no traits,
// so a map generic over its key would need a hash and an equality passed
// in as function values on every call, and nothing a program keys on
// today is anything but bytes -- a JSON object's member names, an HTTP
// header, a route, a file name, an identifier. So the key is `&[byte]`,
// the map copies it into its own storage on `put` (a caller's slice
// need not outlive the call), and the value is any `val` type, the same
// bound and the same two reasons `std.vec` gives for it.
//
// What it is: open addressing with linear probing over a table of
// entry numbers, and the entries themselves in insertion order in three
// parallel runs. Two things follow that a program can rely on:
//
//   * **iteration is deterministic** -- entries come back in the order
//     they were first put, whatever the hash did, so the same program
//     prints the same bytes on every run and every machine; and
//   * **removal leaves a hole**, not a shifted run: an entry is marked
//     dead and its table slot becomes a tombstone, and the next growth
//     compacts both away.
//
// The table never holds more than half-full counting tombstones, because
// it is sized from the entry capacity and every tombstone is an entry
// that has not yet been compacted.
//
// The hash is FNV-1a over the bytes, seeded, with a murmur3 finalizer so
// the low bits that pick a slot depend on every byte. It is **not** a
// defence against a hostile key set: pick the seed per process if keys
// come from outside, and §6 of the design says exactly what that does
// and does not buy.

pub res struct Map[V: val] {
    // `0` is empty, `-1` is a tombstone, anything else is an entry
    // number plus one. Its length is a power of two.
    slots: Box[[int]],
    // Four ints an entry: where its key starts in `keys`, how long it
    // is, its full hash, and whether it is alive (`1`) or removed (`0`).
    meta: Box[[int]],
    vals: Box[[V]],
    // Every key's bytes, back to back.
    keys: buffer.Buffer,
    // Entries ever appended since the last compaction, dead ones
    // included; the next entry's number.
    entries: int,
    live: int,
    seed: int,
    fill: V,
}

// ---------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------

// The 64-bit right shift by 33 with zeros shifted in. `>>` is
// arithmetic here (`docs/bitwise.md` §2), so the sign copies are masked
// off.
fn shr33(x: int) -> [] int {
    return x >> 33 & 0x7fffffff;
}

// A 64-bit hash of `key` under `seed`.
//
// FNV-1a: xor a byte in, multiply by the 64-bit prime. The offset
// basis, `0xcbf29ce484222325`, has its top bit set, which does not
// parse as one literal (`docs/sha512.md`), so it is built from halves.
// The finalizer is murmur3's `fmix64`.
pub fn hash[&k](key: &k [byte], seed: int) -> [] int {
    var h = (0xcbf29ce4 << 32 | 0x84222325) ^ seed;
    var i = 0;
    while i < len(key) {
        h = wrapping_mul(h ^ int_of(key[i]), 1099511628211);
        i = i + 1;
    }
    h = h ^ shr33(h);
    h = wrapping_mul(h, 0xff51afd7 << 32 | 0xed558ccd);
    h = h ^ shr33(h);
    h = wrapping_mul(h, 0xc4ceb9fe << 32 | 0x1a85ec53);
    h = h ^ shr33(h);
    return h;
}

// ---------------------------------------------------------------------
// Making and ending one
// ---------------------------------------------------------------------

// The table length for `entry_cap` entries: the least power of two, and
// at least 8, that is twice as large -- so it is never more than half
// full.
fn slot_count(entry_cap: int) -> [] int {
    var n = 8;
    while n < entry_cap * 2 {
        n = n * 2;
    }
    return n;
}

// A map with room for `capacity` entries before it first grows. `fill`
// is what the unused value room holds (there is no uninitialised memory
// here, `std.vec`'s reason) and `seed` is mixed into every hash.
pub fn empty[V: val, &h](heap: &!h Heap, capacity: int, fill: V, seed: int) -> [heap] Map[V] {
    var cap = capacity;
    if cap < 4 {
        cap = 4;
    }
    return Map { slots: box_slice(heap, slot_count(cap), 0), meta: box_slice(heap, cap * 4, 0), vals: box_slice(heap, cap, fill), keys: buffer.empty(heap, cap * 16), entries: 0, live: 0, seed: seed, fill: fill };
}

// End it, answering how many entries it held.
pub fn drop[V: val, &h](heap: &!h Heap, m: Map[V]) -> [heap] int {
    let Map { slots, meta, vals, keys, entries, live, seed, fill } = m;
    unbox_slice(heap, slots);
    unbox_slice(heap, meta);
    unbox_slice(heap, vals);
    buffer.drop(heap, keys);
    return live;
}

// How many entries are in it.
pub fn size[V: val, &m](m: &m Map[V]) -> [] int {
    return m.live;
}

// ---------------------------------------------------------------------
// Looking up
// ---------------------------------------------------------------------

// The table position holding `key`, or -1. This is the one probe loop;
// `find`, `remove` and `put` are all this and a little more.
fn find_slot[V: val, &m, &k](m: &m Map[V], key: &k [byte]) -> [] int {
    let h = hash(key, m.seed);
    let slots = contents(m.slots);
    let meta = contents(m.meta);
    let text = buffer.bytes(m.keys);
    let mask = len(slots) - 1;
    var at = h & mask;
    var found = 0 - 1;
    var going = true;
    // The table is at most half full, so an empty slot is always
    // reached and this stops.
    while going {
        let s = slots[at];
        if s == 0 {
            going = false;
        } else if s > 0 {
            let e = s - 1;
            if meta[e * 4 + 2] == h {
                let start = meta[e * 4];
                let n = meta[e * 4 + 1];
                if bytes.equal(text[start..start + n], key) {
                    found = at;
                    going = false;
                }
            }
        }
        at = at + 1 & mask;
    }
    return found;
}

// The entry number of `key`, or -1 if it is not there.
pub fn find[V: val, &m, &k](m: &m Map[V], key: &k [byte]) -> [] int {
    let at = find_slot(m, key);
    if at < 0 {
        return 0 - 1;
    }
    let slots = contents(m.slots);
    return slots[at] - 1;
}

pub fn has[V: val, &m, &k](m: &m Map[V], key: &k [byte]) -> [] bool {
    return find_slot(m, key) >= 0;
}

// The value for `key`, or `missing` if it has none.
pub fn get[V: val, &m, &k](m: &m Map[V], key: &k [byte], missing: V) -> [] V {
    let e = find(m, key);
    if e < 0 {
        return missing;
    }
    let vals = contents(m.vals);
    return vals[e];
}

// ---------------------------------------------------------------------
// Entries, in the order they were put
// ---------------------------------------------------------------------

// How many entry numbers there are: iterate `0..entries(m)` and skip
// the ones `is_live` refuses. Dead entries are only present between a
// `remove` and the next growth.
pub fn entries[V: val, &m](m: &m Map[V]) -> [] int {
    return m.entries;
}

pub fn is_live[V: val, &m](m: &m Map[V], e: int) -> [] bool {
    let meta = contents(m.meta);
    return e >= 0 && e < m.entries && meta[e * 4 + 3] == 1;
}

// The key of entry `e`, as a view into the map's own storage. It is
// valid until the map is next written to.
pub fn key_at[V: val, &m](m: &m Map[V], e: int) -> [] &m [byte] {
    let meta = contents(m.meta);
    let text = buffer.bytes(m.keys);
    let start = meta[e * 4];
    return text[start..start + meta[e * 4 + 1]];
}

pub fn value_at[V: val, &m](m: &m Map[V], e: int) -> [] V {
    let vals = contents(m.vals);
    return vals[e];
}

pub fn set_value_at[V: val, &m](m: &!m Map[V], e: int, value: V) -> [] int {
    let vals = contents(m.vals);
    vals[e] = value;
    return e;
}

// ---------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------

// Put entry number `entry`, whose hash is `h`, in the first empty slot
// along its probe sequence. Tombstones are not reused: they are bounded
// by the entry capacity, and the next compaction removes them.
fn place[&s](slots: &!s [int], h: int, entry: int) -> [] int {
    let mask = len(slots) - 1;
    var at = h & mask;
    while slots[at] != 0 {
        at = at + 1 & mask;
    }
    slots[at] = entry + 1;
    return at;
}

// A copy of `m` with room for twice its live entries (at least 8), dead
// entries and their key bytes dropped, live ones kept in order.
fn rebuild[V: val, &h](heap: &!h Heap, m: Map[V]) -> [heap] Map[V] {
    let Map { slots, meta, vals, keys, entries, live, seed, fill } = m;
    var cap = live * 2;
    if cap < 8 {
        cap = 8;
    }
    var used_bytes = 0;
    borrow keys as &ok in {
        used_bytes = buffer.size(ok);
    }

    let new_slots = box_slice(heap, slot_count(cap), 0);
    let new_meta = box_slice(heap, cap * 4, 0);
    let new_vals = box_slice(heap, cap, fill);
    var new_keys = buffer.empty(heap, used_bytes + 16);
    var count = 0;
    var kept_bytes = 0;
    borrow mut new_slots as &!sw in {
        borrow mut new_meta as &!mw in {
            borrow mut new_vals as &!vw in {
                borrow meta as &om in {
                    borrow vals as &ov in {
                        borrow keys as &ok in {
                            let ns = contents(sw);
                            let nm = contents(mw);
                            let nv = contents(vw);
                            let old_meta = contents(om);
                            let old_vals = contents(ov);
                            let text = buffer.bytes(ok);
                            var e = 0;
                            while e < entries {
                                if old_meta[e * 4 + 3] == 1 {
                                    let start = old_meta[e * 4];
                                    let n = old_meta[e * 4 + 1];
                                    let h = old_meta[e * 4 + 2];
                                    new_keys = buffer.append(heap, new_keys, text[start..start + n]);
                                    nm[count * 4] = kept_bytes;
                                    nm[count * 4 + 1] = n;
                                    nm[count * 4 + 2] = h;
                                    nm[count * 4 + 3] = 1;
                                    nv[count] = old_vals[e];
                                    place(ns, h, count);
                                    kept_bytes = kept_bytes + n;
                                    count = count + 1;
                                }
                                e = e + 1;
                            }
                        }
                    }
                }
            }
        }
    }
    unbox_slice(heap, slots);
    unbox_slice(heap, meta);
    unbox_slice(heap, vals);
    buffer.drop(heap, keys);
    return Map { slots: new_slots, meta: new_meta, vals: new_vals, keys: new_keys, entries: count, live: live, seed: seed, fill: fill };
}

// Set `key` to `value`, adding the entry if it is not there and
// replacing its value if it is. The map comes back because growing
// replaces its storage (`std.vec.push`'s reason); a key already present
// keeps its place in the order.
pub fn put[V: val, &h, &k](heap: &!h Heap, m: Map[V], key: &k [byte], value: V) -> [heap] Map[V] {
    var cur = m;
    var e = 0 - 1;
    borrow cur as &r in {
        e = find(r, key);
    }
    if e >= 0 {
        borrow mut cur as &!w in {
            set_value_at(w, e, value);
        }
        return cur;
    }

    // Room for one more entry: the entry arrays are exactly as long as
    // the capacity, so a full one is the signal to compact and grow.
    var full = false;
    borrow cur as &r in {
        full = r.entries * 4 >= len(contents(r.meta));
    }
    if full {
        cur = rebuild(heap, cur);
    }

    let Map { slots, meta, vals, keys, entries, live, seed, fill } = cur;
    var start = 0;
    borrow keys as &ok in {
        start = buffer.size(ok);
    }
    let h = hash(key, seed);
    let grown = buffer.append(heap, keys, key);
    borrow mut slots as &!sw in {
        borrow mut meta as &!mw in {
            borrow mut vals as &!vw in {
                let nm = contents(mw);
                let nv = contents(vw);
                nm[entries * 4] = start;
                nm[entries * 4 + 1] = len(key);
                nm[entries * 4 + 2] = h;
                nm[entries * 4 + 3] = 1;
                nv[entries] = value;
                place(contents(sw), h, entries);
            }
        }
    }
    return Map { slots: slots, meta: meta, vals: vals, keys: grown, entries: entries + 1, live: live + 1, seed: seed, fill: fill };
}

// Remove `key`, answering whether it was there. The entry is marked dead
// and its table slot becomes a tombstone; nothing moves, so entry
// numbers held by a caller stay valid until the next `put` that grows.
pub fn remove[V: val, &m, &k](m: &!m Map[V], key: &k [byte]) -> [] bool {
    let at = find_slot(m, key);
    if at < 0 {
        return false;
    }
    let slots = contents(m.slots);
    let meta = contents(m.meta);
    let e = slots[at] - 1;
    slots[at] = 0 - 1;
    meta[e * 4 + 3] = 0;
    m.live = m.live - 1;
    return true;
}
