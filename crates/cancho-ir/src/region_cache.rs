//! The per-thread stack of region chunks (`docs/region-chunk-cache.md`):
//! the names and numbers both backends build its runtime from, so that the
//! two cannot disagree on the bound.

/// How many freed chunks one thread keeps (`docs/region-chunk-cache.md`
/// §2): the deepest nesting measured was 10 regions on the TLS server and
/// 15 on its client, and a stack shallower than the depth hands the excess
/// back to `free` every time it unwinds. 16 chunks of 64 KiB are 1 MiB of
/// address space a thread at most.
pub const REGION_CACHE_SLOTS: i64 = 16;

/// The bytes of one thread's stack: the count, then the chunks.
pub const REGION_CACHE_BYTES: i64 = 8 + 8 * REGION_CACHE_SLOTS;

/// The module global holding the `pthread` key plus one (zero: no cache,
/// every region `malloc`s and `free`s as before). On WASI, which has no
/// threads, the global holds the one stack's address instead.
pub const REGION_CACHE_GLOBAL: &str = "cancho_region_key";

/// `main` calls this first: makes the key.
pub const REGION_INIT: &str = "cancho_region_init";
/// A region's chunk: the thread's last one given back, or a fresh `malloc`.
pub const REGION_TAKE: &str = "cancho_region_take";
/// The end of a region: clear what it used, then keep the chunk or `free` it.
pub const REGION_GIVE: &str = "cancho_region_give";
/// The key's destructor: a thread that ends frees its chunks and its stack.
pub const REGION_DROP: &str = "cancho_region_drop";

/// The bytes one region holds, in all (`docs/region-limit.md` §1): a region
/// is one chunk, the bump pointer starts at its base, and an allocation
/// that ends at or before the chunk's end fits. Both backends and the
/// checker read this one number, so they cannot disagree on the boundary.
pub const REGION_CHUNK_BYTES: i64 = 64 * 1024;

/// What a program writes to standard error (fd 2) just before it traps on
/// an allocation the chunk cannot hold (`docs/region-limit.md` §3). One
/// text for both backends, so a run says the same on either.
pub const REGION_EXHAUSTED_MESSAGE: &str = "cancho: trap: region chunk exhausted: a region holds 65536 bytes in all; allocate big data on the heap with box_slice (AGENTS.md)\n";
