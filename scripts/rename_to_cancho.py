#!/usr/bin/env python3
"""Rename lex-sys to cancho in a git checkout: paths first (git mv), then the text of every file.

usage: rename.py <checkout>
The package stores under packages/*/.lex-sys-vcs are moved but not edited: they are content
addressed, and are regenerated afterwards.
"""
import os, re, subprocess, sys

root = sys.argv[1]
os.chdir(root)
files = subprocess.run(["git", "ls-files"], capture_output=True, text=True, check=True).stdout.splitlines()

KEEP = "\0KEEPHOST\0"
SUBS = [
    (re.compile(r"lex-sys\.test"), KEEP),
    (re.compile(r"lex-sys"), "cancho"),
    (re.compile(r"lex_sys"), "cancho"),
    (re.compile(r"lexsys"), "cancho"),
    (re.compile(r"LEX_SYS"), "CANCHO"),
    (re.compile(r"\.ls\b"), ".cho"),
    (re.compile(r'"ls"'), '"cho"'),
    (re.compile(r"\{ls,"), "{cho,"),
    (re.compile(r"LEXSYS"), "CANCHO"),
]


def new_path(p):
    parts = p.split("/")
    out = []
    for seg in parts:
        seg = seg.replace("lex-sys", "cancho").replace("lex_sys", "cancho").replace("lexsys", "cancho")
        out.append(seg)
    q = "/".join(out)
    if q.endswith(".ls"):
        q = q[:-3] + ".cho"
    return q


def in_store(p):
    return "/.lex-sys-vcs/" in p or "/.cancho-vcs/" in p


moves = [(p, new_path(p)) for p in files if new_path(p) != p]
for old, new in moves:
    os.makedirs(os.path.dirname(new) or ".", exist_ok=True)
    subprocess.run(["git", "mv", old, new], check=True)

for p in subprocess.run(["git", "ls-files"], capture_output=True, text=True, check=True).stdout.splitlines():
    if in_store(p):
        continue
    try:
        data = open(p, "rb").read()
        text = data.decode("utf-8")
    except (UnicodeDecodeError, FileNotFoundError):
        continue
    out = text
    for rx, rep in SUBS:
        out = rx.sub(rep, out)
    out = out.replace(KEEP, "lex-sys.test")
    if out != text:
        open(p, "w", encoding="utf-8").write(out)
print(len(moves), "paths moved")
