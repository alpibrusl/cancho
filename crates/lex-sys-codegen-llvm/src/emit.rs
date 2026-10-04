//! Lowering `lex_sys_ir::Program` to LLVM textual IR.
//!
//! Every lex-sys value is its leaves, exactly as `lex-sys-codegen`'s own
//! `abi.rs` scalarises one: a capability is zero leaves, a reference is one
//! pointer leaf (whatever it refers to), `int` is one `i64`. Unlike the
//! Cranelift path, which builds SSA `Variable`s that Cranelift itself
//! promotes out of memory, every leaf here is one `alloca`, written and
//! read with plain `store`/`load` -- `clang`'s mandatory `mem2reg` does the
//! promotion this crate does not bother building, which is the one place
//! shelling out to `clang` buys more than a linker.
//!
//! A failure returns `(Option<usize>, String)` -- the function index and a
//! message -- exactly the shape [`lex_sys_codegen::CodegenError`] wants,
//! without this crate depending on Cranelift to build it.
//!
//! `mem2reg` is not automatic: it runs as part of `clang`'s standard `-O1+`
//! pipeline, not at the default `-O0` (`docs/llvm-backend.md` §7 measured
//! the difference and corrected the earlier "mandatory" claim). `lib.rs`'s
//! `run_clang` always passes `-O2` for exactly this reason -- without it,
//! every leaf here stays a real stack slot.

use lex_sys_ir::Program;
use lex_sys_types::Type;
use target_lexicon::Triple;

use crate::*;

/// One arena's chunk, matching `lex-sys-codegen`'s own `abi::ARENA_CHUNK`
/// exactly (§7.5): some `benches/` programs (`sieve_checked.ls`'s own
/// header) size their allocation against this constant, so a mismatched
/// chunk size would trap where the Cranelift build does not, or the
/// reverse.
pub(crate) const ARENA_CHUNK: i64 = 64 * 1024;

/// Every NaN's canonical `bits_of` answer, matching `lex-sys-codegen`'s
/// own `abi::CANONICAL_NAN` exactly (§7.17): x86-64 and aarch64 disagree
/// on the sign bit a generated NaN (`0.0 / 0.0`) carries, so `bits_of`
/// canonicalises every NaN to this one pattern rather than letting the
/// answer depend on where the program ran.
pub(crate) const CANONICAL_NAN: i64 = 0x7ff8_0000_0000_0000;

/// `2^63` as a `float`, both signs -- `truncate`'s own trap bound
/// (`docs/floating-point.md` §4: "any magnitude at or beyond `2^63`",
/// which includes exactly `-2^63` even though it is a representable
/// `i64`, the same choice Cranelift's `fcvt_to_sint` makes to sidestep
/// x86's `cvttsd2si` "integer indefinite" pattern colliding with a
/// legitimate `int::MIN`). Printed in hex float syntax for the same
/// bit-exactness reason `LValue::FConst` is.
pub(crate) const TRUNCATE_UPPER_BOUND: &str = "0x43E0000000000000";
pub(crate) const TRUNCATE_LOWER_BOUND: &str = "0xC3E0000000000000";

/// `docs/reach.md` §3: a capability parameter carries no data at run
/// time, so it never crosses to C. Mirrors `lex-sys-codegen`'s own
/// `abi::crosses_to_c` exactly -- the one exception is a `&r [byte]`
/// reference, which crosses as the pointer-and-length pair every other
/// slice leaf already is.
pub(crate) fn crosses_to_c(ty: &Type) -> bool {
    match ty {
        Type::Ref { inner, .. } => {
            matches!(inner.as_ref(), Type::Slice(element) if **element == Type::Byte)
        }
        _ => true,
    }
}

/// The machine types a leaf may be. `F64` is `docs/floating-point.md`'s
/// `float` -- binary64, one leaf, exactly like `int` (§7.17).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LKind {
    I64,
    I8,
    Ptr,
    F64,
}

impl LKind {
    pub(crate) fn llvm(self) -> &'static str {
        match self {
            LKind::I64 => "i64",
            LKind::I8 => "i8",
            LKind::Ptr => "ptr",
            LKind::F64 => "double",
        }
    }

    pub(crate) fn zero(self) -> &'static str {
        match self {
            LKind::I64 | LKind::I8 => "0",
            LKind::Ptr => "null",
            // Exact: zero is the one float value with a trivial bit
            // pattern, so it round-trips through decimal with nothing
            // lost, unlike every other float constant here (`FConst`
            // below).
            LKind::F64 => "0.0",
        }
    }
}

