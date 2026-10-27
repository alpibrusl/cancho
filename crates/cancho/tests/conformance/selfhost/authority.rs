//! `cancho authority` in cancho (`examples/selfhost/authority.cho`, `docs/cli-in-cancho.md` section 34), against `cancho authority --output json`.

use super::*;

/// The functions a program reaches, as the compiler in cancho lists them (`cli.cho authority-reached`): for each, its name, whether it is pure, whether it performed a label that is not followed, and the labels.
struct Reached {
    name: String,
    pure: bool,
    untracked: bool,
    labels: Vec<String>,
}

fn reached(exe: &Path, file: &Path, with_std: bool) -> Result<Vec<Reached>, i32> {
    let mut command = Command::new(exe);
    command.arg("authority-reached").arg(file);
    if with_std {
        command
            .arg("--std-dir")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../std").canonicalize().unwrap());
    }
    let done = command.output().expect("cli runs");
    if !done.status.success() {
        return Err(done.status.code().unwrap_or(-1));
    }
    Ok(done
        .stdout
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields: Vec<String> =
                line.split(|b| *b == 0).map(|f| String::from_utf8_lossy(f).into_owned()).collect();
            Reached {
                name: fields[0].clone(),
                pure: fields[1] == "1",
                untracked: fields[2] == "1",
                labels: fields[3..].iter().filter(|l| !l.is_empty()).cloned().collect(),
            }
        })
        .collect())
}

/// The programs of `examples/` that have a `main` and import nothing but the library: `cancho authority --output json` and the list of the compiler in cancho are compared for the number of functions, which of
/// them are pure and the labels without an argument. A program in which the checker in cancho does not follow a label (one with an argument) is compared only for the number of functions, and has to be one for
/// which the Rust command reports a label with an argument.
#[test]
fn authority_reached_in_cancho_agrees_with_cancho_authority() {
    let exe = build("cli-authority", &with_front_end("cli.cho"));
    let examples =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").canonicalize().unwrap();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&examples)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cho"))
        .collect();
    files.sort();
    let mut failures = Vec::new();
    let (mut compared, mut followed) = (0, 0);
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let imports_only_std =
            lines.iter().filter(|l| l.starts_with("import ")).all(|l| l.starts_with("import std"));
        if !lines.iter().any(|l| l.starts_with("fn main")) || !imports_only_std {
            continue;
        }
        let with_std = lines.iter().any(|l| l.starts_with("import std"));
        let mut rust = Command::new(BIN);
        rust.args(["authority", "--output", "json"]).arg(&file);
        if with_std {
            rust.arg("--std");
        }
        let done = rust.output().expect("cancho runs");
        if !done.status.success() {
            continue;
        }
        let json: serde_json::Value = serde_json::from_slice(&done.stdout).unwrap();
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(list) = reached(&exe, &file, with_std) else {
            failures.push(format!("{name}: the compiler in cancho answers a status"));
            continue;
        };
        compared += 1;
        if list.len() as u64 != json["functions"].as_u64().unwrap() {
            failures.push(format!(
                "{name}: {} functions, the Rust command {}",
                list.len(),
                json["functions"]
            ));
            continue;
        }
        let with_argument =
            json["labels"].as_array().unwrap().iter().any(|l| !l["argument"].is_null());
        let unfollowed = list.iter().any(|f| f.untracked);
        if unfollowed != with_argument {
            failures.push(format!(
                "{name}: labels not followed {unfollowed}, the Rust command has labels with an argument {with_argument}"
            ));
            continue;
        }
        if unfollowed {
            continue;
        }
        followed += 1;
        let mut pure: Vec<&str> = list.iter().filter(|f| f.pure).map(|f| f.name.as_str()).collect();
        pure.sort_unstable();
        let rust_pure: Vec<&str> =
            json["pure"].as_array().unwrap().iter().map(|p| p.as_str().unwrap()).collect();
        if pure != rust_pure {
            failures.push(format!("{name}: pure {pure:?}, the Rust command {rust_pure:?}"));
        }
        let mut labels: Vec<&str> =
            list.iter().flat_map(|f| f.labels.iter().map(String::as_str)).collect();
        labels.sort_unstable();
        labels.dedup();
        let rust_labels: Vec<&str> = json["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap())
            .collect();
        if labels != rust_labels {
            failures.push(format!("{name}: labels {labels:?}, the Rust command {rust_labels:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {compared} programs differ ({followed} had every label followed):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(compared >= 16, "only {compared} programs were compared");
}
