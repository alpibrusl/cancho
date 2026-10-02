edition 5;
module std.conns;

import std.vec;

// `std.conns` -- many connections, by slot number.
//
// A server holds a thousand connections and finds each by the number the
// `Poller` gave back. `Conn` is a resource and `std.vec` holds only
// copyable things (`docs/collections.md`), so the connections cannot simply
// go in a vector -- and making `Conn` copyable would bring back exactly what
// a typed handle is for: a stale copy that closes the wrong connection after
// the descriptor is reused.
//
// So a connection in a `Table` is a **ticket**: `conn_detach` ends the
// `Conn` and answers an integer, `conn_attach` turns that integer back into
// the `Conn` exactly once, and the runtime's per-descriptor epoch refuses a
// ticket that was never issued, was already redeemed, or was copied
// (`docs/native-sockets.md` §10.3). The integer is not authority: forging one
// reaches nothing.
//
// Every operation here is `attach`, the one builtin, `detach`. The `Conn` is
// a resource for exactly that long and is back in the table when the
// function returns, which is what keeps the linearity the checker enforces
// everywhere else from stopping at the table's edge.
//
// A `Table` must be ended with `drop`, which closes every connection still
// in it. Slot numbers are reused, newest freed first.

pub res struct Table {
    // Slot -> ticket for a live connection (never negative), or, for a
    // free slot, `0 - 2 - next_free` so the free slots form a chain
    // without needing the heap to remember them.
    tickets: vec.Vec[int],
    // The first free slot, or -1.
    free_head: int,
    live: int,
}

pub fn empty[&h](heap: &!h Heap, capacity: int) -> [heap] Table {
    return Table { tickets: vec.empty(heap, capacity, 0 - 1), free_head: 0 - 1, live: 0 };
}

// How many connections are in it.
pub fn live[&t](table: &t Table) -> [] int {
    return table.live;
}

// How many slots there are, free or not: slot numbers run `0..slots`, which
// is what a sweep over every connection walks.
pub fn slots[&t](table: &t Table) -> [] int {
    return vec.size(table.tickets);
}

// The ticket in `slot`, or -1 for a slot that is free or out of range.
fn ticket_at[&t](table: &t Table, slot: int) -> [] int {
    if slot < 0 || slot >= vec.size(table.tickets) {
        return 0 - 1;
    }
    let ticket = vec.get(table.tickets, slot);
    if ticket < 0 {
        return 0 - 1;
    }
    return ticket;
}

// Put a connection in, answering its slot -- or -1 if the runtime could not
// ticket it (a descriptor too large for the epoch table), in which case it
// has been closed.
pub fn put[&h](heap: &!h Heap, table: Table, conn: Conn) -> [heap] (Table, int) {
    let ticket = conn_detach(conn);
    if ticket < 0 {
        return (table, 0 - 1);
    }
    let Table { tickets, free_head, live } = table;
    var held = tickets;
    if free_head >= 0 {
        var next = 0 - 1;
        borrow mut held as &!v in {
            next = 0 - 2 - vec.get(v, free_head);
            vec.set(v, free_head, ticket);
        }
        return (Table { tickets: held, free_head: next, live: live + 1 }, free_head);
    }
    var slot = 0;
    borrow held as &v in {
        slot = vec.size(v);
    }
    held = vec.push(heap, held, ticket);
    return (Table { tickets: held, free_head: free_head, live: live + 1 }, slot);
}

// Give a slot back after its connection is gone.
fn release_slot[&t](table: &!t Table, slot: int) -> [] int {
    vec.set(table.tickets, slot, 0 - 2 - table.free_head);
    table.free_head = slot;
    table.live = table.live - 1;
    return 0;
}

// Read from the connection in `slot`. A slot with nothing in it is
// `Failed(EBADF)`, as a stale ticket is.
pub fn read[&t, &b](table: &!t Table, slot: int, into: &!b [byte]) -> [conn_read] Received {
    let ticket = ticket_at(table, slot);
    if ticket < 0 {
        return Received::Failed(9);
    }
    match conn_attach(ticket) {
        Attached::Ok(c) => {
            var conn = c;
            var answer = Received::Failed(9);
            borrow mut conn as &!h in {
                answer = conn_read(h, into);
            }
            let back = conn_detach(conn);
            if back < 0 {
                release_slot(table, slot);
            } else {
                vec.set(table.tickets, slot, back);
            }
            return answer;
        }
        Attached::Failed(e) => {
            return Received::Failed(e);
        }
    }
}

