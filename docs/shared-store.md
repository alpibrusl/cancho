# Several threads, one store: the answer, and what it costs

> **Status: design, answering #354.** The question was whether a design
> exists in which N threads each own a shard of an arena and key table
> (share-nothing), connections handed to a shard by key hash, and
> requests/replies exchanged through a channel of ints naming slots in
> lent buffers — or whether the honest answer is "not before T-P3 and a
> wake-up primitive", in which case cancho-cache stays process-per-core
> and the topic closes.

---

## 1. The answer

**Yes, the design exists — but it is built on two primitives that do not
exist yet, and both of them are the load-bearing kind.** The shard
architecture itself needs nothing new: it is the channel-and-int design
`atomics.md` already sketches (§9.1), plus per-thread arenas, which
`threads.md` already gives (a payload's regions belong to its thread).
What does not exist:

1. **A safe lend of a buffer to a running thread.** `thread-payloads.md`
   §4 names it T-P3: today a unique slice crosses to a thread only as
   the whole borrow, once, at spawn. The shard design needs to hand a
   *request buffer* to a shard's thread while both run, and get it (or
   its reply) back. That is a transfer of ownership between threads,
   and the checker's occurs-check has no rule for it: a reference into
   a lent buffer that outlives the lend is `reference-escapes-region`
   today *within* one thread, and nothing is known about the same
   question across two. This is not a gap the design can route around —
   the alternative (copying every request into channel-owned memory)
   is a copy per request on the hot path, which is the throughput the
   design exists to protect.
2. **A poller wake-up from another thread.** A shard sits in
   `poller_wait`; a request arriving on its channel must wake it.
   `atomics.md` §9.1 lists exactly this as unsolved (a wake-up today
   comes from a descriptor, and a channel is memory).

Without both, the honest answer to cancho-cache is the issue's first
option: **process-per-core** (`SO_REUSEPORT` or slot ownership), which
the epic already says it does not depend on this issue for. The design
below is therefore *recorded*, not proposed for building — it is the
shape the two primitives would unlock, so the next reader can see what
T-P3 and the wake-up buy.

## 2. The design, for the record

```
N shards, one per thread (spawn/join), each owning:
    its own region stack (threads.md: a payload's regions are the
                             thread's; no arena is shared)
    its own key table — a shard is chosen by crc16(key) mod N
                             (std.crc, #352: the cluster-slot hash, the
                             same function Redis Cluster uses to pick
                             the shard that owns a key)
    one reader of its channel slot and its poller set

the acceptor (the main thread) owns the listener:
    accept -> crc16(id) mod N -> "slot k, buffer j" posted to shard k
    a request is a *slot number and a buffer number*, both ints —
    the channel atomics.md designs carries ints, and that is enough,
    because the buffer is what T-P3 would lend, not copy
```

The store's own data structures need nothing new: cancho-cache's table
is per-shard, written by exactly one thread, and the linear checker
already holds within one thread. The hard parts are exactly the two
primitives, and they are general — T-P3 is also the gateway to any
worker pool, and the wake-up to any event loop fed from another thread.

## 3. What each primitive costs, so the decision can be priced

* **T-P3 (lending across threads)** is a checker rule first and a
  runtime mechanism second. The rule: a reference into a lent buffer is
  valid on the lending thread only between `lend` and `reclaim`, and on
  the borrowing thread only between arrival and return — a
  happens-before pair the type system must express as a new region
  relationship (`where`-style), not infer. The runtime is a queue of
  buffer descriptors; the memory does not move. The cost is one new
  region rule in the linearity checker and its refusal fixtures; the
  risk is that the rule is checkable *locally* (each side sees only its
  own half), or the lend has to be a whole-slice transfer, which is
  the copy the design wanted to avoid.
* **The wake-up** is a runtime primitive (an eventfd/pipe written by
  the channel's send, added to the target's poller), no language change
  at all — the poller API already takes descriptors, it just needs one
  more kind. Small, but only meaningful *with* T-P3, since without
  lending the wake-up has nothing to hand over but a copy.

That asymmetry is the sequencing argument: T-P3 first, wake-up
immediately after, both before any of §2 is attempted.

## 4. The recommendation

cancho-cache stays **process-per-core** for the epic and this issue
closes as answered: the missing pieces are named (T-P3's cross-thread
lend, the poller wake-up), and the shard design that would use them is
recorded in §2 so it is not re-derived. The two primitives are filed as
their own asks if and when a program needs them — the same bar this
repository holds every feature to, and nothing in cancho-cache's epic
clears it today.
