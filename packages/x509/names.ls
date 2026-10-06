module x509_names;
import std.bytes;
import x509;

// `x509_names` -- the server's name against a certificate, and name
// constraints (`docs/x509-verify.md` §5). Sub-issue 9 (#206) of the
// self-contained TLS 1.3 client (#197). Not independently reviewed
// (#209).
//
// Everything here reads public data. Every loop is bounded by its
// input's length, and the name-constraint comparisons by a budget the
// caller passes (`docs/x509-verify.md` §5.3).

// ---- Refusals: `x509_verify`'s codes that this module answers ----

pub fn name_mismatch() -> [] int {
    return -34;
}

pub fn name_constraint() -> [] int {
    return -37;
}

// ---- The host (`docs/x509-verify.md` §5.1) ----

fn hex_digit(c: int) -> [] int {
    if c >= 48 && c <= 57 {
        return c - 48;
    }
    let l = bytes.to_lower(c);
    if l >= 97 && l <= 102 {
        return l - 87;
    }
    return -1;
}

// `text` as an IPv4 literal into `out[at..at + 4]`: four decimal parts
// of 0 to 255, no leading zeros. 4, or -1.
fn ipv4[&t, &o](text: &t [byte], out: &!o [byte], at: int) -> [] int {
    var part = 0;
    var digits = 0;
    var value = 0;
    var i = 0;
    while i <= len(text) {
        var c = 46;
        if i < len(text) {
            c = int_of(text[i]);
        }
        if c == 46 {
            if digits == 0 || part > 3 {
                return -1;
            }
            out[at + part] = byte_of(value);
            part = part + 1;
            digits = 0;
            value = 0;
        } else if c >= 48 && c <= 57 {
            if digits > 0 && value == 0 {
                return -1;
            }
            value = value * 10 + c - 48;
            digits = digits + 1;
            if value > 255 {
                return -1;
            }
        } else {
            return -1;
        }
        i = i + 1;
    }
    if part != 4 {
        return -1;
    }
    return 4;
}

// `text` as an IPv6 literal (RFC 4291 §2.2: groups of 1 to 4 hex
// digits, one `::`, an IPv4 tail) into `out[0..16]`. 16, or -1.
fn ipv6[&t, &o](text: &t [byte], out: &!o [byte]) -> [] int {
    let n = len(text);
    var groups = 0;
    var gap = -1;
    var i = 0;
    if n >= 2 && int_of(text[0]) == 58 && int_of(text[1]) == 58 {
        gap = 0;
        i = 2;
        if n == 2 {
            var k = 0;
            while k < 16 {
                out[k] = byte_of(0);
                k = k + 1;
            }
            return 16;
        }
    } else if n >= 1 && int_of(text[0]) == 58 {
        return -1;
    }
    var done = false;
    while !done {
        // One group, or an IPv4 tail.
        var j = i;
        var value = 0;
        var dots = false;
        while j < n && int_of(text[j]) != 58 {
            if int_of(text[j]) == 46 {
                dots = true;
            }
            j = j + 1;
        }
        if dots {
            if j != n || groups > 6 {
                return -1;
            }
            if ipv4(text[i..n], out, 2 * groups) != 4 {
                return -1;
            }
            groups = groups + 2;
            done = true;
        } else {
            if j - i < 1 || j - i > 4 || groups > 7 {
                return -1;
            }
            var k = i;
            while k < j {
                let d = hex_digit(int_of(text[k]));
                if d < 0 {
                    return -1;
                }
                value = value * 16 + d;
                k = k + 1;
            }
            out[2 * groups] = byte_of(value >> 8);
            out[2 * groups + 1] = byte_of(value & 255);
            groups = groups + 1;
            if j == n {
                done = true;
            } else if j + 1 < n && int_of(text[j + 1]) == 58 {
                if gap >= 0 {
                    return -1;
                }
                gap = groups;
                i = j + 2;
                if i == n {
                    done = true;
                }
            } else if j + 1 == n {
                return -1;
            } else {
                i = j + 1;
            }
        }
    }
    if gap < 0 {
        if groups != 8 {
            return -1;
        }
        return 16;
    }
    if groups > 7 {
        return -1;
    }
    // Move the groups after `::` to the end, zeros between.
    let tail = groups - gap;
    var k = tail - 1;
    while k >= 0 {
        out[2 * (8 - tail + k)] = out[2 * (gap + k)];
        out[2 * (8 - tail + k) + 1] = out[2 * (gap + k) + 1];
        k = k - 1;
    }
    k = gap;
    while k < 8 - tail {
        out[2 * k] = byte_of(0);
        out[2 * k + 1] = byte_of(0);
        k = k + 1;
    }
    return 16;
}

