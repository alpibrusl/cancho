#!/usr/bin/env python3
"""A matrix of chains made with the OpenSSL CLI (docs/x509-verify.md §6.2).

    python3 scripts/x509_matrix.py <verify driver>
    python3 scripts/x509_matrix.py chain <verify driver>    # §10.4: no name, both purposes

Every certificate is made by `openssl req` and `openssl ca` (OpenSSL 3.0.13),
with fixed dates, under one root that is the store. Each case is a leaf, its
intermediates, a host and the tag it must get. For each case, both run:
- the verifier, through `tests/programs/x509_verify_driver.cho`;
- `openssl verify -x509_strict -purpose sslserver -attime <t> -CAfile root
  -untrusted <intermediates> -verify_hostname <host> (or -verify_ip) leaf`.

Each case must get its own tag, and no case OpenSSL refuses may be accepted.
Writes tests/vectors/x509/verify/matrix.txt: each case's name, its tag, what
OpenSSL said, and the driver's lines and answers, so the replay in
`conformance/x509_verify.rs` needs no OpenSSL. The keys are random, so a second
run writes different certificates with the same answers. Exit status 1 on any
difference.
"""
import os
import subprocess
import sys
import tempfile

T = "20260601000000Z"  # the time every case is checked at
AT = 1780272000  # the same, in seconds
VALID = ("20260101000000Z", "20360101000000Z")
CFG = """[ ca ]
default_ca = d
[ d ]
database = {d}/index.txt
new_certs_dir = {d}/new
serial = {d}/serial
default_md = sha256
policy = p
email_in_dn = no
unique_subject = no
copy_extensions = none
[ p ]
commonName = supplied
[ req ]
distinguished_name = dn
prompt = no
[ dn ]
CN = x
"""
CA_EXT = "basicConstraints=critical,CA:TRUE{pl}\nkeyUsage=critical,{ku}\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid\n{more}"
LEAF_EXT = ("basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage={eku}\n"
            "subjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid\n{san}{more}")


def sh(*cmd, cwd=None):
    r = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd)
    if r.returncode != 0:
        raise RuntimeError(" ".join(cmd) + "\n" + r.stderr)
    return r.stdout


class Maker:
    def __init__(self, d):
        self.d = d
        os.makedirs(f"{d}/new")
        open(f"{d}/index.txt", "w").close()
        open(f"{d}/serial", "w").write("1000\n")
        open(f"{d}/cfg", "w").write(CFG.format(d=d))
        self.n = 0

    def key(self, kind):
        self.n += 1
        path = f"{self.d}/k{self.n}.pem"
        if kind.startswith("rsa"):
            sh("openssl", "genpkey", "-algorithm", "RSA", "-pkeyopt", f"rsa_keygen_bits:{kind[3:]}", "-out", path)
        elif kind == "ed25519":
            sh("openssl", "genpkey", "-algorithm", "ED25519", "-out", path)
        else:
            sh("openssl", "genpkey", "-algorithm", "EC", "-pkeyopt", f"ec_paramgen_curve:{kind}", "-out", path)
        return path

    def cert(self, cn, key, issuer, ext, dates=VALID, md="sha256", sigopt=()):
        """`issuer` is (cert, key), or None for self-signed."""
        self.n += 1
        csr, out, extf = f"{self.d}/r{self.n}.csr", f"{self.d}/c{self.n}.pem", f"{self.d}/e{self.n}.cnf"
        open(extf, "w").write("[ v ]\n" + ext)
        sh("openssl", "req", "-new", "-config", f"{self.d}/cfg", "-key", key, "-subj", f"/CN={cn}", "-out", csr)
        cmd = ["openssl", "ca", "-batch", "-notext", "-config", f"{self.d}/cfg", "-in", csr, "-out", out,
               "-startdate", dates[0], "-enddate", dates[1], "-extfile", extf, "-extensions", "v"]
        if md:
            cmd += ["-md", md]
        for o in sigopt:
            cmd += ["-sigopt", o]
        if issuer is None:
            cmd += ["-selfsign", "-keyfile", key]
        else:
            cmd += ["-cert", issuer[0], "-keyfile", issuer[1]]
        sh(*cmd)
        return out


