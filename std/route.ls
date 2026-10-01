module std.route;

import std.buffer;
import std.bytes;
import std.map;
import std.vec;

// `std.route` — which handler a request's method and path belong to.
//
// `docs/http.md` §5 is the design. A router here answers one question:
// given `GET` and `/users/42/posts`, which of the routes a program
// registered is it, and which parts of the path were the parameters? It
// answers with a route **id** the program chose, and the program `match`es
// on it -- there is no registry of function values, so dispatch is a jump
// the compiler can see, and a handler that is never routed to is a handler
// the reachability pass can drop.
//
// Patterns are `/`-separated segments:
//
//     /health              a literal path
//     /users/:id           `:name` matches exactly one non-empty segment
//     /files/*path         `*name`, last only, matches the rest, even if empty
//
// Matching rules, all of them, because a router with a hidden rule is a
// router with a security bug:
//
//   * **Static first.** A pattern with no parameters is found by hash;
//     only if no static route has the path are the parameterised routes
//     tried, and of those the one **added first** that fits wins. They
//     are indexed by their first literal segment, so a table of a thousand
//     `/svc17/...` routes costs a lookup the same as a table of ten; only
//     routes that share a first segment, or begin with a parameter, are
//     compared one by one.
//   * **The path is matched as sent.** `%`-escapes are not decoded
//     before matching, so `/%75sers` does not match `/users`. The
//     parameters come back as ranges of the raw path; decode them with
//     `std.http.percent_decode`. There is exactly one spelling of a path
//     that reaches a route, which is what keeps a proxy and this router
//     from disagreeing about which one that is.
//   * **No normalisation.** `/a/` is not `/a`; `/a//b` is not `/a/b`; an
//     empty segment matches no `:param`.
//   * **Methods are exact and case-sensitive** (RFC 9110 §9.1).
//   * **405 is not 404.** A path some route has under another method
//     answers `-2`, a path no route has answers `-1`.
//
// A pattern that is malformed, or an id below zero, traps at registration:
// it is the program's bug and a router table is written once, at start.

pub res struct Router {
    // Static path -> the index of the newest route that has it.
    exact: map.Map[int],
    // Eight ints a route: where its method starts in `text`, the method's
    // length, where its pattern starts, the pattern's length, the id, 1 if
    // it is static, the next older route with the same static path (or -1),
    // and the next older parameterised route with the same first segment
    // (or -1).
    routes: vec.Vec[int],
    // First literal segment -> the newest parameterised route with it.
    buckets: map.Map[int],
    // Indices of the parameterised routes whose first segment is itself a
    // parameter, oldest first: they could match anything, so every lookup
    // tries them.
    wild: vec.Vec[int],
    // Every method and pattern, back to back.
    text: buffer.Buffer,
    // The most parameters any route has.
    most: int,
}

pub fn empty[&h](heap: &!h Heap) -> [heap] Router {
    return Router { exact: map.empty(heap, 8, 0 - 1, 0), routes: vec.empty(heap, 64, 0), buckets: map.empty(heap, 8, 0 - 1, 0), wild: vec.empty(heap, 8, 0), text: buffer.empty(heap, 256), most: 0 };
}

pub fn drop[&h](heap: &!h Heap, router: Router) -> [heap] int {
    let Router { exact, routes, buckets, wild, text, most } = router;
    map.drop(heap, exact);
    let n = vec.drop(heap, routes) / 8;
    map.drop(heap, buckets);
    vec.drop(heap, wild);
    buffer.drop(heap, text);
    return n;
}

// How many routes were added.
pub fn count[&r](router: &r Router) -> [] int {
    return vec.size(router.routes) / 8;
}

// The most parameters any route has: a caller's parameter table is
// `2 * most_params` ints (a start and an end for each).
pub fn most_params[&r](router: &r Router) -> [] int {
    return router.most;
}

