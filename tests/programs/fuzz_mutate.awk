# fuzz_mutate.awk -- the mutator of the front-end fuzzer (tests/commands/fuzz.sh; docs/fuzzing.md section 2), in awk: it was `mutate` of crates/cancho/tests/fuzz.rs.
#
#     awk -v seed=N -v count=M -v dir=D -f fuzz_mutate.awk FILE...
#
# Every FILE is a seed. A mutant is a seed with a few token-level edits: a token deleted, duplicated, swapped with another, replaced by one from anywhere in the corpus, or a run of tokens
# spliced in from another file; half of the edits replace a token with another of the same kind (a name for a name, a number for a number, an operator for an operator), which keeps the program
# parsing far more often than a random edit does. The tokens of a mutant are joined by single spaces, so a comment is not a token (it would end the line). M mutants are written to D/mNNNNN.cho
# (NNNNN from 00001), and `seed N count` reproduce a run exactly: the generator is Park-Miller (16807 mod 2^31 - 1), whose products an awk number holds exactly.

function next_random() {
    state = (state * 16807) % 2147483647
    return state
}
function below(n) {
    if (n < 1) n = 1
    return next_random() % n
}
function kind_of(t,   c) {
    c = substr(t, 1, 1)
    if (c == "\"") return "string"
    if (c == "'") return "char"
    if (c ~ /[A-Za-z_]/) return "name"
    if (c ~ /[0-9]/) return "number"
    if (t ~ /^(\+|-|\*|\/|%|==|!=|<|>|<=|>=|&&|\|\||&|\||\^|<<|>>)$/) return "operator"
    return t
}
# The tokens of one line, appended to the file's list.
function tokenize(f, line,   rest, t) {
    rest = line
    while (rest != "") {
        if (match(rest, /^[ \t\r]+/)) { rest = substr(rest, RLENGTH + 1); continue }
        if (substr(rest, 1, 2) == "//") break
        if (match(rest, /^"([^"\\]|\\.)*"/) || match(rest, /^'([^'\\]|\\.)*'/) || match(rest, /^[A-Za-z_][A-Za-z0-9_]*/) || match(rest, /^[0-9][0-9A-Za-z_.]*/) || match(rest, /^(->|=>|::|==|!=|<=|>=|&&|\|\||<<|>>)/)) {
            t = substr(rest, 1, RLENGTH)
        } else {
            t = substr(rest, 1, 1)
        }
        rest = substr(rest, length(t) + 1)
        len[f]++
        text[f, len[f]] = t
        kind[f, len[f]] = kind_of(t)
    }
}
FNR == 1 { files++; f = files }
{ tokenize(f, $0) }
END {
    # The pool: every token of the corpus by kind; the corpus: the seeds that have tokens.
    for (f = 1; f <= files; f++) {
        if (len[f] == 0) continue
        usable[++nusable] = f
        for (i = 1; i <= len[f]; i++) {
            k = kind[f, i]
            pool[k, ++pool_n[k]] = text[f, i]
        }
    }
    state = (seed * 2 + 1) % 2147483646 + 1
    for (m = 1; m <= count; m++) {
        f = usable[below(nusable) + 1]
        n = len[f]
        for (i = 1; i <= n; i++) { cur[i] = text[f, i]; curkind[i] = kind[f, i] }
        edits = (below(4) == 0) ? 2 + below(2) : 1
        for (e = 0; e < edits; e++) {
            if (n == 0) break
            at = below(n) + 1
            op = below(10)
            if (op <= 4) {
                k = curkind[at]
                cur[at] = pool[k, below(pool_n[k]) + 1]
            } else if (op == 5) {
                for (i = at; i < n; i++) { cur[i] = cur[i + 1]; curkind[i] = curkind[i + 1] }
                n--
            } else if (op == 6) {
                for (i = n; i >= at; i--) { cur[i + 1] = cur[i]; curkind[i + 1] = curkind[i] }
                n++
            } else if (op == 7) {
                other = below(n) + 1
                t = cur[at]; cur[at] = cur[other]; cur[other] = t
                t = curkind[at]; curkind[at] = curkind[other]; curkind[other] = t
            } else {
                d = usable[below(nusable) + 1]
                from = below(len[d]) + 1
                span = 1 + below(len[d] - from + 1 < 8 ? len[d] - from + 1 : 8)
                for (i = n; i >= at; i--) { cur[i + span] = cur[i]; curkind[i + span] = curkind[i] }
                for (i = 0; i < span; i++) { cur[at + i] = text[d, from + i]; curkind[at + i] = kind[d, from + i] }
                n += span
            }
        }
        name = sprintf("%s/m%05d.cho", dir, m)
        line = ""
        for (i = 1; i <= n; i++) line = line (i > 1 ? " " : "") cur[i]
        print line > name
        close(name)
    }
}
