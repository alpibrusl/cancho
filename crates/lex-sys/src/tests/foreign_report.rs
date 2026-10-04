use super::*;

fn reach(scope: &str, symbol: &str) -> ForeignReach {
    ForeignReach { scope: scope.to_owned(), symbol: symbol.to_owned() }
}

#[test]
fn no_foreign_reach_is_bounded() {
    assert_eq!(attribute(false, &[]), Attribution { bounded: true, unbounded_by: vec![] });
}

#[test]
fn a_symbol_makes_the_program_unbounded_and_is_named() {
    let found = attribute(true, &[reach("libc", "statx")]);
    assert!(!found.bounded);
    assert_eq!(found.unbounded_by, ["libc:statx"]);
}

#[test]
fn the_pairs_are_sorted_and_each_once() {
    let found = attribute(
        true,
        &[reach("openssl", "SSL_new"), reach("libc", "statx"), reach("libc", "statx")],
    );
    assert_eq!(found.unbounded_by, ["libc:statx", "openssl:SSL_new"]);
}

#[test]
fn the_same_symbol_under_two_scopes_is_two_entries() {
    let found = attribute(true, &[reach("a", "f"), reach("b", "f")]);
    assert_eq!(found.unbounded_by, ["a:f", "b:f"]);
}

#[test]
fn foreign_effects_with_no_accounted_symbol_fail_closed() {
    let found = attribute(true, &[]);
    assert!(!found.bounded);
    assert_eq!(found.unbounded_by, [UNLISTED]);
}

#[test]
fn a_reachable_symbol_is_unbounded_even_if_no_label_says_ffi() {
    let found = attribute(false, &[reach("libc", "getpid")]);
    assert!(!found.bounded);
    assert_eq!(found.unbounded_by, ["libc:getpid"]);
}
