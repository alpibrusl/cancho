# `c_ptr`: an opaque handle that crosses without being trusted

> **Status: built (edition 3).** §3's proposal is implemented as
> written: `Type::CPtr`, valid in both `extern fn` parameter and return
> position, `val`, equality/nullness only, with `null_ptr()` as the one
> non-foreign producer. `tests/accept/opaque_pointer.cho` exercises the
> full round trip — a foreign call returning a handle, the same handle
> passed to a second foreign call, and a null comparison on a real
> failure — through real libc (`fdopen`/`fclose`) on both backends
> (`crates/cancho/tests/conformance/backends.rs`'s
> `the_two_backends_agree_on_an_opaque_pointer`); three `tests/reject/`
> fixtures pin arithmetic, coercion, and the "not a general type" rule
> §4 predicted. §5's TLS example was not built at the time: it needed
> the build pipeline to link a library beyond libc, which nothing in
> this project had ever needed before. **Corrected:
> [`docs/foreign-linking.md`](foreign-linking.md)** closed that gap and
> built the example — `examples/tls_client/`, a real handshake and an
> encrypted round trip against `openssl s_server` on both backends,
> that document's §4 has the transcript. §1's real OpenSSL signatures
> and §3's design are otherwise exactly what shipped — reached by writing a
> first accept fixture against `fopen`/`fopen`'s two string parameters,
> watching it segfault, and finding the actual bug: a pre-existing,
> `c_ptr`-unrelated gap in how a `&r [byte]` parameter crosses (§4 note
> below), not anything wrong with the type this document proposed.
>
> `docs/reach.md` §3.1 named the rule this document has to satisfy — "a
> value crosses into a cancho program only if the checker can say
> where it came from" — and then listed everything that rule puts out
> of reach: OpenSSL, libpq, libcurl, `FILE *`, `dlopen`. This is the
> design for letting the first of those in, TLS specifically, without
> weakening that rule. It was written before any code, per this
> project's own "design before code" discipline (`CLAUDE.md`), and
> grounded against the real OpenSSL 3.0 header and a live loopback
> handshake, not a remembered signature.

---

## 1. What TLS actually asks for

`docs/reach.md` §2's table already answers "can it reach TLS?" with
**No**, and names the reason: `SSL_CTX *` (§3.1). The real OpenSSL
client handshake this document verified against (`/usr/include/openssl/
ssl.h`, OpenSSL 3.0.13, confirmed present in this environment via
`openssl version`, `pkg-config --exists openssl`, and every symbol below
resolved live in `libssl.so.3` with `nm -D`) is nine calls:

```c
const SSL_METHOD *TLS_client_method(void);
SSL_CTX          *SSL_CTX_new(const SSL_METHOD *meth);
SSL              *SSL_new(SSL_CTX *ctx);
int               SSL_set_fd(SSL *s, int fd);
int               SSL_connect(SSL *ssl);
int               SSL_write(SSL *ssl, const void *buf, int num);
int               SSL_read(SSL *ssl, void *buf, int num);
int               SSL_get_error(const SSL *s, int ret_code);
int               SSL_shutdown(SSL *s);
void              SSL_free(SSL *ssl);
void              SSL_CTX_free(SSL_CTX *);
```

Split these by what §3.1 already handles and what it does not:

- `int fd`, `int num`, every `int` return — already `int`, already
  crosses today. `SSL_write`/`SSL_read`'s `void *buf`/`const void *buf`
  are not opaque handles at all; they are the same "a length and a
  pointer that cannot disagree" shape `write`/`read` already use
  (`strings.md` §6), so they cross as `&r [byte]` exactly like an
  existing `read`/`write` declaration, unchanged by this document.
- `const SSL_METHOD *`, `SSL_CTX *`, `SSL *` — three distinct opaque
  handle types, each produced by one call and consumed by the next few,
  never inspected, never dereferenced, only threaded through in order
  and eventually freed. **This is the whole gap.** Three call sites
  return a handle (`TLS_client_method`, `SSL_CTX_new`, `SSL_new`); five
  take one as a parameter (`SSL_set_fd`, `SSL_connect`, `SSL_write`,
  `SSL_read`, `SSL_get_error`, `SSL_shutdown`, `SSL_free`,
  `SSL_CTX_free` — eight, not five; corrected count below in §3).

