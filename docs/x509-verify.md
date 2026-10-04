# `packages/x509`: verifying a server's chain

> **Status: design (#206, PR 1 of 3).** Sub-issue 9 of the self-contained TLS 1.3 client (#197). `docs/tls-pure.md` §5 already
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
  because x509-limbo sets it per case (§6.1).
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
  Exhausting the budget is `x509-path-too-long`.
- **Signatures are checked before anything else about a candidate.** A candidate whose key does not verify is not an issuer.
  That is the only way to tell two certificates with the same subject apart.

## 4. What each certificate is checked for

`docs/tls-pure.md` §5.2 lists the checks. These are the decisions it left open.

| Certificate | Signature | Validity | `cA` and `pathLen` | keyUsage | EKU | Name constraints |
|---|---|---|---|---|---|---|
| leaf | checked by its issuer's key | checked | not read, so `pathlen::validation-ignores-pathlen-in-leaf` holds | `digitalSignature` if present | `serverAuth` if present; `anyExtendedKeyUsage` alone is refused, as OpenSSL's server purpose does | subject to every issuer's |
| intermediate | checked | checked | `cA` true; `pathLen` against the intermediates below it, self-issued ones not counted (RFC 5280 §6.1.4) | `keyCertSign` if present | `serverAuth` if present (OpenSSL and webpki both refuse an intermediate whose EKU excludes it) | its own apply below it |
| root | **not checked** (§5.1 of `tls-pure.md`) | **not checked** | `cA` true, except an X.509 v1 root, which has no extensions; `pathLen` as an intermediate's | `keyCertSign` if present | not read | its own apply below it |

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
  wildcard SAN `*.a.example` is checked as `a.example` against permitted subtrees, and as itself against excluded ones, as
  webpki does. The subject's common name is never read (no CN fallback, `docs/tls-pure.md` §5.3), so it is never constrained.
- **Permitted, then excluded.** A name of a type that has permitted subtrees must match one of them, and it must match none of
  the excluded subtrees.
- **A subtree of any other type**, `rfc822Name`, `directoryName`, URI or `otherName`, is unreadable: the chain is refused with
  `x509-name-constraint`. This is RFC 5280 §4.2.1.10's rule for a critical extension the verifier cannot apply, and
  `docs/tls-pure.md` §8 already maps OpenSSL's 51 to 53 to that tag. It costs every x509-limbo case whose constraint has one
  of those types, and §6.1 counts them.
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

- Whether byte-for-byte name comparison loses a real chain. x509-limbo has cases for it, and §6.1 will list them.
- Whether `*.co.uk`-style wildcards need a public-suffix list. That is `docs/tls-pure.md` §10's question 5; limbo's
  `pedantic-public-suffix-wildcard` cases (3) will be listed as disagreements if they fail, not hidden.
