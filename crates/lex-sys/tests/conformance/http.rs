//! `std.http` and `std.route` (`docs/http.md`): the parser against
//! `httparse`, the router against a reference written here, and the
//! library's own tests.

use super::json::{Lcg, build_driver, feed};
use super::*;

/// One request from the generator: valid by construction.
fn valid_request(rng: &mut Lcg) -> Vec<u8> {
    const METHODS: [&str; 7] = ["GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS", "PATCH"];
    const TARGET: &[u8] = b"abcXYZ019-._~!$&'()*+,;=:@/%";
    const NAME: &[u8] = b"abcXYZ019-_.!#$%&'*+^`|~";
    let mut out = Vec::new();
    out.extend(METHODS[rng.below(METHODS.len() as u64) as usize].bytes());
    out.push(b' ');
    out.push(b'/');
    for _ in 0..rng.below(24) {
        out.push(TARGET[rng.below(TARGET.len() as u64) as usize]);
    }
    if rng.below(2) == 0 {
        out.push(b'?');
        for _ in 0..rng.below(16) {
            out.push(TARGET[rng.below(TARGET.len() as u64) as usize]);
        }
    }
    let minor = rng.below(2);
    out.extend(format!(" HTTP/1.{minor}\r\n").bytes());
    let mut headers: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    if minor == 1 {
        headers.push((b"Host".to_vec(), b"example.com".to_vec()));
    }
    for _ in 0..rng.below(6) {
        let name: Vec<u8> =
            (0..1 + rng.below(12)).map(|_| NAME[rng.below(NAME.len() as u64) as usize]).collect();
        let mut value = Vec::new();
        for _ in 0..rng.below(20) {
            value.push(match rng.below(8) {
                0 => b' ',
                1 => b'\t',
                _ => 33 + rng.below(94) as u8,
            });
        }
        headers.push((name, value));
    }
    match rng.below(4) {
        0 => headers.push((b"Content-Length".to_vec(), rng.below(1000).to_string().into_bytes())),
        1 => headers.push((b"Transfer-Encoding".to_vec(), b"chunked".to_vec())),
        2 => headers.push((b"Connection".to_vec(), b"close".to_vec())),
        _ => {}
    }
    for (name, value) in headers {
        out.extend(name);
        out.push(b':');
        for _ in 0..rng.below(3) {
            out.push(if rng.below(4) == 0 { b'\t' } else { b' ' });
        }
        out.extend(value);
        out.extend(b"\r\n");
    }
    out.extend(b"\r\n");
    // Whatever follows is the body or the next request; the parser must not look.
    for _ in 0..rng.below(8) {
        out.push(rng.below(256) as u8);
    }
    out
}

/// What the reference parser says about the same bytes.
enum Reference {
    Complete {
        body: usize,
        method: Vec<u8>,
        target: Vec<u8>,
        minor: u8,
        headers: Vec<(Vec<u8>, Vec<u8>)>,
    },
    Partial,
    Refused,
}

fn reference(bytes: &[u8]) -> Reference {
    let mut slots = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut slots);
    match request.parse(bytes) {
        Ok(httparse::Status::Complete(body)) => Reference::Complete {
            body,
            method: request.method.unwrap().as_bytes().to_vec(),
            target: request.path.unwrap().as_bytes().to_vec(),
            minor: request.version.unwrap(),
            headers: request
                .headers
                .iter()
                .map(|h| (h.name.as_bytes().to_vec(), h.value.to_vec()))
                .collect(),
        },
        Ok(httparse::Status::Partial) => Reference::Partial,
        Err(_) => Reference::Refused,
    }
}

fn frame(cases: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for case in cases {
        out.extend(format!("{}\n", case.len()).bytes());
        out.extend(case);
    }
    out
}

fn mutate(rng: &mut Lcg, mut bytes: Vec<u8>) -> Vec<u8> {
    for _ in 0..1 + rng.below(3) {
        if bytes.is_empty() {
            break;
        }
        let at = rng.below(bytes.len() as u64) as usize;
        match rng.below(5) {
            0 => bytes[at] = rng.below(256) as u8,
            1 => {
                bytes.remove(at);
            }
            2 => bytes.insert(at, rng.below(256) as u8),
            3 => bytes.insert(at, *b"\r\n :\t".get(rng.below(5) as usize).unwrap()),
            _ => bytes.truncate(at),
        }
    }
    bytes
}

