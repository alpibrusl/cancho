//! The socket handles (`docs/native-sockets.md` §3): `tcp_listen`,
//! `tcp_accept`, `conn_read`, `conn_write`, the non-blocking switches.
//! Mirrors `cancho-codegen-llvm`'s own `body/sockets.rs`; the numbers the
//! two kernels disagree on live in `cancho_ir::SocketOs`, once.

use super::net::port_bound_of;
use crate::*;
use cancho_ir::{EBADF, EINVAL, F_GETFL, F_SETFD, F_SETFL, FD_CLOEXEC, SocketOs};

impl<'a, 'f> BodyEmitter<'a, 'f> {
    pub(crate) fn is_darwin(&self) -> bool {
        matches!(
            self.module.isa().triple().operating_system,
            target_lexicon::OperatingSystem::Darwin(_)
        )
    }

    pub(crate) fn socket_os(&self) -> SocketOs {
        SocketOs::for_darwin(self.is_darwin())
    }

    /// One call to a libc function, answering its first result.
    pub(crate) fn libc_call(
        &mut self,
        name: &str,
        params: &[cranelift_codegen::ir::Type],
        returns: &[cranelift_codegen::ir::Type],
        args: &[Value],
    ) -> Value {
        let id = self.libc_fn(name, params, returns);
        let id = self.module.declare_func_in_func(id, self.builder.func);
        let call = self.builder.ins().call(id, args);
        self.builder.inst_results(call)[0]
    }

    /// `fcntl(fd, command, argument)`; `F_GETFL` ignores the argument.
    ///
    /// `fcntl` is variadic, and on Apple arm64 a variadic argument is passed
    /// on the *stack* where a fixed one goes in a register -- the reason
    /// `docs/native-sockets.md` §3 makes this a builtin. Cranelift cannot
    /// declare a variadic callee, so on that one target the call is shaped
    /// the way the callee reads it: nine integer parameters, the first
    /// eight in `x0..x7` and the ninth -- the one `va_arg` reads -- in the
    /// first stack slot. Everywhere else a variadic and a fixed call agree.
    /// One signature per module either way, because the module refuses two.
    pub(crate) fn fcntl(&mut self, fd: Value, command: i64, argument: Value) -> Value {
        let apple_arm64 = self.is_darwin()
            && matches!(
                self.module.isa().triple().architecture,
                target_lexicon::Architecture::Aarch64(_)
            );
        let command = self.builder.ins().iconst(types::I32, command);
        if apple_arm64 {
            let filler = self.builder.ins().iconst(types::I64, 0);
            let argument = self.builder.ins().uextend(types::I64, argument);
            let mut params = vec![types::I32, types::I32];
            params.extend([types::I64; 7]);
            let mut args = vec![fd, command];
            args.extend([filler; 6]);
            args.push(argument);
            self.libc_call("fcntl", &params, &[types::I32], &args)
        } else {
            self.libc_call(
                "fcntl",
                &[types::I32, types::I32, types::I32],
                &[types::I32],
                &[fd, command, argument],
            )
        }
    }

