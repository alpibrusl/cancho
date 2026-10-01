module std.json;

// `std.json` -- parsing and writing JSON (RFC 8259), strictly, with no
// allocation the caller did not hand it.
//
// `docs/json.md` is the design; this header is the part to read before the
// code. There is no value tree, because a tree of owned boxes is a heap
// and a free list and a drop order for every document, and a request
// handler wants none of them. A document is parsed into a **tape**: one
// flat `[int]` the caller provides, three ints per node, that points back
// into the source bytes instead of copying them. Navigating it is index
// arithmetic; a string is not decoded until it is asked for, and an
// object's keys are never decoded at all unless they contain an escape.
// This is the shape simdjson settled on for the same reasons, reached here
// for another one: it is the shape that needs no ownership.
//
//     let tape = alloc_slice[a](json.tape_len(body), 0);   // or a heap slice
//     let nodes = json.parse(body, tape);
//     if nodes < 0 { ... json.error_message(json.error_code(nodes)) ... }
//     let user = json.get(body, tape, 0, "user");           // node 0 is the root
//     let age = json.to_int(body, tape, json.get(body, tape, user, "age"));
//
// A node is named by its **index**: node `i` is at `tape[3*i .. 3*i+3]`.
// An index of -1 is "no such node", and every accessor answers a sensible
// default for it, so a lookup chain does not need a check at every step.

import std.buffer;
import std.bytes;
import std.fmt;
import std.utf8;
import std.bignum;
import std.math;

// ---- the tape ------------------------------------------------------------
//
// `tape[3i]`   the kind in its low three bits, and a flag above them;
// `tape[3i+1]` and `tape[3i+2]`:
//
//     null, false, true   unused
//     int, float          the byte range of the number's text in the source
//     string              the byte range *inside* the quotes; the flag says
//                         whether it contains an escape (and so needs decoding)
//     array, object       the number of elements (pairs, for an object), and
//                         the index of the node after the whole subtree
//
// An object's children are its pairs in order, a string node for the key
// and then the value's subtree, so a key at node `k` has its value at
// `k + 1`, and the next key is at `skip(k + 1)`.

pub fn kind_null() -> [] int {
    return 0;
}

pub fn kind_false() -> [] int {
    return 1;
}

pub fn kind_true() -> [] int {
    return 2;
}

pub fn kind_int() -> [] int {
    return 3;
}

pub fn kind_float() -> [] int {
    return 4;
}

pub fn kind_string() -> [] int {
    return 5;
}

pub fn kind_array() -> [] int {
    return 6;
}

pub fn kind_object() -> [] int {
    return 7;
}

// How many ints the tape needs for `src`, always enough: a node is at least
// one byte of the source, so there are never more nodes than bytes.
pub fn tape_len[&s](src: &s [byte]) -> [] int {
    return 3 * (len(src) + 1);
}

// ---- errors --------------------------------------------------------------
//
// `parse` answers the number of nodes, or a negative number that says what
// went wrong and where: `0 - (position * 16 + code)`. `error_code` and
// `error_position` take it apart again.

pub fn error_code(result: int) -> [] int {
    return (0 - result) % 16;
}

pub fn error_position(result: int) -> [] int {
    return (0 - result) / 16;
}

fn err_syntax() -> [] int {
    return 1;
}

fn err_end() -> [] int {
    return 2;
}

fn err_control() -> [] int {
    return 3;
}

fn err_escape() -> [] int {
    return 4;
}

fn err_number() -> [] int {
    return 5;
}

fn err_utf8() -> [] int {
    return 6;
}

fn err_depth() -> [] int {
    return 7;
}

fn err_full() -> [] int {
    return 8;
}

fn err_trailing() -> [] int {
    return 9;
}

// The one-line reason for an error code, for a `400` body or a log.
pub fn error_message(code: int) -> [] &static [byte] {
    if code == 1 {
        return "unexpected character";
    }
    if code == 2 {
        return "unexpected end of input";
    }
    if code == 3 {
        return "unescaped control character in string";
    }
    if code == 4 {
        return "invalid escape sequence in string";
    }
    if code == 5 {
        return "malformed number";
    }
    if code == 6 {
        return "string is not valid UTF-8";
    }
    if code == 7 {
        return "nesting too deep";
    }
    if code == 8 {
        return "the tape is too small for this document";
    }
    if code == 9 {
        return "data after the end of the value";
    }
    return "unknown error";
}

// Nesting past this is refused: a request body that is 100,000 `[` in a
// row would otherwise recurse until the stack ran out, and the stack is the
// one resource a parser cannot be handed by its caller.
fn max_depth() -> [] int {
    return 128;
}

// ---- parsing -------------------------------------------------------------

fn is_ws(c: int) -> [] bool {
    return c == 32 || c == 9 || c == 10 || c == 13;
}

fn is_digit(c: int) -> [] bool {
    return c >= '0' && c <= '9';
}

fn skip_ws[&s, &q](src: &s [byte], st: &!q [int]) -> [] int {
    var p = st[0];
    while p < len(src) && is_ws(int_of(src[p])) {
        p = p + 1;
    }
    st[0] = p;
    return 0;
}

// Append a node; `-1` if the tape is full.
fn push_node[&t, &q](tape: &!t [int], st: &!q [int], w0: int, x: int, y: int) -> [] int {
    let n = st[1];
    if 3 * n + 3 > len(tape) {
        return 0 - 1;
    }
    tape[3 * n] = w0;
    tape[3 * n + 1] = x;
    tape[3 * n + 2] = y;
    st[1] = n + 1;
    return n;
}

// The value of a hex digit, or -1.
fn hex_value(c: int) -> [] int {
    if c >= '0' && c <= '9' {
        return c - '0';
    }
    if c >= 'a' && c <= 'f' {
        return c - 'a' + 10;
    }
    if c >= 'A' && c <= 'F' {
        return c - 'A' + 10;
    }
    return 0 - 1;
}

// `\uXXXX` at `p` (pointing at the backslash): the 16-bit value, or -1.
fn hex4[&s](src: &s [byte], p: int) -> [] int {
    if p + 6 > len(src) {
        return 0 - 1;
    }
    var v = 0;
    var k = 2;
    while k < 6 {
        let h = hex_value(int_of(src[p + k]));
        if h < 0 {
            return 0 - 1;
        }
        v = v * 16 + h;
        k = k + 1;
    }
    return v;
}

