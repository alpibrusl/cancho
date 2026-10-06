# Summary

[Introduction](README.md)

# Foundations

- [Linear types and effects](linearity-and-effects.md)
- [What M0 settled](bootstrap.md)
- [Canonical AST and per-unit identity](canonical-ast.md)
- [Defined behaviour](defined-behaviour.md)

# Ownership, borrowing and memory

- [Reading through a reference](reading-references.md)
- [The heap and Box\[T\]](heap.md)
- [Boxed slices](boxed-slices.md)
- [Sharing, and why Rc is not expressible](sharing.md)
- [Aliasing: does `&!` mean `&mut`?](aliasing.md)
- [Collections](collections.md)
- [Testing](testing.md)
- [Formatting](formatting.md)
- [JSON](json.md)
- [Hash map](map.md)
- [HTTP and routing](http.md)
- [The API server](server.md)
- [The server loop as a package](http-server.md)
- [Parallelism: threads and vectorization](parallelism.md)
- [Native sockets](native-sockets.md)
- [Signals](signals.md)
- [Directory handles](directory-handles.md)
- [Directory listing](directory-listing.md)
- [Processes](processes.md)
- [Foreign authority](foreign-authority.md)
- [A WebSocket spike](websocket-spike.md)
- [Slicing](slicing.md)
- [Tuples](tuples.md)
- [Shadowing](shadowing.md)
- [Mode polymorphism](mode-polymorphism.md)
- [Function values](function-values.md)
- [Threads: spawn and join](threads.md)
- [Threads that carry more than one value](thread-payloads.md)
- [Atomics and a channel](atomics.md)
- [Effect polymorphism](effect-polymorphism.md)

# Capabilities, effects and authority

- [Command-line arguments](arguments.md)
- [The filesystem capability](filesystem.md)
- [Standard input](standard-input.md)
- [Standard error](standard-error.md)
- [The authority report](authority.md)
- [Whether \[budget\] is a type-system feature](budget.md)
- [What a program can reach](reach.md)
- [Opaque pointers and c_ptr](opaque-pointers.md)
- [Linking beyond libc](foreign-linking.md)
- [Non-blocking TLS](tls-nonblocking.md)
- [Under a lex-os grant](under-a-grant.md)

# Types and data

- [Floating point](floating-point.md)
- [`f32`, and the program that asked for it](f32.md)
- [Character literals](character-literals.md)
- [Bitwise operators](bitwise.md)
- [Strings](strings.md)
- [UTF-8 decoding](utf8.md)

# Modules and programs

- [A program in more than one file](many-files.md)
- [Modules](modules.md)
- [defer](defer.md)

# Networking

- [Net: sockets, taken out of libc](net.md)
- [connect, and an HTTP client](connect.md)
- [listen, and the inbound counterpart](listen.md)

# Standard library

- [What belongs in std](standard-library.md)
- [Shortest round-trip float printing](float-printing.md)
- [sqrt as a builtin](float-math.md)
- [Command-line flags](flags.md)
- [Whether std needs a line reader](line-reading.md)
- [Bulk I/O](bulk-io.md)
- [File handles](file-handles.md)
- [Files larger than memory](large-files.md)
- [File writes](file-writes.md)
- [std.crypto: SHA-256](crypto.md)
- [std.crypto: SHA-512](sha512.md)
- [std.ed25519](ed25519.md)
- [std.chacha20: ChaCha20-Poly1305](chacha20.md)
- [SHA-384, HMAC and HKDF](hkdf.md)
- [A TLS 1.3 client with no C library](tls-pure.md)
- [X25519, and the field it shares with Ed25519](x25519.md)
- [`packages/x509`: strict DER and X.509 certificates](x509.md)
- [`std.bigmod` and `std.rsa`: RSA signature verification](rsa.md)
- [`std.ecdsa`: ECDSA verification on P-256 and P-384](ecdsa.md)
- [`packages/tls`: the TLS 1.3 handshake and record layer](tls-core.md)
- [`packages/x509`: verifying a server's chain](x509-verify.md)
- [TLS parity with the OpenSSL backend](tls-parity.md)
- [`std.ecdh`: P-256 and P-384 key exchange](ecdh.md)
- [`value_barrier`: a value the optimiser cannot see through](value-barrier.md)
- [The pure TLS backend in `lexsys-hooks`](tls-hooks.md)
- [TLS 1.3 session resumption](tls-resumption.md)

# Compile time and program identity

- [Compile-time evaluation](compile-time.md)
- [Compile-time data](compile-time-data.md)
- [How often a hash actually moves](hash-stability.md)
- [Editions](editions.md)

# Version control: the op log

- [What lex-sys-vcs would need](vcs.md)
- [The first publish](vcs-publish.md)
- [A package system](package-system.md)

# Backend and performance

- [What the overflow trap costs](overflow-cost.md)
- [lex-sys against C and Rust](against-c-and-rust.md)
- [The checked purity proof](purity.md)
- [Layout](layout.md)
- [The Computer Language Benchmarks Game](benchmarks-game.md)
- [The price of every check](check-cost.md)
- [Poison instead of a trap](poison.md)
- [What Cranelift can and cannot do](backend-limits.md)
- [The LLVM backend](llvm-backend.md)
- [WebAssembly target](wasm.md)
- [Every emitted check, priced](emitted-checks.md)
- [Whether lex-sys can run on a GPU](gpu.md)
- [The constant folder against the backend](differential.md)
- [When the compiler is wrong](internal-errors.md)

# Real programs, ported

- [base64 and sort](porting.md)

# Agent and tooling ergonomics

- [Refusals a machine can read](agent-errors.md)
- [A tool shaped for an agent](agent-tools.md)
- [An agent toolbox](agent-toolbox.md)
- [Fuzzing the compiler](fuzzing.md)
- [The first page](first-page.md)

# Process

- [Related work](related-work.md)
- [Self-hosting, measured](self-hosting.md)

---

[Roadmap: what shipped, and what each slice found](ROADMAP.md)
