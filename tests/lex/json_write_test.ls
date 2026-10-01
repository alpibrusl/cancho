// `std.json`'s writer, run by `lex-sys test --std tests/lex/json_write_test.ls`.
import std.json;
import std.test;
import std.bytes;
import std.buffer;

// Does the document the writer holds equal `expected`? Ends the writer.
fn finished[&h, &e](heap: &!h Heap, w: json.Writer, expected: &e [byte]) -> [heap] bool {
    var same = false;
    borrow w as &r in {
        same = bytes.equal(json.bytes(r), expected);
    }
    json.drop(heap, w);
    return same;
}

fn test_an_object_with_every_kind_of_value[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 8);
    w = json.begin_object(heap, w);
    w = json.put_key(heap, w, "name");
    w = json.put_string(heap, w, "Ada");
    w = json.put_key(heap, w, "age");
    w = json.put_int(heap, w, 36);
    w = json.put_key(heap, w, "pi");
    w = json.put_float(heap, w, 3.5);
    w = json.put_key(heap, w, "ok");
    w = json.put_bool(heap, w, true);
    w = json.put_key(heap, w, "no");
    w = json.put_bool(heap, w, false);
    w = json.put_key(heap, w, "none");
    w = json.put_null(heap, w);
    w = json.put_key(heap, w, "tags");
    w = json.begin_array(heap, w);
    w = json.put_string(heap, w, "a");
    w = json.put_int(heap, w, 2);
    w = json.begin_object(heap, w);
    w = json.end_object(heap, w);
    w = json.end_array(heap, w);
    w = json.put_key(heap, w, "empty");
    w = json.begin_array(heap, w);
    w = json.end_array(heap, w);
    w = json.end_object(heap, w);
    test.assert(finished(heap, w, "{\"name\":\"Ada\",\"age\":36,\"pi\":3.5,\"ok\":true,\"no\":false,\"none\":null,\"tags\":[\"a\",2,{}],\"empty\":[]}"));
    return 0;
}

fn test_a_top_level_scalar_is_a_document[&h](heap: &!h Heap) -> [heap] int {
    var a = json.writer(heap, 4);
    a = json.put_int(heap, a, 7);
    test.assert(finished(heap, a, "7"));
    var b = json.writer(heap, 4);
    b = json.put_string(heap, b, "x");
    test.assert(finished(heap, b, "\"x\""));
    return 0;
}

fn test_integers_at_both_ends[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 4);
    w = json.begin_array(heap, w);
    w = json.put_int(heap, w, 0);
    w = json.put_int(heap, w, 0 - 1);
    w = json.put_int(heap, w, 9223372036854775807);
    w = json.put_int(heap, w, 0 - 9223372036854775807 - 1);
    w = json.end_array(heap, w);
    test.assert(finished(heap, w, "[0,-1,9223372036854775807,-9223372036854775808]"));
    return 0;
}

fn test_floats_are_laid_out_like_javascript_and_python[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 4);
    w = json.begin_array(heap, w);
    w = json.put_float(heap, w, 0.0);
    w = json.put_float(heap, w, 0.0 * (0.0 - 1.0));
    w = json.put_float(heap, w, 1.0);
    w = json.put_float(heap, w, 0.0 - 2.5);
    w = json.put_float(heap, w, 0.1);
    w = json.put_float(heap, w, 123456789.125);
    w = json.put_float(heap, w, 1.0e20);
    w = json.put_float(heap, w, 1.0e21);
    w = json.put_float(heap, w, 1.5e22);
    w = json.put_float(heap, w, 0.000001);
    w = json.put_float(heap, w, 0.0000001);
    w = json.put_float(heap, w, 1.5e-7);
    w = json.put_float(heap, w, 5.0e-324);
    w = json.put_float(heap, w, 1.7976931348623157e308);
    w = json.put_float(heap, w, 0.30000000000000004);
    w = json.end_array(heap, w);
    test.assert(finished(heap, w, "[0.0,-0.0,1.0,-2.5,0.1,123456789.125,100000000000000000000.0,1e21,1.5e22,0.000001,1e-7,1.5e-7,5e-324,1.7976931348623157e308,0.30000000000000004]"));
    return 0;
}

fn test_a_float_that_is_not_a_number_is_null[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 4);
    w = json.begin_array(heap, w);
    w = json.put_float(heap, w, 0.0 / 0.0);
    w = json.put_float(heap, w, 1.0 / 0.0);
    w = json.put_float(heap, w, 0.0 - 1.0 / 0.0);
    w = json.end_array(heap, w);
    test.assert(finished(heap, w, "[null,null,null]"));
    return 0;
}

