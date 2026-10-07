//! The console on WASI, without libc's stdio (`docs/wasm.md`, W2).
//!
//! `putchar`, `getchar` and `fwrite` bring in wasi-libc's stdio, and stdio brings
//! imports the program never asked for: `fd_fdstat_get` (is stdout a terminal),
//! `fd_seek`, `fd_close`, and `clock_time_get` -- the last from a futex timeout in
//! stdio's locking. W1 measured it: a program whose effect row is `[io_write]`, with
//! no `clock` label, imported a clock. The import section is the half of the
//! authority fact the *runtime* enforces, so it has to say what the row says.
//!
//! This is the same console, written against the two WASI calls it needs, so a
//! module that prints imports `fd_write` and a module that reads imports `fd_read`
//! and neither imports anything else for it:
//!
//! * **stdout** is buffered (4 KiB), flushed when it fills, on `flush_out`, and when
//!   `main` returns, which is what libc does to a pipe, so the bytes and their order
//!   are the ones the native build writes. A write that fails sets a sticky error,
//!   as the `FILE`'s indicator does, and `flush_out` reports it.
//! * **stderr** is unbuffered, as libc's is.
//! * **stdin** is read 4 KiB at a time through its own buffer; end of input or a
//!   failed read is `-1`, as `getchar`'s is.
//!
//! Everything here is `internal`, so a program that uses none of it carries none of
//! it. The one thing that is not free is the flush when `main` returns: it is
//! emitted only for a module that writes (`uses_console`), because an unconditional
//! one would make every pure program import `fd_write`.

use target_lexicon::{Architecture, Triple};

/// Does this target take its console from [`definitions`] rather than from libc?
pub(crate) fn applies(triple: &Triple) -> bool {
    triple.architecture == Architecture::Wasm32
}

/// Does emitted module text write to stdout, so that `main` has to flush it?
pub(crate) fn uses_console(module: &str) -> bool {
    module.contains("@cancho_stdout_write(")
        || module.contains("@cancho_putchar(")
        || module.contains("@cancho_console_flush(")
}

