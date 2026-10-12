"""The key_share extension omitted, or empty (RFC 8446 \u00a74.2.8, \u00a79.2):
a ClientHello without `key_share` must be refused `missing_extension`;
one with an empty `key_share` must be refused `decode_error`. The stock
`test-tls13-keyshare-omitted.py` under this server's assumptions.
"""
from __future__ import print_function
import getopt
import sys

from tlsfuzzer.runner import Runner
from tlsfuzzer.messages import Connect
from tlsfuzzer.expect import ExpectAlert, ExpectClose
from tlslite.constants import AlertDescription as AD, ExtensionType
from tlsfuzzer.helpers import AutoEmptyExtension

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from _base import client_hello, established, close_clean, refused  # noqa: E402

conversations = {}


def build(host, port):
    # sanity: a full handshake and clean close, so a failure here is the harness.
    conv = Connect(host, port)
    close_clean(established(conv))
    conversations["sanity"] = conv

    # No key_share at all: RFC 8446 \u00a79.2 makes it mandatory in TLS 1.3.
    conv = Connect(host, port)
    node = conv
    node = node.add_child(client_hello(with_key_share=False))
    node = node.add_child(ExpectAlert(2, AD.missing_extension))
    node = node.add_child(ExpectClose())
    conversations["key_share extension omitted"] = conv

    # An empty key_share: the extension is present but names no shares,
    # which is a decode error in its body.
    for name, extra in [
        ("empty key_share extension", None),
        ("empty key_share extension, psk_key_exchange_modes present", "psk"),
    ]:
        ext = {ExtensionType.key_share: AutoEmptyExtension()}
        if extra == "psk":
            from tlslite.extensions import PskKeyExchangeModesExtension
            from tlslite.constants import PskKeyExchangeMode
            ext[ExtensionType.psk_key_exchange_modes] = \
                PskKeyExchangeModesExtension().create(
                    [PskKeyExchangeMode.psk_ke, PskKeyExchangeMode.psk_dhe_ke])
        conv = Connect(host, port)
        node = conv
        node = node.add_child(client_hello(extensions=ext))
        node = node.add_child(ExpectAlert(2, AD.decode_error))
        node = node.add_child(ExpectClose())
        conversations[name] = conv


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
