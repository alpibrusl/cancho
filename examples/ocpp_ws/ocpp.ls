edition 5;

module ocpp;

import std.bytes;
import std.json;
import timefmt;

// `ocpp` -- OCPP-J messages (OCPP 1.6, section 4): read a call from a charge point, write the answer.
//
// A call is `[2,"<unique id>","<Action>",{payload}]` and its answer `[3,"<unique id>",{payload}]`; a refusal is
// `[4,"<unique id>","<code>","<description>",{}]`. The whole frame is checked as JSON (`std.json`, strictly), because that is
// what a central system does and what the comparison with `lex-csms` has to include; the payload is not otherwise looked at.

// What `read_message` answered: a call, a result, an error, or why it is none.
pub fn call() -> [] int {
    return 2;
}

pub fn result() -> [] int {
    return 3;
}

pub fn error() -> [] int {
    return 4;
}

// Answers `(kind, id_start, id_end, action_start, action_end)`, offsets into `text`. `kind` is 2, 3 or 4, or negative if the frame
// is not an OCPP message: -1 not JSON, -2 not an array of the right shape, -3 a unique id longer than 36 characters or one with
// an escape in it. For a result or an error the action offsets are 0. `tape` must hold `json.tape_len(text)` ints.
pub fn read_message[&s, &t](text: &s [byte], tape: &!t [int]) -> [] (int, int, int, int, int) {
    let nodes = json.parse(text, tape);
    if nodes < 0 {
        return (0 - 1, 0, 0, 0, 0);
    }
    if !json.is_array(tape, 0) {
        return (0 - 2, 0, 0, 0, 0);
    }
    let n = json.count(tape, 0);
    if n < 3 || n > 5 {
        return (0 - 2, 0, 0, 0, 0);
    }
    let kind_node = json.at(tape, 0, 0);
    let id_node = json.at(tape, 0, 1);
    if !json.is_int(tape, kind_node) || !json.is_string(tape, id_node) {
        return (0 - 2, 0, 0, 0, 0);
    }
    let kind = json.to_int(text, tape, kind_node);
    let id_start = tape[3 * id_node + 1];
    let id_end = tape[3 * id_node + 2];
    if id_end - id_start > 36 || !json.string_plain(tape, id_node) {
        return (0 - 3, 0, 0, 0, 0);
    }
    if kind == 2 {
        if n != 4 {
            return (0 - 2, 0, 0, 0, 0);
        }
        let act_node = json.at(tape, 0, 2);
        if !json.is_string(tape, act_node) || !json.string_plain(tape, act_node) {
            return (0 - 2, 0, 0, 0, 0);
        }
        return (2, id_start, id_end, tape[3 * act_node + 1], tape[3 * act_node + 2]);
    }
    if kind == 3 && n == 3 {
        return (3, id_start, id_end, 0, 0);
    }
    if kind == 4 && n >= 4 {
        return (4, id_start, id_end, 0, 0);
    }
    return (0 - 2, 0, 0, 0, 0);
}

// Copy `s` into `out` at `at`; answers the offset after it.
fn put[&o, &s](out: &!o [byte], at: int, s: &s [byte]) -> [] int {
    var i = 0;
    while i < len(s) {
        out[at + i] = s[i];
        i = i + 1;
    }
    return at + len(s);
}

// Which actions this server answers with an empty `{}` (besides `BootNotification` and `Heartbeat`, which have a payload).
fn plain_action[&s](action: &s [byte]) -> [] bool {
    return bytes.equal(action, "StatusNotification") || bytes.equal(action, "MeterValues");
}

// The answer to the call `text[id_start..id_end]`/`action`, into `out` at `at`, with `now_ms` as the time. Answers the length written.
// `interval` is the heartbeat interval (seconds) a `BootNotification` is told to keep.
pub fn reply[&o, &s](out: &!o [byte], at: int, text: &s [byte], id_start: int, id_end: int, action_start: int, action_end: int, now_ms: int, interval: int) -> [] int {
    let action = text[action_start..action_end];
    var p = at;
    if bytes.equal(action, "BootNotification") {
        p = put(out, p, "[3,\"");
        p = put(out, p, text[id_start..id_end]);
        p = put(out, p, "\",{\"currentTime\":\"");
        p = p + timefmt.iso(out, p, now_ms);
        p = put(out, p, "\",\"interval\":");
        p = p + put_nat(out, p, interval);
        p = put(out, p, ",\"status\":\"Accepted\"}]");
    } else if bytes.equal(action, "Heartbeat") {
        p = put(out, p, "[3,\"");
        p = put(out, p, text[id_start..id_end]);
        p = put(out, p, "\",{\"currentTime\":\"");
        p = p + timefmt.iso(out, p, now_ms);
        p = put(out, p, "\"}]");
    } else if plain_action(action) {
        p = put(out, p, "[3,\"");
        p = put(out, p, text[id_start..id_end]);
        p = put(out, p, "\",{}]");
    } else {
        p = put(out, p, "[4,\"");
        p = put(out, p, text[id_start..id_end]);
        p = put(out, p, "\",\"NotImplemented\",\"\",{}]");
    }
    return p - at;
}

// A `CallError` for a frame that could not be read, with the unique id `unknown` (OCPP 1.6 section 4.2.3: when there is no id, the
// charge point is told with an id it cannot match, and the code says what was wrong). `FormationViolation` for text that is not
// JSON, `ProtocolError` for JSON that is not an OCPP message.
pub fn refusal[&o](out: &!o [byte], at: int, why: int) -> [] int {
    var p = put(out, at, "[4,\"-1\",\"");
    if why == 0 - 1 {
        p = put(out, p, "FormationViolation");
    } else {
        p = put(out, p, "ProtocolError");
    }
    p = put(out, p, "\",\"\",{}]");
    return p - at;
}

fn put_nat[&o](out: &!o [byte], at: int, n: int) -> [] int {
    var width = 1;
    var t = n;
    while t >= 10 {
        t = t / 10;
        width = width + 1;
    }
    var k = width;
    var m = n;
    while k > 0 {
        out[at + k - 1] = byte_of('0' + m % 10);
        m = m / 10;
        k = k - 1;
    }
    return width;
}