So the feature this document proposes is narrow on purpose: a value that
can be *returned by* and *passed to* a foreign function, compared for
nullness, and nothing else. Not a general pointer type, not a `malloc`
story (`reach.md` §3.1 is right that nothing here wants that), not a
struct-crossing story (§3.2 already has its own answer, unrelated).

## 2. Why `c_int` is the wrong template, and where it still helps

`c_int` (`reach.md` §3.4) looks like the closest precedent — a named
type that exists only to make one foreign crossing honest — but its
mechanism is the opposite of what a handle needs. `c_int` is recognized
by name at exactly one syntactic position (`extern fn`'s return type,
`crates/cancho-ir/src/lib.rs`) and **collapses immediately to plain
`Type::Int`** for everything downstream: every assignment, comparison
and arithmetic operation on a `c_int` result sees an ordinary 64-bit
`int` from the very next line on. That collapse is exactly right for
`c_int`, because the thing it is narrowing — a real C `int32_t` that
gets sign-extended once at the boundary — really is an integer
afterward, with real arithmetic and real comparisons against `0`.

A handle is not an integer afterward. `let s: &static [byte] = ssl_ctx;`
must stay an error the same way `reach.md` §3.1.1 shows it staying an
error for `malloc`'s pointer today, and `ssl_ctx + 1` must be an error
too — neither of which `c_int`'s collapse-to-`Int` trick can give,
because collapsing to `Int` is precisely what makes arithmetic and
comparison legal. So `c_ptr` needs to remain a **distinct type all the
way through the program**, not a spelling that resolves away at parse
time. What `c_ptr` does keep from `c_int`'s precedent: a foreign
**return** is exactly where extra scrutiny already lives in this
codebase (§3.4's whole point), and the same trust posture applies here
— the declaration is checked for shape, never against the real header
(`reach.md` §3.1.1: "the declaration is trusted — nothing checks a
cancho signature against the C header").

## 3. The proposal

**A new type, `Type::CPtr`, spelled `c_ptr` in source.** Unlike
`c_int`, it is a real type, not a resolve-away name:

- **Valid in both parameter and return position** of an `extern fn`
  declaration — `c_int` is return-only because only a return has the
  width problem; a handle has no width problem (it is opaque, never
  read), but it flows in both directions (`SSL_CTX_new` returns one,
  `SSL_new` takes one), so the restriction `c_int` needs does not apply
  here.
- **`val`, not linear.** The existing capability/region system exists
  to track *ownership* of something this program allocated and must
  release exactly once. A `c_ptr` is not owned by this program at all —
  it is C's memory, released by calling `SSL_free`/`SSL_CTX_free`, and
  nothing here can express "must call `SSL_free` exactly once" any more
  than it expresses "must call `close` exactly once" for a raw `int`
  file descriptor today. That is not a new gap `c_ptr` opens; it is the
  same gap `reach.md` §3.1.1 already accepts for `fork`'s pid and every
  file descriptor this language already hands back integer-shaped. A
  `c_ptr` is copyable and comparable, freely, the same as an `int` is.
- **Equality and nullness only.** `==`, `!=`, and a comparison against a
  literal null handle. No arithmetic (`+`, `-`, indexing), no
  dereference, no coercion to or from any reference type, no coercion
  to or from `int`. This is what keeps §3.1's rule intact: the checker
  never claims to know a `c_ptr`'s provenance the way it knows a
  reference's region, so it never lets a `c_ptr` be used as if it did.
  An `int` is safe to accept because "a number's provenance is nothing"
  (§3.1) — a `c_ptr` is safe to accept for exactly the same reason, as
  long as the only things done with it are the things that also don't
  need provenance: pass it back, compare it, check it against null.
- **A null sentinel**, `null_ptr()`, a builtin of type `c_ptr` — needed
  because OpenSSL's own error convention is "returns `NULL` on
  failure" for `SSL_CTX_new` and `SSL_new` alike, and the program needs
  a `c_ptr` value to compare against without ever constructing one from
  an `int` (which the no-coercion rule above forbids). `null_ptr()`
  is the one producer of a `c_ptr` that is not a foreign call's return.

Corrected count from §1: eight foreign declarations become the surface
this touches — three returning `c_ptr` (`TLS_client_method`,
`SSL_CTX_new`, `SSL_new`), and the same handle type appearing as a
parameter in `SSL_set_fd`, `SSL_connect`, `SSL_write`, `SSL_read`,
`SSL_get_error`, `SSL_shutdown`, `SSL_free`, `SSL_CTX_free` (eight
consumers, some declarations consuming more than one handle type at
once — `SSL_set_fd` takes `SSL *` and an `int`, not two handles).

### What this does not solve

- **No `struct sockaddr_in`-style layout problem exists for TLS.**
  Every OpenSSL call in §1 takes only handles, integers, or a
  length-carrying byte slice — there is no struct to build the way
  `bind` needs one (`reach.md` §3.2). This document has nothing to add
  to §3.2's answer.
- **Does not open `malloc`.** A `c_ptr` cannot be dereferenced, indexed,
  or offset, so it is useless for the thing `reach.md` §3.1 already
  refuses on purpose ("the heap is a capability with `box`"). Nothing
  about accepting a handle back from `SSL_CTX_new` lets a program treat
  it as addressable memory.
- **Does not give "must free exactly once."** Stated above; a `c_ptr`
  leak is possible, the same way an unclosed file descriptor is
  possible today. A future capability wrapping `c_ptr` the way `heap.md`
  wraps raw allocation is future work, not required for TLS to work at
  all, and not proposed here.
- **Does not solve certificate verification's own trust story** — a
  correct TLS client needs `SSL_CTX_set_verify` and a trust store, which
  are more foreign declarations of the same three shapes already
  covered (`c_ptr`, `int`, byte slice), not a new type-system problem.
  Left to the example program in §5, not this design.

## 4. Where this plugs into the existing structure

(Sequencing for the implementation that follows this document — not
itself part of the design decision.)

- `crates/cancho-types`: a new `Type::CPtr` variant alongside `Int`,
  `Byte`, `Bool`, ….
- The resolver/name layer that recognizes `c_int` by string in
  `crates/cancho-ir/src/lib.rs`'s extern-fn checking gains a sibling
  branch for `c_ptr`, constructing `Type::CPtr` instead of collapsing
  to `Type::Int`, allowed in both the parameter and return position
  checks (currently `Type::Int | Type::Bool` for parameters,
  `Type::Int | Type::Bool | Type::Unit` for returns).
  `c_ptr`, like `c_int`, is not registered as a general type name
  anywhere else, so writing it in any other position stays an
  unresolved-name refusal for free — the same reason `c_int` cannot be
  written outside an extern return today.
  the binary-operator type checker gains `Type::CPtr` to its
  equality/inequality arms and excludes it from every arithmetic and
  ordering arm.
- Both backends' scalar-classification (`scalar_kind` in
  `cancho-codegen-llvm`, and Cranelift's equivalent width lowering)
  treat `Type::CPtr` as an i64-width scalar for register purposes —
  the same width `int` already gets, since the handle rides in a
  register exactly like an integer-shaped file descriptor does.
  `null_ptr()` lowers to a 64-bit zero constant on both backends.
- Differential coverage: `crates/cancho/tests/conformance/backends.rs`
  gets a case exercising a `c_ptr`-returning and `c_ptr`-consuming
  extern declaration on both backends, mirroring
  `the_two_backends_agree_on_a_narrow_foreign_return`'s existing
  pattern for `c_int`.
- `tests/reject/`: a case proving `c_ptr` arithmetic, dereference, and
  coercion to/from `int` or a reference type are all refused — the
  positive mirror of `reach.md` §3.1.1's `malloc` example, now checked
  by the compiler instead of only by prose.

## 5. Verification plan

A design that only compiles is not verified; `reach.md`'s whole method
is building the thing and reporting what happened. The plan:

1. A new example, `examples/tls_client/`, declares the nine functions in
   §1 (plus `SSL_CTX_set_verify`/a trust-store call for a real, not
   toy, verification posture) using `c_ptr` for every handle.
2. It connects to a real TLS server over loopback: `openssl s_server
   -accept <port> -cert <cert> -key <key>` (already available — this
   environment has genuine OpenSSL 3.0.13, confirmed via
   `openssl version`), sends a request, and reads the response back
   through the existing `&r [byte]` byte-slice crossing.
3. The test harness (`crates/cancho/tests/conformance/`, following the
   existing pattern for `examples/serve/`'s
   `an_http_server_written_in_cancho_answers_a_real_request`) starts
   `openssl s_server` as a subprocess, runs the compiled example against
   it, and asserts the plaintext response bytes match — a real
   handshake and a real encrypted round trip, not a stub.
4. Both backends run the same example through
   `crates/cancho/tests/conformance/backends.rs`'s existing
   `--backend cranelift`/`--backend llvm` split, so `c_ptr` is proven on
   both, not just the default.

If step 2 or 3 cannot be made reliable in CI (a subprocess-managed real
TLS server is a heavier test fixture than anything this suite runs
today), the fallback is the same one `docs/self-hosting.md` used for a
comparably large claim: land the type-system change and its `tests/
accept`/`tests/reject` coverage on its own, and record the live example
as a spike verified once by hand in this document (with its exact
output) rather than as a CI-gated test — a decision to make explicit at
implementation time, not assumed here.

**Corrected: this is the fallback that was taken, for a reason
independent of `c_ptr`.** Step 1 needs the final linking step to pull
in `-lssl -lcrypto`; every example this project has ever built links
only libc, which the compiler always links, so nothing in the current
`cancho build` pipeline can express "also link this library" at all.
Adding that is a real, separate feature — a general "link an external
library" story, not a `c_ptr` question — so §1's nine declarations were
grounded against the real header and never compiled into a program.
`tests/accept/opaque_pointer.cho` verifies the type feature itself
end-to-end against `fdopen`/`fclose` instead, real libc, no extra
linking.

## 6. What building the fixture found, unrelated to `c_ptr`

The first attempt at that fixture declared `fopen(path: &p [byte],
mode: &m [byte]) -> c_ptr` — two string parameters, matching real
`fopen`'s own signature — and segfaulted on both backends. `gdb` on the
core: `_IO_new_file_fopen` received `mode=0xe`, a small integer where a
pointer belongs. The cause has nothing to do with `c_ptr`: a `&r
[byte]` parameter already crosses as *two* real arguments, a pointer
and a separate length (`docs/strings.md` §6), and `path`'s own length
leaf (`14`, `len("/etc/hostname") + 1`) landed in the register real
`fopen` reads as its second parameter, because `fopen` was never
compiled expecting that extra register at all. `path`'s pointer arrived
correctly (it is the *first* real argument either way); `mode`'s
pointer was silently dropped.

**This means a `&r [byte]` parameter is only safe as the *last*
crossing parameter of an `extern fn` declaration.** `write`/`read`'s
own long-standing declarations already follow this by construction —
`fd: int` first, the slice last, nothing after it — and so does every
declaration in §1: not one OpenSSL function needed two string-shaped
parameters, so this gap never touched the TLS design at all. The
fixture that found it was rewritten around `fdopen(fd: int, mode: &m
[byte])`, matching the safe shape exactly, rather than working around
the gap. **Declaring a real C function that takes two NUL-terminated
strings (`fopen`'s own `path, mode` included) has no correct spelling
in this language today** — a pre-existing limitation of the byte-slice
crossing convention, not of `c_ptr`, and out of scope for this
document to fix. Worth a line in `docs/strings.md` §6 or a `tests/
reject/` fixture of its own; left as found, not chased, the same
judgment call `docs/llvm-backend.md`'s own corrections made about
scope.
