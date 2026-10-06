# `packages/x509`: verifying a server's chain

> **Status: built (#206, all three PRs): the verifier (§8), and `packages/tls` verifying with it (§9).** Sub-issue 9 of the self-contained TLS 1.3 client (#197). `docs/tls-pure.md` §5 already
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

`packages/x509/x509.ls` is 1,598 lines, so verification is two new modules beside it, each under 2,000 lines
(`crates/lex-sys/tests/files.rs`):

| File | Module | What |
|---|---|---|
| `packages/x509/names.ls` | `x509_names` | a host as a DNS name or an IP address; matching a SAN; reading and applying name constraints |
| `packages/x509/verify.ls` | `x509_verify` | the root store; building a path; the checks of `docs/tls-pure.md` §5.2 on each certificate |

```
// The store: the roots of a PEM bundle, each as a 3-byte length and its DER, the format of
// docs/tls-core.md §5's pins. A block that is not a certificate, or a certificate this parser
// refuses, is skipped and counted, never fatal (docs/x509.md §5.3).
x509_verify.store_load(pem, store, info) -> roots | refusal   // info[0] bytes used, info[1] skipped

// A chain as the server sent it: `certs` holds the DERs, `ranges` their [start, end) pairs,
// the leaf first. `host` is a DNS name or an IP literal. `now` is seconds since 1970.
x509_verify.verify(store, certs, ranges, host, now, max_intermediates) -> 0 | refusal
```

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
  `pathological::intermediate-cycle-*`.
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
certificate.

### 5.3 Name constraints

The issue names x509-limbo's BetterTLS cases as a gate. They are 9,572 of its 9,802 cases, and every one is about name
constraints. Among the 30,379 certificates of `limbo.json`, the constraint subtrees are 24,681 `dNSName`, 11,736
`iPAddress`, 12 `rfc822Name`, 5 `directoryName`, 2 URI and 2 `otherName` (measured with pyca, §6.1).

- **`dNSName` subtrees.** A name matches a subtree when it equals it, or ends with `.` followed by it, compared without case. A
  subtree with a leading dot (`.example.com`) matches only proper subdomains. An empty subtree matches every name.
- **`iPAddress` subtrees** are an address and a mask of the same length, 8 or 32 bytes. An address of the other family never
  matches. A mask that is not contiguous ones then zeros is unreadable (below).
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
| `packages/x509/names.ls` (`x509_names`) | 565 | the host as a DNS name or an IPv4 or IPv6 address; SAN matching; name constraints |
| `packages/x509/verify.ls` (`x509_verify`) | 778 | `store_load`, `verify`, the refusal tags; path building, the checks of §4, the signatures |
| `packages/x509/x509.ls` | 1,665 | four OIDs for RSASSA-PSS's parameters (SHA-256, -384, -512, MGF1), from `scripts/x509_oids.py` and checked against `openssl asn1parse`; `is_ip_literal`, moved here from `packages/tls/message.ls` so the two share one copy (`conformance/duplication.rs`) |
| `tests/programs/x509_verify_driver.ls` | 203 | a store line and chain lines, from standard input |
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
- **`tests/programs/tls_driver.ls`.** `C` takes a PEM root bundle and a time instead of pins. `tls_many` checks against
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

