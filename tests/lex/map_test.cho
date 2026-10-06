import std.buffer;
import std.map;
import std.test;

fn test_put_get_and_overwrite[&h](heap: &!h Heap) -> [heap] int {
    var m = map.empty(heap, 4, 0 - 1, 7);
    m = map.put(heap, m, "alpha", 1);
    m = map.put(heap, m, "beta", 2);
    m = map.put(heap, m, "alpha", 10);
    borrow m as &r in {
        test.assert_eq(map.size(r), 2);
        test.assert_eq(map.get(r, "alpha", 0 - 1), 10);
        test.assert_eq(map.get(r, "beta", 0 - 1), 2);
        test.assert_eq(map.get(r, "gamma", 0 - 1), 0 - 1);
        test.assert(map.has(r, "beta"));
        test.assert(!map.has(r, "alph"));
    }
    map.drop(heap, m);
    return 0;
}

fn test_remove_then_put_again[&h](heap: &!h Heap) -> [heap] int {
    var m = map.empty(heap, 4, 0, 1);
    m = map.put(heap, m, "a", 1);
    m = map.put(heap, m, "b", 2);
    m = map.put(heap, m, "c", 3);
    borrow mut m as &!w in {
        test.assert(map.remove(w, "b"));
        test.assert(!map.remove(w, "b"));
        test.assert(!map.remove(w, "zz"));
    }
    borrow m as &r in {
        test.assert_eq(map.size(r), 2);
        test.assert(!map.has(r, "b"));
        test.assert_eq(map.get(r, "c", 0), 3);
    }
    m = map.put(heap, m, "b", 20);
    borrow m as &r in {
        test.assert_eq(map.size(r), 3);
        test.assert_eq(map.get(r, "b", 0), 20);
        test.assert_eq(map.get(r, "a", 0), 1);
    }
    map.drop(heap, m);
    return 0;
}

// The empty key is a key like any other.
fn test_the_empty_key[&h](heap: &!h Heap) -> [heap] int {
    var m = map.empty(heap, 4, 0, 3);
    m = map.put(heap, m, "", 5);
    m = map.put(heap, m, "x", 6);
    borrow m as &r in {
        test.assert_eq(map.get(r, "", 0), 5);
        test.assert_eq(map.get(r, "x", 0), 6);
        test.assert_eq(map.size(r), 2);
    }
    map.drop(heap, m);
    return 0;
}

// Entries come back in the order they were first put, whatever the
// hash did, across growth and across a removal that growth compacts.
fn test_iteration_is_insertion_order[&h](heap: &!h Heap) -> [heap] int {
    var m = map.empty(heap, 4, 0, 99);
    var n = 0;
    while n < 40 {
        let key = buffer.empty(heap, 8);
        let text = buffer.push_nat(heap, key, n);
        borrow text as &t in {
            m = map.put(heap, m, buffer.bytes(t), n * 10);
        }
        buffer.drop(heap, text);
        n = n + 1;
    }
    borrow mut m as &!w in {
        test.assert(map.remove(w, "7"));
        test.assert(map.remove(w, "30"));
    }
    // Enough more puts to force a compaction.
    n = 40;
    while n < 100 {
        let key = buffer.empty(heap, 8);
        let text = buffer.push_nat(heap, key, n);
        borrow text as &t in {
            m = map.put(heap, m, buffer.bytes(t), n * 10);
        }
        buffer.drop(heap, text);
        n = n + 1;
    }
    borrow m as &r in {
        test.assert_eq(map.size(r), 98);
        var expect = 0;
        var e = 0;
        while e < map.entries(r) {
            if map.is_live(r, e) {
                if expect == 7 {
                    expect = 8;
                }
                if expect == 30 {
                    expect = 31;
                }
                test.assert_eq(map.value_at(r, e), expect * 10);
                expect = expect + 1;
            }
            e = e + 1;
        }
        test.assert_eq(expect, 100);
    }
    map.drop(heap, m);
    return 0;
}

fn next_random(state: int) -> [] int {
    // A 31-bit LCG; only the high bits are used by the caller.
    return (state * 1103515245 + 12345) % 2147483648;
}

fn key_of[&h](heap: &!h Heap, k: int) -> [heap] buffer.Buffer {
    let b = buffer.empty(heap, 8);
    return buffer.push_nat(heap, b, k);
}

// 30,000 random puts, removes and gets over 300 keys against a plain
// array holding the answer: every `get` and the final size must agree.
// The small starting capacity and the churn force repeated growth and
// compaction with tombstones in the probe chains.
fn test_random_operations_agree_with_a_model[&h](heap: &!h Heap) -> [heap] int {
    var m = map.empty(heap, 4, 0 - 1, 12345);
    let held = box_slice(heap, 300, 0 - 1);
    var live = 0;
    borrow mut held as &!hw in {
        let model = contents(hw);
        var state = 42;
        var step = 0;
        while step < 30000 {
            state = next_random(state);
            let which = state / 65536 % 3;
            state = next_random(state);
            let k = state / 65536 % 300;
            state = next_random(state);
            let value = state / 65536 % 1000;
            let key = key_of(heap, k);
            borrow key as &kb in {
                let text = buffer.bytes(kb);
                if which == 0 {
                    if model[k] < 0 {
                        live = live + 1;
                    }
                    model[k] = value;
                    m = map.put(heap, m, text, value);
                } else if which == 1 {
                    var removed = false;
                    borrow mut m as &!w in {
                        removed = map.remove(w, text);
                    }
                    test.assert(removed == model[k] >= 0);
                    if model[k] >= 0 {
                        live = live - 1;
                    }
                    model[k] = 0 - 1;
                } else {
                    borrow m as &r in {
                        test.assert_eq(map.get(r, text, 0 - 1), model[k]);
                    }
                }
            }
            buffer.drop(heap, key);
            step = step + 1;
        }
    }
    borrow m as &r in {
        test.assert_eq(map.size(r), live);
    }
    unbox_slice(heap, held);
    map.drop(heap, m);
    return 0;
}
