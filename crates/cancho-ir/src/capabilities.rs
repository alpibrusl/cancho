//! The prelude's capability predicates: which types are capabilities, which occupy no machine values, and which have
//! exactly one consumer (`release`, `close`, `unbox`). Split out of `defs.rs`, which outgrew its line budget when
//! `Out` and `Ports` (`docs/net-bounds.md` §3) joined the lists.

use crate::*;

/// Does a value of this type occupy no machine values at all?
///
/// §8.1: "capabilities erase at compile time except where they carry data".
/// A struct with no fields has no leaves, so threading one is free -- which
/// is a claim worth a test rather than a comment.
pub fn leaf_free(ty: &Type) -> bool {
    matches!(ty, Type::Named(def, _)
    if matches!(
        def.0 as usize,
        PRELUDE_WORLD
            | PRELUDE_IO
            | PRELUDE_FFI
            | PRELUDE_FS
            | PRELUDE_HEAP
            | PRELUDE_ARGS
            | PRELUDE_NET
            | PRELUDE_OUT
            | PRELUDE_PORTS
            | PRELUDE_CLOCK
            | PRELUDE_SIGNALS
            | PRELUDE_EXEC
    ))
}

/// Is this the file handle (`docs/file-handles.md`)?
///
/// Unlike the six capabilities above it is **not** leaf-free: a descriptor
/// is a number the kernel gave us, so a `File` is one leaf. That is the
/// whole difference between it and an `Io`, which is authority with nothing
/// behind it.
pub fn is_file(def: DefId) -> bool {
    def.0 as usize == PRELUDE_FILE
}

/// Is this one of the prelude's capability types?
///
/// §8.2: authority comes from exactly one place, and a program that could
/// *write* `Io { }` would have an ambient constructor by another name — the
/// very thing "no `Io::global()`, no `unsafe { }` that conjures one" rules
/// out. So a capability has no literal form, and the only `Io` in existence
/// is the one the runtime handed to `main` inside a `World`.
pub(crate) fn is_capability(def: DefId) -> bool {
    matches!(
        def.0 as usize,
        PRELUDE_WORLD
            | PRELUDE_IO
            | PRELUDE_FFI
            | PRELUDE_FS
            | PRELUDE_HEAP
            | PRELUDE_ARGS
            | PRELUDE_SPLIT
            | PRELUDE_SPLIT_NET
            | PRELUDE_SPLIT_CLOCK
            | PRELUDE_SPLIT_SIGNALS
            | PRELUDE_SPLIT_EXEC
            | PRELUDE_EXEC
            | PRELUDE_CHILD
            | PRELUDE_PIPE
            | PRELUDE_CHILD_END
            | PRELUDE_CLOCK
            | PRELUDE_SIGNALS
            | PRELUDE_SIGNAL_WATCH
            | PRELUDE_NET
            | PRELUDE_OUT
            | PRELUDE_PORTS
            | PRELUDE_SPLIT_PORTS
            | PRELUDE_TTY
            | PRELUDE_SPLIT_TTY
            // §8.2 again, and more sharply: a program that could write
            // `File { }` would be conjuring a descriptor, which is worse
            // than conjuring authority because the number would be someone
            // else's open file.
            | PRELUDE_FILE
            | PRELUDE_DIR
            | PRELUDE_DIR_LIST
            | PRELUDE_LISTENER
            | PRELUDE_CONN
            | PRELUDE_UDP
            | PRELUDE_POLLER
    )
}

/// Is this a capability whose only consumer is `release`?
///
/// `Split` is not: taking it apart is the whole point of having one.
/// `World` and `Io` are, because destructuring either would destroy
/// authority without naming the function that knows how (§4.1) — and for
/// `World` and `Io`, which carry no fields, it would do it silently.
pub(crate) fn released_only(def: DefId) -> bool {
    matches!(
        def.0 as usize,
        PRELUDE_WORLD
            | PRELUDE_IO
            | PRELUDE_FFI
            | PRELUDE_FS
            | PRELUDE_HEAP
            | PRELUDE_ARGS
            | PRELUDE_NET
            | PRELUDE_OUT
            | PRELUDE_PORTS
            | PRELUDE_CLOCK
            | PRELUDE_SIGNALS
            | PRELUDE_EXEC
            // `docs/tty.md` §4: the serial-port capability, released the
            // way every root capability is.
            | PRELUDE_TTY
    )
}

/// Is this a type whose only consumer is `close` (`docs/file-handles.md`)?
///
/// The same rule as [`released_only`] and [`unboxed_only`], pointed at the
/// third thing that owns something the language cannot see: a descriptor.
/// Destructuring a `File` would drop it without calling `close`, which is
/// a leak the kernel keeps rather than one the allocator does.
pub(crate) fn closed_only(def: DefId) -> bool {
    matches!(
        def.0 as usize,
        PRELUDE_FILE
            | PRELUDE_DIR
            | PRELUDE_DIR_LIST
            | PRELUDE_LISTENER
            | PRELUDE_CONN
            | PRELUDE_UDP
            | PRELUDE_TTY
            | PRELUDE_POLLER
            | PRELUDE_SIGNAL_WATCH
            // `docs/processes.md` §4.7: a child is ended by `child_wait`, a
            // channel's end by `pipe_close`, and the child's end by
            // `exec_spawn` or `child_end_close` -- never by a pattern.
            | PRELUDE_CHILD
            | PRELUDE_PIPE
            | PRELUDE_CHILD_END
    )
}

/// Is this a type whose only consumer is `unbox` (`docs/heap.md` §3)?
///
/// `Box[T]` carries no *fields* — what it owns is an allocation, and the
/// pointer to it is not something a pattern can name. Destructuring one
/// would end the allocation without naming the function that frees it,
/// which is §4.1's rule and the same reason a capability may not be taken
/// apart.
pub(crate) fn unboxed_only(def: DefId) -> bool {
    def.0 as usize == PRELUDE_BOX
}
