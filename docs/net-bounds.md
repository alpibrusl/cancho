# Net bounds: separate ways in and out, set-valued, exact host matching

> **Status: design, answering #362. Steps 1 (the report, §2.4), 2 (several children from one `narrow`, §2.2) and 3A (exact hosts, §2.3, edition 10) of §3 are built; 3B (the `out`/`ports` split, §2.1, edition 11) is built too; §3 below says how.** The measurements
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
   half stays a prefix until step 3A makes it exact (edition 10). Rust lowering in `crates/cancho-ir/src/lower/narrow.rs`, the compiler in cancho
   in `examples/selfhost/narrowing.cho`; fixtures `tests/accept/narrow_many_net.cho`,
   `tests/accept/narrow_many_net_ports.cho`, `tests/reject/narrow_many_net_{nested,equal}_bounds.cho`, `tests/reject/narrow_net_widens_port.cho`; `tests/reject/narrow_many_on_net.cho` (which asserted the
   refusal this step removes) is deleted.
3. **§2.3, then §2.1**, in editions. Edition 7 and 9 were closed or taken (`editions.md` §6.4), so the number is **10**.
   * **3A, exact hosts. Built, edition 10.** `Split` is unchanged (ten fields), and nothing but the host half moves: an
     empty host half still admits every name (the unnarrowed root, and `:9`); `*.suffix` admits exactly one more label
     (`x.suffix`, not `x.y.suffix`, not `suffix`); every other host admits only itself. The edition travels as a flag on
     the nodes (`Expr::Connect`/`TcpConnect`'s `exact_host`), both Rust emitters (`cancho-codegen`, `cancho-codegen-llvm`) check it, and the compiler in
     cancho marks an edition-10 dial bound with a leading byte 1 that `examples/selfhost/runtime/net.c` reads (no
     bound begins with it; the runtime was regenerated with `runtime/make.sh`, and `common` kept on the two `lexs_*`
     globals clang 18 would turn into plain ones). Narrowing follows: from edition 10 a `Net` child is narrower than its
     parent by `net_contains` with exact hosts, so `127.0.0.1:9` cannot narrow to `127.0.0.10:9` or to `*.0.0.1:9`,
     `*.0.0.1:9` narrows to `127.0.0.1:9` but not to `*.127.0.0.1:9`, and the pairwise rule of the several form uses
     the same relation (`127.0.0.1` and `127.0.0.10` are now disjoint siblings; `*.0.0.1` and `127.0.0.1` are nested).
     Earlier editions keep the prefix, so no existing file changes meaning. Measured on both backends with IP-literal
     names (so no resolver is asked; `*.0.0.1` is the one-label wildcard that `127.0.0.1` matches):
     `crates/cancho/tests/conformance/net_hosts.rs`; fixtures `tests/accept/narrow_net_hosts_edition_ten.cho`,
     `tests/accept/narrow_many_net_exact_hosts.cho`, `tests/reject/narrow_net_{exact_host_widens,wildcard_from_exact,wildcard_two_labels}.cho`,
     `tests/reject/narrow_many_net_wildcard_nested.cho`.
   * **3B, the `out`/`ports` split: edition 11.** Decisions, before the code:
     * **Two new prelude types, `Out("bound")` and `Ports("bound")`,** each a leaf-free `res` capability indexed by its
       bound as `Net` is, and **an eleventh-field `Split`** for edition 11: `io, ffi, fs, heap, args, out, ports, clock,
       signals, exec, tty` (`net` is replaced by the two, in its place). Appended to the prelude after `TtyOpened`, so no
       earlier index moves. An edition-11 file's `split()` answers it; earlier editions' are unchanged.
     * **The dial verbs (`connect`, `tcp_connect`, `tcp_connect_start`, `udp_connect`) take a borrowed `Out`; the listen
       verbs (`bind`, `tcp_listen`, `tcp_listen_on`, `udp_bind`) take a borrowed `Ports`.** A borrowed `Net` is still
       accepted by both families, so a package written before edition 11 and handed a `Net` keeps working, and nothing
       in an edition-11 file can *make* a `Net`. The labels are the ones they always were, `net_out(bound)` for a dial and
       `net_in(bound)` for a listener: the report does not change shape, only what is held.
     * **What owning each discharges.** `Out(b)`: `net_out(b)` and the path-free handle labels a dialled socket carries
       (`conn_read`, `conn_write`, `udp_send`, `udp_recv`, `poll`). `Ports(b)`: `net_in(b)` and those a listener carries
       (`conn_accept`, `conn_read`, `conn_write`, `udp_recv`, `udp_send`, `poll`). Neither discharges the other's verb
       label, so a program holding only `Ports` cannot dial and one holding only `Out` cannot listen, which is the point.
     * **`narrow`** takes either, one literal or several, with the edition-10 containment (exact host, port exact) and the
       same refusals. `release` consumes either. A thread can own either (`spawn`).
     * **Not in it:** an `Out` that dials and a `Ports` that listens do not share a bound, so a program that wants one host
       for both says it twice; an inbound host list (`§4`) stays out.
     **Built.** Fixtures: `tests/accept/split_out_ports.cho` (a resolver-shaped program holds both and the report lists `net_out`
     for the upstream and `net_in` for the port), `tests/reject/{dial_through_ports,listen_through_out,split_net_is_edition_ten,narrow_ports_widens}.cho`;
     `crates/cancho/tests/conformance/net_split.rs` (the report, and each half's bound enforced on both backends). The compiler in
     cancho mirrors it (`narrowing.cho`: `is_net`, `dial_refused`, `listen_refused` taking `Ports`; `builtins.split_for`; the
     prelude and discharge tables are generated).

Each step leaves cancho-dns unblocked further, and step 1 alone
corrects what its design document measured.

## 4. What is deliberately not here

* **No `connect` to an address at runtime.** The bound stays
  compile-time; `connect.md` §1's question is untouched.
* **No inbound host bounds.** A listener's authority is a port on
  every interface; naming interfaces is a separate ask nobody has made.
* **No DNS resolution in the checker**, for §2.3's reason.
