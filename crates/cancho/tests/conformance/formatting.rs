//! `cancho fmt` (`docs/formatting.md`): the formatter against every
//! program this repository tracks, and the command around it.

use super::*;

/// What the formatter is asked to handle: every program that is meant to be a program (`benches/` too, which the older sweep left out
/// because most of it is not a program the other sweeps are about). `tests/reject` is excluded -- most of it does not parse, on purpose.
const CORPUS: [&str; 6] =
    ["std", "examples", "packages", "tests/accept", "tests/programs", "benches"];

/// `cancho fmt` over the whole corpus, through the command and nothing else (these tests named the Rust crate's `format` before, so the cancho-written
/// command could not be run against them): `fmt --check` exits 0 and writes nothing only if no file is refused (a layout the formatter cannot
/// reproduce, a file that does not parse) and every file is already canonical, which is what `cancho fmt` was applied to every tracked program
/// to make true (`docs/formatting.md` section 5).
#[test]
fn every_tracked_program_formats_and_is_formatted() {
    let root = repo_root();
    let out = Command::new(BIN)
        .current_dir(&root)
        .args(["fmt", "--check"])
        .args(CORPUS)
        .output()
        .expect("the compiler runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "not canonical or refused -- run `cancho fmt` on them:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn formatting_never_panics_on_a_damaged_program() {
    // `CLAUDE.md`: no input may reach a panic. Delete each line of a few
    // real files, one at a time, and format what is left: most of those
    // do not parse, some do and are a different program, and every one
    // must come back with an answer, 0 or 1, never a crash.
    let root = repo_root();
    let dir = scratch("fmt-damaged");
    let file = dir.join("damaged.cho");
    for name in ["tests/accept/vec_shrink.cho", "std/option.cho", "examples/cut/cut.cho"] {
        let text = std::fs::read_to_string(root.join(name)).expect("a readable source file");
        let lines: Vec<&str> = text.lines().collect();
        for skip in 0..lines.len() {
            let damaged: String = lines
                .iter()
                .enumerate()
                .filter(|(n, _)| *n != skip)
                .map(|(_, l)| format!("{l}\n"))
                .collect();
            std::fs::write(&file, damaged).unwrap();
            let out = Command::new(BIN).args(["fmt", "--check"]).arg(&file).output().unwrap();
            assert!(
                matches!(out.status.code(), Some(0 | 1)),
                "{name} without line {}: {:?}\n{}",
                skip + 1,
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn already_formatted_files_are_a_fixed_point_of_the_command() {
    let dir = scratch("fmt-fixed-point");
    let file = dir.join("a.cho");
    std::fs::write(&file, "fn f() -> [] int {\n    // c\n    return 0; // d\n}\n").unwrap();
    let out = Command::new(BIN).args(["fmt", "--check"]).arg(&file).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_reports_and_writes_nothing_and_fmt_rewrites() {
    let dir = scratch("fmt-check-write");
    let file = dir.join("a.cho");
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
    std::fs::create_dir_all(dir.join(".cancho-vcs")).unwrap();
    let messy = "fn f() -> [] int { return 0; }\n";
    std::fs::write(dir.join("sub").join("deep.cho"), messy).unwrap();
    std::fs::write(dir.join(".cancho-vcs").join("blob.cho"), messy).unwrap();
    std::fs::write(dir.join("notes.txt"), "not a program").unwrap();

    let out = Command::new(BIN).args(["fmt", "--check"]).arg(&dir).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("deep.cho"), "{text}");
    assert!(!text.contains("blob.cho"), "{text}");
    assert!(!text.contains("notes.txt"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_it_cannot_format_is_left_untouched_and_the_rest_are_still_done() {
    let dir = scratch("fmt-refuse");
    let late_import = "fn f() -> [] int {\n    return 0;\n}\nimport std.io;\n";
    let fine = "fn g() -> [] int { return 0; }\n";
    std::fs::write(dir.join("a_late.cho"), late_import).unwrap();
    std::fs::write(dir.join("b_fine.cho"), fine).unwrap();

    let out = Command::new(BIN).arg("fmt").arg(&dir).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("a_late.cho: not formatted"));
    assert_eq!(std::fs::read_to_string(dir.join("a_late.cho")).unwrap(), late_import);
    assert_eq!(
        std::fs::read_to_string(dir.join("b_fine.cho")).unwrap(),
        "fn g() -> [] int {\n    return 0;\n}\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_does_not_parse_is_reported_with_its_position() {
    let dir = scratch("fmt-parse-error");
    let file = dir.join("bad.cho");
    std::fs::write(&file, "fn f( {\n").unwrap();
    let out = Command::new(BIN).arg("fmt").arg(&file).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("bad.cho:1:"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn printing_a_comment_free_program_and_formatting_it_agree() {
    // `print` is the layout; `fmt` is the layout plus what `print` cannot
    // know. For a file with none of the latter -- no comments, no blank
    // lines, no literal that `print` respells -- the two must be the same
    // text, or one of them has a second opinion about layout.
    let dir = scratch("fmt-agrees-with-print");
    let file = dir.join("a.cho");
    std::fs::write(
        &file,
        "fn f(a: int) -> [] int {\n    if a == 1 {\n        return 1;\n    } else if a == 2 {\n        return 2;\n    } else {\n        return 3;\n    }\n}\n",
    )
    .unwrap();
    let printed = Command::new(BIN).arg("print").arg(&file).output().unwrap();
    let formatted = dir.join("formatted.cho");
    std::fs::copy(&file, &formatted).unwrap();
    let wrote = Command::new(BIN).arg("fmt").arg(&formatted).output().unwrap();
    assert_eq!(wrote.status.code(), Some(0), "{}", String::from_utf8_lossy(&wrote.stderr));
    assert_eq!(
        String::from_utf8_lossy(&printed.stdout),
        std::fs::read_to_string(&formatted).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
