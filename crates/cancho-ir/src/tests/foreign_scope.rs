//! The scope of an `Ffi` is a set of libraries (`docs/foreign-authority.md`
//! section 4): parsed once, canonical, and covered as a set.

use super::*;
use crate::Label;

fn ffi(scope: &str) -> Label {
    Label { name: "ffi".to_owned(), argument: Some(scope.to_owned()) }
}

#[test]
fn a_scope_is_canonical_whatever_order_it_was_written_in() {
    assert_eq!(parse_scope("libssl,libc,libcrypto").unwrap(), "libc,libcrypto,libssl");
    assert_eq!(parse_scope("libc").unwrap(), "libc");
}

#[test]
fn the_root_parses_as_the_empty_scope() {
    assert_eq!(parse_scope("").unwrap(), "");
}

#[test]
fn every_malformed_scope_says_why() {
    let cases: &[(&str, &str)] = &[
        ("libc,", "empty library name"),
        (",libc", "empty library name"),
        ("libc,,libm", "empty library name"),
        ("libc,libc", "named twice"),
        ("libc libm", "not a library name"),
        ("lib/c", "not a library name"),
        ("libc:statx", "not a library name"),
        ("libc\"", "not a library name"),
    ];
    for (text, why) in cases {
        let message = parse_scope(text).expect_err(text);
        assert!(message.contains(why), "`{text}`: {message}");
    }
}

#[test]
fn the_characters_of_real_library_names_are_allowed() {
    for name in ["libc", "libstdc++", "libgcc_s", "libz-ng", "libssl.so.3", "tls", "A1"] {
        assert_eq!(parse_scope(name).as_deref(), Ok(name));
    }
}

#[test]
fn a_set_covers_its_subsets_and_nothing_else() {
    assert!(scope_covers("libc,libssl", "libc"));
    assert!(scope_covers("libc,libssl", "libssl,libc"));
    assert!(scope_covers("libc", "libc"));
    assert!(!scope_covers("libc", "libc,libssl"));
    assert!(!scope_covers("libc", "libm"));
}

#[test]
fn a_prefix_of_the_text_is_not_a_cover() {
    // The reason this is a set: `Ffi("libc")` used to cover `Ffi("libcrypto")`.
    assert!(!scope_covers("libc", "libcrypto"));
    assert!(!scope_covers("libcrypto", "libc"));
    assert!(!ffi("libc").covers(&ffi("libcrypto")));
    assert!(!ffi("libc").covers(&ffi("libc,libcrypto")));
}

#[test]
fn the_root_covers_every_scope_and_nothing_covers_the_root() {
    assert!(scope_covers("", "libc"));
    assert!(scope_covers("", "libc,libssl"));
    assert!(scope_covers("", ""));
    assert!(!scope_covers("libc", ""));
}

#[test]
fn a_malformed_scope_covers_and_is_covered_by_nothing() {
    assert!(!scope_covers("libc,,libm", "libc"));
    assert!(!scope_covers("libc", "libc,,libm"));
}

#[test]
fn the_label_follows_the_scope() {
    assert!(ffi("libc,libssl").covers(&ffi("libssl")));
    assert!(ffi("").covers(&ffi("libssl")));
    assert!(!ffi("libssl").covers(&ffi("libc")));
    // The set rule is `ffi`'s: another label with an argument stays a prefix.
    let fs = |p: &str| Label { name: "fs_read".to_owned(), argument: Some(p.to_owned()) };
    assert!(fs("/tmp").covers(&fs("/tmp/x")));
}

#[test]
fn a_declaration_names_one_library() {
    assert!(is_single_library("libc"));
    assert!(!is_single_library("libc,libm"));
    assert!(!is_single_library(""));
}