#[test]
fn the_library_tests_pass_on_both_backends() {
    // `tests/lex/*.ls` are `lex-sys test` files and `json.rs` runs every
    // one of them on both backends, these included; this is the check that
    // the two files this module owns are among them.
    for name in ["http_test.ls", "route_test.ls"] {
        assert!(repo_root().join("tests/lex").join(name).exists(), "{name}");
    }
}

#[test]
fn the_parser_agrees_with_httparse_and_is_never_looser() {
    let mut rng = Lcg(0x477);
    let mut cases: Vec<Vec<u8>> = Vec::new();
    for _ in 0..600 {
        let request = valid_request(&mut rng);
        cases.push(request.clone());
        for _ in 0..4 {
            cases.push(mutate(&mut rng, request.clone()));
        }
        // Every strict prefix of the head is "incomplete", not an error.
        let head =
            request.windows(4).position(|w| w == b"\r\n\r\n").map_or(request.len(), |p| p + 4);
        let cut = rng.below(head as u64) as usize;
        cases.push(request[..cut].to_vec());
    }

    let (dir, exe) = build_driver("http-driver", "http_driver.ls");
    let out = feed(&exe, &frame(&cases));
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), cases.len());

    let (mut accepted, mut incomplete, mut stricter) = (0, 0, std::collections::BTreeMap::new());
    for (case, line) in cases.iter().zip(&lines) {
        let words: Vec<&str> = line.split(' ').collect();
        let shown = String::from_utf8_lossy(case).escape_debug().to_string();
        let theirs = reference(case);
        if words[0] == "OK" {
            accepted += 1;
            let n: Vec<usize> =
                words[1..].iter().map(|w| w.parse::<i64>().unwrap_or(-1) as usize).collect();
            let Reference::Complete { body, method, target, minor, headers } = theirs else {
                panic!("accepted what httparse refuses or finds partial: {shown}\n{line}");
            };
            assert_eq!(n[0], body, "body offset: {shown}");
            assert_eq!(&case[..n[1]], method.as_slice(), "{shown}");
            assert_eq!(&case[n[2]..n[3]], target.as_slice(), "{shown}");
            assert_eq!(n[5], 10 + minor as usize, "{shown}");
            assert_eq!(n[6], headers.len(), "{shown}");
            for (i, (name, value)) in headers.iter().enumerate() {
                let o = 10 + 4 * i;
                assert_eq!(&case[n[o]..n[o + 1]], name.as_slice(), "{shown}");
                assert_eq!(&case[n[o + 2]..n[o + 3]], value.as_slice(), "{shown}");
            }
        } else {
            let code: u32 = words[1].parse().unwrap();
            if code == 1 {
                incomplete += 1;
                // Not enough yet: the reference must not have a whole request
                // *unless* it takes a bare line feed as a line ending, which
                // this refuses to.
                if let Reference::Complete { .. } = theirs {
                    assert!(bare_line_feed(case), "incomplete where httparse is complete: {shown}");
                }
            } else if let Reference::Complete { .. } = theirs {
                // Stricter than the reference. Allowed for exactly the reasons
                // the design names (`docs/http.md` §3), and counted.
                let allowed = matches!(code, 2 | 4 | 6 | 8 | 9 | 11 | 12);
                assert!(
                    allowed,
                    "refused (code {code}) what httparse accepts, for no declared reason: {shown}"
                );
                *stricter.entry(code).or_insert(0) += 1;
            }
        }
    }
    eprintln!(
        "accepted {accepted}, incomplete {incomplete}, stricter than httparse by code: {stricter:?}"
    );
    assert!(accepted > 500, "the generator must produce plenty of valid requests");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `\n` not preceded by `\r`.
fn bare_line_feed(bytes: &[u8]) -> bool {
    bytes.iter().enumerate().any(|(i, &b)| b == b'\n' && (i == 0 || bytes[i - 1] != b'\r'))
}

