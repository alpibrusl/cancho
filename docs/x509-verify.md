# `packages/x509`: verifying a server's chain

> **Status: built (#206, all three PRs): the verifier (§8), and `packages/tls` verifying with it (§9). §10, a chain
> verified without a host name (for `verify-ca` and client certificates), is designed there and built in the same PR.** Sub-issue 9 of the self-contained TLS 1.3 client (#197). `docs/tls-pure.md` §5 already
> fixes the rules: the root store, the depth limit, the checks per certificate, the key sizes, name matching, and what is not
> checked. Its §8 fixes the refusal tags. This document settles what those sections left open for the code:
> - where the code goes, and its API;
> - how a chain is built when there is more than one candidate issuer;
> - how name constraints are read, and what happens to a constraint the verifier cannot read;
> - what the trust anchor itself is checked for;
> - how each of the issue's four gates is met, with what it costs.

---

## 1. Three PRs

1. **This design.**
2. **The verifier**, in `packages/x509`, with the four gates of §6: x509-limbo, a generated OpenSSL matrix, saved real chains
   against the system roots, and mutants.
3. **`packages/tls` uses it.** `tls.trust` loads roots instead of pins, and `Certificate` is verified against them, the host and
   the clock (`docs/tls-core.md` §5 ends). The live tests and the recorded streams of `docs/tls-core.md` §10 move from a pinned
   certificate to a CA made for them.

## 2. Files and API

`packages/x509/x509.cho` is 1,598 lines, so verification is two new modules beside it, each under 2,000 lines
(`crates/cancho/tests/files.rs`):

| File | Module | What |
|---|---|---|
| `packages/x509/names.cho` | `x509_names` | a host as a DNS name or an IP address; matching a SAN; reading and applying name constraints |
| `packages/x509/verify.cho` | `x509_verify` | the root store; building a path; the checks of `docs/tls-pure.md` §5.2 on each certificate |

```
// The store: the roots of a PEM bundle, each as a 3-byte length and its DER, the format of
// docs/tls-core.md §5's pins. A block that is not a certificate, or a certificate this parser
// refuses, is skipped and counted, never fatal (docs/x509.md §5.3).
x509_verify.store_load(pem, store, info) -> roots | refusal   // info[0] bytes used, info[1] skipped

// A chain as the server sent it: `certs` holds the DERs, `ranges` their [start, end) pairs,
// the leaf first. `host` is a DNS name or an IP literal. `now` is seconds since 1970.
x509_verify.verify(store, certs, ranges, host, now, max_intermediates) -> 0 | refusal
```

*§10 adds `verify_chain` (the chain for a purpose, with no name), `verify_name` (the name, as its own step), and
`san_next`; `verify` is those two steps, with the same answers.*

- **`ranges` rather than a copy.** `packages/tls` already has the `Certificate` message in a slot, and
  `tls_message.certificate` already gives each certificate's range (`docs/tls-core.md` §9.1). The verifier reads them in place.
- **`max_intermediates`** is 6 for TLS (`docs/tls-pure.md` §5.2: 8 certificates, leaf and root included). It is a parameter
  because x509-limbo sets it per case (§6.1). *As built (§8.3): it counts intermediates that are not self-issued, as RFC
  5280 §6.1.4 and limbo's `max-chain-depth-1-self-issued` count them, and no path has more than 6 intermediates of any kind.*
- **No capability.** The clock and the store are numbers and bytes the caller passes, as for the rest of the engine
  (`docs/tls-pure.md` §2.2).
- **Working memory** is the verifier's own regions: up to 7 views of 40 words (`docs/x509.md` §2), a candidate list, and the
  RSA or ECDSA work area. Nothing grows with the input past the limits of §3.

## 3. Building a path

The server sends its leaf first and the rest "in any order" (RFC 8446 §4.4.2). A sent intermediate may be unused, and more
than one certificate may carry the issuer's name: cross-signs, a renewed intermediate, or an attacker's decoy. x509-limbo tests
each of these (`pathological::*`, `rfc5280::*`).

- **Depth-first, with backtracking.** From the certificate being checked, the candidates for its issuer are:
  - first the roots whose subject equals its issuer;
  - then the sent certificates whose subject equals its issuer.

  A candidate is tried only if its signature over the certificate verifies and its checks of §4 pass. If the path from it fails,
  the next candidate is tried. The path's result is the first that reaches a root. When every path fails, the refusal is the one
  from the deepest attempt, so a chain that is merely expired says `x509-expired`, not `x509-unknown-issuer`.
- **Names are compared byte for byte.** That is webpki's choice, and it is what RFC 5280 §7.1's case-insensitive comparison
  reduces to for every name in a real chain. A pair of names that differ only in case is "no issuer", and `x509-unknown-issuer`
  says so.
- **No certificate twice in one path.** That refuses a loop instead of following it (`docs/tls-pure.md` §5.2), and it covers
  `pathological::intermediate-cycle-*`. *Corrected (review finding D-2, #209): "twice" is by position in what the server sent,
  not by content (`verify.cho`, `in_path`). The same self-issued CA certificate sent at two positions can stand twice in one
  path. That is no loop: each hop still costs a signature of the 64-signature budget below and one of the depth slots, so the
  search stays bounded, and the second copy adds no authority the first did not have.*
- **A budget.** At most 64 signature verifications per `verify`. x509-limbo's `pathological-chain-*` cases send 100
  intermediates with the same subject. Without a budget, backtracking over them is exponential. With one, the worst case is 64
  signatures: about 0.1 s at 1.5 ms each on LLVM for P-256 (`docs/ecdsa.md` §5.4), or 0.6 s at 9.9 ms for P-384 on Cranelift.
  *Corrected (review finding C-1, #317): that assumed an RSA key's exponent was 65537. The server chooses its keys, and
  nothing bounded the exponent below the modulus; measured on an Apple M4 (LLVM), one RSA-4096 exponentiation took 0.79 ms
  with 65537 and 176 ms with a 4,095-bit exponent, so 64 of them 11.3 s. `std.rsa` now refuses an exponent over 64 bits, whose
  worst case measured 3.15 ms: 0.2 s for the budget.*
  Exhausting the budget is `x509-path-too-long`.
- **Signatures are checked before anything else about a candidate.** A candidate whose key does not verify is not an issuer.
  *Corrected (§8.3): this said a signature is the only way to tell two certificates with the same subject apart. As built,
  two cheaper facts come first, as OpenSSL's `X509_check_akid` and RFC 5280 §4.1.2.6 have them:*
  - *a candidate whose subjectKeyIdentifier differs from the certificate's authorityKeyIdentifier keyIdentifier is not its
    issuer, when both are present;*
  - *a CA with an empty subject issues nothing.*

## 4. What each certificate is checked for

`docs/tls-pure.md` §5.2 lists the checks. These are the decisions it left open.

| Certificate | Signature | Validity | `cA` and `pathLen` | keyUsage | EKU | Name constraints |
|---|---|---|---|---|---|---|
| leaf | checked by its issuer's key | checked | not read | `digitalSignature` if present; `keyCertSign` only with `cA` | `serverAuth` if present; `anyExtendedKeyUsage` alone is refused, as OpenSSL's server purpose does (measured, §8.2) | subject to every issuer's; its own refused unless it is a CA |
| intermediate | checked | checked | `cA` true; `pathLen` against the intermediates below it, self-issued ones not counted (RFC 5280 §6.1.4) | `keyCertSign` if present | `serverAuth` if present (OpenSSL and webpki both refuse an intermediate whose EKU excludes it) | its own apply below it |
| root | **not checked** (§5.1 of `tls-pure.md`) | checked | `cA` true, except an X.509 v1 root, which has no extensions; `pathLen` as an intermediate's | `keyCertSign` if present | not read | its own apply below it |

*Corrected (§8.3), two cells:*
- *The leaf's pathLen cell said "not read, so `pathlen::validation-ignores-pathlen-in-leaf` holds". That case's leaf is a CA
  whose keyUsage has no `digitalSignature`, so it is refused for that, as OpenSSL refuses it.*
- *The root's validity cell said "not checked", following `docs/tls-pure.md` §5.1's claim that this is OpenSSL's default.
  Measured, OpenSSL refuses an expired root (`rfc5280::validity::expired-root`, error 10 at the root's depth). The root's
  dates are checked, and that claim is corrected where it was made.*

- **Validity** is `notBefore <= now <= notAfter`, both inclusive (RFC 5280 §4.1.2.5), in whole seconds.
- **Signature algorithms**, by the outer AlgorithmIdentifier, which `parse` already checks equals the inner one:
  - `sha256`, `sha384` and `sha512WithRSAEncryption` (PKCS#1 v1.5, `std.rsa`);
  - RSASSA-PSS with SHA-256, SHA-384 or SHA-512, MGF1 with the same hash, and the salt the hash's length (the only parameters
    the Web PKI uses). Reading those parameters needs the hash and MGF1 OIDs, which `docs/x509.md` §2.3's table lacks; PR 2 adds
    them through `scripts/x509_oids.py`;
  - `ecdsa-with-SHA256` and `ecdsa-with-SHA384`, on P-256 and P-384 (`std.ecdsa`). `ecdsa-with-SHA512` is accepted on either
    curve, truncated as FIPS 186-5 says;
  - Ed25519;
  - anything else, SHA-1 included, is `x509-unsupported-algorithm`. That is 14 certificates of `ecdsa-with-SHA1`, 12 of ML-DSA
    and 2 of DSA in x509-limbo (§6.1).
- **Keys.** RSA from 2048 to 4096 bits, else `x509-key-size`. EC on P-256 or P-384, else `x509-unsupported-algorithm` (x509-limbo
  has two P-192 keys).
- **Leniencies** (`docs/x509.md` §3.2) are accepted on every certificate, as OpenSSL accepts them.
- **An unknown critical extension** is already refused by `parse` (`docs/x509.md` §2.2).

## 5. Names

### 5.1 The host

`host` is an IP literal when `tls_message.is_ip_literal` says so: only digits and dots, or any colon. An IPv4 literal must be
four decimal parts of 0 to 255 with no leading zeros. An IPv6 literal is RFC 4291 §2.2's text form, `::` included and a
trailing dotted IPv4 included. Anything else is `x509-name-mismatch`, since no certificate can match it. A DNS host is
lowercased ASCII, with at most one trailing dot removed.

### 5.2 Matching the leaf

`docs/tls-pure.md` §5.3 holds without change. The SAN only; an IP host matches only an `iPAddress` entry, byte for byte, and a
DNS host only a `dNSName` entry. A wildcard is the whole left-most label, needs two labels after it, and matches exactly one
label. A dNSName entry that is not a valid name (empty labels, a `*` anywhere else) matches nothing; it does not refuse the
certificate. *Corrected (review finding D-5, #209): it does not when no CA above it has nameConstraints. Under a constrained CA
every dNSName of the SAN is constrained (§5.3), and one that is not a valid name cannot be placed inside or outside a subtree,
so it refuses the chain as `x509-name-constraint` (`names.cho`, `constraints_ok`): a leaf naming `example.com` and
`example.com.` under a constrained intermediate is refused. That is the safe side; webpki skips such an entry instead.*

### 5.3 Name constraints

The issue names x509-limbo's BetterTLS cases as a gate. They are 9,572 of its 9,802 cases, and every one is about name
constraints. Among the 30,379 certificates of `limbo.json`, the constraint subtrees are 24,681 `dNSName`, 11,736
`iPAddress`, 12 `rfc822Name`, 5 `directoryName`, 2 URI and 2 `otherName` (measured with pyca, §6.1).

- **`dNSName` subtrees.** A name matches a subtree when it equals it, or ends with `.` followed by it, compared without case. A
  subtree with a leading dot (`.example.com`) matches only proper subdomains. An empty subtree matches every name.
- **`iPAddress` subtrees** are an address and a mask of the same length, 8 or 32 bytes. An address of the other family never
  matches. A mask that is not contiguous ones then zeros is unreadable (below). *Corrected (review finding D-3, #209): only
  when it is read, which is when a certificate below carries an `iPAddress` of the subtree's family (`names.cho`, `subtrees`).
  The pass that reads the constraints once with no name checks a subtree's length, not its mask, so a CA with such a mask is
  accepted above a chain with no address of that family. Whenever the constraint would apply, it is refused: no name escapes
  it.*
- **What is constrained.** Every `dNSName` and `iPAddress` in the SAN of every certificate below the constraining one. A
  wildcard SAN `*.a.example` is checked as `a.example` against permitted subtrees. *Corrected (§8.3): this said "and as
  itself against excluded ones, as webpki does". As built, against an excluded subtree it is refused when either holds the
  other: `a.example` inside the subtree, or the subtree (`x.a.example`) inside `a.example`, since the wildcard names it.*
  *Corrected again (review finding D-1, #318): that missed a subtree with a leading dot. `.a.example` holds only proper
  subdomains of `a.example`, so neither held the other, and `*.a.example`, which names only such subdomains, passed an
  exclusion of `.a.example`. A wildcard is now also refused by a dotted subtree whose base is its own, as OpenSSL refuses it;
  `.x.a.example` still excludes none of the names `*.a.example` can match. Both are cases of the OpenSSL matrix (§6.2). Against a
  permitted `.a.example`, `*.a.example` is still refused, which is the safe side of the same reading.* The subject's common name is never read (no CN fallback, `docs/tls-pure.md` §5.3), so it is never constrained.
- **Permitted, then excluded.** A name of a type that has permitted subtrees must match one of them, and it must match none of
  the excluded subtrees.
- **A subtree of any other type**, `rfc822Name`, `directoryName`, URI or `otherName`, is unreadable: the chain is refused with
  `x509-name-constraint`. This is RFC 5280 §4.2.1.10's rule for a critical extension the verifier cannot apply, and
  `docs/tls-pure.md` §8 already maps OpenSSL's 51 to 53 to that tag. It costs every x509-limbo case whose constraint has one
  of those types, and §6.1 counts them.
- **Malformed constraints** are unreadable too: a nameConstraints with neither subtree list, an empty list (RFC 5280 says
  `SIZE (1..MAX)`), a `minimum` or `maximum`, or the lists out of order. *(Added in PR 2, §8.3.)*
- **Limits.** At most 1,024 subtrees in one certificate, and at most 2^20 name-against-subtree comparisons in one chain.
  `pathological::nc-dos-*` exist to find a verifier without that second limit. Over either is `x509-name-constraint`.

## 6. The gates

### 6.1 x509-limbo

`scripts/x509_limbo.py` (`docs/x509.md` §5.3) gains a `verify` mode. For every case in `limbo.json` it passes the trusted
certificates as the store, the intermediates and the leaf as the chain, `expected_peer_name` as the host, `validation_time`
(or the time of the run when it is null) as `now`, and `max_chain_depth` when set. Each case's result is one of:

- **pass**: SUCCESS accepted, or FAILURE refused;
- **refused for the right reason**: a FAILURE whose tag is the reason the case's description gives. It is listed by tag;
- **not applicable**: a case outside this client's scope, each kind named with its count:
  - `validation_kind` CLIENT (10 cases: this is a server-certificate verifier);
  - revocation (`has-crl`, 17 cases: not checked, `docs/tls-pure.md` §5.4);
  - ML-DSA and DSA, which `docs/tls-pure.md` §5.2 leaves out;
  - a name-constraint type of §5.3's unreadable list, when the case expects SUCCESS;
- **a disagreement**: anything else, each one read and either fixed or explained in the results.

limbo is 41.8 MB and fetched, not committed (as in `docs/x509.md` §5.3). `cargo test` runs a committed subset: every case that is
not BetterTLS (230), and one BetterTLS case in twenty, picked by a hash of its id. The subset is written by the script with the
expected outcomes.

### 6.2 An OpenSSL matrix

`scripts/x509_matrix.py` makes a CA, an intermediate and leaves with the OpenSSL CLI, one case per row of the issue: valid,
expired, not yet valid, wrong host, self-signed, untrusted root, an intermediate that is not a CA, `pathlen` exceeded, and the
three wildcard abuses (`*.com`, `a.*.b.com`, and `*.a.b.com` against `a.b.com`). It also adds the cases §4 decided: keyUsage
without `keyCertSign`, a leaf EKU of `clientAuth` only, an RSA-1024 leaf, a SHA-1 signature, P-192, and a constraint excluding
the host. Each case runs both:
- `openssl verify -x509_strict -purpose sslserver -verify_hostname <host> -attime <t> -CAfile root -untrusted intermediates leaf`
  (`-purpose` matters: without it `openssl verify` checks no EKU, while a TLS client's OpenSSL does);
- the verifier.

The case's tag must be the one expected, and no case OpenSSL refuses may be accepted. The certificates are committed
(`tests/vectors/x509/matrix/`), with `-attime` fixed, so the test needs no OpenSSL.

### 6.3 Saved real chains, the system roots

x509-limbo's 14 `online::*` cases are real chains, saved from `akamai.com`, `google.com`, `cloudflare.com` and eleven more, each
with the time it was saved. Among them:
- RSA with SHA-256 and SHA-384;
- ECDSA P-256 and P-384 leaves;
- chains of two and three certificates.

Each root is in this machine's 128-root bundle, byte for byte (measured). They are verified against the whole bundle, through
`store_load`, at their saved time and at a time outside the leaf's validity. A live connection is not part of the gate, as the
issue says. It also could not be: this container's outbound HTTPS goes through an intercepting proxy, which would show its own
chain.

### 6.4 Mutants

At least 12, each killed by §6.1 to §6.3:
- a skipped signature check;
- a wildcard crossing a dot;
- `notAfter` compared off by one;
- a missed `cA`;
- `pathLen` counting self-issued intermediates;
- `pathLen` not checked;
- a wildcard on a one-label suffix (`*.com`);
- a partial wildcard (`f*.example.com`);
- the excluded subtrees ignored;
- a `dNSName` subtree matched as a plain suffix (`example.com` permitting `badexample.com`);
- an IP mask ignored;
- EKU not checked;
- the leaf's keyUsage not checked;
- the budget off.

## 7. Questions this does not settle

- Whether byte-for-byte name comparison loses a real chain. x509-limbo has cases for it, and §6.1 will list them. *Answered
  (§8.2): no limbo case and none of the 14 real chains is lost to it.*
- Whether `*.co.uk`-style wildcards need a public-suffix list. That is `docs/tls-pure.md` §10's question 5; limbo's
  `pedantic-public-suffix-wildcard` cases (3) will be listed as disagreements if they fail, not hidden. *Measured (§8.2): two
  of the three are accepted here and by OpenSSL, and listed. The question stays a person's.*

## 8. PR 2: the verifier (results)

### 8.1 What was built

| File | Lines | What |
|---|---|---|
| `packages/x509/names.cho` (`x509_names`) | 565 | the host as a DNS name or an IPv4 or IPv6 address; SAN matching; name constraints |
| `packages/x509/verify.cho` (`x509_verify`) | 778 | `store_load`, `verify`, the refusal tags; path building, the checks of §4, the signatures |
| `packages/x509/x509.cho` | 1,665 | four OIDs for RSASSA-PSS's parameters (SHA-256, -384, -512, MGF1), from `scripts/x509_oids.py` and checked against `openssl asn1parse`; `is_ip_literal`, moved here from `packages/tls/message.cho` so the two share one copy (`conformance/duplication.rs`) |
| `tests/programs/x509_verify_driver.cho` | 203 | a store line and chain lines, from standard input |
| `scripts/x509_limbo.py verify`, `x509_matrix.py`, `x509_online.py`, `x509_verify_mutants.py` | | §6's four gates |

`conformance/x509_verify.rs` replays all three committed files (`tests/vectors/x509/verify/`), the matrix and the real chains on
both backends, in 4 seconds. The verifier needs `--std` (`std.rsa`, `std.ecdsa`, `std.ed25519`, `std.crypto`, `std.bytes`).

### 8.2 Evidence

**x509-limbo** (`limbo.json` SHA-256 `611e337b…`, the file of `docs/x509.md` §5.3). All 9,802 cases in 26 s:
- **9,743 pass.** Every BetterTLS case is among them: 8,707 expected failures refused and 865 expected successes accepted.
  FAILURE cases are refused with these tags:

  | Tag | Cases |
  |---|---|
  | `x509-name-constraint` | 5,364 |
  | `x509-name-mismatch` | 2,814 |
  | `der-tag` and `x509-name` (the SANs of `docs/x509.md` §5.3) | 552 |
  | `x509-unknown-issuer` | 19 |
  | `x509-path-too-long` | 13 |
  | `x509-unsupported-algorithm` | 12 |
  | `x509-expired`, `x509-not-yet-valid` | 14 |
  | `x509-not-ca` | 10 |
  | `x509-key-usage` | 8 |
  | nine other tags | 14 |

  limbo gives no machine-readable reason for a FAILURE, so "refused for the right reason" is read from the tag against the
  case's name. The BetterTLS name-constraint cases are refused as `x509-name-constraint` or, where the leaf's own name is the
  one outside the constraint, as `x509-name-mismatch`.
- **20 not applicable:**
  - revocation (10);
  - a client certificate (5);
  - ML-DSA or DSA expected to succeed (3);
  - a case with no peer name (1);
  - a `directoryName` constraint expected to succeed (1).
- **39 disagreements, each read and checked against `openssl verify -purpose sslserver` (OpenSSL 3.0.13), with and without
  `-x509_strict`.** `scripts/x509_limbo.py`'s `KNOWN` lists each one with its reason, and the script fails on any other:

  | Disagreement | Cases | OpenSSL |
  |---|---|---|
  | a CA as the leaf, with no `digitalSignature` in its keyUsage, expected to succeed | 2 | refuses too (26) |
  | a root whose AKI does not name itself, so OpenSSL does not take it as self-issued and finds no anchor | 5 | refuses, only for that |
  | CA/Browser Forum or webpki rules beyond RFC 5280: CN contents, EKU presence, RSA sizes not a multiple of 8, a CA flag on a leaf | 16 | accepts |
  | public-suffix wildcards (`docs/tls-pure.md` §10 question 5) | 2 | accepts |
  | missing AKI or SKI, a non-critical basicConstraints on a root, an empty subject with a non-critical SAN | 6 | accepts; refuses with `-x509_strict` |
  | others RFC 5280 or CABF state and OpenSSL does not check: non-critical name constraints or policy constraints, a leading period in a dNSName constraint, an underscore, serial zero, a malformed AIA, a critical SAN with a subject, a root's AKI without a keyIdentifier | 8 | accepts |

  So every limbo case this verifier accepts and OpenSSL refuses is one of the 5 whose root is unusual, and in each of them the
  leaf is signed by a root in the store.

**The OpenSSL matrix** (`scripts/x509_matrix.py`, `tests/vectors/x509/verify/matrix.txt`). 33 cases, every one with its own tag,
and no case OpenSSL refuses accepted:
- the issue's rows: valid, expired, not yet valid, wrong host, self-signed, untrusted root, an intermediate that is not a CA,
  `pathlen` exceeded, `*.com`, `a.*.b.com`, `*.a.b.com` against `a.b.com`;
- §4's additions: an intermediate without `keyCertSign`, a leaf EKU of `clientAuth` only or `anyExtendedKeyUsage` only
  (OpenSSL: 26), an RSA-1024 leaf, P-192, SHA-1, an unknown critical extension, an expired intermediate, constraints
  excluding and permitting the host;
- the signatures: P-256, RSA-4096 with SHA-512, Ed25519, RSA PKCS#1 v1.5 and RSA-PSS issuers, one bit of a signature changed;
- names: a wildcard, IPv4 and IPv6 SANs, a host only in the CN.

Four of them are refused here and accepted by OpenSSL's defaults: the CN-only host (no CN fallback, `docs/tls-pure.md` §5.3),
RSA-1024, P-192 and SHA-1 (§4).

**Saved real chains against the system roots** (`scripts/x509_online.py`). All 128 roots of the bundle load, none skipped. Each
of the 14 chains is then checked six ways, 84 checks, all as wanted:
- at its saved time;
- at exactly the leaf's notAfter, and at exactly its notBefore (both `ok`: both are inclusive);
- a second after (`x509-expired`) and a second before (`x509-not-yet-valid`);
- under `<name>.invalid` (`x509-name-mismatch`).

The bundle is committed as `tests/vectors/x509/verify/roots.pem` (Debian's ca-certificates 20260601~24.04.1), so the replay
needs no system file.

**22 mutants, 22 killed** (`scripts/x509_verify_mutants.py`, 3 minutes):
- a skipped signature check, on an intermediate and on a root;
- a wildcard crossing a dot; a wildcard on a one-label suffix;
- notAfter and notBefore off by one;
- a missed `cA`;
- `pathLen` counting self-issued intermediates; `pathLen` not checked;
- excluded and permitted subtrees ignored; a `dNSName` subtree as a plain suffix; an IP mask ignored;
- the leaf's EKU, and its keyUsage, not checked; `keyCertSign` not required of an issuer;
- the budget off (limbo's pathological chains then do not finish in 120 s);
- a root's and an intermediate's dates not checked;
- the host not matched;
- RSA-1024 allowed;
- the AKI not matched against the SKI.

§6.4's partial-wildcard mutant (`f*.example.com`) cannot be written: a pattern with a `*` outside the left-most label never equals
a host, which has no `*`, so removing the check changes nothing.

**Cost.** The 14 real chains (two signatures each, RSA and ECDSA) against the 128-root store take 3.4 to 3.7 ms a chain on
either backend, measured through the driver, so this includes its byte-by-byte reading of hex. Loading the store takes 8 ms.

### 8.3 Found

limbo found these before any of them reached `main`. Each is corrected where its claim was made:
- **`docs/tls-pure.md` §5.1's claim that OpenSSL accepts an expired root was false.** It refuses one. The root's dates are now
  checked. The claim is corrected there, and in its §10 question 4, and in §4 here.
- **Issuer selection needed the AKI and SKI, and a non-empty CA subject** (§3), to agree with OpenSSL where a sent certificate
  carries the issuer's name but not its key.
- **`max_intermediates` must not count self-issued intermediates** (§2).
- **Five RFC 5280 MUSTs** were added to the leaf and the constraints: keyCertSign only with `cA`, name constraints only in a CA,
  neither list empty, no constraint with neither list (§4, §5.3).
- **The wildcard rule against excluded subtrees** was written backwards in §5.3, and is corrected there.
- Two test expectations were wrong, not the verifier:
  - *docs.python.org* holds `*.python.org`, so `not-docs.python.org` does match it;
  - a leaf signed by an impostor is `x509-unknown-issuer`, not `x509-bad-signature`, once the AKI is matched.

## 9. PR 3: `packages/tls` verifies with it (results)

### 9.1 What changed

- **`tls_client`.**
  - `start` takes `now` (seconds), kept in the slot.
  - `Certificate` is verified by `x509_verify.verify`, over the message's own ranges, against the store, the host `start` was
    given and `now`. The leaf is still kept for `CertificateVerify`.
  - The pin check is gone.
- **`tls`.**
  - `trust` is `x509_verify.store_load`: a root it cannot read is skipped and counted (`tls.skipped`), never fatal.
  - `start` passes `now_unix_ms / 1000`.
- **Tags and alerts.** `tls_record` gains the verifier's tags as codes -25 to -33. Each refusal sends the alert RFC 8446 §6.2
  names:

  | Refusal | Alert |
  |---|---|
  | `x509-unknown-issuer` | `unknown_ca` (48) |
  | `x509-expired`, `x509-not-yet-valid` | `certificate_expired` (45) |
  | `x509-key-usage`, `x509-critical-extension`, `x509-unsupported-algorithm`, `x509-key-size` | `unsupported_certificate` (43) |
  | `x509-bad-signature`, `x509-name-mismatch`, `x509-not-ca`, `x509-path-too-long`, `x509-name-constraint`, `x509-chain-too-large`, `x509-decode` | `bad_certificate` (42) |
- **`tests/programs/tls_driver.cho`.** `C` takes a PEM root bundle and a time instead of pins. `tls_many` checks against
  `clock_unix_ms`. Its first version passed `clock_ms`, a monotonic clock, and every certificate was "not yet valid".
- **Every recording was made again with a CA:**
  - the tlslite-ng traces: an RSA-2048 CA for the RSA leaf, a P-256 CA for the ECDSA leaf;
  - the lying server: an Ed25519 CA from a fixed seed, so its recording is still identical run to run;
  - the 64 streams.

  The replays are byte for byte as before. The test of a wrong root is now a root from another CA.

### 9.2 Evidence

- **`conformance/tls.rs`, all six tests:**
  - both traces replayed byte for byte on both backends;
  - any split of the server's bytes;
  - a certificate from another CA refused as `x509-unknown-issuer`, with `unknown_ca`;
  - the 15 ServerHellos;
  - the 29 lying-server connections;
  - 64 connections on one poller, one byte and 65,536 a read, and cut short.
- **Live, 64 connections at once** (`scripts/tls_live.py`). Every server has a certificate from a CA made for the run, and the
  CA is all `tls_many` trusts:
  - Python `ssl` with P-256, P-384, RSA-2048, RSA-4096 and Ed25519 leaves, one byte and 65,536 a read: every connection `ok`;
  - `openssl s_server` and tlslite-ng: every connection `ok`;
  - no close_notify: every connection `tls-peer-closed`;
  - a host the certificate does not name: every connection `x509-name-mismatch`;
  - another CA's root: every connection `x509-unknown-issuer`.
- **Mutants** (`scripts/tls_mutants.py`): **24 of 24 killed**. "The pin not checked" is replaced by three:
  - the chain not verified;
  - the time not given to the verifier;
  - an unknown issuer reported as `x509-decode`.

### 9.3 Found

- **Two CAs with one name are one issuer, as far as a name goes.** The first re-recording gave both trace CAs the same subject
  and no key identifiers. The "another CA" test then failed, correctly, as `x509-bad-signature` instead of
  `x509-unknown-issuer`. Each test CA now has its own name. The verifier is unchanged: a same-named issuer whose key does not
  verify is a bad signature, as OpenSSL says (error 7).
- **`clock_ms` is not a time of day** (§9.1).


## 10. A chain without a name

Two users need a chain checked with no host name:
- **`sslmode=verify-ca`** in cancho-pg (its `docs/tls.md`, PR #14): the chain to a trusted CA, and no name. Its §3 found that
  `verify` parses the host and matches it inside one function, and that the path builder, `extend`, is private, so only
  `verify-full` could be offered.
- **Client certificates**, step 4 of `docs/tls-server.md` §8: a server verifies a client's chain against a configured trust
  store. There is no name to expect; the program is given the verified subject and SANs and decides what they may do.

### 10.1 The API

```
// Which extendedKeyUsage the leaf and every intermediate must allow, when they carry one.
x509_verify.purpose_server_auth() -> 1   // a server's certificate: the TLS client, verify-full, verify-ca
x509_verify.purpose_client_auth() -> 2   // a client's certificate: a TLS server's step 4

// The chain only: the path, every signature, every validity, cA and pathLen, keyUsage and EKU for `purpose`, name
// constraints over every SAN. No name is read. `leaf` (x509.view_len() words) gets x509.parse's view of the leaf when the
// answer is 0, and all zeros on a refusal.
x509_verify.verify_chain(store, certs, ranges, now, max_intermediates, purpose, leaf) -> 0 | refusal

// The name, as its own step: the leaf's subjectAltName against `host`. 0, or x509-name-mismatch.
x509_verify.verify_name(leaf_der, leaf, host) -> 0 | refusal

// Unchanged, and now those two steps: the host must be readable (else x509-name-mismatch, first, as before), then
// verify_chain(purpose_server_auth()), then verify_name.
x509_verify.verify(store, certs, ranges, host, now, max_intermediates) -> 0 | refusal

// The verified leaf's subjectAltName, one GeneralName at a time: `at` 0 starts; answers where the next one starts, or 0
// when there is none. entry[0] is the tag (0x81 rfc822Name, 0x82 dNSName, 0x86 URI, 0x87 iPAddress, 0xa0 otherName,
// 0xa4 directoryName, ...), entry[1], entry[2] its content's [start, end) in `leaf_der`.
x509_verify.san_next(leaf_der, leaf, at, entry) -> next | 0
```

The subject is `leaf_der[leaf[x509.subject_start()]..leaf[x509.subject_end()]]`, the Name's DER, as every other field of the
view is read (`docs/x509.md` §2). A function that picks a common name out of it is left until a program asks
(`CONTRIBUTING.md`: two askers); the broker of step 4 is the first candidate.

- **One path builder.** `verify_chain` is the code `verify` ran up to the name: the same parse, `leaf_ok`, `extend`, budget and
  refusal order. The purpose is passed down to the two places that read an EKU. `verify` keeps its signature, its refusal
  tags and their order; the matrix, the real chains and the limbo subset replay unchanged (§10.5).
- **A purpose that is neither** is refused, `x509-purpose` (-41), before anything is read. There is no default and no "any".
- **`leaf` shorter than `x509.view_len()`** is `x509-structure`, as `x509.parse` answers for a short view.

### 10.2 The purposes

The two purposes differ only where an EKU is read; every other check of §4 is the same.

| | `purpose_server_auth()` | `purpose_client_auth()` |
|---|---|---|
| the leaf's EKU, when present | must hold `serverAuth` | must hold `clientAuth` |
| `anyExtendedKeyUsage` alone in the leaf | refused (§4) | refused, as OpenSSL's `sslclient` purpose does (measured, §10.5) |
| an intermediate's EKU, when present | must hold `serverAuth` | must hold `clientAuth` |
| the root's EKU | not read (§4) | must hold `clientAuth`, when present. *Corrected while building (§10.5): this cell said "not read". Measured, OpenSSL's `sslclient` refuses a root whose EKU excludes `clientAuth` (error 26 at the root's depth), and so does its `sslserver` for `serverAuth`. A client-certificate store is a file an operator writes for the purpose, so the stricter reading costs nothing there; for a server's purpose §4's rule stays, since `verify`'s answers do not change in this PR, and the difference is listed (§10.5)* |
| the leaf's keyUsage, when present | `digitalSignature` | `digitalSignature`: a TLS 1.3 client signs CertificateVerify with it. OpenSSL's `sslclient` also accepts `keyAgreement` alone, which no TLS 1.3 client can use, so that is refused here and accepted there |
| OpenSSL's equivalent | `openssl verify -purpose sslserver` | `openssl verify -purpose sslclient` |

- **The TLS client and `verify-full`** use `verify`, so `serverAuth`, as today.
- **`verify-ca` is `verify_chain` with `purpose_server_auth()`.** That is what libpq does: with `verify-ca` or `verify-full` it
  sets `SSL_VERIFY_PEER` with the root file and no purpose or host of its own (`fe-secure-openssl.c`, PostgreSQL 16), so
  OpenSSL's TLS client applies its default for a server's chain, the `ssl_server` purpose (`ssl_verify_cert_chain` in
  `ssl/ssl_cert.c`, OpenSSL 3.0.13: `X509_STORE_CTX_set_default(ctx, s->server ? "ssl_client" : "ssl_server")`). The name is
  libpq's own check, made only under `verify-full` (`pq_verify_peer_name_matches_certificate` returns at once for any other
  mode, `fe-secure-common.c`). So `verify-ca` there requires `serverAuth` when an EKU is present, and every chain check; it
  skips the name and nothing else. It is the same here.
- **Step 4** uses `purpose_client_auth()`. The same `ssl_verify_cert_chain` line is why: an OpenSSL server (PostgreSQL's,
  mosquitto's) checks a client's chain with the `ssl_client` purpose.

### 10.3 The hazard, and how the API says it

A chain verified with no name authenticates **"a key some CA in this store vouched for, for this purpose, today"**. It does not
say *which* server or *which* client. Two consequences:

- **The store decides everything.** Under `verify-ca` against the system bundle, anyone who can get a certificate for any
  domain from any public CA passes, so `verify-ca` is only safe with a store holding a CA that issues only to the servers the
  client means to reach: a private CA per database, which is libpq's own advice for `verify-ca`. For client certificates the
  same holds more sharply: public CAs have issued certificates with both `serverAuth` and `clientAuth`, so a server that
  trusted the system bundle for client certificates would admit the holder of any such web certificate. **A client-certificate
  store is its own file, never the system roots.** That is for step 4 and for cancho-pg to enforce where they load the store;
  `packages/x509` cannot tell a public CA from a private one.
- **Authentication is not authorization.** After `verify_chain` answers 0 the program has an identity, the subject and the
  SANs, that the CA put there. Deciding what that identity may do (which MQTT topics, which database role) is the program's,
  and it must read the identity from the `leaf` view `verify_chain` filled, which is the certificate that was verified.

How the API makes the choice explicit rather than easy to fall into:
- **`verify` cannot be told to skip the name.** An empty host, `-`, `*` or anything else that is not a DNS name or an IP
  literal is still `x509-name-mismatch` (§5.1), before the chain is read. There is no flag. Tested (§10.4).
- **A chain without a name is a different function**, with a name that says what it checks, and it takes a purpose with no
  default. A caller that wants the name must call `verify` (or `verify_name` after `verify_chain`): that is the only way the
  TLS client verifies, and `packages/tls` is unchanged by this section. Offering `verify-ca` through `packages/tls` would be
  its own named entry there, asked for by cancho-pg, not a flag on `start`.
- **A refusal leaves no identity behind.** On a refusal `leaf` is all zeros: no subject range and no SAN, so a program that
  forgets to look at the answer reads nothing, rather than a name from a certificate that failed.
- **The CA's limits still apply.** Name constraints are checked over every SAN of the leaf in `verify_chain`, whether or not a
  name is asked, so a CA constrained to `example.com` cannot vouch for a client whose SAN says `admin.other.test`.
  A subject (a `directoryName`) is constrained only by a `directoryName` subtree, which this verifier refuses to read (§5.3),
  so a CA with one is refused; a CA with only `dNSName` subtrees says nothing about the subject. A program that authorizes
  by subject must trust each CA of its store for every subject, which is RFC 5280's reading.

### 10.4 Tests

- **Unchanged and passing:** `conformance/x509_verify.rs` (limbo subset, the 14 real chains, the 33-case matrix), the TLS
  client's suites (`conformance/tls.rs`: the traces, the liar, the 64 streams; tickets; the differentials), the fuzz corpora,
  and `publish_packages.py --check` with the stores republished.
- **A second OpenSSL matrix, with no name** (`scripts/x509_matrix.py chain`, `tests/vectors/x509/verify/chain_matrix.txt`).
  Each case runs `openssl verify -x509_strict -purpose sslclient` or `sslserver`, with no `-verify_hostname`, and the
  driver's new `C` line (`verify_chain`). The tag must be the one expected, and no case OpenSSL refuses may be accepted:
  - for both purposes: a valid chain; an expired leaf; an expired intermediate; an unknown CA; `pathlen` 0 exceeded; an
    intermediate that is not a CA; a leaf with no SAN (accepted: there is no name to match); a bad signature;
  - a client-auth-only leaf: accepted for `clientAuth`, `x509-key-usage` for `serverAuth`;
  - a server-auth-only leaf: the reverse;
  - a leaf with both, and a leaf with no EKU: accepted for both;
  - `anyExtendedKeyUsage` alone: refused for both;
  - an intermediate whose EKU is `serverAuth` only, under a `clientAuth` leaf: refused for `clientAuth`; and the reverse;
  - a root whose EKU is `serverAuth` only, and one whose EKU is `clientAuth` only, for both purposes;
  - a leaf whose keyUsage is `keyAgreement` only, for `clientAuth` (refused here, accepted by OpenSSL: §10.2);
  - a constrained CA and a client leaf whose SAN is outside it: `x509-name-constraint` with no name asked;
  - the leaf's view: each accepted case's answer carries the subject and the SAN entries read back through `san_next`,
    compared with what the script put in the certificate.
- **`verify` cannot skip the name:** the same valid chain through `V` with the host empty, `-`'s byte and `*`: each
  `x509-name-mismatch`.
- **`verify_name` on its own:** the right host, a wrong one, an IP, a leaf with no SAN.
- **A purpose of 0 or 3:** `x509-purpose`.
- **Mutants** over the new code (`scripts/x509_verify_mutants.py`, with `chain_matrix.txt` added to what it replays): the
  purpose ignored at the leaf; ignored at an intermediate; the client purpose made the server's; an invalid purpose
  accepted; a root's EKU not read for client certificates; the leaf view not cleared on a refusal (the driver prints
  `left-behind` when a refused chain's view still holds a subject or a SAN); `san_next` skipping the first entry;
  `verify_name` matching nothing; `verify` built on `purpose_client_auth()`; `verify` no longer refusing an unreadable host
  before the chain.
- **No trap:** `tests/programs/fuzz_chain.cho` also runs `verify_chain` for both purposes and walks `san_next` over every
  corpus input, on both backends.

### 10.5 Results

**What was built.**

| File | What |
|---|---|
| `packages/x509/verify.cho` (914 lines) | `purpose_server_auth`, `purpose_client_auth`, `verify_chain`, `verify_name`, `san_next`, the tag `x509-purpose` (-41); `verify` is now the host check, `chain` (the shared path) for `serverAuth`, and `verify_name`. `leaf_ok`, `issuer_ok` and `extend` take the purpose's EKU flag where they read an EKU |
| `tests/programs/x509_verify_driver.cho` | `C` lines (`verify_chain`, then the subject and every SAN through `san_next`) and `N` lines (`verify_name` alone) |
| `scripts/x509_matrix.py chain` | the no-name matrix of §10.4, against `openssl verify -x509_strict -purpose sslclient` / `sslserver` |
| `tests/vectors/x509/verify/chain_matrix.txt` | its 56 cases, replayed by `conformance/x509_verify.rs` on both backends |
| `tests/programs/fuzz_chain.cho` | also `verify_chain` for both purposes, `san_next` and `verify_name` over every input |

**Evidence.**
- **`verify` unchanged.** The 33-case matrix, the 14 real chains (84 checks) and the limbo subset (705 answers) replay
  byte for byte through the new code, on both backends; so do `conformance/tls.rs` and the TLS fuzz corpora.
  `publish_packages.py --check` passes with the stores republished.
- **The no-name matrix**: 56 cases, each with its own tag; OpenSSL 3.6.4 (Homebrew) beside each of the 39 chain cases:
  - accepted for both purposes: a leaf with both EKUs, with none, with no SAN (`device-17`, the subject only), a client leaf
    under `pathlen:0`, a constrained CA with the SAN inside it, an RSA-2048 leaf; each answer carries the subject's DER and
    the SAN entries, equal to what the script put in the certificate (`DNS`, `IP` and `email` entries read back in order);
  - `x509-key-usage`: a client-auth-only leaf for `serverAuth` and a server-auth-only leaf for `clientAuth` (OpenSSL: 26),
    `anyExtendedKeyUsage` alone (26), an intermediate whose EKU excludes the purpose (26), a root whose EKU excludes
    `clientAuth` (26), an intermediate without keyCertSign (79), and a leaf with keyUsage `keyAgreement` only for
    `clientAuth` (OpenSSL accepts it: refused here, §10.2);
  - `x509-expired` (10) for a leaf and an intermediate, `x509-not-yet-valid` (9), `x509-unknown-issuer` for another CA's
    chain (19) and a self-signed leaf (18), `x509-path-too-long` (25), `x509-not-ca` (79), `x509-name-constraint` for a
    SAN outside a constrained CA with no name asked (47), `x509-bad-signature` (7);
  - `verify` with an empty host, `-`, `*` or a space: `x509-name-mismatch`, and before an expired chain is read;
  - `verify_name` alone: the leaf's name in any case with a trailing dot, an IP SAN, a wrong name, an IP the leaf does not
    have, an empty host, a leaf with no SAN;
  - purposes 0, 3, -1 and 4: `x509-purpose`.
  
  No chain OpenSSL refuses is accepted, but one, listed in the file as a known disagreement: **a root whose EKU is
  `clientAuth` only, under `serverAuth`, is accepted by `verify` and `verify_chain` and refused by OpenSSL (26).** That is
  §4's existing rule for a server's chain ("not read", as webpki has it), which this PR does not change because `verify`
  must answer as before. Whether a server's purpose should read the root's EKU too is a question for a person. Measured:
  4 of the 128 system roots (`roots.pem`) carry an EKU, and each is `serverAuth` alone, so reading it would refuse no chain to
  a system root; it would make `verify-ca` and `verify-full` agree with libpq's OpenSSL on a private root restricted to
  another purpose. (The same 4 roots would refuse every client certificate under `clientAuth`: one more reason a
  client-certificate store is never the system bundle, §10.3.)
- **Mutants: 33 of 33 killed** (the 22 of §8.2, with three texts moved to the new code, and 11 new).
- **No trap:** 20,000 chains from the no-name matrix with random byte changes, through `C`, `N` and `V` lines, ended with no
  trap (a scratch run, not committed); and the committed chain corpus, now through `verify_chain`, on both backends.

**Not done here.** `packages/tls` does not offer `verify-ca`, and the server's step 4 is not built: each is its own change, in
`packages/tls`, asked for by its program. x509-limbo's 10 `CLIENT` cases (§8.2's "a client certificate", 5 of them not
revocation cases) could now run through `verify_chain`; that needs `limbo.json`, which is fetched rather than committed, and
was not run for this PR.