// How many bytes the escape at `p` takes -- 2, 6, or 12 for a surrogate pair
// -- or 0 if it is not a valid one. A lone surrogate is invalid: it has no
// UTF-8 encoding, and the alternative to refusing it is quietly writing
// something that is not a character.
fn escape_width[&s](src: &s [byte], p: int) -> [] int {
    if p + 1 >= len(src) {
        return 0;
    }
    let c = int_of(src[p + 1]);
    if c == '"' || c == '\\' || c == '/' || c == 'b' || c == 'f' || c == 'n' || c == 'r' || c == 't' {
        return 2;
    }
    if c != 'u' {
        return 0;
    }
    let v = hex4(src, p);
    if v < 0 {
        return 0;
    }
    if v >= 0xdc00 && v <= 0xdfff {
        return 0;
    }
    if v >= 0xd800 && v <= 0xdbff {
        if p + 12 > len(src) || int_of(src[p + 6]) != '\\' || int_of(src[p + 7]) != 'u' {
            return 0;
        }
        let low = hex4(src, p + 6);
        if low >= 0xdc00 && low <= 0xdfff {
            return 12;
        }
        return 0;
    }
    return 6;
}

fn string_node[&s, &t, &q](src: &s [byte], tape: &!t [int], st: &!q [int]) -> [] int {
    let n = len(src);
    let start = st[0] + 1;
    var p = start;
    var escaped = 0;
    var code = 0;
    var open = true;
    while open {
        if p >= n {
            code = err_end();
            open = false;
        } else {
            let c = int_of(src[p]);
            if c == '"' {
                open = false;
            } else if c < 32 {
                code = err_control();
                open = false;
            } else if c == '\\' {
                escaped = 1;
                let w = escape_width(src, p);
                if w == 0 {
                    code = err_escape();
                    open = false;
                } else {
                    p = p + w;
                }
            } else if c >= 128 {
                match utf8.decode(src, p) {
                    utf8.Step::Code(point, w) => {
                        p = p + w;
                    }
                    utf8.Step::Invalid(w) => {
                        code = err_utf8();
                        open = false;
                    }
                }
            } else {
                p = p + 1;
            }
        }
    }
    st[0] = p;
    if code != 0 {
        return code;
    }
    if push_node(tape, st, 5 + 8 * escaped, start, p) < 0 {
        return err_full();
    }
    st[0] = p + 1;
    return 0;
}

fn number_node[&s, &t, &q](src: &s [byte], tape: &!t [int], st: &!q [int]) -> [] int {
    let n = len(src);
    let start = st[0];
    var p = start;
    var fractional = 0;
    if int_of(src[p]) == '-' {
        p = p + 1;
    }
    if p >= n || !is_digit(int_of(src[p])) {
        st[0] = p;
        return err_number();
    }
    if int_of(src[p]) == '0' {
        p = p + 1;
    } else {
        while p < n && is_digit(int_of(src[p])) {
            p = p + 1;
        }
    }
    if p < n && int_of(src[p]) == '.' {
        fractional = 1;
        p = p + 1;
        if p >= n || !is_digit(int_of(src[p])) {
            st[0] = p;
            return err_number();
        }
        while p < n && is_digit(int_of(src[p])) {
            p = p + 1;
        }
    }
    if p < n && (int_of(src[p]) == 'e' || int_of(src[p]) == 'E') {
        fractional = 1;
        p = p + 1;
        if p < n && (int_of(src[p]) == '+' || int_of(src[p]) == '-') {
            p = p + 1;
        }
        if p >= n || !is_digit(int_of(src[p])) {
            st[0] = p;
            return err_number();
        }
        while p < n && is_digit(int_of(src[p])) {
            p = p + 1;
        }
    }
    if push_node(tape, st, 3 + fractional, start, p) < 0 {
        return err_full();
    }
    st[0] = p;
    return 0;
}

// `true`, `false` or `null`: the word at `st[0]`, and the node for it.
fn literal_node[&s, &t, &q, &w](src: &s [byte], tape: &!t [int], st: &!q [int], word: &w [byte], kind: int) -> [] int {
    let p = st[0];
    if p + len(word) > len(src) || !bytes.equal(src[p..p + len(word)], word) {
        return err_syntax();
    }
    if push_node(tape, st, kind, 0, 0) < 0 {
        return err_full();
    }
    st[0] = p + len(word);
    return 0;
}

fn array_node[&s, &t, &q](src: &s [byte], tape: &!t [int], st: &!q [int], depth: int) -> [] int {
    if depth >= max_depth() {
        return err_depth();
    }
    let me = push_node(tape, st, 6, 0, 0);
    if me < 0 {
        return err_full();
    }
    st[0] = st[0] + 1;
    skip_ws(src, st);
    var count = 0;
    var code = 0;
    if st[0] < len(src) && int_of(src[st[0]]) == ']' {
        st[0] = st[0] + 1;
    } else {
        var more = true;
        while more {
            code = value_node(src, tape, st, depth + 1);
            if code != 0 {
                more = false;
            } else {
                count = count + 1;
                skip_ws(src, st);
                if st[0] >= len(src) {
                    code = err_end();
                    more = false;
                } else {
                    let c = int_of(src[st[0]]);
                    if c == ',' {
                        st[0] = st[0] + 1;
                        skip_ws(src, st);
                    } else if c == ']' {
                        st[0] = st[0] + 1;
                        more = false;
                    } else {
                        code = err_syntax();
                        more = false;
                    }
                }
            }
        }
    }
    if code == 0 {
        tape[3 * me + 1] = count;
        tape[3 * me + 2] = st[1];
    }
    return code;
}

fn object_node[&s, &t, &q](src: &s [byte], tape: &!t [int], st: &!q [int], depth: int) -> [] int {
    if depth >= max_depth() {
        return err_depth();
    }
    let me = push_node(tape, st, 7, 0, 0);
    if me < 0 {
        return err_full();
    }
    st[0] = st[0] + 1;
    skip_ws(src, st);
    var count = 0;
    var code = 0;
    if st[0] < len(src) && int_of(src[st[0]]) == '}' {
        st[0] = st[0] + 1;
    } else {
        var more = true;
        while more {
            // A key: always a string.
            if st[0] >= len(src) {
                code = err_end();
            } else if int_of(src[st[0]]) != '"' {
                code = err_syntax();
            } else {
                code = string_node(src, tape, st);
            }
            if code == 0 {
                skip_ws(src, st);
                if st[0] >= len(src) {
                    code = err_end();
                } else if int_of(src[st[0]]) != ':' {
                    code = err_syntax();
                } else {
                    st[0] = st[0] + 1;
                    skip_ws(src, st);
                    code = value_node(src, tape, st, depth + 1);
                }
            }
            if code != 0 {
                more = false;
            } else {
                count = count + 1;
                skip_ws(src, st);
                if st[0] >= len(src) {
                    code = err_end();
                    more = false;
                } else {
                    let c = int_of(src[st[0]]);
                    if c == ',' {
                        st[0] = st[0] + 1;
                        skip_ws(src, st);
                    } else if c == '}' {
                        st[0] = st[0] + 1;
                        more = false;
                    } else {
                        code = err_syntax();
                        more = false;
                    }
                }
            }
        }
    }
    if code == 0 {
        tape[3 * me + 1] = count;
        tape[3 * me + 2] = st[1];
    }
    return code;
}

