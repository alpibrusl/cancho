import std.buffer;
import std.bytes;
import std.route;
import std.test;

fn test_static_dynamic_and_the_405[&h](heap: &!h Heap) -> [heap] int {
    var r = route.empty(heap);
    r = route.add(heap, r, "GET", "/health", 1);
    r = route.add(heap, r, "POST", "/health", 2);
    r = route.add(heap, r, "GET", "/users/:id", 3);
    r = route.add(heap, r, "GET", "/users/:id/posts/:post", 4);
    r = route.add(heap, r, "GET", "/files/*path", 5);
    r = route.add(heap, r, "GET", "/", 6);
    var widest = 0;
    borrow r as &rr in {
        widest = route.most_params(rr);
    }
    let table = box_slice(heap, 2 * widest, 0);
    borrow mut table as &!w in {
        let t = contents(w);
        borrow r as &rr in {
            test.assert_eq(route.count(rr), 6);
            test.assert_eq(route.find(rr, "GET", "/health", t), 1);
            test.assert_eq(route.find(rr, "POST", "/health", t), 2);
            test.assert_eq(route.find(rr, "DELETE", "/health", t), 0 - 2);
            test.assert_eq(route.find(rr, "GET", "/", t), 6);
            test.assert_eq(route.find(rr, "GET", "/nothing", t), 0 - 1);
            test.assert_eq(route.find(rr, "GET", "/users/42", t), 3);
            test.assert_eq(t[0], 7);
            test.assert_eq(t[1], 9);
            test.assert_eq(route.find(rr, "GET", "/users/42/posts/abc", t), 4);
            test.assert_eq(t[0], 7);
            test.assert_eq(t[1], 9);
            test.assert_eq(t[2], 16);
            test.assert_eq(t[3], 19);
            test.assert_eq(route.find(rr, "PUT", "/users/42", t), 0 - 2);
            // No normalisation: a trailing slash, a doubled one, an empty segment.
            test.assert_eq(route.find(rr, "GET", "/users/42/", t), 0 - 1);
            test.assert_eq(route.find(rr, "GET", "/users//posts/1", t), 0 - 1);
            test.assert_eq(route.find(rr, "GET", "/health/", t), 0 - 1);
            // A rest parameter takes anything, the empty string included.
            test.assert_eq(route.find(rr, "GET", "/files/a/b/c.txt", t), 5);
            test.assert_eq(t[0], 7);
            test.assert_eq(t[1], 16);
            test.assert_eq(route.find(rr, "GET", "/files/", t), 5);
            test.assert_eq(t[0], 7);
            test.assert_eq(t[1], 7);
            test.assert_eq(route.find(rr, "GET", "/files", t), 0 - 1);
            // Escapes are not decoded before matching.
            test.assert_eq(route.find(rr, "GET", "/%75sers/1", t), 0 - 1);
            test.assert_eq(route.find(rr, "get", "/health", t), 0 - 2);
        }
    }
    unbox_slice(heap, table);
    route.drop(heap, r);
    return 0;
}

// Static routes win over a parameterised one that also fits, and
// parameterised ones are tried in the order they were added.
fn test_priority[&h](heap: &!h Heap) -> [heap] int {
    var r = route.empty(heap);
    r = route.add(heap, r, "GET", "/users/:id", 1);
    r = route.add(heap, r, "GET", "/users/me", 2);
    r = route.add(heap, r, "GET", "/a/:x", 3);
    r = route.add(heap, r, "GET", "/a/*rest", 4);
    var widest = 0;
    borrow r as &rr in {
        widest = route.most_params(rr);
    }
    let table = box_slice(heap, 2 * widest, 0);
    borrow mut table as &!w in {
        let t = contents(w);
        borrow r as &rr in {
            test.assert_eq(route.find(rr, "GET", "/users/me", t), 2);
            test.assert_eq(route.find(rr, "GET", "/users/you", t), 1);
            test.assert_eq(route.find(rr, "GET", "/a/b", t), 3);
            test.assert_eq(route.find(rr, "GET", "/a/b/c", t), 4);
        }
    }
    unbox_slice(heap, table);
    route.drop(heap, r);
    return 0;
}

