module std.signals;

// `std.signals` -- the bits `signals_pending` answers, by name.
//
// `docs/signals.md` section 6. `signals_pending(watch)` answers an `int`
// with one bit per claimed signal that arrived since the previous call.
// The bits are fixed by lex-sys and the same on every target (the signal
// numbers are not: `USR1` is 10 on Linux and 30 on macOS), and a number like
// `8` for `TERM` is one a reader has to look up. So a program says
// `has(mask, sigterm())`.
//
// No effects and no capability: these are numbers. Claiming a signal is
// `signals_watch`, whose row names it.

pub fn sighup() -> [] int {
    return 1;
}

pub fn sigint() -> [] int {
    return 2;
}

pub fn sigquit() -> [] int {
    return 4;
}

pub fn sigterm() -> [] int {
    return 8;
}

pub fn sigusr1() -> [] int {
    return 16;
}

pub fn sigusr2() -> [] int {
    return 32;
}

pub fn sigalrm() -> [] int {
    return 64;
}

pub fn sigwinch() -> [] int {
    return 128;
}

// The signals that mean "stop": `INT`, `QUIT` and `TERM`.
pub fn stop_signals() -> [] int {
    return 2 | 4 | 8;
}

// Is every bit of `bits` set in `mask`? `has(mask, sigterm())` is "a `TERM`
// arrived"; `has(mask, stop_signals())` asks for all three at once, and
// `any(mask, stop_signals())` for any.
pub fn has(mask: int, bits: int) -> [] bool {
    return mask & bits == bits;
}

// Is any bit of `bits` set in `mask`? `any(mask, stop_signals())` is "something asked us to stop".
pub fn any(mask: int, bits: int) -> [] bool {
    return mask & bits != 0;
}
