//! The listener address and the inbound bound (`docs/native-sockets.md`
//! section 12): what is an address, and which addresses a bound grants.

use super::*;

const LOOPBACK: u32 = 0x7f00_0001;

#[test]
fn a_dotted_quad_reads_as_its_address() {
    assert_eq!(parse_ipv4("127.0.0.1"), Ok(LOOPBACK));
    assert_eq!(parse_ipv4("0.0.0.0"), Ok(0));
    assert_eq!(parse_ipv4("255.255.255.255"), Ok(u32::MAX));
    assert_eq!(parse_ipv4("10.0.20.200"), Ok(0x0a00_14c8));
}

#[test]
fn everything_that_is_not_a_strict_dotted_quad_is_refused() {
    for text in [
        "",
        " ",
        "localhost",
        "::1",
        "127.1",
        "127.0.1",
        "127.0.0.1.1",
        "127.0.0.1.",
        ".127.0.0.1",
        "127..0.1",
        "256.0.0.1",
        "1.2.3.256",
        "010.0.0.1",
        "00.0.0.1",
        "+1.0.0.1",
        "-1.0.0.1",
        "1.2.3.4 ",
        " 1.2.3.4",
        "1.2.3.4\n",
        "0x7f.0.0.1",
        "1.2.3.0x4",
        "1e2.0.0.1",
        "１.2.3.4",
        "1.2.3.4:80",
        "1.2.3.",
        "1.2.3.-4",
        "999.999.999.999",
        "1.2.3.0004",
    ] {
        let why = parse_ipv4(text).expect_err(text);
        assert!(why.contains("not an IPv4 address"), "{text:?}: {why}");
    }
}

#[test]
fn the_parser_agrees_with_the_standard_library_on_every_octet_quad_it_accepts() {
    // The standard library is stricter or equal on leading zeros (it refuses
    // them too since 1.7x), so every accepted text must be one it accepts
    // with the same value; and a sample of generated texts must agree on
    // accept/refuse.
    let pieces =
        ["0", "1", "9", "10", "99", "100", "255", "256", "01", "001", "", "a", "-1", "1000"];
    let mut checked = 0;
    for a in pieces {
        for b in pieces {
            for c in ["0", "7", "255", "256", "00", ""] {
                for d in ["0", "1", "254", "300", "x"] {
                    let text = format!("{a}.{b}.{c}.{d}");
                    let ours = parse_ipv4(&text).ok();
                    let theirs = text.parse::<std::net::Ipv4Addr>().ok().map(u32::from);
                    assert_eq!(ours, theirs, "{text:?}");
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 3000);
}

#[test]
fn a_bound_reads_as_the_hosts_and_the_port_it_grants() {
    let root = parse_listen_bound("");
    assert_eq!((root.host, root.port.as_str(), root.plain_port), (HostBound::Any, "", true));
    let port = parse_listen_bound("8080");
    assert_eq!((port.host, port.port.as_str(), port.plain_port), (HostBound::Any, "8080", true));
    let one = parse_listen_bound("127.0.0.1:8080");
    assert_eq!(
        (one.host, one.port.as_str(), one.plain_port),
        (HostBound::Host(LOOPBACK), "8080", false)
    );
    let any_port = parse_listen_bound("127.0.0.1:");
    assert_eq!((any_port.host, any_port.port.as_str()), (HostBound::Host(LOOPBACK), ""));
    let bare_host = parse_listen_bound("127.0.0.1");
    assert_eq!((bare_host.host, bare_host.port.as_str()), (HostBound::Host(LOOPBACK), ""));
    let wild = parse_listen_bound("0.0.0.0:8080");
    assert_eq!((wild.host, wild.port.as_str(), wild.plain_port), (HostBound::Any, "8080", false));
}

#[test]
fn a_host_that_is_a_name_or_ipv6_matches_no_address() {
    for bound in
        ["localhost:80", "localhost", "::1", "[::1]:80", "example.com:443", "127.0.0.01:80"]
    {
        let parsed = parse_listen_bound(bound);
        assert_eq!(parsed.host, HostBound::Unmatchable, "{bound}");
        assert!(!parsed.covers(LOOPBACK) && !parsed.covers(0), "{bound}");
    }
}

#[test]
fn an_address_in_a_bound_covers_only_itself_and_the_wildcard_covers_all() {
    let loopback = parse_listen_bound("127.0.0.1:80");
    assert!(loopback.covers(LOOPBACK));
    // Not a prefix: 127.0.0.10 starts with the text of 127.0.0.1.
    assert!(!loopback.covers(0x7f00_000a));
    assert!(!loopback.covers(0), "0.0.0.0 is everywhere, wider than loopback");
    assert!(!loopback.covers(0x7f00_0002));
    for wild in ["", "80", "0.0.0.0:80"] {
        let parsed = parse_listen_bound(wild);
        assert!(parsed.covers(LOOPBACK) && parsed.covers(0) && parsed.covers(u32::MAX), "{wild}");
    }
}

#[test]
fn no_text_reaches_a_panic() {
    for text in
        ["", ":", "::", ":::", "a:b:c", "999999999999999999999:1", "\u{0}", "é:é", "1.2.3.4:"]
    {
        let _ = parse_listen_bound(text);
        let _ = parse_ipv4(text);
    }
}