/// An operand: a compile-time constant, printed inline, or a named
/// register a prior instruction produced. LLVM textual IR allows a
/// constant wherever a register is expected, so a literal never needs an
/// instruction of its own the way Cranelift's `iconst` does.
#[derive(Clone)]
pub(crate) enum LValue {
    Const(i64),
    /// A `float` constant, stored as its raw bits -- `Expr::Float`'s own
    /// shape (`docs/floating-point.md` §1) -- and printed in LLVM's hex
    /// float syntax rather than decimal, so the constant this backend
    /// emits is the bit pattern the parser read, not whatever a decimal
    /// round-trip through `f64`'s `Display` happens to preserve.
    FConst(u64),
    Reg(String),
}

pub(crate) fn operand(v: &LValue) -> String {
    match v {
        LValue::Const(n) => n.to_string(),
        LValue::FConst(bits) => format!("0x{bits:016X}"),
        LValue::Reg(name) => name.clone(),
    }
}

/// The LLVM type a multi-leaf value returns as: an anonymous struct, one
/// field per leaf -- the same shape `checked_arith`'s own `{i64, i1}`
/// already is for LLVM's overflow intrinsics, applied here to a
/// `lex-sys` value with more than one leaf (`docs/llvm-backend.md`
/// §7.7: `contents(b)`'s own two leaves, called through a function like
/// `reduce_checked.ls`'s `fill`, is what this exists for).
pub(crate) fn struct_ty(kinds: &[LKind]) -> String {
    format!("{{{}}}", kinds.iter().map(|k| k.llvm()).collect::<Vec<_>>().join(", "))
}

/// The leaves a type scalarises to, matching `lex-sys-codegen`'s
/// `abi::leaves_into` for the subset this slice supports.
///
/// `Err` names what is missing rather than panicking: a type this function
/// refuses is a type the first slice does not lower yet, not a checker bug.
pub(crate) fn leaves_of(ty: &Type, program: &Program) -> Result<Vec<LKind>, String> {
    let mut out = Vec::new();
    leaves_into(ty, program, &mut out)?;
    Ok(out)
}