#[test]
fn no_byte_string_makes_the_parser_trap() {
    // The differential above feeds mutations of valid requests. This feeds
    // bytes: random ones, of random length, and prefixes of every one. A
    // trap anywhere makes the driver die and the exit status non-zero.
    let mut rng = Lcg(0xbad);
    let mut cases: Vec<Vec<u8>> = Vec::new();
    for _ in 0..1500 {
        let len = rng.below(120) as usize;
        // Biased towards the bytes the grammar is made of, so a good share
        // of these get past the first character.
        const ALPHABET: &[u8] =
            b"GET POST/?:#%\r\n \tHTTP/1.01Host:Content-Length:0123456789a\x00\xff";
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                if rng.below(5) == 0 {
                    rng.below(256) as u8
                } else {
                    ALPHABET[rng.below(ALPHABET.len() as u64) as usize]
                }
            })
            .collect();
        cases.push(bytes);
    }
    let (dir, exe) = build_driver("http-fuzz", "http_driver.ls");
    let out = feed(&exe, &frame(&cases));
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).lines().count(), cases.len());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the router -----------------------------------------------------------

struct RefRoute {
    method: &'static str,
    pattern: String,
    id: usize,
}

/// The matcher, written from the rules in `docs/http.md` §5 and nothing
/// else: segments, `:name`, `*name`, static first, then in order.
fn ref_segments(pattern: &str, path: &str) -> Option<Vec<(usize, usize)>> {
    let pattern_segments: Vec<&str> = pattern[1..].split('/').collect();
    let mut offsets = Vec::new();
    let mut at = 1;
    for part in path[1..].split('/') {
        offsets.push((at, at + part.len()));
        at += part.len() + 1;
    }
    let mut params = Vec::new();
    for (i, seg) in pattern_segments.iter().enumerate() {
        if seg.starts_with('*') {
            let &(start, _) = offsets.get(i)?;
            params.push((start, path.len()));
            return Some(params);
        }
        let &(start, end) = offsets.get(i)?;
        if seg.starts_with(':') {
            if start == end {
                return None;
            }
            params.push((start, end));
        } else if &path[start..end] != *seg {
            return None;
        }
    }
    (offsets.len() == pattern_segments.len()).then_some(params)
}

fn ref_find(routes: &[RefRoute], method: &str, path: &str) -> (i64, Vec<(usize, usize)>) {
    let is_static = |p: &str| !p.contains(':') && !p.contains('*');
    let mut known = false;
    for r in routes.iter().filter(|r| is_static(&r.pattern) && r.pattern == path) {
        if r.method == method {
            return (r.id as i64, Vec::new());
        }
        known = true;
    }
    for r in routes.iter().filter(|r| !is_static(&r.pattern)) {
        if let Some(params) = ref_segments(&r.pattern, path) {
            if r.method == method {
                return (r.id as i64, params);
            }
            known = true;
        }
    }
    (if known { -2 } else { -1 }, Vec::new())
}

