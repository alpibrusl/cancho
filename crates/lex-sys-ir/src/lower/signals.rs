//! `signals_watch` (`docs/signals.md` section 2): the one signal builtin whose
//! row is read off a type, and the set check `narrow` shares.

use crate::*;

impl<'a> FnLowering<'a> {
    /// `signals_watch(&Signals("S"))` -- claim the signals the capability was
    /// narrowed to. Checked here rather than through a written signature
    /// because the row it performs is that set, exactly the reason
    /// [`Self::tcp_listen`] is checked here and not there. The set travels to
    /// the backend as a second argument, the bits of `S`, so the backends
    /// need no new node.
    pub(crate) fn signals_watch(
        &mut self,
        args: &[ExprId],
        span: Span,
    ) -> Result<(Expr, Type), Diagnostic> {
        let [capability] = args else {
            return Err(Diagnostic::new(
                Rule::ArityMismatch,
                format!(
                    "`signals_watch` takes 1 argument -- the borrowed capability -- but {} were given",
                    args.len()
                ),
                span,
            ));
        };
        let capability_span = self.ast.expr_span(*capability);
        let (value, found) = self.expr(*capability)?;
        let resolved = self.unifier.resolve(&found);
        let set = match &resolved {
            Type::Ref { inner, .. } => match self.unifier.resolve(inner) {
                Type::Named(def, args) if def.0 as usize == PRELUDE_SIGNALS => match args.first() {
                    Some(Type::Lit(set)) => Some(set.clone()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        let Some(set) = set else {
            return Err(Diagnostic::new(
                Rule::CapabilityMisused,
                format!(
                    "`{}` is not a borrowed `Signals`; signals are claimed through the capability that names which ones",
                    self.unifier.display(&resolved)
                ),
                capability_span,
            ));
        };
        if set.is_empty() {
            return Err(Diagnostic::new(
                Rule::CapabilityMisused,
                "this `Signals` is not narrowed to the signals it will claim; `narrow(signals, \"INT,TERM\")` first, so the authority report can say which"
                    .to_owned(),
                capability_span,
            ));
        }
        let parsed = parse_signal_set(&set)
            .map_err(|why| Diagnostic::new(Rule::SignalNotClaimable, why, capability_span))?;
        if parsed.canonical != set {
            return Err(Diagnostic::new(
                Rule::SignalNotClaimable,
                format!(
                    "the set `{set}` is not written canonically; a set is alphabetical, `{}`",
                    parsed.canonical
                ),
                capability_span,
            ));
        }

        self.performed.union(&Effects::new([Label {
            name: "signals".to_owned(),
            argument: Some(parsed.canonical),
        }]));

        Ok((
            Expr::Call {
                callee: Callee::Builtin(Builtin::SignalsWatch),
                args: vec![value, Expr::Int(parsed.bits)],
            },
            Type::Named(self.prelude()[PRELUDE_WATCHING], Vec::new()),
        ))
    }

    /// `narrow(signals, "...")`: the set checked as a set. Called from
    /// [`Self::narrow`] with the capability's current set and the literal.
    ///
    /// Every name must be claimable (`signal-not-claimable`), and the result
    /// must be strictly inside what the capability holds -- the empty
    /// string is the root and holds every claimable signal -- so a
    /// program cannot grant itself a signal it was not given.
    pub(crate) fn narrow_signals(
        &self,
        current: &str,
        target: &str,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let wanted = parse_signal_set(target)
            .map_err(|why| Diagnostic::new(Rule::SignalNotClaimable, why, span))?;
        let held_bits = if current.is_empty() {
            CLAIMABLE_SIGNALS.iter().fold(0, |bits, s| bits | s.bit)
        } else {
            parse_signal_set(current)
                .map_err(|why| Diagnostic::new(Rule::SignalNotClaimable, why, span))?
                .bits
        };
        if wanted.bits & !held_bits != 0 {
            return Err(Diagnostic::new(
                Rule::CapabilityNotNarrowable,
                format!(
                    "`{current}` cannot be narrowed to `{}`: a capability is attenuated, never widened, and a program must not be able to grant itself a signal it was not given",
                    wanted.canonical
                ),
                span,
            ));
        }
        if wanted.canonical == current {
            return Err(Diagnostic::new(
                Rule::CapabilityNotNarrowable,
                format!("this narrows `{current}` to itself, which grants nothing new"),
                span,
            ));
        }
        Ok(wanted.canonical)
    }
}
