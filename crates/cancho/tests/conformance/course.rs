//! `course/`: the tutorial is checked the way `AGENTS.md` is.
//!
//! A course that teaches something the compiler no longer does is worse
//! than none, and one whose planted examples stopped being wrong teaches
//! nothing while looking fine. So: every reference solution passes what its
//! own header asks, every planted fixture fails in the way its lesson says it
//! does, every starter fails until it is changed, and every code block in a
//! lesson compiles (or is refused with the rule it names).
//!
//! `course/check.py` is the course's own checker. The tests run it rather
//! than reimplement it, so a learner and CI read the same header the same
//! way.

use super::*;

fn course() -> PathBuf {
    repo_root().join("course")
}

/// Every `.cho` file under `dir`, sorted.
fn cho_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).expect("a readable directory") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "cho") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Run `course/check.py` on the given files: did it pass, and what it said.
fn check(files: &[&Path]) -> (bool, String) {
    let out = Command::new("python3")
        .arg("-I")
        .arg(course().join("check.py"))
        .args(files)
        .env("CANCHO", BIN)
        .output()
        .expect("python3 runs the course checker");
    let said =
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), said)
}

/// Contracts have no `main` and no candidate of their own: they are checked
/// by the candidates that name them in a `//~ SATISFIES` line.
fn is_contract(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".contract.cho"))
}

#[test]
fn every_solution_and_fixture_does_what_its_header_says() {
    let mut files = cho_files(&course().join("solutions"));
    files.extend(cho_files(&course().join("fixtures")));
    files.retain(|p| !is_contract(p));
    assert!(files.len() >= 15, "the walk found only {} course files", files.len());

    for file in &files {
        let (passed, said) = check(&[file]);
        assert!(passed, "{} does not do what its header says:\n{said}", file.display());
        // `ok` is the checker's word for a header it ran. A file with no
        // header at all would pass by saying nothing, so it must say it.
        assert!(
            said.contains("ok "),
            "{} passed without checking anything:\n{said}",
            file.display()
        );
    }
}

/// An exercise that already passes teaches nothing, and the only way to know
/// is to run it. These are the starters a learner is meant to change.
#[test]
fn every_starter_fails_until_it_is_changed() {
    let starters = [
        "exercises/01-hello/liftoff.cho",
        "exercises/02-numbers/numbers.cho",
        "exercises/03-own-it-once/laugh.cho",
        "exercises/04-authority/shrink.cho",
        "exercises/07-audit/hostname_len.cho",
    ];
    for starter in starters {
        let path = course().join(starter);
        let (passed, said) = check(&[&path]);
        assert!(!passed, "{starter} already passes, so it asks nothing of the learner:\n{said}");
    }
}

/// The starter contracts are the thin first drafts the lessons say they are:
/// satisfied by the planted wrong candidate (the header on each fixture says
/// so and `check.py` verifies it above), and refused by the solution's
/// stronger one. This asserts the pair exists for each Part B lesson.
#[test]
fn each_part_b_lesson_has_a_thin_contract_a_strong_one_and_a_planted_candidate() {
    let pairs = [
        ("05-contract", "count_words.contract.cho", "attempt.candidate.cho"),
        ("06-refusals", "parse_nat.contract.cho", "attempt_2.candidate.cho"),
        ("08-capstone", "logstats.contract.cho", "logstats.candidate.cho"),
    ];
    for (dir, contract, candidate) in pairs {
        let thin = course().join("exercises").join(dir).join(contract);
        let strong = course().join("solutions").join(dir).join(contract);
        let planted = course().join("fixtures").join(dir).join(candidate);
        for path in [&thin, &strong, &planted] {
            assert!(path.exists(), "{} is missing", path.display());
        }
        let thin_tests =
            std::fs::read_to_string(&thin).expect("readable").matches("fn test_").count();
        let strong_tests =
            std::fs::read_to_string(&strong).expect("readable").matches("fn test_").count();
        assert!(
            strong_tests > thin_tests,
            "{dir}: the strong contract has {strong_tests} tests and the thin one {thin_tests}"
        );
        let header = std::fs::read_to_string(&planted).expect("readable");
        assert!(header.contains("//~ SATISFIES") && header.contains("//~ UNSATISFIED"), "{dir}");
    }
}

