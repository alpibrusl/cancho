"""One entry per language: the file, the feedback tool (its output goes back to the model), and how to run the result."""
import os, subprocess, shutil

CANCHO = os.environ.get("CANCHO_BIN", "cancho")

def sh(cmd, cwd, timeout=120, stdin=None):
    try:
        r = subprocess.run(cmd, cwd=cwd, capture_output=True, timeout=timeout, input=stdin)
        return r.returncode, r.stdout, r.stderr
    except subprocess.TimeoutExpired:
        return 124, b"", b"timeout"

LANGS = {
    # Each build() returns (ok, feedback_text, run_cmd). The feedback is the language's own best checker.
    "cancho": dict(file="sol.cho", name="cancho"),
    "rust":   dict(file="sol.rs",  name="Rust"),
    "python": dict(file="sol.py",  name="Python 3"),
    "c":      dict(file="sol.c",   name="C"),
}

def build(lang, d):
    f = LANGS[lang]["file"]
    if lang == "cancho":
        rc, out, err = sh([CANCHO, "check", f, "--std", "--output", "json"], d)
        if rc != 0: return False, (out + err).decode(errors="replace")[:4000], None
        rc, out, err = sh([CANCHO, "build", f, "--std", "-o", "sol"], d, 300)
        if rc != 0: return False, (out + err).decode(errors="replace")[:4000], None
        return True, "", ["./sol"]
    if lang == "rust":
        rc, out, err = sh(["rustc", "--edition", "2021", "-O", f, "-o", "sol"], d, 300)
        if rc != 0: return False, (out + err).decode(errors="replace")[:4000], None
        return True, "", ["./sol"]
    if lang == "python":
        rc, out, err = sh(["python3", "-m", "py_compile", f], d)
        if rc != 0: return False, (out + err).decode(errors="replace")[:4000], None
        return True, "", ["python3", f]
    if lang == "c":
        rc, out, err = sh(["clang", "-O1", "-g", "-Wall", "-Wextra", "-fsanitize=address,undefined", f, "-o", "sol"], d, 300)
        if rc != 0: return False, (out + err).decode(errors="replace")[:4000], None
        return True, "", ["./sol"]

def run_cases(cmd, d, cases):
    results = []
    for stdin, want in cases:
        rc, out, err = sh(cmd, d, 20, stdin)
        results.append((rc == 0 and out == want, rc, out[:200], err[:300]))
    return results
