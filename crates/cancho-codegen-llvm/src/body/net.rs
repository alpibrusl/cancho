//! `bind` and `connect` (`docs/net.md` §2.1, §4.1; `docs/listen.md` §6;
//! `docs/connect.md` §10): all four of `Net`'s builtins this backend
//! lowers, `listen`/`accept` (§7.20) and `bind` (§7.21) first.

use crate::*;

/// `struct addrinfo`'s byte offsets that agree on every target this
/// project supports -- the same constants `cancho-codegen`'s own
/// `body/net.rs` declares, POSIX's `<netdb.h>` fixing the four `int`s and
/// `ai_addrlen` first, `ai_next` last, the same way on glibc and on
/// Darwin's libc.
const AI_FLAGS: i32 = 0;
const AI_FAMILY: i32 = 4;
const AI_SOCKTYPE: i32 = 8;
const AI_PROTOCOL: i32 = 12;
const AI_ADDRLEN: i32 = 16;
const AI_NEXT: i32 = 40;
const ADDRINFO_SIZE: i64 = 48;

/// A bound's port half, read at compile time: `None` for no restriction,
/// `Some(port)` for one that names a port. Mirrors `cancho-codegen`'s own
/// `port_bound_of` (`body/net.rs`) exactly -- a bound whose port half does
/// not parse as a number reads as `-1`, a port `bind`/`connect` can never
/// be asked for, rather than as `None`, so a narrowing that failed to
/// *tighten* as intended still enforces something (`docs/listen.md`
/// §6.2).
pub(super) fn port_bound_of(text: &str) -> Option<i64> {
    if text.is_empty() { None } else { Some(text.parse().unwrap_or(-1)) }
}

impl<'a> FuncEmitter<'a> {
    /// Store one byte -- a constant (`"2"`) or a register (`%t3`) -- at
    /// `base + offset`.
    pub(crate) fn store_byte(&mut self, base: &str, offset: i32, byte: &str) {
        self.store_field(base, offset, "i8", byte);
    }

    /// Store any fixed-width field -- `store_byte`'s general form, needed
    /// once `connect`'s `struct addrinfo hints` has `i32` and `ptr`
    /// fields alongside the `i8`s `bind`'s own `sockaddr_in` was all of.
    pub(crate) fn store_field(&mut self, base: &str, offset: i32, ty: &str, value: &str) {
        let addr = self.fresh();
        self.out.push_str(&format!("  {addr} = getelementptr i8, ptr {base}, i64 {offset}\n"));
        self.out.push_str(&format!("  store {ty} {value}, ptr {addr}\n"));
    }

    /// Load any fixed-width field at `base + offset` -- `store_field`'s
    /// mirror, needed to read `ai_addr`/`ai_addrlen` back out of the
    /// `struct addrinfo` `getaddrinfo` filled in.
    pub(crate) fn load_field(&mut self, base: &str, offset: i32, ty: &str) -> String {
        let addr = self.fresh();
        self.out.push_str(&format!("  {addr} = getelementptr i8, ptr {base}, i64 {offset}\n"));
        let reg = self.fresh();
        self.out.push_str(&format!("  {reg} = load {ty}, ptr {addr}\n"));
        reg
    }

