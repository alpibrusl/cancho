# Tty: a capability for serial ports

> **Status: design (T1 of #422), nothing built.** Every ABI number below
> is measured, not remembered: `cancho-robot`'s spike
> (`spike/ping-at-1mbaud`, `spikes/ping/abi.c` on darwin-aarch64 and
> `spikes/ping/abi_linux.c` on the robot's Raspberry Pi 5, its research
> log entries of 2026-10-08), which also holds the one measurement this
> document is built to make unnecessary: the variadic-`ioctl` hack that
> is the difference between a working and a silently dead servo bus.

---

## 1. Why a capability, in one paragraph

The spike drove a Feetech servo bus from cancho at 1,000,000 baud
through nine foreign functions, and the program source carried the ABI
itself: `sizeof(struct termios)`, the offset of `c_cflag`, the octal
`B1000000`, `O_NONBLOCK` — all measured, all *per-platform* (the
numbers differ between macOS and Linux in almost every case), and all
invisible to the authority report, which answered eleven `libc` symbol
names and "never touches the filesystem" while opening
`/dev/cu.usbmodem5B610332201` (research log, finding 6). A port is a
capability, not a libc escape hatch: `Tty` gives serial lines the shape
`File` and `Conn` already have — the program borrows, the row says the
truth, and the termios layout, the octal B-constants, the raw-mode
flags and the macOS `ioctl` variadic trap move into the backend, where
they are written once in Rust instead of per-program in cancho.

The `usleep` the spike's tenth symbol was is already gone: waiting is
a `Poller` with nothing registered, and ten waits of 50 ms measured
501 ms by `clock_ms` — the `[poll]` row is the whole difference (#422,
T3's measurement).

## 2. The two ABIs, measured

| | darwin-aarch64 (spike) | linux-aarch64, glibc 2.41 (Pi 5) |
|---|---|---|
| `sizeof(struct termios)` | 72 | 60 |
| field width | 8 bytes | 4 bytes |
| `c_cflag` at | 16 | 8 |
| `c_cc` at | 32 | 17 |
| `c_ispeed` / `c_ospeed` at | 56 / 64 | not printed in the log; glibc 2.41 stores the B-constant in them (measured 4104 after `cfsetspeed(B1000000)`) — the offsets come from `abi_linux.c`'s table when T2 implements, not from memory |
| `VMIN` / `VTIME` | 16 / 17 | 6 / 5 |
| `O_NONBLOCK` | 4 | 2048 |
| `O_NOCTTY` | 131072 | 256 |
| `CLOCAL` / `CREAD` | 0x8000 / 0x800 | 0x800 / 0x80 |
| `TCIFLUSH` | 1 | 0 |
| 1,000,000 baud | not a standard rate: `tcsetattr` answers `EINVAL`, nothing applied — raw mode at a standard rate first, then `ioctl(fd, IOSSIOSPEED, &speed)` | a standard rate: `B1000000` (010010 octal) in `c_cflag`'s `CBAUD` bits; `tcsetattr` alone |
| the macOS `IOSSIOSPEED` | `0x80085402` as measured by this spike (`_IOW('T', 2, speed_t)`, 8-byte `speed_t`); **pyserial 3.5 hard-codes `0x80045402`** (a 4-byte size) — both accepted by this driver as far as observed, but the number in a program should be the measured one, and neither belongs in a program |


Every one of these is a constant a program writing `extern fn` has to
get right per platform today, with no help from the compiler. None of
them reaches program source under `Tty`.

**The variadic trap, recorded because it is the argument for the whole
epic.** On darwin-aarch64, named arguments travel in x0–x7 and a
variadic one on the stack. `ioctl` declared as an ordinary three-argument
function returned **0** — success — and set the speed to garbage (read
back `0xfffffffffffffff0`); with six padding integers so the pointer
lands in the first stack slot, the speed took and every servo answered.
A silent wrong configuration, not an error (research log, findings 4
and 9: the control run that makes it the whole difference). In the
backend this is one `ioctl` call written correctly, once; in a program
it is an ABI hack that happens to work on one target.

## 3. The surface

```cho
res Tty   -- a serial port, linear: one closer

tty_open(t: &Tty(""), path: &[byte]) -> Opening     // Ok(Tty) | Failed(errno)
tty_configure(t: &!Tty, baud: int) -> int           // raw, 8N1, a speed; 0, or the errno
tty_read(t: &!Tty, into: &![byte]) -> int          // what is there, never blocks; -1 on error
tty_write(t: &Tty, bytes: &[byte]) -> int          // whole, or short; -1 on error
tty_flush_input(t: &!Tty) -> int
tty_close(t: Tty) -> int
poller_add_tty(p: &!Poller, t: &Tty, token: int, events: int) -> int
```

`Opening` is the enum `File`'s verbs answer (`Opened`/`Failed` shape,
the `File`/`Udp` precedent) rather than a bare `int`, so a refusal is
a value the caller matches, not a number it has to know the sign
convention of. Configuration is one call rather than a mode algebra:
the robot needs exactly one setting (raw, 8N1, a speed), and a mode
algebra in the language would re-open the termios surface this
capability exists to close. `tty_read` never blocks: the port is opened
`O_RDWR | O_NOCTTY | O_NONBLOCK`, which is also why read and write
take a `&!Tty` / `&Tty` split like `Conn`'s verbs, and why the poller —
not a `VMIN`/`VTIME` pair — is the waiting story.

`poller_add_tty` takes `events` like `poller_add_conn`'s, registering
the raw descriptor; the family (`poller_add_listener`, `_conn`, `_udp`,
`_signals`, `_pipe`) is the precedent and the poller table is where it
plugs in.

## 4. Authority: the row says the truth

`tty_open` reads a path under the capability's bound, so the labels
are argument-carrying and narrowable exactly as `fs_read` is:

```
performs
    tty_read("/dev")
    tty_write("/dev")
never touches
    ...
```

and `narrow(tty, "/dev/serial")` pins it further. The bounded/narrowed
shape answers the report's headline gap (finding 6): a robot-shaped
program reports `bounded: true` with its tty labels and **no `foreign`
lines** — T4's acceptance, and the sentence cancho-robot was founded to
make.

**Decision 1 — narrowing: prefix-shaped, non-empty.** `Tty` narrows by
path prefix the way `Fs` does, and the bound is non-empty for the same
reason `narrow(fs, "")` is refused: an empty bound is the unnarrowed
root, and reporting it as bounded is the lie `net-bounds.md` §2.4 just
recorded for `Net`. A tool whose port is an argument cannot narrow by
a runtime value (that is #330's open question, unchanged by this
epic); it reports the prefix it was granted, and its ceiling file says
so. `Tty("/dev")` is the shape a program names the node beneath
(`tls_echo_fixed`'s pattern), not `Tty("")`.

**Decision 2 — speeds: a plain integer, validated by the backend.**
Named baud constants would re-encode libc's octal table into the
language; exposing the B-constants is the leak this epic closes. The
backend validates the integer against the platform's supported set
(Linux: the standard rates including 1,000,000; macOS: the standard
rates for the `tcsetattr` step, then `IOSSIOSPEED` for anything else)
and answers the `errno` on refusal. The robot needs exactly one
speed; the vocabulary stays one `int` wide.

## 5. What is refused, not trapped

Every refusal carries a rule tag; none of these reaches a panic:

| Condition | Answer |
|---|---|
| `tty_open` under a bound the path is outside | a located `tty-path` refusal at compile time (the `fs_read` shape) |
| `tty_open` of a path that is not a device or will not open | `Failed(errno)` in `Opening` — the program's question, not the compiler's |
| `tty_configure` of a speed the platform refuses | the `errno` (`EINVAL` on macOS past the standard set, measured), `0` on success |
| read/write/flush on a closed or errored port | `-1`, the `Conn` verbs' convention |
| `poller_add_tty` of an unconfigured port | allowed: registration is orthogonal to configuration, the `poller_add_udp` precedent |

A new rule tag `tty-path` joins the catalogue for the compile-time
bound refusal; the runtime answers are values, not diagnostics.

## 6. Edition

**Edition 8.** `Split` grows its ninth field (`Tty`, beside `exec`'s
eighth), gated the way `Net`, `Clock` and `Signals` each were
(`PRELUDE_TTY` in `ir.rs`, the `defs.rs` capability tables, the
parser's edition gate, the edition marker in every fixture the new
surface touches). Edition 7 is shipped and stable at head; no
migration is in flight to collide with, which answers the epic's
fourth open decision. Note for sequencing: #400's word-scan also
targets edition 8 — both are additive and can share it; whichever
lands second inherits the marker.

## 7. Non-goals, held explicitly

* **Not a device framework.** V4L2/camera `ioctl` is a measured
  question (the robot's spike list), not a commitment; `Tty`'s
  vocabulary is sized so answering it does not reopen this epic.
* **No new waiting story.** The `Poller` is the waiter; `Tty` joins its
  family rather than inventing a sleep.
* **Not `Fs`.** A device node is not a file: no line properties
  through `Fs`, and `fs_read`'s whole-file shape is not a poll.

## 8. The slices

The epic's T1–T5, each with its acceptance as written there. T2's
example (`examples/tty_hello.cho`) is shaped so its
hardware-independent halves — the refused paths, the configuration
validation, the flush — are fixture-gated in CI, with the one
real-port run the maintainer's checkbox: the servo bus is the
acceptance criterion T5 re-runs on the robot, not something cancho's
CI can or should simulate.

## 9. What this document does not claim

* No throughput number for the capability vs the FFI path: T3's
  measurement (the 501 ms baseline) is about *waiting*, not copying,
  and is the only performance claim in the epic. Any other claim
  waits for its measurement.
* No Windows story. The robot lives on a Pi and the spike hit macOS;
  a third target is a new spike, not a port of this design.
* `cancho-robot`'s `serial.cho` rewrite (T5) is cross-repo and
  hardware-gated; nothing here can run those gates, which is why they
  stay the epic's acceptance rather than this document's.
