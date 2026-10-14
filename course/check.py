#!/usr/bin/env python3
"""Check one course file against what its own header says it should do.

    course/check.py work/01-hello/card.cho

The header is the same comment syntax the compiler's own fixtures use
(`crates/cancho/tests/conformance/main.rs`), so a file says what it is
for and the checker is only the thing that reads it:

    //~ STDOUT <a line the program must print>     run it, compare stdout
    //~ EXIT <n>                                   ...and its exit status
    //~ RULE <tag>                                 it must be refused, with these rules
    //~ AUTHORITY <label; label>                   `cancho authority` must report exactly these,
                                                  a label with its argument: fs_read("/etc/hostname")
    //~ WITH <file.cho>                            other files of the same program, beside this one
    //~ SATISFIES <contract.cho>                   `cancho satisfy` must say satisfied
    //~ UNSATISFIED <contract.cho>                 ...and for a fixture, that it must not

With none of those: a file with `fn test_*` is run with `cancho test`, and
anything else is `cancho check`. The checker never prints a solution and
never edits a file; it tells you what the compiler said.

The compiler is `$CANCHO`, else `cancho` on the path, else the repository's
own `target/` build.
"""
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent


def compiler() -> str:
    if os.environ.get("CANCHO"):
        return os.environ["CANCHO"]
    found = shutil.which("cancho")
    if found:
        return found
    for profile in ("release", "debug"):
        built = ROOT / "target" / profile / "cancho"
        if built.exists():
            return str(built)
    sys.exit("no `cancho` found: build it with `cargo build --release -p cancho` and put target/release on your PATH")


def run(args, stdin=None):
    return subprocess.run(
        [compiler(), *args, "--std"],
        input=stdin,
        capture_output=True,
        text=True,
        timeout=120,
    )


def directives(text: str):
    found = {}
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("//~ "):
            key, _, value = stripped[4:].partition(" ")
            found.setdefault(key, []).append(value.strip())
    return found


def refusals(path: Path, extra=()):
    out = run(["check", str(path), *extra, "--output", "json"])
    try:
        return out, [r["rule"] for r in json.loads(out.stdout)["refused"]]
    except (ValueError, KeyError):
        return out, None


def fail(message: str):
    print(f"FAIL  {message}")
    return 1


def check(path: Path) -> int:
    text = path.read_text()
    want = directives(text)

    if "SATISFIES" in want or "UNSATISFIED" in want:
        status = 0
        for kind in ("SATISFIES", "UNSATISFIED"):
            for name in want.get(kind, []):
                contract = (path.parent / name).resolve()
                out = run(["satisfy", str(contract), str(path)])
                if kind == "SATISFIES" and out.returncode == 0:
                    print(f"ok    {path.name} satisfies {contract.name}")
                elif kind == "SATISFIES":
                    status |= fail(f"{path.name} does not satisfy {contract.name} (exit {out.returncode})\n{out.stdout}{out.stderr}")
                elif out.returncode == 0:
                    status |= fail(f"{path.name} satisfies {contract.name}, and this file is meant to be a candidate it does not")
                else:
                    print(f"ok    {path.name} is refused by {contract.name} (exit {out.returncode}), as it should be")
        return status

    if "RULE" in want:
        out, reported = refusals(path)
        if reported is None:
            return fail(f"could not read the compiler's answer:\n{out.stdout}{out.stderr}")
        if sorted(reported) != sorted(want["RULE"]):
            return fail(f"expected the refusals {sorted(want['RULE'])}, the compiler answered {sorted(reported)}")
        print(f"ok    {path.name} is refused with {', '.join(reported)}")
        return 0

    if "fn test_" in text and "fn main" not in text:
        # `cancho test` supplies its own `main`, so `check` would refuse the file.
        out = run(["test", str(path)])
        if out.returncode != 0:
            return fail(f"`cancho test` exit {out.returncode}\n{out.stdout}{out.stderr}")
        lines = out.stdout.strip().splitlines()
        print(f"ok    {path.name}: {lines[-1] if lines else 'tests pass'}")
        return 0

    extra = [str((path.parent / name).resolve()) for name in want.get("WITH", [])]
    out, reported = refusals(path, extra)
    if reported is None:
        return fail(f"could not read the compiler's answer:\n{out.stdout}{out.stderr}")
    if reported:
        shown = subprocess.run([compiler(), "check", str(path), *extra, "--std"], capture_output=True, text=True)
        return fail(f"refused: {', '.join(reported)}\n{shown.stderr or shown.stdout}")

    if "STDOUT" in want or "EXIT" in want:
        stdin = "\n".join(want["STDIN"]) + "\n" if "STDIN" in want else None
        out = run(["run", str(path), *extra], stdin)
        expected = "\n".join(want.get("STDOUT", [])) + ("\n" if "STDOUT" in want else "")
        code = int(want.get("EXIT", ["0"])[0])
        if "STDOUT" in want and out.stdout != expected:
            return fail(f"output differs\n--- expected\n{expected}--- got\n{out.stdout}")
        if out.returncode != code:
            return fail(f"exit status {out.returncode}, expected {code}\n{out.stderr}")
        print(f"ok    {path.name} prints what it should")
    else:
        print(f"ok    {path.name} checks")

    if "AUTHORITY" in want:
        shown = run(["authority", str(path), *extra, "--output", "json"])
        got = sorted(
            label["name"] if label["argument"] is None else f'{label["name"]}("{label["argument"]}")'
            for label in json.loads(shown.stdout)["labels"]
        )
        expected = sorted(x.strip() for x in want["AUTHORITY"][0].split(";") if x.strip())
        if got != expected:
            return fail(f"authority is {got}, expected {expected}")
        print(f"ok    {path.name} reaches exactly {got or 'nothing'}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    sys.exit(max(check(Path(p)) for p in sys.argv[1:]))
