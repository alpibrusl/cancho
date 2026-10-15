//! The address a listener may bind (`docs/native-sockets.md` section 12):
//! an IPv4 literal read once, at compile time, and the inbound bound that
//! says which hosts and which port a `Net` grants. Pure functions over text,
//! so the checker, the tests and the fuzzer share one reading.

/// The host half of an inbound bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostBound {
    /// Every address: an unnarrowed bound, a bare port, or `0.0.0.0`.
    Any,
    /// Exactly this address (host order), compared whole, never as a prefix
    /// (`docs/net-bounds.md` section 2.3).
    Host(u32),
    /// A host half that is not an IPv4 literal -- a name, IPv6 -- and so
    /// matches no address: there is no resolution (section 12.8).
    Unmatchable,
}

/// An inbound bound split into the two halves the checker and the backends
/// read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenBound {
    pub host: HostBound,
    /// The port half as text for the backend's own port check: empty (any
    /// port) or the digits `tcp_listen` has always compared against.
    pub port: String,
    /// The bound grants every interface and nothing narrower is written: the
    /// only shape the all-interfaces `tcp_listen` may run under.
    pub plain_port: bool,
}

fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// Read `bound` (`docs/native-sockets.md` section 12.3). Total: any text
/// reads as something, and a malformed port half is left for the backend's
/// existing check, which reads it as a port nothing can ask for.
pub fn parse_listen_bound(bound: &str) -> ListenBound {
    if bound.is_empty() || all_digits(bound) {
        return ListenBound { host: HostBound::Any, port: bound.to_owned(), plain_port: true };
    }
    let (host_text, port) = match bound.rsplit_once(':') {
        Some((host, port)) => (host, port),
        None => (bound, ""),
    };
    let host = if host_text.is_empty() {
        HostBound::Any
    } else {
        match parse_ipv4(host_text) {
            Ok(0) => HostBound::Any,
            Ok(address) => HostBound::Host(address),
            Err(_) => HostBound::Unmatchable,
        }
    };
    ListenBound { host, port: port.to_owned(), plain_port: false }
}

impl ListenBound {
    /// May a listener bind `address`?
    pub fn covers(&self, address: u32) -> bool {
        match self.host {
            HostBound::Any => true,
            HostBound::Host(only) => only == address,
            HostBound::Unmatchable => false,
        }
    }
}

/// A dotted-quad IPv4 address, strictly: four decimal octets of one to three
/// digits, none over 255, none with a leading zero (C's parsers disagree on
/// whether that is octal), no sign, no space, no trailing dot. The `Err` is
/// the sentence the `listen-host` refusal carries.
pub fn parse_ipv4(text: &str) -> Result<u32, String> {
    let wrong = |why: &str| {
        Err(format!(
            "`{text}` is not an IPv4 address ({why}); a listener's address is a dotted quad such as `\"127.0.0.1\"` or `\"0.0.0.0\"`, with no names and no IPv6"
        ))
    };
    let mut address = 0u32;
    let mut count = 0;
    for part in text.split('.') {
        count += 1;
        if count > 4 {
            return wrong("more than four octets");
        }
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return wrong("each octet is one to three decimal digits");
        }
        if part.len() > 1 && part.starts_with('0') {
            return wrong("a leading zero reads as octal in some parsers");
        }
        let Ok(octet) = part.parse::<u32>() else {
            return wrong("an octet is a number");
        };
        if octet > 255 {
            return wrong("an octet is at most 255");
        }
        address = (address << 8) | octet;
    }
    if count != 4 {
        return wrong("it has fewer than four octets");
    }
    Ok(address)
}

#[cfg(test)]
#[path = "tests/listen.rs"]
mod tests;
