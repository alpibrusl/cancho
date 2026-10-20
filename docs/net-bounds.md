# Net bounds: separate ways in and out, set-valued, exact host matching

> **Status: design, answering #362. Steps 1 (the report, §2.4) and 2 (several children from one `narrow`, §2.2) of §3 are built; step 3 is not.** The measurements
> it rests on are reproduced in the issue and were taken on `main` at
> `0567e72` by cancho-dns's design document (alpibrusl/cancho-dns#19 §2);
> this document is the answer to "what should the language do instead",
> written where the next reader looks.

---

## 1. What is wrong, in one paragraph each

The issue's three findings, restated as design defects:

1. **One capability, one bound, two meanings.** `narrow(net, s)`
   consumes the `Net` (a `res` value), so a program holds at most one
   bound string — but outbound reads it as `host:port` and inbound reads
   it as a bare port. A program that listens *and* dials (a resolver, a
   pooler, a proxy) cannot carry a narrowed `Net` at all; the issue's
   probe traps under every spelling of the bound, and only the
   unnarrowed `Net("")` from `split` works.
2. **The host half is a prefix.** `Net("127.0.0.1:5353")` admits
   `127.0.0.10` and, by the same code, `ns1.example.com` admits
   `ns1.example.com.attacker.net` — while the report prints
   `net_out("127.0.0.1:5353")`, which reads as an exact claim.
3. **The report cannot say "several".** A resolver wants one inbound port
   and several upstreams; the vocabulary has neither the direction
   split at the *capability* level nor a set of bounds, even though
   `narrow(fs, "a", "b", ...)` — several children from one narrow —
   already exists for `Fs` and is the recommended shape there
   (`narrowing-into-several.md`).

## 2. The design

### 2.1 Two capabilities, split at `split`

`Split` gains an outbound and an inbound capability where today's
eighth field is one `net`:

```cho
let Split { io, ffi, fs, heap, args, out, ports, clock } = split(world);
```