def ca_ext(pathlen=None, ku="keyCertSign,cRLSign", more=""):
    pl = "" if pathlen is None else f",pathlen:{pathlen}"
    return CA_EXT.format(pl=pl, ku=ku, more=more)


def leaf_ext(san="DNS:leaf.example.com", eku="serverAuth", more="", ku="digitalSignature"):
    text = LEAF_EXT.format(eku=eku, san=f"subjectAltName={san}\n" if san else "", more=more)
    if eku is None:
        text = text.replace("extendedKeyUsage=None\n", "")
    return text.replace("keyUsage=critical,digitalSignature\n", f"keyUsage=critical,{ku}\n")


def der(pem_path):
    return subprocess.run(["openssl", "x509", "-in", pem_path, "-outform", "DER"], capture_output=True, check=True).stdout


def build(m):
    root_key = m.key("prime256v1")
    root = m.cert("Matrix Root", root_key, None, ca_ext())
    ica_key = m.key("prime256v1")
    ica = m.cert("Matrix Intermediate", ica_key, (root, root_key), ca_ext())
    ica_pair = (ica, ica_key)
    cases = []

    def leaf(name, host, tag, key="prime256v1", ext=None, issuer=None, chain=None, **kw):
        k = m.key(key)
        c = m.cert(host if not host[0].isdigit() and ":" not in host else "ip", k, issuer or ica_pair, ext or leaf_ext(), **kw)
        cases.append((name, host, tag, [c] + (chain if chain is not None else [ica])))
        return c

    leaf("valid", "leaf.example.com", "ok")
    leaf("valid, a wildcard", "x.example.com", "ok", ext=leaf_ext(san="DNS:*.example.com"))
    leaf("valid, an IPv4 address", "192.0.2.7", "ok", ext=leaf_ext(san="IP:192.0.2.7"))
    leaf("valid, an IPv6 address", "2001:db8::7", "ok", ext=leaf_ext(san="IP:2001:db8::7"))
    leaf("valid, RSA-4096 and SHA-512", "leaf.example.com", "ok", key="rsa4096", md="sha512")
    leaf("valid, Ed25519", "leaf.example.com", "ok", key="ed25519")
    leaf("expired", "leaf.example.com", "x509-expired", dates=("20250101000000Z", "20260101000000Z"))
    leaf("not yet valid", "leaf.example.com", "x509-not-yet-valid", dates=("20270101000000Z", "20280101000000Z"))
    leaf("the wrong host", "other.example.com", "x509-name-mismatch")
    leaf("no subjectAltName, the host only in CN", "leaf.example.com", "x509-name-mismatch", ext=leaf_ext(san=None))
    leaf("wildcard *.com", "a.com", "x509-name-mismatch", ext=leaf_ext(san="DNS:*.com"))
    leaf("wildcard a.*.b.com", "a.x.b.com", "x509-name-mismatch", ext=leaf_ext(san="DNS:a.*.b.com"))
    leaf("wildcard *.a.b.com against a.b.com", "a.b.com", "x509-name-mismatch", ext=leaf_ext(san="DNS:*.a.b.com"))
    leaf("wildcard across two labels", "x.y.example.com", "x509-name-mismatch", ext=leaf_ext(san="DNS:*.example.com"))
    leaf("EKU clientAuth only", "leaf.example.com", "x509-key-usage", ext=leaf_ext(eku="clientAuth"))
    leaf("EKU anyExtendedKeyUsage only", "leaf.example.com", "x509-key-usage", ext=leaf_ext(eku="anyExtendedKeyUsage"))
    leaf("an RSA-1024 key", "leaf.example.com", "x509-key-size", key="rsa1024")
    leaf("a P-192 key", "leaf.example.com", "x509-unsupported-algorithm", key="prime192v1")
    leaf("signed with SHA-1", "leaf.example.com", "x509-unsupported-algorithm", md="sha1")
    leaf("an unknown critical extension", "leaf.example.com", "x509-critical-extension",
         ext=leaf_ext(more="1.3.6.1.4.1.55555.1=critical,ASN1:NULL\n"))
    # The leaf signed by another key with the intermediate's name, sent with
    # the real intermediate: its AKI names the other key, so the real one is
    # not its issuer (OpenSSL: 20, no local issuer).
    other_key = m.key("prime256v1")
    impostor = m.cert("Matrix Intermediate", other_key, (root, root_key), ca_ext())
    leaf("signed by another key of the same name", "leaf.example.com", "x509-unknown-issuer",
         issuer=(impostor, other_key), chain=[ica])
    # A self-signed leaf.
    sk = m.key("prime256v1")
    ss = m.cert("leaf.example.com", sk, None, leaf_ext())
    cases.append(("self-signed", "leaf.example.com", "x509-unknown-issuer", [ss]))
    # An untrusted root.
    ok_ = m.key("prime256v1")
    other_root = m.cert("Other Root", ok_, None, ca_ext())
    ik = m.key("prime256v1")
    other_ica = m.cert("Other Intermediate", ik, (other_root, ok_), ca_ext())
    leaf("an untrusted root", "leaf.example.com", "x509-unknown-issuer", issuer=(other_ica, ik), chain=[other_ica, other_root])
    # Intermediates that may not issue.
    nk = m.key("prime256v1")
    not_ca = m.cert("Not A CA", nk, (root, root_key), leaf_ext(san=None))
    leaf("an intermediate that is not a CA", "leaf.example.com", "x509-not-ca", issuer=(not_ca, nk), chain=[not_ca])
    kk = m.key("prime256v1")
    no_sign = m.cert("No CertSign", kk, (root, root_key), ca_ext(ku="digitalSignature"))
    leaf("an intermediate without keyCertSign", "leaf.example.com", "x509-key-usage", issuer=(no_sign, kk), chain=[no_sign])
    pk = m.key("prime256v1")
    pl0 = m.cert("Pathlen Zero", pk, (root, root_key), ca_ext(pathlen=0))
    qk = m.key("prime256v1")
    below = m.cert("Below Pathlen Zero", qk, (pl0, pk), ca_ext())
    leaf("pathlen 0 exceeded", "leaf.example.com", "x509-path-too-long", issuer=(below, qk), chain=[below, pl0])
    ek = m.key("prime256v1")
    old_ica = m.cert("Expired Intermediate", ek, (root, root_key), ca_ext(), dates=("20200101000000Z", "20250101000000Z"))
    leaf("an expired intermediate", "leaf.example.com", "x509-expired", issuer=(old_ica, ek), chain=[old_ica])
    ck = m.key("prime256v1")
    excl = m.cert("Excluding", ck, (root, root_key), ca_ext(more="nameConstraints=critical,excluded;DNS:example.com\n"))
    leaf("a name constraint excluding the host", "leaf.example.com", "x509-name-constraint", issuer=(excl, ck), chain=[excl])
    pk2 = m.key("prime256v1")
    perm = m.cert("Permitting", pk2, (root, root_key), ca_ext(more="nameConstraints=critical,permitted;DNS:example.com\n"))
    leaf("a name constraint permitting the host", "leaf.example.com", "ok", issuer=(perm, pk2), chain=[perm])
    leaf("a name constraint permitting another name", "leaf.other.test", "x509-name-constraint", issuer=(perm, pk2), chain=[perm],
         ext=leaf_ext(san="DNS:leaf.other.test"))
    # A wildcard under an excluded subtree with a leading dot (#318): `*.a.example` names only proper subdomains of
    # `a.example`, all of which `.a.example` excludes; `.x.a.example` excludes none of the names it can match.
    dk = m.key("prime256v1")
    dotted = m.cert("Excluding Dotted", dk, (root, root_key), ca_ext(more="nameConstraints=critical,excluded;DNS:.a.example\n"))
    leaf("a wildcard under a leading-dot excluded subtree", "x.a.example", "x509-name-constraint", issuer=(dotted, dk), chain=[dotted],
         ext=leaf_ext(san="DNS:*.a.example"))
    dk2 = m.key("prime256v1")
    deeper = m.cert("Excluding Deeper", dk2, (root, root_key), ca_ext(more="nameConstraints=critical,excluded;DNS:.x.a.example\n"))
    leaf("a wildcard above a leading-dot excluded subtree", "y.a.example", "ok", issuer=(deeper, dk2), chain=[deeper],
         ext=leaf_ext(san="DNS:*.a.example"))
    # RSA intermediates: PKCS#1 v1.5 and PSS signatures.
    rk = m.key("rsa2048")
    rsa_ica = m.cert("RSA Intermediate", rk, (root, root_key), ca_ext())
    leaf("valid, RSA PKCS#1 v1.5 issuer", "leaf.example.com", "ok", issuer=(rsa_ica, rk), chain=[rsa_ica])
    leaf("valid, RSA-PSS issuer", "leaf.example.com", "ok", issuer=(rsa_ica, rk), chain=[rsa_ica],
         sigopt=("rsa_padding_mode:pss", "rsa_pss_saltlen:digest"))
    # A broken signature: the leaf's last signature byte changed.
    k = m.key("prime256v1")
    c = m.cert("leaf.example.com", k, ica_pair, leaf_ext())
    raw = bytearray(der(c))
    raw[-1] ^= 1
    bad = f"{m.d}/bad.der"
    open(bad, "wb").write(bytes(raw))
    bad_pem = f"{m.d}/bad.pem"
    sh("openssl", "x509", "-inform", "DER", "-in", bad, "-out", bad_pem)
    cases.append(("one bit of the signature changed", "leaf.example.com", "x509-bad-signature", [bad_pem, ica]))
    return root, cases