fn test_strings_are_escaped[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 4);
    w = json.begin_array(heap, w);
    w = json.put_string(heap, w, "plain");
    w = json.put_string(heap, w, "quote\" slash\\ nl\n tab\t cr\r");
    w = json.put_string(heap, w, "é日😀");
    w = json.put_string(heap, w, "");
    w = json.end_array(heap, w);
    test.assert(finished(heap, w, "[\"plain\",\"quote\\\" slash\\\\ nl\\n tab\\t cr\\r\",\"é日😀\",\"\"]"));
    return 0;
}

fn test_other_control_characters_are_unicode_escapes[&h](heap: &!h Heap) -> [heap] int {
    var text = buffer.empty(heap, 8);
    text = buffer.push(heap, text, byte_of(0));
    text = buffer.push(heap, text, byte_of(8));
    text = buffer.push(heap, text, byte_of(12));
    text = buffer.push(heap, text, byte_of(0x1f));
    var w = json.writer(heap, 4);
    borrow text as &r in {
        w = json.put_string(heap, w, buffer.bytes(r));
    }
    buffer.drop(heap, text);
    test.assert(finished(heap, w, "\"\\u0000\\b\\f\\u001f\""));
    return 0;
}

fn test_invalid_utf8_becomes_the_replacement_character[&h](heap: &!h Heap) -> [heap] int {
    var text = buffer.empty(heap, 8);
    text = buffer.push(heap, text, byte_of('a'));
    text = buffer.push(heap, text, byte_of(0xff));
    text = buffer.push(heap, text, byte_of('b'));
    text = buffer.push(heap, text, byte_of(0xc3));
    var w = json.writer(heap, 4);
    borrow text as &r in {
        w = json.put_string(heap, w, buffer.bytes(r));
    }
    buffer.drop(heap, text);
    test.assert(finished(heap, w, "\"a\\ufffdb\\ufffd\""));
    return 0;
}

fn test_keys_are_escaped_like_strings[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 4);
    w = json.begin_object(heap, w);
    w = json.put_key(heap, w, "a\"b");
    w = json.put_int(heap, w, 1);
    w = json.end_object(heap, w);
    test.assert(finished(heap, w, "{\"a\\\"b\":1}"));
    return 0;
}

fn test_a_big_document_grows_the_buffer[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 1);
    w = json.begin_array(heap, w);
    var i = 0;
    while i < 500 {
        w = json.put_int(heap, w, i);
        i = i + 1;
    }
    w = json.end_array(heap, w);
    var length = 0;
    borrow w as &r in {
        length = len(json.bytes(r));
    }
    json.drop(heap, w);
    // "[" + "]" + 500 numbers + 499 commas: 10 one-digit, 90 two, 400 three.
    test.assert_eq(length, 2 + 10 + 180 + 1200 + 499);
    return 0;
}

fn test_what_is_written_reads_back[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 16);
    w = json.begin_object(heap, w);
    w = json.put_key(heap, w, "s");
    w = json.put_string(heap, w, "line\nbreak \"quoted\" é日😀");
    w = json.put_key(heap, w, "n");
    w = json.put_int(heap, w, 0 - 12345);
    w = json.put_key(heap, w, "f");
    w = json.put_float(heap, w, 0.1);
    w = json.put_key(heap, w, "list");
    w = json.begin_array(heap, w);
    w = json.put_bool(heap, w, true);
    w = json.put_null(heap, w);
    w = json.end_array(heap, w);
    w = json.end_object(heap, w);
    borrow w as &r in {
        let doc = json.bytes(r);
        region a {
            let tape = alloc_slice[a](json.tape_len(doc), 0);
            test.assert(json.parse(doc, tape) > 0);
            test.assert(json.string_equals(doc, tape, json.get(doc, tape, 0, "s"), "line\nbreak \"quoted\" é日😀"));
            test.assert_eq(json.to_int(doc, tape, json.get(doc, tape, 0, "n")), 0 - 12345);
            test.assert(json.to_float(doc, tape, json.get(doc, tape, 0, "f")) == 0.1);
            let list = json.get(doc, tape, 0, "list");
            test.assert(json.to_bool(tape, json.at(tape, list, 0)));
            test.assert(json.is_null(tape, json.at(tape, list, 1)));
        }
    }
    json.drop(heap, w);
    return 0;
}

