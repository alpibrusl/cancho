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
