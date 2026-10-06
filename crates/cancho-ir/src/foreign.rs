//! The libraries a foreign capability names (`docs/foreign-authority.md`
//! section 4), in one place so the narrowing, the label's cover test and the
//! type a program writes cannot disagree about what a scope is.
//!
//! `Ffi("libc,libssl")` is a **set** of library names: comma separated, each
//! once, in any order when written and alphabetical when answered, so two
//! programs that name one set have one type. The empty string is the root
//! `split` hands out, which holds every library there could be. It is the
//! shape `docs/signals.md` section 2.1 gave `Signals`, for the same reason: a
//! prefix test would say `Ffi("libc")` covers `Ffi("libcrypto")`.
//!
//! A scope is a **claim** the declaration makes, not a fact the compiler
//! checks (`docs/foreign-authority.md` section 3): nothing ties a library's
//! name to the symbols it defines. The fact is the symbol.

/// Parse a scope written as `"libc,libssl"`: the canonical spelling, or the
/// sentence the `foreign-scope` refusal carries. `""` is the root and parses.
pub fn parse_scope(text: &str) -> Result<String, String> {
    if text.is_empty() {
        return Ok(String::new());
    }
    let mut names: Vec<&str> = Vec::new();
    for name in text.split(',') {
        if name.is_empty() {
            return Err(format!(
                "`{text}` has an empty library name; a scope is names separated by one comma, \
                 such as `\"libc\"` or `\"libc,libssl\"`"
            ));
        }
        if let Some(bad) = name.chars().find(|c| !is_library_char(*c)) {
            return Err(format!(
                "`{name}` is not a library name: `{bad}` is not allowed. A library is named with \
                 letters, digits, `_`, `.`, `+` and `-`, as in `libc` or `libstdc++`"
            ));
        }
        if names.contains(&name) {
            return Err(format!(
                "`{name}` is named twice in `{text}`; a scope names each library once"
            ));
        }
        names.push(name);
    }
    names.sort_unstable();
    Ok(names.join(","))
}

fn is_library_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-')
}

/// Does holding the scope `held` authorise `wanted`? The root covers every
/// scope; any other covers exactly its subsets, and nothing covers the root
/// but the root. A malformed spelling covers nothing and is covered by
/// nothing: it was refused where it was written.
pub(crate) fn scope_covers(held: &str, wanted: &str) -> bool {
    if held.is_empty() {
        return true;
    }
    if wanted.is_empty() {
        return false;
    }
    let (Ok(held), Ok(wanted)) = (parse_scope(held), parse_scope(wanted)) else {
        return false;
    };
    let members: Vec<&str> = held.split(',').collect();
    wanted.split(',').all(|name| members.contains(&name))
}

/// Is this scope exactly one library? What a foreign *declaration* names:
/// a symbol lives in one library, and a set would make the report's
/// "scope:symbol" pair say less than the declaration did.
pub fn is_single_library(scope: &str) -> bool {
    !scope.is_empty() && !scope.contains(',')
}

#[cfg(test)]
#[path = "tests/foreign_scope.rs"]
mod tests;
