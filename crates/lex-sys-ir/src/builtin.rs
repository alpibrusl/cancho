//! The functions the compiler provides rather than a program defining
//! them, and what each one's signature and effect are.

use crate::*;

/// Functions the compiler provides rather than the program defining them.
///
/// M0/M1 scaffolding: `putchar` is how a program produces output before there
/// is any FFI. M2 replaces it with a capability-gated foreign call — output is
/// an effect, and an effect must be granted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Builtin {
    /// `putchar[&i](io: &!i Io, c: int) -> [io_write] int` — libc's `putchar`,
    /// byte for byte, behind the capability that authorises it.
    ///
    /// The `Io` is not passed to libc and has no runtime representation. It
    /// is there so that a function which does not hold one cannot call this,
    /// which is the whole safety story stated as a type (§8.2).
    PutChar,
    /// `write_bytes[&i, &b](io: &!i Io, bytes: &b [byte]) -> [io_write] int` —
    /// a whole slice, behind the same capability (`docs/bulk-io.md` §3).
    ///
    /// The comment above `PutChar` said M2 would replace it with "a
    /// capability-gated foreign call". That turned out to be the wrong
    /// shape and `bulk-io.md` §2 is why: `examples/serve/` *had* the
    /// foreign call, and reaching it cost `Ffi("libc")` — which
    /// `reach.md` §5 establishes is every authority at once. So a
    /// program that wanted to print quickly had to ask for everything,
    /// and the incentive ran backwards.
    ///
    /// This is a second primitive behind the **same** capability
    /// instead. It changes what a grant of `Io` is worth without
    /// changing what it permits: the row is still `io_write`, and a
    /// faster program is not a more powerful one (§3.2).
    Write,
    /// `write_err[&i, &b](io: &!i Io, bytes: &b [byte]) -> [err_write] int`
    /// — the same slice, on the other stream (`docs/standard-error.md`).
    ///
    /// A third label under the *same* capability, which is
    /// `standard-input.md` §2's rule applied a third time: the capability
    /// is what you hold, the label is what you did with it. Standard error
    /// is part of the console — same process, same three descriptors, same
    /// shell redirecting them — so it is not an eighth field on `Split`.
    ///
    /// There is no per-byte twin. `putchar` exists beside `write_bytes`
    /// because it came first, not because two primitives were wanted, and
    /// a diagnostic is short: §3.2.
    WriteErr,
    /// `flush_out[&i](io: &!i Io) -> [io_write] Done` -- flush standard
    /// output and say whether everything written to it arrived
    /// (`docs/checked-output.md`, edition 5).
    ///
    /// `write_bytes` answers what the stdio buffer took; the last buffer's
    /// worth was written by libc at exit with the result ignored, so a
    /// program writing to a full disk could not know (`docs/bulk-io.md`
    /// §3.3, corrected). This is `fflush(stdout)` and then
    /// `ferror(stdout)`: the second because `fflush` answers 0 for an empty
    /// buffer even after an earlier write failed. Its label is `io_write`,
    /// the label of what it completes.
    FlushOut,
    /// `getchar[&i](io: &!i Io) -> [io_read] int` — libc's `getchar`, one
    /// byte in, behind the capability that authorises it.
    ///
    /// `docs/standard-input.md`. The mirror of [`Builtin::PutChar`] in
    /// every respect: the same capability, the other direction, its own
    /// effect label, and the `Io` erased on the way to libc because it
    /// carries no data.
    ///
    /// `-1` at end of input. A byte is 0..255, so the sentinel cannot be
    /// one — which is why C's `getchar` returns `int` too — and it matches
    /// `fs_read`'s `-1` for a file that could not be read. §3.1 says why
    /// an enum would be better and §6 keeps it open.
    GetChar,
    /// `split(w: World) -> [] Split` — consumes the root of all authority
    /// and hands back its parts (§8.2).
    ///
    /// The one place a capability comes from. There is no ambient
    /// constructor, no `Io::global()`, and nothing that conjures one.
    Split,
    /// `narrow(f: Ffi(a), "libc") -> [] Ffi("libc")` — attenuation (§7.4).
    ///
    /// Consumes the wider capability and hands back the narrower one, which
    /// is what makes it a trade rather than a copy: a program cannot keep
    /// the broad authority *and* the narrow one.
    ///
    /// Narrowing only, in both directions — the same commitment `lex-os`
    /// makes for manifests, for the same reason: a program must not be able
    /// to grant itself what it was not given. The literal argument is what
    /// makes the refinement checkable structurally, which is why §7.4
    /// requires one.
    Narrow,
    /// `fork_heap(h: &!x Heap) -> [heap] Heap` — a second owned `Heap`, made
    /// from a unique borrow of the first (`docs/parallelism.md` §8).
    ///
    /// Nothing is amplified: the caller already holds the authority and the
    /// label stays `heap`. The child is an ordinary `res` value, moved into a
    /// worker's struct or a spawn payload. Sound only while `Heap` has no
    /// state of its own (§8.3); it carries no leaves, so neither backend
    /// emits anything for the call. Edition 4, like `spawn`.
    ForkHeap,
    /// `copy_within(buf: &!r [byte], dst: int, src: int, n: int) -> int` — move `n` bytes inside one slice from `src` to
    /// `dst`, as `memmove` does: the ranges may overlap and the result is as if the bytes were copied out first.
    ///
    /// Bounds-checked like indexing: it traps unless `0 <= dst`, `0 <= src`, `0 <= n`, `dst + n <= len(buf)` and
    /// `src + n <= len(buf)` (the sums are not formed, so nothing can overflow). Answers 0. Pure and capability-free:
    /// it reads and writes only the slice it was given. Edition 5. `docs/memory-moves.md`.
    CopyWithin,
    /// `copy_into(dst: &!d [byte], src: &s [byte]) -> int` — copy all of `src` to the front of `dst`, as `memmove` does,
    /// and answer `len(src)`.
    ///
    /// Bounds-checked like indexing: it traps unless `len(src) <= len(dst)`. The two slices may be views of one buffer,
    /// so the copy is defined for overlap. Pure and capability-free: it touches only the slices it was given. Edition 5.
    /// `docs/bulk-copy.md`.
    CopyInto,
    /// `index_of_byte(text: &t [byte], b: byte) -> int` — where `b` first occurs in `text`, or -1 if it does not, as
    /// `memchr` answers. Pure and capability-free: it reads only the slice it was given. Edition 5.
    /// `docs/byte-search.md`.
    IndexOfByte,
    /// `fork_clock(c: &x Clock) -> Clock` — a second owned `Clock` from a
    /// shared borrow of the first (`docs/parallelism.md` §9).
    ///
    /// A thread that runs a server loop needs a clock of its own, and `split`
    /// hands out one. The capability reads the monotonic clock and nothing
    /// else, the parent already holds the authority, and the effect row has
    /// no label for it, so nothing is amplified. It does end the property
    /// that a capability has exactly one holder, for `Heap` and `Clock` only:
    /// `Net` deliberately has no fork. Edition 5, like `clock_ms`.
    ForkClock,
    /// `release(io: Io) -> [] int` — destroys a capability.
    ///
    /// Authority is a resource and a resource is destroyed exactly once, so
    /// a program that forgets this does not compile (§8.3). It is an
    /// ordinary consumer, in the sense §4.1 means.
    Release,
    /// `wrapping_add(a: int, b: int) -> [] int`, and its two siblings —
    /// two's-complement arithmetic that wraps instead of trapping.
    ///
    /// `+` is checked (`docs/defined-behaviour.md`), because a silently
    /// wrong answer is what the whole design refuses. But wraparound is the
    /// *intent* in a checksum, a hash or a counter, and a language that
    /// cannot express it forces the workaround to be worse than the thing.
    /// So it is spelled out: wrapping is what you asked for, not what you
    /// got away with.
    WrappingAdd,
    WrappingSub,
    WrappingMul,
    /// `value_barrier(x: int) -> [] int` -- `x`, which the optimiser may
    /// assume nothing about (`docs/value-barrier.md`, edition 6 only).
    ///
    /// Constant-time code selects under masks, and `clang -O2` can prove
    /// a mask built from a sign bit is 0 or -1 and turn the `and` back
    /// into a branch on the secret. A mask passed through this is just a
    /// number to it. On LLVM it is an empty `asm` tying output to input;
    /// on Cranelift, which makes no such branches, the identity.
    ValueBarrier,
    /// `byte_of(n: int) -> [] byte` — narrow an integer to a byte, or trap.
    ///
    /// `docs/strings.md` §2: it traps outside 0..255 rather than
    /// truncating, because truncation is the silently wrong answer
    /// `defined-behaviour.md` §2.1 already refused for `+`. A caller that
    /// wants the low eight bits says so, once there is a mask to say it
    /// with.
    ByteOf,
    /// `float_of(n: int) -> [] float` — the nearest `float` to an `int`
    /// (`docs/floating-point.md` §4).
    ///
    /// Rounds rather than trapping, which is the deliberate difference
    /// from `byte_of`: `byte_of(300)` loses the magnitude and hands back
    /// a different number, where this loses at most one unit in the last
    /// place, under IEEE's round-to-nearest-even. One is a lie about
    /// which number this is; the other is what a floating type *means*.
    FloatOf,
    /// `truncate(x: float) -> [] int` — toward zero, trapping on NaN,
    /// ±infinity and any magnitude at or past `2^63` (§4).
    ///
    /// Exactly the inputs C leaves undefined. The name states the
    /// rounding because the rounding is what a reader needs to know;
    /// `floor`, `ceil` and round-to-nearest belong in `std.math`, where
    /// each can say which it is.
    Truncate,
    /// `bits_of(x: float) -> [] int` — the IEEE-754 representation, read
    /// as an integer (`docs/float-printing.md` §2).
    ///
    /// A *reinterpretation*, not a conversion: the bits are unchanged and
    /// IEEE-754 says exactly what they mean. `float_of` and `truncate`
    /// are the conversions, and both are about values.
    ///
    /// It exists so numeric library code can be written **in the
    /// language** rather than in the compiler. Without it, decomposing a
    /// float into sign, exponent and mantissa is impossible, and every
    /// routine that needs to — printing, `copysign`, `frexp`, a total
    /// order — has to become a builtin. `std.fmt` is the first caller and
    /// is the argument: a correct shortest-round-trip printer, written in
    /// lex-sys, rather than a hole in the standard library.
    BitsOf,
    /// `f32_of(x: float) -> [] f32` — the nearest `f32` to a `float`,
    /// ties to even, an overflow giving infinity (`docs/f32.md` §2).
    ///
    /// A rounding, and named for being one: it is the one crossing from
    /// `float` to `f32`, written where a reader can see it. Never traps.
    F32Of,
    /// `float_of32(x: f32) -> [] float` — exact, every binary32 value is
    /// a binary64 value (`docs/f32.md` §2).
    ///
    /// Not `float_of`: that name is `int -> float` and a builtin has one
    /// signature, so the width is part of the name, as in `bits_of32`.
    FloatOf32,
    /// `bits_of32(x: f32) -> [] int` — the 32 bits of binary32, zero
    /// extended (`docs/f32.md` §2). A reinterpretation like `bits_of`,
    /// and like it every NaN answers one pattern, `0x7fc00000`, so the
    /// answer does not depend on which target generated the NaN.
    BitsOf32,
    /// `f32_of_bits(n: int) -> [] f32` — the `f32` whose bits are the low
    /// 32 of `n` (`docs/f32.md` §2). The inverse `float-printing.md` §8
    /// left for when something needs it; `lexsys-gpu` and the gate in
    /// `docs/f32.md` §5 both do.
    F32OfBits,
    /// `sqrt32(x: f32) -> [] f32` -- the correctly rounded square root, one
    /// instruction (`docs/f32.md` §2, `docs/float-math.md` §3). Named as
    /// `bits_of32` is: the width is part of the name because `sqrt` is
    /// `float -> float` and a builtin has one signature.
    Sqrt32,
    /// `f32_of_int(n: int) -> [] f32` -- the nearest `f32`, ties to even;
    /// every `int` is in range, so nothing traps (`docs/f32.md` §2).
    F32OfInt,
    /// `int_of_f32(x: f32) -> [] int` -- toward zero, trapping on NaN,
    /// infinity and any magnitude at or beyond `2^63`: `truncate`'s rule
    /// at the narrower width (`docs/floating-point.md` §4).
    IntOfF32,
    /// `is_nan(x: float) -> [] bool` (§5).
    ///
    /// Exists because NaN breaks comparison — `x == x` is false for it —
    /// so the hazard has to be checkable, and `x != x` is a riddle
    /// rather than a test.
    IsNan,
    /// `sqrt(x: float) -> [] float` — the square root, correctly rounded.
    ///
    /// A builtin rather than library code, which is the opposite of the
    /// call `float-printing.md` made for printing, and the reason is
    /// measured rather than assumed (`docs/float-math.md` §2): **a
    /// correctly-rounded square root cannot be written in lex-sys.** The
    /// two programs that hand-rolled one got 58.4% of values wrong in
    /// the last place and one of them was wrong by 143 orders of
    /// magnitude on a large input. IEEE-754 requires `sqrt` to be
    /// correctly rounded and the hardware instruction is, so the
    /// instruction is the only correct implementation available.
    ///
    /// Not a libc call, which is the whole of the capability question
    /// (§3): `sqrtsd` and `fsqrt` are one instruction each, so this
    /// reaches no library, needs no `Ffi`, and its row is `[]`.
    Sqrt,
    /// `int_of(b: byte) -> [] int` — widen a byte, which is always defined
    /// and always lands in 0..255.
    IntOf,
    /// `fs_read(fs, path, into) -> [fs_read(p)] int` — read a whole file.
    ///
    /// `docs/filesystem.md` §2: a builtin rather than an `extern fn`,
    /// because an `extern` would be gated by `Ffi("libc")` and then holding
    /// the *FFI* capability would open any path, with `Fs` contributing
    /// nothing. The authority that guards the filesystem has to be the one
    /// that names it, so the backend reaches libc itself, the way `putchar`
    /// and the arena already do.
    ///
    /// Returns the byte count, or `-1` if the file could not be read: a
    /// missing file is an ordinary outcome, not a broken promise. A path
    /// *outside* the capability's prefix is the broken promise, and traps.
    FsRead,
    /// `open_read[&c, &a](fs: &c Fs(p), path: &a [byte]) -> [fs_read(p)] Opened`
    /// — `docs/file-handles.md` §2.1.
    ///
    /// Checked at the call site like [`Builtin::FsRead`] and for the same
    /// reason: the prefix lives in the capability's type, and a fixed
    /// signature has no parameter to name it. The whole path check is paid
    /// here, once, which is §4's fourth answer — the handle it returns
    /// cannot be widened, so `read` performs a path-free label.
    OpenRead,
    /// `file_read[&f, &b](file: &!f File, into: &!b [byte]) -> [file_read] Read`
    /// — §3.
    ///
    /// Named the way `fs_read` is — the subject, then the verb — and
    /// sharing its name with the label it performs, exactly as `fs_read`
    /// does. The design doc wrote it `read`, and `read` turned out to be
    /// a name a program wants: `examples/serve/` declares `extern fn read`
    /// for libc's, on a socket rather than a file.
    ///
    /// Three outcomes and three constructors. A sentinel is how `getchar`
    /// and `fs_read` came to disagree about `-1`, so this is the API that
    /// does not repeat it.
    ReadFile,
    /// `file_close(file: File) -> [] int` — §2. Renamed from `close` for
    /// the reason above: 31 fixtures had a `close` of their own.
    ///
    /// Consumes the handle, which is what `res` means; the checker needed
    /// nothing new to enforce it. The `int` is the outcome of `close(2)`,
    /// which can fail even though nothing can be done about it.
    Close,
    /// `open_append`, `open_write`, `open_new` and `open_rw`
    /// (`docs/file-writes.md` section 3): `open_read`'s shape, one for each
    /// way a log opens a file. Checked at the call site for the reason
    /// `open_read` is: the prefix lives in the capability's type.
    OpenAppend,
    OpenWrite,
    OpenNew,
    OpenRw,
    /// `fs_rename(fs, from, to)` and `fs_remove(fs, path)`, both `[fs_write(p)] Done`
    /// (`docs/file-writes.md` section 7): checked at the call site, because
    /// the prefix is in the capability's type and every path is checked against it.
    FsRename,
    FsRemove,
    /// `file_lock(&!File) -> [file_write] Done`: `flock(LOCK_EX | LOCK_NB)`, an advisory
    /// lock the kernel releases when the process ends, however it ends.
    FileLock,
    /// `file_write(&!File, &[byte]) -> [file_write] Done`: one `write(2)`,
    /// which may take fewer bytes than asked (section 4.1).
    FileWrite,
    /// `file_pwrite(&!File, at, &[byte]) -> [file_write] Done`: `pwrite(2)`.
    /// On a handle opened for append it still appends (section 4.2).
    FilePwrite,
    /// `file_pread(&!File, at, &![byte]) -> [file_read] Read`: `pread(2)`.
    FilePread,
    /// `file_sync(&!File) -> [file_write] Done`: `fsync(2)` (section 5).
    FileSync,
    /// `file_truncate(&!File, len) -> [file_write] Done`: `ftruncate(2)`.
    FileTruncate,
    /// `file_size(&!File) -> [file_read] Done`: the length, with the cursor
    /// left where it was (section 4.3).
    FileSize,
    /// `fs_write(fs, path, bytes) -> [fs_write(p)] int` — write a whole file.
    FsWrite,
    /// `box(h, value) -> [heap] Box[T]` — one value, one allocation.
    ///
    /// `docs/heap.md` §3. A builtin rather than an `extern fn` for the same
    /// reason the file operations are (`filesystem.md` §2): an `extern`
    /// would be gated by `Ffi("libc")`, and then the FFI capability would
    /// allocate, with `Heap` contributing nothing.
    ///
    /// Checked at the call site, because the result's type is the
    /// argument's and a fixed signature has no parameter to bind it to.
    Box,
    /// `unbox(h, b: Box[T]) -> [heap] T` — free the allocation, yield the value.
    ///
    /// The only consumer a `Box` has. That is what makes §3.1's claim hold:
    /// a box is `res`, so a program that never unboxes one does not compile,
    /// and the general heap cannot leak.
    Unbox,
    /// `contents(b: &r Box[T]) -> [] &r T` — the dereference.
    ///
    /// Mode- and region-preserving: a shared borrow of a box yields a shared
    /// borrow of what it holds, for exactly as long. There is nothing to
    /// check at run time, because there is no way to hold a reference into a
    /// box that has been freed -- `unbox` consumes, and §5 already refuses a
    /// reference that outlives its borrow.
    Contents,
    /// `box_slice(h, count, fill) -> [heap] Box[[T]]` — a run of values on
    /// the heap (`docs/boxed-slices.md` §3).
    ///
    /// The second shape a box comes in: a pointer *and* a length, where an
    /// ordinary box is a pointer alone, because nothing else knows how many
    /// elements there are.
    BoxSlice,
    /// `unbox_slice(h, b) -> [heap] int` — free it, and answer how many.
    ///
    /// A different operation from `unbox`, and it has to be: `unbox` hands
    /// back what the box held, and `[T]` is unsized so there is nothing to
    /// hand back. It is still the *only* consumer a boxed slice has, so
    /// `heap.md` §3.1 holds unchanged.
    UnboxSlice,
    /// `arg_count(a) -> [args] int` — `argc`, exactly as the runtime gave it.
    ///
    /// `docs/arguments.md` §3. A builtin rather than an `extern fn` for the
    /// reason `filesystem.md` §2 gives: an `extern` would be gated by
    /// `Ffi("libc")`, and then the FFI capability would read the command
    /// line with `Args` contributing nothing.
    ArgCount,
    /// `arg(a, n) -> [args] &static [byte]` — one argument, as bytes.
    ///
    /// `arg(a, 0)` is the program name. The region is `static` because
    /// argv outlives every region in the program (§3.1), and the slice is
    /// *shared* because a program does not own its own command line.
    Arg,
    /// `len(s: &r [T]) -> [] int` — how many elements a slice has.
    ///
    /// Checked at the call site rather than through a written signature,
    /// because the element type is whatever the argument's is and a fixed
    /// signature cannot say that without a type parameter the builtin
    /// table has no way to bind.
    Len,
    /// `connect(net, name, port) -> [net_out(bound)] int` —
    /// `docs/net.md` §4.1, edition 2 only (`docs/editions.md` §7).
    ///
    /// The name is checked against the capability's bound, then resolved
    /// with `getaddrinfo` (`docs/connect.md` §10). A socket operation
    /// rather than an `extern fn`, for the reason `filesystem.md` §2 gives
    /// for `fs_read`: an `extern` would be gated by `Ffi("libc")` alone,
    /// and then the FFI capability would open any socket, with `Net`
    /// contributing nothing.
    ///
    /// Checked at the call site like [`Builtin::FsRead`], because the row
    /// it performs is the bound its `Net` capability was narrowed to, and
    /// a fixed signature has nowhere to put it.
    Connect,
    /// `bind(net, port) -> [net_in(bound)] int` — `docs/net.md` §2.1,
    /// edition 2 only (`docs/listen.md` §6).
    ///
    /// The inbound mirror of [`Builtin::Connect`]: folds `socket`,
    /// `setsockopt(SO_REUSEADDR)` and `bind` into one call, and checks
    /// `port` against the capability's bound -- here just a port, not a
    /// `host:port` pair, because `net.md` §2.1 bounds inbound by *which
    /// port* alone (`docs/listen.md` §6.1). Checked at the call site for
    /// the same reason `connect` is.
    Bind,
    /// `listen(fd, backlog) -> [] int` — `listen(2)`, unchanged.
    ///
    /// Takes no capability: the fd already proves the authority `bind`
    /// checked, the same way `read`/`close` need none once `open_read`
    /// has run (`docs/file-handles.md` §4.1). A fixed signature, unlike
    /// `bind` and `connect`, because nothing about it depends on a
    /// literal written at the call (`docs/listen.md` §6).
    Listen,
    /// `accept(fd) -> [] int` — `accept(2)`, the peer address ignored
    /// (`NULL, NULL`, as `examples/serve/`'s own hand-written call
    /// already does). Fixed, for the same reason `listen` is.
    Accept,
    /// `tcp_listen(net, port, backlog, flags) -> [net_in(bound)] Listening`
    /// -- `docs/native-sockets.md` §3, edition 5 only.
    ///
    /// `socket`, `SO_REUSEADDR`, `bind` and `listen` in one call, answering
    /// a `Listener` handle (or the `errno`) rather than a descriptor a
    /// program could forge. `flags` bit 1 is `SO_REUSEPORT`. Checked at the
    /// call site, like [`Builtin::Bind`], because the row it performs is
    /// the bound its `Net` was narrowed to.
    TcpListen,
    /// `tcp_connect(net, host, port) -> [net_out(bound)] Dialed` --
    /// `docs/native-sockets.md` §3, edition 5 only: `connect`, answering a
    /// `Conn` handle (or the reason) rather than a descriptor.
    TcpConnect,
    /// `tcp_connect_start(net, host, port) -> [net_out(bound)] Dialed` --
    /// `docs/native-sockets.md` §10.6, edition 5 only: `tcp_connect` that does
    /// not wait. The `Conn` is non-blocking and the connection may still be in
    /// progress: watch it for *writable* with a `Poller`, then ask
    /// `conn_connect_status`. A name is still resolved by a blocking call; an IP
    /// literal needs none.
    TcpConnectStart,
    /// `poller_new() -> [] Polling` -- `docs/native-sockets.md` §4: an
    /// `epoll` (Linux) or `kqueue` (Darwin) set, empty. It needs no
    /// capability: a set watching nothing observes nothing.
    PollerNew,
    /// `poller_add_listener(&!Poller, &Listener, token) -> [poll] int`:
    /// watch a listener for connections. `0`, or the `errno`.
    PollerAddListener,
    /// `poller_add_conn(&!Poller, &Conn, token, events) -> [poll] int`:
    /// `events` is 1 for readable, 2 for writable, 3 for both.
    PollerAddConn,
    /// `poller_modify(&!Poller, &Conn, token, events) -> [poll] int`.
    PollerModify,
    /// `poller_remove(&!Poller, &Conn) -> [poll] int`.
    PollerRemove,
    /// `poller_wait(&!Poller, &![int], timeout_ms) -> [poll] int`: writes
    /// `(token, events)` pairs into the slice (at most 64 at a time) and
    /// answers how many, or `-errno`. A negative timeout waits for ever.
    PollerWait,
    /// `poller_close(Poller) -> [] int`.
    PollerClose,
    /// `clock_ms(&Clock) -> [clock] int` -- `docs/native-sockets.md` §5,
    /// edition 5 only: monotonic milliseconds from an arbitrary origin, so
    /// an idle timeout survives the wall clock being set.
    ClockMs,
    /// `clock_unix_ms(&Clock) -> [clock] int` -- `docs/native-sockets.md`
    /// §10.5, edition 5 only: milliseconds since 1970-01-01 UTC, the
    /// wall clock. It can jump backwards or forwards when the host's clock
    /// is set, so it is for stamping messages and never for timeouts.
    ClockUnixMs,
    /// `conn_detach(Conn) -> [] int` -- `docs/native-sockets.md` §10.3:
    /// turns a connection into an inert **ticket** (an integer) so it can
    /// sit in a `Vec`, which holds only copyable things. The `Conn` is
    /// consumed; the descriptor stays open. `-1` if it could not be done,
    /// in which case the connection has been closed.
    ConnDetach,
    /// `conn_attach(int) -> [] Attached` -- redeems a ticket **once**. A
    /// ticket that was never issued, was already redeemed, or belongs to a
    /// descriptor since reused is `Failed(EBADF)`: the number is not
    /// authority, and forging one reaches nothing.
    ConnAttach,
    /// `tcp_accept(&!Listener) -> [conn_accept] Accepted`.
    TcpAccept,
    /// `conn_read(&!Conn, &![byte]) -> [conn_read] Received`.
    ConnRead,
    /// `conn_write(&!Conn, &[byte]) -> [conn_write] Sent`: never waits on a
    /// non-blocking connection, and never raises `SIGPIPE`.
    ConnWrite,
    /// `conn_nonblocking(&!Conn) -> [] int`: one way, explicit.
    ConnNonblocking,
    /// `conn_nodelay(&!Conn) -> [] int`: `TCP_NODELAY` on, one way, explicit. `0`, or
    /// the `errno`. A program that writes a message in more than one piece, or answers
    /// a request in several writes, waits on the other end's delayed acknowledgement
    /// (tens of milliseconds) without it (`docs/native-sockets.md` section 11).
    ConnNodelay,
    /// `conn_connect_status(&!Conn) -> [] int` -- `docs/native-sockets.md` §10.6:
    /// how a connection started with `tcp_connect_start` ended: `0` connected, or
    /// the `errno` (`SO_ERROR`). Meaningful only once a `Poller` has reported the
    /// connection writable (or hung up); before that it answers `0` whether or not
    /// the connection is made.
    ConnConnectStatus,
    /// `listener_nonblocking(&!Listener) -> [] int`.
    ListenerNonblocking,
    /// `conn_close(Conn) -> [] int`: consumes the handle.
    ConnClose,
    /// `listener_close(Listener) -> [] int`.
    ListenerClose,
    /// `signals_watch(&Signals("S")) -> [signals("S")] Watching` --
    /// `docs/signals.md` section 2, edition 6 only: claim the signals `S` the
    /// capability was narrowed to. Checked at the call site (`Expr::Call`
    /// with the set's bits as a second argument) because its row is the set
    /// in the capability's type, as `tcp_listen`'s is the bound's.
    SignalsWatch,
    /// `signals_pending(&!SignalWatch) -> [signals_read] int`: the bits of the
    /// claimed signals that arrived since the previous call, cleared. Never
    /// waits.
    SignalsPending,
    /// `poller_add_signals(&!Poller, &SignalWatch, token) -> [poll] int`:
    /// watch a claim for readability. `0`, or the `errno`.
    PollerAddSignals,
    /// `signals_close(SignalWatch) -> [] int`: ends the claim and puts the
    /// signals back to the default; consumes the handle.
    SignalsClose,
    /// `open_dir(&Fs(p), path) -> [fs_read(p)] DirOpened`: a directory handle,
    /// the anchor everything after it is opened beneath. Lowered as
    /// `Expr::OpenFile` with `OpenMode::Directory`, so the prefix check is
    /// `open_read`'s. Edition 6. `docs/directory-handles.md`.
    OpenDir,
    /// `dir_enter(&Dir, name) -> [dir_read] DirOpened`: one child directory,
    /// `openat(dir, name, O_DIRECTORY | O_NOFOLLOW)`. `name` is one component:
    /// empty, `.`, `..`, a `/` or a NUL is `Failed(EINVAL)` with no call.
    DirEnter,
    /// `dir_open_read(&Dir, name) -> [dir_read] Opened`: one child file for
    /// reading, `openat(dir, name, O_NOFOLLOW)`, with `dir_enter`'s check.
    DirOpenRead,
    /// `dir_close(Dir) -> [] int`: `close`'s answer; consumes the handle.
    DirClose,
    /// `dir_open_new(&Dir, name) -> [dir_write] Opened`: create one child file
    /// that must not exist, `openat(O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW,
    /// 0644)`. `dir_enter`'s check on `name`. `docs/directory-handles.md` §3.
    DirOpenNew,
    /// `dir_open_append(&Dir, name) -> [dir_write] Opened`: one child file
    /// for appending, created if missing, `openat(O_WRONLY | O_CREAT |
    /// O_APPEND | O_NOFOLLOW, 0644)`.
    DirOpenAppend,
    /// `dir_rename(&Dir, from, to) -> [dir_write] Done`: `renameat` with both
    /// names in the one directory; each name has `dir_enter`'s check.
    DirRename,
    /// `dir_remove(&Dir, name) -> [dir_write] Done`: `unlinkat(dir, name, 0)`.
    /// A link is removed, never followed.
    DirRemove,
    /// `dir_sync(&Dir) -> [dir_write] Done`: `fsync` on the directory itself,
    /// so a rename in it is durable.
    DirSync,
    /// `dir_list(&Dir) -> [dir_read] Listing`: a stream of the directory's
    /// names, on a descriptor of its own (`docs/directory-listing.md` §3.1).
    DirList,
    /// `dir_next(&!DirList, &![byte]) -> [dir_read] Listed`: the next name,
    /// copied into the buffer, with its kind; `.` and `..` never.
    DirNext,
    /// `dir_list_close(DirList) -> [] int`: `closedir`'s answer; consumes
    /// the listing.
    DirListClose,
    /// `dir_stat(&Dir, name) -> [dir_read] DirStat`: `fstatat` with
    /// `AT_SYMLINK_NOFOLLOW` on one checked component -- a link is reported,
    /// never followed (`docs/directory-listing.md` §3.2).
    DirStat,
    /// `pipe_open() -> [] Piped` -- `docs/processes.md` §3.2, edition 7: a
    /// channel's two ends, the parent's and the one a child is handed. An
    /// unnamed channel inside this process reaches nothing, so no capability.
    PipeOpen,
    /// `exec_spawn(&Exec(p), path, args, env, stdin, stdout, stderr) ->
    /// [exec(p)] Spawned`: start the program at `path` under `p`. Lowered as
    /// `Expr::ExecSpawn`, so the prefix travels with it, as `open_read`'s does.
    ExecSpawn,
    /// `child_wait(Child) -> [] Exited`: wait for the child to end and reap it;
    /// the only consumer of a `Child` (§4.7).
    ChildWait,
    /// `child_kill(&Child, signal) -> [child_signal] int`: one of
    /// `std.signals`' bits, or `KILL` (256). `0`, or the `errno`.
    ChildKill,
    /// `pipe_read(&!Pipe, &![byte]) -> [pipe_read] Received`.
    PipeRead,
    /// `pipe_write(&!Pipe, &[byte]) -> [pipe_write] Sent`: never raises `SIGPIPE`.
    PipeWrite,
    /// `pipe_nonblocking(&!Pipe) -> [] int`: one way, explicit.
    PipeNonblocking,
    /// `pipe_close(Pipe) -> [] int`: consumes the parent's end.
    PipeClose,
    /// `child_end_close(ChildEnd) -> [] int`: consumes a child's end that was
    /// never handed to a child.
    ChildEndClose,
    /// `poller_add_pipe(&!Poller, &Pipe, token, events) -> [poll] int` --
    /// `docs/processes.md` §4.8, edition 7: watch a channel's parent end as
    /// `poller_add_conn` watches a `Conn`. `0`, or the `errno`.
    PollerAddPipe,
    /// `poller_add_child(&!Poller, &Child, token) -> [poll] int` -- §4.8,
    /// edition 7: report the child's exit as readable. After `poller_wait`
    /// names the token, `child_wait` answers without blocking. `0`, or the
    /// `errno` -- `ENOSYS` where the kernel gave the child no `pidfd`, `EMFILE`
    /// where the program had no descriptor to spare for it.
    PollerAddChild,
    /// `null_ptr() -> [] c_ptr` — the one producer of a `c_ptr` that is
    /// not a foreign call's return, edition 3 only
    /// (`docs/opaque-pointers.md` §3).
    ///
    /// Needed because OpenSSL's own error convention is "returns `NULL`
    /// on failure" for `SSL_CTX_new`/`SSL_new`, and the no-coercion rule
    /// on `c_ptr` forbids building that comparison value out of an `int`
    /// literal. Fixed, like `sqrt`: nothing about it depends on the
    /// call site.
    NullPtr,
    /// `spawn(payload: T, body: fn(T) -> [row] R) -> [conc] res
    /// Thread[T, R]` — a real OS thread, edition 4 only
    /// (`docs/threads.md` §2).
    ///
    /// Checked at the call site, like [`Builtin::Len`]: `T` and `R` are
    /// read off `payload`'s and `body`'s own types, not fixed by a
    /// signature, and this first slice restricts both to exactly one
    /// pointer-width leaf (`int`, `bool`, `c_ptr`, or a reference) --
    /// what `pthread_create`'s own `void *(*)(void *)` start routine
    /// can carry without a compiler-synthesised trampoline function,
    /// which nothing in this IR can build yet. `body`'s own compiled
    /// entry point becomes the start routine directly.
    Spawn,
    /// `join(handle: res Thread[T, R]) -> [row] R` — blocks until the
    /// thread `spawn` started returns, edition 4 only.
    ///
    /// Checked at the call site: `R` is read off `handle`'s own type.
    /// The only operation that consumes a `Thread[T, R]`, the same
    /// "one consumer" shape `unbox`/`close` already have.
    Join,
    /// `trap() -> [] int` — end the process now, the same way every
    /// checked operation already does (`docs/defined-behaviour.md` §1).
    ///
    /// `docs/testing.md` §2 is why: a test framework needs a primitive
    /// a library can build `assert` out of, and this repository's own
    /// answer to "what happens when an assumption fails" has been a
    /// trap since M0 -- no message, no exception, `SIGILL` on both
    /// targets, exactly like an overflowing `+` or an out-of-range
    /// index. Both backends already carry the one instruction this
    /// needs (`trapnz`/`trap_if`); this is the first builtin that
    /// reaches it **unconditionally** rather than behind an
    /// arithmetic or bounds check the compiler emits on its own.
    /// Fixed, like `sqrt`: nothing about it depends on the call site,
    /// and it needs no capability, because deciding to trap is not an
    /// effect on the world.
    Trap,
}

