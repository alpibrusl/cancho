//! The signal table (`docs/signals.md` section 2.2): a set is checked once,
//! where `narrow` is written, and every refusal says why.

use super::*;
use crate::Label;

#[test]
fn a_set_is_canonical_whatever_order_it_was_written_in() {
    let set = parse_signal_set("TERM,INT").expect("a claimable set");
    assert_eq!(set.canonical, "INT,TERM");
    assert_eq!(set.bits, 2 | 8);
    assert_eq!(parse_signal_set("INT,TERM").unwrap(), set);
}

#[test]
fn every_claimable_signal_parses_alone_and_in_the_whole_set() {
    let mut all = Vec::new();
    for signal in CLAIMABLE_SIGNALS {
        let set = parse_signal_set(signal.name).expect("claimable");
        assert_eq!(set.bits, signal.bit, "{}", signal.name);
        assert_eq!(set.canonical, signal.name);
        all.push(signal.name);
    }
    let whole = parse_signal_set(&all.join(",")).expect("all eight");
    assert_eq!(whole.bits, 255);
    assert_eq!(whole.canonical, all.join(","));
}

#[test]
fn the_table_is_in_canonical_order_with_distinct_bits_and_numbers() {
    let names: Vec<&str> = CLAIMABLE_SIGNALS.iter().map(|s| s.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "the table is the canonical order");
    let mut bits = 0;
    for signal in CLAIMABLE_SIGNALS {
        assert_eq!(signal.bit.count_ones(), 1, "{}", signal.name);
        assert_eq!(bits & signal.bit, 0, "{} shares a bit", signal.name);
        bits |= signal.bit;
        // A native mask puts signal `n` in bit `n - 1` of a word the kernel
        // reads, and `signalfd`'s record and `sigprocmask` agree on 31.
        assert!((1..=31).contains(&signal.linux), "{}", signal.name);
        assert!((1..=31).contains(&signal.darwin), "{}", signal.name);
    }
    for darwin in [false, true] {
        let mut numbers: Vec<i64> =
            CLAIMABLE_SIGNALS.iter().map(|s| native_signal_number(s, darwin)).collect();
        numbers.sort_unstable();
        numbers.dedup();
        assert_eq!(
            numbers.len(),
            CLAIMABLE_SIGNALS.len(),
            "two signals share a number (darwin: {darwin})"
        );
    }
}

#[test]
fn the_native_mask_follows_each_kernels_numbers() {
    // TERM and INT are the same on both; USR1 is where they differ.
    assert_eq!(native_signal_mask(8 | 2, false), 1 << 14 | 1 << 1);
    assert_eq!(native_signal_mask(8 | 2, true), 1 << 14 | 1 << 1);
    assert_eq!(native_signal_mask(16, false), 1 << 9);
    assert_eq!(native_signal_mask(16, true), 1 << 29);
    assert_eq!(native_signal_mask(0, false), 0);
    assert_eq!(native_signal_mask(255, false).count_ones(), 8);
}

#[test]
fn the_signals_a_program_cannot_have_are_refused_with_their_reason() {
    for name in ["KILL", "STOP"] {
        let why = parse_signal_set(name).expect_err("uncatchable");
        assert!(why.contains("cannot be caught"), "{name}: {why}");
    }
    for name in ["SEGV", "ILL", "BUS", "FPE", "ABRT", "TRAP", "SYS"] {
        let why = parse_signal_set(name).expect_err("a fault");
        assert!(why.contains("a fault, not a request"), "{name}: {why}");
    }
    for name in ["PIPE", "CHLD", "CONT", "TSTP", "TTIN", "TTOU", "URG", "IO", "XCPU", "XFSZ"] {
        let why = parse_signal_set(name).expect_err("not yet");
        assert!(why.contains("not claimable (yet)"), "{name}: {why}");
    }
}

#[test]
fn a_malformed_set_is_refused() {
    for (text, fragment) in [
        ("", "at least one"),
        ("TERM,", "``"),
        (",TERM", "``"),
        ("TERM,TERM", "named twice"),
        ("INT, TERM", "` TERM`"),
        ("SIGTERM", "write `TERM`, not `SIGTERM`"),
        ("term", "`term`"),
        ("FOO", "`FOO`"),
        ("TERM;INT", "`TERM;INT`"),
    ] {
        let why = parse_signal_set(text).expect_err(text);
        assert!(why.contains(fragment), "{text:?}: {why}");
    }
}

#[test]
fn one_bad_name_refuses_the_whole_set() {
    let why = parse_signal_set("INT,KILL,TERM").expect_err("KILL is in it");
    assert!(why.contains("`KILL`"), "{why}");
}

#[test]
fn holding_a_set_covers_its_subsets_and_the_root_covers_all() {
    // A set, not a prefix: `INT` is a text prefix of `INT,TERM` and covers nothing of it.
    assert!(signal_set_covers("INT,TERM", "INT"));
    assert!(signal_set_covers("INT,TERM", "TERM"));
    assert!(signal_set_covers("INT,TERM", "INT,TERM"));
    assert!(!signal_set_covers("INT", "INT,TERM"));
    assert!(!signal_set_covers("INT,TERM", "HUP"));
    assert!(!signal_set_covers("INT,TERM", "HUP,INT"));
    // The root covers every set, and anything that is not a set is covered by no one.
    assert!(signal_set_covers("", "INT,TERM"));
    assert!(signal_set_covers("", &CLAIMABLE_SIGNALS.map(|s| s.name).join(",")));
    assert!(!signal_set_covers("INT", "KILL"));
    assert!(!signal_set_covers("KILL", "KILL"));
}

#[test]
fn a_label_covers_by_set_for_signals_and_by_prefix_for_the_rest() {
    let label = |name: &str, argument: &str| Label {
        name: name.to_owned(),
        argument: Some(argument.to_owned()),
    };
    assert!(label("signals", "INT,TERM").covers(&label("signals", "INT")));
    assert!(!label("signals", "INT").covers(&label("signals", "INT,TERM")));
    assert!(label("signals", "").covers(&label("signals", "HUP,TERM")));
    // Text prefixes still mean what they meant for every other label.
    assert!(label("net_out", "host").covers(&label("net_out", "host:80")));
    assert!(!label("signals", "INT").covers(&label("net_out", "INT")));
}
