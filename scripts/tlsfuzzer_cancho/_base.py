"""The shared conversation builders of `scripts/tlsfuzzer_cancho/` (not a test
itself; the runner only runs files named `test-*.py`).

This server's surface, which the stock scripts' assumptions do not match: a
P-256 identity, so `signature_algorithms` offers `ecdsa_secp256r1_sha256`;
no session tickets, so no `ExpectNewSessionTicket`; the compatibility CCS
(RFC 8446 Appendix D.4), so `ExpectChangeCipherSpec` in its place.
"""
from tlsfuzzer.messages import Connect, ClientHelloGenerator, \
    FinishedGenerator, ApplicationDataGenerator, AlertGenerator
from tlsfuzzer.expect import ExpectServerHello, ExpectEncryptedExtensions, \
    ExpectCertificate, ExpectCertificateVerify, ExpectFinished, \
    ExpectApplicationData, ExpectAlert, ExpectClose, ExpectChangeCipherSpec
from tlslite.constants import CipherSuite, GroupName, SignatureScheme, \
    AlertLevel, AlertDescription as AD, ExtensionType
from tlsfuzzer.helpers import key_share_gen
from tlslite.extensions import ClientKeyShareExtension, SupportedGroupsExtension, \
    SupportedVersionsExtension, SignatureAlgorithmsExtension, \
    SignatureAlgorithmsCertExtension

CIPHERS = [CipherSuite.TLS_AES_128_GCM_SHA256,
           CipherSuite.TLS_EMPTY_RENEGOTIATION_INFO_SCSV]
GROUPS = [GroupName.secp256r1]
SIG_ALGS = [SignatureScheme.ecdsa_secp256r1_sha256]


def client_hello(extensions=None, ciphers=None, with_key_share=True):
    """A ClientHello this server accepts (or the caller's `extensions`)."""
    ext = {} if extensions is None else dict(extensions)
    if ExtensionType.supported_versions not in ext:
        ext[ExtensionType.supported_versions] = \
            SupportedVersionsExtension().create([(3, 4), (3, 3)])
    if ExtensionType.supported_groups not in ext:
        ext[ExtensionType.supported_groups] = SupportedGroupsExtension().create(GROUPS)
    if ExtensionType.signature_algorithms not in ext:
        ext[ExtensionType.signature_algorithms] = \
            SignatureAlgorithmsExtension().create(SIG_ALGS)
    if ExtensionType.signature_algorithms_cert not in ext:
        ext[ExtensionType.signature_algorithms_cert] = \
            SignatureAlgorithmsCertExtension().create(SIG_ALGS)
    if with_key_share and ExtensionType.key_share not in ext:
        ext[ExtensionType.key_share] = ClientKeyShareExtension().create(
            [key_share_gen(g) for g in GROUPS])
    return ClientHelloGenerator(ciphers or CIPHERS, extensions=ext)


def established(conversation, payload=b"GET / HTTP/1.0\r\n\r\n"):
    """`conversation` through a full handshake, ending after the first
    application data comes back: the caller's node continues from there."""
    node = conversation
    node = node.add_child(client_hello())
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    node = node.add_child(FinishedGenerator())
    node = node.add_child(ApplicationDataGenerator(bytearray(payload)))
    node = node.add_child(ExpectApplicationData())
    return node


def close_clean(node):
    """The clean end: close_notify both ways, then the close."""
    node = node.add_child(AlertGenerator(AlertLevel.warning, AD.close_notify))
    node = node.add_child(ExpectAlert(AlertLevel.warning, AD.close_notify))
    return node.add_child(ExpectClose())


def refused(conversation, alert):
    """`conversation` from its hello, expecting the fatal `alert` and close."""
    node = conversation
    node = node.add_child(client_hello())
    node = node.add_child(ExpectAlert(AlertLevel.fatal, alert))
    return node.add_child(ExpectClose())
