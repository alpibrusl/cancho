#!/usr/bin/env python3
"""A module of LLVM text with its functions taken out: the data, for clang, with every name visible to the linker."""
import re, sys
out = []
inside = False
for line in open(sys.argv[1]):
    if inside:
        if line.startswith("}"):
            inside = False
        continue
    if line.startswith("define "):
        inside = True
        head = line[:line.rindex(")") + 1]
        head = re.sub(r"^define (internal |private )?", "declare ", head)
        head = re.sub(r" %[\w.\-]+(?=[,)])", "", head)
        out.append(head + "\n")
        continue
    if line.startswith("@"):
        line = re.sub(r"= (private|internal) ", "= ", line).replace(" unnamed_addr", "")
    out.append(line)
open(sys.argv[2], "w").write("".join(out))