fn value_node[&s, &t, &q](src: &s [byte], tape: &!t [int], st: &!q [int], depth: int) -> [] int {
    if st[0] >= len(src) {
        return err_end();
    }
    let c = int_of(src[st[0]]);
    if c == '{' {
        return object_node(src, tape, st, depth);
    }
    if c == '[' {
        return array_node(src, tape, st, depth);
    }
    if c == '"' {
        return string_node(src, tape, st);
    }
    if c == '-' || is_digit(c) {
        return number_node(src, tape, st);
    }
    if c == 't' {
        return literal_node(src, tape, st, "true", 2);
    }
    if c == 'f' {
        return literal_node(src, tape, st, "false", 1);
    }
    if c == 'n' {
        return literal_node(src, tape, st, "null", 0);
    }
    return err_syntax();
}

// Parse the document in `src` into `tape`. Answers the number of nodes
// (node 0 is the root), or a negative error (`error_code`,
// `error_position`).
//
// Strict: RFC 8259 and nothing else -- no comments, no trailing commas, no
// leading zeros, no `NaN`, no single quotes -- and every string must be
// valid UTF-8 with no unescaped control character and no lone surrogate. A
// parser that accepts what it should refuse is the one two services
// disagree about, and disagreement about what a body *says* is where
// request-smuggling bugs live.
pub fn parse[&s, &t](src: &s [byte], tape: &!t [int]) -> [] int {
    var result = 0;
    region a {
        // `st[0]` the position, `st[1]` the node count.
        let st = alloc_slice[a](2, 0);
        skip_ws(src, st);
        var code = value_node(src, tape, st, 0);
        if code == 0 {
            skip_ws(src, st);
            if st[0] != len(src) {
                code = err_trailing();
            }
        }
        if code == 0 {
            result = st[1];
        } else {
            result = 0 - (st[0] * 16 + code);
        }
    }
    return result;
}

// ---- reading the tape ----------------------------------------------------

// The kind of node `i`, or -1 if `i` is not a node.
pub fn kind[&t](tape: &t [int], i: int) -> [] int {
    if i < 0 || 3 * i + 2 >= len(tape) {
        return 0 - 1;
    }
    return tape[3 * i] & 7;
}

pub fn is_null[&t](tape: &t [int], i: int) -> [] bool {
    return kind(tape, i) == 0;
}

pub fn is_bool[&t](tape: &t [int], i: int) -> [] bool {
    let k = kind(tape, i);
    return k == 1 || k == 2;
}

pub fn is_int[&t](tape: &t [int], i: int) -> [] bool {
    return kind(tape, i) == 3;
}

pub fn is_number[&t](tape: &t [int], i: int) -> [] bool {
    let k = kind(tape, i);
    return k == 3 || k == 4;
}

pub fn is_string[&t](tape: &t [int], i: int) -> [] bool {
    return kind(tape, i) == 5;
}

pub fn is_array[&t](tape: &t [int], i: int) -> [] bool {
    return kind(tape, i) == 6;
}

pub fn is_object[&t](tape: &t [int], i: int) -> [] bool {
    return kind(tape, i) == 7;
}

// The name of the kind, for an error message.
pub fn kind_name[&t](tape: &t [int], i: int) -> [] &static [byte] {
    let k = kind(tape, i);
    if k == 0 {
        return "null";
    }
    if k == 1 || k == 2 {
        return "boolean";
    }
    if k == 3 || k == 4 {
        return "number";
    }
    if k == 5 {
        return "string";
    }
    if k == 6 {
        return "array";
    }
    if k == 7 {
        return "object";
    }
    return "nothing";
}

// How many elements an array has, or pairs an object has; 0 for anything else.
pub fn count[&t](tape: &t [int], i: int) -> [] int {
    let k = kind(tape, i);
    if k == 6 || k == 7 {
        return tape[3 * i + 1];
    }
    return 0;
}

// The index of the node after node `i` and everything inside it: the
// node's own successor for a scalar, past the whole subtree for a
// container. Walking the children of a container is `j = i + 1`, then
// `j = skip(tape, j)` once per child.
pub fn skip[&t](tape: &t [int], i: int) -> [] int {
    let k = kind(tape, i);
    if k == 6 || k == 7 {
        return tape[3 * i + 2];
    }
    return i + 1;
}

// The `n`th element of the array at node `arr`, or -1. Linear in `n`: the
// tape is a flat list, and reaching element `n` means stepping over the
// `n` subtrees before it.
pub fn at[&t](tape: &t [int], arr: int, n: int) -> [] int {
    if kind(tape, arr) != 6 || n < 0 || n >= tape[3 * arr + 1] {
        return 0 - 1;
    }
    var j = arr + 1;
    var left = n;
    while left > 0 {
        j = skip(tape, j);
        left = left - 1;
    }
    return j;
}

// The decoded code point at `p` in a string's body, and where the next one
// starts. The body was validated by `parse`, so this does not check.
fn next_point[&s](src: &s [byte], p: int) -> [] (int, int) {
    let c = int_of(src[p]);
    if c != '\\' {
        if c < 128 {
            return (c, p + 1);
        }
        match utf8.decode(src, p) {
            utf8.Step::Code(point, w) => {
                return (point, p + w);
            }
            utf8.Step::Invalid(w) => {
                return (0xfffd, p + w);
            }
        }
    }
    let e = int_of(src[p + 1]);
    if e == 'n' {
        return (10, p + 2);
    }
    if e == 't' {
        return (9, p + 2);
    }
    if e == 'r' {
        return (13, p + 2);
    }
    if e == 'b' {
        return (8, p + 2);
    }
    if e == 'f' {
        return (12, p + 2);
    }
    if e != 'u' {
        // `"`, `\` or `/`: itself.
        return (e, p + 2);
    }
    let high = hex4(src, p);
    if high >= 0xd800 && high <= 0xdbff {
        let low = hex4(src, p + 6);
        return (0x10000 + (high - 0xd800) * 1024 + (low - 0xdc00), p + 12);
    }
    return (high, p + 6);
}

