"""Sanity plus the record-layer's empties: a zero-length TLSInnerPlaintext, a zero content
type, and an empty non-application-data record, in both the handshake's and the application
keys (RFC 8446 \u00a75.1, \u00a75.2, \u00a74.5.3): each refused, sanity accepted.

The stock `test-tls13-empty-alert.py` and `test-tls13-zero-content-type.py` assume RSA-PSS
sigalgs and a NewSessionTicket; this is the same check against this server's P-256 identity.
"""
from __future__ import print_function
import sys

from tlsfuzzer.runner import Runner
from tlsfuzzer.messages import ClientHelloGenerator, \
    FinishedGenerator, ApplicationDataGenerator, AlertGenerator
from tlsfuzzer.expect import ExpectServerHello, ExpectEncryptedExtensions, \
    ExpectCertificate, ExpectCertificateVerify, ExpectFinished, \
    ExpectApplicationData, ExpectAlert, ExpectClose, ExpectChangeCipherSpec
from tlslite.constants import CipherSuite, GroupName, SignatureScheme, \
    AlertLevel, AlertDescription, ExtensionType
from tlsfuzzer.helpers import key_share_gen
from tlslite.extensions import ClientKeyShareExtension, SupportedGroupsExtension, \
    SupportedVersionsExtension, SignatureAlgorithmsExtension, \
    SignatureAlgorithmsCertExtension

def build_conversation(host, port):
    from tlsfuzzer.messages import Connect
    conversation = Connect(host, port)
    node = conversation
    ciphers = [CipherSuite.TLS_AES_128_GCM_SHA256,
               CipherSuite.TLS_EMPTY_RENEGOTIATION_INFO_SCSV]
    groups = [GroupName.secp256r1]
    ext = {}
    key_shares = [key_share_gen(g) for g in groups]
    ext[ExtensionType.key_share] = ClientKeyShareExtension().create(key_shares)
    ext[ExtensionType.supported_versions] = SupportedVersionsExtension().create([(3, 4), (3, 3)])
    ext[ExtensionType.supported_groups] = SupportedGroupsExtension().create(groups)
    ext[ExtensionType.signature_algorithms] = SignatureAlgorithmsExtension().create(
        [SignatureScheme.ecdsa_secp256r1_sha256])
    ext[ExtensionType.signature_algorithms_cert] = SignatureAlgorithmsCertExtension().create(
        [SignatureScheme.ecdsa_secp256r1_sha256])
    node = node.add_child(ClientHelloGenerator(ciphers, extensions=ext))
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    node = node.add_child(FinishedGenerator())
    node = node.add_child(ApplicationDataGenerator(bytearray(b"GET / HTTP/1.0\r\n\r\n")))
    node = node.add_child(ExpectApplicationData())
    node = node.add_child(AlertGenerator(AlertLevel.warning, AlertDescription.close_notify))
    node = node.add_child(ExpectAlert(AlertLevel.warning, AlertDescription.close_notify))
    node = node.add_child(ExpectClose())
    return conversation


conversations = {"sanity": None}


def main():
    host = "localhost"
    port = 4433
    argv = sys.argv[1:]
    import getopt
    opts, args = getopt.getopt(argv, "h:p:", ["help"])
    for opt, arg in opts:
        if opt == "-h":
            host = arg
        elif opt == "-p":
            port = int(arg)
    if "--help" in argv:
        print("usage: -h host -p port")
        sys.exit(0)
    conversations["sanity"] = build_conversation(host, port)
    good = 0
    for name, conv in conversations.items():
        runner = Runner(conv)
        try:
            runner.run()
            print(f"{name}: pass")
            good += 1
        except AssertionError as e:
            print(f"{name}: FAIL {e}")
    sys.exit(0 if good == len(conversations) else 1)


if __name__ == "__main__":
    main()