fn dns_char(c: int) -> [] bool {
    return c >= 97 && c <= 122 || c >= 48 && c <= 57 || c == 45 || c == 95;
}

// Whether `name` is a host name: labels of 1 to 63 letters, digits,
// `-` or `_`, at most 253 bytes. `name` is lowercase.
fn dns_ok[&n](name: &n [byte]) -> [] bool {
    let n = len(name);
    if n < 1 || n > 253 {
        return false;
    }
    var label = 0;
    var i = 0;
    while i < n {
        let c = int_of(name[i]);
        if c == 46 {
            if label == 0 {
                return false;
            }
            label = 0;
        } else if dns_char(c) {
            label = label + 1;
            if label > 63 {
                return false;
            }
        } else {
            return false;
        }
        i = i + 1;
    }
    return label > 0;
}

// The host as the verifier compares it: into `out` (at least
// `len(host)` bytes), with `info[0]` its length and `info[1]` its kind:
// 4 or 16 for an IP address (its bytes), 0 for a DNS name (lowercase,
// one trailing dot removed). 0, or `name_mismatch()` for a host no
// certificate can match.
pub fn host_parse[&h, &o, &i](host: &h [byte], out: &!o [byte], info: &!i [int]) -> [] int {
    if len(out) < 16 || len(out) < len(host) {
        return name_mismatch();
    }
    if x509.is_ip_literal(host) {
        var k = 0;
        var colon = false;
        while k < len(host) {
            if int_of(host[k]) == 58 {
                colon = true;
            }
            k = k + 1;
        }
        var n = -1;
        if colon {
            n = ipv6(host, out);
        } else {
            n = ipv4(host, out, 0);
        }
        if n < 0 {
            return name_mismatch();
        }
        info[0] = n;
        info[1] = n;
        return 0;
    }
    var n = len(host);
    if n > 0 && int_of(host[n - 1]) == 46 {
        n = n - 1;
    }
    var k = 0;
    while k < n {
        out[k] = byte_of(bytes.to_lower(int_of(host[k])));
        k = k + 1;
    }
    if !dns_ok(out[0..n]) {
        return name_mismatch();
    }
    info[0] = n;
    info[1] = 0;
    return 0;
}

// ---- Matching the leaf (`docs/x509-verify.md` §5.2) ----

// Whether `a` equals `b` without case.
fn same[&a, &b](a: &a [byte], b: &b [byte]) -> [] bool {
    if len(a) != len(b) {
        return false;
    }
    var i = 0;
    while i < len(a) {
        if bytes.to_lower(int_of(a[i])) != bytes.to_lower(int_of(b[i])) {
            return false;
        }
        i = i + 1;
    }
    return true;
}

// Whether a SAN dNSName `pattern` (any case) matches `host`, a valid
// lowercase name. A wildcard is the whole left-most label, needs two
// labels after it, and stands for exactly one label.
pub fn dns_matches[&p, &h](pattern: &p [byte], host: &h [byte]) -> [] bool {
    let n = len(pattern);
    if n >= 2 && int_of(pattern[0]) == 42 && int_of(pattern[1]) == 46 {
        let rest = pattern[2..n];
        // Two labels after the wildcard, and no other `*`.
        var dots = 0;
        var i = 0;
        while i < len(rest) {
            let c = bytes.to_lower(int_of(rest[i]));
            if c == 46 {
                dots = dots + 1;
            } else if !dns_char(c) {
                return false;
            }
            i = i + 1;
        }
        if dots < 1 {
            return false;
        }
        // `host` is one label, a dot, then `rest`.
        var first = 0;
        while first < len(host) && int_of(host[first]) != 46 {
            first = first + 1;
        }
        if first == 0 || first >= len(host) {
            return false;
        }
        return same(rest, host[first + 1..len(host)]);
    }
    var i = 0;
    while i < n {
        if int_of(pattern[i]) == 42 {
            return false;
        }
        i = i + 1;
    }
    return same(pattern, host);
}

