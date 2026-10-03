//! `docs/package-system.md` §8: the project file, `add`, `install` and `build` with no files.
//!
//! The dependency is a local git repository (`vcs_remote::library_repo`), so nothing touches a network; each test has a
//! cache directory of its own.

use super::vcs_remote::{BASE, TOP, files_in, git, library_repo, ok, vcs, write};
use super::*;

const APP: &str = "\
import libx.top;
fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(io); release(ffi); release(fs); release(heap); release(args);
    return top.twice(2);
}
";

const PROJECT: &str = "\
[package]
name = \"app\"

[[bin]]
name = \"app\"
sources = [\"src\"]
";

/// A project directory beside a library repository: `src/app.ls` imports `libx.top`.
fn project(tag: &str) -> (PathBuf, PathBuf, PathBuf, String) {
    let (repo, cache, rev) = library_repo(tag);
    let dir = cache.parent().unwrap().join("app");
    write(&dir.join("src"), "app.ls", APP);
    write(&dir, "lex-sys.toml", PROJECT);
    (dir, repo, cache, rev)
}

/// `lex-sys <args>` in `dir`, with its own cache.
fn run(dir: &Path, cache: &Path, args: &[&dyn AsRef<std::ffi::OsStr>]) -> std::process::Output {
    let mut cmd = Command::new(BIN);
    cmd.current_dir(dir).env("LEX_SYS_CACHE", cache);
    for a in args {
        cmd.arg(a.as_ref());
    }
    cmd.output().expect("the compiler runs")
}

fn add_libx(dir: &Path, cache: &Path, repo: &Path, rev: &str) -> std::process::Output {
    run(dir, cache, &[&"add", &"libx", &repo, &"--rev", &rev, &"--path", &".lex-sys-vcs/libx.top"])
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn add_then_build_makes_a_program_that_uses_the_dependency() {
    let (dir, repo, cache, rev) = project("project-e2e");
    let out = add_libx(&dir, &cache, &repo, &rev);
    let stdout = ok(&out);
    assert!(stdout.contains("added libx") && stdout.contains("installed libx"), "{stdout}");

    let toml = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();
    assert!(toml.contains("[dependencies.libx]") && toml.contains(&rev), "{toml}");
    assert!(toml.starts_with(PROJECT), "the file is appended to, what was there is untouched");
    assert_eq!(
        files_in(&dir.join("build/deps")).len(),
        2,
        "the top and, through its requirement, the base"
    );

    // `build` installs first: with the fetched files gone it still builds.
    std::fs::remove_dir_all(dir.join("build/deps")).unwrap();
    ok(&run(&dir, &cache, &[&"build"]));
    assert_eq!(files_in(&dir.join("build/deps")).len(), 2, "build brought the dependencies back");
    let exe = dir.join("build/app");
    let result = Command::new(&exe).output().expect("the program runs");
    assert_eq!(result.status.code(), Some(60), "twice(2) is pick(2) * 2");
}

#[test]
fn a_ref_is_resolved_once_and_the_hash_is_what_is_written() {
    let (dir, repo, cache, rev) = project("project-ref");
    let branch = git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let out = run(
        &dir,
        &cache,
        &[&"add", &"libx", &repo, &"--ref", &branch, &"--path", &".lex-sys-vcs/libx.top"],
    );
    ok(&out);
    let toml = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();
    assert!(toml.contains(&format!("rev = \"{rev}\"")), "{toml}");
    assert!(!toml.contains(&format!("\"{branch}\"")), "the name of the ref is not written: {toml}");
}

#[test]
fn add_refuses_what_it_should_and_leaves_the_file_as_it_was() {
    let (dir, repo, cache, rev) = project("project-add-refusals");
    let before = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();

    // A branch name is not a pin.
    let by_name = run(&dir, &cache, &[&"add", &"libx", &repo, &"--rev", &"master"]);
    assert!(!by_name.status.success());
    assert!(stderr(&by_name).contains("full commit hash"), "{}", stderr(&by_name));

    // The repository's store directory holds stores, not one: say which.
    let wrong = run(&dir, &cache, &[&"add", &"libx", &repo, &"--rev", &rev]);
    assert!(!wrong.status.success());
    let message = stderr(&wrong);
    assert!(
        message.contains("not a store") && message.contains(".lex-sys-vcs/libx.top"),
        "{message}"
    );

    // A nonsense name, a path that is not there, and both of --rev and --ref.
    let named = run(
        &dir,
        &cache,
        &[&"add", &"a b", &repo, &"--rev", &rev, &"--path", &".lex-sys-vcs/libx.top"],
    );
    assert!(
        !named.status.success() && stderr(&named).contains("is not a dependency name"),
        "{}",
        stderr(&named)
    );
    let missing = run(&dir, &cache, &[&"add", &"libx", &repo, &"--rev", &rev, &"--path", &"nope"]);
    assert!(!missing.status.success());
    assert!(
        !run(&dir, &cache, &[&"add", &"libx", &repo, &"--rev", &rev, &"--ref", &"main"])
            .status
            .success()
    );
    assert_eq!(std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap(), before);

    // The same name twice.
    ok(&add_libx(&dir, &cache, &repo, &rev));
    let once = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();
    let again = add_libx(&dir, &cache, &repo, &rev);
    assert!(!again.status.success() && stderr(&again).contains("already has a dependency"));
    assert_eq!(std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap(), once);
}

#[test]
fn an_add_whose_install_fails_puts_the_file_back_and_names_the_dependency() {
    let (dir, repo, cache, _rev) = project("project-add-rollback");
    // A store whose source blob was changed after it was published: it is there, the arguments are fine, and the
    // fetch fails -- after the file was written.
    let blob = files_in(&repo.join(".lex-sys-vcs/libx.top/sources"))
        .into_iter()
        .find(|p| p.extension().is_some_and(|e| e == "ls"))
        .expect("the top's source blob");
    let text = std::fs::read_to_string(&blob).unwrap();
    std::fs::write(&blob, format!("{text}// changed\n")).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "corrupt"]);
    let bad = git(&repo, &["rev-parse", "HEAD"]);

    let before = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();
    let out = add_libx(&dir, &cache, &repo, &bad);
    assert!(!out.status.success(), "a store that fails its own check is not added");
    assert!(stderr(&out).contains("dependency `libx`"), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap(),
        before,
        "the file is put back"
    );

    // A commit the repository does not have fails earlier still, and also changes nothing.
    let nowhere = "0123456789abcdef0123456789abcdef01234567";
    assert!(!add_libx(&dir, &cache, &repo, nowhere).status.success());
    assert_eq!(std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap(), before);
}