// Does the string at node `i` hold exactly `text`? Compared as decoded
// text, so `"café"` equals `"café"`.
pub fn string_equals[&s, &t, &k](src: &s [byte], tape: &t [int], i: int, text: &k [byte]) -> [] bool {
    if kind(tape, i) != 5 {
        return false;
    }
    let start = tape[3 * i + 1];
    let end = tape[3 * i + 2];
    if tape[3 * i] >> 3 == 0 {
        return bytes.equal(src[start..end], text);
    }
    var p = start;
    var q = 0;
    while p < end {
        if q >= len(text) {
            return false;
        }
        let (point, np) = next_point(src, p);
        match utf8.decode(text, q) {
            utf8.Step::Code(other, w) => {
                if other != point {
                    return false;
                }
                q = q + w;
            }
            utf8.Step::Invalid(w) => {
                return false;
            }
        }
        p = np;
    }
    return q == len(text);
}

// The value for `key` in the object at node `obj`, or -1. The first of any
// duplicate keys wins -- the document's order, not an arbitrary one.
pub fn get[&s, &t, &k](src: &s [byte], tape: &t [int], obj: int, key: &k [byte]) -> [] int {
    if kind(tape, obj) != 7 {
        return 0 - 1;
    }
    var j = obj + 1;
    var left = tape[3 * obj + 1];
    while left > 0 {
        if string_equals(src, tape, j, key) {
            return j + 1;
        }
        j = skip(tape, j + 1);
        left = left - 1;
    }
    return 0 - 1;
}

// `true` for a `true` node; `false` for anything else, including no node.
pub fn to_bool[&t](tape: &t [int], i: int) -> [] bool {
    return kind(tape, i) == 2;
}

// ---- numbers -------------------------------------------------------------

// The integer in `src[start..end]`, saturating, and whether it saturated.
// Accumulated *downward*, so that the one magnitude `int` has an extra of
// (-2^63) is reachable without an intermediate that overflows.
fn parse_int[&s](src: &s [byte], start: int, end: int) -> [] (int, bool) {
    let min = 0 - 9223372036854775807 - 1;
    var p = start;
    var negative = false;
    if int_of(src[p]) == '-' {
        negative = true;
        p = p + 1;
    }
    var acc = 0;
    while p < end {
        let d = int_of(src[p]) - '0';
        if acc < (min + d) / 10 {
            if negative {
                return (min, true);
            }
            return (9223372036854775807, true);
        }
        acc = acc * 10 - d;
        p = p + 1;
    }
    if negative {
        return (acc, false);
    }
    if acc == min {
        return (9223372036854775807, true);
    }
    return (0 - acc, false);
}

// The value of an integer node, saturating at the ends of `int`; for a
// `float` node, truncated toward zero and saturated the same way; 0 for
// anything else (including no node). `fits_int` says whether it was exact.
pub fn to_int[&s, &t](src: &s [byte], tape: &t [int], i: int) -> [] int {
    let k = kind(tape, i);
    if k == 3 {
        let (v, saturated) = parse_int(src, tape[3 * i + 1], tape[3 * i + 2]);
        return v;
    }
    if k == 4 {
        let x = to_float(src, tape, i);
        if is_nan(x) {
            return 0;
        }
        if x >= 9223372036854775807.0 {
            return 9223372036854775807;
        }
        if x <= 0.0 - 9223372036854775807.0 {
            return 0 - 9223372036854775807 - 1;
        }
        return truncate(x);
    }
    return 0;
}

// Is the node an integer that `to_int` returns exactly: a `3`, not a
// `3.0`, not `1e3`, and not wider than 64 bits?
pub fn fits_int[&s, &t](src: &s [byte], tape: &t [int], i: int) -> [] bool {
    if kind(tape, i) != 3 {
        return false;
    }
    let (v, saturated) = parse_int(src, tape[3 * i + 1], tape[3 * i + 2]);
    return !saturated;
}

fn add_small[&a](a: &!a [int], value: int) -> [] int {
    var carry = value;
    var i = 0;
    while carry > 0 && i < len(a) {
        let sum = a[i] + carry;
        a[i] = sum & 0xffffffff;
        carry = sum >> 32;
        i = i + 1;
    }
    return carry;
}

// `a >> 1`.
fn halve[&a](a: &!a [int]) -> [] int {
    var i = 0;
    while i < len(a) {
        var high = 0;
        if i + 1 < len(a) {
            high = (a[i + 1] & 1) << 31;
        }
        a[i] = a[i] >> 1 | high;
        i = i + 1;
    }
    return 0;
}

fn is_zero[&a](a: &a [int]) -> [] bool {
    var i = 0;
    while i < len(a) {
        if a[i] != 0 {
            return false;
        }
        i = i + 1;
    }
    return true;
}

// How many bits the number takes: 0 for zero.
fn bit_length[&a](a: &a [int]) -> [] int {
    var i = len(a) - 1;
    while i >= 0 && a[i] == 0 {
        i = i - 1;
    }
    if i < 0 {
        return 0;
    }
    var bits = 0;
    var top = a[i];
    while top > 0 {
        top = top >> 1;
        bits = bits + 1;
    }
    return 32 * i + bits;
}

// `a = a * 10^k`, nine digits at a time: a limb times 10^9 is under 2^62.
fn mul_pow10[&a](a: &!a [int], k: int) -> [] int {
    var left = k;
    while left >= 9 {
        bignum.mul_small(a, 1000000000);
        left = left - 9;
    }
    var scale = 1;
    while left > 0 {
        scale = scale * 10;
        left = left - 1;
    }
    bignum.mul_small(a, scale);
    return 0;
}