// Write to the connection in `slot`.
pub fn write[&t, &b](table: &!t Table, slot: int, data: &b [byte]) -> [conn_write] Sent {
    let ticket = ticket_at(table, slot);
    if ticket < 0 {
        return Sent::Failed(9);
    }
    match conn_attach(ticket) {
        Attached::Ok(c) => {
            var conn = c;
            var answer = Sent::Failed(9);
            borrow mut conn as &!h in {
                answer = conn_write(h, data);
            }
            let back = conn_detach(conn);
            if back < 0 {
                release_slot(table, slot);
            } else {
                vec.set(table.tickets, slot, back);
            }
            return answer;
        }
        Attached::Failed(e) => {
            return Sent::Failed(e);
        }
    }
}

// Make the connection in `slot` non-blocking. `0`, or an `errno`.
pub fn nonblocking[&t](table: &!t Table, slot: int) -> [] int {
    let ticket = ticket_at(table, slot);
    if ticket < 0 {
        return 9;
    }
    match conn_attach(ticket) {
        Attached::Ok(c) => {
            var conn = c;
            var answer = 9;
            borrow mut conn as &!h in {
                answer = conn_nonblocking(h);
            }
            let back = conn_detach(conn);
            if back < 0 {
                release_slot(table, slot);
            } else {
                vec.set(table.tickets, slot, back);
            }
            return answer;
        }
        Attached::Failed(e) => {
            return e;
        }
    }
}

// Turn `TCP_NODELAY` on for the connection in `slot`: what is written goes out at once instead of waiting for the acknowledgement
// of what was written before it (which a peer delays by tens of milliseconds). `0`, or an `errno`.
pub fn nodelay[&t](table: &!t Table, slot: int) -> [] int {
    let ticket = ticket_at(table, slot);
    if ticket < 0 {
        return 9;
    }
    match conn_attach(ticket) {
        Attached::Ok(c) => {
            var conn = c;
            var answer = 9;
            borrow mut conn as &!h in {
                answer = conn_nodelay(h);
            }
            let back = conn_detach(conn);
            if back < 0 {
                release_slot(table, slot);
            } else {
                vec.set(table.tickets, slot, back);
            }
            return answer;
        }
        Attached::Failed(e) => {
            return e;
        }
    }
}

// Watch the connection in `slot`, as `poller_add_conn` does: `events` is 1
// for readable, 2 for writable, and what the poller reports back is `token`.
pub fn watch[&t, &p](table: &!t Table, poller: &!p Poller, slot: int, token: int, events: int) -> [poll] int {
    return register(table, poller, slot, token, events, false);
}

// Change what the connection in `slot` is watched for.
pub fn rewatch[&t, &p](table: &!t Table, poller: &!p Poller, slot: int, token: int, events: int) -> [poll] int {
    return register(table, poller, slot, token, events, true);
}

fn register[&t, &p](table: &!t Table, poller: &!p Poller, slot: int, token: int, events: int, modify: bool) -> [poll] int {
    let ticket = ticket_at(table, slot);
    if ticket < 0 {
        return 9;
    }
    match conn_attach(ticket) {
        Attached::Ok(c) => {
            var conn = c;
            var answer = 9;
            borrow conn as &h in {
                if modify {
                    answer = poller_modify(poller, h, token, events);
                } else {
                    answer = poller_add_conn(poller, h, token, events);
                }
            }
            let back = conn_detach(conn);
            if back < 0 {
                release_slot(table, slot);
            } else {
                vec.set(table.tickets, slot, back);
            }
            return answer;
        }
        Attached::Failed(e) => {
            return e;
        }
    }
}

// Close the connection in `slot` and free the slot. `0`, or an `errno`
// (`EBADF` for a slot with nothing in it).
pub fn close[&t](table: &!t Table, slot: int) -> [] int {
    let ticket = ticket_at(table, slot);
    if ticket < 0 {
        return 9;
    }
    match conn_attach(ticket) {
        Attached::Ok(c) => {
            let answer = conn_close(c);
            release_slot(table, slot);
            return answer;
        }
        Attached::Failed(e) => {
            return e;
        }
    }
}

// End the table, closing every connection still in it. Answers how many it
// closed.
pub fn drop[&h](heap: &!h Heap, table: Table) -> [heap] int {
    let Table { tickets, free_head, live } = table;
    var held = tickets;
    var i = 0;
    var n = 0;
    borrow held as &v in {
        n = vec.size(v);
    }
    while i < n {
        var ticket = 0 - 1;
        borrow held as &v in {
            ticket = vec.get(v, i);
        }
        if ticket >= 0 {
            match conn_attach(ticket) {
                Attached::Ok(c) => {
                    conn_close(c);
                }
                Attached::Failed(e) => {
                }
            }
        }
        i = i + 1;
    }
    vec.drop(heap, held);
    return live;
}
