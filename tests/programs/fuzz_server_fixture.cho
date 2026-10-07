edition 5;
module fuzz_server_fixture;

// The server's identity for `fuzz_server` (docs/tls-server.md §7): the chain
// and P-256 key of `scripts/tls_liar_client.py`'s first identity, for
// `liar.lex-sys.test`, from fixed seeds, and the time it is loaded at.

pub fn chain() -> [] &static [byte] {
    return "-----BEGIN CERTIFICATE-----\nMIIBNjCB6aADAgECAgEHMAUGAytlcDAdMRswGQYDVQQDDBJ0bHNfbGlhcl9jbGll\nbnQgY2EwHhcNMjYwMTAxMDAwMDAwWhcNMzUxMjMwMDAwMDAwWjAcMRowGAYDVQQD\nDBFsaWFyLmxleC1zeXMudGVzdDBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABG5j\n0KxljpBfHu2cnqg4fXMUVoaF9WmeRP5ULsShpiFFrHPwo6b1lfIXwBMbDA2pBFrs\nOTiRKn4Fre8jnv9vupmjIDAeMBwGA1UdEQQVMBOCEWxpYXIubGV4LXN5cy50ZXN0\nMAUGAytlcANBAIYr2ICh6qjBNZZZsqy2j2Nqpku1usKxjF2x9tpNr7Gh3Ox6sjYo\nBwO4pqdnNOf0yGQUpfLCF3Uj16ygDO0xrQ0=\n-----END CERTIFICATE-----\n-----BEGIN CERTIFICATE-----\nMIH7MIGuoAMCAQICAQEwBQYDK2VwMB0xGzAZBgNVBAMMEnRsc19saWFyX2NsaWVu\ndCBjYTAeFw0yNjAxMDEwMDAwMDBaFw0zNTEyMzAwMDAwMDBaMB0xGzAZBgNVBAMM\nEnRsc19saWFyX2NsaWVudCBjYTAqMAUGAytlcAMhAFIl3fqi3k6Nmh5EUDHpVvqa\np1RfQJuY5LVZdN8iarPloxMwETAPBgNVHRMBAf8EBTADAQH/MAUGAytlcANBADP+\nqA6Kif1yHBgc9UKoDvjQpGWbB4iPQlcHyoE6/P9ZSJNVnXtoAUG4+1EMc9sLP0D7\njpnxRg2DySyrpRaA2wo=\n-----END CERTIFICATE-----\n";
}

pub fn key() -> [] &static [byte] {
    return "-----BEGIN PRIVATE KEY-----\nMIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg5m/LHp8GkW1doOKY\nFZfGSnk9BJaF6/ZLboIuv1uE70+hRANCAARuY9CsZY6QXx7tnJ6oOH1zFFaGhfVp\nnkT+VC7EoaYhRaxz8KOm9ZXyF8ATGwwNqQRa7Dk4kSp+Ba3vI57/b7qZ\n-----END PRIVATE KEY-----\n";
}

pub fn names() -> [] &static [byte] {
    return "liar.lex-sys.test";
}

pub fn now_ms() -> [] int {
    return 1780272000000;
}

// The engine's DRBG seed: 32 bytes.
pub fn seed() -> [] &static [byte] {
    return "fuzz_server's fixed seed, 32 by.";
}
