#!/usr/bin/env python3
"""`packages/x509` over every certificate in x509-limbo (docs/x509.md §5.3).

    curl -sSLO https://raw.githubusercontent.com/C2SP/x509-limbo/main/limbo.json
    python3 scripts/x509_limbo.py <driver> limbo.json
    python3 scripts/x509_limbo.py verify <verify driver> limbo.json [<subset.txt>]

`driver` is `tests/programs/x509_driver.ls` built with `--std` and
`packages/x509/x509.ls`. x509-limbo's cases are about path validation
(#206), not parsing, so each one is used here only as a source of
certificates: every distinct certificate in it is parsed, and each refusal
is listed with the cases that use it and what those cases expect. Also
compared: whether pyca/cryptography loads the same certificate.

Exit status 1 when a case that expects SUCCESS has its leaf refused, or when
a certificate pyca refuses is accepted here.

`verify` runs every case through `tests/programs/x509_verify_driver.ls`
(built with `--std`, `packages/x509/verify.ls`, `names.ls` and `x509.ls`),
as docs/x509-verify.md §6.1 says: the trusted certificates as the store, the
leaf and the intermediates as the chain, `expected_peer_name` as the host,
`validation_time` (now, when null) as the time, `max_chain_depth` (6 when
null) as the most intermediates. Each case is a pass, not applicable (a kind
this verifier leaves out, named), or a disagreement, listed. With
`<subset.txt>`, every case that is not BetterTLS and one BetterTLS case in
forty (by a hash of its id) are written with the driver's answers, grouped
by store so that a store line is written only when the store changes, for
`conformance/x509_verify.rs` to replay. Exit status 1 on a disagreement
that is not listed in `KNOWN` with its reason.
"""
import collections
import datetime
import hashlib
import json
import time
import subprocess
import sys
import warnings

from cryptography import x509
from cryptography.hazmat.primitives import serialization

warnings.simplefilter("ignore")


def main():
    driver, path = sys.argv[1], sys.argv[2]
    cases = json.load(open(path))["testcases"]
    pems, index, uses = [], {}, collections.defaultdict(list)
    for case in cases:
        roles = [("trusted", p) for p in case["trusted_certs"]]
        roles += [("intermediate", p) for p in case["untrusted_intermediates"]]
        roles.append(("leaf", case["peer_certificate"]))
        for role, pem in roles:
            if pem not in index:
                index[pem] = len(pems)
                pems.append(pem)
            uses[index[pem]].append((case["id"], case["expected_result"], role))
    text = ("\n".join(pems) + "\n").encode()
    lines = subprocess.run([driver], input=text, capture_output=True, check=True).stdout.decode().splitlines()
    assert len(lines) == len(pems), (len(lines), len(pems))
    bad = 0
    tags = collections.Counter()
    by_case = collections.defaultdict(collections.Counter)
    for n, (pem, line) in enumerate(zip(pems, lines)):
        tag = line.split(" ")[1]
        tags[tag] += 1
        try:
            x509.load_pem_x509_certificate(pem.encode())
            pyca = True
        except ValueError:
            pyca = False
        if tag == "ok" and not pyca:
            print(f"accepted here, refused by pyca: {uses[n][0][0]}")
            bad += 1
        if tag == "ok":
            continue
        for case, result, role in uses[n]:
            family = case.split("::")[0] if case.startswith("bettertls") else case
            by_case[(family, result, role)][tag] += 1
            if result == "SUCCESS" and role == "leaf":
                bad += 1
    print(f"{len(cases)} cases, {len(pems)} distinct certificates")
    print("  " + ", ".join(f"{t} {c}" for t, c in tags.most_common()))
    print("refusals, by the case that uses the certificate:")
    for (case, result, role), counts in sorted(by_case.items()):
        print(f"  {case} (expects {result}, as {role}): " + ", ".join(f"{t} {c}" for t, c in counts.items()))
    print(f"{bad} problems")
    sys.exit(1 if bad else 0)


# Disagreements read and explained in docs/x509-verify.md §8, by case id. Each
# reason was checked against `openssl verify -purpose sslserver` (OpenSSL
# 3.0.13), with and without `-x509_strict`.
_PURPOSE = "the leaf's keyUsage has no digitalSignature; OpenSSL's server purpose refuses it too"
_ANCHOR = ("OpenSSL refuses only because, by default, its anchor must be self-issued by its own AKI; "
           "this verifier trusts a stored root as it is (tls-pure.md §5.1)")
