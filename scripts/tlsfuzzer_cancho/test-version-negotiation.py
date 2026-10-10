"""Version negotiation (RFC 8446 \u00a74.1.2, \u00a74.2.1): the legacy_version field of
a TLS 1.3 ClientHello must be (3,3) whatever supported_versions says -- a
client that puts anything else is old or broken, and a TLS 1.3 server answers
by negotiating (3,3) or refusing; supported_versions without (3,4) is a
protocol_version. The stock `test-tls13-version-negotiation.py`'s core cases
under this server's assumptions (`scripts/tlsfuzzer_cancho/__init__.py`).
"""
from __future__ import print_function
import getopt
import sys

from tlsfuzzer.runner import Runner
from tlsfuzzer.messages import Connect
from tlsfuzzer.expect import ExpectAlert, ExpectClose, ExpectServerHello, \
    ExpectChangeCipherSpec, ExpectEncryptedExtensions, ExpectCertificate, \
    ExpectCertificateVerify, ExpectFinished
from tlsfuzzer.messages import SetRecordVersion
from tlslite.constants import AlertDescription as AD, ExtensionType
from tlslite.extensions import SupportedVersionsExtension

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from _base import client_hello, established, close_clean  # noqa: E402

conversations = {}


def hello_with_versions(node, versions, legacy=None):
    ext = {ExtensionType.supported_versions:
           SupportedVersionsExtension().create(versions)}
    return node.add_child(client_hello(extensions=ext)), legacy


def build(host, port):
    # sanity.
    conv = Connect(host, port)
    close_clean(established(conv))
    conversations["sanity"] = conv

    # legacy_version (3,0) -- an SSL 3 client -- but supported_versions with
    # (3,4): RFC 8446 \u00a74.1.2's MUST says legacy_version must be (3,3); the
    # server takes the supported_versions extension's word and negotiates
    # TLS 1.3 anyway (RFC 8446 \u00a74.2.1 lets it).
    for legacy in [(3, 0), (3, 2), (3, 9)]:
        conv = Connect(host, port)
        node = conv
        ext = {ExtensionType.supported_versions:
               SupportedVersionsExtension().create([(3, 4), (3, 3)])}
        node = node.add_child(client_hello(extensions=ext))
        node = node.add_child(SetRecordVersion(legacy))
        node = node.add_child(ExpectServerHello())
        node = node.add_child(ExpectChangeCipherSpec())
        node = node.add_child(ExpectEncryptedExtensions())
        node = node.add_child(ExpectCertificate())
        node = node.add_child(ExpectCertificateVerify())
        node = node.add_child(ExpectFinished())
        conversations[f"legacy_version {legacy}, record version, TLS 1.3 negotiated"] = conv

    # supported_versions without (3,4): no common version, so
    # protocol_version (RFC 8446 \u00a76.1).
    conv = Connect(host, port)
    node = conv
    ext = {ExtensionType.supported_versions:
           SupportedVersionsExtension().create([(3, 3)])}
    node = node.add_child(client_hello(extensions=ext))
    node = node.add_child(ExpectAlert(2, AD.protocol_version))
    node = node.add_child(ExpectClose())
    conversations["supported_versions with TLS 1.2 only: protocol_version"] = conv

    # A draft version where (3,4) belongs: refused as a version too.
    conv = Connect(host, port)
    node = conv
    ext = {ExtensionType.supported_versions:
           SupportedVersionsExtension().create([(0x7f, 0x12), (3, 3)])}
    node = node.add_child(client_hello(extensions=ext))
    node = node.add_child(ExpectAlert(2, AD.protocol_version))
    node = node.add_child(ExpectClose())
    conversations["supported_versions with a draft number only"] = conv


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
