//! `lex-sys fmt` (`docs/formatting.md`): the formatter against every
//! program this repository tracks, and the command around it.

use super::*;
use lex_sys_syntax::FormatError;

/// What the formatter is asked to handle: every program that is meant to
/// be a program. `tests/reject` is excluded -- most of it does not parse,
/// on purpose.
fn corpus() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in ["std", "examples", "packages", "tests/accept"] {
        super::duplication::walk_ls_files(&root.join(dir), &mut files);
    }
    files.sort();
    assert!(files.len() > 100, "the corpus walk found only {} files", files.len());
    files
}

#[test]
fn every_tracked_program_formats() {
    // Not "does not crash": `format` returns text only if that text parses
    // to the same program, keeps every comment, and is a fixed point
    // (`format.rs`'s `verify`), so an `Ok` here is all three. What is
    // left to assert is that nothing is *refused* -- a refusal is the
    // formatter admitting a layout it cannot reproduce, and a tracked
    // program that does that is either a formatter gap to close or a file
    // to tidy (`std/ed25519.ls` had an import in the middle).
    let mut refused = Vec::new();
    for path in corpus() {
        let text = std::fs::read_to_string(&path).expect("a readable source file");
        match lex_sys_syntax::format(&text) {
            Ok(_) => {}
            Err(FormatError::Parse(d)) => {
                refused.push(format!("{}: {}", path.display(), d.message))
            }
            Err(FormatError::Cannot(why)) => refused.push(format!("{}: {why}", path.display())),
        }
    }
    assert!(refused.is_empty(), "files the formatter refused:\n{}", refused.join("\n"));
}

#[test]
fn formatting_never_panics_on_a_damaged_program() {
    // `CLAUDE.md`: no input may reach a panic. Delete each line of a few
    // real files, one at a time, and format what is left: most of those
    // do not parse, some do and are a different program, and every one
    // must come back as `Ok` or an error.
    let root = repo_root();
    for name in ["tests/accept/vec_shrink.ls", "std/option.ls", "examples/cut/cut.ls"] {
        let text = std::fs::read_to_string(root.join(name)).expect("a readable source file");
        let lines: Vec<&str> = text.lines().collect();
        for skip in 0..lines.len() {
            let damaged: String = lines
                .iter()
                .enumerate()
                .filter(|(n, _)| *n != skip)
                .map(|(_, l)| format!("{l}\n"))
                .collect();
            let _ = lex_sys_syntax::format(&damaged);
        }
    }
}

#[test]
fn already_formatted_files_are_a_fixed_point_of_the_command() {
    let dir = scratch("fmt-fixed-point");
    let file = dir.join("a.ls");
    std::fs::write(&file, "fn f() -> [] int {\n    // c\n    return 0; // d\n}\n").unwrap();
    let out = Command::new(BIN).args(["fmt", "--check"]).arg(&file).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_reports_and_writes_nothing_and_fmt_rewrites() {
    let dir = scratch("fmt-check-write");
    let file = dir.join("a.ls");
    let messy = "fn f() -> [] int {   \n    return 0;// c\n}\n";
    std::fs::write(&file, messy).unwrap();

    let check = Command::new(BIN).args(["fmt", "--check"]).arg(&file).output().unwrap();
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("would reformat"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), messy, "--check must not write");

    let write = Command::new(BIN).arg("fmt").arg(&file).output().unwrap();
    assert_eq!(write.status.code(), Some(0), "{}", String::from_utf8_lossy(&write.stderr));
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "fn f() -> [] int {\n    return 0; // c\n}\n"
    );

    let again = Command::new(BIN).args(["fmt", "--check"]).arg(&file).output().unwrap();
    assert_eq!(again.status.code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_directory_is_walked_and_hidden_directories_are_not() {
    let dir = scratch("fmt-walk");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::create_dir_all(dir.join(".lex-sys-vcs")).unwrap();
    let messy = "fn f() -> [] int { return 0; }\n";
    std::fs::write(dir.join("sub").join("deep.ls"), messy).unwrap();
    std::fs::write(dir.join(".lex-sys-vcs").join("blob.ls"), messy).unwrap();
    std::fs::write(dir.join("notes.txt"), "not a program").unwrap();

    let out = Command::new(BIN).args(["fmt", "--check"]).arg(&dir).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("deep.ls"), "{text}");
    assert!(!text.contains("blob.ls"), "{text}");
    assert!(!text.contains("notes.txt"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_it_cannot_format_is_left_untouched_and_the_rest_are_still_done() {
    let dir = scratch("fmt-refuse");
    let late_import = "fn f() -> [] int {\n    return 0;\n}\nimport std.io;\n";
    let fine = "fn g() -> [] int { return 0; }\n";
    std::fs::write(dir.join("a_late.ls"), late_import).unwrap();
    std::fs::write(dir.join("b_fine.ls"), fine).unwrap();

    let out = Command::new(BIN).arg("fmt").arg(&dir).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("a_late.ls: not formatted"));
    assert_eq!(std::fs::read_to_string(dir.join("a_late.ls")).unwrap(), late_import);
    assert_eq!(
        std::fs::read_to_string(dir.join("b_fine.ls")).unwrap(),
        "fn g() -> [] int {\n    return 0;\n}\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_does_not_parse_is_reported_with_its_position() {
    let dir = scratch("fmt-parse-error");
    let file = dir.join("bad.ls");
    std::fs::write(&file, "fn f( {\n").unwrap();
    let out = Command::new(BIN).arg("fmt").arg(&file).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("bad.ls:1:"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn printing_a_comment_free_program_and_formatting_it_agree() {
    // `print` is the layout; `fmt` is the layout plus what `print` cannot
    // know. For a file with none of the latter -- no comments, no blank
    // lines, no literal that `print` respells -- the two must be the same
    // text, or one of them has a second opinion about layout.
    let dir = scratch("fmt-agrees-with-print");
    let file = dir.join("a.ls");
    std::fs::write(
        &file,
        "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else if a == 2 {\n        return 2;\n    } else {\n        return 3;\n    }\n}\n",
    )
    .unwrap();
    let printed = Command::new(BIN).arg("print").arg(&file).output().unwrap();
    let formatted = lex_sys_syntax::format(&std::fs::read_to_string(&file).unwrap()).ok().unwrap();
    assert_eq!(String::from_utf8_lossy(&printed.stdout), formatted);
    let _ = std::fs::remove_dir_all(&dir);
}