def openssl_says(root, chain, host):
    cmd = ["openssl", "verify", "-x509_strict", "-purpose", "sslserver", "-attime", str(AT), "-CAfile", root]
    if len(chain) > 1:
        un = chain[0] + ".untrusted"
        open(un, "w").write("".join(open(c).read() for c in chain[1:]))
        cmd += ["-untrusted", un]
    cmd += ["-verify_ip" if host[0].isdigit() or ":" in host else "-verify_hostname", host, chain[0]]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode == 0:
        return "ok"
    err = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith("error ")]
    return err[0].split(" ")[1] if err else "refused"


def main():
    driver = sys.argv[1]
    with tempfile.TemporaryDirectory() as d:
        root, cases = build(Maker(d))
        lines = [f"S {open(root, 'rb').read().hex()}"]
        for _, host, _, chain in cases:
            lines.append(f"V {AT} 6 {host.encode().hex()} " + " ".join(der(c).hex() for c in chain))
        out = subprocess.run([driver], input=("\n".join(lines) + "\n").encode(), capture_output=True, check=True).stdout.decode().splitlines()
        bad = 0
        rows = []
        for (name, host, tag, chain), line, got in zip(cases, lines[1:], out[1:]):
            ours = got.split(" ")[1]
            theirs = openssl_says(root, chain, host)
            fine = ours == tag and not (ours == "ok" and theirs != "ok")
            bad += not fine
            print(f"{'' if fine else 'WRONG '}{name}: {ours} (wanted {tag}); openssl {theirs}")
            rows.append(f"## {tag} openssl={theirs} {name}\n{line}\n= {got}\n")
        with open("tests/vectors/x509/verify/matrix.txt", "w") as f:
            f.write("# scripts/x509_matrix.py: chains made with the OpenSSL CLI, checked at 2026-06-01.\n")
            f.write("# `## <tag> openssl=<ok or its error number> <case>`, then the driver's line and its answer.\n")
            f.write(f"{lines[0]}\n= {out[0]}\n")
            f.writelines(rows)
        print(f"{len(cases)} cases, {bad} wrong")
        sys.exit(1 if bad else 0)


