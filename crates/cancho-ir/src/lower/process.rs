//! `exec_spawn`, `exec_spawn_in` and `exec_spawn_detached` (`docs/processes.md` §3.2, §4.10, §4.11): the
//! process builtins whose row is read off a type.

use crate::*;

impl<'a> FnLowering<'a> {
    /// `exec_spawn(&Exec(p), path, args, env, stdin, stdout, stderr)` -- start
    /// the program at `path`; `exec_spawn_in` (`in_dir`) takes a borrowed `Dir`
    /// after the capability, the directory the child starts in (§4.10). Checked here rather than through a written
    /// signature because the row it performs is the prefix the capability was
    /// narrowed to, exactly the reason [`Self::open_file`] is checked here.
    pub(crate) fn exec_spawn(
        &mut self,
        args: &[ExprId],
        span: Span,
        in_dir: bool,
        detached: bool,
    ) -> Result<(Expr, Type), Diagnostic> {
        self.exec_spawn_with(args, span, in_dir, detached, false)
    }

    /// `exec_spawn_under(&Exec, &Dir, name, args, env, stdin, stdout, stderr)` (§4.14): `exec_spawn_in` whose path
    /// is a name beneath the `Dir`. Only the capability that bounds nothing (`Exec("")`) may be asked: a name
    /// resolved against a directory chosen at run time cannot be shown to lie under a literal, and a spawn the
    /// literal does not cover would widen the capability. The row is `exec_under`, which names no path.
    pub(crate) fn exec_spawn_under(
        &mut self,
        args: &[ExprId],
        span: Span,
    ) -> Result<(Expr, Type), Diagnostic> {
        self.exec_spawn_with(args, span, true, false, true)
    }

    fn exec_spawn_with(
        &mut self,
        args: &[ExprId],
        span: Span,
        in_dir: bool,
        detached: bool,
        under: bool,
    ) -> Result<(Expr, Type), Diagnostic> {
        let (name, wanted) = match (in_dir, detached) {
            (true, _) if under => ("exec_spawn_under", 8),
            (true, _) => ("exec_spawn_in", 8),
            (_, true) => ("exec_spawn_detached", 7),
            _ => ("exec_spawn", 7),
        };
        let arity = || {
            Diagnostic::new(
                Rule::ArityMismatch,
                format!(
                    "`{name}` takes {wanted} arguments -- the borrowed capability, {}the path, the arguments, the environment and the three streams -- but {} were given",
                    if in_dir { "the borrowed `Dir` to start in, " } else { "" },
                    args.len()
                ),
                span,
            )
        };
        if args.len() != wanted {
            return Err(arity());
        }
        let capability = &args[0];
        let [path, arguments, environment, stdin, stdout, stderr] =
            &args[1 + usize::from(in_dir)..]
        else {
            return Err(arity());
        };
        let capability_span = self.ast.expr_span(*capability);
        let (value, found) = self.expr(*capability)?;
        let prefix = self.granted_exec_prefix(&found, capability_span)?;
        if under && !prefix.is_empty() {
            return Err(Diagnostic::new(
                Rule::CapabilityMisused,
                format!(
                    "`exec_spawn_under` needs the `Exec` that bounds nothing: a program found beneath a directory chosen at run time cannot be shown to lie under `{prefix}`, and spawning it would widen the capability (`docs/processes.md` §4.14)"
                ),
                capability_span,
            ));
        }

        // The path, the arguments and the environment: three byte slices,
        // read and never written.
        let mut lowered = vec![value];
        // §4.10: the directory the child starts in is borrowed, shared, and
        // only read: its descriptor is named to the kernel, never closed.
        if in_dir {
            let wanted = Type::Ref {
                unique: false,
                region: self.unifier.fresh_region(),
                inner: Box::new(Type::Named(self.prelude()[PRELUDE_DIR], Vec::new())),
            };
            let dir_span = self.ast.expr_span(args[1]);
            let (dir_value, dir_ty) = self.expr(args[1])?;
            self.expect_type(&wanted, &dir_ty, dir_span)?;
            lowered.push(dir_value);
        }
        for bytes in [path, arguments, environment] {
            let wanted = Type::Ref {
                unique: false,
                region: self.unifier.fresh_region(),
                inner: Box::new(Type::Slice(Box::new(Type::Byte))),
            };
            let bytes_span = self.ast.expr_span(*bytes);
            let (bytes_value, bytes_ty) = self.expr(*bytes)?;
            self.expect_type(&wanted, &bytes_ty, bytes_span)?;
            lowered.push(bytes_value);
        }
        // The three streams, by value: a `ChildEnd` or a `File` in one is
        // handed to the child and is the parent's no longer (§4.4).
        let stdio = Type::Named(self.prelude()[PRELUDE_STDIO], Vec::new());
        for stream in [stdin, stdout, stderr] {
            let stream_span = self.ast.expr_span(*stream);
            let (stream_value, stream_ty) = self.expr(*stream)?;
            self.expect_type(&stdio, &stream_ty, stream_span)?;
            lowered.push(stream_value);
        }

        if under {
            // §4.14: the program is one the directory holds, so the row names no path.
            self.performed.union(&Effects::plain(["exec_under"]));
        } else {
            self.performed.union(&Effects::new([Label {
                name: "exec".to_owned(),
                argument: Some(prefix.clone()),
            }]));
        }
        // §4.11: a child that outlives the program is more than `exec` says.
        if detached {
            self.performed.union(&Effects::plain(["exec_detached"]));
        }
        Ok((
            Expr::ExecSpawn { prefix, in_dir, detached, under, args: lowered },
            Type::Named(self.prelude()[PRELUDE_SPAWNED], Vec::new()),
        ))
    }