/// Lesson 8's `main.cho` is given, and the planted candidate gets *past* it:
/// the sample log ends in a newline, so the bug is invisible there. That is
/// the claim the lesson makes, so it is the claim checked.
#[test]
fn the_capstone_program_runs_with_the_good_candidate_and_the_planted_one() {
    let dir = scratch("course-capstone");
    let main = course().join("exercises/08-capstone/main.cho");

    let alone = dir.join("alone");
    std::fs::create_dir_all(&alone).expect("a scratch directory");
    std::fs::copy(&main, alone.join("main.cho")).expect("main.cho copies");
    let (passed, said) = check(&[&alone.join("main.cho")]);
    assert!(!passed, "main.cho passes with no candidate beside it:\n{said}");

    for (tag, candidate) in [
        ("solution", "solutions/08-capstone/logstats.candidate.cho"),
        ("planted", "fixtures/08-capstone/logstats.candidate.cho"),
    ] {
        let with = dir.join(tag);
        std::fs::create_dir_all(&with).expect("a scratch directory");
        std::fs::copy(&main, with.join("main.cho")).expect("main.cho copies");
        std::fs::copy(course().join(candidate), with.join("logstats.candidate.cho"))
            .expect("the candidate copies");
        let (passed, said) = check(&[&with.join("main.cho")]);
        assert!(passed, "main.cho with the {tag} candidate fails:\n{said}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `start.py` copies a lesson out of the repository, and `--offline` hands
/// over the planted fixtures with their explanations stripped.
#[test]
fn start_copies_a_lesson_and_offline_strips_the_answer_key() {
    let dir = scratch("course-start").join("five");
    let run = |extra: &[&str]| {
        Command::new("python3")
            .arg("-I")
            .arg(course().join("start.py"))
            .args(["5", "--offline", "--dest"])
            .arg(&dir)
            .args(extra)
            .output()
            .expect("python3 runs start.py")
    };
    let first = run(&[]);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
    for name in
        ["brief.md", "count_words.contract.cho", "BUILDER.md", "offline/attempt.candidate.cho"]
    {
        assert!(dir.join(name).exists(), "start.py 5 left out {name}");
    }
    let stand_in =
        std::fs::read_to_string(dir.join("offline/attempt.candidate.cho")).expect("readable");
    assert!(!stand_in.contains("//"), "the stand-in still carries a comment:\n{stand_in}");
    assert!(!stand_in.contains("UNSATISFIED"), "the stand-in still carries the answer key");

    // It will not write over somebody's work.
    let second = run(&[]);
    assert!(!second.status.success(), "start.py overwrote a directory with files in it");
    let _ = std::fs::remove_dir_all(dir.parent().expect("a parent"));
}

/// Lesson code blocks are fixtures, exactly as `AGENTS.md`'s are: `cancho`
/// must compile, `cancho-refused` must be refused with the rule it names.
#[test]
fn the_lesson_code_blocks_compile_as_written() {
    let dir = scratch("course-lessons");
    let mut lessons: Vec<PathBuf> = std::fs::read_dir(course().join("lessons"))
        .expect("lessons/ is readable")
        .map(|e| e.expect("a readable entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    lessons.sort();
    assert_eq!(lessons.len(), 8, "the course has eight lessons: {lessons:?}");

    let mut checked = 0;
    for lesson in &lessons {
        let text = std::fs::read_to_string(lesson).expect("a readable lesson");
        let mut rest = text.as_str();
        while let Some(start) = rest.find("```cancho") {
            let after = &rest[start + 3..];
            let (refused, body_start) = match after.strip_prefix("cancho-refused\n") {
                Some(body) => (true, body),
                None => match after.strip_prefix("cancho\n") {
                    Some(body) => (false, body),
                    None => {
                        rest = &rest[start + 3..];
                        continue;
                    }
                },
            };
            let end = body_start.find("```").expect("an unterminated code block");
            let body = &body_start[..end];
            rest = &body_start[end..];

            let path = dir.join(format!("block{checked}.cho"));
            std::fs::write(&path, body).expect("the block is written");
            let out = Command::new(BIN)
                .arg("check")
                .arg(&path)
                .args(["--std", "--output", "json"])
                .output()
                .expect("the compiler runs");
            let json = String::from_utf8_lossy(&out.stdout);
            let reported = json
                .lines()
                .find_map(|line| line.trim().strip_prefix("\"rule\": \""))
                .map(|r| r.trim_end_matches("\","));
            if refused {
                let declared = body
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("//~ RULE "))
                    .map(str::trim)
                    .unwrap_or_else(|| {
                        panic!("{} has a refused block that names no rule", lesson.display())
                    });
                assert_eq!(reported, Some(declared), "{}:\n{body}\n{json}", lesson.display());
            } else {
                assert!(
                    out.status.success(),
                    "{} shows a block as valid cancho and it is not:\n{body}\n{json}",
                    lesson.display()
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} checked blocks in the lessons");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every relative link in the course's pages resolves. (The documentation
/// link test covers `README.md`, `AGENTS.md` and `docs/`, not this.)
#[test]
fn every_course_link_resolves() {
    let mut pages = vec![course().join("README.md"), course().join("trace-sheet.md")];
    pages.push(course().join("rubric.md"));
    pages.push(course().join("agent/builder.md"));
    for entry in std::fs::read_dir(course().join("lessons")).expect("lessons/ is readable") {
        pages.push(entry.expect("a readable entry").path());
    }
    pages.sort();

    let mut checked = 0;
    let mut broken = Vec::new();
    for page in &pages {
        let source = std::fs::read_to_string(page).expect("a readable page");
        let mut fenced = false;
        let mut prose = String::new();
        for line in source.lines() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
            } else if !fenced {
                prose.push_str(line);
                prose.push('\n');
            }
        }
        let mut rest = prose.as_str();
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            let Some(close) = rest.find(')') else { break };
            let target = &rest[..close];
            rest = &rest[close + 1..];
            let path = target.split('#').next().unwrap_or(target);
            if target.starts_with("http") || path.is_empty() || path.contains(char::is_whitespace) {
                continue;
            }
            checked += 1;
            if !page.parent().expect("a parent").join(path).exists() {
                broken.push(format!("{} -> {target}", page.display()));
            }
        }
    }
    assert!(checked >= 20, "only {checked} links found; the scan is broken, not the links");
    assert!(broken.is_empty(), "broken course link(s):\n{}", broken.join("\n"));
}