// Whether the subjectAltName content `der[s..e]` names the host
// (`host_parse`'s output and kind).
pub fn san_matches[&d, &h](der: &d [byte], s: int, e: int, host: &h [byte], kind: int) -> [] bool {
    var found = false;
    region r {
        let t = alloc_slice[r](3, 0);
        var p = s;
        while p < e && !found {
            if x509.tlv(der, p, e, t) != 0 {
                p = e;
            } else {
                if kind == 0 && t[0] == 0x82 {
                    found = dns_matches(der[t[1]..t[2]], host);
                } else if kind != 0 && t[0] == 0x87 && t[2] - t[1] == kind {
                    var k = 0;
                    while k < kind && der[t[1] + k] == host[k] {
                        k = k + 1;
                    }
                    found = k == kind;
                }
                p = t[2];
            }
        }
    }
    return found;
}

// ---- Name constraints (`docs/x509-verify.md` §5.3) ----

pub fn max_subtrees() -> [] int {
    return 1024;
}

// The comparisons one chain may make, all certificates together.
pub fn max_comparisons() -> [] int {
    return 1 << 20;
}

// Whether the DNS name `name` is inside the dNSName subtree `sub`,
// without case: equal, or ending with `.` and it. A subtree with a
// leading dot holds only proper subdomains; an empty one holds all.
fn dns_within[&n, &s](name: &n [byte], sub: &s [byte]) -> [] bool {
    let k = len(sub);
    let n = len(name);
    if k == 0 {
        return true;
    }
    if int_of(sub[0]) == 46 {
        return n > k && same(sub, name[n - k..n]);
    }
    if n == k {
        return same(sub, name);
    }
    return n > k && int_of(name[n - k - 1]) == 46 && same(sub, name[n - k..n]);
}

// Whether an iPAddress subtree `sub` (an address and a mask) holds
// `ip`. -1 when the mask is not ones then zeros.
fn ip_within[&a, &s](ip: &a [byte], sub: &s [byte]) -> [] int {
    let n = len(ip);
    var zeros = false;
    var k = 0;
    while k < n {
        let m = int_of(sub[n + k]);
        var b = 7;
        while b >= 0 {
            if m >> b & 1 == 1 {
                if zeros {
                    return -1;
                }
            } else {
                zeros = true;
            }
            b = b - 1;
        }
        k = k + 1;
    }
    k = 0;
    while k < n {
        let m = int_of(sub[n + k]);
        if int_of(ip[k]) & m != int_of(sub[k]) & m {
            return 0;
        }
        k = k + 1;
    }
    return 1;
}

// The DNS name a SAN dNSName constrains, into `out`: lowercase, a
// leading `*.` removed (`*.a.example` is checked as `a.example`).
// Its length, or -1 for a name no constraint can be applied to.
fn constrained_dns[&p, &o](pattern: &p [byte], out: &!o [byte]) -> [] int {
    var start = 0;
    if len(pattern) >= 2 && int_of(pattern[0]) == 42 && int_of(pattern[1]) == 46 {
        start = 2;
    }
    let n = len(pattern) - start;
    if n > len(out) {
        return -1;
    }
    var k = 0;
    while k < n {
        out[k] = byte_of(bytes.to_lower(int_of(pattern[start + k])));
        k = k + 1;
    }
    if !dns_ok(out[0..n]) {
        return -1;
    }
    return n;
}

