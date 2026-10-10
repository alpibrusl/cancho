//! The entry point of a WASI command module (`docs/wasm.md`, W2c).
//!
//! wasi-libc's `crt1-command.o` defines `_start`, which runs the constructors, calls
//! `__main_void`, runs the destructors, and exits with `main`'s status if it is not
//! zero. `__main_void` is what fetches the command line, with `args_sizes_get` and
//! `args_get`, **before every program**, so a program that released its `args`
//! capability and never reads one still imported both (W1 measured it). The import
//! section is the half of the authority fact the runtime enforces, and a module that
//! imports `args_get` has been granted the command line whatever its row says.
//!
//! So the module defines `_start` itself, with the same steps, and fetches the command
//! line only if the program reads it: the loader below is emitted only for a module
//! that loads `@lexs_argc` or `@lexs_argv`, which is exactly a program with the `args`
//! label in its row. A program that does not has no `args_get` in it to import.
//!
//! The link is without `crt1-command.o` (`link_wasm` in the CLI). What it supplied is
//! all here: `__wasm_call_ctors` (synthesised by the linker, and where wasi-libc hangs
//! its own set-up), `__wasm_call_dtors` (libc's `exit` handlers), and `_Exit` for a
//! non-zero status, which is `proc_exit`, the one import every program keeps.

/// Does emitted module text read the command line?
///
/// The entry stores `argc` and `argv` into two globals before any program code runs, and
/// the `arg_count` and `arg` builtins are the only things that load them.
pub(crate) fn reads_args(module: &str) -> bool {
    module.contains("= load i64, ptr @lexs_argc") || module.contains("= load ptr, ptr @lexs_argv")
}

/// `_start`, and the command-line loader if the program reads one. `cancho_entry` is the
/// wrapper `emit_module` writes around `main`: `(argc, argv) -> status`.
pub(crate) fn definitions(uses_args: bool) -> String {
    let mut out = String::from(
        r#"
; --- the entry point of a WASI command, without crt1-command.o (docs/wasm.md, W2c) ---
declare void @__wasm_call_ctors()
declare void @__wasm_call_dtors()
declare void @_Exit(i32) noreturn

define void @_start() {
entry:
  call void @__wasm_call_ctors()
  %argc = alloca i32
  %argv = alloca ptr
  store i32 0, ptr %argc
  store ptr null, ptr %argv
"#,
    );
    if uses_args {
        out.push_str("  call void @cancho_args_load(ptr %argc, ptr %argv)\n");
    }
    out.push_str(
        r#"  %c = load i32, ptr %argc
  %v = load ptr, ptr %argv
  %status = call i32 @cancho_entry(i32 %c, ptr %v)
  call void @__wasm_call_dtors()
  %bad = icmp ne i32 %status, 0
  br i1 %bad, label %leave, label %done
leave:
  call void @_Exit(i32 %status)
  unreachable
done:
  ret void
}
"#,
    );
    if uses_args {
        out.push_str(
            r#"
declare i32 @cancho_args_sizes_get(ptr, ptr) #9003
declare i32 @cancho_args_get(ptr, ptr) #9004
attributes #9003 = { "wasm-import-module"="wasi_snapshot_preview1" "wasm-import-name"="args_sizes_get" }
attributes #9004 = { "wasm-import-module"="wasi_snapshot_preview1" "wasm-import-name"="args_get" }

; What `__main_void` did: ask how many arguments and how many bytes, allocate both, fill
; them in, and end `argv` with a null. A failed call or an allocation that does not
; succeed stops the program, as an out-of-memory `malloc` does everywhere else.
define internal void @cancho_args_load(ptr %argc_out, ptr %argv_out) {
entry:
  %n = alloca i32
  %bytes = alloca i32
  %rc = call i32 @cancho_args_sizes_get(ptr %n, ptr %bytes)
  %bad = icmp ne i32 %rc, 0
  br i1 %bad, label %stop, label %go
go:
  %count = load i32, ptr %n
  %size = load i32, ptr %bytes
  %slots = add i32 %count, 1
  %arrbytes = shl i32 %slots, 2
  %argv = call ptr @cancho_malloc(i32 %arrbytes)
  %buf = call ptr @cancho_malloc(i32 %size)
  %nargv = icmp eq ptr %argv, null
  %nbuf = icmp eq ptr %buf, null
  %notempty = icmp ne i32 %size, 0
  %nbuf_fatal = and i1 %nbuf, %notempty
  %failed = or i1 %nargv, %nbuf_fatal
  br i1 %failed, label %stop, label %fill
fill:
  %rc2 = call i32 @cancho_args_get(ptr %argv, ptr %buf)
  %bad2 = icmp ne i32 %rc2, 0
  br i1 %bad2, label %stop, label %done
done:
  %end = getelementptr ptr, ptr %argv, i32 %count
  store ptr null, ptr %end
  store i32 %count, ptr %argc_out
  store ptr %argv, ptr %argv_out
  ret void
stop:
  call void asm sideeffect "unreachable", ""()
  unreachable
}
"#,
        );
    }
    out
}
