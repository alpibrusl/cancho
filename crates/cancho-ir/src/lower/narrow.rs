//! `narrow` into several capabilities (`docs/narrowing-into-several.md`): the
//! per-literal check the single form shares, the pairwise check, and the tuple
//! the multi-literal form answers.

use crate::*;

/// `Net` and its two halves of edition 11 (`docs/net-bounds.md` §3) are bounded by the same `host:port` text.
pub(super) fn is_net(which: usize) -> bool {
    which == PRELUDE_NET || which == PRELUDE_OUT || which == PRELUDE_PORTS
}

/// The device capabilities (`docs/tty.md` §4, `docs/camera.md` §4, `docs/audio.md` §4) are bounded by a device-path
/// prefix, narrowed as `Fs`'s is.
pub(super) fn is_device(which: usize) -> bool {
    which == PRELUDE_TTY
        || which == PRELUDE_CAMERA
        || which == PRELUDE_MIC
        || which == PRELUDE_SPEAKER
}

/// A `Net` bound as the run time reads it: the host half, matched as a prefix, and the port half, matched
/// exactly (`crates/cancho-codegen/src/body/net.rs`, `connect_raw`, and `docs/connect.md` section 10.1). A bound
/// with a `:` splits at the last one; bare digits are a port with no host (a listener's, `docs/listen.md`);
/// anything else is a host with no port named, which any port satisfies.
fn net_parts(bound: &str) -> (&str, Option<&str>) {
    match bound.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None if !bound.is_empty() && bound.bytes().all(|b| b.is_ascii_digit()) => ("", Some(bound)),
        None => (bound, None),
    }
}

/// Whether everything `inner` admits, `outer` admits too: `inner`'s host starts with `outer`'s, and `outer` names
/// no port or the same one. This is the relation between a `Net` and a `Net` narrowed from it, and the one that
/// makes two sibling bounds redundant; the whole-string prefix it replaces is neither (`127.0.0.1:80` is a prefix of
/// `127.0.0.1:8080`, which it does not admit, and not of `127.0.0.10:80`, which it does).
///
/// With `exact` (edition 10, `docs/net-bounds.md` §2.3) the host half is not a prefix: an empty host admits any,
/// `*.suffix` admits that bound and any one label ahead of `.suffix`, and every other host admits only itself.
fn net_contains(outer: &str, inner: &str, exact: bool) -> bool {
    let (outer_host, outer_port) = net_parts(outer);
    let (inner_host, inner_port) = net_parts(inner);
    let host = if !exact {
        inner_host.starts_with(outer_host)
    } else if outer_host.is_empty() || outer_host == inner_host {
        true
    } else if let Some(suffix) = outer_host.strip_prefix("*.") {
        inner_host
            .strip_suffix(suffix)
            .and_then(|label| label.strip_suffix('.'))
            .is_some_and(|label| !label.is_empty() && !label.contains(['.', '*']))
    } else {
        false
    };
    host && (outer_port.is_none() || outer_port == inner_port)
}

/// What one literal must satisfy to narrow `current`, the single form and
/// each member of the multi-literal form alike: inside `current`, at a
/// separator for the two path capabilities, and not equal to it.
pub(super) fn check_narrowing(
    which: usize,
    current: &str,
    target: &str,
    span: Span,
    edition: u32,
) -> Result<(), Diagnostic> {
    // From edition 10 a `Net` bound is not a textual prefix of what it admits (`*.example.com` admits
    // `api.example.com`), so `net_contains` below is its whole containment test.
    if !(is_net(which) && edition >= 10) && !target.starts_with(current) {
        return Err(Diagnostic::new(
            Rule::CapabilityNotNarrowable,
            format!(
                "`{current}` cannot be narrowed to `{target}`: a capability is attenuated, never widened, and a program must not be able to grant itself what it was not given"
            ),
            span,
        ));
    }
    // A path prefix extends at a separator or not at all. `/tmp` is not
    // a prefix of `/tmpevil` in any sense a filesystem would recognise,
    // and a textual check that said otherwise would hand a program the
    // directory next door. `Ffi` has no separator and no such case, and
    // neither does `Net`: `docs/net.md` §4 bounds a `net_out` label by
    // plain textual prefix on `"host:port"`, the same way an `egress`
    // entry does, with no boundary character of its own.
    // `docs/processes.md` §4.1: `Exec`'s prefix is a path, with `Fs`'s rule.
    if (which == PRELUDE_FS || which == PRELUDE_EXEC || is_device(which))
        && !extends_path(current, target)
    {
        return Err(Diagnostic::new(
            Rule::CapabilityNotNarrowable,
            format!(
                "`{current}` cannot be narrowed to `{target}`: a path prefix extends at a `/`, and `{target}` is a different name that merely starts with the same bytes"
            ),
            span,
        ));
    }
    // A `Net` bound's port is matched exactly, so a longer string is not a narrower grant: `127.0.0.1:80` is
    // a prefix of `127.0.0.1:8080`, whose port the parent does not admit.
    if is_net(which) && !net_contains(current, target, edition >= 10) {
        return Err(Diagnostic::new(
            Rule::CapabilityNotNarrowable,
            format!(
                "`{current}` cannot be narrowed to `{target}`: a `Net` bound's port is exact, and from edition 10 so is its host, and `{target}` admits what `{current}` does not, which would widen it"
            ),
            span,
        ));
    }
    if target == current {
        return Err(Diagnostic::new(
            Rule::CapabilityNotNarrowable,
            format!("this narrows `{current}` to itself, which grants nothing new"),
            span,
        ));
    }
    Ok(())
}

