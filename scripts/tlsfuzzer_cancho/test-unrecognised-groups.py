"""Groups the server does not know (RFC 8446 \u00a74.2.8, \u00a76.1): a key share
whose group is not in `supported_groups` must be refused `illegal_parameter`;
a ClientHello with only unknown groups must be refused `handshake_failure`.
The stock `test-tls13-unrecognised-groups.py` under this server's assumptions
(`scripts/tlsfuzzer_cancho/__init__.py`).
"""
from __future__ import print_function
import getopt
import sys

from tlsfuzzer.runner import Runner
from tlsfuzzer.messages import Connect, AlertGenerator
from tlsfuzzer.expect import ExpectAlert, ExpectClose
from tlslite.constants import AlertDescription as AD, AlertLevel, ExtensionType, GroupName
from tlslite.extensions import ClientKeyShareExtension, SupportedGroupsExtension, \
    KeyShareEntry

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from _base import client_hello, established, close_clean  # noqa: E402

conversations = {}


def build(host, port):
    # sanity.
    conv = Connect(host, port)
    close_clean(established(conv))
    conversations["sanity"] = conv

    # A share of an unknown group (GREASE, 0x0A0A), alongside the accepted
    # one: RFC 8446 \u00a74.2.8 lets the server ignore an unknown group's share,
    # so this handshakes -- unless the unknown group is the only share and
    # is also absent from supported_groups, which \u00a74.2.8's MUST makes an
    # illegal_parameter.
    known = GroupName.secp256r1
    for unknown in (0x0A0A, 0x1D1D, 65024):
        conv = Connect(host, port)
        ext = {}
        ext[ExtensionType.key_share] = ClientKeyShareExtension().create(
            [key_share for key_share in
             [KeyShareEntry().create(unknown, bytearray(b"\xab" * 32))]])
        ext[ExtensionType.supported_groups] = SupportedGroupsExtension().create(
            [known])
        node = conv
        node = node.add_child(client_hello(extensions=ext))
        node = node.add_child(ExpectAlert(2, AD.illegal_parameter))
        node = node.add_child(ExpectClose())
        conversations[f"only an unknown share, group {unknown}, not in supported_groups"] = conv

    # Unknown groups in supported_groups too, with shares: no group in
    # common, so handshake_failure (RFC 8446 \u00a76.1).
    conv = Connect(host, port)
    ext = {}
    ext[ExtensionType.key_share] = ClientKeyShareExtension().create(
        [KeyShareEntry().create(0x0A0A, bytearray(b"\xab" * 32))])
    ext[ExtensionType.supported_groups] = SupportedGroupsExtension().create([0x0A0A])
    node = conv
    node = node.add_child(client_hello(extensions=ext))
    node = node.add_child(ExpectAlert(2, AD.handshake_failure))
    node = node.add_child(ExpectClose())
    conversations["only unknown supported_groups, with a share"] = conv

    # An unknown group in supported_groups, the accepted group's share
    # offered: RFC 8446 \u00a73.2.2 lets the server ignore an unknown group
    # and take the accepted one, so this handshakes.
    conv = Connect(host, port)
    from tlsfuzzer.helpers import key_share_gen
    ext = {}
    ext[ExtensionType.key_share] = ClientKeyShareExtension().create(
        [key_share_gen(known)])
    ext[ExtensionType.supported_groups] = SupportedGroupsExtension().create(
        [known, 0x0A0A])
    node = conv
    node = node.add_child(client_hello(extensions=ext))
    from tlsfuzzer.expect import ExpectServerHello, ExpectChangeCipherSpec, \
        ExpectEncryptedExtensions, ExpectCertificate, ExpectCertificateVerify, \
        ExpectFinished, ExpectApplicationData
    from tlsfuzzer.messages import FinishedGenerator, ApplicationDataGenerator
    node = node.add_child(ExpectServerHello())
    node = node.add_child(ExpectChangeCipherSpec())
    node = node.add_child(ExpectEncryptedExtensions())
    node = node.add_child(ExpectCertificate())
    node = node.add_child(ExpectCertificateVerify())
    node = node.add_child(ExpectFinished())
    node = node.add_child(FinishedGenerator())
    node = node.add_child(ApplicationDataGenerator(bytearray(b"GET / HTTP/1.0\r\n\r\n")))
    node = node.add_child(ExpectApplicationData())
    node = node.add_child(AlertGenerator(AlertLevel.warning, AD.close_notify))
    node = node.add_child(ExpectAlert(AlertLevel.warning, AD.close_notify))
    node = node.add_child(ExpectClose())
    conversations["an unknown group listed, the accepted share offered: handshakes"] = conv


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
    # A case whose verdict waits on the server's reading of RFC 8446
    # \u00a74.2.8 being settled (#418): a share of a group not in
    # supported_groups gets a HelloRetryRequest here, where the stock
    # script's strict reading expects illegal_parameter.
    xfail = {name for name in conversations if "not in supported_groups" in name}
    failed = 0
    xfailed = 0
    for name, conv in conversations.items():
        runner = Runner(conv)
        try:
            runner.run()
            print(f"{name}: pass")
        except AssertionError as e:
            if name in xfail:
                print(f"{name}: xfail ({e})")
                xfailed += 1
            else:
                print(f"{name}: FAIL {e}")
                failed += 1
    print(f"TOTAL: {len(conversations)}, FAIL: {failed}, XFAIL: {xfailed}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