fn leaves_into(ty: &Type, program: &Program, out: &mut Vec<LKind>) -> Result<(), String> {
    match ty {
        Type::Int => out.push(LKind::I64),
        // `docs/opaque-pointers.md` §4: LLVM's own `ptr` kind, not `i64`
        // -- an opaque handle is never arithmetic here, so there is no
        // reason to spell it as an integer the way Cranelift's pointer-
        // width leaf does; `LKind::Ptr`'s `zero()` is already `"null"`,
        // exactly the sentinel `null_ptr()` needs.
        Type::CPtr => out.push(LKind::Ptr),
        // `docs/function-values.md` §4.2: `val`, one leaf, a pointer to
        // the target's compiled entry point -- LLVM's own `ptr` kind,
        // the same one `Type::CPtr` and a reference's own leaf use.
        Type::Fn(..) => out.push(LKind::Ptr),
        Type::Byte | Type::Bool => out.push(LKind::I8),
        Type::Float => out.push(LKind::F64),
        Type::Ref { inner, .. } => {
            out.push(LKind::Ptr);
            if matches!(inner.as_ref(), Type::Slice(_)) {
                out.push(LKind::I64);
            }
        }
        Type::Tuple(parts) => {
            for part in parts {
                leaves_into(part, program, out)?;
            }
        }
        // `docs/file-handles.md`: a handle at run time is a descriptor
        // and nothing else -- one leaf, where the six capabilities are
        // zero. Matches `lex-sys-codegen`'s own `abi::leaves_into`
        // exactly; `File` is a prelude type, not an ordinary struct
        // `program.type_info` would scalarise correctly on its own --
        // `Opened`/`Read` are ordinary prelude *enums* instead, and
        // fall through to the general `TypeInfo::Enum` arm below
        // unaided, the same way they do for Cranelift.
        Type::Named(def, _)
            if matches!(
                def.0 as usize,
                lex_sys_ir::PRELUDE_FILE
                    | lex_sys_ir::PRELUDE_LISTENER
                    | lex_sys_ir::PRELUDE_CONN
                    | lex_sys_ir::PRELUDE_POLLER
            ) =>
        {
            out.push(LKind::I64);
        }
        // `docs/heap.md` §3: a box at run time is a pointer and nothing
        // else -- no header, no refcount, no tag -- except a box of an
        // *unsized* referent, which carries the length too, because
        // nothing else knows how many elements there are (`docs/boxed-
        // slices.md` §2, the same pair `&r [T]` already is). Matches
        // `lex-sys-codegen`'s own `abi::leaves_into` exactly; `Box` is a
        // prelude type, not an ordinary struct `program.type_info` would
        // scalarise correctly on its own.
        Type::Named(def, args) if def.0 as usize == lex_sys_ir::PRELUDE_BOX => {
            out.push(LKind::Ptr);
            if matches!(args.first(), Some(Type::Slice(_))) {
                out.push(LKind::I64);
            }
        }
        // `docs/threads.md` §2: a `Thread[T, R]` at run time is a real
        // `pthread_t`, one leaf -- the same "no fields on purpose" shape
        // `Box` already has above. Matches `lex-sys-codegen`'s own
        // `abi::leaves_into` exactly.
        Type::Named(def, _) if def.0 as usize == lex_sys_ir::PRELUDE_THREAD => {
            out.push(LKind::Ptr);
        }
        Type::Named(def, args) => match program.type_info(*def) {
            lex_sys_ir::TypeInfo::Struct { fields, .. } => {
                for (_, field) in fields {
                    leaves_into(&field.substitute(args, &[]), program, out)?;
                }
            }
            // A tag, then *every* variant's payload leaves -- wasteful and
            // deliberately so, matching `lex-sys-codegen`'s own rule
            // (`abi::leaves_into`): overlaying the payloads is a layout
            // decision no M1 backend makes. The tag is an `i64` for the
            // same reason -- picking a narrower integer would be one too.
            lex_sys_ir::TypeInfo::Enum { variants, .. } => {
                out.push(LKind::I64);
                for (_, payload) in variants {
                    for ty in payload {
                        leaves_into(&ty.substitute(args, &[]), program, out)?;
                    }
                }
            }
        },
        other => {
            return Err(format!(
                "`{other:?}` is not part of the LLVM backend's first slice (docs/llvm-backend.md §5)"
            ));
        }
    }
    Ok(())
}