_OPENSSL = "accepted by OpenSSL's default verification too"
_STRICT = "accepted by OpenSSL too; refused only with -x509_strict"
_CABF = "a CA/Browser Forum or webpki rule beyond RFC 5280; OpenSSL accepts it too"
_SUFFIX = "a wildcard on a public suffix: no suffix list here (tls-pure.md §10 question 5); OpenSSL accepts it too"
KNOWN = {
    "pathlen::validation-ignores-pathlen-in-leaf": _PURPOSE,
    "rfc5280::ca-as-leaf": _PURPOSE,
    "rfc5280::aki::cross-signed-root-missing-aki": _ANCHOR,
    "webpki::aki::root-with-aki-authoritycertissuer": _ANCHOR,
    "webpki::aki::root-with-aki-authoritycertserialnumber": _ANCHOR,
    "webpki::aki::root-with-aki-all-fields": _ANCHOR,
    "webpki::aki::root-with-aki-ski-mismatch": _ANCHOR,
    "webpki::aki::root-with-aki-missing-keyidentifier": _OPENSSL,
    "rfc5280::aki::leaf-missing-aki": _STRICT,
    "rfc5280::aki::intermediate-missing-aki": _STRICT,
    "rfc5280::ski::root-missing-ski": _STRICT,
    "rfc5280::ski::intermediate-missing-ski": _STRICT,
    "rfc5280::root-non-critical-basic-constraints": _STRICT,
    "rfc5280::san::noncritical-with-empty-subject": _STRICT,
    "rfc5280::nc::permitted-dns-match-noncritical": _OPENSSL,
    "rfc5280::nc::invalid-dnsname-leading-period": _OPENSSL + "; a leading period is read as subdomains only (§5.3)",
    "rfc5280::pc::ica-noncritical-pc": _OPENSSL,
    "rfc5280::san::underscore-dns": _OPENSSL,
    "rfc5280::serial::zero": _OPENSSL + " (docs/x509.md §3.1 accepts any serial)",
    "webpki::san::san-critical-with-nonempty-subject": _OPENSSL,
    "webpki::malformed-aia": _OPENSSL + "; AIA is not read",
    "webpki::cn::ipv4-hex-mismatch": _CABF,
    "webpki::cn::ipv4-leading-zeros-mismatch": _CABF,
    "webpki::cn::ipv6-uppercase-mismatch": _CABF,
    "webpki::cn::ipv6-uncompressed-mismatch": _CABF,
    "webpki::cn::ipv6-non-rfc5952-mismatch": _CABF,
    "webpki::cn::punycode-not-in-san": _CABF,
    "webpki::cn::utf8-vs-punycode-mismatch": _CABF,
    "webpki::cn::not-in-san": _CABF,
    "webpki::cn::case-mismatch": _CABF,
    "webpki::eku::ee-anyeku": _CABF + " (the leaf also has serverAuth)",
    "webpki::eku::ee-critical-eku": _CABF,
    "webpki::eku::ee-without-eku": _CABF,
    "webpki::eku::root-has-eku": _CABF + "; a root's EKU is not read (§4)",
    "webpki::forbidden-rsa-not-divisible-by-8-in-root": _CABF,
    "webpki::forbidden-rsa-key-not-divisible-by-8-in-leaf": _CABF,
    "webpki::ee-basicconstraints-ca": _CABF,
    "webpki::san::public-suffix-multi-label-wildcard-san": _SUFFIX,
    "webpki::san::public-suffix-private-namespace-wildcard-san": _SUFFIX,
}

OTHER_NC = (x509.RFC822Name, x509.DirectoryName, x509.UniformResourceIdentifier, x509.OtherName)


def nc_unreadable(pems):
    """Whether a certificate of the case constrains a name type §5.3 cannot read."""
    for pem in pems:
        try:
            c = x509.load_pem_x509_certificate(pem.encode())
            nc = c.extensions.get_extension_for_class(x509.NameConstraints).value
        except Exception:  # noqa: BLE001 -- no constraints, or not loadable
            continue
        for sub in (nc.permitted_subtrees or []) + (nc.excluded_subtrees or []):
            if isinstance(sub, OTHER_NC):
                return True
    return False


def algorithms(pems):
    out = set()
    for pem in pems:
        try:
            c = x509.load_pem_x509_certificate(pem.encode())
            out.add(c.signature_algorithm_oid.dotted_string)
            out.add(c.public_key_algorithm_oid.dotted_string)
        except Exception:  # noqa: BLE001
            out.add("unloadable")
    return out


ML_DSA_OR_DSA = {"2.16.840.1.101.3.4.3.17", "2.16.840.1.101.3.4.3.18", "2.16.840.1.101.3.4.3.19",
                 "1.2.840.10040.4.1", "2.16.840.1.101.3.4.3.2"}


