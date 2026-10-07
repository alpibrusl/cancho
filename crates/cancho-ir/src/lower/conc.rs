//! `spawn`/`join` (`docs/threads.md` §2): a real OS thread, with
//! `body`'s own compiled entry point becoming `pthread_create`'s start
//! routine directly. No compiler-synthesised trampoline exists in this
//! IR yet, which is why this first slice restricts the payload and the
//! return type to one pointer-width leaf each.

use crate::*;

/// Does `ty` cross to a real thread as exactly one pointer-width leaf,
/// or none at all (`docs/threads.md` §1)?
///
/// `pthread_create`'s start routine is `void *(*)(void *)` — one
/// pointer in, one pointer out — and every kind allowed here already
/// lowers to a single leaf that fits the general-purpose register
/// `void *` does: `int` is `i64`, `bool`/`byte` are narrower but cost
/// nothing wider (the same "a caller writing more than the callee
/// asked for costs nothing" rule a foreign parameter already relies
/// on, `docs/reach.md` §3.4), and `c_ptr`, a function value and a
/// reference are already pointers. `float` is refused even though it
/// is one leaf too: the wrong register class, and pthread's own C
/// signature has no way to carry one.
///
/// `docs/threads.md` §5 step 3: an *owned* capability, checked by
/// `DefId` rather than by walking its fields structurally. `File`
/// (`abi::leaves_into`'s own `PRELUDE_FILE` arm, both backends) is one
/// `i64` leaf — the fd, no different in shape from a plain `int` —
/// and `Io`/`Ffi`/`Fs`/`Args`/`Heap`/`Net`/`Clock` are all declared with **no**
/// fields at all (`defs.rs`'s own `prelude_types`), so neither backend's
/// `leaves_into` gives any of them a leaf to carry: the same zero-leaf
/// path this function already gives `Unit`. Both shapes are exactly
/// what this slice's codegen already handles for `()` and `int`, so
/// admitting them costs no new machinery — only the check. (`Net` and
/// `Clock` were described here as zero-field and left off the list until
/// `docs/parallelism.md` §3.4 found them refused: an omission, not a
/// position.) Not every capability is listed: `World` and `Split` are the *root* and a
/// bundle of every other one, and nothing here has asked to move a
/// thread the whole program's authority yet.
fn crosses_to_a_thread(ty: &Type) -> bool {
    match ty {
        Type::Unit | Type::Int | Type::Byte | Type::Bool | Type::CPtr | Type::Fn(..) => true,
        // A reference is one pointer leaf, unless it points at a slice,
        // which is a pointer *and* a length (`docs/strings.md` §6) --
        // two leaves, one too many for this first slice.
        Type::Ref { inner, .. } => !matches!(inner.as_ref(), Type::Slice(_)),
        Type::Named(def, _) => matches!(
            def.0 as usize,
            PRELUDE_FILE
                | PRELUDE_IO
                | PRELUDE_FFI
                | PRELUDE_FS
                | PRELUDE_ARGS
                | PRELUDE_HEAP
                | PRELUDE_NET
                | PRELUDE_CLOCK
        ),
        _ => false,
    }
}