// Scan the decimal text `src[start..end]` (a number node's text, already
// validated) into `out`:
//
//     out[0]  1 if negative
//     out[1]  the first up to 18 significant digits, as an integer
//     out[2]  how many significant digits were kept (up to `cap`)
//     out[3]  the power of ten they are to be scaled by: value = kept * 10^out[3]
//     out[4]  1 if a nonzero digit was dropped past `cap` -- the value is then
//             strictly between `kept * 10^e` and `(kept + 1) * 10^e`
//     out[5]  how many significant digits there were in all, kept or not
//
// and, if `big` is not empty, the kept digits as a bignum in `big`. Leading
// zeros are not significant; a digit dropped before the point still moves the
// exponent up, and one after it does not.
fn scan_decimal[&s, &b, &o](src: &s [byte], start: int, end: int, cap: int, big: &!b [int], out: &!o [int]) -> [] int {
    var p = start;
    var negative = 0;
    if int_of(src[p]) == '-' {
        negative = 1;
        p = p + 1;
    }
    var m = 0;
    var kept = 0;
    var e10 = 0;
    var sticky = 0;
    var total = 0;
    var seen_point = false;
    var chunk = 0;
    var in_chunk = 0;
    while p < end && (is_digit(int_of(src[p])) || int_of(src[p]) == '.') {
        let c = int_of(src[p]);
        if c == '.' {
            seen_point = true;
        } else {
            let d = c - '0';
            if kept == 0 && d == 0 {
                if seen_point {
                    e10 = e10 - 1;
                }
            } else if kept < cap {
                total = total + 1;
                if kept < 18 {
                    m = m * 10 + d;
                }
                kept = kept + 1;
                if seen_point {
                    e10 = e10 - 1;
                }
                if len(big) > 0 {
                    chunk = chunk * 10 + d;
                    in_chunk = in_chunk + 1;
                    if in_chunk == 9 {
                        mul_pow10(big, 9);
                        add_small(big, chunk);
                        chunk = 0;
                        in_chunk = 0;
                    }
                }
            } else {
                total = total + 1;
                if d != 0 {
                    sticky = 1;
                }
                if !seen_point {
                    e10 = e10 + 1;
                }
            }
        }
        p = p + 1;
    }
    if len(big) > 0 && in_chunk > 0 {
        mul_pow10(big, in_chunk);
        add_small(big, chunk);
    }
    // The exponent, saturated: past 100,000 the value is 0 or infinity
    // however many digits there are, and a longer exponent must not
    // overflow `int` on its way there.
    if p < end {
        p = p + 1;
        var exponent_negative = false;
        if int_of(src[p]) == '-' {
            exponent_negative = true;
            p = p + 1;
        } else if int_of(src[p]) == '+' {
            p = p + 1;
        }
        var exponent = 0;
        while p < end {
            if exponent < 100000 {
                exponent = exponent * 10 + (int_of(src[p]) - '0');
            }
            p = p + 1;
        }
        if exponent_negative {
            e10 = e10 - exponent;
        } else {
            e10 = e10 + exponent;
        }
    }
    out[0] = negative;
    out[1] = m;
    out[2] = kept;
    out[3] = e10;
    out[4] = sticky;
    out[5] = total;
    return 0;
}

fn signed(x: float, negative: bool) -> [] float {
    if negative {
        return x * (0.0 - 1.0);
    }
    return x;
}

// `10^k` for `0 <= k <= 22`: every power up to there is a float exactly, and
// so is each product on the way.
fn exact_pow10(k: int) -> [] float {
    var x = 1.0;
    var n = 0;
    while n < k {
        x = x * 10.0;
        n = n + 1;
    }
    return x;
}

// The correctly rounded value of a decimal too hard for the fast path: more
// than 15 digits, or a power of ten outside 10^-22..10^22. Exact arithmetic
// all the way: the digits and the power of ten are bignums, the quotient of
// the two is taken to 57 bits by long division, and the rounding is done on
// those bits with the remainder as the sticky bit -- so a decimal that is
// *exactly* halfway between two floats goes to the even one and anything
// either side of it does not, which no floating-point shortcut can promise.
// A few hundred microseconds for a 17-digit number, which is why the fast
// path exists.
fn slow_decimal[&s](src: &s [byte], start: int, end: int, digits: int, scale: int) -> [] float {
    var result = 0.0;
    region a {
        // Limbs enough for the digits, the power of ten, and the 57-bit
        // quotient scaling on top: four bits a decimal digit is generous (it is
        // 3.33), and a 17-digit number needs a dozen limbs where a worst-case
        // 767-digit one needs a few hundred.
        //
        // The power of ten is capped: a decimal whose exponent is past
        // about 1,100 is decided as 0 or infinity below without touching a
        // bignum, and `1e999999999999` must not ask the arena for the room
        // its exponent would need.
        let span = digits + math.min(math.abs(scale), 1200) + digits;
        let limbs = (4 * span + 200) / 32 + 2;
        let n = alloc_slice[a](limbs, 0);
        let q = alloc_slice[a](limbs, 0);
        let aux = alloc_slice[a](6, 0);
        // 767 significant digits are enough to decide any rounding: that is
        // the most a double's halfway point can have.
        scan_decimal(src, start, end, 767, n, aux);
        let negative = aux[0] == 1;
        let kept = aux[2];
        var e10 = aux[3];
        let decimal_exponent = kept + e10;
        if kept == 0 {
            result = signed(0.0, negative);
        } else if decimal_exponent > 310 {
            result = signed(1.0 / 0.0, negative);
        } else if decimal_exponent < 0 - 325 {
            result = signed(0.0, negative);
        } else {
            // A dropped nonzero digit becomes a final `1`: strictly more than
            // the digits kept and strictly less than one more in the last
            // place, which is all rounding can need to know about it.
            if aux[4] == 1 {
                bignum.mul_small(n, 10);
                add_small(n, 1);
                e10 = e10 - 1;
            }
            // value = n * 10^e10 = N / Q.
            if e10 > 0 {
                mul_pow10(n, e10);
            }
            bignum.set(q, 1);
            if e10 < 0 {
                mul_pow10(q, 0 - e10);
            }
            // Scale one side by a power of two so that N / Q lies in
            // (2^55, 2^57): the quotient then has 56 or 57 bits, two or three
            // more than a float keeps and enough to round on.
            let shift = 56 - (bit_length(n) - bit_length(q));
            if shift >= 0 {
                bignum.shift_left(n, shift);
            } else {
                bignum.shift_left(q, 0 - shift);
            }
            let e_q = 0 - shift;
            bignum.shift_left(q, 56);
            // Binary long division, most significant bit first.
            var quotient = 0;
            var k = 56;
            while k >= 0 {
                quotient = quotient * 2;
                if bignum.compare(n, q) >= 0 {
                    bignum.subtract(n, q);
                    quotient = quotient + 1;
                }
                halve(q);
                k = k - 1;
            }
            let inexact = !is_zero(n);
            let bits = bit_length_of(quotient);
            let top = bits - 1 + e_q;
            if top > 1023 {
                result = signed(1.0 / 0.0, negative);
            } else {
                // Keep 53 bits, or fewer where the result is subnormal and
                // its last place is fixed at 2^-1074.
                var lsb = top - 52;
                if lsb < 0 - 1074 {
                    lsb = 0 - 1074;
                }
                let drop = lsb - e_q;
                if drop > 60 {
                    result = signed(0.0, negative);
                } else {
                    var kept_bits = quotient >> drop;
                    let rest = quotient - (kept_bits << drop);
                    let half = 1 << drop - 1;
                    if rest > half || rest == half && (inexact || kept_bits & 1 == 1) {
                        kept_bits = kept_bits + 1;
                    }
                    result = signed(math.ldexp(float_of(kept_bits), lsb), negative);
                }
            }
        }
    }
    return result;
}