#[test]
fn a_store_with_nothing_in_it_is_not_a_dependency() {
    let (dir, repo, cache, _rev) = project("project-empty-store");
    // A file that declares a module and nothing in it publishes nothing.
    write(&repo.join("src"), "empty.ls", "module empty;\n");
    ok(&vcs(
        &cache,
        &[&"publish", &"--dir", &repo.join("src"), &"--store", &repo.join(".lex-sys-vcs")],
    ));
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "empty"]);
    let rev = git(&repo, &["rev-parse", "HEAD"]);
    let before = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();
    let out = run(
        &dir,
        &cache,
        &[&"add", &"nothing", &repo, &"--rev", &rev, &"--path", &".lex-sys-vcs/empty"],
    );
    assert!(
        !out.status.success() && stderr(&out).contains("nothing is published"),
        "{}",
        stderr(&out)
    );
    assert_eq!(std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap(), before);
}

#[test]
fn a_project_file_that_is_not_one_is_refused() {
    let (dir, _repo, cache, rev) = project("project-bad-files");
    // Each case, and the part of the message that says why: not only that something refused.
    let cases: Vec<(&str, String, &str)> = vec![
        ("an unknown key in a program", format!("{PROJECT}lex = \"0.1\"\n"), "unknown field `lex`"),
        ("an unknown section", format!("{PROJECT}\n[extra]\nx = 1\n"), "unknown field `extra`"),
        (
            "an unknown key in a package",
            PROJECT.replacen("name = \"app\"", "name = \"app\"\nversion = \"1\"", 1),
            "unknown field `version`",
        ),
        (
            "a dependency rev that is a name",
            format!("{PROJECT}[dependencies.x]\ngit = \"https://example.com/r\"\nrev = \"main\"\n"),
            "[dependencies.x]",
        ),
        (
            "a dependency path that leaves the repository",
            format!(
                "{PROJECT}[dependencies.x]\ngit = \"https://example.com/r\"\nrev = \"{rev}\"\npath = \"../x\"\n"
            ),
            "[dependencies.x]",
        ),
        (
            "a lex-sys that is not a hash",
            PROJECT.replacen("name = \"app\"", "name = \"app\"\nlex-sys = \"f804ce7\"", 1),
            "full commit hash",
        ),
        (
            "two programs of one name",
            format!("{PROJECT}[[bin]]\nname = \"app\"\nsources = [\"src\"]\n"),
            "must be unique",
        ),
        (
            "a program with no sources",
            PROJECT.replace("sources = [\"src\"]", "sources = []"),
            "has no `sources`",
        ),
        (
            "a bad package name",
            PROJECT.replace("name = \"app\"\n\n", "name = \"a b\"\n\n"),
            "package name",
        ),
    ];
    for (what, text, why) in cases {
        std::fs::write(dir.join("lex-sys.toml"), &text).unwrap();
        for command in ["install", "build"] {
            let out = run(&dir, &cache, &[&command]);
            assert!(!out.status.success(), "{what}: `{command}` should refuse:\n{text}");
            assert!(
                stderr(&out).contains(why),
                "{what}: `{command}` should say `{why}`: {}",
                stderr(&out)
            );
        }
    }
}