// How many parameters `pattern` has, or -1 if it is not a valid pattern.
fn analyse[&p](pattern: &p [byte]) -> [] int {
    let n = len(pattern);
    if n == 0 || int_of(pattern[0]) != 47 {
        return 0 - 1;
    }
    if n == 1 {
        return 0;
    }
    if int_of(pattern[n - 1]) == 47 {
        return 0 - 1;
    }
    var count = 0;
    var at = 1;
    while at < n {
        var end = at;
        while end < n && int_of(pattern[end]) != 47 {
            end = end + 1;
        }
        if end == at {
            return 0 - 1;
        }
        let first = int_of(pattern[at]);
        if first == 58 || first == 42 {
            if end == at + 1 {
                return 0 - 1;
            }
            // A rest parameter takes everything, so nothing may follow it.
            if first == 42 && end != n {
                return 0 - 1;
            }
            count = count + 1;
        }
        at = end + 1;
    }
    return count;
}

// Whether `path` fits `pattern` (which `analyse` accepted), writing each
// parameter's `(start, end)` into `params` as it goes. On a failed match
// `params` holds whatever the attempt wrote and is not meaningful.
fn matches[&p, &q, &t](pattern: &p [byte], path: &q [byte], params: &!t [int]) -> [] bool {
    var p = 0;
    var q = 0;
    var k = 0;
    while p < len(pattern) {
        if q >= len(path) || int_of(path[q]) != 47 {
            return false;
        }
        p = p + 1;
        q = q + 1;
        var pe = p;
        while pe < len(pattern) && int_of(pattern[pe]) != 47 {
            pe = pe + 1;
        }
        if int_of(pattern[p]) == 42 {
            params[2 * k] = q;
            params[2 * k + 1] = len(path);
            return true;
        }
        var qe = q;
        while qe < len(path) && int_of(path[qe]) != 47 {
            qe = qe + 1;
        }
        if int_of(pattern[p]) == 58 {
            if qe == q {
                return false;
            }
            params[2 * k] = q;
            params[2 * k + 1] = qe;
            k = k + 1;
        } else if !bytes.equal(pattern[p..pe], path[q..qe]) {
            return false;
        }
        p = pe;
        q = qe;
    }
    return q == len(path);
}

// Add a route: requests with this method and a path matching `pattern`
// are `id`'s. Traps on a malformed pattern, an `id` below zero, or a
// second static route with the same method and path.
pub fn add[&h, &m, &p](heap: &!h Heap, router: Router, method: &m [byte], pattern: &p [byte], id: int) -> [heap] Router {
    let shape = analyse(pattern);
    if shape < 0 || id < 0 || len(method) == 0 {
        trap();
    }
    let Router { exact, routes, buckets, wild, text, most } = router;
    var ex = exact;
    var rt = routes;
    var bk = buckets;
    var wl = wild;
    var tx = text;

    var method_start = 0;
    borrow tx as &t in {
        method_start = buffer.size(t);
    }
    tx = buffer.append(heap, tx, method);
    let pattern_start = method_start + len(method);
    tx = buffer.append(heap, tx, pattern);
    var index = 0;
    borrow rt as &v in {
        index = vec.size(v) / 8;
    }

    var older = 0 - 1;
    if shape == 0 {
        borrow ex as &e in {
            older = map.get(e, pattern, 0 - 1);
        }
        // A second route for the same method and path could never be
        // reached; refuse it rather than pick one silently.
        var walk = older;
        while walk >= 0 {
            borrow rt as &v in {
                borrow tx as &t in {
                    let ms = vec.get(v, walk * 8);
                    let ml = vec.get(v, walk * 8 + 1);
                    if bytes.equal(buffer.bytes(t)[ms..ms + ml], method) {
                        trap();
                    }
                    walk = vec.get(v, walk * 8 + 6);
                }
            }
        }
    }

    var is_static = 0;
    if shape == 0 {
        is_static = 1;
    }
    rt = vec.push(heap, rt, method_start);
    rt = vec.push(heap, rt, len(method));
    rt = vec.push(heap, rt, pattern_start);
    rt = vec.push(heap, rt, len(pattern));
    rt = vec.push(heap, rt, id);
    rt = vec.push(heap, rt, is_static);
    rt = vec.push(heap, rt, older);
    // The first segment of a parameterised pattern: a literal is a bucket,
    // a parameter makes the route a wildcard.
    var first_end = 1;
    while first_end < len(pattern) && int_of(pattern[first_end]) != 47 {
        first_end = first_end + 1;
    }
    var in_bucket = 0 - 1;
    if shape > 0 && int_of(pattern[1]) != 58 && int_of(pattern[1]) != 42 {
        borrow bk as &b in {
            in_bucket = map.get(b, pattern[1..first_end], 0 - 1);
        }
    }
    rt = vec.push(heap, rt, in_bucket);
    if shape == 0 {
        ex = map.put(heap, ex, pattern, index);
    } else if int_of(pattern[1]) == 58 || int_of(pattern[1]) == 42 {
        wl = vec.push(heap, wl, index);
    } else {
        bk = map.put(heap, bk, pattern[1..first_end], index);
    }
    var widest = most;
    if shape > widest {
        widest = shape;
    }
    return Router { exact: ex, routes: rt, buckets: bk, wild: wl, text: tx, most: widest };
}

