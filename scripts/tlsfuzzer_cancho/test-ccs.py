"""The record layer's other cases tlsfuzzer's stock scripts check, against this
server's P-256 identity and ECDSA sigalgs (the stock scripts offer RSA-PSS
only, so their sanity cannot negotiate here; `scripts/tlsfuzzer_cancho/__init__.py`):

- a ChangeCipherSpec record mid-handshake (RFC 8446 \u00a75.1, Appendix D.4):
  the middlebox CCS is allowed exactly once, before the client's Finished;
- a second CCS after the handshake has started in earnest, and a two-byte
  one: each unexpected_message (the stock `test-tls13-ccs.py`).
"""
from __future__ import print_function
import getopt
import sys

from tlsfuzzer.runner import Runner
from tlsfuzzer.messages import Connect, ClientHelloGenerator, \
    FinishedGenerator, ApplicationDataGenerator, AlertGenerator, \
    RawMessageGenerator
from tlsfuzzer.expect import ExpectServerHello, ExpectEncryptedExtensions, \
    ExpectCertificate, ExpectCertificateVerify, ExpectFinished, \
    ExpectApplicationData, ExpectAlert, ExpectClose, ExpectChangeCipherSpec
from tlslite.constants import CipherSuite, GroupName, SignatureScheme, \
    AlertLevel, AlertDescription as AD, ContentType, ExtensionType, \
    HandshakeType
from tlsfuzzer.helpers import key_share_gen
from tlslite.extensions import ClientKeyShareExtension, SupportedGroupsExtension, \
    SupportedVersionsExtension, SignatureAlgorithmsExtension, \
    SignatureAlgorithmsCertExtension

conversations = {}


def start(host, port):
    conv = Connect(host, port)
    node = conv
    ciphers = [CipherSuite.TLS_AES_128_GCM_SHA256,
               CipherSuite.TLS_EMPTY_RENEGOTIATION_INFO_SCSV]
    groups = [GroupName.secp256r1]
    ext = {}
    ext[ExtensionType.key_share] = ClientKeyShareExtension().create(
        [key_share_gen(g) for g in groups])
    ext[ExtensionType.supported_versions] = SupportedVersionsExtension().create([(3, 4), (3, 3)])
    ext[ExtensionType.supported_groups] = SupportedGroupsExtension().create(groups)
    ext[ExtensionType.signature_algorithms] = SignatureAlgorithmsExtension().create(
        [SignatureScheme.ecdsa_secp256r1_sha256])
    ext[ExtensionType.signature_algorithms_cert] = SignatureAlgorithmsCertExtension().create(
        [SignatureScheme.ecdsa_secp256r1_sha256])
    node = node.add_child(ClientHelloGenerator(ciphers, extensions=ext))
    return conv, node


def build(host, port):
    # sanity: the plain conversation, CCS once, as Appendix D.4 sends it.
    conv, node = start(host, port)
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    node = node.add_child(RawMessageGenerator(ContentType.change_cipher_spec,
                                              bytearray(b"\x01")))
    node = node.add_child(FinishedGenerator())
    node = node.add_child(ApplicationDataGenerator(bytearray(b"ping")))
    node = node.add_child(ExpectApplicationData())
    node = node.add_child(AlertGenerator(AlertLevel.warning, AD.close_notify))
    node = node.add_child(ExpectAlert(AlertLevel.warning, AD.close_notify))
    node = node.add_child(ExpectClose())
    conversations["sanity, compatibility CCS before Finished"] = conv

    # A second CCS after the client's Finished, in the application phase:
    # RFC 8446 Appendix D.4 -- a CCS the server did not ask for, after the
    # one the compatibility mode allows, is unexpected_message.
    conv, node = start(host, port)
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    node = node.add_child(FinishedGenerator())
    node = node.add_child(RawMessageGenerator(ContentType.change_cipher_spec,
                                              bytearray(b"\x01")))
    node = node.add_child(ExpectAlert(AlertLevel.fatal, AD.unexpected_message))
    node = node.add_child(ExpectClose())
    conversations["CCS after the handshake"] = conv

    # A two-byte CCS: the message's body is one byte, anything else is a
    # decode error (the stock script's own check).
    conv, node = start(host, port)
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    node = node.add_child(RawMessageGenerator(ContentType.change_cipher_spec,
                                              bytearray(b"\x01\x00")))
    node = node.add_child(ExpectAlert(AlertLevel.fatal, AD.unexpected_message))
    node = node.add_child(ExpectClose())
    conversations["two byte long CCS"] = conv


def main():
    host = "localhost"
    port = 4433
    opts, args = getopt.getopt(sys.argv[1:], "h:p:", ["help"])
    for opt, arg in opts:
        if opt == "-h":
            host = arg
        elif opt == "-p":
            port = int(arg)
    if "--help" in sys.argv:
        print("usage: -h host -p port")
        sys.exit(0)
    build(host, port)
    failed = 0
    for name, conv in conversations.items():
        runner = Runner(conv)
        try:
            runner.run()
            print(f"{name}: pass")
        except AssertionError as e:
            print(f"{name}: FAIL {e}")
            failed += 1
    print(f"TOTAL: {len(conversations)}, FAIL: {failed}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
