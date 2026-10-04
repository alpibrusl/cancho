# Linking beyond libc

> **Status: built.** §3's proposal is implemented as written: `-l`/`-L`
> on `build` and `run`, refused everywhere else, passed to `cc` between
> the object and `-o` in the order §3.1 measured. Verified against a
> real, running TLS server, not a stub: `examples/tls_client/` (§4)
> links `-lssl -lcrypto`, performs a genuine handshake against
> `openssl s_server` over loopback, and exchanges an encrypted message
> — on both backends. Two things were found building it, neither about
> linking itself:
>
> 1. **`c_ptr` (and `null_ptr()`) cannot name an ordinary function's own
>    parameter or return type** — only an `extern fn`'s, exactly as
>    `opaque-pointers.md` §4 specifies but a reader could miss: the
>    handshake, the write and the read had to become one function
>    rather than three, with every `SSL *` staying a local binding
>    inferred from an extern call's return. Recorded here because this
>    document's own example is what ran into it, not because
>    `opaque-pointers.md` was wrong.
> 2. **`examples/tls_client/`'s own raw `socket`/`connect` collided with
>    edition 2's `connect` builtin** — a foreign declaration's Lex name
>    is the linked symbol (confirmed by trying to rename it: the linker
>    then looks for a symbol that does not exist), so it cannot be
>    renamed around the collision, and this document's `edition 3;` was
>    needed for `null_ptr()`. Fixed by `docs/many-files.md`, not by this
>    document: the raw socket moved to its own file, `socket.ls`, kept
>    on edition 1 (where `connect` still resolves to the `extern fn`,
>    not the builtin) while `tls_client.ls` stays on edition 3 — the
>    per-file edition `docs/editions.md` §7 designed for exactly this,
>    a file that predates a name becoming a builtin not having to be
>    rewritten around it. `docs/net.md` §5 recounts outbound to 5.
>
> `opaque-pointers.md` §5's own fallback — "record the live example as
> a spike verified once by hand… rather than as a CI-gated test" — is
> the one taken here too, but for a different reason than the one it
> named: the blocker was never CI's ability to manage a subprocess
> server (that part works and is straightforward). It is that
> `-lssl -lcrypto` cannot be assumed to *link* on this project's own
> darwin-aarch64 CI runner without also pointing the linker at wherever
> that runner's OpenSSL happens to live — Apple ships no OpenSSL headers
> or libraries in the default search path on purpose, unlike every
> library this project has linked so far. Gating `cargo test --workspace`
> on that would make this document's own feature the reason the *other*
> target turns red, which is a worse failure mode than the one being
> fixed. So: the example is real, checked in, and built and run by hand
> against a real server (§4's exact transcript); it is deliberately not
> part of the conformance suite that CI runs on every push. A CI-gated
> version is future work, gated on knowing where CI's own OpenSSL is
> without assuming it.

## 1. What is actually missing

`crates/lex-sys/src/main.rs`'s `link` function is the whole of this
project's linking story:

```rust
fn link(object: &Path, output: &Path) -> Result<(), Failure> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let status = Command::new(&cc).arg(object).arg("-o").arg(output).status()...
```

One object file, one output path, nothing else. That has been enough
because every program this project has ever built links only against
libc, which `cc` always links whether asked to or not. `c_ptr`
(`opaque-pointers.md` §3) removed the type-system reason a program
could not *declare* `SSL_CTX_new`/`SSL_new`/etc — but declaring a
foreign function and linking the library that defines it are two
different problems, and only the first one has ever been solved here.
`opaque-pointers.md` §5's own fallback exists only because of this: its
TLS example was grounded against the real OpenSSL 3.0 header and never
compiled, because compiling it needs `-lssl -lcrypto` and there is no
way to hand the compiler those two words.

This is not a TLS-shaped gap. `reach.md` §2's table lists a Postgres
client as a second, unrelated **No**, for the same reason — `PGconn *`
is the identical handle shape `c_ptr` already covers, and what stops a
libpq client compiling is `-lpq`, not the type system. Two real,
already-named programs (`CONTRIBUTING.md` rule 3's bar) are blocked on
one missing CLI feature, not two.

## 2. The shape of the fix: pass through, invent nothing

The library this project already delegates to — `cc` — has had this
solved since long before lex-sys existed: `-l<name>` links `lib<name>`,
`-L<path>` adds a directory to search for it. `M0` already made the
decision this document does not need to re-argue (`link`'s own doc
comment): *"`M0` shells out to `cc` rather than driving a linker
itself."* Given that decision, the only question worth asking is
whether `lex-sys` needs its own syntax for "link this library," and the
answer is no — nothing about *which* library a foreign declaration
needs is knowable from the declaration itself (`reach.md` §3.1.1: "the
declaration is trusted — nothing checks a lex-sys signature against the
C header"), so there is nothing here for the compiler to infer and
nothing to validate. The flag is the whole feature: take `-l`/`-L`
exactly as `cc` defines them, and hand them to `cc` unexamined.

This also answers the question `docs/standard-library.md` §2 raises
implicitly by not raising it: is this a package manifest question? No.
A manifest resolves *which* library and *which version* satisfies a
declared dependency; this document is one step earlier — there is no
dependency declaration anywhere in a `.ls` file to resolve, only an
`extern fn` block whose author already knows which system library it
needs, the same way they already know its exact C signature. A future
package system, if one is ever built, would generate these same flags
from something it manages; it would not replace them.

## 3. The proposal

**`-l <name>`** (repeatable) and **`-L <path>`** (repeatable), accepted
by `build` and `run` — the two commands that reach `link` — and passed
to `cc` verbatim, once per occurrence, in the order given:

```sh
lex-sys build tls_client.ls --std -l ssl -l crypto -o tls_client
lex-sys run   tls_client.ls --std -l ssl -l crypto
```

Not `--link`, and not one flag taking a comma-separated list: `cc`
itself spells these `-l`/`-L`, one library or path per flag, and this
feature has nothing to add to that spelling — the same judgment
`opaque-pointers.md` §3 made choosing `c_ptr` over inventing a new
convention where an existing one already fit. `check`, `ids`,
`authority`, `layout` and `print` never link, so `-l`/`-L` are refused
there the same way `-o`/`--emit` already are on every command but
`build` (`allow_output` in `parse_args`) — an unknown option today,
staying that way rather than becoming a silently ignored one.

### 3.1 Order matters, and it is measured rather than assumed

A library named with `-l` must come **after** the object file that
needs its symbols, not before:

```
$ cc t.o -lssl -lcrypto -o t     # links
$ cc -lssl -lcrypto t.o -o t     # ld: undefined reference to `SSL_CTX_new`
```

Confirmed against this environment's real linker (`/usr/bin/ld` via
GNU `cc` 13.3.0): a static or lazily-resolved archive is scanned once,
left to right, for symbols the input already named as undefined, so a
`-l` naming a library before anything has asked for its symbols finds
nothing left to resolve. `link` already writes the object first and
the output flag last; `-l`/`-L` therefore go **after** the object and
**before** `-o`, matching the order that already works, not the order
that happens to be convenient to build.

### 3.2 Where this plugs into the existing structure

- `Invocation` (`crates/lex-sys/src/main.rs`) gains `link_libs:
  Vec<String>` and `link_paths: Vec<String>`.
- `parse_args` gains a second gate alongside `allow_output` --
  `allow_link` -- true for `build` and `run`, false everywhere else,
  the same reason `run` already gets `--backend` but not `-o` (it
  links into a temp directory it names itself, but it still links).
  `-l`/`-L` collect into the two new vectors under that gate.
- `build()` and `link()` gain the two vectors as parameters. `link`
  appends `-L<path>` for every `link_paths` entry, then `-l<name>` for
  every `link_libs` entry, between the object argument and `-o`.
- `USAGE` gains the two flags on `build`'s and `run`'s own lines, and a
  paragraph, the same as every other flag documented there.

### What this does not solve

- **No dependency resolution, no manifest, no version pinning.** The
  two vectors are handed to `cc` exactly as given; nothing here decides
  *which* `-lssl` a system has, the way `docs/standard-library.md` §2.1
  already named as the first thing to revisit "when a package story
  exists." This is not that story.
- **No static-vs-dynamic choice beyond what `cc`/`ld` already default
  to.** `-static`, `-Wl,-rpath`, and every other linker flag stay
  unreachable — a program that needs one is a program with a second,
  separate ask, counted the same way this one was.
- **Does not change what can be declared.** `c_ptr` and the ordinary
  `int`/`bool`/`&r [byte]` shapes `reach.md` §2 already allows are the
  entire foreign surface; this document only makes the library that
  defines them reachable at link time.

## 4. Verified

`opaque-pointers.md` §5 named the exact test this unblocks, and this is
it, done: `examples/tls_client/` declares the eleven OpenSSL functions
§1 of that document grounded against the real header (`TLS_client_method`,
`SSL_CTX_new`, `SSL_new`, `SSL_set_fd`, `SSL_connect`, `SSL_write`,
`SSL_read`, `SSL_shutdown`, `SSL_free`, `SSL_CTX_free`, and
`SSL_get_error` named but not needed — every failure path here has
nothing more specific to say without it), built with `-l ssl -l crypto`
against this environment's genuine OpenSSL 3.0.13 (`openssl version`;
`libssl.so`/`libcrypto.so` present as unversioned linker symlinks, the
`-dev` package, not only the runtime `.so.3`), and run against a real
`openssl s_server -rev` (an echo server that reverses one line) over
loopback, on both backends:

```sh
$ lex-sys build examples/tls_client/tls_client.ls examples/tls_client/socket.ls \
      packages/net-connect/connect.ls packages/net-sockets/sockets.ls \
      --std -l ssl -l crypto -o tls_client
$ openssl s_server -accept 48395 -cert cert.pem -key key.pem -rev -naccept 1 -quiet &
$ ./tls_client 127.0.0.1 48395 "hello lex-sys"
sys-xel olleh
```

The two extra paths are `docs/next-phase.md` §4.1's own migration:
`socket.ls` used to declare `socket`/`connect`/`close`/`address` for
itself; it now `import`s `net.connect`/`net.sockets` and needs both on
the command line the same way any package-importing example does
(`vcs fetch` for a pinned copy, or — as here, since both are already
checked into this repository — the source files directly).

A genuine TLS 1.3 handshake (confirmed in the server's own diagnostic
output: `Protocol version: TLSv1.3`, `Ciphersuite: TLS_AES_256_GCM_SHA384`)
and a real encrypted round trip, not a stub — the reply is `"hello
lex-sys"` reversed, exactly what `-rev` does to what it received, so
the bytes crossed the connection both ways intact. Repeated with
`--backend cranelift` against a fresh server: `oot tfilenarc` for
`"cranelift too"`, same result. Neither is CI-gated (the status note
above says why); both are hand-verified transcripts, recorded here
rather than only run once and discarded.

> **Corrected (`tls-nonblocking.md` §2): the example's handshake failures were read as successes.** `examples/tls_client/tls_client.ls`
> declared `SSL_connect`, `SSL_write`, `SSL_read`, `SSL_set_fd` and `SSL_shutdown` as returning `int`, and a C `int` result is only
> sign-extended if the declaration says `c_int`: a failing `SSL_connect` (-1) was read as 4294967295, `<= 0` was false, and the program
> wrote to a session that never existed (`examples/tls_nb/gaps/g13_int_vs_c_int.ls`). The five are `c_int` now. Separately, run against
> a server that accepts and closes, the example is still killed by `SIGPIPE`: OpenSSL's socket BIO answers the end of file with a fatal
> alert written by `write(2)`. It is a one-connection demonstration and is left as it is; a service must use the memory-BIO transport of
> `examples/tls_nb/` or ignore the signal.