#[test]
fn the_router_agrees_with_a_reference_matcher() {
    const METHODS: [&str; 3] = ["GET", "POST", "PUT"];
    const LITERALS: [&str; 5] = ["a", "b", "users", "x1", "files"];
    let mut rng = Lcg(0x907e);
    let mut script = String::new();
    let mut routes: Vec<RefRoute> = Vec::new();
    let mut queries = Vec::new();

    for _ in 0..14 {
        // Routes: one to four segments, some parameters, maybe a rest.
        let n = 1 + rng.below(4) as usize;
        let mut segs: Vec<String> = Vec::new();
        for i in 0..n {
            segs.push(match rng.below(6) {
                0 => format!(":p{i}"),
                1 if i > 0 && i + 1 == n => "*rest".to_string(),
                _ => LITERALS[rng.below(LITERALS.len() as u64) as usize].to_string(),
            });
        }
        let pattern =
            if rng.below(12) == 0 { "/".to_string() } else { format!("/{}", segs.join("/")) };
        let method = METHODS[rng.below(3) as usize];
        let is_static = !pattern.contains(':') && !pattern.contains('*');
        if is_static && routes.iter().any(|r| r.method == method && r.pattern == pattern) {
            continue;
        }
        let id = routes.len();
        script.push_str(&format!("R {method} {pattern} {id}\n"));
        routes.push(RefRoute { method, pattern, id });
    }
    for _ in 0..4000 {
        // Paths from the same alphabet, with the awkward ones mixed in:
        // empty segments, trailing slashes, escapes.
        let n = rng.below(6) as usize;
        let mut path = String::new();
        for _ in 0..n {
            path.push('/');
            path.push_str(match rng.below(10) {
                0 => "",
                1 => "%41",
                2 => "42",
                3 => "zzz",
                _ => LITERALS[rng.below(LITERALS.len() as u64) as usize],
            });
        }
        if path.is_empty() || rng.below(10) == 0 {
            path.push('/');
        }
        let method = METHODS[rng.below(3) as usize];
        script.push_str(&format!("Q {method} {path}\n"));
        queries.push((method, path));
    }

    let (dir, exe) = build_driver("route-driver", "route_driver.ls");
    let out = feed(&exe, script.as_bytes());
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), queries.len());

    let mut counts = [0usize; 3];
    for ((method, path), line) in queries.iter().zip(&lines) {
        let words: Vec<i64> = line.split(' ').map(|w| w.parse().unwrap()).collect();
        let (id, params) = ref_find(&routes, method, path);
        assert_eq!(words[0], id, "{method} {path}");
        for (k, &(start, end)) in params.iter().enumerate() {
            assert_eq!(
                (words[1 + 2 * k], words[2 + 2 * k]),
                (start as i64, end as i64),
                "{method} {path} param {k}"
            );
        }
        counts[if id >= 0 {
            0
        } else if id == -2 {
            1
        } else {
            2
        }] += 1;
    }
    eprintln!(
        "routes {}, queries: matched {}, wrong method {}, no route {}",
        routes.len(),
        counts[0],
        counts[1],
        counts[2]
    );
    assert!(counts.iter().all(|&c| c > 100), "the corpus must exercise every answer: {counts:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_misused_router_or_response_traps_instead_of_going_on() {
    // Each is a program bug, found at registration or at the first response,
    // not on the first unlucky request.
    for (tag, body) in [
        ("no-leading-slash", "r = route.add(heap, r, \"GET\", \"users\", 1);"),
        ("empty-pattern", "r = route.add(heap, r, \"GET\", \"\", 1);"),
        ("trailing-slash", "r = route.add(heap, r, \"GET\", \"/a/\", 1);"),
        ("empty-segment", "r = route.add(heap, r, \"GET\", \"/a//b\", 1);"),
        ("nameless-param", "r = route.add(heap, r, \"GET\", \"/a/:\", 1);"),
        ("rest-not-last", "r = route.add(heap, r, \"GET\", \"/a/*r/b\", 1);"),
        ("negative-id", "r = route.add(heap, r, \"GET\", \"/a\", 0 - 1);"),
        ("empty-method", "r = route.add(heap, r, \"\", \"/a\", 1);"),
        (
            "duplicate-static",
            "r = route.add(heap, r, \"GET\", \"/a\", 1); r = route.add(heap, r, \"GET\", \"/a\", 2);",
        ),
    ] {
        let dir = scratch(&format!("route-misuse-{tag}"));
        let source = dir.join("misuse.ls");
        std::fs::write(
            &source,
            format!(
                "import std.route;\n\n\
                 fn run[&r](heap: &!r Heap) -> [heap] int {{\n\
                     var r = route.empty(heap);\n\
                     {body}\n\
                     route.drop(heap, r);\n\
                     return 0;\n\
                 }}\n\n\
                 fn main(world: World) -> [] int {{\n\
                     let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
                     release(args); release(fs); release(ffi); release(io);\n\
                     var status = 0;\n\
                     borrow mut heap as &!h in {{ status = run(h); }}\n\
                     release(heap);\n\
                     return status;\n\
                 }}\n"
            ),
        )
        .expect("a writable fixture");
        let exe = dir.join("misuse");
        let build = Command::new(BIN)
            .args(["build", "--std"])
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{tag}: {}", String::from_utf8_lossy(&build.stderr));
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), None, "{tag} should be killed by a signal, not exit");
        let _ = std::fs::remove_dir_all(&dir);
    }

    for (tag, body) in [
        (
            "header-injection",
            "out = http.respond_head(heap, out, 200, \"text/plain\\r\\nSet-Cookie: x=1\", 0, true);",
        ),
        ("status-out-of-range", "out = http.respond_head(heap, out, 42, \"text/plain\", 0, true);"),
        (
            "negative-length",
            "out = http.respond_head(heap, out, 200, \"text/plain\", 0 - 1, true);",
        ),
        // `extra` is whole `name: value` lines or it traps: a bare line feed
        // would end the head early, and what followed would be a body the client
        // never asked for.
        (
            "extra-bare-line-feed",
            "out = http.respond_head_with(heap, out, 200, \"text/plain\", 0, true, \"X: a\\nSet-Cookie: b\\r\\n\");",
        ),
        (
            "extra-bare-carriage-return",
            "out = http.respond_head_with(heap, out, 200, \"text/plain\", 0, true, \"X: a\\rY: b\\r\\n\");",
        ),
        (
            "extra-without-a-colon",
            "out = http.respond_head_with(heap, out, 200, \"text/plain\", 0, true, \"not a header\\r\\n\");",
        ),
        (
            "extra-without-its-line-ending",
            "out = http.respond_head_with(heap, out, 200, \"text/plain\", 0, true, \"X: a\");",
        ),
        (
            "extra-a-blank-line",
            "out = http.respond_head_with(heap, out, 200, \"text/plain\", 0, true, \"X: a\\r\\n\\r\\nbody\");",
        ),
    ] {
        let dir = scratch(&format!("http-misuse-{tag}"));
        let source = dir.join("misuse.ls");
        std::fs::write(
            &source,
            format!(
                "import std.buffer;\nimport std.http;\n\n\
                 fn run[&r](heap: &!r Heap) -> [heap] int {{\n\
                     var out = buffer.empty(heap, 16);\n\
                     {body}\n\
                     buffer.drop(heap, out);\n\
                     return 0;\n\
                 }}\n\n\
                 fn main(world: World) -> [] int {{\n\
                     let Split {{ io, ffi, fs, heap, args }} = split(world);\n\
                     release(args); release(fs); release(ffi); release(io);\n\
                     var status = 0;\n\
                     borrow mut heap as &!h in {{ status = run(h); }}\n\
                     release(heap);\n\
                     return status;\n\
                 }}\n"
            ),
        )
        .expect("a writable fixture");
        let exe = dir.join("misuse");
        let build = Command::new(BIN)
            .args(["build", "--std"])
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("the compiler runs");
        assert!(build.status.success(), "{tag}: {}", String::from_utf8_lossy(&build.stderr));
        let run = Command::new(&exe).output().expect("the program runs");
        assert_eq!(run.status.code(), None, "{tag} should be killed by a signal, not exit");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------------------------------------------------------------------
// `std.http.dechunk`, against a second decoder
// ---------------------------------------------------------------------

/// The same rules as `dechunk`'s header states them, written independently --
/// line by line over the bytes rather than by a cursor -- so that agreeing is
/// evidence and not an echo. `(consumed, decoded)`; `consumed` is -1 for "not
/// all here", -2 a bad size, -3 bad framing, -4 too large for `room`, -5 an
/// extension or a trailer.
fn reference_dechunk(src: &[u8], room: usize) -> (i64, Vec<u8>) {
    let mut at = 0usize;
    let mut out = Vec::new();
    loop {
        // The size: hex digits up to the first byte that is not one.
        let start = at;
        while at < src.len() && src[at].is_ascii_hexdigit() {
            if at - start == 8 {
                return (-2, Vec::new());
            }
            at += 1;
        }
        if at >= src.len() {
            return (-1, Vec::new());
        }
        if at == start {
            return (-2, Vec::new());
        }
        let size =
            usize::from_str_radix(std::str::from_utf8(&src[start..at]).unwrap(), 16).unwrap();
        match src[at] {
            b';' => return (-5, Vec::new()),
            b'\r' => {}
            _ => return (-3, Vec::new()),
        }
        match src.get(at + 1) {
            None => return (-1, Vec::new()),
            Some(b'\n') => {}
            Some(_) => return (-3, Vec::new()),
        }
        at += 2;
        if size == 0 {
            return match (src.get(at), src.get(at + 1)) {
                (None, _) => (-1, Vec::new()),
                (Some(b'\r'), None) => (-1, Vec::new()),
                (Some(b'\r'), Some(b'\n')) => ((at + 2) as i64, out),
                (Some(b'\r'), Some(_)) => (-3, Vec::new()),
                (Some(_), _) => (-5, Vec::new()),
            };
        }
        if out.len() + size > room {
            return (-4, Vec::new());
        }
        if at + size + 2 > src.len() {
            return (-1, Vec::new());
        }
        out.extend_from_slice(&src[at..at + size]);
        if &src[at + size..at + size + 2] != b"\r\n" {
            return (-3, Vec::new());
        }
        at += size + 2;
    }
}

fn chunked_body(rng: &mut Lcg) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..rng.below(5) {
        let size = rng.below(40) as usize + 1;
        // Data may hold anything, a CRLF and a chunk-looking line included.
        let data: Vec<u8> = (0..size)
            .map(|_| match rng.below(8) {
                0 => b'\r',
                1 => b'\n',
                2 => b';',
                _ => rng.below(256) as u8,
            })
            .collect();
        let text = if rng.below(2) == 0 { format!("{size:x}") } else { format!("{size:X}") };
        out.extend(text.bytes());
        out.extend(b"\r\n");
        out.extend(data);
        out.extend(b"\r\n");
    }
    out.extend(b"0\r\n\r\n");
    // And sometimes something pipelined behind it.
    if rng.below(3) == 0 {
        out.extend(b"GET / HTTP/1.1\r\n");
    }
    out
}

