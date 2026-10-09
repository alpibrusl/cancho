"""tlsfuzzer scripts for `packages/tls`'s server (docs/tls-server.md \u00a710; #209's evidence).

tlsfuzzer's stock TLS 1.3 scripts assume an RSA-capable server (their `signature_algorithms`
offer RSA-PSS only) and expect `NewSessionTicket`; this server has a P-256 identity, no session
tickets and no CCS of its own. These scripts are the same checks -- malformed and unexpected
records and messages, RFC 8446's musts -- with the assumptions corrected, so a failure here
means the server, not the harness. Run by `scripts/tls_server_tlsfuzzer.py`, which sets up the
identity and the port.

Each script is a file with `conversations` and `main(host, port)` in tlsfuzzer's own shape,
built on the same library the stock scripts use.
"""
