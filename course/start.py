#!/usr/bin/env python3
"""Set up one lesson's working directory -- outside this repository.

    course/start.py 5                  # -> ~/cancho-course/05-contract
    course/start.py 5 --dest /tmp/w    # somewhere else
    course/start.py 5 --offline        # no agent? also get stand-in agent output

The exercise files are copied out of the repository on purpose. The
agent you hand a lesson to can read whatever its directory can reach, and
the reference solutions live in `course/solutions/`; the honest way to
keep an agent from reading them is to run it somewhere they are not.

`--offline` is for working without an agent. It adds `offline/`, the
stand-in for what an agent would have handed back (a planted fixture with
with every comment stripped, since the fixtures explain themselves in them), and you treat it
exactly as you would the real thing.
"""
import argparse
import shutil
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def strip_comments(text: str) -> str:
    """Drop every comment-only line: the planted fixtures explain themselves in them."""
    lines = [line for line in text.splitlines() if not line.strip().startswith("//")]
    while lines and not lines[0].strip():
        lines.pop(0)
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("lesson", help="lesson number, e.g. 5")
    parser.add_argument("--dest", help="where to put it (default ~/cancho-course/<lesson>)")
    parser.add_argument("--offline", action="store_true", help="also copy stand-in agent output")
    args = parser.parse_args()

    prefix = f"{int(args.lesson):02d}-"
    found = sorted(p for p in (HERE / "exercises").iterdir() if p.name.startswith(prefix))
    if not found:
        sys.exit(f"no lesson {args.lesson}: see course/README.md for the list")
    source = found[0]
    dest = Path(args.dest) if args.dest else Path.home() / "cancho-course" / source.name
    if dest.exists() and any(dest.iterdir()):
        sys.exit(f"{dest} already has files in it; pick another --dest or remove it. Nothing was copied.")
    dest.mkdir(parents=True, exist_ok=True)

    for item in sorted(source.iterdir()):
        shutil.copy2(item, dest / item.name)
    copied = [p.name for p in sorted(dest.iterdir())]

    if int(args.lesson) >= 5:
        shutil.copy2(HERE / "agent" / "builder.md", dest / "BUILDER.md")
        shutil.copy2(HERE / "stub_main.cho", dest / "stub_main.cho")
        copied += ["BUILDER.md", "stub_main.cho"]

    if args.offline:
        fixtures = HERE / "fixtures" / source.name
        offline = dest / "offline"
        offline.mkdir()
        for item in sorted(fixtures.glob("*.cho")):
            (offline / item.name).write_text(strip_comments(item.read_text()))
        copied.append("offline/")

    print(f"{source.name} -> {dest}")
    for name in copied:
        print(f"  {name}")
    print(f"\nRead course/lessons/{source.name}.md, then work in {dest}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