    /// `bind(net, port)` (`docs/listen.md` §6): the inbound mirror of the
    /// still-unbuilt `connect`. Folds `socket`, `setsockopt(SO_REUSEADDR)`
    /// and `bind` into one call, building the same `struct sockaddr_in`
    /// `cancho-codegen`'s own `bind` builds by hand and
    /// `examples/serve/serve.cho` builds by hand again -- family bytes, the
    /// port big-endian, then `INADDR_ANY`: eight zero bytes where
    /// `connect`'s own has four octets, because a listener binds every
    /// address the host has. `args` is the capability (zero-sized,
    /// stopping here) and the port.
    ///
    /// Unlike `Sqrt`/`Listen`/`Accept`, this cannot be one straight-line
    /// call: `socket`/`bind` can each fail, and a failure returns `-1`
    /// rather than trapping (only a bound mismatch traps). This backend
    /// builds no `phi`, the same rule `if_stmt`/`while_stmt` already
    /// follow, so the two failure paths and the success path each store
    /// into one `alloca i64` result cell instead of merging through a
    /// block parameter.
    pub(crate) fn bind(&mut self, bound: &str, args: &[Expr]) -> Result<Vec<LValue>, String> {
        let port = self.scalar(&args[1])?;

        // §6.1: the bound is the port alone, not `"host:port"`, so there
        // is no host half to split off first.
        if let Some(expected) = port_bound_of(bound) {
            let wrong_port = self.fresh();
            self.out.push_str(&format!(
                "  {wrong_port} = icmp ne i64 {}, {expected}\n",
                operand(&port)
            ));
            self.trap_if(&wrong_port)?;
        }

        let addr = self.fresh();
        self.hoist(format!("  {addr} = alloca i8, i64 16\n"));
        // Family bytes -- `2, 0` -- the same choice `connect`'s own
        // `docs/connect.md` §3 measured: BSD kernels read family `0` as
        // `AF_INET` too, for backward compatibility, so writing only the
        // Linux byte layout works on both targets with no branch.
        self.store_byte(&addr, 0, "2");
        self.store_byte(&addr, 1, "0");
        let port32 = self.fresh();
        self.out.push_str(&format!("  {port32} = trunc i64 {} to i32\n", operand(&port)));
        let high32 = self.fresh();
        self.out.push_str(&format!("  {high32} = lshr i32 {port32}, 8\n"));
        let high8 = self.fresh();
        self.out.push_str(&format!("  {high8} = trunc i32 {high32} to i8\n"));
        let low8 = self.fresh();
        self.out.push_str(&format!("  {low8} = trunc i32 {port32} to i8\n"));
        self.store_byte(&addr, 2, &high8);
        self.store_byte(&addr, 3, &low8);
        // `INADDR_ANY`: every remaining byte, including the address
        // itself, is zero.
        for offset in 4..16 {
            self.store_byte(&addr, offset, "0");
        }

        let fd = self.tcp_socket();

        let result_cell = self.fresh();
        self.hoist(format!("  {result_cell} = alloca i64\n"));
        let minus_one = LValue::Const(-1);

        let bad_socket = self.fresh();
        self.out.push_str(&format!("  {bad_socket} = icmp slt i32 {fd}, 0\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (no_socket, have_socket, bound_ok, bind_failed, merge) = (
            format!("nosocket{n}"),
            format!("havesocket{n}"),
            format!("boundok{n}"),
            format!("bindfailed{n}"),
            format!("bindmerge{n}"),
        );
        self.out
            .push_str(&format!("  br i1 {bad_socket}, label %{no_socket}, label %{have_socket}\n"));

        self.out.push_str(&format!("{no_socket}:\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {result_cell}\n", operand(&minus_one)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{have_socket}:\n"));
        // `SOL_SOCKET` (1), `SO_REUSEADDR` (2): a C `int`, four bytes,
        // least significant first, the same value `serve.cho` assembles by
        // hand (`docs/reach.md` §3.2).
        let reuse = self.fresh();
        self.hoist(format!("  {reuse} = alloca i8, i64 4\n"));
        self.store_byte(&reuse, 0, "1");
        self.store_byte(&reuse, 1, "0");
        self.store_byte(&reuse, 2, "0");
        self.store_byte(&reuse, 3, "0");
        self.out.push_str(&format!(
            "  call i32 @setsockopt(i32 {fd}, i32 1, i32 2, ptr {reuse}, i32 4)\n"
        ));

        let bind_result = self.fresh();
        self.out
            .push_str(&format!("  {bind_result} = call i32 @bind(i32 {fd}, ptr {addr}, i32 16)\n"));
        let ok = self.fresh();
        self.out.push_str(&format!("  {ok} = icmp eq i32 {bind_result}, 0\n"));
        self.out.push_str(&format!("  br i1 {ok}, label %{bound_ok}, label %{bind_failed}\n"));

        self.out.push_str(&format!("{bound_ok}:\n"));
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = sext i32 {fd} to i64\n"));
        self.out.push_str(&format!("  store i64 {fd64}, ptr {result_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{bind_failed}:\n"));
        self.out.push_str(&format!("  call i32 @close(i32 {fd})\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {result_cell}\n", operand(&minus_one)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = load i64, ptr {result_cell}\n"));
        Ok(vec![LValue::Reg(result)])
    }

    /// The bound a `Net` was narrowed to, checked against a dialled name
    /// at run time and NUL-terminated for `getaddrinfo` -- mirrors
    /// `cancho-codegen`'s own `checked_host` (`body/net.rs`) line for
    /// line, the loop built the same "no `phi`" way `bind`'s own byte
    /// loop already is: a cursor in an `alloca i64` cell rather than a
    /// block parameter.
    fn checked_host(&mut self, bound: &str, name: &[LValue]) -> Result<String, String> {
        const HOST_MAX: i64 = 256;
        let (source, length) = (operand(&name[0]), operand(&name[1]));

        let too_long = self.fresh();
        self.out.push_str(&format!("  {too_long} = icmp uge i64 {length}, {HOST_MAX}\n"));
        self.trap_if(&too_long)?;

        let short = self.fresh();
        self.out.push_str(&format!("  {short} = icmp ult i64 {length}, {}\n", bound.len()));
        self.trap_if(&short)?;

        let buffer = self.fresh();
        self.hoist(format!("  {buffer} = alloca i8, i64 {HOST_MAX}\n"));
        let expected = operand(&self.bytes_lit(bound)[0]);

        let cursor = self.fresh();
        self.hoist(format!("  {cursor} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 0, ptr {cursor}\n"));

        let n = self.blocks;
        self.blocks += 1;
        let (header, body, done) =
            (format!("hosthdr{n}"), format!("hostbody{n}"), format!("hostdone{n}"));
        self.out.push_str(&format!("  br label %{header}\n"));

        self.out.push_str(&format!("{header}:\n"));
        let i = self.fresh();
        self.out.push_str(&format!("  {i} = load i64, ptr {cursor}\n"));
        let more = self.fresh();
        self.out.push_str(&format!("  {more} = icmp ult i64 {i}, {length}\n"));
        self.out.push_str(&format!("  br i1 {more}, label %{body}, label %{done}\n"));

        self.out.push_str(&format!("{body}:\n"));
        let at = self.fresh();
        self.out.push_str(&format!("  {at} = getelementptr i8, ptr {source}, i64 {i}\n"));
        let byte = self.fresh();
        self.out.push_str(&format!("  {byte} = load i8, ptr {at}\n"));
        let into = self.fresh();
        self.out.push_str(&format!("  {into} = getelementptr i8, ptr {buffer}, i64 {i}\n"));
        self.out.push_str(&format!("  store i8 {byte}, ptr {into}\n"));

        // Inside the bound, the bytes have to match -- a plain prefix,
        // with no separator to land on (`docs/net.md` §4).
        self.check_against(&expected, bound.len(), &i, &byte)?;

        let next = self.fresh();
        self.out.push_str(&format!("  {next} = add i64 {i}, 1\n"));
        self.out.push_str(&format!("  store i64 {next}, ptr {cursor}\n"));
        self.out.push_str(&format!("  br label %{header}\n"));

        self.out.push_str(&format!("{done}:\n"));
        let end = self.fresh();
        self.out.push_str(&format!("  {end} = getelementptr i8, ptr {buffer}, i64 {length}\n"));
        self.out.push_str(&format!("  store i8 0, ptr {end}\n"));

        Ok(buffer)
    }

    /// `connect(net, name, port)` (`docs/connect.md` §10): checks `name`
    /// against the bound, resolves it with `getaddrinfo`, patches the
    /// port into whatever `sockaddr` the resolver filled in, and
    /// connects. Mirrors `cancho-codegen`'s own `connect` line for line;
    /// like `bind`, `getaddrinfo`/`socket`/`connect` can each fail
    /// without trapping, so the three failure paths and the one success
    /// path all store into one `alloca i64` result cell rather than
    /// merging through a block parameter.
    pub(crate) fn connect(&mut self, bound: &str, args: &[Expr]) -> Result<Vec<LValue>, String> {
        let (fd, _) = self.connect_raw(bound, args, false, false, false)?;
        Ok(vec![fd])
    }

    /// [`Self::connect`]'s walk, answering the descriptor **and why it
    /// failed**: `(fd, errno)`, with `fd == -1` on failure and `errno == -1`
    /// when the *name* did not resolve (no `errno` has that value, so a
    /// program can tell the resolver from the kernel). `connection` is
    /// `tcp_connect`'s: the descriptor becomes a `Conn`, so on Darwin it
    /// gets `SO_NOSIGPIPE` (`docs/native-sockets.md` §3).
    pub(crate) fn connect_raw(
        &mut self,
        bound: &str,
        args: &[Expr],
        connection: bool,
        start: bool,
        datagram: bool,
    ) -> Result<(LValue, LValue), String> {
        let name = self.expr(&args[1])?;
        let port = self.scalar(&args[2])?;

        // The bound is `"host:port"` (`docs/net.md` §4); split once, at
        // compile time. A bound with no `:` -- `""`, unnarrowed,
        // included -- restricts the port to none.
        let (host_bound, port_bound) = match bound.rsplit_once(':') {
            Some((host, digits)) => (host, port_bound_of(digits)),
            None => (bound, None),
        };
        if let Some(expected) = port_bound {
            let wrong_port = self.fresh();
            self.out.push_str(&format!(
                "  {wrong_port} = icmp ne i64 {}, {expected}\n",
                operand(&port)
            ));
            self.trap_if(&wrong_port)?;
        }
        let host = self.checked_host(host_bound, &name)?;

        // `ai_addr` and `ai_canonname` swap places between glibc and
        // Darwin's libc (`docs/connect.md` §10.2's table) -- every other
        // field agrees.
        let (ai_addr, ai_canonname) = match self.triple.operating_system {
            target_lexicon::OperatingSystem::Darwin(_) => (32, 24),
            _ => (24, 32),
        };

        // `struct addrinfo hints`, zeroed except the two fields that ask
        // for one address family and one socket kind -- IPv4 and TCP, the
        // only shape this project has ever built a `sockaddr_in` for.
        let hints = self.fresh();
        self.hoist(format!("  {hints} = alloca i8, i64 {ADDRINFO_SIZE}\n"));
        self.store_field(&hints, AI_FLAGS, "i32", "0");
        self.store_field(&hints, AI_FAMILY, "i32", "2");
        self.store_field(&hints, AI_SOCKTYPE, "i32", if datagram { "2" } else { "1" });
        self.store_field(&hints, AI_PROTOCOL, "i32", "0");
        self.store_field(&hints, AI_ADDRLEN, "i32", "0");
        self.store_field(&hints, ai_addr, "ptr", "null");
        self.store_field(&hints, ai_canonname, "ptr", "null");
        self.store_field(&hints, AI_NEXT, "ptr", "null");

        let res_slot = self.fresh();
        self.hoist(format!("  {res_slot} = alloca ptr\n"));

        // `service` is always `NULL`: the port is read out of the
        // bound's own text and patched in afterward, not looked up by
        // name (`docs/connect.md` §10.2's reasons).
        let resolve_result = self.fresh();
        self.out.push_str(&format!(
            "  {resolve_result} = call i32 @getaddrinfo(ptr {host}, ptr null, ptr {hints}, ptr {res_slot})\n"
        ));

        let result_cell = self.fresh();
        self.hoist(format!("  {result_cell} = alloca i64\n"));
        let reason_cell = self.fresh();
        self.hoist(format!("  {reason_cell} = alloca i64\n"));
        self.out.push_str(&format!("  store i64 0, ptr {reason_cell}\n"));
        let minus_one = LValue::Const(-1);

        let failed_resolve = self.fresh();
        self.out.push_str(&format!("  {failed_resolve} = icmp ne i32 {resolve_result}, 0\n"));
        let n = self.blocks;
        self.blocks += 1;
        let (not_resolved, resolved, no_socket, have_socket, connected, not_connected, merge) = (
            format!("notresolved{n}"),
            format!("resolved{n}"),
            format!("nosocket{n}"),
            format!("havesocket{n}"),
            format!("connected{n}"),
            format!("notconnected{n}"),
            format!("connectmerge{n}"),
        );
        self.out.push_str(&format!(
            "  br i1 {failed_resolve}, label %{not_resolved}, label %{resolved}\n"
        ));

        self.out.push_str(&format!("{not_resolved}:\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {result_cell}\n", operand(&minus_one)));
        self.out.push_str(&format!("  store i64 -1, ptr {reason_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{resolved}:\n"));
        // The first result is enough: this program connects once, and
        // every candidate is the one host it asked for.
        let res = self.fresh();
        self.out.push_str(&format!("  {res} = load ptr, ptr {res_slot}\n"));
        let addr = self.load_field(&res, ai_addr, "ptr");
        let addrlen = self.load_field(&res, AI_ADDRLEN, "i32");

        // The port, big-endian, at the one offset `connect.md` §3 found
        // portable: bytes 0-1 are where Linux and macOS disagree, and
        // `getaddrinfo` already wrote the platform's own correct bytes
        // there, so nothing here has to know which one it is running on.
        let port32 = self.fresh();
        self.out.push_str(&format!("  {port32} = trunc i64 {} to i32\n", operand(&port)));
        let high32 = self.fresh();
        self.out.push_str(&format!("  {high32} = lshr i32 {port32}, 8\n"));
        let high8 = self.fresh();
        self.out.push_str(&format!("  {high8} = trunc i32 {high32} to i8\n"));
        let low8 = self.fresh();
        self.out.push_str(&format!("  {low8} = trunc i32 {port32} to i8\n"));
        self.store_byte(&addr, 2, &high8);
        self.store_byte(&addr, 3, &low8);

        let fd = if datagram { self.udp_socket() } else { self.tcp_socket() };
        let bad_socket = self.fresh();
        self.out.push_str(&format!("  {bad_socket} = icmp slt i32 {fd}, 0\n"));
        self.out
            .push_str(&format!("  br i1 {bad_socket}, label %{no_socket}, label %{have_socket}\n"));

        self.out.push_str(&format!("{no_socket}:\n"));
        // `errno` before anything that could overwrite it.
        let reason = self.errno();
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  call void @freeaddrinfo(ptr {res})\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {result_cell}\n", operand(&minus_one)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{have_socket}:\n"));
        if start {
            // `tcp_connect_start` (`docs/native-sockets.md` §10.6): non-blocking *before*
            // `connect`, so that it answers `EINPROGRESS` instead of waiting.
            let os = self.os();
            let flags = self.fresh();
            self.out.push_str(&format!(
                "  {flags} = call i32 (i32, i32, ...) @fcntl(i32 {fd}, i32 {})\n",
                cancho_ir::F_GETFL
            ));
            let set = self.fresh();
            self.out.push_str(&format!("  {set} = or i32 {flags}, {}\n", os.o_nonblock as i32));
            let ignored = self.fresh();
            self.out.push_str(&format!(
                "  {ignored} = call i32 (i32, i32, ...) @fcntl(i32 {fd}, i32 {}, i32 {set})\n",
                cancho_ir::F_SETFL
            ));
        }
        let connect_result = self.fresh();
        self.out.push_str(&format!(
            "  {connect_result} = call i32 @connect(i32 {fd}, ptr {addr}, i32 {addrlen})\n"
        ));
        let mut ok = self.fresh();
        self.out.push_str(&format!("  {ok} = icmp eq i32 {connect_result}, 0\n"));
        if start {
            // A connection still in progress is a success here: the caller watches it for
            // *writable* and asks `conn_connect_status`. `errno` is read before anything else runs.
            let os = self.os();
            let reason = self.errno();
            let in_progress = self.fresh();
            self.out.push_str(&format!(
                "  {in_progress} = icmp eq i64 {}, {}\n",
                operand(&reason),
                os.einprogress
            ));
            let negative = self.fresh();
            self.out.push_str(&format!("  {negative} = icmp slt i32 {connect_result}, 0\n"));
            let pending = self.fresh();
            self.out.push_str(&format!("  {pending} = and i1 {in_progress}, {negative}\n"));
            let either = self.fresh();
            self.out.push_str(&format!("  {either} = or i1 {ok}, {pending}\n"));
            ok = either;
        }
        self.out.push_str(&format!("  br i1 {ok}, label %{connected}, label %{not_connected}\n"));

        self.out.push_str(&format!("{connected}:\n"));
        self.out.push_str(&format!("  call void @freeaddrinfo(ptr {res})\n"));
        if connection {
            self.suppress_sigpipe(&fd);
        }
        let fd64 = self.fresh();
        self.out.push_str(&format!("  {fd64} = sext i32 {fd} to i64\n"));
        self.out.push_str(&format!("  store i64 {fd64}, ptr {result_cell}\n"));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{not_connected}:\n"));
        let reason = self.errno();
        self.out.push_str(&format!("  store i64 {}, ptr {reason_cell}\n", operand(&reason)));
        self.out.push_str(&format!("  call i32 @close(i32 {fd})\n"));
        self.out.push_str(&format!("  call void @freeaddrinfo(ptr {res})\n"));
        self.out.push_str(&format!("  store i64 {}, ptr {result_cell}\n", operand(&minus_one)));
        self.out.push_str(&format!("  br label %{merge}\n"));

        self.out.push_str(&format!("{merge}:\n"));
        let result = self.fresh();
        self.out.push_str(&format!("  {result} = load i64, ptr {result_cell}\n"));
        let reason = self.fresh();
        self.out.push_str(&format!("  {reason} = load i64, ptr {reason_cell}\n"));
        Ok((LValue::Reg(result), LValue::Reg(reason)))
    }

    /// `tcp_connect(net, host, port)` (`docs/native-sockets.md` §3):
    /// `connect`'s check and walk, answering `Dialed`'s three leaves --
    /// `Ok` 0 with the descriptor, `Failed` 1 with the reason.
    pub(crate) fn tcp_connect(
        &mut self,
        bound: &str,
        args: &[Expr],
        start: bool,
        datagram: bool,
    ) -> Result<Vec<LValue>, String> {
        let (fd, reason) = self.connect_raw(bound, args, !datagram, start, datagram)?;
        let failed = self.fresh();
        self.out.push_str(&format!("  {failed} = icmp slt i64 {}, 0\n", operand(&fd)));
        let tag = self.fresh();
        self.out.push_str(&format!("  {tag} = select i1 {failed}, i64 1, i64 0\n"));
        Ok(vec![LValue::Reg(tag), fd, reason])
    }
}