impl<'a> FnLowering<'a> {
    /// `spawn(payload, body) -> [conc] res Thread[T, R]`
    /// (`docs/threads.md` §2).
    ///
    /// Checked here rather than through a written signature because
    /// `T` and `R` are read off `payload`'s and `body`'s own types,
    /// the same reason [`Self::boxed`] is checked here and not there.
    pub(crate) fn spawn(
        &mut self,
        args: &[ExprId],
        span: Span,
    ) -> Result<(Expr, Type), Diagnostic> {
        let [payload, body] = args else {
            return Err(Diagnostic::new(
                Rule::ArityMismatch,
                format!(
                    "`spawn` takes 2 arguments -- the payload and the function to run -- but {} were given",
                    args.len()
                ),
                span,
            ));
        };
        let payload_span = self.ast.expr_span(*payload);
        let reads_from = self.reads.len();
        let (payload_value, payload_ty) = self.expr(*payload)?;
        let payload_ty = self.unifier.resolve(&payload_ty);
        let payload_reads = self.reads[reads_from..].to_vec();

        let body_span = self.ast.expr_span(*body);
        let (body_value, body_ty) = self.expr(*body)?;
        let Type::Fn(params, effects, ret) = self.unifier.shallow(&body_ty) else {
            return Err(Diagnostic::new(
                Rule::NotAFunction,
                format!(
                    "`spawn`'s second argument must be a captureless function value, and `{}` is not one",
                    self.unifier.display(&body_ty)
                ),
                body_span,
            ));
        };
        if params.len() != 1 {
            return Err(Diagnostic::new(
                Rule::ArityMismatch,
                format!(
                    "`spawn`'s `body` must take exactly one parameter (the payload), but it takes {}",
                    params.len()
                ),
                body_span,
            ));
        }
        self.expect_type(&params[0], &payload_ty, payload_span)?;
        let ret = *ret;

        // `docs/threads.md` §1: the one wall this first slice does not
        // cross. Checked *after* the arity/type agreement above, so a
        // caller who passed the wrong kind of function hears about that
        // first.
        if !crosses_to_a_thread(&payload_ty) {
            return Err(Diagnostic::new(
                Rule::ThreadPayloadType,
                format!(
                    "`spawn`'s payload has type `{}`, which cannot cross to a real thread yet",
                    self.unifier.display(&payload_ty)
                ),
                payload_span,
            ));
        }
        if !crosses_to_a_thread(&ret) {
            return Err(Diagnostic::new(
                Rule::ThreadPayloadType,
                format!(
                    "`body` returns `{}`, which cannot cross back from a real thread yet",
                    self.unifier.display(&ret)
                ),
                body_span,
            ));
        }

        // `docs/aliasing.md` §6.1: a `&!` that crosses to a thread is lent
        // to it until the handle is joined. Nothing else may touch the
        // object it points at meanwhile, which includes a second thread
        // given another copy of the same reference.
        if matches!(payload_ty, Type::Ref { unique: true, .. }) && !payload_reads.is_empty() {
            self.trace.emit(Event::Lease { from: payload_reads, span });
            self.pending_lease = true;
        }

        // `docs/threads.md` §2: the row this costs is `conc` -- real
        // concurrency entering the program's authority surface -- plus
        // whatever `body` itself performs, because `body` only ever
        // runs because this call caused it to. Exactly the same union
        // `Expr::CallIndirect`'s own lowering already does for an
        // ordinary call through a value.
        self.performed.union(&Effects::plain(["conc"]));
        self.performed.union(&Effects::new(
            effects.iter().map(|l| Label { name: l.name.clone(), argument: l.argument.clone() }),
        ));

        Ok((
            Expr::Call {
                callee: Callee::Builtin(Builtin::Spawn),
                args: vec![payload_value, body_value],
            },
            // `T` rides along purely for its region: a payload that
            // borrows makes this type mention that region through
            // `Type::Named`'s own already-generic `mentions`/
            // `regions_into` walk into its type arguments, which is
            // what stops the handle from escaping the borrow the same
            // way any other reference is stopped (`docs/threads.md`
            // §3).
            Type::Named(self.prelude()[PRELUDE_THREAD], vec![payload_ty, ret]),
        ))
    }

    /// `join(handle) -> [row] R` (`docs/threads.md` §2) — the one
    /// consumer a `Thread[T, R]` has, the same "one consumer" shape
    /// [`Self::unboxed`] already has for `Box`.
    pub(crate) fn join(&mut self, args: &[ExprId], span: Span) -> Result<(Expr, Type), Diagnostic> {
        let [handle] = args else {
            return Err(Diagnostic::new(
                Rule::ArityMismatch,
                format!("`join` takes 1 argument, but {} were given", args.len()),
                span,
            ));
        };
        let handle_span = self.ast.expr_span(*handle);
        let (handle_value, handle_ty) = self.expr(*handle)?;
        // `docs/aliasing.md` §6.1: this is where a lease ends. Only the two
        // shapes the checker can follow end one -- a handle named by a
        // binding, and a `spawn` joined where it is made. A handle passed
        // through anything else keeps its lease for the rest of the borrow,
        // which refuses a program rather than admitting a race.
        match self.ast.expr(*handle) {
            AstExpr::Name(name) => {
                if let Some(binding) = self.lookup(*name) {
                    let slot = binding.slot;
                    self.trace.emit(Event::Join { handle: Some(slot) });
                }
            }
            AstExpr::Call { .. } if self.pending_lease => {
                self.trace.emit(Event::Join { handle: None });
            }
            _ => {}
        }
        self.pending_lease = false;
        let resolved = self.unifier.resolve(&handle_ty);
        let Type::Named(def, type_args) = &resolved else {
            return Err(Diagnostic::new(
                Rule::TypeMismatch,
                format!(
                    "`join` takes a thread handle from `spawn`, and `{}` is not one",
                    self.unifier.display(&resolved)
                ),
                handle_span,
            ));
        };
        if *def != self.prelude()[PRELUDE_THREAD] {
            return Err(Diagnostic::new(
                Rule::TypeMismatch,
                format!(
                    "`join` takes a thread handle from `spawn`, and `{}` is not one",
                    self.unifier.display(&resolved)
                ),
                handle_span,
            ));
        }
        let ret = type_args[1].clone();

        Ok((Expr::Joined { handle: Box::new(handle_value), ret: Box::new(ret.clone()) }, ret))
    }
}
