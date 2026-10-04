//! The signals a program may claim, and the numbers the two kernels give them
//! (`docs/signals.md` section 2.2), in one place so the two backends cannot
//! disagree and so a name is checked once, where `narrow` is written.

/// One claimable signal.
#[derive(Clone, Copy, Debug)]
pub struct Signal {
    /// The name without the `SIG` prefix, as it is written in a set.
    pub name: &'static str,
    /// Its bit in the `int` `signals_pending` answers: fixed by lex-sys, the
    /// same on every target, because the numbers are not.
    pub bit: i64,
    /// Its number on Linux (x86-64 and aarch64 agree).
    pub linux: i64,
    /// Its number on Darwin.
    pub darwin: i64,
}

/// Every claimable signal, **in canonical (alphabetical) order**: the order a
/// set is printed in, so two programs that name one set have one type and one
/// row (`docs/signals.md` section 2.1).
pub const CLAIMABLE_SIGNALS: [Signal; 8] = [
    Signal { name: "ALRM", bit: 64, linux: 14, darwin: 14 },
    Signal { name: "HUP", bit: 1, linux: 1, darwin: 1 },
    Signal { name: "INT", bit: 2, linux: 2, darwin: 2 },
    Signal { name: "QUIT", bit: 4, linux: 3, darwin: 3 },
    Signal { name: "TERM", bit: 8, linux: 15, darwin: 15 },
    Signal { name: "USR1", bit: 16, linux: 10, darwin: 30 },
    Signal { name: "USR2", bit: 32, linux: 12, darwin: 31 },
    Signal { name: "WINCH", bit: 128, linux: 28, darwin: 28 },
];

/// `EBUSY`, the same on Linux and Darwin: a `signals_watch` that cannot be
/// granted because a thread is running or the signal is already claimed.
pub const EBUSY: i64 = 16;

/// The runtime's two words of signal state, defined once per program by the
/// entry point: the native mask of every signal a live `SignalWatch` holds,
/// then the number of spawned threads still running.
pub const SIGNAL_STATE_GLOBAL: &str = "lexs_signal_state";

/// A set of claimable signals: its canonical spelling and its bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalSet {
    /// `"INT,TERM"`: alphabetical, comma-separated, no spaces.
    pub canonical: String,
    /// The OR of the members' [`Signal::bit`].
    pub bits: i64,
}

/// Signals that cannot be caught: the kernel does not let a program have them.
const UNCATCHABLE: [&str; 2] = ["KILL", "STOP"];

/// Signals a fault raises. Blocking one a fault raised does not defer it, the
/// kernel ends the process; and lex-sys ends a trap with `SIGILL`.
const FAULTS: [&str; 7] = ["SEGV", "ILL", "BUS", "FPE", "ABRT", "TRAP", "SYS"];

/// Signals a program could ask for and none has: the table grows when a
/// program asks (`CONTRIBUTING.md`: two askers).
const NOT_YET: [&str; 14] = [
    "PIPE", "CHLD", "CONT", "TSTP", "TTIN", "TTOU", "URG", "IO", "XCPU", "XFSZ", "VTALRM", "PROF",
    "STKFLT", "PWR",
];

/// Parse and check a set written as `"INT,TERM"`: names without `SIG`, comma
/// separated, each once, in any order. The `Err` is the sentence the
/// refusal (`signal-not-claimable`) carries.
pub fn parse_signal_set(text: &str) -> Result<SignalSet, String> {
    if text.is_empty() {
        return Err("a set of signals names at least one, such as `\"TERM\"` or `\"INT,TERM\"`; \
                    the empty set claims nothing"
            .to_owned());
    }
    let mut bits = 0;
    for name in text.split(',') {
        let Some(found) = CLAIMABLE_SIGNALS.iter().find(|s| s.name == name) else {
            return Err(refusal(name));
        };
        if bits & found.bit != 0 {
            return Err(format!(
                "`{name}` is named twice in `{text}`; a set names each signal once"
            ));
        }
        bits |= found.bit;
    }
    Ok(SignalSet { canonical: canonical_signal_set(bits), bits })
}

/// The canonical spelling of a set of bits.
pub fn canonical_signal_set(bits: i64) -> String {
    let names: Vec<&str> =
        CLAIMABLE_SIGNALS.iter().filter(|s| bits & s.bit != 0).map(|s| s.name).collect();
    names.join(",")
}

/// Why a name is not in the table, as a sentence.
fn refusal(name: &str) -> String {
    let claimable = CLAIMABLE_SIGNALS.map(|s| s.name).join(", ");
    if UNCATCHABLE.contains(&name) {
        return format!(
            "`{name}` cannot be caught: the kernel does not let a program have it. The signals a \
             program may claim are {claimable}"
        );
    }
    if FAULTS.contains(&name) {
        return format!(
            "`{name}` is a fault, not a request: blocking one that a fault raised does not defer \
             it (the kernel ends the process), and lex-sys ends a trap with `SIGILL`. The signals \
             a program may claim are {claimable}"
        );
    }
    if NOT_YET.contains(&name) {
        return format!(
            "`{name}` is not claimable (yet): the table grows when a program asks for a signal. \
             The signals a program may claim are {claimable}"
        );
    }
    if let Some(bare) = name.strip_prefix("SIG")
        && (CLAIMABLE_SIGNALS.iter().any(|s| s.name == bare)
            || UNCATCHABLE.contains(&bare)
            || FAULTS.contains(&bare)
            || NOT_YET.contains(&bare))
    {
        return format!(
            "write `{bare}`, not `{name}`: a set names signals without the `SIG` prefix. The \
             signals a program may claim are {claimable}"
        );
    }
    format!(
        "`{name}` is not a signal name this compiler knows; a set is names without the `SIG` \
         prefix, upper case, comma separated, with no spaces. The signals a program may claim \
         are {claimable}"
    )
}

/// Does holding the signals `held` authorise claiming `wanted`? A set, not a
/// prefix: the empty set is the root and covers everything, any other covers
/// exactly its subsets (`Label::covers`, `docs/signals.md` section 2.1).
pub(crate) fn signal_set_covers(held: &str, wanted: &str) -> bool {
    if held.is_empty() {
        return true;
    }
    match (parse_signal_set(held), parse_signal_set(wanted)) {
        (Ok(held), Ok(wanted)) => wanted.bits & !held.bits == 0,
        _ => false,
    }
}

/// The native signal number of a claimable signal on a target.
pub fn native_signal_number(signal: &Signal, darwin: bool) -> i64 {
    if darwin { signal.darwin } else { signal.linux }
}

/// The kernel's own bitmask (bit `number - 1`) for a set of [`Signal::bit`]s.
pub fn native_signal_mask(bits: i64, darwin: bool) -> i64 {
    CLAIMABLE_SIGNALS
        .iter()
        .filter(|s| bits & s.bit != 0)
        .fold(0, |mask, s| mask | 1 << (native_signal_number(s, darwin) - 1))
}

#[cfg(test)]
#[path = "tests/signals.rs"]
mod tests;