// Every float written is read back as the same float: the property that
// makes the pair a serialization format and not a printer.
fn test_floats_round_trip_exactly[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 64);
    w = json.begin_array(heap, w);
    var count = 0;
    var x = 1.0;
    var i = 0;
    while i < 1500 {
        // Values over the whole exponent range, with long mantissas.
        let v = float_of(i * 7919 + 13) / 3.0 * math_scale(i);
        w = json.put_float(heap, w, v);
        w = json.put_float(heap, w, 0.0 - v);
        count = count + 2;
        i = i + 1;
    }
    w = json.end_array(heap, w);
    borrow w as &r in {
        let doc = json.bytes(r);
        let tape = box_slice(heap, json.tape_len(doc), 0);
        borrow mut tape as &!t in {
            let nodes = json.parse(doc, contents(t));
            test.assert_eq(nodes, count + 1);
            var k = 0;
            while k < 1500 {
                let v = float_of(k * 7919 + 13) / 3.0 * math_scale(k);
                test.assert_eq(bits_of(json.to_float(doc, contents(t), 1 + 2 * k)), bits_of(v));
                test.assert_eq(bits_of(json.to_float(doc, contents(t), 2 + 2 * k)), bits_of(0.0 - v));
                k = k + 1;
            }
        }
        unbox_slice(heap, tape);
    }
    json.drop(heap, w);
    return 0;
}

// A power of two from 2^-1000 to 2^1000, scrambled, by exact multiplication.
fn math_scale(i: int) -> [] float {
    var scale = 1.0;
    var e = i * 37 % 2000 - 1000;
    var step = 2.0;
    if e < 0 {
        step = 0.5;
        e = 0 - e;
    }
    while e > 0 {
        scale = scale * step;
        e = e - 1;
    }
    return scale;
}

// A fragment goes where a value goes: after a key, between array elements,
// as a whole document -- and the writer supplies the commas and colons.
fn test_put_fragment_is_a_value_like_any_other[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 16);
    w = json.begin_object(heap, w);
    w = json.put_key(heap, w, "user");
    w = json.put_fragment(heap, w, "{\"id\":1,\"tags\":[\"a\",\"b\"]}");
    w = json.put_key(heap, w, "items");
    w = json.begin_array(heap, w);
    w = json.put_fragment(heap, w, "1");
    w = json.put_int(heap, w, 2);
    w = json.put_fragment(heap, w, "[3, 4]");
    w = json.put_fragment(heap, w, "null");
    w = json.end_array(heap, w);
    w = json.put_key(heap, w, "last");
    w = json.put_fragment(heap, w, "\"s\"");
    w = json.end_object(heap, w);
    borrow w as &wr in {
        let want = "{\"user\":{\"id\":1,\"tags\":[\"a\",\"b\"]},\"items\":[1,2,[3, 4],null],\"last\":\"s\"}";
        test.assert(bytes.equal(json.bytes(wr), want));
    }
    json.drop(heap, w);
    return 0;
}

fn test_a_fragment_may_be_the_whole_document_and_may_have_whitespace_around_it[&h](heap: &!h Heap) -> [heap] int {
    var w = json.writer(heap, 16);
    w = json.put_fragment(heap, w, "  {\"a\": 1}\n");
    borrow w as &wr in {
        test.assert(bytes.equal(json.bytes(wr), "  {\"a\": 1}\n"));
    }
    json.drop(heap, w);
    return 0;
}

// What is spliced in reads back: the document the writer made parses, and the
// fragment is in it unchanged.
fn test_a_document_with_fragments_reads_back[&h](heap: &!h Heap) -> [heap] int {
    var inner = json.writer(heap, 16);
    inner = json.begin_object(heap, inner);
    inner = json.put_key(heap, inner, "n");
    inner = json.put_int(heap, inner, 42);
    inner = json.end_object(heap, inner);
    let kept = json.finish(inner);
    var w = json.writer(heap, 16);
    w = json.begin_array(heap, w);
    borrow kept as &kr in {
        w = json.put_fragment(heap, w, buffer.bytes(kr));
        w = json.put_fragment(heap, w, buffer.bytes(kr));
    }
    w = json.end_array(heap, w);
    buffer.drop(heap, kept);
    borrow w as &wr in {
        let doc = json.bytes(wr);
        let tape = box_slice(heap, json.tape_len(doc), 0);
        borrow mut tape as &!tw in {
            let t = contents(tw);
            test.assert(json.parse(doc, t) > 0);
            test.assert_eq(json.count(t, 0), 2);
            test.assert_eq(json.to_int(doc, t, json.get(doc, t, json.at(t, 0, 1), "n")), 42);
        }
        unbox_slice(heap, tape);
    }
    json.drop(heap, w);
    return 0;
}