/// The console runtime as LLVM IR, for a wasm32 module.
pub(crate) fn definitions() -> String {
    r#"
; --- the console on WASI, without libc's stdio (docs/wasm.md, W2) ---
declare i32 @cancho_fd_write(i32, ptr, i32, ptr) #9001
declare i32 @cancho_fd_read(i32, ptr, i32, ptr) #9002
declare void @llvm.memcpy.p0.p0.i32(ptr, ptr, i32, i1)
attributes #9001 = { "wasm-import-module"="wasi_snapshot_preview1" "wasm-import-name"="fd_write" }
attributes #9002 = { "wasm-import-module"="wasi_snapshot_preview1" "wasm-import-name"="fd_read" }

@cancho_out_buf = internal global [4096 x i8] zeroinitializer
@cancho_out_len = internal global i32 0
@cancho_out_err = internal global i32 0
@cancho_in_buf = internal global [4096 x i8] zeroinitializer
@cancho_in_pos = internal global i32 0
@cancho_in_len = internal global i32 0

; Write all `n` bytes to `fd`, looping over short writes. 0, or a WASI errno.
define internal i32 @cancho_write_all(i32 %fd, ptr %p, i32 %n) {
entry:
  %iov = alloca [2 x i32]
  %nw = alloca i32
  br label %loop
loop:
  %cur = phi ptr [ %p, %entry ], [ %next, %cont ]
  %left = phi i32 [ %n, %entry ], [ %left2, %cont ]
  %done = icmp eq i32 %left, 0
  br i1 %done, label %ok, label %go
go:
  store ptr %cur, ptr %iov
  %lenslot = getelementptr i8, ptr %iov, i32 4
  store i32 %left, ptr %lenslot
  %rc = call i32 @cancho_fd_write(i32 %fd, ptr %iov, i32 1, ptr %nw)
  %failed = icmp ne i32 %rc, 0
  br i1 %failed, label %err, label %cont
cont:
  %w = load i32, ptr %nw
  %next = getelementptr i8, ptr %cur, i32 %w
  %left2 = sub i32 %left, %w
  %stuck = icmp eq i32 %w, 0
  br i1 %stuck, label %wedged, label %loop
wedged:
  ret i32 29
err:
  ret i32 %rc
ok:
  ret i32 0
}

; Remember the first failed stdout write, as `ferror` does.
define internal void @cancho_out_fail(i32 %rc) {
entry:
  %prev = load i32, ptr @cancho_out_err
  %had = icmp ne i32 %prev, 0
  %keep = select i1 %had, i32 %prev, i32 %rc
  store i32 %keep, ptr @cancho_out_err
  ret void
}

; Write out what stdout has buffered. 0, or the WASI errno that stopped it.
define internal i32 @cancho_console_flush() {
entry:
  %len = load i32, ptr @cancho_out_len
  %empty = icmp eq i32 %len, 0
  br i1 %empty, label %none, label %go
go:
  store i32 0, ptr @cancho_out_len
  %rc = call i32 @cancho_write_all(i32 1, ptr @cancho_out_buf, i32 %len)
  %bad = icmp ne i32 %rc, 0
  br i1 %bad, label %fail, label %none
fail:
  call void @cancho_out_fail(i32 %rc)
  ret i32 %rc
none:
  ret i32 0
}

; `fwrite(p, 1, n, stdout)`: the count written, which is `n` unless the write failed.
define internal i32 @cancho_stdout_write(ptr %p, i32 %n) {
entry:
  %len = load i32, ptr @cancho_out_len
  %room = sub i32 4096, %len
  %fits = icmp ule i32 %n, %room
  br i1 %fits, label %copy, label %spill
copy:
  %dst = getelementptr i8, ptr @cancho_out_buf, i32 %len
  call void @llvm.memcpy.p0.p0.i32(ptr %dst, ptr %p, i32 %n, i1 false)
  %nl = add i32 %len, %n
  store i32 %nl, ptr @cancho_out_len
  ret i32 %n
spill:
  %f = call i32 @cancho_console_flush()
  %big = icmp uge i32 %n, 4096
  br i1 %big, label %direct, label %refill
direct:
  %rc = call i32 @cancho_write_all(i32 1, ptr %p, i32 %n)
  %bad = icmp ne i32 %rc, 0
  br i1 %bad, label %directfail, label %directok
directfail:
  call void @cancho_out_fail(i32 %rc)
  ret i32 0
directok:
  ret i32 %n
refill:
  call void @llvm.memcpy.p0.p0.i32(ptr @cancho_out_buf, ptr %p, i32 %n, i1 false)
  store i32 %n, ptr @cancho_out_len
  ret i32 %n
}

; `fwrite(p, 1, n, stderr)`: unbuffered.
define internal i32 @cancho_stderr_write(ptr %p, i32 %n) {
entry:
  %rc = call i32 @cancho_write_all(i32 2, ptr %p, i32 %n)
  %bad = icmp ne i32 %rc, 0
  %count = select i1 %bad, i32 0, i32 %n
  ret i32 %count
}

; `putchar(c)`: the byte written as an unsigned char, or -1.
define internal i32 @cancho_putchar(i32 %c) {
entry:
  %b = alloca i8
  %byte = trunc i32 %c to i8
  store i8 %byte, ptr %b
  %wrote = call i32 @cancho_stdout_write(ptr %b, i32 1)
  %ok = icmp eq i32 %wrote, 1
  %v = zext i8 %byte to i32
  %r = select i1 %ok, i32 %v, i32 -1
  ret i32 %r
}

; `getchar()`: the next byte of standard input, or -1 at the end or on an error.
define internal i32 @cancho_getchar() {
entry:
  %pos = load i32, ptr @cancho_in_pos
  %len = load i32, ptr @cancho_in_len
  %have = icmp ult i32 %pos, %len
  br i1 %have, label %take, label %fill
fill:
  %iov = alloca [2 x i32]
  %nr = alloca i32
  store ptr @cancho_in_buf, ptr %iov
  %lenslot = getelementptr i8, ptr %iov, i32 4
  store i32 4096, ptr %lenslot
  %rc = call i32 @cancho_fd_read(i32 0, ptr %iov, i32 1, ptr %nr)
  %n = load i32, ptr %nr
  %okrc = icmp eq i32 %rc, 0
  %some = icmp ne i32 %n, 0
  %good = and i1 %okrc, %some
  br i1 %good, label %loaded, label %eof
loaded:
  store i32 %n, ptr @cancho_in_len
  store i32 1, ptr @cancho_in_pos
  %first = load i8, ptr @cancho_in_buf
  %v0 = zext i8 %first to i32
  ret i32 %v0
eof:
  store i32 0, ptr @cancho_in_len
  store i32 0, ptr @cancho_in_pos
  ret i32 -1
take:
  %at = getelementptr i8, ptr @cancho_in_buf, i32 %pos
  %b = load i8, ptr %at
  %np = add i32 %pos, 1
  store i32 %np, ptr @cancho_in_pos
  %v = zext i8 %b to i32
  ret i32 %v
}
"#
    .to_owned()
}
