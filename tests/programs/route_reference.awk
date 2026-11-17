# route_reference.awk -- the reference matcher `std.route` is held to (docs/http.md section 5), written from the rules and not from the library: segments, `:name`, `*name`,
# static first, then in order (tests/commands/http.sh). Two runs of it, with the same seed (a Park-Miller generator, small enough to be exact in an awk number):
#     awk -v mode=gen -v seed=N -f route_reference.awk       prints the script `tests/programs/route_driver.cho` reads: R (a route), Q (a query), A (the methods a 405 lists)
#     awk -v mode=check -v seed=N -v out=FILE -f route_reference.awk    makes the same script again and compares FILE (what the driver printed) with the reference, line by line
# `check` prints `ok queries=N matched=A wrong_method=B no_route=C allowed_root=D routes=R` or one line per difference (`DIFF ...`).

function below(n) { seed = (seed * 16807) % 2147483647; return seed % n }
function is_static(p) { return index(p, ":") == 0 && index(p, "*") == 0 }

# Does PATTERN match PATH? The parameters (offsets into PATH) are left in PS, PE and NP.
function segs(pattern, path,    s, pn, pseg, on, oseg, off_s, off_e, at, i, seg) {
    NP = 0
    s = substr(pattern, 2)
    if (s == "") { pn = 1; pseg[1] = "" } else pn = split(s, pseg, "/")
    s = substr(path, 2)
    if (s == "") { on = 1; oseg[1] = "" } else on = split(s, oseg, "/")
    at = 1
    for (i = 1; i <= on; i++) { off_s[i] = at; off_e[i] = at + length(oseg[i]); at += length(oseg[i]) + 1 }
    for (i = 1; i <= pn; i++) {
        seg = pseg[i]
        if (i > on) return 0
        if (substr(seg, 1, 1) == "*") { NP++; PS[NP] = off_s[i]; PE[NP] = length(path); return 1 }
        if (substr(seg, 1, 1) == ":") {
            if (off_s[i] == off_e[i]) return 0
            NP++; PS[NP] = off_s[i]; PE[NP] = off_e[i]
        } else if (oseg[i] != seg) return 0
    }
    return on == pn
}

# The id the router must answer (-1 no route, -2 a route with another method), and the parameters in PS, PE, NP.
function find(method, path,    i, known) {
    known = 0
    NP = 0
    for (i = 1; i <= nr; i++) if (is_static(rp[i]) && rp[i] == path) { if (rm[i] == method) { NP = 0; return rid[i] } known = 1 }
    for (i = 1; i <= nr; i++) if (!is_static(rp[i])) { if (segs(rp[i], path)) { if (rm[i] == method) return rid[i]; known = 1 } }
    NP = 0
    return known ? -2 : -1
}

# The methods of the routes that fit PATH, once each, in order, joined by ", ".
function allowed(path,    i, fits, list, m) {
    list = ""
    for (i = 1; i <= nr; i++) {
        fits = is_static(rp[i]) ? (rp[i] == path) : segs(rp[i], path)
        if (fits && index("," list ",", "," rm[i] ",") == 0) list = (list == "" ? rm[i] : list ", " rm[i])
    }
    return list
}

function build(    t, n, i, r, segstr, pattern, method, dup, j, lit, path, k) {
    split("GET POST PUT", METHODS, " ")
    split("a b users x1 files", LIT, " ")
    nr = 0
    for (t = 0; t < 14; t++) {
        n = 1 + below(4)
        segstr = ""
        for (i = 0; i < n; i++) {
            r = below(6)
            if (r == 0) seg = ":p" i
            else if (r == 1 && i > 0 && i + 1 == n) seg = "*rest"
            else seg = LIT[1 + below(5)]
            segstr = segstr "/" seg
        }
        pattern = (below(12) == 0) ? "/" : segstr
        method = METHODS[1 + below(3)]
        dup = 0
        if (is_static(pattern)) for (j = 1; j <= nr; j++) if (rm[j] == method && rp[j] == pattern) dup = 1
        if (dup) continue
        nr++; rm[nr] = method; rp[nr] = pattern; rid[nr] = nr - 1
    }
    dup = 0
    for (j = 1; j <= nr; j++) if (rm[j] == "GET" && rp[j] == "/") dup = 1
    if (!dup) { nr++; rm[nr] = "GET"; rp[nr] = "/"; rid[nr] = nr - 1 }
    nq = 4000
    for (k = 1; k <= nq; k++) {
        n = below(6)
        path = ""
        for (i = 0; i < n; i++) {
            r = below(10)
            if (r == 0) lit = ""
            else if (r == 1) lit = "%41"
            else if (r == 2) lit = "42"
            else if (r == 3) lit = "zzz"
            else lit = LIT[1 + below(5)]
            path = path "/" lit
        }
        if (path == "" || below(10) == 0) path = path "/"
        qm[k] = METHODS[1 + below(3)]
        qp[k] = path
    }
    na = 1200
}

BEGIN {
    build()
    if (mode == "gen") {
        for (i = 1; i <= nr; i++) print "R " rm[i] " " rp[i] " " rid[i]
        for (k = 1; k <= nq; k++) print "Q " qm[k] " " qp[k]
        for (k = 1; k <= na; k++) print "A " qp[k]
        exit 0
    }
    n = 0
    while ((getline line < out) > 0) { n++; got[n] = line }
    bad = 0
    if (n != nq + na) { print "DIFF the driver printed " n " lines, wanted " (nq + na); bad = 1 }
    for (k = 1; k <= nq && k <= n; k++) {
        id = find(qm[k], qp[k])
        nw = split(got[k], w, " ")
        ok = (w[1] == id)
        for (j = 1; ok && j <= NP; j++) if (w[2 * j] != PS[j] || w[2 * j + 1] != PE[j]) ok = 0
        if (!ok) { print "DIFF " qm[k] " " qp[k] ": wanted " id " (" NP " parameters), the driver said '" got[k] "'"; bad = 1 }
        if (id >= 0) matched++; else if (id == -2) wrong++; else none++
    }
    root = 0
    for (k = 1; k <= na && nq + k <= n; k++) {
        want = allowed(qp[k])
        if (got[nq + k] != want) { print "DIFF allowed for " qp[k] ": wanted '" want "', the driver said '" got[nq + k] "'"; bad = 1 }
        if (qp[k] == "/" && want != "") root++
    }
    if (root == 0) { print "DIFF the corpus must ask what the root route allows"; bad = 1 }
    if (matched <= 100 || wrong <= 100 || none <= 100) { print "DIFF the corpus must exercise every answer: matched " matched ", wrong method " wrong ", no route " none; bad = 1 }
    if (!bad) print "ok queries=" nq " matched=" matched " wrong_method=" wrong " no_route=" none " allowed_root=" root " routes=" nr
    exit bad
}