`out` carries every dialing verb (`tcp_connect*`, `udp_connect`,
`udp_send_to`); `ports` carries every listening verb (`tcp_listen`,
`udp_bind`, `udp_connect` to a *bound* socket). The names are chosen to
read in the report: `net_out("…")` and `net_in(…)` label rows exactly as
they already do (`docs/net.md` §4's two-labels-for-two-questions), but
the axis is now the capability rather than the verb — which is what
makes a program that does both sayable.

An existing program's `net` field splits mechanically; the edition that
introduces it is the next edition (its number is not fixed here: other proposals claim
edition 8), additive by construction, the
same way every capability has arrived (`docs/editions.md` §7).

### 2.2 Set-valued bounds, by several children from one narrow

The shape `narrowing-into-several.md` already recommends for `Fs`:

```cho
let (primary, secondary) = narrow(out, "ns1.example.com:53", "ns2.example.com:53");
```

Each child is its own capability with its own label, so the report lists
two `net_out` bounds and the run-time check on each `connect` compares
against its own child's bound — no set membership at runtime, just the
per-capability check that exists today. A resolver's authority report
then reads:

```
performs
    net_out("ns1.example.com:53")
    net_out("ns2.example.com:53")
    net_in(5353)
```

which is the sentence cancho-dns wanted to be able to make.

### 2.3 Exact host matching, with an explicit wildcard

A bound matches a destination only by equality of the host half, except
when the host half is written `*.suffix`, which matches exactly one
more label (`*.example.com` matches `api.example.com`, not
`a.b.example.com`). The reasons:

* the prefix rule's defence in `docs/net.md` §4 was borrowed from
  `Fs(prefix)`, where a prefix is the natural shape of a directory tree;
  hostnames are not trees, and `ns1.example.com` admitting
  `ns1.example.com.attacker.net` is the counterexample that settles it;
* IP literals compare as literals — the issue's `127.0.0.1` admitting
  `127.0.0.10` is the same defect one level down;
* DNS names in bounds are compared as written, without resolution: the
  bound is a compile-time claim (`net.md` §4.1's correction), and
  resolving at check time would make the claim depend on the resolver,
  which is the very thing this capability exists to bound.

### 2.4 What the report says about an unnarrowed capability

The issue's closing question: the JSON marks `net_in("")` and
`net_out("")` with `"bounded": true`. It should not. An empty bound is
the *unnarrowed root* — the same state `narrow(fs, "")` is refused for
("narrows to itself, which grants nothing new"). The fix is in the
report, not the vocabulary: `bounded` is true only when at least one
non-empty bound exists for the direction, and the unnarrowed half
answers `"bounded": false` — closing the silence-reads-as-bounded gap
that `foreign-authority.md`'s `unbounded_by` already closed for FFI.
This half is small enough to ship independently of the capability split
and should: it is a report lie, not a vocabulary gap.

## 3. Sequence

1. **§2.4 first** (report only, no language change): `bounded: false`
   for an empty bound. One PR, fixes a lie the report tells today.
   **Built** (`cancho authority`): an unnarrowed `net_out("")` or `net_in("")` makes the top-level `bounded` `false`, the
   label's own `bounded` `false`, and appears in `unbounded_by` as `net_out("")` beside the foreign symbols; the prose
   report opens with `UNBOUNDED: this program holds the network without narrowing it.` The four examples that hold the
   network unnarrowed on purpose (`http_fetch_nb`, `tls_echo`, `tls_echo_fixed`, `https_hello`: the operator names the
   hosts and ports) now pin `bounded: false` with the reason, which is what they always were; gates in
   `tests/conformance/net_bounds.rs`. The compiler in cancho (`examples/selfhost`) has no `authority` report, so there is
   nothing to port.
2. **§2.2** on the existing single `Net`: several children from one
   `narrow`, mirroring the `Fs` shape and its gates.
   **Built.** `narrow(net, "a:53", "b:53")` answers a tuple of `Net`s, one per bound, each its own label in the report
   (`net_out("a:53")`, `net_out("b:53")`) and each checked at run time against its own bound only (measured: dialing
   `:443` through the `:80` child traps on both backends). The pairwise rule is `Net`'s own: equal is refused, and so is one
   bound containing another. Containment is measured, not textual: the bound splits at its last `:`, the host is a
   **prefix** match and the port is **exact** (a bound with no `:` checks no port), so `127.0.0.1:80` contains
   `127.0.0.10:80` but not `127.0.0.1:8080`, which is accepted as a disjoint sibling. The same containment decides a single
   `narrow`: narrowing `127.0.0.1:80` to `127.0.0.1:8080` was accepted before this fix and widened the port; it is now
   `capability-not-narrowable`. (The first version of this step used whole-string prefix and was wrong both ways.) The host
   half stays a prefix until step 3 makes it exact. Rust lowering in `crates/cancho-ir/src/lower/narrow.rs`, the compiler in cancho
   in `examples/selfhost/narrowing.cho`; fixtures `tests/accept/narrow_many_net.cho`,
   `tests/accept/narrow_many_net_ports.cho`, `tests/reject/narrow_many_net_{nested,equal}_bounds.cho`, `tests/reject/narrow_net_widens_port.cho`; `tests/reject/narrow_many_on_net.cho` (which asserted the
   refusal this step removes) is deleted.
3. **§2.1 + §2.3** together in the next edition: the direction split at
   `split` and exact host matching — both breaking for a narrowed
   `net`, both additive under an edition marker.

Each step leaves cancho-dns unblocked further, and step 1 alone
corrects what its design document measured.

## 4. What is deliberately not here

* **No `connect` to an address at runtime.** The bound stays
  compile-time; `connect.md` §1's question is untouched.
* **No inbound host bounds.** A listener's authority is a port on
  every interface; naming interfaces is a separate ask nobody has made.
* **No DNS resolution in the checker**, for §2.3's reason.