impl Builtin {
    pub const ALL: &'static [Builtin] = &[
        Builtin::PutChar,
        Builtin::Write,
        Builtin::WriteErr,
        Builtin::FlushOut,
        Builtin::GetChar,
        Builtin::Split,
        Builtin::Release,
        Builtin::Narrow,
        Builtin::ForkHeap,
        Builtin::ForkClock,
        Builtin::CopyWithin,
        Builtin::CopyInto,
        Builtin::IndexOfByte,
        Builtin::WrappingAdd,
        Builtin::WrappingSub,
        Builtin::WrappingMul,
        Builtin::ValueBarrier,
        Builtin::Len,
        Builtin::ByteOf,
        Builtin::IntOf,
        Builtin::FloatOf,
        Builtin::Truncate,
        Builtin::IsNan,
        Builtin::Sqrt,
        Builtin::BitsOf,
        Builtin::F32Of,
        Builtin::FloatOf32,
        Builtin::BitsOf32,
        Builtin::F32OfBits,
        Builtin::Sqrt32,
        Builtin::F32OfInt,
        Builtin::IntOfF32,
        Builtin::FsRead,
        Builtin::FsWrite,
        Builtin::OpenRead,
        Builtin::ReadFile,
        Builtin::Close,
        Builtin::OpenAppend,
        Builtin::OpenWrite,
        Builtin::OpenNew,
        Builtin::OpenRw,
        Builtin::FileWrite,
        Builtin::FilePwrite,
        Builtin::FilePread,
        Builtin::FileSync,
        Builtin::FileTruncate,
        Builtin::FileSize,
        Builtin::FsRename,
        Builtin::FsRemove,
        Builtin::FileLock,
        Builtin::Box,
        Builtin::Unbox,
        Builtin::Contents,
        Builtin::ArgCount,
        Builtin::Arg,
        Builtin::BoxSlice,
        Builtin::UnboxSlice,
        Builtin::Connect,
        Builtin::Bind,
        Builtin::Listen,
        Builtin::Accept,
        Builtin::TcpListen,
        Builtin::TcpConnect,
        Builtin::TcpConnectStart,
        Builtin::TcpAccept,
        Builtin::PollerNew,
        Builtin::PollerAddListener,
        Builtin::PollerAddConn,
        Builtin::PollerModify,
        Builtin::PollerRemove,
        Builtin::PollerWait,
        Builtin::PollerClose,
        Builtin::ClockMs,
        Builtin::ClockUnixMs,
        Builtin::ConnDetach,
        Builtin::ConnAttach,
        Builtin::ConnRead,
        Builtin::ConnWrite,
        Builtin::ConnNonblocking,
        Builtin::ConnNodelay,
        Builtin::ConnConnectStatus,
        Builtin::ListenerNonblocking,
        Builtin::ConnClose,
        Builtin::ListenerClose,
        Builtin::SignalsWatch,
        Builtin::SignalsPending,
        Builtin::PollerAddSignals,
        Builtin::SignalsClose,
        Builtin::OpenDir,
        Builtin::DirEnter,
        Builtin::DirOpenRead,
        Builtin::DirClose,
        Builtin::DirOpenNew,
        Builtin::DirOpenAppend,
        Builtin::DirRename,
        Builtin::DirRemove,
        Builtin::DirSync,
        Builtin::DirList,
        Builtin::DirNext,
        Builtin::DirListClose,
        Builtin::DirStat,
        Builtin::PipeOpen,
        Builtin::ExecSpawn,
        Builtin::ChildWait,
        Builtin::ChildKill,
        Builtin::PipeRead,
        Builtin::PipeWrite,
        Builtin::PipeNonblocking,
        Builtin::PipeClose,
        Builtin::ChildEndClose,
        Builtin::PollerAddPipe,
        Builtin::PollerAddChild,
        Builtin::NullPtr,
        Builtin::Spawn,
        Builtin::Join,
        Builtin::Trap,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::PutChar => "putchar",
            Builtin::Write => "write_bytes",
            Builtin::FlushOut => "flush_out",
            Builtin::WriteErr => "write_err",
            Builtin::GetChar => "getchar",
            Builtin::Split => "split",
            Builtin::Release => "release",
            Builtin::Narrow => "narrow",
            Builtin::ForkHeap => "fork_heap",
            Builtin::ForkClock => "fork_clock",
            Builtin::CopyWithin => "copy_within",
            Builtin::CopyInto => "copy_into",
            Builtin::IndexOfByte => "index_of_byte",
            Builtin::WrappingAdd => "wrapping_add",
            Builtin::WrappingSub => "wrapping_sub",
            Builtin::WrappingMul => "wrapping_mul",
            Builtin::ValueBarrier => "value_barrier",
            Builtin::Len => "len",
            Builtin::ByteOf => "byte_of",
            Builtin::IntOf => "int_of",
            Builtin::FloatOf => "float_of",
            Builtin::Truncate => "truncate",
            Builtin::IsNan => "is_nan",
            Builtin::Sqrt => "sqrt",
            Builtin::BitsOf => "bits_of",
            Builtin::F32Of => "f32_of",
            Builtin::FloatOf32 => "float_of32",
            Builtin::BitsOf32 => "bits_of32",
            Builtin::F32OfBits => "f32_of_bits",
            Builtin::Sqrt32 => "sqrt32",
            Builtin::F32OfInt => "f32_of_int",
            Builtin::IntOfF32 => "int_of_f32",
            Builtin::FsRead => "fs_read",
            Builtin::OpenRead => "open_read",
            Builtin::ReadFile => "file_read",
            Builtin::Close => "file_close",
            Builtin::OpenAppend => "open_append",
            Builtin::OpenWrite => "open_write",
            Builtin::OpenNew => "open_new",
            Builtin::OpenRw => "open_rw",
            Builtin::FileWrite => "file_write",
            Builtin::FilePwrite => "file_pwrite",
            Builtin::FilePread => "file_pread",
            Builtin::FileSync => "file_sync",
            Builtin::FileTruncate => "file_truncate",
            Builtin::FileSize => "file_size",
            Builtin::FsRename => "fs_rename",
            Builtin::FsRemove => "fs_remove",
            Builtin::FileLock => "file_lock",
            Builtin::FsWrite => "fs_write",
            Builtin::Box => "box",
            Builtin::Unbox => "unbox",
            Builtin::Contents => "contents",
            Builtin::ArgCount => "arg_count",
            Builtin::Arg => "arg",
            Builtin::BoxSlice => "box_slice",
            Builtin::UnboxSlice => "unbox_slice",
            Builtin::Connect => "connect",
            Builtin::Bind => "bind",
            Builtin::Listen => "listen",
            Builtin::Accept => "accept",
            Builtin::TcpListen => "tcp_listen",
            Builtin::TcpConnect => "tcp_connect",
            Builtin::TcpConnectStart => "tcp_connect_start",
            Builtin::TcpAccept => "tcp_accept",
            Builtin::PollerNew => "poller_new",
            Builtin::PollerAddListener => "poller_add_listener",
            Builtin::PollerAddConn => "poller_add_conn",
            Builtin::PollerModify => "poller_modify",
            Builtin::PollerRemove => "poller_remove",
            Builtin::PollerWait => "poller_wait",
            Builtin::PollerClose => "poller_close",
            Builtin::ClockMs => "clock_ms",
            Builtin::ClockUnixMs => "clock_unix_ms",
            Builtin::ConnDetach => "conn_detach",
            Builtin::ConnAttach => "conn_attach",
            Builtin::ConnRead => "conn_read",
            Builtin::ConnWrite => "conn_write",
            Builtin::ConnNonblocking => "conn_nonblocking",
            Builtin::ConnNodelay => "conn_nodelay",
            Builtin::ConnConnectStatus => "conn_connect_status",
            Builtin::ListenerNonblocking => "listener_nonblocking",
            Builtin::ConnClose => "conn_close",
            Builtin::ListenerClose => "listener_close",
            Builtin::SignalsWatch => "signals_watch",
            Builtin::SignalsPending => "signals_pending",
            Builtin::PollerAddSignals => "poller_add_signals",
            Builtin::SignalsClose => "signals_close",
            Builtin::OpenDir => "open_dir",
            Builtin::DirEnter => "dir_enter",
            Builtin::DirOpenRead => "dir_open_read",
            Builtin::DirClose => "dir_close",
            Builtin::DirOpenNew => "dir_open_new",
            Builtin::DirOpenAppend => "dir_open_append",
            Builtin::DirRename => "dir_rename",
            Builtin::DirRemove => "dir_remove",
            Builtin::DirSync => "dir_sync",
            Builtin::DirList => "dir_list",
            Builtin::DirNext => "dir_next",
            Builtin::DirListClose => "dir_list_close",
            Builtin::DirStat => "dir_stat",
            Builtin::PipeOpen => "pipe_open",
            Builtin::ExecSpawn => "exec_spawn",
            Builtin::ChildWait => "child_wait",
            Builtin::ChildKill => "child_kill",
            Builtin::PipeRead => "pipe_read",
            Builtin::PipeWrite => "pipe_write",
            Builtin::PipeNonblocking => "pipe_nonblocking",
            Builtin::PipeClose => "pipe_close",
            Builtin::ChildEndClose => "child_end_close",
            Builtin::PollerAddPipe => "poller_add_pipe",
            Builtin::PollerAddChild => "poller_add_child",
            Builtin::NullPtr => "null_ptr",
            Builtin::Spawn => "spawn",
            Builtin::Join => "join",
            Builtin::Trap => "trap",
        }
    }

    /// The edition a file must be at to name this builtin
    /// (`docs/editions.md` §7). `1` for every builtin that predates
    /// editions; `Net`'s are the first to answer `2`.
    ///
    /// This is what keeps `connect` from shadowing the `extern fn connect`
    /// an edition-1 file may already declare against libc, the way
    /// `examples/fetch/` does today: name resolution only answers this
    /// builtin when the calling file's edition is at least this one, so to
    /// an earlier file the name is not a builtin at all.
    pub fn since(self) -> u32 {
        match self {
            Builtin::Connect | Builtin::Bind | Builtin::Listen | Builtin::Accept => 2,
            // `docs/opaque-pointers.md` §4: purely additive, the same
            // reason `Net`'s builtins needed edition 2 rather than
            // silently widening edition 1 -- an edition-1 file may
            // already declare its own `extern fn null_ptr`.
            Builtin::NullPtr => 3,
            // `docs/threads.md` §4: purely additive, same reasoning.
            Builtin::Spawn | Builtin::Join | Builtin::ForkHeap => 4,
            // `docs/native-sockets.md` §3: edition 5, and for the same
            // reason -- `conn_read` is a name an edition-1 file may
            // already declare against libc.
            Builtin::TcpListen
            | Builtin::TcpConnect
            | Builtin::TcpConnectStart
            | Builtin::TcpAccept
            | Builtin::PollerNew
            | Builtin::PollerAddListener
            | Builtin::PollerAddConn
            | Builtin::PollerModify
            | Builtin::PollerRemove
            | Builtin::PollerWait
            | Builtin::PollerClose
            | Builtin::ClockMs
            | Builtin::ClockUnixMs
            | Builtin::ConnDetach
            | Builtin::ConnAttach
            | Builtin::ConnRead
            | Builtin::ConnWrite
            | Builtin::ConnNonblocking
            | Builtin::ConnNodelay
            | Builtin::ConnConnectStatus
            | Builtin::ListenerNonblocking
            | Builtin::ConnClose
            | Builtin::ForkClock
            | Builtin::CopyWithin
            | Builtin::CopyInto
            | Builtin::IndexOfByte
            | Builtin::ListenerClose => 5,
            // `docs/checked-output.md`: a name a program may already have
            // declared for itself, so it is visible from edition 5 only.
            Builtin::FlushOut => 5,
            // `docs/value-barrier.md` §3: edition 6, the latest, for the
            // same reason -- a program may already declare the name.
            Builtin::ValueBarrier => 6,
            // `docs/f32.md` §6: edition 6, the latest, like `value_barrier`
            // -- a program may already declare `f32_of`. None in this
            // repository does (counted there), so no edition 7 is made.
            Builtin::F32Of
            | Builtin::FloatOf32
            | Builtin::BitsOf32
            | Builtin::F32OfBits
            | Builtin::Sqrt32
            | Builtin::F32OfInt
            | Builtin::IntOfF32 => 6,
            // `docs/signals.md`: edition 6, for the same reason --
            // `signals_watch` is a name a program may already declare.
            Builtin::SignalsWatch
            | Builtin::SignalsPending
            | Builtin::PollerAddSignals
            | Builtin::SignalsClose => 6,
            // `docs/directory-handles.md`: edition 6 -- `open_dir` and the
            // `dir_*` names are ones a program may already declare.
            Builtin::OpenDir
            | Builtin::DirEnter
            | Builtin::DirOpenRead
            | Builtin::DirClose
            | Builtin::DirOpenNew
            | Builtin::DirOpenAppend
            | Builtin::DirRename
            | Builtin::DirRemove
            | Builtin::DirSync
            | Builtin::DirList
            | Builtin::DirNext
            | Builtin::DirListClose
            | Builtin::DirStat => 6,
            // `docs/processes.md`: edition 7 -- `pipe_open` and `child_wait`
            // are names a program may already declare.
            Builtin::PipeOpen
            | Builtin::ExecSpawn
            | Builtin::ChildWait
            | Builtin::ChildKill
            | Builtin::PipeRead
            | Builtin::PipeWrite
            | Builtin::PipeNonblocking
            | Builtin::PipeClose
            | Builtin::ChildEndClose
            | Builtin::PollerAddPipe
            | Builtin::PollerAddChild => 7,
            // `docs/file-writes.md`: edition 5, for the same reason --
            // `file_write` and `open_new` are names a program may already
            // declare against libc.
            Builtin::OpenAppend
            | Builtin::OpenWrite
            | Builtin::OpenNew
            | Builtin::OpenRw
            | Builtin::FileWrite
            | Builtin::FilePwrite
            | Builtin::FilePread
            | Builtin::FileSync
            | Builtin::FileTruncate
            | Builtin::FileSize
            | Builtin::FsRename
            | Builtin::FsRemove
            | Builtin::FileLock => 5,
            _ => 1,
        }
    }

    /// The libc symbol the backend calls, for the ones that reach libc.
    ///
    /// `split` and `release` reach nothing: they are the ceremony that moves
    /// authority around, and authority erases (§8.1). The backend emits no
    /// call for them at all.
    pub fn symbol(self) -> Option<&'static str> {
        match self {
            Builtin::PutChar => Some("putchar"),
            // `fwrite`, not POSIX `write`: `putchar` goes through stdio,
            // and a bulk write on a raw descriptor would interleave
            // wrongly with it. The stream has to be the same one.
            Builtin::Write => Some("fwrite"),
            // The same call on the other stream. C guarantees `stderr`
            // is not fully buffered, which is what makes a diagnostic
            // written just before a trap arrive at all
            // (`docs/standard-error.md` §1.2).
            Builtin::WriteErr => Some("fwrite"),
            Builtin::GetChar => Some("getchar"),
            _ => None,
        }
    }

    /// How many leading arguments carry authority rather than data, and so
    /// do not reach the foreign function underneath.
    ///
    /// `putchar`'s `Io` is a *borrowed* capability, which is one pointer at
    /// a zero-sized value — real enough for the checker to track and
    /// meaningless to libc, which wants the character and nothing else.
    /// Passing it along made libc print the pointer.
    pub fn erased_args(self) -> usize {
        match self {
            // Each of these takes a borrowed capability first. It is
            // leaf-free, so it contributes no values either way, and
            // skipping it keeps the argument positions honest.
            Builtin::PutChar | Builtin::GetChar | Builtin::ArgCount | Builtin::Arg => 1,
            Builtin::Write | Builtin::WriteErr | Builtin::FlushOut => 1,
            _ => 0,
        }
    }

    /// How many region parameters the builtin takes, so a call site can
    /// instantiate them the same way it does for a written function (§5.1).
    ///
    /// A builtin that forgets to count one here keeps `Region::Param(0)`
    /// *rigid*, and then no caller's block region can ever unify with it —
    /// the call works from inside a region-polymorphic function and fails
    /// from inside a `borrow` block, which is a confusing way to find out.
    pub fn regions(self) -> usize {
        match self {
            Builtin::PutChar
            | Builtin::GetChar
            | Builtin::FlushOut
            | Builtin::ArgCount
            | Builtin::Arg => 1,
            // Two: the borrowed `Io` and the slice's own region.
            Builtin::Write | Builtin::WriteErr => 2,
            // Two: the borrowed handle and the buffer's own region.
            Builtin::ReadFile => 2,
            // The handle's region and the buffer's.
            Builtin::FileWrite | Builtin::FilePwrite | Builtin::FilePread => 2,
            // The handle's region alone.
            Builtin::FileSync | Builtin::FileTruncate | Builtin::FileSize | Builtin::FileLock => 1,
            // The handle's region, and for `conn_read`/`conn_write` the
            // buffer's own.
            Builtin::ConnRead | Builtin::ConnWrite => 2,
            // The poller's region and the handle's (or the buffer's).
            Builtin::PollerAddListener
            | Builtin::PollerAddConn
            | Builtin::PollerModify
            | Builtin::PollerRemove
            | Builtin::PollerWait
            | Builtin::PollerAddSignals => 2,
            Builtin::SignalsPending => 1,
            // The handle's region and the name's.
            Builtin::DirEnter
            | Builtin::DirOpenRead
            | Builtin::DirOpenNew
            | Builtin::DirOpenAppend => 2,
            Builtin::DirRemove => 2,
            // The handle's region and each name's.
            Builtin::DirRename => 3,
            Builtin::DirSync => 1,
            // The handle's region; for `dir_next`, the listing's and the
            // buffer's.
            Builtin::DirList => 1,
            Builtin::DirNext => 2,
            // The handle's region and the name's.
            Builtin::DirStat => 2,
            // The handle's region, and for a read or a write the buffer's.
            Builtin::PipeRead | Builtin::PipeWrite => 2,
            Builtin::ChildKill | Builtin::PipeNonblocking => 1,
            // The poller's region and the handle's.
            Builtin::PollerAddPipe | Builtin::PollerAddChild => 2,
            Builtin::TcpAccept
            | Builtin::ConnNonblocking
            | Builtin::ConnNodelay
            | Builtin::ConnConnectStatus
            | Builtin::ListenerNonblocking
            | Builtin::ForkClock
            | Builtin::CopyWithin
            | Builtin::IndexOfByte
            | Builtin::ClockMs
            | Builtin::ClockUnixMs => 1,
            // The destination's region and the source's.
            Builtin::CopyInto => 2,
            _ => 0,
        }
    }

    /// Parameter types and return type, in terms of the prelude's ids.
    ///
    /// `prelude` is `[World, Io, Split]` — the ids `collect_types` handed
    /// out, which are fixed because the prelude is collected first.
    pub fn signature(self, prelude: &[DefId]) -> (Vec<Type>, Type) {
        let named = |i: usize| Type::Named(prelude[i], Vec::new());
        match self {
            Builtin::PutChar => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_IO)),
                    },
                    Type::Int,
                ],
                Type::Int,
            ),
            // The same borrowed `Io`, and a shared slice of the bytes.
            // Shared rather than unique because writing reads them, and
            // `strings.md` §4's coercion lets a caller hand over a
            // unique one anyway.
            //
            // `write_err` is the same signature on the other stream, so
            // it shares this arm rather than repeating it: a difference
            // between them here would be a difference nothing asked for
            // (`docs/standard-error.md` §3).
            Builtin::Write | Builtin::WriteErr => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_IO)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                Type::Int,
            ),
            // The borrowed `Io` and nothing else; the answer is the write
            // side's `Done`, so the errno survives (`docs/checked-output.md`).
            Builtin::FlushOut => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_IO)),
                }],
                named(PRELUDE_DONE),
            ),
            // The mirror: the same borrowed `Io`, no character to take.
            Builtin::GetChar => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_IO)),
                }],
                Type::Int,
            ),
            // Checked at the call site (`docs/editions.md` §7): the return
            // type depends on the caller's edition, and a fixed signature
            // cannot say that.
            Builtin::Split => (Vec::new(), Type::Unit),
            Builtin::WrappingAdd | Builtin::WrappingSub | Builtin::WrappingMul => {
                (vec![Type::Int, Type::Int], Type::Int)
            }
            Builtin::ValueBarrier => (vec![Type::Int], Type::Int),
            Builtin::Len => (Vec::new(), Type::Int),
            // Both are checked at the call site: the prefix in the
            // capability's type is what decides the row, and a fixed
            // signature cannot say that.
            Builtin::FsRead | Builtin::FsWrite => (Vec::new(), Type::Unit),
            // Checked at the call site, exactly as `fs_read` is: the prefix
            // is in the capability's type (`docs/file-handles.md` §2.1).
            Builtin::OpenRead
            | Builtin::OpenAppend
            | Builtin::OpenWrite
            | Builtin::OpenNew
            | Builtin::OpenRw
            | Builtin::FsRename
            | Builtin::FsRemove
            | Builtin::OpenDir => (Vec::new(), Type::Unit),
            // The handle is borrowed uniquely because the read moves the
            // descriptor's offset, and the buffer uniquely because the read
            // writes into it -- the same pair `fs_read` takes, with the
            // capability replaced by the handle it was spent on.
            Builtin::ReadFile => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_FILE)),
                    },
                    Type::Ref {
                        unique: true,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_READ),
            ),
            // `docs/file-writes.md` section 4. The handle is borrowed
            // uniquely (a write moves the cursor and a sync is an
            // operation on it), the source buffer shared and the
            // destination buffer unique, the pair `conn_write` and
            // `file_read` take.
            Builtin::FileWrite => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_FILE)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_DONE),
            ),
            Builtin::FilePwrite => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_FILE)),
                    },
                    Type::Int,
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_DONE),
            ),
            Builtin::FilePread => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_FILE)),
                    },
                    Type::Int,
                    Type::Ref {
                        unique: true,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_READ),
            ),
            Builtin::FileSync | Builtin::FileSize | Builtin::FileLock => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_FILE)),
                }],
                named(PRELUDE_DONE),
            ),
            Builtin::FileTruncate => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_FILE)),
                    },
                    Type::Int,
                ],
                named(PRELUDE_DONE),
            ),
            // By value: `close` ends the handle, which is what `res` means.
            Builtin::Close => (vec![named(PRELUDE_FILE)], Type::Int),
            // All three depend on the type being boxed, which a fixed
            // signature has no parameter to name (`docs/heap.md` §3).
            Builtin::Box
            | Builtin::Unbox
            | Builtin::Contents
            | Builtin::BoxSlice
            | Builtin::UnboxSlice => (Vec::new(), Type::Unit),
            // `docs/arguments.md` §3. Written out rather than checked at
            // the call site, because neither depends on a type the caller
            // chose: an argument is always `&static [byte]`.
            Builtin::ArgCount => (
                vec![Type::Ref {
                    unique: false,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_ARGS)),
                }],
                Type::Int,
            ),
            Builtin::Arg => (
                vec![
                    Type::Ref {
                        unique: false,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_ARGS)),
                    },
                    Type::Int,
                ],
                Type::Ref {
                    unique: false,
                    region: Region::Static,
                    inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                },
            ),
            Builtin::ByteOf => (vec![Type::Int], Type::Byte),
            Builtin::IntOf => (vec![Type::Byte], Type::Int),
            Builtin::FloatOf => (vec![Type::Int], Type::Float),
            Builtin::Truncate => (vec![Type::Float], Type::Int),
            Builtin::IsNan => (vec![Type::Float], Type::Bool),
            // Float in, float out, and nothing else: no capability, because
            // it reaches no library (`docs/float-math.md` §3).
            Builtin::Sqrt => (vec![Type::Float], Type::Float),
            Builtin::BitsOf => (vec![Type::Float], Type::Int),
            Builtin::F32Of => (vec![Type::Float], Type::F32),
            Builtin::FloatOf32 => (vec![Type::F32], Type::Float),
            Builtin::BitsOf32 => (vec![Type::F32], Type::Int),
            Builtin::F32OfBits => (vec![Type::Int], Type::F32),
            Builtin::Sqrt32 => (vec![Type::F32], Type::F32),
            Builtin::F32OfInt => (vec![Type::Int], Type::F32),
            Builtin::IntOfF32 => (vec![Type::F32], Type::Int),
            // Both are checked at the call site rather than here, because a
            // fixed signature cannot say what they need. `release` ends any
            // capability, and there is more than one kind; `narrow` has an
            // argument *and* a result that depend on the literal written at
            // the call.
            Builtin::Release | Builtin::Narrow => (Vec::new(), Type::Unit),
            // Checked at the call site, like `box`: the argument must be a
            // uniquely borrowed `Heap`.
            Builtin::ForkHeap => (Vec::new(), Type::Unit),
            // Checked at the call site, exactly as `fs_read` is: the bound
            // is in the capability's type, and a fixed signature cannot
            // say that (`docs/net.md` §4.1).
            Builtin::Connect => (Vec::new(), Type::Unit),
            // Same reason, for the inbound half's bound (`docs/listen.md`
            // §6.1).
            Builtin::Bind => (Vec::new(), Type::Unit),
            // Neither takes a capability -- the fd already proves the
            // authority `bind` checked -- so both are ordinary fixed
            // signatures (`docs/listen.md` §6).
            Builtin::Listen => (vec![Type::Int, Type::Int], Type::Int),
            Builtin::Accept => (vec![Type::Int], Type::Int),
            // Checked at the call site, like `bind`: the port is spent
            // against the bound in the capability's type.
            Builtin::TcpListen | Builtin::TcpConnect | Builtin::TcpConnectStart => {
                (Vec::new(), Type::Unit)
            }
            Builtin::TcpAccept => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_LISTENER)),
                }],
                named(PRELUDE_ACCEPTED),
            ),
            // The handle is borrowed uniquely for a read (it moves the
            // stream) and the buffer is written into.
            Builtin::ConnRead => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_CONN)),
                    },
                    Type::Ref {
                        unique: true,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_RECEIVED),
            ),
            // The handle is unique -- a write moves the stream -- and the
            // buffer is only read, so a program can send from the same
            // bytes it is parsing.
            Builtin::ConnWrite => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_CONN)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_SENT),
            ),
            Builtin::PollerNew => (Vec::new(), named(PRELUDE_POLLING)),
            Builtin::PollerAddListener => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_POLLER)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(named(PRELUDE_LISTENER)),
                    },
                    Type::Int,
                ],
                Type::Int,
            ),
            Builtin::PollerAddConn | Builtin::PollerModify => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_POLLER)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(named(PRELUDE_CONN)),
                    },
                    Type::Int,
                    Type::Int,
                ],
                Type::Int,
            ),
            Builtin::PollerRemove => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_POLLER)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(named(PRELUDE_CONN)),
                    },
                ],
                Type::Int,
            ),
            // The events land in a slice of ints: `(token, events)` pairs.
            Builtin::PollerWait => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_POLLER)),
                    },
                    Type::Ref {
                        unique: true,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Int))),
                    },
                    Type::Int,
                ],
                Type::Int,
            ),
            Builtin::PollerClose => (vec![named(PRELUDE_POLLER)], Type::Int),
            // Checked at the call site: the set is in the capability's type.
            Builtin::SignalsWatch => (Vec::new(), Type::Unit),
            Builtin::SignalsPending => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_SIGNAL_WATCH)),
                }],
                Type::Int,
            ),
            Builtin::PollerAddSignals => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_POLLER)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(named(PRELUDE_SIGNAL_WATCH)),
                    },
                    Type::Int,
                ],
                Type::Int,
            ),
            Builtin::SignalsClose => (vec![named(PRELUDE_SIGNAL_WATCH)], Type::Int),
            Builtin::DirEnter | Builtin::DirOpenRead => (
                vec![
                    Type::Ref {
                        unique: false,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_DIR)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                if self == Builtin::DirEnter {
                    named(PRELUDE_DIR_OPENED)
                } else {
                    named(PRELUDE_OPENED)
                },
            ),
            Builtin::DirClose => (vec![named(PRELUDE_DIR)], Type::Int),
            Builtin::DirOpenNew | Builtin::DirOpenAppend | Builtin::DirRemove => (
                vec![
                    Type::Ref {
                        unique: false,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_DIR)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                if self == Builtin::DirRemove {
                    named(PRELUDE_DONE)
                } else {
                    named(PRELUDE_OPENED)
                },
            ),
            Builtin::DirRename => (
                vec![
                    Type::Ref {
                        unique: false,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_DIR)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(2),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_DONE),
            ),
            Builtin::DirSync => (
                vec![Type::Ref {
                    unique: false,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_DIR)),
                }],
                named(PRELUDE_DONE),
            ),
            // `docs/directory-listing.md` §3.1. The directory is shared, as
            // every step beneath it is; the listing is unique, because a
            // step moves it, and so is the buffer the name is copied into.
            Builtin::DirList => (
                vec![Type::Ref {
                    unique: false,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_DIR)),
                }],
                named(PRELUDE_LISTING),
            ),
            Builtin::DirNext => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_DIR_LIST)),
                    },
                    Type::Ref {
                        unique: true,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_LISTED),
            ),
            Builtin::DirListClose => (vec![named(PRELUDE_DIR_LIST)], Type::Int),
            // §3.2: `dir_enter`'s shape, answering a status.
            Builtin::DirStat => (
                vec![
                    Type::Ref {
                        unique: false,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_DIR)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(PRELUDE_DIR_STAT),
            ),
            Builtin::ConnDetach => (vec![named(PRELUDE_CONN)], Type::Int),
            Builtin::ConnAttach => (vec![Type::Int], named(PRELUDE_ATTACHED)),
            Builtin::ClockMs | Builtin::ClockUnixMs => (
                vec![Type::Ref {
                    unique: false,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_CLOCK)),
                }],
                Type::Int,
            ),
            Builtin::CopyWithin => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                    Type::Int,
                    Type::Int,
                    Type::Int,
                ],
                Type::Int,
            ),
            Builtin::CopyInto => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                Type::Int,
            ),
            Builtin::IndexOfByte => (
                vec![
                    Type::Ref {
                        unique: false,
                        region: Region::Param(0),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                    Type::Byte,
                ],
                Type::Int,
            ),
            Builtin::ForkClock => (
                vec![Type::Ref {
                    unique: false,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_CLOCK)),
                }],
                named(PRELUDE_CLOCK),
            ),
            Builtin::ConnNonblocking | Builtin::ConnNodelay | Builtin::ConnConnectStatus => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_CONN)),
                }],
                Type::Int,
            ),
            Builtin::ListenerNonblocking => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_LISTENER)),
                }],
                Type::Int,
            ),
            // By value: `close` ends the handle.
            Builtin::ConnClose => (vec![named(PRELUDE_CONN)], Type::Int),
            // `docs/processes.md` §3.2.
            Builtin::PipeOpen => (Vec::new(), named(PRELUDE_PIPED)),
            // Checked at the call site: the prefix is in the capability's type.
            Builtin::ExecSpawn => (Vec::new(), Type::Unit),
            // By value: waiting ends the child.
            Builtin::ChildWait => (vec![named(PRELUDE_CHILD)], named(PRELUDE_EXITED)),
            Builtin::ChildKill => (
                vec![
                    Type::Ref {
                        unique: false,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_CHILD)),
                    },
                    Type::Int,
                ],
                Type::Int,
            ),
            Builtin::PipeRead | Builtin::PipeWrite => (
                vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_PIPE)),
                    },
                    Type::Ref {
                        unique: self == Builtin::PipeRead,
                        region: Region::Param(1),
                        inner: Box::new(Type::Slice(Box::new(Type::Byte))),
                    },
                ],
                named(if self == Builtin::PipeRead { PRELUDE_RECEIVED } else { PRELUDE_SENT }),
            ),
            Builtin::PipeNonblocking => (
                vec![Type::Ref {
                    unique: true,
                    region: Region::Param(0),
                    inner: Box::new(named(PRELUDE_PIPE)),
                }],
                Type::Int,
            ),
            Builtin::PipeClose => (vec![named(PRELUDE_PIPE)], Type::Int),
            Builtin::ChildEndClose => (vec![named(PRELUDE_CHILD_END)], Type::Int),
            Builtin::PollerAddPipe | Builtin::PollerAddChild => {
                let handle =
                    if self == Builtin::PollerAddPipe { PRELUDE_PIPE } else { PRELUDE_CHILD };
                let mut params = vec![
                    Type::Ref {
                        unique: true,
                        region: Region::Param(0),
                        inner: Box::new(named(PRELUDE_POLLER)),
                    },
                    Type::Ref {
                        unique: false,
                        region: Region::Param(1),
                        inner: Box::new(named(handle)),
                    },
                    Type::Int,
                ];
                if self == Builtin::PollerAddPipe {
                    params.push(Type::Int);
                }
                (params, Type::Int)
            }
            Builtin::ListenerClose => (vec![named(PRELUDE_LISTENER)], Type::Int),
            // No capability, no data in, one opaque handle out
            // (`docs/opaque-pointers.md` §3) -- a fixed signature like
            // `sqrt`'s, not a call-site check like `len`'s.
            Builtin::NullPtr => (Vec::new(), Type::CPtr),
            // Checked at the call site, like `len`: `T` and `R` come
            // from `payload`'s and `body`'s own types
            // (`docs/threads.md` §2).
            Builtin::Spawn => (Vec::new(), Type::Unit),
            Builtin::Join => (Vec::new(), Type::Unit),
            // No arguments, no capability, and a fixed `int` result like
            // `byte_of`'s -- the value is never actually produced, since
            // the call never returns, but the type checker needs one to
            // check the call site the ordinary way.
            Builtin::Trap => (Vec::new(), Type::Int),
        }
    }

    /// What performing this builtin costs a caller's row.
    ///
    /// `putchar` writes to the console, so it performs `io_write`. This is the
    /// *grounding* of the whole system: every `io_write` in every row above it
    /// traces back here, because a label nothing performs can never appear
    /// in an exact row (§7.3).
    pub fn effects(self) -> Effects {
        match self {
            Builtin::PutChar | Builtin::Write | Builtin::FlushOut => Effects::plain(["io_write"]),
            // Its own label rather than `io_write`, because the stream is
            // the unit a reader can act on: `1>` and `2>` are two
            // redirections (`docs/standard-error.md` §3.1).
            Builtin::WriteErr => Effects::plain(["err_write"]),
            // `docs/standard-input.md` §2: the same capability, the other
            // direction, its own label. A row saying `[io_write]` does not
            // permit a read, which is what makes the two labels a
            // distinction rather than a spelling.
            Builtin::GetChar => Effects::plain(["io_read"]),
            // `docs/heap.md` §2. Both reach the allocator, so both perform
            // `heap`; `contents` is a load and performs nothing.
            Builtin::Box
            | Builtin::Unbox
            | Builtin::BoxSlice
            | Builtin::UnboxSlice
            | Builtin::ForkHeap => Effects::plain(["heap"]),
            // §2: reading the command line is an effect, because a
            // function whose behaviour depends on it should say so.
            Builtin::ArgCount | Builtin::Arg => Effects::plain(["args"]),
            // `docs/file-handles.md` §4.1: a path-free label, because the
            // path was spent at `open_read` and the row there still names
            // the directory. `close` performs nothing for the same reason
            // `release` does not -- ending a capability is not using one --
            // even though this one ends with a syscall.
            Builtin::ReadFile | Builtin::FilePread | Builtin::FileSize => {
                Effects::plain(["file_read"])
            }
            // `docs/file-writes.md` section 4: the same path-free rule on
            // the write side. A sync is `file_write`, the conservative
            // label (section 5.2).
            Builtin::FileWrite
            | Builtin::FilePwrite
            | Builtin::FileSync
            | Builtin::FileTruncate
            | Builtin::FileLock => Effects::plain(["file_write"]),
            // `docs/native-sockets.md` §3: path-free labels named after the
            // handle and the direction; `tcp_listen`'s row comes from the
            // bound at the call site.
            Builtin::TcpAccept => Effects::plain(["conn_accept"]),
            Builtin::ConnRead => Effects::plain(["conn_read"]),
            Builtin::ClockMs | Builtin::ClockUnixMs => Effects::plain(["clock"]),
            Builtin::PollerAddListener
            | Builtin::PollerAddConn
            | Builtin::PollerModify
            | Builtin::PollerRemove
            | Builtin::PollerWait
            | Builtin::PollerAddPipe
            | Builtin::PollerAddChild
            | Builtin::PollerAddSignals => Effects::plain(["poll"]),
            // `docs/signals.md` section 2.1: path-free, the set was spent at
            // `signals_watch`. Closing performs nothing, as `conn_close` does not.
            Builtin::SignalsPending => Effects::plain(["signals_read"]),
            // `docs/directory-handles.md` §2: the handle is the authority.
            Builtin::DirEnter | Builtin::DirOpenRead => Effects::plain(["dir_read"]),
            // `docs/directory-listing.md` §3.3: listing is reading beneath the
            // directory, and closing a listing performs nothing.
            Builtin::DirList | Builtin::DirNext | Builtin::DirStat => Effects::plain(["dir_read"]),
            // §3: everything that changes what is beneath a directory.
            Builtin::DirOpenNew
            | Builtin::DirOpenAppend
            | Builtin::DirRename
            | Builtin::DirRemove
            | Builtin::DirSync => Effects::plain(["dir_write"]),
            Builtin::ConnWrite => Effects::plain(["conn_write"]),
            // `docs/processes.md` §3.2: path-free, the program was named at
            // `exec_spawn`. Waiting and closing perform nothing, as
            // `conn_close` does not; `exec_spawn`'s row comes from the prefix
            // at the call site.
            Builtin::ChildKill => Effects::plain(["child_signal"]),
            Builtin::PipeRead => Effects::plain(["pipe_read"]),
            Builtin::PipeWrite => Effects::plain(["pipe_write"]),
            // Moving authority around is not an effect. Splitting a `World`
            // observes nothing outside the program and releasing a
            // capability only ends one; what a capability *authorises* is
            // where the effect is.
            // Arithmetic is not an effect, wrapping or not: it observes
            // nothing outside the program and needs no authority.
            _ => Effects::pure(),
        }
    }

    pub fn from_name(name: &str) -> Option<Builtin> {
        Builtin::ALL.iter().copied().find(|b| b.name() == name)
    }
}