// Bits in a non-negative `int` (up to 62).
fn bit_length_of(v: int) -> [] int {
    var bits = 0;
    var rest = v;
    while rest > 0 {
        rest = rest >> 1;
        bits = bits + 1;
    }
    return bits;
}

// The value of a number node as a `float`, correctly rounded: the nearest
// float to the decimal text, a tie going to the even one, exactly as
// `strtod` and every conforming parser answers. 0.0 for anything but a
// number. A number past the float range is +-infinity, and one below it is
// +-0.0 -- JSON has no such values, but the number it spells does.
pub fn to_float[&s, &t](src: &s [byte], tape: &t [int], i: int) -> [] float {
    let k = kind(tape, i);
    if k != 3 && k != 4 {
        return 0.0;
    }
    let start = tape[3 * i + 1];
    let end = tape[3 * i + 2];
    var answer = 0.0;
    var decided = false;
    var digits = 0;
    var scale = 0;
    region a {
        let aux = alloc_slice[a](6, 0);
        let none = alloc_slice[a](0, 0);
        scan_decimal(src, start, end, 18, none, aux);
        let negative = aux[0] == 1;
        let m = aux[1];
        let e10 = aux[3];
        digits = aux[5];
        if digits > 767 {
            digits = 767;
        }
        scale = e10;
        if aux[2] == 0 {
            answer = signed(0.0, negative);
            decided = true;
        } else if aux[4] == 0 && m <= 9007199254740992 && e10 >= 0 - 22 && e10 <= 22 {
            // Clinger's fast path: both operands exact, one correctly rounded
            // operation.
            if e10 < 0 {
                answer = signed(float_of(m) / exact_pow10(0 - e10), negative);
            } else {
                answer = signed(float_of(m) * exact_pow10(e10), negative);
            }
            decided = true;
        }
    }
    if decided {
        return answer;
    }
    return slow_decimal(src, start, end, digits, scale);
}

// ---- strings -------------------------------------------------------------

// Append `point`, UTF-8 encoded, to `out` at `at`: where the next byte goes,
// or -1 if there is no room.
fn put_point[&o](out: &!o [byte], at: int, point: int) -> [] int {
    if point < 128 {
        if at + 1 > len(out) {
            return 0 - 1;
        }
        out[at] = byte_of(point);
        return at + 1;
    }
    if point < 2048 {
        if at + 2 > len(out) {
            return 0 - 1;
        }
        out[at] = byte_of(192 | point >> 6);
        out[at + 1] = byte_of(128 | point & 63);
        return at + 2;
    }
    if point < 65536 {
        if at + 3 > len(out) {
            return 0 - 1;
        }
        out[at] = byte_of(224 | point >> 12);
        out[at + 1] = byte_of(128 | point >> 6 & 63);
        out[at + 2] = byte_of(128 | point & 63);
        return at + 3;
    }
    if at + 4 > len(out) {
        return 0 - 1;
    }
    out[at] = byte_of(240 | point >> 18);
    out[at + 1] = byte_of(128 | point >> 12 & 63);
    out[at + 2] = byte_of(128 | point >> 6 & 63);
    out[at + 3] = byte_of(128 | point & 63);
    return at + 4;
}

// How many bytes a decoded point takes.
fn point_width(point: int) -> [] int {
    if point < 128 {
        return 1;
    }
    if point < 2048 {
        return 2;
    }
    if point < 65536 {
        return 3;
    }
    return 4;
}

// Does the string at node `i` have no escapes, so that its bytes in the
// source *are* its value?
pub fn string_plain[&t](tape: &t [int], i: int) -> [] bool {
    return kind(tape, i) == 5 && tape[3 * i] >> 3 == 0;
}

// The bytes of the string at node `i`, as they are written between the
// quotes. For a string with no escapes (`string_plain`) that is the value,
// borrowed from the source with no copy; for one with escapes it is the
// undecoded text, and `string_into` is the way to the value. An empty slice
// for a node that is not a string.
pub fn string_view[&s, &t](src: &s [byte], tape: &t [int], i: int) -> [] &s [byte] {
    if kind(tape, i) != 5 {
        return src[0..0];
    }
    return src[tape[3 * i + 1]..tape[3 * i + 2]];
}

// How many bytes the string at node `i` is once decoded, or -1 if it is
// not a string.
pub fn string_length[&s, &t](src: &s [byte], tape: &t [int], i: int) -> [] int {
    if kind(tape, i) != 5 {
        return 0 - 1;
    }
    let start = tape[3 * i + 1];
    let end = tape[3 * i + 2];
    if tape[3 * i] >> 3 == 0 {
        return end - start;
    }
    var total = 0;
    var p = start;
    while p < end {
        let (point, np) = next_point(src, p);
        total = total + point_width(point);
        p = np;
    }
    return total;
}

// Decode the string at node `i` into `out`, as UTF-8: the number of bytes
// written, or -1 if it is not a string or `out` is too short (and then what
// was written is not meaningful).
pub fn string_into[&s, &t, &o](src: &s [byte], tape: &t [int], i: int, out: &!o [byte]) -> [] int {
    if kind(tape, i) != 5 {
        return 0 - 1;
    }
    let start = tape[3 * i + 1];
    let end = tape[3 * i + 2];
    var at = 0;
    var p = start;
    while p < end {
        let (point, np) = next_point(src, p);
        at = put_point(out, at, point);
        if at < 0 {
            return 0 - 1;
        }
        p = np;
    }
    return at;
}