def not_applicable(case, got):
    """The kind of case this verifier leaves out, or None."""
    if case["validation_kind"] != "SERVER":
        return "a client certificate (validation_kind CLIENT)"
    if not case.get("expected_peer_name"):
        return "no peer name: this verifier always matches a host"
    if "has-crl" in (case.get("features") or []):
        return "revocation (has-crl): not checked"
    pems = case["trusted_certs"] + case["untrusted_intermediates"] + [case["peer_certificate"]]
    if case["expected_result"] == "SUCCESS" and algorithms(pems) & ML_DSA_OR_DSA:
        return "ML-DSA or DSA: not supported"
    if case["expected_result"] == "SUCCESS" and got == "x509-name-constraint" and nc_unreadable(pems):
        return "a name constraint of a type this verifier cannot read"
    return None


def host_hex(case):
    peer = case.get("expected_peer_name")
    if not peer:
        return "-"
    return peer["value"].encode().hex()


def when(case):
    t = case.get("validation_time")
    if t is None:
        return int(time.time())
    return int(datetime.datetime.fromisoformat(t.replace("Z", "+00:00")).timestamp())


def der_hex(pem):
    return x509.load_pem_x509_certificate(pem.encode()).public_bytes(serialization.Encoding.DER).hex()


def pem_der_hex(pem):
    """The DER of a PEM block, decoded here only to pass it on as hex: a
    certificate pyca cannot load is passed by its base64 decoded by hand."""
    import base64
    body = "".join(l for l in pem.strip().splitlines() if not l.startswith("-----"))
    return base64.b64decode(body).hex()


def lines_for(case):
    store = "".join(case["trusted_certs"]).encode().hex()
    depth = case.get("max_chain_depth")
    chain = [case["peer_certificate"]] + case["untrusted_intermediates"]
    v = f"V {when(case)} {6 if depth is None else depth} {host_hex(case)} " + " ".join(pem_der_hex(p) for p in chain)
    return [f"S {store}", v]


def verify_main(driver, path, subset):
    cases = json.load(open(path))["testcases"]
    script = []
    per_case = []
    for case in cases:
        per_case.append(lines_for(case))
        script += per_case[-1]
    started = time.time()
    out = subprocess.run([driver], input=("\n".join(script) + "\n").encode(), capture_output=True, check=True).stdout.decode().splitlines()
    elapsed = time.time() - started
    assert len(out) == len(script), (len(out), len(script))
    results = collections.Counter()
    refused = collections.Counter()
    na = collections.Counter()
    disagreements = []
    chosen = []
    for n, case in enumerate(cases):
        answer = out[2 * n + 1]
        got = answer.split(" ")[1]
        expect = case["expected_result"]
        kind = not_applicable(case, got)
        if kind and kind.startswith("no peer name"):
            results["not applicable"] += 1
            na[kind] += 1
            klass = "na"
        elif (expect == "SUCCESS") == (got == "ok"):
            results["pass"] += 1
            if expect == "FAILURE":
                refused[got] += 1
            klass = "pass"
        elif kind:
            results["not applicable"] += 1
            na[kind] += 1
            klass = "na"
        else:
            results["disagreement"] += 1
            disagreements.append((case["id"], expect, got))
            klass = "disagreement"
        bettertls = case["id"].startswith("bettertls")
        if not bettertls or hashlib.sha256(case["id"].encode()).digest()[0] < 7:
            chosen.append((case, klass, out[2 * n], answer, per_case[n]))
    print(f"{len(cases)} cases in {elapsed:.1f} s: " + ", ".join(f"{k} {v}" for k, v in results.most_common()))
    print("FAILURE cases refused, by tag: " + ", ".join(f"{t} {c}" for t, c in refused.most_common()))
    print("not applicable: " + "; ".join(f"{k} ({v})" for k, v in na.most_common()))
    unexplained = 0
    for cid, expect, got in disagreements:
        why = KNOWN.get(cid)
        print(f"  disagreement: {cid} expects {expect}, got {got}" + (f" -- {why}" if why else ""))
        unexplained += why is None
    if subset:
        with open(subset, "w") as f:
            f.write("# scripts/x509_limbo.py verify: x509-limbo cases (every one not BetterTLS, one BetterTLS in forty)\n")
            f.write("# `## <id> <expected> <pass|na|disagreement>`, then the driver's lines, each answer after `= `.\n")
            f.write("# A case with no `S` line uses the store of the case before it.\n")
            last = None
            for case, klass, s_answer, v_answer, (s_line, v_line) in sorted(chosen, key=lambda c: (c[4][0], c[0]["id"])):
                # A null time is the run's, recorded in the V line, so the replay is fixed.
                f.write(f"## {case['id']} {case['expected_result']} {klass}\n")
                if s_line != last:
                    f.write(f"{s_line}\n= {s_answer}\n")
                    last = s_line
                f.write(f"{v_line}\n= {v_answer}\n")
        print(f"{subset}: {len(chosen)} cases")
    print(f"{unexplained} unexplained disagreements")
    sys.exit(1 if unexplained else 0)


if __name__ == "__main__":
    if sys.argv[1] == "verify":
        verify_main(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None)
    else:
        main()
