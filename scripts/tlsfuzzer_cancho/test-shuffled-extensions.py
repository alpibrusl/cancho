"""Extension order and unknown extensions (RFC 8446 \u00a74.1.2, \u00a74.2): the
ClientHello's extensions may appear in any order, and an extension the server
does not know must be skipped, not refused -- except `pre_shared_key`, which
must be last. The stock `test-tls13-shuffled-extentions.py`'s core cases under
this server's assumptions (`scripts/tlsfuzzer_cancho/__init__.py`).
"""
from __future__ import print_function
import getopt
import sys

from tlsfuzzer.runner import Runner
from tlsfuzzer.messages import Connect, ApplicationDataGenerator, \
    AlertGenerator
from tlslite.extensions import PaddingExtension
from tlsfuzzer.expect import ExpectServerHello, ExpectChangeCipherSpec, \
    ExpectEncryptedExtensions, ExpectCertificate, ExpectCertificateVerify, \
    ExpectFinished, ExpectApplicationData, ExpectAlert, ExpectClose
from tlslite.constants import AlertDescription as AD, AlertLevel, \
    ExtensionType
from tlslite.extensions import TLSExtension
from _base import client_hello, established, close_clean  # noqa: E402

conversations = {}


def finish(node):
    node = node.add_child(ExpectApplicationData())
    return close_clean(node)


def build(host, port):
    # sanity.
    conv = Connect(host, port)
    close_clean(established(conv))
    conversations["sanity"] = conv

    # The required extensions in reverse order: RFC 8446 \u00a74.1.2 makes the
    # order free, so this must handshake.
    from tlslite.extensions import ClientKeyShareExtension, SupportedGroupsExtension, \
        SupportedVersionsExtension, SignatureAlgorithmsExtension, \
        SignatureAlgorithmsCertExtension
    from tlsfuzzer.helpers import key_share_gen
    from tlslite.constants import GroupName, SignatureScheme
    groups = [GroupName.secp256r1]
    sig = [SignatureScheme.ecdsa_secp256r1_sha256]
    # Reverse of _base's insertion order.
    ext = {
        ExtensionType.signature_algorithms_cert:
            SignatureAlgorithmsCertExtension().create(sig),
        ExtensionType.signature_algorithms:
            SignatureAlgorithmsExtension().create(sig),
        ExtensionType.supported_groups:
            SupportedGroupsExtension().create(groups),
        ExtensionType.supported_versions:
            SupportedVersionsExtension().create([(3, 4), (3, 3)]),
        ExtensionType.key_share:
            ClientKeyShareExtension().create([key_share_gen(g) for g in groups]),
    }
    conv = Connect(host, port)
    node = conv
    node = node.add_child(client_hello(extensions=ext))
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    from tlsfuzzer.messages import FinishedGenerator
    node = node.add_child(FinishedGenerator())
    node = node.add_child(ApplicationDataGenerator(bytearray(b"GET / HTTP/1.0\r\n\r\n")))
    finish(node)
    conversations["required extensions in reverse order"] = conv

    # An unassigned extension id with a body: skipped (\u00a74.1.2: "unrecognised
    # extensions MUST be ignored"), and the handshake goes on.
    for ext_id in (0x0A0A, 65024, 131):
        ext = {ext_id: TLSExtension().create(ext_id, bytearray(b"\x00\x01\x02"))}
        conv = Connect(host, port)
        node = conv
        node = node.add_child(client_hello(extensions=ext))
        node = node.add_child(ExpectServerHello())
        node = node.add_child(ExpectChangeCipherSpec())
        node = node.add_child(ExpectEncryptedExtensions())
        node = node.add_child(ExpectCertificate())
        node = node.add_child(ExpectCertificateVerify())
        node = node.add_child(ExpectFinished())
        from tlsfuzzer.messages import FinishedGenerator
        node = node.add_child(FinishedGenerator())
        node = node.add_child(ApplicationDataGenerator(bytearray(b"GET / HTTP/1.0\r\n\r\n")))
        finish(node)
        conversations[f"unrecognised extension {ext_id}, ignored"] = conv

    # A padding extension, the largest legal one: still a ClientHello under
    # the server's 16 KiB bound, so it handshakes.
    conv = Connect(host, port)
    node = conv
    ext = {ExtensionType.client_hello_padding: PaddingExtension().create(512)}
    node = node.add_child(client_hello(extensions=ext))
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    from tlsfuzzer.messages import FinishedGenerator
    node = node.add_child(FinishedGenerator())
    node = node.add_child(ApplicationDataGenerator(bytearray(b"GET / HTTP/1.0\r\n\r\n")))
    finish(node)
    conversations["a padding extension of 512 bytes"] = conv


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