/// Why `narrow(cap, "a", "b", ...)` is refused on a capability that is not a
/// path (section 5, item 4): the message names the set form where there is one.
pub(super) fn narrow_many_refusal(which: usize) -> String {
    match which {
        PRELUDE_FFI => "`narrow` into several capabilities is for `Fs`, `Exec` and `Net`; `Ffi` narrows to a set in one capability: `narrow(ffi, \"libc,libm\")` (`docs/foreign-authority.md` section 5.2)".to_string(),
        PRELUDE_SIGNALS => "`narrow` into several capabilities is for `Fs`, `Exec` and `Net`; `Signals` narrows to a set in one capability: `narrow(signals, \"TERM,INT\")` (`docs/signals.md` section 2.1)".to_string(),
        _ => "`narrow` into several capabilities is for `Fs`, `Exec` and `Net`; this capability is not supported there (`docs/narrowing-into-several.md` section 5, item 4)".to_string(),
    }
}

/// `narrow(cap, "a", "b", ...)`: every literal checked as the single form
/// checks it, no two equal and none inside another, and a tuple of `cap`'s
/// type narrowed to each, in the order written.
///
/// The capability is lowered once, as an owned use (so it is consumed, and the
/// children are the only way on). It is zero-sized, so the tuple's other
/// components are empty tuples: they carry no leaf, and neither backend emits
/// anything for them (`docs/narrowing-into-several.md` section 4.1).
pub(super) fn narrow_into_several(
    value: Expr,
    which: usize,
    current: &str,
    targets: &[String],
    spans: &[Span],
    span: Span,
    edition: u32,
) -> Result<(Expr, Type), Diagnostic> {
    for (target, at) in targets.iter().zip(spans) {
        check_narrowing(which, current, target, *at, edition)?;
    }
    for (i, a) in targets.iter().enumerate() {
        for (j, b) in targets.iter().enumerate().skip(i + 1) {
            // A `Net` bound is a host prefix and an exact port, so one inside another is `net_contains`; the
            // paths extend at a `/`.
            let nested = if is_net(which) {
                net_contains(a, b, edition >= 10) || net_contains(b, a, edition >= 10)
            } else {
                extends_path(a, b) || extends_path(b, a)
            };
            let what = if is_net(which) { "bounds" } else { "paths" };
            let why = if a == b && is_net(which) {
                "are the same bound, so the second grants nothing the first does not"
            } else if a == b {
                "are the same path, so the second grants nothing the first does not"
            } else if nested {
                "are nested, so the inner one grants nothing the outer one does not"
            } else {
                continue;
            };
            return Err(Diagnostic::new(
                Rule::CapabilityNotNarrowable,
                format!(
                    "`narrow` into several capabilities needs unrelated {what}, and `{a}` and `{b}` {why}"
                ),
                spans.get(j).copied().unwrap_or(span),
            ));
        }
    }
    let mut parts = vec![value];
    parts.extend(targets.iter().skip(1).map(|_| Expr::Tuple { parts: Vec::new() }));
    let child = |target: &String| Type::Named(DefId(which as u32), vec![Type::Lit(target.clone())]);
    Ok((Expr::Tuple { parts }, Type::Tuple(targets.iter().map(child).collect())))
}