# ---- §10.4: chains checked with no name, for a purpose ----

PURPOSE = {"server": 1, "client": 2}
OPENSSL_PURPOSE = {"server": "sslserver", "client": "sslclient"}


def tlv(b, at):
    """(tag, content start, content end) of the DER TLV at `at`."""
    tag, n = b[at], b[at + 1]
    if n < 0x80:
        return tag, at + 2, at + 2 + n
    k = n & 0x7f
    length = int.from_bytes(b[at + 2:at + 2 + k], "big")
    return tag, at + 2 + k, at + 2 + k + length


def subject_hex(raw):
    """The subject Name's whole TLV, walked here apart from `x509.cho`."""
    _, cs, _ = tlv(raw, 0)  # Certificate
    _, p, _ = tlv(raw, cs)  # TBSCertificate's content
    fields = []
    while len(fields) < 6:
        t, s, e = tlv(raw, p)
        fields.append((p, e))
        p = e
    if raw[fields[0][0]] != 0xa0:  # a v1 certificate has no [0] version
        raise SystemExit("a v1 leaf")
    start, end = fields[5]
    return raw[start:end].hex()


def san_text(san):
    """What `san_next` must give back for an OpenSSL `subjectAltName=` value."""
    if not san:
        return "-"
    out = []
    for item in san.split(","):
        kind, value = item.split(":", 1)
        if kind == "DNS":
            out.append(f"130:{value.encode().hex()}")
        elif kind == "email":
            out.append(f"129:{value.encode().hex()}")
        elif kind == "IP":
            import ipaddress
            out.append(f"135:{ipaddress.ip_address(value).packed.hex()}")
        else:
            raise SystemExit(f"no expectation for {kind}")
    return ",".join(out)