// ---- writing -------------------------------------------------------------
//
// A `Writer` owns the buffer it appends to and puts the commas, colons and
// quotes in itself, so that the only way to write bad JSON is to call things
// in an order JSON does not allow -- and that is a trap, not a malformed
// body: a value inside an object with no key before it, a key outside an
// object, a close that does not match its open, nesting past 60. Those are
// the caller's bugs, and a service that sends `{"a":}` to its clients has
// not handled them.
//
//     var w = json.writer(heap, 256);
//     w = json.begin_object(heap, w);
//     w = json.put_key(heap, w, "name");
//     w = json.put_string(heap, w, name);
//     w = json.put_key(heap, w, "scores");
//     w = json.begin_array(heap, w);
//     w = json.put_int(heap, w, 3);
//     w = json.end_array(heap, w);
//     w = json.end_object(heap, w);
//     // json.bytes(&w) is the document; json.drop(heap, w) ends it.
//
// Like `std.buffer`, which it is built on, a `Writer` is moved through every
// call -- `w = json.put_int(heap, w, 3)` -- because growing it replaces the
// allocation under it.

pub res struct Writer {
    out: buffer.Buffer,
    // How many containers are open.
    depth: int,
    // Bit `d`: the container at depth `d` already has an element.
    has: int,
    // Bit `d`: the container at depth `d` is an object.
    objects: int,
    // A key has been written and its value has not.
    pending: bool,
}

pub fn writer[&h](heap: &!h Heap, capacity: int) -> [heap] Writer {
    return Writer { out: buffer.empty(heap, capacity), depth: 0, has: 0, objects: 0, pending: false };
}

// End the writer and free its buffer; answers how many bytes it held.
pub fn drop[&h](heap: &!h Heap, w: Writer) -> [heap] int {
    let Writer { out, depth, has, objects, pending } = w;
    return buffer.drop(heap, out);
}

// The document so far, borrowed from the writer.
pub fn bytes[&w](w: &w Writer) -> [] &w [byte] {
    return buffer.bytes(w.out);
}

// Give up the writer and keep its buffer. A document that is not finished --
// an open container, a key with no value -- is a trap.
pub fn finish(w: Writer) -> [] buffer.Buffer {
    let Writer { out, depth, has, objects, pending } = w;
    if depth != 0 || pending {
        let unfinished = trap();
    }
    return out;
}

// What goes before a value: nothing after a key, a comma after the first
// element of a container, and it is a trap to put a bare value in an object.
fn before_value[&h](heap: &!h Heap, w: Writer) -> [heap] Writer {
    let Writer { out, depth, has, objects, pending } = w;
    if pending {
        return Writer { out: out, depth: depth, has: has, objects: objects, pending: false };
    }
    var o = out;
    var h = has;
    if depth > 0 {
        let bit = 1 << depth;
        if objects & bit != 0 {
            let keyless = trap();
        }
        if h & bit != 0 {
            o = buffer.push(heap, o, byte_of(','));
        }
        h = h | bit;
    }
    return Writer { out: o, depth: depth, has: h, objects: objects, pending: false };
}

fn open_container[&h](heap: &!h Heap, w: Writer, opener: int, object: bool) -> [heap] Writer {
    let prepared = before_value(heap, w);
    let Writer { out, depth, has, objects, pending } = prepared;
    if depth >= 60 {
        let too_deep = trap();
    }
    let d = depth + 1;
    var kinds = objects & ~(1 << d);
    if object {
        kinds = objects | 1 << d;
    }
    return Writer { out: buffer.push(heap, out, byte_of(opener)), depth: d, has: has & ~(1 << d), objects: kinds, pending: false };
}

fn close_container[&h](heap: &!h Heap, w: Writer, closer: int, object: bool) -> [heap] Writer {
    let Writer { out, depth, has, objects, pending } = w;
    var is_object = false;
    if depth > 0 {
        is_object = objects & 1 << depth != 0;
    }
    if depth == 0 || pending || is_object != object {
        let mismatched = trap();
    }
    return Writer { out: buffer.push(heap, out, byte_of(closer)), depth: depth - 1, has: has, objects: objects, pending: false };
}

pub fn begin_object[&h](heap: &!h Heap, w: Writer) -> [heap] Writer {
    return open_container(heap, w, '{', true);
}

pub fn end_object[&h](heap: &!h Heap, w: Writer) -> [heap] Writer {
    return close_container(heap, w, '}', true);
}

pub fn begin_array[&h](heap: &!h Heap, w: Writer) -> [heap] Writer {
    return open_container(heap, w, '[', false);
}

pub fn end_array[&h](heap: &!h Heap, w: Writer) -> [heap] Writer {
    return close_container(heap, w, ']', false);
}

fn hex_digit(v: int) -> [] int {
    let digits = "0123456789abcdef";
    return int_of(digits[v]);
}

// `text` as a JSON string, quotes included. Bytes that are not valid UTF-8
// are written as U+FFFD (`\ufffd`): the output is always a valid document,
// whatever was handed in. A run of bytes that need no escape is appended in
// one piece.
fn write_string[&h, &r](heap: &!h Heap, b: buffer.Buffer, text: &r [byte]) -> [heap] buffer.Buffer {
    var out = buffer.push(heap, b, byte_of('"'));
    var run = 0;
    var p = 0;
    while p < len(text) {
        let c = int_of(text[p]);
        if c >= 32 && c != '"' && c != '\\' && c < 128 {
            p = p + 1;
        } else if c >= 128 {
            match utf8.decode(text, p) {
                utf8.Step::Code(point, w) => {
                    p = p + w;
                }
                utf8.Step::Invalid(w) => {
                    out = buffer.append(heap, out, text[run..p]);
                    out = buffer.append(heap, out, "\\ufffd");
                    p = p + w;
                    run = p;
                }
            }
        } else {
            out = buffer.append(heap, out, text[run..p]);
            out = buffer.push(heap, out, byte_of('\\'));
            if c == '"' || c == '\\' {
                out = buffer.push(heap, out, byte_of(c));
            } else if c == 10 {
                out = buffer.push(heap, out, byte_of('n'));
            } else if c == 13 {
                out = buffer.push(heap, out, byte_of('r'));
            } else if c == 9 {
                out = buffer.push(heap, out, byte_of('t'));
            } else if c == 8 {
                out = buffer.push(heap, out, byte_of('b'));
            } else if c == 12 {
                out = buffer.push(heap, out, byte_of('f'));
            } else {
                out = buffer.append(heap, out, "u00");
                out = buffer.push(heap, out, byte_of(hex_digit(c >> 4)));
                out = buffer.push(heap, out, byte_of(hex_digit(c & 15)));
            }
            p = p + 1;
            run = p;
        }
    }
    out = buffer.append(heap, out, text[run..p]);
    return buffer.push(heap, out, byte_of('"'));
}

