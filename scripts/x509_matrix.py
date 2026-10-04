#!/usr/bin/env python3
"""A matrix of chains made with the OpenSSL CLI (docs/x509-verify.md §6.2).

    python3 scripts/x509_matrix.py <verify driver>

Every certificate is made by `openssl req` and `openssl ca` (OpenSSL 3.0.13),
with fixed dates, under one root that is the store. Each case is a leaf, its
intermediates, a host and the tag it must get. For each case, both run:
- the verifier, through `tests/programs/x509_verify_driver.ls`;
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


def leaf_ext(san="DNS:leaf.example.com", eku="serverAuth", more=""):
    return LEAF_EXT.format(eku=eku, san=f"subjectAltName={san}\n" if san else "", more=more)


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


if __name__ == "__main__":
    main()