    /// Set `FD_CLOEXEC` on `fd` when it is a descriptor, for a platform that
    /// cannot ask for it in the call that made it (Darwin's `socket` and
    /// `accept`). A failed call is left alone, so the `errno` its caller reads
    /// next is still the call's own (`docs/processes.md` §4.5).
    pub(crate) fn close_on_exec(&mut self, fd: Value) {
        let set = self.builder.create_block();
        let done = self.builder.create_block();
        let opened = self.builder.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, fd, 0);
        self.builder.ins().brif(opened, set, &[], done, &[]);
        self.builder.switch_to_block(set);
        self.builder.seal_block(set);
        let flag = self.builder.ins().iconst(types::I32, FD_CLOEXEC);
        self.fcntl(fd, F_SETFD, flag);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
    }

    /// `socket(AF_INET, SOCK_STREAM, 0)`, close-on-exec: `SOCK_CLOEXEC` in
    /// the type on Linux, so no other thread's `exec` can see it open
    /// without the flag; `fcntl` straight after on Darwin, which has no such
    /// flag.
    pub(crate) fn tcp_socket(&mut self) -> Value {
        self.inet_socket(1)
    }

    /// `socket(AF_INET, SOCK_DGRAM, 0)`, close-on-exec the same way (`docs/udp.md` §3).
    pub(crate) fn udp_socket(&mut self) -> Value {
        self.inet_socket(2)
    }

    fn inet_socket(&mut self, kind: i64) -> Value {
        let os = self.socket_os();
        let domain = self.builder.ins().iconst(types::I32, 2);
        let kind = self.builder.ins().iconst(types::I32, kind | os.sock_cloexec);
        let proto = self.builder.ins().iconst(types::I32, 0);
        let fd = self.libc_call(
            "socket",
            &[types::I32, types::I32, types::I32],
            &[types::I32],
            &[domain, kind, proto],
        );
        if self.is_darwin() {
            self.close_on_exec(fd);
        }
        fd
    }

    /// `accept(fd, NULL, NULL)`, close-on-exec: `accept4` with
    /// `SOCK_CLOEXEC` on Linux, `accept` and `fcntl` on Darwin.
    pub(crate) fn accept_cloexec(&mut self, listener: Value) -> Value {
        let pointer = self.pointer;
        let null = self.builder.ins().iconst(pointer, 0);
        if self.is_darwin() {
            let fd = self.libc_call(
                "accept",
                &[types::I32, pointer, pointer],
                &[types::I32],
                &[listener, null, null],
            );
            self.close_on_exec(fd);
            fd
        } else {
            let cloexec = self.socket_os().sock_cloexec;
            let flags = self.builder.ins().iconst(types::I32, cloexec);
            self.libc_call(
                "accept4",
                &[types::I32, pointer, pointer, types::I32],
                &[types::I32],
                &[listener, null, null, flags],
            )
        }
    }

    /// Darwin has no `MSG_NOSIGNAL`: a socket opts out of `SIGPIPE` for
    /// good with `SO_NOSIGPIPE`. Linux does it per send, so this is nothing.
    pub(crate) fn suppress_sigpipe(&mut self, fd: Value) {
        if self.is_darwin() {
            let os = self.socket_os();
            self.set_int_option(fd, os.sol_socket, os.so_nosigpipe, 1);
        }
    }

    /// `setsockopt(fd, level, name, &value, 4)` with a C `int` value.
    fn set_int_option(&mut self, fd: Value, level: i64, name: i64, value: i64) {
        let pointer = self.pointer;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            4,
            2,
        ));
        let cell = self.builder.ins().stack_addr(pointer, slot, 0);
        let value = self.builder.ins().iconst(types::I32, value);
        self.builder.ins().store(MemFlags::trusted(), value, cell, 0);
        let level = self.builder.ins().iconst(types::I32, level);
        let name = self.builder.ins().iconst(types::I32, name);
        let len = self.builder.ins().iconst(types::I32, 4);
        self.libc_call(
            "setsockopt",
            &[types::I32, types::I32, types::I32, pointer, types::I32],
            &[types::I32],
            &[fd, level, name, cell, len],
        );
    }

    /// A handle's descriptor, read through the reference it arrived as.
    pub(crate) fn handle_fd(&mut self, reference: Value) -> Value {
        let fd = self.builder.ins().load(types::I64, MemFlags::trusted(), reference, 0);
        self.builder.ins().ireduce(types::I32, fd)
    }

    /// `tcp_listen(net, port, backlog, flags)` -- see the LLVM backend's
    /// own for the walk. Answers `Listening`'s three leaves: tag (`Ok` 0,
    /// `Failed` 1), descriptor, `errno` read before the `close` that would
    /// overwrite it.
    pub(crate) fn tcp_listen(&mut self, bound: &str, args: &[Expr], datagram: bool) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let port = self.scalar(&args[1]);
        let backlog = self.scalar(&args[2]);
        let flags = self.scalar(&args[3]);

        if let Some(expected) = port_bound_of(bound) {
            let wrong_port = self.builder.ins().icmp_imm(IntCC::NotEqual, port, expected);
            self.builder.ins().trapnz(wrong_port, TrapCode::HEAP_OUT_OF_BOUNDS);
        }

        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            16,
            0,
        ));
        let addr = self.builder.ins().stack_addr(pointer, slot, 0);
        let zero8 = self.builder.ins().iconst(types::I8, 0);
        let family = self.builder.ins().iconst(types::I8, 2);
        self.builder.ins().store(MemFlags::trusted(), family, addr, 0);
        self.builder.ins().store(MemFlags::trusted(), zero8, addr, 1);
        let port32 = self.builder.ins().ireduce(types::I32, port);
        let high = self.builder.ins().ushr_imm(port32, 8);
        let high = self.builder.ins().ireduce(types::I8, high);
        let low = self.builder.ins().ireduce(types::I8, port32);
        self.builder.ins().store(MemFlags::trusted(), high, addr, 2);
        self.builder.ins().store(MemFlags::trusted(), low, addr, 3);
        for i in 4..16i32 {
            self.builder.ins().store(MemFlags::trusted(), zero8, addr, i);
        }

        let fd = if datagram { self.udp_socket() } else { self.tcp_socket() };

        // merge(descriptor, errno)
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let minus_one = self.builder.ins().iconst(types::I64, -1);

        let no_socket = self.builder.create_block();
        let have_socket = self.builder.create_block();
        let bad_socket = self.builder.ins().icmp_imm(IntCC::SignedLessThan, fd, 0);
        self.builder.ins().brif(bad_socket, no_socket, &[], have_socket, &[]);

        self.builder.switch_to_block(no_socket);
        self.builder.seal_block(no_socket);
        let reason = self.errno();
        self.builder.ins().jump(merge, &[minus_one.into(), reason.into()]);

        self.builder.switch_to_block(have_socket);
        self.builder.seal_block(have_socket);
        self.set_int_option(fd, os.sol_socket, os.so_reuseaddr, 1);
        let wants = self.builder.ins().band_imm(flags, 1);
        let port_on = self.builder.ins().icmp_imm(IntCC::NotEqual, wants, 0);
        let reuse_port = self.builder.create_block();
        let after_port = self.builder.create_block();
        self.builder.ins().brif(port_on, reuse_port, &[], after_port, &[]);
        self.builder.switch_to_block(reuse_port);
        self.builder.seal_block(reuse_port);
        self.set_int_option(fd, os.sol_socket, os.so_reuseport, 1);
        self.builder.ins().jump(after_port, &[]);
        self.builder.switch_to_block(after_port);
        self.builder.seal_block(after_port);

        let len = self.builder.ins().iconst(types::I32, 16);
        let bound_result = self.libc_call(
            "bind",
            &[types::I32, pointer, types::I32],
            &[types::I32],
            &[fd, addr, len],
        );
        let bound_ok = self.builder.create_block();
        let bind_failed = self.builder.create_block();
        let ok = self.builder.ins().icmp_imm(IntCC::Equal, bound_result, 0);
        self.builder.ins().brif(ok, bound_ok, &[], bind_failed, &[]);

        self.builder.switch_to_block(bind_failed);
        self.builder.seal_block(bind_failed);
        let reason = self.errno();
        self.libc_call("close", &[types::I32], &[types::I32], &[fd]);
        self.builder.ins().jump(merge, &[minus_one.into(), reason.into()]);

        self.builder.switch_to_block(bound_ok);
        self.builder.seal_block(bound_ok);
        let backlog = self.builder.ins().ireduce(types::I32, backlog);
        // A datagram socket has nothing to listen for (`docs/udp.md` §2).
        let listened = if datagram {
            self.builder.ins().iconst(types::I32, 0)
        } else {
            self.libc_call("listen", &[types::I32, types::I32], &[types::I32], &[fd, backlog])
        };
        let listening = self.builder.create_block();
        let listen_failed = self.builder.create_block();
        let ok = self.builder.ins().icmp_imm(IntCC::Equal, listened, 0);
        self.builder.ins().brif(ok, listening, &[], listen_failed, &[]);

        self.builder.switch_to_block(listen_failed);
        self.builder.seal_block(listen_failed);
        let reason = self.errno();
        self.libc_call("close", &[types::I32], &[types::I32], &[fd]);
        self.builder.ins().jump(merge, &[minus_one.into(), reason.into()]);

        self.builder.switch_to_block(listening);
        self.builder.seal_block(listening);
        let fd64 = self.builder.ins().sextend(types::I64, fd);
        let no_error = self.builder.ins().iconst(types::I64, 0);
        self.builder.ins().jump(merge, &[fd64.into(), no_error.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let handle = self.builder.block_params(merge)[0];
        let reason = self.builder.block_params(merge)[1];
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, handle, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let tag = self.builder.ins().select(failed, one, zero);
        vec![tag, handle, reason]
    }

    /// `tcp_accept(&!Listener)`: `Accepted` is `Ok` 0, `Again` 1,
    /// `Failed` 2. A connection arrives blocking and with `SIGPIPE`
    /// suppressed on both kernels (see the LLVM backend's own).
    pub(crate) fn tcp_accept(&mut self, args: &[Value]) -> Vec<Value> {
        let os = self.socket_os();
        let listener = self.handle_fd(args[0]);
        let conn = self.accept_cloexec(listener);
        let reason = self.errno();

        if self.is_darwin() {
            let fix = self.builder.create_block();
            let skip = self.builder.create_block();
            let accepted = self.builder.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, conn, 0);
            self.builder.ins().brif(accepted, fix, &[], skip, &[]);
            self.builder.switch_to_block(fix);
            self.builder.seal_block(fix);
            self.set_int_option(conn, os.sol_socket, os.so_nosigpipe, 1);
            let none = self.builder.ins().iconst(types::I32, 0);
            let flags = self.fcntl(conn, F_GETFL, none);
            let cleared = self.builder.ins().band_imm(flags, !os.o_nonblock);
            self.fcntl(conn, F_SETFL, cleared);
            self.builder.ins().jump(skip, &[]);
            self.builder.switch_to_block(skip);
            self.builder.seal_block(skip);
        }

        let conn64 = self.builder.ins().sextend(types::I64, conn);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, conn, 0);
        let would_wait = self.builder.ins().icmp_imm(IntCC::Equal, reason, os.eagain);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let bad = self.builder.ins().select(would_wait, one, two);
        let tag = self.builder.ins().select(failed, bad, zero);
        vec![tag, conn64, reason]
    }

    /// `conn_read(&!Conn, &![byte])`: `Received` is `Data` 0, `End` 1,
    /// `Again` 2, `Failed` 3. An empty buffer is `Failed(EINVAL)` and
    /// never reaches the kernel: a blocking `recv` of zero bytes waits for
    /// data before answering the zero that would read as the peer closing.
    pub(crate) fn conn_read(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let fd = self.handle_fd(args[0]);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let refuse = self.builder.create_block();
        let receive = self.builder.create_block();
        let no_room = self.builder.ins().icmp_imm(IntCC::Equal, args[2], 0);
        self.builder.ins().brif(no_room, refuse, &[], receive, &[]);

        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        let einval = self.builder.ins().iconst(types::I64, EINVAL);
        self.builder.ins().jump(merge, &[minus_one.into(), einval.into()]);

        self.builder.switch_to_block(receive);
        self.builder.seal_block(receive);
        let flags = self.builder.ins().iconst(types::I32, 0);
        let got = self.libc_call(
            "recv",
            &[types::I32, pointer, types::I64, types::I32],
            &[types::I64],
            &[fd, args[1], args[2], flags],
        );
        let errno = self.errno();
        self.builder.ins().jump(merge, &[got.into(), errno.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let moved = self.builder.block_params(merge)[0];
        let reason = self.builder.block_params(merge)[1];

        let negative = self.builder.ins().icmp_imm(IntCC::SignedLessThan, moved, 0);
        let empty = self.builder.ins().icmp_imm(IntCC::Equal, moved, 0);
        let would_wait = self.builder.ins().icmp_imm(IntCC::Equal, reason, os.eagain);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let three = self.builder.ins().iconst(types::I64, 3);
        let bad = self.builder.ins().select(would_wait, two, three);
        let not_failed = self.builder.ins().select(empty, one, zero);
        let tag = self.builder.ins().select(negative, bad, not_failed);
        vec![tag, moved, reason]
    }

    /// `udp_recv(&!Udp, &![byte])` (`docs/udp.md` §3): `Datagram` is `Got` 0, `Truncated` 1,
    /// `Again` 2, `Failed` 3, and its leaves are `[tag, got, truncated, errno]`. One datagram per
    /// call. An empty buffer is `Failed(EINVAL)` and never reaches the kernel, as `conn_read`'s is.
    /// `Got(0)` is an empty datagram. On Linux `MSG_TRUNC` makes `recv` answer the datagram's real
    /// length, so `Truncated(n)` carries it; Darwin has no such flag, so there a buffer filled
    /// exactly is reported `Truncated` and `n` is the buffer's length.
    pub(crate) fn udp_recv(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let fd = self.handle_fd(args[0]);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let refuse = self.builder.create_block();
        let receive = self.builder.create_block();
        let no_room = self.builder.ins().icmp_imm(IntCC::Equal, args[2], 0);
        self.builder.ins().brif(no_room, refuse, &[], receive, &[]);

        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        let einval = self.builder.ins().iconst(types::I64, EINVAL);
        self.builder.ins().jump(merge, &[minus_one.into(), einval.into()]);

        self.builder.switch_to_block(receive);
        self.builder.seal_block(receive);
        let flags = self.builder.ins().iconst(types::I32, os.msg_trunc);
        let got = self.libc_call(
            "recv",
            &[types::I32, pointer, types::I64, types::I32],
            &[types::I64],
            &[fd, args[1], args[2], flags],
        );
        let errno = self.errno();
        self.builder.ins().jump(merge, &[got.into(), errno.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let moved = self.builder.block_params(merge)[0];
        let reason = self.builder.block_params(merge)[1];
        self.datagram_leaves(moved, reason, args[2])
    }

    /// `Datagram`'s leaves `[tag, got, truncated, errno]` from what `recv` answered (`moved`, or a
    /// negative number with `reason`) and the buffer's length: `Got` 0, `Truncated` 1, `Again` 2,
    /// `Failed` 3.
    fn datagram_leaves(&mut self, moved: Value, reason: Value, room: Value) -> Vec<Value> {
        let os = self.socket_os();
        let negative = self.builder.ins().icmp_imm(IntCC::SignedLessThan, moved, 0);
        let cut = if self.is_darwin() {
            self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, moved, room)
        } else {
            self.builder.ins().icmp(IntCC::SignedGreaterThan, moved, room)
        };
        let would_wait = self.builder.ins().icmp_imm(IntCC::Equal, reason, os.eagain);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let three = self.builder.ins().iconst(types::I64, 3);
        let bad = self.builder.ins().select(would_wait, two, three);
        let delivered = self.builder.ins().select(cut, one, zero);
        let tag = self.builder.ins().select(negative, bad, delivered);
        vec![tag, moved, moved, reason]
    }

    /// The address of the peer-ring entry a ticket names, and the address of the ticket counter.
    fn peer_entry(&mut self, ticket: Value) -> (Value, Value) {
        let ring = self.global(cancho_ir::UDP_PEER_GLOBAL);
        let slot = self.builder.ins().band_imm(ticket, cancho_ir::UDP_PEER_SLOTS - 1);
        let offset = self.builder.ins().imul_imm(slot, cancho_ir::UDP_PEER_STRIDE);
        let entry = self.builder.ins().iadd(ring, offset);
        let counter = self
            .builder
            .ins()
            .iadd_imm(ring, cancho_ir::UDP_PEER_SLOTS * cancho_ir::UDP_PEER_STRIDE);
        (entry, counter)
    }

    /// `udp_recv_from(&!Udp, &![byte], &![int])` (`docs/udp.md` §4): `udp_recv` that also writes the
    /// sender into the next entry of the peer ring and a ticket for it into the first cell of the
    /// `int` slice. An empty buffer or an empty ticket slice is `Failed(EINVAL)` before the kernel.
    pub(crate) fn udp_recv_from(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let fd = self.handle_fd(args[0]);
        let (buf, room, cells_at, cells) = (args[1], args[2], args[3], args[4]);

        // The ticket this datagram will be issued under, and the entry it will land in.
        let probe = self.global(cancho_ir::UDP_PEER_GLOBAL);
        let counter_at = self
            .builder
            .ins()
            .iadd_imm(probe, cancho_ir::UDP_PEER_SLOTS * cancho_ir::UDP_PEER_STRIDE);
        let issued = self.builder.ins().load(types::I64, MemFlags::trusted(), counter_at, 0);
        let ticket = self.builder.ins().iadd_imm(issued, 1);
        let (entry, counter) = self.peer_entry(ticket);

        let len_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            4,
            2,
        ));
        let len_at = self.builder.ins().stack_addr(pointer, len_slot, 0);
        let sixteen = self.builder.ins().iconst(types::I32, 16);
        self.builder.ins().store(MemFlags::trusted(), sixteen, len_at, 0);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let refuse = self.builder.create_block();
        let receive = self.builder.create_block();
        let no_room = self.builder.ins().icmp_imm(IntCC::Equal, room, 0);
        let no_cell = self.builder.ins().icmp_imm(IntCC::Equal, cells, 0);
        let refused = self.builder.ins().bor(no_room, no_cell);
        self.builder.ins().brif(refused, refuse, &[], receive, &[]);

        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        let einval = self.builder.ins().iconst(types::I64, EINVAL);
        self.builder.ins().jump(merge, &[minus_one.into(), einval.into()]);

        self.builder.switch_to_block(receive);
        self.builder.seal_block(receive);
        let flags = self.builder.ins().iconst(types::I32, os.msg_trunc);
        let got = self.libc_call(
            "recvfrom",
            &[types::I32, pointer, types::I64, types::I32, pointer, pointer],
            &[types::I64],
            &[fd, buf, room, flags, entry, len_at],
        );
        let errno = self.errno();
        self.builder.ins().jump(merge, &[got.into(), errno.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let moved = self.builder.block_params(merge)[0];
        let reason = self.builder.block_params(merge)[1];

        // Only a datagram that arrived is remembered and ticketed.
        let record = self.builder.create_block();
        let after = self.builder.create_block();
        let arrived = self.builder.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, moved, 0);
        self.builder.ins().brif(arrived, record, &[], after, &[]);

        self.builder.switch_to_block(record);
        self.builder.seal_block(record);
        self.builder.ins().store(MemFlags::trusted(), ticket, entry, cancho_ir::UDP_PEER_TICKET_AT);
        self.builder.ins().store(MemFlags::trusted(), fd, entry, cancho_ir::UDP_PEER_FD_AT);
        self.builder.ins().store(MemFlags::trusted(), ticket, counter, 0);
        self.builder.ins().store(MemFlags::trusted(), ticket, cells_at, 0);
        self.builder.ins().jump(after, &[]);

        self.builder.switch_to_block(after);
        self.builder.seal_block(after);
        self.datagram_leaves(moved, reason, room)
    }

    /// `udp_send_to(&!Udp, &[byte], ticket)` (`docs/udp.md` §4): `sendto` the sender a ticket names,
    /// if the ticket is positive, is still the one its ring entry holds, and was issued to *this*
    /// socket; otherwise `Failed(EBADF)` and nothing is sent. `Sent` is `Wrote` 0, `Again` 1,
    /// `Failed` 2.
    pub(crate) fn udp_send_to(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let fd = self.handle_fd(args[0]);
        let (buf, length, ticket) = (args[1], args[2], args[3]);
        let (entry, _) = self.peer_entry(ticket);

        let stored = self.builder.ins().load(
            types::I64,
            MemFlags::trusted(),
            entry,
            cancho_ir::UDP_PEER_TICKET_AT,
        );
        let owner = self.builder.ins().load(
            types::I32,
            MemFlags::trusted(),
            entry,
            cancho_ir::UDP_PEER_FD_AT,
        );
        let positive = self.builder.ins().icmp_imm(IntCC::SignedGreaterThan, ticket, 0);
        let current = self.builder.ins().icmp(IntCC::Equal, stored, ticket);
        let mine = self.builder.ins().icmp(IntCC::Equal, owner, fd);
        let valid = self.builder.ins().band(positive, current);
        let valid = self.builder.ins().band(valid, mine);

        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        self.builder.append_block_param(merge, types::I64);
        let refuse = self.builder.create_block();
        let send = self.builder.create_block();
        self.builder.ins().brif(valid, send, &[], refuse, &[]);

        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        let ebadf = self.builder.ins().iconst(types::I64, EBADF);
        self.builder.ins().jump(merge, &[minus_one.into(), ebadf.into()]);

        self.builder.switch_to_block(send);
        self.builder.seal_block(send);
        let flags = self.builder.ins().iconst(types::I32, os.msg_nosignal);
        let sixteen = self.builder.ins().iconst(types::I32, 16);
        let moved = self.libc_call(
            "sendto",
            &[types::I32, pointer, types::I64, types::I32, pointer, types::I32],
            &[types::I64],
            &[fd, buf, length, flags, entry, sixteen],
        );
        let errno = self.errno();
        self.builder.ins().jump(merge, &[moved.into(), errno.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let moved = self.builder.block_params(merge)[0];
        let reason = self.builder.block_params(merge)[1];
        let negative = self.builder.ins().icmp_imm(IntCC::SignedLessThan, moved, 0);
        let would_wait = self.builder.ins().icmp_imm(IntCC::Equal, reason, os.eagain);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let bad = self.builder.ins().select(would_wait, one, two);
        let tag = self.builder.ins().select(negative, bad, zero);
        vec![tag, moved, reason]
    }

    /// `udp_local_port(&Udp)`: the port the kernel chose, from `getsockname`, or `-errno`.
    pub(crate) fn udp_local_port(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let fd = self.handle_fd(args[0]);
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            16,
            0,
        ));
        let addr = self.builder.ins().stack_addr(pointer, slot, 0);
        let len_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            4,
            2,
        ));
        let len_at = self.builder.ins().stack_addr(pointer, len_slot, 0);
        let sixteen = self.builder.ins().iconst(types::I32, 16);
        self.builder.ins().store(MemFlags::trusted(), sixteen, len_at, 0);
        let result = self.libc_call(
            "getsockname",
            &[types::I32, pointer, pointer],
            &[types::I32],
            &[fd, addr, len_at],
        );
        let reason = self.errno();
        // Big-endian at bytes 2 and 3 of a `sockaddr_in`, on both kernels.
        let high = self.builder.ins().load(types::I8, MemFlags::trusted(), addr, 2);
        let low = self.builder.ins().load(types::I8, MemFlags::trusted(), addr, 3);
        let high = self.builder.ins().uextend(types::I64, high);
        let low = self.builder.ins().uextend(types::I64, low);
        let shifted = self.builder.ins().ishl_imm(high, 8);
        let port = self.builder.ins().bor(shifted, low);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let negated = self.builder.ins().ineg(reason);
        vec![self.builder.ins().select(failed, negated, port)]
    }

    /// `conn_write(&!Conn, &[byte])`: `Sent` is `Wrote` 0, `Again` 1,
    /// `Failed` 2. Cannot raise `SIGPIPE`.
    pub(crate) fn conn_write(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let fd = self.handle_fd(args[0]);
        let flags = self.builder.ins().iconst(types::I32, os.msg_nosignal);
        let moved = self.libc_call(
            "send",
            &[types::I32, pointer, types::I64, types::I32],
            &[types::I64],
            &[fd, args[1], args[2], flags],
        );
        let reason = self.errno();
        let negative = self.builder.ins().icmp_imm(IntCC::SignedLessThan, moved, 0);
        let would_wait = self.builder.ins().icmp_imm(IntCC::Equal, reason, os.eagain);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let one = self.builder.ins().iconst(types::I64, 1);
        let two = self.builder.ins().iconst(types::I64, 2);
        let bad = self.builder.ins().select(would_wait, one, two);
        let tag = self.builder.ins().select(negative, bad, zero);
        vec![tag, moved, reason]
    }

    /// `conn_nonblocking` / `listener_nonblocking`: `0`, or the `errno`.
    pub(crate) fn nonblocking(&mut self, args: &[Value]) -> Vec<Value> {
        let os = self.socket_os();
        let fd = self.handle_fd(args[0]);
        let none = self.builder.ins().iconst(types::I32, 0);
        let flags = self.fcntl(fd, F_GETFL, none);
        let set = self.builder.ins().bor_imm(flags, os.o_nonblock);
        let result = self.fcntl(fd, F_SETFL, set);
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let zero = self.builder.ins().iconst(types::I64, 0);
        vec![self.builder.ins().select(failed, reason, zero)]
    }

    /// `conn_connect_status` (`docs/native-sockets.md` §10.6): `SO_ERROR` of the
    /// connection, which is `0` once a non-blocking `connect` has succeeded and the
    /// `errno` it failed with otherwise. `0`, or the `errno`, also if `getsockopt` itself fails.
    pub(crate) fn connect_status(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let os = self.socket_os();
        let fd = self.handle_fd(args[0]);
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            8,
            2,
        ));
        let value = self.builder.ins().stack_addr(pointer, slot, 0);
        // The value starts as 0 and the length as 4 (a C `int`); both are 32-bit cells.
        let zero = self.builder.ins().iconst(types::I32, 0);
        self.builder.ins().store(MemFlags::trusted(), zero, value, 0);
        let four = self.builder.ins().iconst(types::I32, 4);
        self.builder.ins().store(MemFlags::trusted(), four, value, 4);
        let len_addr = self.builder.ins().iadd_imm(value, 4);
        let level = self.builder.ins().iconst(types::I32, os.sol_socket);
        let name = self.builder.ins().iconst(types::I32, os.so_error);
        let result = self.libc_call(
            "getsockopt",
            &[types::I32, types::I32, types::I32, pointer, pointer],
            &[types::I32],
            &[fd, level, name, value, len_addr],
        );
        let reason = self.errno();
        let pending = self.builder.ins().load(types::I32, MemFlags::trusted(), value, 0);
        let pending = self.builder.ins().uextend(types::I64, pending);
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        vec![self.builder.ins().select(failed, reason, pending)]
    }

    /// `conn_nodelay`: `TCP_NODELAY` on (`IPPROTO_TCP` is 6 and `TCP_NODELAY` is 1
    /// on Linux and on Darwin). `0`, or the `errno`.
    pub(crate) fn nodelay(&mut self, args: &[Value]) -> Vec<Value> {
        let pointer = self.pointer;
        let fd = self.handle_fd(args[0]);
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            4,
            2,
        ));
        let cell = self.builder.ins().stack_addr(pointer, slot, 0);
        let one = self.builder.ins().iconst(types::I32, 1);
        self.builder.ins().store(MemFlags::trusted(), one, cell, 0);
        let level = self.builder.ins().iconst(types::I32, 6);
        let name = self.builder.ins().iconst(types::I32, 1);
        let len = self.builder.ins().iconst(types::I32, 4);
        let result = self.libc_call(
            "setsockopt",
            &[types::I32, types::I32, types::I32, pointer, types::I32],
            &[types::I32],
            &[fd, level, name, cell, len],
        );
        let reason = self.errno();
        let failed = self.builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
        let zero = self.builder.ins().iconst(types::I64, 0);
        vec![self.builder.ins().select(failed, reason, zero)]
    }

    /// `clock_ms(&Clock)` (`docs/native-sockets.md` §5): `CLOCK_MONOTONIC`
    /// as milliseconds. The clock id is 1 on Linux and 6 on Darwin; both
    /// answer a `timespec` of two 64-bit fields. With `wall`, the same
    /// reading of `CLOCK_REALTIME` (id 0 on both), which is
    /// `clock_unix_ms` (§10.5).
    pub(crate) fn clock_ms(&mut self, wall: bool) -> Vec<Value> {
        let pointer = self.pointer;
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            16,
            3,
        ));
        let ts = self.builder.ins().stack_addr(pointer, slot, 0);
        let monotonic = if wall {
            0
        } else if self.is_darwin() {
            6
        } else {
            1
        };
        let id = self.builder.ins().iconst(types::I32, monotonic);
        self.libc_call("clock_gettime", &[types::I32, pointer], &[types::I32], &[id, ts]);
        let seconds = self.builder.ins().load(types::I64, MemFlags::trusted(), ts, 0);
        let nanos = self.builder.ins().load(types::I64, MemFlags::trusted(), ts, 8);
        let millis = self.builder.ins().imul_imm(seconds, 1000);
        let rest = self.builder.ins().udiv_imm(nanos, 1_000_000);
        vec![self.builder.ins().iadd(millis, rest)]
    }

    /// The address of a descriptor's epoch counter.
    fn epoch_slot(&mut self, fd: Value) -> Value {
        let table = self.global(cancho_ir::FD_EPOCH_GLOBAL);
        let offset = self.builder.ins().imul_imm(fd, 4);
        self.builder.ins().iadd(table, offset)
    }

    /// `conn_detach(Conn)` (`docs/native-sockets.md` §10.3): the descriptor
    /// stays open, the `Conn` ends, and what comes back is a ticket -- the
    /// descriptor's epoch, bumped to an odd number, over its number. A
    /// descriptor too large for the table is closed and answers `-1`.
    pub(crate) fn conn_detach(&mut self, args: &[Value]) -> Vec<Value> {
        let fd = args[0];
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        let refuse = self.builder.create_block();
        let issue = self.builder.create_block();
        let too_big = self.builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            fd,
            cancho_ir::FD_EPOCH_SLOTS,
        );
        self.builder.ins().brif(too_big, refuse, &[], issue, &[]);

        self.builder.switch_to_block(refuse);
        self.builder.seal_block(refuse);
        let fd32 = self.builder.ins().ireduce(types::I32, fd);
        self.libc_call("close", &[types::I32], &[types::I32], &[fd32]);
        let minus_one = self.builder.ins().iconst(types::I64, -1);
        self.builder.ins().jump(merge, &[minus_one.into()]);

        self.builder.switch_to_block(issue);
        self.builder.seal_block(issue);
        let slot = self.epoch_slot(fd);
        let epoch = self.builder.ins().load(types::I32, MemFlags::trusted(), slot, 0);
        let next = self.builder.ins().iadd_imm(epoch, 1);
        self.builder.ins().store(MemFlags::trusted(), next, slot, 0);
        let next64 = self.builder.ins().uextend(types::I64, next);
        let masked = self.builder.ins().band_imm(next64, 0x7fff_ffff);
        let high = self.builder.ins().ishl_imm(masked, 32);
        let ticket = self.builder.ins().bor(high, fd);
        self.builder.ins().jump(merge, &[ticket.into()]);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        vec![self.builder.block_params(merge)[0]]
    }

    /// `conn_attach(int)`: valid only if the descriptor is in range, the
    /// ticket's epoch is odd, and it is the descriptor's *current* epoch --
    /// then the epoch moves on, so the ticket is spent. `Attached` is `Ok`
    /// 0 with the descriptor, `Failed` 1 with `EBADF`.
    pub(crate) fn conn_attach(&mut self, args: &[Value]) -> Vec<Value> {
        let ticket = args[0];
        let fd = self.builder.ins().band_imm(ticket, 0xffff_ffff);
        let epoch = self.builder.ins().ushr_imm(ticket, 32);
        let in_range =
            self.builder.ins().icmp_imm(IntCC::UnsignedLessThan, fd, cancho_ir::FD_EPOCH_SLOTS);
        let zero = self.builder.ins().iconst(types::I64, 0);
        // Index zero when out of range, so the load below is always inside
        // the table; the answer is discarded then.
        let index = self.builder.ins().select(in_range, fd, zero);
        let slot = self.epoch_slot(index);
        let current = self.builder.ins().load(types::I32, MemFlags::trusted(), slot, 0);
        let current64 = self.builder.ins().uextend(types::I64, current);
        let current_masked = self.builder.ins().band_imm(current64, 0x7fff_ffff);
        let same = self.builder.ins().icmp(IntCC::Equal, current_masked, epoch);
        let odd_bit = self.builder.ins().band_imm(epoch, 1);
        let odd = self.builder.ins().icmp_imm(IntCC::NotEqual, odd_bit, 0);
        let non_negative = self.builder.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, ticket, 0);
        let a = self.builder.ins().band(in_range, same);
        let b = self.builder.ins().band(odd, non_negative);
        let valid = self.builder.ins().band(a, b);

        let spend = self.builder.create_block();
        let after = self.builder.create_block();
        self.builder.ins().brif(valid, spend, &[], after, &[]);
        self.builder.switch_to_block(spend);
        self.builder.seal_block(spend);
        let next = self.builder.ins().iadd_imm(current, 1);
        self.builder.ins().store(MemFlags::trusted(), next, slot, 0);
        self.builder.ins().jump(after, &[]);
        self.builder.switch_to_block(after);
        self.builder.seal_block(after);

        let one = self.builder.ins().iconst(types::I64, 1);
        let tag = self.builder.ins().select(valid, zero, one);
        let ebadf = self.builder.ins().iconst(types::I64, 9);
        vec![tag, fd, ebadf]
    }
}
