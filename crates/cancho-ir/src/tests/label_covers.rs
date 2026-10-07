//! `Label::covers` for the labels that name a path or a program
//! (`docs/filesystem.md` §1.1, `docs/narrowing-into-several.md` §6): a path
//! prefix is not a byte prefix, and the rule is `extends_path`, the one
//! `narrow` already uses.

use super::tests::lower_src;
use crate::{Label, extends_path};

fn label(name: &str, argument: &str) -> Label {
    Label { name: name.to_owned(), argument: Some(argument.to_owned()) }
}

const PATH_LABELS: [&str; 3] = ["fs_read", "fs_write", "exec"];

#[test]
fn a_directory_covers_itself_and_what_is_inside_it() {
    for name in PATH_LABELS {
        let tmp = label(name, "/tmp");
        assert!(tmp.covers(&label(name, "/tmp")), "{name}: equal");
        assert!(tmp.covers(&label(name, "/tmp/x")), "{name}: child");
        assert!(tmp.covers(&label(name, "/tmp/x/y")), "{name}: grandchild");
    }
}

#[test]
fn a_sibling_that_shares_the_bytes_is_not_covered() {
    for name in PATH_LABELS {
        let tmp = label(name, "/tmp");
        assert!(!tmp.covers(&label(name, "/tmpevil")), "{name}");
        assert!(!tmp.covers(&label(name, "/tmp2/x")), "{name}");
        assert!(!tmp.covers(&label(name, "/tm")), "{name}: a parent is not covered");
        assert!(!label(name, "/tmp/x").covers(&label(name, "/tmp")), "{name}: never widens");
        assert!(!label(name, "/tmp/x").covers(&label(name, "/tmp/xy")), "{name}");
    }
}

#[test]
fn a_trailing_slash_names_what_is_beneath_it() {
    // `extends_path`: a prefix that already ends at a separator needs none
    // after it. It does not cover the directory without the slash.
    for name in PATH_LABELS {
        let dir = label(name, "/tmp/");
        assert!(dir.covers(&label(name, "/tmp/")), "{name}");
        assert!(dir.covers(&label(name, "/tmp/x")), "{name}");
        assert!(!dir.covers(&label(name, "/tmp")), "{name}");
        assert!(!dir.covers(&label(name, "/tmpevil")), "{name}");
        // and `/tmp` covers `/tmp/`, which is inside it
        assert!(label(name, "/tmp").covers(&dir), "{name}");
    }
}

#[test]
fn the_empty_path_covers_every_path() {
    for name in PATH_LABELS {
        let root = label(name, "");
        assert!(root.covers(&label(name, "")), "{name}");
        assert!(root.covers(&label(name, "/")), "{name}");
        assert!(root.covers(&label(name, "/tmpevil")), "{name}");
        assert!(root.covers(&label(name, "relative/x")), "{name}");
        assert!(!label(name, "/tmp").covers(&root), "{name}: a directory does not cover the root");
    }
}

#[test]
fn dot_and_dot_dot_are_not_interpreted() {
    // `extends_path` is lexical on literals, and `covers` is exactly it: the
    // checker normalises nothing, so `/tmp/..` is a path *inside* `/tmp` by
    // the text, as it is for `narrow`. The run-time check refuses `..`
    // (`docs/filesystem.md` §4); a label does not claim more than its text.
    let tmp = label("fs_read", "/tmp");
    assert!(tmp.covers(&label("fs_read", "/tmp/..")));
    assert!(tmp.covers(&label("fs_read", "/tmp/./x")));
    assert!(!tmp.covers(&label("fs_read", "/tmp.")));
    assert!(!tmp.covers(&label("fs_read", "/tmp..")));
    assert!(!label("fs_read", "/tmp/.").covers(&label("fs_read", "/tmp/.hidden")));
}

#[test]
fn covers_is_extends_path_for_every_pair() {
    let paths = [
        "", "/", "/tmp", "/tmp/", "/tmp/x", "/tmp/x/", "/tmp/x/y", "/tmpevil", "/tmp.", "/tmp/..",
        "tmp", "a/b", "a/b/c",
    ];
    for name in PATH_LABELS {
        for held in paths {
            for wanted in paths {
                assert_eq!(
                    label(name, held).covers(&label(name, wanted)),
                    extends_path(held, wanted),
                    "{name}: `{held}` against `{wanted}`"
                );
            }
        }
    }
}

#[test]
fn a_path_label_never_covers_another_name_or_a_bare_label() {
    assert!(!label("fs_read", "/tmp").covers(&label("fs_write", "/tmp/x")));
    assert!(!label("fs_read", "/tmp").covers(&label("exec", "/tmp/x")));
    let bare = Label { name: "fs_read".to_owned(), argument: None };
    assert!(!label("fs_read", "").covers(&bare));
    assert!(!bare.covers(&label("fs_read", "/tmp")));
    assert!(bare.covers(&bare));
}

#[test]
fn net_bounds_keep_their_textual_prefix() {
    // `docs/net.md` §4: a `Net` bound is `"host:port"`, narrowed by plain
    // text, and `narrow` accepts exactly that. Not a path, so `covers` is
    // unchanged for `net_out`/`net_in`.
    for name in ["net_out", "net_in"] {
        assert!(label(name, "").covers(&label(name, "example.com:443")));
        assert!(label(name, "example.").covers(&label(name, "example.com:443")));
        assert!(label(name, "example.com:443").covers(&label(name, "example.com:443")));
    }
}

#[test]
fn a_function_owning_an_fs_does_not_hide_the_directory_next_door() {
    let source = "
        fn evil[&e, &o](x: &e Fs(\"/tmpevil\"), out: &!o [byte]) -> [fs_read(\"/tmpevil\")] int {
            return fs_read(x, \"/tmpevil/a\", out);
        }
        fn owner[&e, &o](mine: Fs(\"/tmp\"), x: &e Fs(\"/tmpevil\"), out: &!o [byte]) -> [] int {
            release(mine);
            return evil(x, out);
        }
        fn main(world: World) -> [] int { release(world); return 0; }
    ";
    let refusal = lower_src(source).expect_err("owner's row hides fs_read(\"/tmpevil\")");
    assert_eq!(refusal.rule.tag(), "effect-not-declared", "{}", refusal.message);
    assert!(refusal.message.contains("`owner`"), "{}", refusal.message);
    assert!(refusal.message.contains("fs_read(\"/tmpevil\")"), "{}", refusal.message);
}

#[test]
fn a_function_owning_an_fs_still_discharges_what_is_inside_it() {
    let source = "
        fn inside[&e, &o](mine: Fs(\"/tmp\"), x: &e Fs(\"/tmp/app\"), out: &!o [byte]) -> [] int {
            release(mine);
            return fs_read(x, \"/tmp/app/a\", out);
        }
        fn main(world: World) -> [] int { release(world); return 0; }
    ";
    lower_src(source).unwrap_or_else(|d| panic!("{}", d.message));
}