def build_chain(m):
    root_key = m.key("prime256v1")
    root = m.cert("Chain Root", root_key, None, ca_ext())
    ica_key = m.key("prime256v1")
    ica = m.cert("Chain Intermediate", ica_key, (root, root_key), ca_ext())
    ica_pair = (ica, ica_key)
    cases = []

    def leaf(name, purposes, san="DNS:client.example.com", eku="clientAuth", issuer=None, chain=None, ku="digitalSignature",
             more="", cn="client.example.com", **kw):
        """`purposes` is {purpose: tag}; one case per purpose, each its own row."""
        k = m.key(kw.pop("key", "prime256v1"))
        c = m.cert(cn, k, issuer or ica_pair, leaf_ext(san=san, eku=eku, ku=ku, more=more), **kw)
        for purpose, tag in purposes.items():
            cases.append((f"{name} ({purpose})", purpose, tag, [c] + (chain if chain is not None else [ica]), san))
        return c

    both_ok = {"client": "ok", "server": "ok"}
    leaf("valid, clientAuth and serverAuth", both_ok, eku="clientAuth,serverAuth")
    leaf("client-auth only", {"client": "ok", "server": "x509-key-usage"}, eku="clientAuth")
    leaf("server-auth only", {"client": "x509-key-usage", "server": "ok"}, eku="serverAuth", san="DNS:db.example.com",
         cn="db.example.com")
    leaf("no EKU", both_ok, eku=None)
    leaf("anyExtendedKeyUsage only", {"client": "x509-key-usage", "server": "x509-key-usage"}, eku="anyExtendedKeyUsage")
    leaf("no subjectAltName: no name to match", both_ok, san=None, eku="clientAuth,serverAuth", cn="device-17")
    leaf("SANs of three kinds, read back", {"client": "ok"}, san="DNS:a.example.com,IP:192.0.2.1,email:dev@example.com")
    leaf("keyUsage keyAgreement only", {"client": "x509-key-usage"}, ku="keyAgreement")
    leaf("expired", {"client": "x509-expired", "server": "x509-expired"}, eku="clientAuth,serverAuth",
         dates=("20250101000000Z", "20260101000000Z"))
    leaf("not yet valid", {"client": "x509-not-yet-valid"}, dates=("20270101000000Z", "20280101000000Z"))
    leaf("an RSA-2048 leaf", {"client": "ok"}, key="rsa2048")
    # An unknown CA: a chain to another root.
    ok_ = m.key("prime256v1")
    other_root = m.cert("Other Chain Root", ok_, None, ca_ext())
    ik = m.key("prime256v1")
    other_ica = m.cert("Other Chain Intermediate", ik, (other_root, ok_), ca_ext())
    leaf("an unknown CA", {"client": "x509-unknown-issuer", "server": "x509-unknown-issuer"}, eku="clientAuth,serverAuth",
         issuer=(other_ica, ik), chain=[other_ica, other_root])
    sk = m.key("prime256v1")
    ss = m.cert("self.example.com", sk, None, leaf_ext(san="DNS:self.example.com", eku="clientAuth"))
    cases.append(("self-signed (client)", "client", "x509-unknown-issuer", [ss], "DNS:self.example.com"))
    # Path length, cA, keyCertSign, dates of an intermediate.
    pk = m.key("prime256v1")
    pl0 = m.cert("Chain Pathlen Zero", pk, (root, root_key), ca_ext(pathlen=0))
    qk = m.key("prime256v1")
    below = m.cert("Chain Below Pathlen Zero", qk, (pl0, pk), ca_ext())
    leaf("pathlen 0 exceeded", {"client": "x509-path-too-long", "server": "x509-path-too-long"}, eku="clientAuth,serverAuth",
         issuer=(below, qk), chain=[below, pl0])
    leaf("pathlen 0 kept", {"client": "ok"}, issuer=(pl0, pk), chain=[pl0])
    nk = m.key("prime256v1")
    not_ca = m.cert("Chain Not A CA", nk, (root, root_key), leaf_ext(san=None, eku="clientAuth"))
    leaf("an intermediate that is not a CA", {"client": "x509-not-ca"}, issuer=(not_ca, nk), chain=[not_ca])
    kk = m.key("prime256v1")
    no_sign = m.cert("Chain No CertSign", kk, (root, root_key), ca_ext(ku="digitalSignature"))
    leaf("an intermediate without keyCertSign", {"client": "x509-key-usage"}, issuer=(no_sign, kk), chain=[no_sign])
    ek = m.key("prime256v1")
    old_ica = m.cert("Chain Expired Intermediate", ek, (root, root_key), ca_ext(), dates=("20200101000000Z", "20250101000000Z"))
    leaf("an expired intermediate", {"client": "x509-expired"}, issuer=(old_ica, ek), chain=[old_ica])
    # An intermediate's EKU.
    sak = m.key("prime256v1")
    server_ica = m.cert("Chain Server-Only Intermediate", sak, (root, root_key), ca_ext(more="extendedKeyUsage=serverAuth\n"))
    leaf("an intermediate with EKU serverAuth only", {"client": "x509-key-usage", "server": "ok"},
         eku="clientAuth,serverAuth", issuer=(server_ica, sak), chain=[server_ica])
    cak = m.key("prime256v1")
    client_ica = m.cert("Chain Client-Only Intermediate", cak, (root, root_key), ca_ext(more="extendedKeyUsage=clientAuth\n"))
    leaf("an intermediate with EKU clientAuth only", {"client": "ok", "server": "x509-key-usage"},
         eku="clientAuth,serverAuth", issuer=(client_ica, cak), chain=[client_ica])
    # Name constraints hold with no name asked.
    ck = m.key("prime256v1")
    perm = m.cert("Chain Permitting", ck, (root, root_key), ca_ext(more="nameConstraints=critical,permitted;DNS:example.com\n"))
    leaf("a constrained CA, the SAN inside", {"client": "ok"}, issuer=(perm, ck), chain=[perm])
    leaf("a constrained CA, the SAN outside", {"client": "x509-name-constraint", "server": "x509-name-constraint"},
         eku="clientAuth,serverAuth", san="DNS:admin.other.test", issuer=(perm, ck), chain=[perm])
    # A broken signature.
    k = m.key("prime256v1")
    c = m.cert("client.example.com", k, ica_pair, leaf_ext(san="DNS:client.example.com", eku="clientAuth"))
    raw = bytearray(der(c))
    raw[-1] ^= 1
    open(f"{m.d}/chain-bad.der", "wb").write(bytes(raw))
    bad_pem = f"{m.d}/chain-bad.pem"
    sh("openssl", "x509", "-inform", "DER", "-in", f"{m.d}/chain-bad.der", "-out", bad_pem)
    cases.append(("one bit of the signature changed (client)", "client", "x509-bad-signature", [bad_pem, ica], "DNS:client.example.com"))
    # A root's EKU (§10.2): read for client certificates, as OpenSSL's `sslclient` reads it; not read for a server's
    # (§4), where OpenSSL refuses and this verifier accepts -- a known disagreement of `verify`, kept and listed.
    roots = [root]
    for only, other in [("serverAuth", "clientAuth"), ("clientAuth", "serverAuth")]:
        rk = m.key("prime256v1")
        r = m.cert(f"Chain Root EKU {only}", rk, None, ca_ext(more=f"extendedKeyUsage={only}\n"))
        roots.append(r)
        purposes = {"client": "x509-key-usage" if only == "serverAuth" else "ok",
                    "server": "ok"}
        leaf(f"a root with EKU {only} only", purposes, eku="clientAuth,serverAuth", issuer=(r, rk), chain=[])
    cases = [(("known disagreement: " + c[0]) if c[0] == "a root with EKU clientAuth only (server)" else c[0],) + c[1:]
             for c in cases]
    return roots, ica, cases