#[test]
fn a_compiler_other_than_the_one_named_is_refused() {
    let (dir, _repo, cache, _rev) = project("project-compiler");
    let other = "0".repeat(40);
    let text =
        PROJECT.replacen("name = \"app\"", &format!("name = \"app\"\nlex-sys = \"{other}\""), 1);
    std::fs::write(dir.join("lex-sys.toml"), &text).unwrap();

    for command in ["install", "build"] {
        let out = run(&dir, &cache, &[&command]);
        assert!(!out.status.success(), "`{command}` must refuse a compiler that is not {other}");
        let message = stderr(&out);
        assert!(message.contains(&other) && message.contains("this compiler is"), "{message}");
        assert!(
            run(&dir, &cache, &[&command, &"--ignore-compiler-rev"]).status.success()
                || command == "build",
            "the escape hatch works for `install`"
        );
    }
    let ignored = run(&dir, &cache, &[&"build", &"--ignore-compiler-rev"]);
    // The program imports a library this project does not depend on, so the build fails: but on that, with the
    // compiler only noted.
    let said = stderr(&ignored);
    assert!(said.contains("note: this project was written for lex-sys"), "{said}");
    assert!(said.contains("no module `libx.top`"), "{said}");

    // The compiler's own revision, when it is a clean commit: accepted. (A dirty or unknown one cannot be written down.)
    let version = Command::new(BIN).arg("--version").output().unwrap();
    let version = String::from_utf8_lossy(&version.stdout).into_owned();
    let rev =
        version.split("rev ").nth(1).and_then(|r| r.split(',').next()).unwrap_or("").to_owned();
    assert!(version.contains("(rev "), "--version names the revision: {version}");
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        std::fs::write(
            dir.join("lex-sys.toml"),
            PROJECT.replacen("name = \"app\"", &format!("name = \"app\"\nlex-sys = \"{rev}\""), 1),
        )
        .unwrap();
        ok(&run(&dir, &cache, &[&"install"]));
    }
}