// How `route` relates to a request: 0 if its pattern does not fit `path`,
// 1 if it fits but under another method, 2 if it fits and the method
// matches too.
fn relation[&r, &t, &m, &p, &q](router: &r Router, text: &t [byte], route: int, method: &m [byte], path: &p [byte], params: &!q [int]) -> [] int {
    let ps = vec.get(router.routes, route * 8 + 2);
    let pl = vec.get(router.routes, route * 8 + 3);
    if !matches(text[ps..ps + pl], path, params) {
        return 0;
    }
    let ms = vec.get(router.routes, route * 8);
    let ml = vec.get(router.routes, route * 8 + 1);
    if bytes.equal(text[ms..ms + ml], method) {
        return 2;
    }
    return 1;
}

// Find the route for `method` and `path`.
//
// Answers the route's id; `-1` if no route has this path under any method;
// `-2` if some route has it under a different method. `params` receives a
// `(start, end)` pair per parameter of the matched route, in pattern order,
// as offsets into `path`; it must hold `2 * most_params(router)` ints and
// is untouched by a static match.
pub fn find[&r, &m, &p, &t](router: &r Router, method: &m [byte], path: &p [byte], params: &!t [int]) -> [] int {
    let text = buffer.bytes(router.text);
    var known = false;

    // Static routes: one hash lookup, then the (short) chain of methods.
    var at = map.get(router.exact, path, 0 - 1);
    while at >= 0 {
        let ms = vec.get(router.routes, at * 8);
        let ml = vec.get(router.routes, at * 8 + 1);
        if bytes.equal(text[ms..ms + ml], method) {
            return vec.get(router.routes, at * 8 + 4);
        }
        known = true;
        at = vec.get(router.routes, at * 8 + 6);
    }

    // Parameterised routes: those sharing the path's first segment, and the
    // wildcards. The winner is the one added first that fits with this
    // method; every candidate is looked at, because the two lists are each
    // in order but not merged.
    var first_end = 1;
    while first_end < len(path) && int_of(path[first_end]) != 47 {
        first_end = first_end + 1;
    }
    var best = 0 - 1;
    var bucket = 0 - 1;
    if len(path) > 1 {
        bucket = map.get(router.buckets, path[1..first_end], 0 - 1);
    }
    while bucket >= 0 {
        let r = relation(router, text, bucket, method, path, params);
        if r == 2 && (best < 0 || bucket < best) {
            best = bucket;
        } else if r == 1 {
            known = true;
        }
        bucket = vec.get(router.routes, bucket * 8 + 7);
    }
    var i = 0;
    while i < vec.size(router.wild) {
        let route = vec.get(router.wild, i);
        let r = relation(router, text, route, method, path, params);
        if r == 2 && (best < 0 || route < best) {
            best = route;
        } else if r == 1 {
            known = true;
        }
        i = i + 1;
    }
    if best >= 0 {
        // The candidates overwrote `params` in turn; write the winner's.
        relation(router, text, best, method, path, params);
        return vec.get(router.routes, best * 8 + 4);
    }
    if known {
        return 0 - 2;
    }
    return 0 - 1;
}