def openssl_chain_says(root, chain, purpose):
    """`root` is the store's PEM bundle, every root of the matrix."""
    cmd = ["openssl", "verify", "-x509_strict", "-purpose", OPENSSL_PURPOSE[purpose], "-attime", str(AT), "-CAfile", root]
    if len(chain) > 1:
        un = chain[0] + ".untrusted"
        open(un, "w").write("".join(open(c).read() for c in chain[1:]))
        cmd += ["-untrusted", un]
    cmd.append(chain[0])
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode == 0:
        return "ok"
    err = [l for l in (r.stdout + r.stderr).splitlines() if l.startswith("error ")]
    return err[0].split(" ")[1] if err else "refused"


def chain_main(driver):
    """Every case through `C` (verify_chain) and `openssl verify -purpose`; then the rows that keep the name a separate step:
    `V` with hosts that are no name, `N` (verify_name alone), and purposes that are neither."""
    with tempfile.TemporaryDirectory() as d:
        roots, ica, cases = build_chain(Maker(d))
        root = f"{d}/chain-roots.pem"
        open(root, "w").write("".join(open(r).read() for r in roots))
        lines = [f"S {open(root, 'rb').read().hex()}"]
        for _, purpose, _, chain, _ in cases:
            lines.append(f"C {AT} 6 {PURPOSE[purpose]} " + " ".join(der(c).hex() for c in chain))
        valid = cases[0][3]
        chain_hex = " ".join(der(c).hex() for c in valid)
        extra = []  # (name, tag, line)
        for host, why in [(b"", "the host empty"), (b"-", "the host `-`"), (b"*", "the host `*`"), (b" ", "the host a space")]:
            extra.append((f"verify cannot skip the name: {why}", "x509-name-mismatch", f"V {AT} 6 {host.hex() or '-'} {chain_hex}"))
        extra.append(("verify with the leaf's name", "ok", f"V {AT} 6 {b'client.example.com'.hex()} {chain_hex}"))
        expired = [c for c in cases if c[0] == "expired (client)"][0]
        extra.append(("verify: a host that is no name is refused before an expired chain is read", "x509-name-mismatch",
                      f"V {AT} 6 - " + " ".join(der(c).hex() for c in expired[3])))
        leaf_hex = der(valid[0]).hex()
        for host, tag in [(b"client.example.com", "ok"), (b"CLIENT.example.com.", "ok"), (b"other.example.com", "x509-name-mismatch"),
                          (b"192.0.2.1", "x509-name-mismatch"), (b"", "x509-name-mismatch")]:
            extra.append((f"verify_name alone: {host.decode() or 'empty'}", tag, f"N 0 0 {host.hex() or '-'} {leaf_hex}"))
        three = [c for c in cases if c[0].startswith("SANs of three kinds")][0]
        extra.append(("verify_name alone: an IP SAN", "ok", f"N 0 0 {b'192.0.2.1'.hex()} {der(three[3][0]).hex()}"))
        nosan = [c for c in cases if c[0].startswith("no subjectAltName")][0]
        extra.append(("verify_name alone: no SAN", "x509-name-mismatch", f"N 0 0 {b'device-17'.hex()} {der(nosan[3][0]).hex()}"))
        for p in [0, 3, -1, 4]:
            extra.append((f"purpose {p} is neither", "x509-purpose", f"C {AT} 6 {p} {chain_hex}"))
        lines += [l for _, _, l in extra]
        out = subprocess.run([driver], input=("\n".join(lines) + "\n").encode(), capture_output=True, check=True).stdout.decode().splitlines()
        bad = 0
        rows = []
        for (name, purpose, tag, chain, san), line, got in zip(cases, lines[1:], out[1:]):
            f = got.split(" ")
            ours = f[1]
            theirs = openssl_chain_says(root, chain, purpose)
            fine = ours == tag and not (ours == "ok" and theirs != "ok" and not name.startswith("known disagreement"))
            if ours == "ok":
                want = [subject_hex(der(chain[0])), san_text(san)]
                fine = fine and f[2:] == want
            bad += not fine
            print(f"{'' if fine else 'WRONG '}{name}: {' '.join(f[1:2])} (wanted {tag}); openssl {theirs}")
            rows.append(f"## {tag} openssl={theirs} {name}\n{line}\n= {got}\n")
        for (name, tag, line), got in zip(extra, out[1 + len(cases):]):
            ours = got.split(" ")[1]
            fine = ours == tag
            bad += not fine
            print(f"{'' if fine else 'WRONG '}{name}: {ours} (wanted {tag})")
            rows.append(f"## {tag} openssl=- {name}\n{line}\n= {got}\n")
        with open("tests/vectors/x509/verify/chain_matrix.txt", "w") as f:
            f.write("# scripts/x509_matrix.py chain: chains made with the OpenSSL CLI, checked at 2026-06-01 with no name\n")
            f.write("# (docs/x509-verify.md §10.4). `## <tag> openssl=<ok, its error number, or - for none> <case>`, then the\n")
            f.write("# driver's line and its answer. A `C` answer that is ok carries the subject and the SAN entries.\n")
            f.write(f"{lines[0]}\n= {out[0]}\n")
            f.writelines(rows)
        print(f"{len(cases) + len(extra)} cases, {bad} wrong")
        sys.exit(1 if bad else 0)


if __name__ == "__main__":
    if sys.argv[1] == "chain":
        chain_main(sys.argv[2])
    else:
        main()