#[test]
fn install_replaces_what_an_older_pin_fetched() {
    let (dir, repo, cache, rev) = project("project-restale");
    ok(&add_libx(&dir, &cache, &repo, &rev));
    let first: Vec<_> = files_in(&dir.join("build/deps"));

    // The library moves on; the project moves its pin.
    write(&repo.join("src"), "base.ls", &BASE.replace("t[2] = 30;", "t[2] = 31;"));
    ok(&vcs(
        &cache,
        &[&"publish", &"--dir", &repo.join("src"), &"--store", &repo.join(".lex-sys-vcs")],
    ));
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "moved"]);
    let moved = git(&repo, &["rev-parse", "HEAD"]);
    let toml = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();
    std::fs::write(dir.join("lex-sys.toml"), toml.replace(&rev, &moved)).unwrap();

    ok(&run(&dir, &cache, &[&"install"]));
    let second: Vec<_> = files_in(&dir.join("build/deps"));
    assert_eq!(second.len(), 2, "two files, not the old pin's and the new pin's: {second:?}");
    assert!(second.iter().any(|p| !first.contains(p)), "the base changed, so one file is new");

    // And the program built from it is the new one.
    ok(&run(&dir, &cache, &[&"build"]));
    let result = Command::new(dir.join("build/app")).output().unwrap();
    assert_eq!(result.status.code(), Some(62), "the moved pin changed t[2] to 31: twice(2) is 62");
}

#[test]
fn a_project_is_found_from_a_directory_below_it_and_files_still_build_as_files() {
    let (dir, repo, cache, rev) = project("project-walk-up");
    ok(&add_libx(&dir, &cache, &repo, &rev));
    let below = dir.join("src");
    ok(&run(&below, &cache, &[&"build"]));
    assert!(
        dir.join("build/app").is_file(),
        "built at the project's root, not the directory it was run from"
    );

    // `build <files>` is the compiler as before, project or no project.
    let tool = write(
        &dir,
        "tool.ls",
        "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io); release(ffi); release(fs); release(heap); release(args);\n    return 3;\n}\n",
    );
    let out = dir.join("tool");
    ok(&run(&dir, &cache, &[&"build", &tool, &"-o", &out]));
    assert_eq!(Command::new(&out).status().unwrap().code(), Some(3));
}

#[test]
fn bin_picks_one_program_and_an_unknown_one_is_refused() {
    let (dir, repo, cache, rev) = project("project-bin");
    ok(&add_libx(&dir, &cache, &repo, &rev));
    let toml = std::fs::read_to_string(dir.join("lex-sys.toml")).unwrap();
    write(
        &dir.join("other"),
        "other.ls",
        "fn main(world: World) -> [] int {\n    let Split { io, ffi, fs, heap, args } = split(world);\n    release(io); release(ffi); release(fs); release(heap); release(args);\n    return 7;\n}\n",
    );
    std::fs::write(
        dir.join("lex-sys.toml"),
        format!("{toml}\n[[bin]]\nname = \"other\"\nsources = [\"other/other.ls\"]\nout = \"out/other\"\n"),
    )
    .unwrap();
    ok(&run(&dir, &cache, &[&"build", &"--bin", &"other"]));
    assert!(dir.join("out/other").is_file());
    assert!(!dir.join("build/app").exists(), "only the named program is built");
    let unknown = run(&dir, &cache, &[&"build", &"--bin", &"nope"]);
    assert!(!unknown.status.success() && stderr(&unknown).contains("no [[bin]] named"));
}

#[test]
fn without_a_project_file_there_is_nothing_to_install() {
    let work = scratch("project-none");
    let out = run(&work, &work.join("cache"), &[&"install"]);
    assert!(!out.status.success() && stderr(&out).contains("lex-sys.toml"), "{}", stderr(&out));
    let _ = (TOP, BASE);
}