// Checks every subtree of the GeneralSubtrees content `nc[s..e]` and
// answers, for one name of SAN tag `tag` (0x82 or 0x87): `found[0]`
// set to 1 when some subtree has that type, `found[1]` when one holds
// the name. For a wildcard name against excluded subtrees, a subtree
// inside the name holds it too: `*.a.example` covers an excluded
// `x.a.example`. 0, or
// `name_constraint()` for a subtree that cannot be read, too many, or
// the budget spent.
fn subtrees[&c, &v, &f, &b](nc: &c [byte], s: int, e: int, tag: int, value: &v [byte], wild_excluded: bool, found: &!f [int], budget: &!b [int]) -> [] int {
    var code = 0;
    var count = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        let g = alloc_slice[r](3, 0);
        var p = s;
        while p < e && code == 0 {
            if x509.tlv(nc, p, e, t) != 0 || t[0] != 0x30 {
                code = name_constraint();
            } else {
                p = t[2];
                count = count + 1;
                budget[0] = budget[0] + 1;
                if count > max_subtrees() || budget[0] > max_comparisons() {
                    code = name_constraint();
                } else if x509.tlv(nc, t[1], t[2], g) != 0 || g[2] != t[2] {
                    // A minimum or maximum after the base: RFC 5280
                    // says neither is used, so neither is read.
                    code = name_constraint();
                } else if g[0] == 0x82 {
                    if tag == 0x82 {
                        found[0] = 1;
                        let sub = nc[g[1]..g[2]];
                        // A wildcard SAN (`value` is its base, `*.`
                        // removed) is excluded when any name it can
                        // match is: a subtree within its base, or, for a
                        // subtree with a leading dot (proper subdomains
                        // only), one whose base is the wildcard's own,
                        // since `*.a.example` names only proper
                        // subdomains of `a.example` (#318, RFC 5280
                        // §4.2.1.10).
                        let dotted = len(sub) > 0 && int_of(sub[0]) == 46;
                        if dns_within(value, sub) || wild_excluded && len(sub) > 0 && !dotted && dns_within(sub, value) || wild_excluded && dotted && dns_within(value, sub[1..len(sub)]) {
                            found[1] = 1;
                        }
                    }
                } else if g[0] == 0x87 {
                    if g[2] - g[1] != 8 && g[2] - g[1] != 32 {
                        code = name_constraint();
                    } else if tag == 0x87 {
                        let sub = nc[g[1]..g[2]];
                        if len(sub) == 2 * len(value) {
                            found[0] = 1;
                            let w = ip_within(value, sub);
                            if w < 0 {
                                code = name_constraint();
                            } else if w == 1 {
                                found[1] = 1;
                            }
                        } else {
                            // The other family: the type is constrained,
                            // and this subtree does not hold the name.
                            found[0] = 1;
                        }
                    }
                } else {
                    // rfc822Name, directoryName, URI, otherName and the
                    // rest: a constraint this verifier cannot apply.
                    code = name_constraint();
                }
            }
        }
    }
    return code;
}

// One name against the nameConstraints content `nc[s..e]` of an issuer:
// inside a permitted subtree when that type has any, and inside no
// excluded one. 0, or `name_constraint()`.
fn name_ok[&c, &v, &b](nc: &c [byte], s: int, e: int, tag: int, value: &v [byte], wildcard: bool, budget: &!b [int]) -> [] int {
    var code = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        let found = alloc_slice[r](2, 0);
        var p = s;
        var last = -1;
        if s == e {
            // Neither permitted nor excluded (RFC 5280 §4.2.1.10).
            code = name_constraint();
        }
        while p < e && code == 0 {
            if x509.tlv(nc, p, e, t) != 0 || t[0] <= last || t[0] != 0xa0 && t[0] != 0xa1 || t[1] == t[2] {
                // Out of order, or an empty GeneralSubtrees (SIZE 1..MAX).
                code = name_constraint();
            } else {
                last = t[0];
                found[0] = 0;
                found[1] = 0;
                code = subtrees(nc, t[1], t[2], tag, value, wildcard && t[0] == 0xa1, found, budget);
                if code == 0 && t[0] == 0xa0 && found[0] == 1 && found[1] == 0 {
                    code = name_constraint();
                }
                if code == 0 && t[0] == 0xa1 && found[1] == 1 {
                    code = name_constraint();
                }
                p = t[2];
            }
        }
    }
    return code;
}

// Every dNSName and iPAddress in the SAN content `der[s..e]` of a
// certificate below the issuer whose nameConstraints content is
// `nc[ns..ne]`. Also reads the constraints once when the certificate
// has no SAN, so a constraint that cannot be read is refused whatever
// the names. 0, or `name_constraint()`.
pub fn constraints_ok[&c, &d, &b](nc: &c [byte], ns: int, ne: int, der: &d [byte], s: int, e: int, budget: &!b [int]) -> [] int {
    var code = 0;
    region r {
        let t = alloc_slice[r](3, 0);
        let name = alloc_slice[r](253, byte_of(0));
        // The constraints themselves, read once with no name.
        code = name_ok(nc, ns, ne, 0, name[0..0], false, budget);
        var p = s;
        while p < e && code == 0 {
            if x509.tlv(der, p, e, t) != 0 {
                code = name_constraint();
            } else {
                if t[0] == 0x82 {
                    let n = constrained_dns(der[t[1]..t[2]], name);
                    if n < 0 {
                        code = name_constraint();
                    } else {
                        let wildcard = t[2] - t[1] >= 2 && int_of(der[t[1]]) == 42;
                        code = name_ok(nc, ns, ne, 0x82, name[0..n], wildcard, budget);
                    }
                } else if t[0] == 0x87 {
                    code = name_ok(nc, ns, ne, 0x87, der[t[1]..t[2]], false, budget);
                }
                p = t[2];
            }
        }
    }
    return code;
}