pub(crate) fn emit_module(
    program: &Program,
    entry: &str,
    triple: &Triple,
) -> Result<String, (Option<usize>, String)> {
    let mut text = String::new();
    text.push_str(&format!("target triple = \"{triple}\"\n\n"));
    text.push_str("declare i32 @putchar(i32)\n");
    // `docs/standard-input.md` §3 (§7.9): the mirror. `int getchar(void)`
    // -- no parameter, and the same `i32` result sign-extended at the
    // edge, which is what carries `EOF`'s `-1` back as `-1` rather than
    // as a very large unsigned number were it zero-extended instead.
    text.push_str("declare i32 @getchar()\n");
    // `docs/bulk-io.md` §3 (§7.11): the whole slice in one call, through
    // the same stdio stream `putchar` uses. `stdout`/`stderr` are `FILE
    // *` *variables* in libc, so the symbol is the address of the
    // pointer and the stream itself is one load away -- and the symbol
    // differs by platform, the same split `lex-sys-codegen`'s own
    // `emit.rs` already makes for it.
    text.push_str("declare i64 @fwrite(ptr, i64, i64, ptr)\n");
    let (stdout_symbol, stderr_symbol) = match triple.operating_system {
        target_lexicon::OperatingSystem::Darwin(_) => ("__stdoutp", "__stderrp"),
        _ => ("stdout", "stderr"),
    };
    text.push_str(&format!("@{stdout_symbol} = external global ptr\n"));
    text.push_str(&format!("@{stderr_symbol} = external global ptr\n"));
    // `region`/`alloc_slice` (§7.5): one `malloc` per arena, one `free` on
    // the way out -- `lex-sys-codegen`'s own `body/memory.rs` `libc_fn`
    // declares these the same way, on first use rather than unconditionally
    // there, but an unused `declare` here costs nothing, the same reasoning
    // `putchar`'s own unconditional declaration already relies on.
    text.push_str("declare ptr @malloc(i64)\n");
    text.push_str("declare void @free(ptr)\n");
    // Checked arithmetic (§5's second slice): the three overflow-reporting
    // intrinsics `Expr::Bin`'s `Add`/`Sub`/`Mul` arms call. Declared
    // unconditionally, the same way `putchar` is -- an unused `declare`
    // costs nothing, and every function in the module shares one `.ll`.
    text.push_str("declare {i64, i1} @llvm.sadd.with.overflow.i64(i64, i64)\n");
    text.push_str("declare {i64, i1} @llvm.ssub.with.overflow.i64(i64, i64)\n");
    text.push_str("declare {i64, i1} @llvm.smul.with.overflow.i64(i64, i64)\n");
    // `sqrt` (§7.17, `docs/float-math.md` §2): correctly rounded per
    // IEEE-754, which is why this is the one arithmetic builtin that is
    // an intrinsic rather than an instruction sequence.
    text.push_str("declare double @llvm.sqrt.f64(double)\n");

    // Every libc symbol from here down is one a program predating `Net`/
    // `Fs` (`examples/serve/`, `examples/vsock/`, and siblings) may
    // declare for itself through `extern fn`, at lex-sys's own crossing
    // widths (`docs/reach.md` §3) rather than libc's true ones -- exactly
    // the collision `read`/`write` were first guarded against
    // (`tests/accept/bytes_to_c.ls`, this backend's #117). Declaring
    // unconditionally, the way `putchar`/`malloc` safely do, made
    // `--backend llvm` refuse every socket-declaring example the moment
    // it became the default (`docs/llvm-backend.md`'s own long-recorded
    // "already-accepted exposure," #92's Cranelift equivalent) -- not
    // hypothetical once this is what `build` reaches without a flag. So
    // every one of these is now guarded the same way: a program naming
    // the symbol itself is trusted to have declared the signature it
    // actually needs, and this backend does not also insist on its own.
    let extern_symbols: std::collections::BTreeSet<&str> =
        program.externs.iter().map(|e| e.symbol.as_str()).collect();
    let declare_libc_unless_own = |text: &mut String, symbol: &str, signature: &str| {
        if !extern_symbols.contains(symbol) {
            text.push_str(&format!("declare {signature}\n"));
        }
    };

    // `listen`/`accept` (`docs/net.md` §7.20, `docs/listen.md` §6):
    // neither takes a capability -- the port was already bound at
    // `bind`.
    // `docs/zeroed-slices.md`: a `box_slice` filled with zero. Guarded like
    // the rest: `calloc` is a name a program may declare for itself.
    declare_libc_unless_own(&mut text, "calloc", "ptr @calloc(i64, i64)");
    declare_libc_unless_own(&mut text, "listen", "i32 @listen(i32, i32)");
    declare_libc_unless_own(&mut text, "accept", "i32 @accept(i32, ptr, ptr)");
    // `bind` (§7.21, `docs/listen.md` §6): `socket`+`setsockopt`+`bind`
    // folded into one call, the same libc surface `examples/serve/
    // serve.ls` reaches by hand and `lex-sys-codegen`'s own `body/net.rs`
    // already declares for Cranelift.
    declare_libc_unless_own(&mut text, "socket", "i32 @socket(i32, i32, i32)");
    declare_libc_unless_own(&mut text, "setsockopt", "i32 @setsockopt(i32, i32, i32, ptr, i32)");
    // `conn_connect_status` (`docs/native-sockets.md` §10.6) reads `SO_ERROR`.
    declare_libc_unless_own(&mut text, "getsockopt", "i32 @getsockopt(i32, i32, i32, ptr, ptr)");
    declare_libc_unless_own(&mut text, "bind", "i32 @bind(i32, ptr, i32)");
    declare_libc_unless_own(&mut text, "close", "i32 @close(i32)");
    // `connect` (§7.22, `docs/connect.md` §10): the last of `Net`'s four
    // builtins, needing `getaddrinfo`/`freeaddrinfo` (host resolution)
    // and `connect` itself alongside the `socket` already declared above.
    declare_libc_unless_own(&mut text, "getaddrinfo", "i32 @getaddrinfo(ptr, ptr, ptr, ptr)");
    declare_libc_unless_own(&mut text, "freeaddrinfo", "void @freeaddrinfo(ptr)");
    declare_libc_unless_own(&mut text, "connect", "i32 @connect(i32, ptr, i32)");
    // The socket handles (`docs/native-sockets.md` §3). `fcntl` is declared
    // variadic, which is what it is: on Apple arm64 a variadic argument is
    // passed on the stack, so a fixed-signature declaration would put the
    // flags where `fcntl` does not look.
    // `docs/native-sockets.md` §4: the poller. Only the target's own
    // facility is declared -- `epoll` on Linux, `kqueue` on Darwin.
    match triple.operating_system {
        target_lexicon::OperatingSystem::Darwin(_) => {
            declare_libc_unless_own(&mut text, "kqueue", "i32 @kqueue()");
            declare_libc_unless_own(
                &mut text,
                "kevent",
                "i32 @kevent(i32, ptr, i32, ptr, i32, ptr)",
            );
        }
        _ => {
            declare_libc_unless_own(&mut text, "epoll_create1", "i32 @epoll_create1(i32)");
            declare_libc_unless_own(&mut text, "epoll_ctl", "i32 @epoll_ctl(i32, i32, i32, ptr)");
            declare_libc_unless_own(&mut text, "epoll_wait", "i32 @epoll_wait(i32, ptr, i32, i32)");
        }
    }
    declare_libc_unless_own(&mut text, "clock_gettime", "i32 @clock_gettime(i32, ptr)");
    declare_libc_unless_own(&mut text, "recv", "i64 @recv(i32, ptr, i64, i32)");
    declare_libc_unless_own(&mut text, "send", "i64 @send(i32, ptr, i64, i32)");
    declare_libc_unless_own(&mut text, "fcntl", "i32 @fcntl(i32, i32, ...)");
    text.push('\n');

    // `Fs` (§7.24, `docs/filesystem.md` §3-4, `docs/file-handles.md`):
    // `fs_read`/`fs_write` (`creat`/`open` then `read`/`write` then
    // `close`), `open_read` (`open`, descriptor kept), `file_read`
    // (`read`) and `file_close` (`close`, already declared above for
    // `bind`'s own use). `errno`'s accessor is a *function* in every
    // modern libc -- `__errno_location` on glibc, `__error` on Darwin --
    // both answering a pointer to a thread-local `int`, the same split
    // `lex-sys-codegen`'s own `errno` already makes.
    declare_libc_unless_own(&mut text, "creat", "i32 @creat(ptr, i32)");
    declare_libc_unless_own(&mut text, "open", "i32 @open(ptr, i32)");
    declare_libc_unless_own(&mut text, "read", "i64 @read(i32, ptr, i64)");
    // `copy_within` (`docs/memory-moves.md`) and `copy_into` (`docs/bulk-copy.md`).
    declare_libc_unless_own(&mut text, "memmove", "ptr @memmove(ptr, ptr, i64)");
    // `index_of_byte` (`docs/byte-search.md`).
    declare_libc_unless_own(&mut text, "memchr", "ptr @memchr(ptr, i32, i64)");
    declare_libc_unless_own(&mut text, "write", "i64 @write(i32, ptr, i64)");
    // `docs/file-writes.md`: the write side of a file handle. `fopen`/`dup`/
    // `fclose` are the opens' bridge (section 3); none of these is variadic.
    declare_libc_unless_own(&mut text, "fopen", "ptr @fopen(ptr, ptr)");
    declare_libc_unless_own(&mut text, "fileno", "i32 @fileno(ptr)");
    declare_libc_unless_own(&mut text, "dup", "i32 @dup(i32)");
    declare_libc_unless_own(&mut text, "fclose", "i32 @fclose(ptr)");
    declare_libc_unless_own(&mut text, "pwrite", "i64 @pwrite(i32, ptr, i64, i64)");
    declare_libc_unless_own(&mut text, "pread", "i64 @pread(i32, ptr, i64, i64)");
    declare_libc_unless_own(&mut text, "fsync", "i32 @fsync(i32)");
    // `flush_out` (`docs/checked-output.md`): the stream `fwrite` uses,
    // flushed, then asked whether an earlier write failed.
    declare_libc_unless_own(&mut text, "fflush", "i32 @fflush(ptr)");
    declare_libc_unless_own(&mut text, "ferror", "i32 @ferror(ptr)");
    declare_libc_unless_own(&mut text, "ftruncate", "i32 @ftruncate(i32, i64)");
    declare_libc_unless_own(&mut text, "lseek", "i64 @lseek(i32, i64, i32)");
    declare_libc_unless_own(&mut text, "unlink", "i32 @unlink(ptr)");
    declare_libc_unless_own(&mut text, "rename", "i32 @rename(ptr, ptr)");
    declare_libc_unless_own(&mut text, "flock", "i32 @flock(i32, i32)");
    let errno_symbol = match triple.operating_system {
        target_lexicon::OperatingSystem::Darwin(_) => "__error",
        _ => "__errno_location",
    };
    text.push_str(&format!("declare ptr @{errno_symbol}()\n\n"));

    // `docs/threads.md` §2: `spawn`/`join`, real `pthread_create`/
    // `pthread_join`. `pthread_t` is opaque on both this project's
    // targets (glibc's own `unsigned long`, Darwin's own pointer) but
    // always one register-width value, so `ptr` -- the same kind
    // `c_ptr` already uses -- holds it either way; `pthread_create`'s
    // own `pthread_t *thread` and `pthread_join`'s own `void **retval`
    // are the same `ptr` a step further, pointing at one.
    declare_libc_unless_own(&mut text, "pthread_create", "i32 @pthread_create(ptr, ptr, ptr, ptr)");
    declare_libc_unless_own(&mut text, "pthread_join", "i32 @pthread_join(ptr, ptr)");

    // `extern fn` (§7.23, §8.4): an import under the symbol the
    // declaration named. A capability parameter carries no data and
    // never reaches C (`crosses_to_c`); everything else crosses at
    // lex-sys's own widths -- an `int` is `i64` here whatever the C
    // function's own parameter width is, the same choice
    // `lex-sys-codegen`'s own `emit.rs` makes and `docs/reach.md` §3
    // documents. `Type::Unit` (no `-> Type` in the declaration) is
    // `void`; it is otherwise unwritable, so this is the one place it
    // is read rather than produced. Declared once per distinct symbol:
    // two `extern fn` declarations naming the same symbol (in different
    // modules, `docs/modules.md` §3) would otherwise redeclare it, which
    // `clang` refuses if the signatures disagree the same way Cranelift's
    // own `declare_function` already does.
    let mut declared_symbols = std::collections::BTreeSet::new();
    for ext in &program.externs {
        if !declared_symbols.insert(ext.symbol.clone()) {
            continue;
        }
        let mut params = Vec::new();
        for param in ext.params.iter().filter(|t| crosses_to_c(t)) {
            for leaf in leaves_of(param, program).map_err(|m| (None, m))? {
                params.push(leaf.llvm().to_owned());
            }
        }
        // `docs/reach.md` §3.4: `c_int` crosses at the real C ABI width
        // (32 bits), not `leaves_of`'s own `i64` -- the call site
        // (`body/expr.rs`'s `Callee::Extern` arm) sign-extends the
        // result back to this backend's own `int` after the call.
        let ret_ty = if matches!(ext.ret, Type::Unit) {
            "void".to_owned()
        } else if ext.narrow_return && matches!(ext.ret, Type::Int) {
            "i32".to_owned()
        } else {
            match leaves_of(&ext.ret, program).map_err(|m| (None, m))?.as_slice() {
                [] => "void".to_owned(),
                [k] => k.llvm().to_owned(),
                kinds => struct_ty(kinds),
            }
        };
        text.push_str(&format!("declare {ret_ty} @{}({})\n", ext.symbol, params.join(", ")));
    }
    if !program.externs.is_empty() {
        text.push('\n');
    }

    // Every `static`'s already-evaluated values (§7.25,
    // `docs/compile-time-data.md` §2) become one read-only global,
    // defined once for the whole program rather than once per reader --
    // `lex-sys-codegen`'s own `emit.rs` does the same, and for the same
    // reason a table is not a greeting. Stride matches `stride_of`
    // exactly (`body/memory.rs`): a `byte` packs one per byte, and
    // `int`/`bool`/`float` -- the only other element types a `static` may
    // hold (`compile-time-data.md` §4) -- pack at eight, the same
    // leaf-stride every other slice in this backend uses. An empty
    // `static` still gets a one-byte `zeroinitializer`-shaped constant,
    // for the reason an empty string literal does: a slice is a pointer
    // and a length, and the pointer has to be *some*thing.
    for data in &program.statics {
        let stride = if matches!(data.element, Type::Byte) {
            1usize
        } else {
            leaves_of(&data.element, program).map_err(|m| (None, m))?.len() * 8
        };
        let mut bytes = vec![0u8; (data.values.len() * stride).max(1)];
        for (i, value) in data.values.iter().enumerate() {
            let at = i * stride;
            bytes[at..at + stride].copy_from_slice(&value.to_le_bytes()[..stride]);
        }
        let items: Vec<String> = bytes.iter().map(|b| format!("i8 {b}")).collect();
        text.push_str(&format!(
            "@lexs_static_{} = private unnamed_addr constant [{} x i8] [{}]\n",
            data.name,
            bytes.len(),
            items.join(", ")
        ));
    }
    if !program.statics.is_empty() {
        text.push('\n');
    }

    // `arg_count`/`arg` (§7.13, `docs/arguments.md` §3): `argc`/`argv` as
    // `main` was handed them, stashed once into module-local storage and
    // never written again -- `lex-sys-codegen`'s own `ARGC_GLOBAL`/
    // `ARGV_GLOBAL` (`abi.rs`), the same reasoning applied to `internal
    // global` here instead of a `Linkage::Local` data object. `arg` reads
    // a NUL-terminated C string back from `argv`, so its length needs
    // libc's own `strlen` the way `docs/arguments.md` §3.2 describes.
    text.push_str("declare i64 @strlen(ptr)\n");
    // `conn_detach`/`conn_attach`'s epoch table (`docs/native-sockets.md`
    // §10.3): a counter per descriptor, in bss.
    text.push_str(&format!(
        "@{} = internal global [{} x i32] zeroinitializer\n",
        lex_sys_ir::FD_EPOCH_GLOBAL,
        lex_sys_ir::FD_EPOCH_SLOTS
    ));
    text.push_str("@lexs_argc = internal global i64 0\n");
    text.push_str("@lexs_argv = internal global ptr null\n\n");

    // Every string literal's bytes (§5's fourth slice) become one global
    // constant, named as it is met rather than once per unique text --
    // `docs/strings.md` §8 leaves interning an open question, so two
    // occurrences of the same literal get two objects here exactly as
    // `lex-sys-codegen`'s own `literals` counter gives them two. Built up
    // across every function before any of it is written into `text`,
    // because a function later in `program.funcs` may be the first one a
    // reader meets textually if `program.funcs` and source order ever
    // diverge (`docs/README.md`'s own "definition order never matters").
    let mut globals = String::new();
    let mut next_literal: u32 = 0;
    let mut bodies: Vec<String> = Vec::with_capacity(program.funcs.len());
    for (index, func) in program.funcs.iter().enumerate() {
        let body = FuncEmitter::new(program, func, triple, &mut globals, &mut next_literal)
            .and_then(|mut fe| fe.emit())
            .map_err(|message| (Some(index), message))?;
        bodies.push(body);
    }
    text.push_str(&globals);
    if !globals.is_empty() {
        text.push('\n');
    }
    for body in bodies {
        text.push_str(&body);
        text.push('\n');
    }

    let entry_id = program
        .find(entry)
        .ok_or_else(|| (None, format!("no function named `{entry}` to use as entry")))?;
    let entry_func = program.func(entry_id);
    let ret = leaves_of(&entry_func.ret, program).map_err(|m| (None, m))?;
    if ret.len() > 1 {
        return Err((None, "the entry point's return type has more than one leaf".to_owned()));
    }
    text.push_str("define i32 @main(i32 %argc, ptr %argv) {\n");
    text.push_str("entry:\n");
    // `docs/arguments.md` §3: written exactly once, before any lex-sys
    // code runs, and never again -- `lex-sys-codegen`'s own `emit_c_main`
    // stashes the same two values the same way, at the same point.
    text.push_str("  %argc64 = sext i32 %argc to i64\n");
    text.push_str("  store i64 %argc64, ptr @lexs_argc\n");
    text.push_str("  store ptr %argv, ptr @lexs_argv\n");
    match ret.first() {
        Some(LKind::I64) => {
            text.push_str(&format!("  %r = call i64 @lexs_{}()\n", entry_func.symbol()));
            text.push_str("  %status = trunc i64 %r to i32\n");
            text.push_str("  ret i32 %status\n");
        }
        // `docs/agent-errors.md`'s own convention applied here too: a
        // located, worded refusal rather than a silent wrong exit code.
        _ => {
            return Err((None, "`main` must return `int`, the process exit status".to_owned()));
        }
    }
    text.push_str("}\n");

    Ok(text)
}
