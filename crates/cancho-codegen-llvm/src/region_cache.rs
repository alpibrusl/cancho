//! The per-thread stack of region chunks (`docs/region-chunk-cache.md` §3),
//! as IR text: `region` takes its chunk from `@cancho_region_take` and gives
//! it back to `@cancho_region_give`, which clears what the region used and
//! keeps the chunk for the thread's next region, up to
//! `cancho_ir::REGION_CACHE_SLOTS`. `cancho-codegen`'s own `region_cache.rs`
//! builds the same four functions for Cranelift.
//!
//! A thread's stack is found through a `pthread` key made by `main` before
//! any cancho code runs; the key's destructor frees a thread's chunks when it
//! ends. WASI has no threads (`docs/wasm.md`), so there the stack is one
//! module global and there is no key.

use cancho_ir::{
    REGION_CACHE_BYTES, REGION_CACHE_GLOBAL, REGION_CACHE_SLOTS, REGION_DROP, REGION_GIVE,
    REGION_INIT, REGION_TAKE,
};
use target_lexicon::{Architecture, OperatingSystem, Triple};

use crate::emit::{ARENA_CHUNK, size_ty};

/// Whether `module` (every function body, before `main`) opens a region, and
/// so needs the runtime and `main`'s call to make the key.
pub(crate) fn used(module: &str) -> bool {
    module.contains(&format!("@{REGION_TAKE}("))
}

/// Whether the target has threads, and so the key (`docs/region-chunk-cache.md` §3.3).
pub(crate) fn threaded(triple: &Triple) -> bool {
    triple.architecture != Architecture::Wasm32
}

/// The libc names the runtime calls, for the module header: each as its
/// name and its declaration, declared there only when the program does not
/// declare the name itself (#388's rule).
pub(crate) fn libc_declarations(triple: &Triple) -> Vec<(&'static str, String)> {
    if !threaded(triple) {
        return Vec::new();
    }
    let key = key_ty(triple);
    vec![
        ("pthread_key_create", "i32 @pthread_key_create(ptr, ptr)".to_owned()),
        ("pthread_getspecific", format!("ptr @pthread_getspecific({key})")),
        ("pthread_setspecific", format!("i32 @pthread_setspecific({key}, ptr)")),
    ]
}

/// `pthread_key_t`: `unsigned int` on glibc and musl, `unsigned long` on Darwin.
fn key_ty(triple: &Triple) -> &'static str {
    if matches!(triple.operating_system, OperatingSystem::Darwin(_)) { "i64" } else { "i32" }
}

/// The call `main` makes first, when [`used`]: the key, on a threaded target.
pub(crate) fn init_call(triple: &Triple) -> String {
    if threaded(triple) { format!("  call void @{REGION_INIT}()\n") } else { String::new() }
}