// An object key. Only inside an object, and only where a key is due.
pub fn put_key[&h, &k](heap: &!h Heap, w: Writer, name: &k [byte]) -> [heap] Writer {
    let Writer { out, depth, has, objects, pending } = w;
    if depth == 0 || objects & 1 << depth == 0 || pending {
        let misplaced = trap();
    }
    var o = out;
    if has & 1 << depth != 0 {
        o = buffer.push(heap, o, byte_of(','));
    }
    o = write_string(heap, o, name);
    o = buffer.push(heap, o, byte_of(':'));
    return Writer { out: o, depth: depth, has: has | 1 << depth, objects: objects, pending: true };
}

fn put_raw[&h, &r](heap: &!h Heap, w: Writer, text: &r [byte]) -> [heap] Writer {
    let prepared = before_value(heap, w);
    let Writer { out, depth, has, objects, pending } = prepared;
    return Writer { out: buffer.append(heap, out, text), depth: depth, has: has, objects: objects, pending: pending };
}

pub fn put_null[&h](heap: &!h Heap, w: Writer) -> [heap] Writer {
    return put_raw(heap, w, "null");
}

pub fn put_bool[&h](heap: &!h Heap, w: Writer, value: bool) -> [heap] Writer {
    if value {
        return put_raw(heap, w, "true");
    }
    return put_raw(heap, w, "false");
}

pub fn put_int[&h](heap: &!h Heap, w: Writer, n: int) -> [heap] Writer {
    let prepared = before_value(heap, w);
    let Writer { out, depth, has, objects, pending } = prepared;
    var o = out;
    if n < 0 {
        o = buffer.push(heap, o, byte_of('-'));
        if n == 0 - 9223372036854775807 - 1 {
            // Its magnitude is not an `int`.
            o = buffer.append(heap, o, "9223372036854775808");
        } else {
            o = buffer.push_nat(heap, o, 0 - n);
        }
    } else {
        o = buffer.push_nat(heap, o, n);
    }
    return Writer { out: o, depth: depth, has: has, objects: objects, pending: pending };
}

pub fn put_string[&h, &r](heap: &!h Heap, w: Writer, text: &r [byte]) -> [heap] Writer {
    let prepared = before_value(heap, w);
    let Writer { out, depth, has, objects, pending } = prepared;
    return Writer { out: write_string(heap, out, text), depth: depth, has: has, objects: objects, pending: pending };
}

// `x` in the shortest decimal that reads back as the same float, laid out
// the way JavaScript and Python lay it out: positional from 1e-6 up to 1e21,
// scientific outside. A whole number keeps a `.0`, so it reads back as a
// float and not an integer. `out` needs 40 bytes; answers how many were used.
fn float_text[&o](out: &!o [byte], x: float) -> [] int {
    var written = 0;
    region a {
        let raw = alloc_slice[a](32, byte_of(0));
        let digits = alloc_slice[a](24, byte_of(0));
        let negative = bits_of(x) < 0;
        var at = 0;
        if negative {
            out[at] = byte_of('-');
            at = at + 1;
        }
        if x == 0.0 {
            out[at] = byte_of('0');
            out[at + 1] = byte_of('.');
            out[at + 2] = byte_of('0');
            written = at + 3;
        } else {
            let n = fmt.float_into(raw, fabs_of(x));
            // `d[.ddd]e[-]k`: the digits, then the exponent of the first.
            var nd = 0;
            var p = 0;
            while p < n && int_of(raw[p]) != 'e' {
                if int_of(raw[p]) != '.' {
                    digits[nd] = raw[p];
                    nd = nd + 1;
                }
                p = p + 1;
            }
            p = p + 1;
            var exponent_negative = false;
            if int_of(raw[p]) == '-' {
                exponent_negative = true;
                p = p + 1;
            }
            var k = 0;
            while p < n {
                k = k * 10 + (int_of(raw[p]) - '0');
                p = p + 1;
            }
            if exponent_negative {
                k = 0 - k;
            }
            // `point`: how many digits come before the decimal point.
            let point = k + 1;
            var i = 0;
            if point > 0 && point <= 21 {
                while i < point {
                    if i < nd {
                        out[at] = digits[i];
                    } else {
                        out[at] = byte_of('0');
                    }
                    at = at + 1;
                    i = i + 1;
                }
                out[at] = byte_of('.');
                at = at + 1;
                if nd > point {
                    while i < nd {
                        out[at] = digits[i];
                        at = at + 1;
                        i = i + 1;
                    }
                } else {
                    out[at] = byte_of('0');
                    at = at + 1;
                }
            } else if point <= 0 && point > 0 - 6 {
                out[at] = byte_of('0');
                out[at + 1] = byte_of('.');
                at = at + 2;
                var zeros = 0 - point;
                while zeros > 0 {
                    out[at] = byte_of('0');
                    at = at + 1;
                    zeros = zeros - 1;
                }
                while i < nd {
                    out[at] = digits[i];
                    at = at + 1;
                    i = i + 1;
                }
            } else {
                out[at] = digits[0];
                at = at + 1;
                if nd > 1 {
                    out[at] = byte_of('.');
                    at = at + 1;
                    i = 1;
                    while i < nd {
                        out[at] = digits[i];
                        at = at + 1;
                        i = i + 1;
                    }
                }
                out[at] = byte_of('e');
                at = at + 1;
                var e = k;
                if e < 0 {
                    out[at] = byte_of('-');
                    at = at + 1;
                    e = 0 - e;
                }
                if e >= 100 {
                    out[at] = byte_of('0' + e / 100);
                    at = at + 1;
                }
                if e >= 10 {
                    out[at] = byte_of('0' + e / 10 % 10);
                    at = at + 1;
                }
                out[at] = byte_of('0' + e % 10);
                at = at + 1;
            }
            written = at;
        }
    }
    return written;
}

// The magnitude of a nonzero finite float.
fn fabs_of(x: float) -> [] float {
    if x < 0.0 {
        return 0.0 - x;
    }
    return x;
}

// A number. JSON has no `NaN` or infinity, so a float that is neither
// finite is written as `null` -- the choice most encoders make, and the one
// that keeps the document valid; a caller that wants an error checks first.
pub fn put_float[&h](heap: &!h Heap, w: Writer, x: float) -> [heap] Writer {
    if is_nan(x) || fabs_of(x) > 1.7976931348623157e308 {
        return put_raw(heap, w, "null");
    }
    let prepared = before_value(heap, w);
    let Writer { out, depth, has, objects, pending } = prepared;
    var o = out;
    region a {
        let text = alloc_slice[a](40, byte_of(0));
        let n = float_text(text, x);
        o = buffer.append(heap, o, text[0..n]);
    }
    return Writer { out: o, depth: depth, has: has, objects: objects, pending: pending };
}
