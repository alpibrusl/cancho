"""The client PKI of the examples' tests (docs/tls-server.md §13.10): a CA, client certificates with subjects that need
escaping, a stranger's, and what OpenSSL reports for a certificate, to compare with the line a server logs.

Used by `scripts/tls_echo_test.py` and `scripts/https_hello_test.py`; not run on its own.
"""
import datetime
import subprocess

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID


def pem(cert):
    return cert.public_bytes(serialization.Encoding.PEM)


def key_pem(key):
    return key.private_bytes(serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption())


def make_ca(name):
    now = datetime.datetime.now(datetime.timezone.utc)
    key = ec.generate_private_key(ec.SECP256R1())
    subject = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, name)])
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(subject).public_key(key.public_key())
            .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(days=1))
            .not_valid_after(now + datetime.timedelta(days=30))
            .add_extension(x509.BasicConstraints(ca=True, path_length=None), True)
            .add_extension(x509.KeyUsage(False, False, False, False, False, True, True, False, False), True)
            .sign(key, hashes.SHA256()))
    return key, cert


def make_client(ca, attributes, dns="device.fleet.test"):
    """A client certificate of `ca` = (key, cert) with the subject attributes `attributes` (oid, value) in order."""
    now = datetime.datetime.now(datetime.timezone.utc)
    key = ec.generate_private_key(ec.SECP256R1())
    subject = x509.Name([x509.NameAttribute(oid, value) for oid, value in attributes])
    cert = (x509.CertificateBuilder().subject_name(subject).issuer_name(ca[1].subject).public_key(key.public_key())
            .serial_number(x509.random_serial_number()).not_valid_before(now - datetime.timedelta(days=1))
            .not_valid_after(now + datetime.timedelta(days=30))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName(dns)]), False)
            .add_extension(x509.KeyUsage(True, False, False, False, False, False, False, False, False), True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.CLIENT_AUTH]), False)
            .sign(ca[0], hashes.SHA256()))
    return key, cert


# Subjects that need the escapes of RFC 4514, as `openssl x509 -nameopt RFC2253` writes them.
PLAIN = [(NameOID.COUNTRY_NAME, "ES"), (NameOID.ORGANIZATION_NAME, "Fleet S.L."), (NameOID.COMMON_NAME, "device-17")]
AWKWARD = [(NameOID.COUNTRY_NAME, "ES"), (NameOID.ORGANIZATION_NAME, "Café, \"Fleet\" + Co"),
           (NameOID.ORGANIZATIONAL_UNIT_NAME, "#lead "), (NameOID.COMMON_NAME, "device;<17>\\x")]


def openssl_view(cert_pem_path):
    """(subject as RFC 2253, the first 16 hex digits of the SHA-256 fingerprint) as OpenSSL reports them."""
    out = subprocess.run(["openssl", "x509", "-in", cert_pem_path, "-noout", "-subject", "-nameopt", "RFC2253",
                          "-fingerprint", "-sha256"], capture_output=True, check=True).stdout.decode()
    subject = next(l for l in out.splitlines() if l.startswith("subject=")).partition("=")[2].strip()
    fp = next(l for l in out.splitlines() if "Fingerprint=" in l).partition("=")[2].replace(":", "").lower()
    return subject, fp[:16]