    /// The prefix a borrowed `Exec` was narrowed to.
    pub(crate) fn granted_exec_prefix(
        &mut self,
        found: &Type,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let resolved = self.unifier.resolve(found);
        if let Type::Ref { inner, .. } = &resolved
            && let Type::Named(def, args) = self.unifier.resolve(inner)
            && def.0 as usize == PRELUDE_EXEC
            && let Some(Type::Lit(prefix)) = args.first()
        {
            return Ok(prefix.clone());
        }
        Err(Diagnostic::new(
            Rule::CapabilityMisused,
            format!(
                "`{}` is not a borrowed `Exec`; a program is started through the capability that names which programs may be",
                self.unifier.display(&resolved)
            ),
            span,
        ))
    }

    /// `exec_covers(exec, path)` and `fs_covers(fs, path)` (§4.13): whether the operations would take the path
    /// under the capability's bound. A borrow of the capability and of the bytes, a `bool`, and **no row**: nothing
    /// is touched, which is the point (the report of a program that asks first is the report of one that does not).
    pub(crate) fn path_covers(
        &mut self,
        op: Builtin,
        args: &[ExprId],
        span: Span,
    ) -> Result<(Expr, Type), Diagnostic> {
        let [capability, path] = args else {
            return Err(Diagnostic::new(
                Rule::ArityMismatch,
                format!(
                    "`{}` takes 2 arguments -- the borrowed capability and the path -- but {} were given",
                    op.name(),
                    args.len()
                ),
                span,
            ));
        };
        let exec = op == Builtin::ExecCovers;
        let capability_span = self.ast.expr_span(*capability);
        let (value, found) = self.expr(*capability)?;
        let prefix = if exec {
            self.granted_exec_prefix(&found, capability_span)?
        } else {
            self.granted_prefix(&found, capability_span)?
        };
        let bytes = Type::Ref {
            unique: false,
            region: self.unifier.fresh_region(),
            inner: Box::new(Type::Slice(Box::new(Type::Byte))),
        };
        let path_span = self.ast.expr_span(*path);
        let (path_value, path_ty) = self.expr(*path)?;
        self.expect_type(&bytes, &path_ty, path_span)?;
        Ok((Expr::PathCovers { prefix, exec, args: vec![value, path_value] }, Type::Bool))
    }
}