/// 600 generated chunked bodies, each mutated four ways and cut once, and the
/// decoder agrees with the reference on all 3,600: the same `consumed`, the
/// same decoded bytes, with 256 bytes of room so that "too large" is reached.
/// Not one of them may trap -- that is what the same run asserts by exiting 0.
#[test]
fn the_chunked_decoder_agrees_with_a_second_decoder_on_valid_and_mutated_bodies() {
    let mut rng = Lcg(0xc4);
    let mut cases: Vec<Vec<u8>> = Vec::new();
    for _ in 0..600 {
        let body = chunked_body(&mut rng);
        cases.push(body.clone());
        for _ in 0..4 {
            cases.push(mutate(&mut rng, body.clone()));
        }
        let cut = rng.below(body.len() as u64 + 1) as usize;
        cases.push(body[..cut].to_vec());
    }
    // Sizes that overflow, that would overflow `size * 16`, and lots of room.
    for text in ["ffffffff\r\n", "100000000\r\n", "fffffffe\r\nx", "7fffffff\r\n"] {
        cases.push(text.as_bytes().to_vec());
    }

    let (dir, exe) = build_driver("dechunk-driver", "dechunk_driver.ls");
    let out = feed(&exe, &frame(&cases));
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), cases.len());

    let mut by_code = std::collections::BTreeMap::new();
    for (case, line) in cases.iter().zip(&lines) {
        let shown = String::from_utf8_lossy(case).escape_debug().to_string();
        let words: Vec<&str> = line.split(' ').collect();
        let consumed: i64 = words[0].parse().unwrap();
        let (want, bytes) = reference_dechunk(case, 256);
        assert_eq!(consumed, want, "consumed, for {shown}\n{line}");
        if want > 0 {
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(words.get(2).copied().unwrap_or(""), hex, "decoded, for {shown}");
        }
        *by_code.entry(if want > 0 { 1 } else { want }).or_insert(0) += 1;
    }
    // Every outcome was exercised, or the test proves less than it says.
    for code in [1, -1, -2, -3, -4, -5] {
        assert!(
            by_code.get(&code).copied().unwrap_or(0) > 0,
            "no case reached {code}: {by_code:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