/// The four functions (three on WASI), when [`used`].
pub(crate) fn definitions(triple: &Triple) -> String {
    let st = size_ty(triple);
    let key = key_ty(triple);
    let slots = REGION_CACHE_SLOTS;
    let bytes = REGION_CACHE_BYTES;
    let chunk = ARENA_CHUNK;
    let global = REGION_CACHE_GLOBAL;
    // How a function finds the calling thread's stack (`%c`, null if it has
    // none) and, on a threaded target, the key (`%key`). `none` is where to go
    // when there is no key.
    let find = |none: &str| {
        if threaded(triple) {
            format!(
                "  %k1 = load i64, ptr @{global}\n  %nokey = icmp eq i64 %k1, 0\n  \
                 br i1 %nokey, label %{none}, label %lookup\nlookup:\n  %kw = sub i64 %k1, 1\n{}  \
                 %c = call ptr @pthread_getspecific({key} %key)\n",
                if key == "i64" {
                    "  %key = add i64 %kw, 0\n".to_owned()
                } else {
                    "  %key = trunc i64 %kw to i32\n".to_owned()
                }
            )
        } else {
            format!("  %c = load ptr, ptr @{global}\n")
        }
    };
    // Where a new stack is recorded as the thread's: `pthread_setspecific`,
    // or the global. `%made` is the new stack; on failure, `fail`.
    let record = |fail: &str| {
        if threaded(triple) {
            format!(
                "  %set = call i32 @pthread_setspecific({key} %key, ptr %made)\n  \
                 %setok = icmp eq i32 %set, 0\n  br i1 %setok, label %push, label %{fail}\n"
            )
        } else {
            format!("  store ptr %made, ptr @{global}\n  br label %push\n")
        }
    };
    let global_ty = if threaded(triple) { "i64 0" } else { "ptr null" };
    let mut text = format!("@{global} = internal global {global_ty}\n");
    text.push_str(&format!("declare void @llvm.memset.p0.{st}(ptr, i8, {st}, i1)\n\n"));

    // take: pop the thread's last chunk, or `malloc` one.
    text.push_str(&format!(
        "define internal ptr @{REGION_TAKE}() {{\nentry:\n{}  %nostack = icmp eq ptr %c, null\n  \
         br i1 %nostack, label %fresh, label %count\ncount:\n  %n = load i64, ptr %c\n  \
         %empty = icmp eq i64 %n, 0\n  br i1 %empty, label %fresh, label %pop\npop:\n  \
         %m = sub i64 %n, 1\n  store i64 %m, ptr %c\n  %at = getelementptr i64, ptr %c, i64 %n\n  \
         %p = load ptr, ptr %at\n  ret ptr %p\nfresh:\n  %q = call ptr @malloc({st} {chunk})\n  ret ptr %q\n}}\n\n",
        find("fresh")
    ));

    // give: clear what the region used, then push the chunk, making the
    // thread's stack on its first release, or `free` it.
    let used_len = if st == "i32" {
        "  %used64 = sub i64 %e, %b\n  %used = trunc i64 %used64 to i32\n"
    } else {
        "  %used = sub i64 %e, %b\n"
    };
    text.push_str(&format!(
        "define internal void @{REGION_GIVE}(ptr %base, ptr %end) {{\nentry:\n  \
         %b = ptrtoint ptr %base to i64\n  %e = ptrtoint ptr %end to i64\n{used_len}  \
         call void @llvm.memset.p0.{st}(ptr %base, i8 0, {st} %used, i1 false)\n{}  \
         %nostack = icmp eq ptr %c, null\n  br i1 %nostack, label %make, label %have\n\
         make:\n  %made = call ptr @malloc({st} {bytes})\n  %nomem = icmp eq ptr %made, null\n  \
         br i1 %nomem, label %release, label %fresh\nfresh:\n  store i64 0, ptr %made\n{}\
         unmade:\n  call void @free(ptr %made)\n  br label %release\n\
         have:\n  br label %push\n\
         push:\n  %s = phi ptr [ %c, %have ], [ %made, %{after} ]\n  %n = load i64, ptr %s\n  \
         %full = icmp sge i64 %n, {slots}\n  br i1 %full, label %release, label %keep\n\
         keep:\n  %n1 = add i64 %n, 1\n  %at = getelementptr i64, ptr %s, i64 %n1\n  \
         store ptr %base, ptr %at\n  store i64 %n1, ptr %s\n  ret void\n\
         release:\n  call void @free(ptr %base)\n  ret void\n}}\n\n",
        find("release"),
        record("unmade"),
        after = "fresh",
    ));

    // drop: the key's destructor, on a thread's way out.
    if threaded(triple) {
        text.push_str(&format!(
            "define internal void @{REGION_DROP}(ptr %s) {{\nentry:\n  %n = load i64, ptr %s\n  \
             br label %head\nhead:\n  %i = phi i64 [ 1, %entry ], [ %next, %body ]\n  \
             %more = icmp sle i64 %i, %n\n  br i1 %more, label %body, label %done\nbody:\n  \
             %at = getelementptr i64, ptr %s, i64 %i\n  %p = load ptr, ptr %at\n  \
             call void @free(ptr %p)\n  %next = add i64 %i, 1\n  br label %head\ndone:\n  \
             call void @free(ptr %s)\n  ret void\n}}\n\n"
        ));
        // init: the key, its destructor `drop`; the key plus one, so that
        // zero means "no key" and a failure leaves every region as before.
        text.push_str(&format!(
            "define internal void @{REGION_INIT}() {{\nentry:\n  %slot = alloca i64\n  \
             store i64 0, ptr %slot\n  \
             %status = call i32 @pthread_key_create(ptr %slot, ptr @{REGION_DROP})\n  \
             %ok = icmp eq i32 %status, 0\n  br i1 %ok, label %set, label %done\nset:\n  \
             %key = load i64, ptr %slot\n  %plus = add i64 %key, 1\n  store i64 %plus, ptr @{global}\n  \
             br label %done\ndone:\n  ret void\n}}\n\n"
        ));
    }
    text
}
