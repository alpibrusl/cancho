//! What a program's foreign reach is attributed to, for `lex-sys authority`
//! (`docs/foreign-authority.md` section 5).
//!
//! The report used to say one thing about foreign code: `bounded: false`. It
//! now also says **which symbols** make it so, as `scope:symbol` pairs, so a
//! reader (or a CI pin) sees exactly what the unboundedness is made of, and a
//! diff when one is added. `bounded` keeps its meaning -- the labels bound
//! the program -- and is `false` whenever any foreign symbol is reachable.

/// A reachable foreign symbol and the scope its declaration claims.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ForeignReach {
    pub(crate) scope: String,
    pub(crate) symbol: String,
}

/// The `unbounded_by` entry for "foreign reach the symbol list does not
/// account for". No program the checker accepts produces it: every foreign
/// transfer of control is a call to a declared symbol. It exists so that if
/// that ever stops being true the report fails closed instead of calling an
/// unaccounted-for program bounded.
pub(crate) const UNLISTED: &str = "*";

/// The attribution: whether the program is bounded, and what it is not
/// bounded by.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Attribution {
    pub(crate) bounded: bool,
    /// `scope:symbol`, sorted, each once; `[UNLISTED]` when the program
    /// performs `ffi` yet no reachable call accounts for it.
    pub(crate) unbounded_by: Vec<String>,
}

/// Attribute a program's foreign reach.
///
/// `ffi_performed` is whether any reachable function performs an `ffi`
/// label; `reach` is every reachable extern. Either one makes the program
/// unbounded; the symbols are the explanation when there are any.
pub(crate) fn attribute(ffi_performed: bool, reach: &[ForeignReach]) -> Attribution {
    let mut pairs: Vec<String> =
        reach.iter().map(|r| format!("{}:{}", r.scope, r.symbol)).collect();
    pairs.sort_unstable();
    pairs.dedup();
    if pairs.is_empty() && ffi_performed {
        pairs.push(UNLISTED.to_owned());
    }
    Attribution { bounded: pairs.is_empty(), unbounded_by: pairs }
}

#[cfg(test)]
#[path = "tests/foreign_report.rs"]
mod tests;
