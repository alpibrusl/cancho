//! `exec_spawn` (`docs/processes.md` §3.2): the one process builtin whose row
//! is read off a type.

use crate::*;

impl<'a> FnLowering<'a> {
    /// `exec_spawn(&Exec(p), path, args, env, stdin, stdout, stderr)` -- start
    /// the program at `path`. Checked here rather than through a written
    /// signature because the row it performs is the prefix the capability was
    /// narrowed to, exactly the reason [`Self::open_file`] is checked here.
    pub(crate) fn exec_spawn(
        &mut self,
        args: &[ExprId],
        span: Span,
    ) -> Result<(Expr, Type), Diagnostic> {
        let [capability, path, arguments, environment, stdin, stdout, stderr] = args else {
            return Err(Diagnostic::new(
                Rule::ArityMismatch,
                format!(
                    "`exec_spawn` takes 7 arguments -- the borrowed capability, the path, the arguments, the environment and the three streams -- but {} were given",
                    args.len()
                ),
                span,
            ));
        };
        let capability_span = self.ast.expr_span(*capability);
        let (value, found) = self.expr(*capability)?;
        let resolved = self.unifier.resolve(&found);
        let prefix = match &resolved {
            Type::Ref { inner, .. } => match self.unifier.resolve(inner) {
                Type::Named(def, args) if def.0 as usize == PRELUDE_EXEC => match args.first() {
                    Some(Type::Lit(prefix)) => Some(prefix.clone()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        let Some(prefix) = prefix else {
            return Err(Diagnostic::new(
                Rule::CapabilityMisused,
                format!(
                    "`{}` is not a borrowed `Exec`; a program is started through the capability that names which programs may be",
                    self.unifier.display(&resolved)
                ),
                capability_span,
            ));
        };

        // The path, the arguments and the environment: three byte slices,
        // read and never written.
        let mut lowered = vec![value];
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

        self.performed.union(&Effects::new([Label {
            name: "exec".to_owned(),
            argument: Some(prefix.clone()),
        }]));
        Ok((
            Expr::ExecSpawn { prefix, args: lowered },
            Type::Named(self.prelude()[PRELUDE_SPAWNED], Vec::new()),
        ))
    }
}