// What a 405 must say it would have allowed: the methods some route has for
// the path, in the order they were added, each once.
fn test_the_methods_a_path_allows[&h](heap: &!h Heap) -> [heap] int {
    var r = route.empty(heap);
    r = route.add(heap, r, "GET", "/users/:id", 1);
    r = route.add(heap, r, "PUT", "/users/me", 2);
    r = route.add(heap, r, "DELETE", "/users/:id", 3);
    r = route.add(heap, r, "GET", "/users/*rest", 4);
    r = route.add(heap, r, "POST", "/users", 5);
    var widest = 0;
    borrow r as &rr in {
        widest = route.most_params(rr);
    }
    let table = box_slice(heap, 2 * widest, 0);
    borrow mut table as &!w in {
        let t = contents(w);
        // /users/me fits the static PUT route and both parameterised GETs and
        // the DELETE: GET once, though two routes give it.
        var a = buffer.empty(heap, 16);
        borrow r as &rr in {
            a = route.allowed(heap, rr, "/users/me", t, a);
        }
        borrow a as &ab in {
            test.assert(bytes.equal(buffer.bytes(ab), "GET, PUT, DELETE"));
        }
        buffer.drop(heap, a);
        var b = buffer.empty(heap, 16);
        borrow r as &rr in {
            b = route.allowed(heap, rr, "/users", t, b);
        }
        borrow b as &bb in {
            test.assert(bytes.equal(buffer.bytes(bb), "POST"));
        }
        buffer.drop(heap, b);
        // A path nothing has contributes nothing.
        var c = buffer.empty(heap, 16);
        borrow r as &rr in {
            c = route.allowed(heap, rr, "/nowhere", t, c);
        }
        borrow c as &cb in {
            test.assert_eq(len(buffer.bytes(cb)), 0);
        }
        buffer.drop(heap, c);
    }
    unbox_slice(heap, table);
    route.drop(heap, r);
    return 0;
}

fn test_typed_parameters[&h](heap: &!h Heap) -> [heap] int {
    var r = route.empty(heap);
    r = route.add(heap, r, "GET", "/users/:id/posts/:post", 1);
    var widest = 0;
    borrow r as &rr in {
        widest = route.most_params(rr);
    }
    let table = box_slice(heap, 2 * widest, 0);
    borrow mut table as &!w in {
        let t = contents(w);
        borrow r as &rr in {
            test.assert_eq(route.find(rr, "GET", "/users/42/posts/007", t), 1);
            test.assert(bytes.equal(route.param("/users/42/posts/007", t, 0), "42"));
            test.assert_eq(route.param_nat("/users/42/posts/007", t, 0), 42);
            test.assert_eq(route.param_nat("/users/42/posts/007", t, 1), 7);
            // Not a number is -1, not zero and not a crash.
            test.assert_eq(route.find(rr, "GET", "/users/4x/posts/-5", t), 1);
            test.assert_eq(route.param_nat("/users/4x/posts/-5", t, 0), 0 - 1);
            test.assert_eq(route.param_nat("/users/4x/posts/-5", t, 1), 0 - 1);
            // More than 17 digits is refused rather than overflowed.
            test.assert_eq(route.find(rr, "GET", "/users/123456789012345678/posts/1", t), 1);
            test.assert_eq(route.param_nat("/users/123456789012345678/posts/1", t, 0), 0 - 1);
            test.assert_eq(route.find(rr, "GET", "/users/12345678901234567/posts/1", t), 1);
            test.assert_eq(route.param_nat("/users/12345678901234567/posts/1", t, 0), 12345678901234567);
        }
    }
    unbox_slice(heap, table);
    route.drop(heap, r);
    return 0;
}

fn test_a_parameter_decodes_on_request[&h](heap: &!h Heap) -> [heap] int {
    var r = route.empty(heap);
    r = route.add(heap, r, "GET", "/search/:term", 1);
    let table = box_slice(heap, 2, 0);
    let scratch = box_slice(heap, 32, byte_of(0));
    borrow mut table as &!w in {
        borrow mut scratch as &!sw in {
            let t = contents(w);
            let o = contents(sw);
            borrow r as &rr in {
                test.assert_eq(route.find(rr, "GET", "/search/a%20b%2Fc", t), 1);
                // The raw parameter is what was matched, escapes and all.
                test.assert(bytes.equal(route.param("/search/a%20b%2Fc", t, 0), "a%20b%2Fc"));
                let n = route.param_decoded("/search/a%20b%2Fc", t, 0, o);
                test.assert_eq(n, 5);
                test.assert(bytes.equal(o[0..n], "a b/c"));
                // A bad escape is -1.
                test.assert_eq(route.find(rr, "GET", "/search/%zz", t), 1);
                test.assert_eq(route.param_decoded("/search/%zz", t, 0, o), 0 - 1);
            }
        }
    }
    unbox_slice(heap, scratch);
    unbox_slice(heap, table);
    route.drop(heap, r);
    return 0;
}
