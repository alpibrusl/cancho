#!/usr/bin/env python3
"""`packages/x509` over every certificate in x509-limbo (docs/x509.md §5.3).

    curl -sSLO https://raw.githubusercontent.com/C2SP/x509-limbo/main/limbo.json
    python3 scripts/x509_limbo.py <driver> limbo.json

`driver` is `tests/programs/x509_driver.ls` built with `--std` and
`packages/x509/x509.ls`. x509-limbo's cases are about path validation
(#206), not parsing, so each one is used here only as a source of
certificates: every distinct certificate in it is parsed, and each refusal
is listed with the cases that use it and what those cases expect. Also
compared: whether pyca/cryptography loads the same certificate.

Exit status 1 when a case that expects SUCCESS has its leaf refused, or when
a certificate pyca refuses is accepted here.
"""
import collections
import json
import subprocess
import sys
import warnings

from cryptography import x509

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


if __name__ == "__main__":
    main()
