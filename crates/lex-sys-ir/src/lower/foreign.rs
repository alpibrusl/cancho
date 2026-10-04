//! `narrow` for an `Ffi` (`docs/foreign-authority.md` section 4): the scope
//! checked as a set of libraries, the way `narrow_signals` checks signals.

use crate::{Diagnostic, FnLowering, PRELUDE_FFI, Rule, Span, Type, parse_scope, scope_covers};

impl<'a> FnLowering<'a> {
    /// `narrow(ffi, "...")`, called from [`Self::narrow`] with the scope the
    /// capability holds and the literal. The answer is the canonical spelling.
    ///
    /// Every name must be a library name (`foreign-scope`), and the result
    /// must be strictly inside what the capability holds -- the empty string
    /// is the root and holds every library -- so a program cannot grant itself
    /// a library it was not given. This used to be a text prefix test, which
    /// let `Ffi("libc")` narrow to `Ffi("libcrypto")`.
    pub(crate) fn narrow_ffi(
        &self,
        current: &str,
        target: &str,
        span: Span,
    ) -> Result<String, Diagnostic> {
        let wanted =
            parse_scope(target).map_err(|why| Diagnostic::new(Rule::ForeignScope, why, span))?;
        if !scope_covers(current, &wanted) {
            return Err(Diagnostic::new(
                Rule::CapabilityNotNarrowable,
                format!(
                    "`{current}` cannot be narrowed to `{wanted}`: a capability is attenuated, never widened, and a program must not be able to grant itself a library it was not given"
                ),
                span,
            ));
        }
        if wanted == current {
            return Err(Diagnostic::new(
                Rule::CapabilityNotNarrowable,
                format!("this narrows `{current}` to itself, which grants nothing new"),
                span,
            ));
        }
        Ok(wanted)
    }

    /// Is `got` an `Ffi` over a set of libraries that strictly contains the
    /// set `want` names? Then a reference to the first may stand where a
    /// reference to the second is wanted: the callee can reach fewer
    /// libraries than the caller holds, never more.
    pub(crate) fn ffi_scope_attenuates(&self, got: &Type, want: &Type) -> bool {
        let (got, want) = (self.unifier.resolve(got), self.unifier.resolve(want));
        let (Type::Named(got_def, got_args), Type::Named(want_def, want_args)) = (&got, &want)
        else {
            return false;
        };
        if got_def.0 as usize != PRELUDE_FFI || want_def.0 as usize != PRELUDE_FFI {
            return false;
        }
        match (got_args.first(), want_args.first()) {
            (Some(Type::Lit(held)), Some(Type::Lit(wanted))) => {
                // The root is excluded: it covers every library, but a
                // program that holds it must `narrow` before it calls out,
                // which is where its libraries are written down.
                !held.is_empty() && held != wanted && scope_covers(held, wanted)
            }
            _ => false,
        }
    }
}
