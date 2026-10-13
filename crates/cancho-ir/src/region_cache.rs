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
