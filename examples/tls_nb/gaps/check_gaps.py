#!/usr/bin/env python3
"""Run every reproducer in this directory and check it still does what docs/tls-nonblocking.md section 9 says it does.

    python3 check_gaps.py          # exits 0 only if every reproducer behaves as recorded

Each row is (file, how the compiler answers `check`, what running it does). `refused:<rule>` is a located refusal with that rule tag;
`accepted` is a program that compiles; `internal` is the compiler's own failure. The run column is the exit status of the built
program (and, where it matters, a fragment of its output). A row that stops being true is a gap that was fixed (or a document that
is now wrong): update the row and the document together.
"""
import json, os, shutil, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
CANCHO = os.environ.get("CANCHO", "/home/user/cancho/target/release/cancho")
WORK = os.environ.get("TLS_NB_WORK", "/tmp/tls_nb_work")

ROWS = [
    # file, check, run exit (None: not run), output fragment
    ("a1_ffi_scope.cho",                "accepted",                      None, None),
    ("a2_scope_is_nominal.cho",         "accepted",                      0, None),
    ("a3_row_exact.cho",                "refused:effect-not-declared",   None, None),
    ("g1_cptr_param.cho",               "refused:unknown-name",          None, None),
    ("g2_cptr_field.cho",               "refused:unknown-name",          None, None),
    ("g3_cptr_array.cho",               "accepted",                      0, None),
    ("g4_generic.cho",                  "accepted",                      0, None),
    ("g5_table_ticket.cho",             "accepted",                      None, None),
    ("g6_extern_free.cho",              "internal",                      None, None),
    ("g7_two_strings.cho",              "accepted",                      139, None),
    ("g8_c_string_result.cho",          "accepted",                      0, "there is no way to read it"),
    ("g9_qualified_function_value.cho", "refused:unknown-name",          None, None),
    ("g10_thread_in_container.cho",     "refused:mode-bound-violated",   None, None),
    ("g11_connect_is_a_builtin.cho.txt","refused:foreign-declaration",   None, None),
    ("g12_scopes_do_not_compose.cho",   "refused:linear-use-after-move", None, None),
    ("g13_int_vs_c_int.cho",            "accepted",                      0, "4294967295\n-1\n"),
    ("g14_out_params.cho",              "accepted",                      139, None),
    ("g15_no_listener_port.cho",        "refused:not-a-function",        None, None),
    ("g16_no_thread_poll.cho",          "refused:not-a-function",        None, None),
    ("g17_conn_payload.cho",            "refused:thread-payload-type",   None, None),
    ("t1_spawn_job.cho",                "accepted",                      0, None),
    ("t2_ref_field.cho.txt",            "refused:region-mismatch",       None, None),
    ("t3_shared_ffi_payload.cho",       "accepted",                      0, None),
]

def check(path, backend=None):
    if path.endswith(".txt"):
        # A reproducer the formatter cannot read (the repository's own formatting test walks every `.cho` under `examples/`): kept as text.
        copy = os.path.join(WORK, os.path.basename(path)[:-4])
        shutil.copyfile(path, copy)
        path = copy
    cmd = [CANCHO, "check", path, "--std", "--output", "json"] + (["--backend", backend] if backend else [])
    p = subprocess.run(cmd, capture_output=True, text=True)
    refused = json.loads(p.stdout)["refused"]
    if not refused:
        return "accepted"
    return "internal" if refused[0]["rule"] == "internal" else "refused:" + refused[0]["rule"]

def main():
    os.makedirs(WORK, exist_ok=True)
    bad = 0
    for name, want_check, want_exit, want_out in ROWS:
        path = os.path.join(HERE, name)
        got = check(path)
        ok = got == want_check
        detail = ""
        if ok and want_exit is not None:
            exe = os.path.join(WORK, "gap_" + name.split(".")[0])
            b = subprocess.run([CANCHO, "build", path, "--std", "-l", "ssl", "-l", "crypto", "-o", exe], capture_output=True, text=True)
            if b.returncode != 0:
                ok, detail = False, "did not build: " + b.stderr[:100]
            else:
                r = subprocess.run([exe], capture_output=True, text=True)
                code = r.returncode if r.returncode >= 0 else 128 + (-r.returncode)
                ok = code == want_exit and (want_out is None or want_out in r.stdout)
                detail = "exit %d" % code
        if name == "g6_extern_free.cho" and ok:
            cl = check(path, "cranelift")
            ok = cl == "accepted"
            detail = "llvm: internal, cranelift: %s" % cl
        bad += 0 if ok else 1
        print("%-4s %-34s check: %-32s %s" % ("ok" if ok else "FAIL", name, got, detail if ok else detail + " (wanted check %s, exit %s)" % (want_check, want_exit)))
    print("FAILED: %d" % bad if bad else "every reproducer behaves as recorded")
    return 1 if bad else 0

sys.exit(main())
