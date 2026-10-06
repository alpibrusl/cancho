// `docs/http.md` §7: what a request costs to parse and route.
//
//     http_bench <rounds> <routes>
//
// Parses one typical request (a browser-sized head with nine headers)
// `rounds` times, and for each looks the path up in a router of `routes`
// parameterised routes plus a few static ones, adding up the answers so
// nothing is dropped. Prints the sum. Timed from outside as the difference
// between two round counts.
import std.buffer;
import std.http;
import std.io;
import std.route;

fn number_at[&g](args: &g Args, at: int, otherwise: int) -> [args] int {
    if arg_count(args) <= at {
        return otherwise;
    }
    let a = arg(args, at);
    var n = 0;
    var i = 0;
    while i < len(a) {
        n = n * 10 + (int_of(a[i]) - 48);
        i = i + 1;
    }
    return n;
}

fn build_router[&h](heap: &!h Heap, routes: int) -> [heap] route.Router {
    var r = route.empty(heap);
    r = route.add(heap, r, "GET", "/health", 1);
    r = route.add(heap, r, "GET", "/", 2);
    var k = 0;
    while k < routes {
        let name = buffer.append(heap, buffer.empty(heap, 32), "/svc");
        let numbered = buffer.push_nat(heap, name, k);
        let pattern = buffer.append(heap, numbered, "/items/:id/parts/:part");
        borrow pattern as &p in {
            r = route.add(heap, r, "GET", buffer.bytes(p), 100 + k);
        }
        buffer.drop(heap, pattern);
        k = k + 1;
    }
    return route.add(heap, r, "GET", "/users/:id/orders/:order", 3);
}

fn run[&h, &i](heap: &!h Heap, io: &!i Io, rounds: int, routes: int) -> [heap, io_write] int {
    let request = "GET /users/12345/orders/987?expand=items&page=2 HTTP/1.1\r\nHost: api.example.com\r\nUser-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36\r\nAccept: application/json, text/plain, */*\r\nAccept-Language: en-US,en;q=0.9\r\nAccept-Encoding: gzip, deflate\r\nConnection: keep-alive\r\nAuthorization: Bearer abcdefghijklmnopqrstuvwxyz0123456789\r\nCookie: session=abc123; theme=dark\r\nX-Request-Id: 7f3a9c1e-55b2-4c3d-9a10-0e1f2a3b4c5d\r\n\r\n";
    var router = build_router(heap, routes);
    var widest = 0;
    borrow router as &r in {
        widest = route.most_params(r);
    }
    let table = box_slice(heap, http.slots(32), 0);
    let params = box_slice(heap, 2 * widest, 0);
    var sum = 0;
    borrow router as &r in {
        borrow mut table as &!tw in {
            borrow mut params as &!pw in {
                let t = contents(tw);
                let p = contents(pw);
                var round = 0;
                while round < rounds {
                    let n = http.parse(request, t);
                    sum = sum + n + route.find(r, http.method(request, t), http.path(request, t), p) + p[1];
                    round = round + 1;
                }
            }
        }
    }
    io.print_int(io, sum);
    io.newline(io);
    unbox_slice(heap, params);
    unbox_slice(heap, table);
    route.drop(heap, router);
    return sum;
}

fn main(world: World) -> [] int {
    let Split { io, ffi, fs, heap, args } = split(world);
    release(ffi);
    release(fs);
    var rounds = 100000;
    var routes = 10;
    borrow args as &g in {
        rounds = number_at(g, 1, rounds);
        routes = number_at(g, 2, routes);
    }
    borrow mut heap as &!h in {
        borrow mut io as &!i in {
            run(h, i, rounds, routes);
        }
    }
    release(args);
    release(io);
    release(heap);
    return 0;
}
