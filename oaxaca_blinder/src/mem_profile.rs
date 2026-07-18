//! Native memory-profiling harness (0014-MERIDIAN, In-Scope 11 / D1).
//!
//! Feature-gated (`mem-profile`), native + dev/test only (INV-01): a tracking
//! `GlobalAlloc` wrapping the system allocator, recording `current` and
//! `peak` byte counters (`stats_alloc`-style), plus a small checkpoint API
//! that `builder.rs::run()` calls at two additive, side-effect-only points —
//! immediately after the categorical dummy `hstack` (checkpoint A) and
//! immediately before the bootstrap `into_par_iter()` loop (checkpoint B).
//! Neither call touches the deterministic bootstrap RNG logic (`unit_rng`,
//! `resample_indices`, `take`, `vstack`) it brackets.
//!
//! Measurement plan (owned by the profiling harness / example, not this
//! module): `H_res(n)` = `current_bytes()` read at checkpoint B. `Sc(n)` =
//! `peak_bytes()` (with `set_reset_peak_at_checkpoint_b(true)`, so checkpoint
//! B re-anchors the peak tracker to its own baseline) minus `H_res(n)`,
//! measured on a `bootstrap_reps(1)` run so exactly one rep is in flight.
//! `H_peak(n)` = `peak_bytes()` (with `set_reset_peak_at_checkpoint_b(false)`
//! and a harness-side `reset_peak()` called once before the whole `run()`),
//! measured on a `bootstrap_reps(100)` run with the rayon global pool pinned
//! to 1 thread (single-threaded per D1's checkpoint-C definition).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Bytes currently live (bytes allocated minus bytes freed), tracked by
/// [`TrackingAllocator`]. Not a substitute for OS RSS — it is an exact count
/// of bytes requested via `GlobalAlloc`, which is what the D1 profile design
/// needs (schedule- and fragmentation-independent).
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// High-water mark of [`CURRENT`] since the last [`reset_peak`].
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// `current_bytes()` recorded the last time checkpoint A fired.
static CKPT_A_CURRENT: AtomicUsize = AtomicUsize::new(0);

/// `current_bytes()` recorded the last time checkpoint B fired.
static CKPT_B_CURRENT: AtomicUsize = AtomicUsize::new(0);

/// `peak_bytes()` recorded the last time checkpoint B fired — lets a harness
/// separate the pre-loop peak (point-estimate + hstack transient) from the
/// bootstrap-loop peak, to attribute where the high-water mark actually comes
/// from (diagnostic for the 0014-MERIDIAN memory profile).
static CKPT_B_PEAK: AtomicUsize = AtomicUsize::new(0);

/// When true, checkpoint B also calls [`reset_peak`] (re-anchoring the peak
/// tracker to the checkpoint-B baseline) so a subsequent [`peak_bytes`] read
/// reports only the bootstrap phase's own marginal high-water mark (the `Sc`
/// measurement design). When false (the `H_peak` measurement design),
/// checkpoint B leaves the peak tracker alone so it keeps running from
/// wherever the harness reset it (normally before the whole `run()` call).
static RESET_PEAK_AT_CHECKPOINT_B: AtomicBool = AtomicBool::new(false);

/// Wraps [`System`] with atomic current/peak byte counters. Lock-free
/// (`fetch_add`/`fetch_sub`/`fetch_max`), so it is safe under rayon's
/// `into_par_iter()` — checkpoint C's 100-rep run allocates concurrently
/// across worker threads even though D1 pins the pool to 1 thread for the
/// profiling measurements themselves.
pub struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            let new = CURRENT.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(new, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc_zeroed(layout);
        if !ptr.is_null() {
            let new = CURRENT.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(new, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        CURRENT.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_ptr = System.realloc(ptr, layout, new_size);
        if !new_ptr.is_null() {
            if new_size >= layout.size() {
                let delta = new_size - layout.size();
                let new = CURRENT.fetch_add(delta, Ordering::SeqCst) + delta;
                PEAK.fetch_max(new, Ordering::SeqCst);
            } else {
                let delta = layout.size() - new_size;
                CURRENT.fetch_sub(delta, Ordering::SeqCst);
            }
        }
        new_ptr
    }
}

/// Current live byte count (allocated minus freed, process-wide).
pub fn current_bytes() -> usize {
    CURRENT.load(Ordering::SeqCst)
}

/// Peak byte count since the last [`reset_peak`].
pub fn peak_bytes() -> usize {
    PEAK.load(Ordering::SeqCst)
}

/// Reset the peak counter to the current live byte count. Does not touch
/// `CURRENT` (which always reflects real live bytes, never resettable to an
/// arbitrary value). Call this immediately before the bracket you want a
/// clean high-water mark for.
pub fn reset_peak() {
    let cur = CURRENT.load(Ordering::SeqCst);
    PEAK.store(cur, Ordering::SeqCst);
}

/// Configure whether checkpoint B re-anchors the peak tracker. See the module
/// doc for the `Sc` vs `H_peak` measurement designs this switches between.
pub fn set_reset_peak_at_checkpoint_b(v: bool) {
    RESET_PEAK_AT_CHECKPOINT_B.store(v, Ordering::SeqCst);
}

/// Checkpoint A: immediately after the categorical dummy `hstack` (a
/// sub-component of `H_res`, per D1). Additive, side-effect-only — does not
/// alter control flow or the bootstrap RNG logic it precedes.
pub fn checkpoint_a() {
    CKPT_A_CURRENT.store(current_bytes(), Ordering::SeqCst);
}

/// Checkpoint B: immediately before the bootstrap `into_par_iter()` loop —
/// the `H_res(n)` measurement point. Additive, side-effect-only.
pub fn checkpoint_b() {
    CKPT_B_CURRENT.store(current_bytes(), Ordering::SeqCst);
    CKPT_B_PEAK.store(peak_bytes(), Ordering::SeqCst);
    if RESET_PEAK_AT_CHECKPOINT_B.load(Ordering::SeqCst) {
        reset_peak();
    }
}

/// `peak_bytes()` as recorded the last time checkpoint B fired — the pre-loop
/// high-water mark (point-estimate + hstack). Compare to the final
/// `peak_bytes()` to attribute the peak between the serial point estimate and
/// the bootstrap loop.
pub fn last_checkpoint_b_peak() -> usize {
    CKPT_B_PEAK.load(Ordering::SeqCst)
}

/// `current_bytes()` as recorded the last time checkpoint A fired.
pub fn last_checkpoint_a() -> usize {
    CKPT_A_CURRENT.load(Ordering::SeqCst)
}

/// `current_bytes()` as recorded the last time checkpoint B fired — `H_res(n)`.
pub fn last_checkpoint_b() -> usize {
    CKPT_B_CURRENT.load(Ordering::SeqCst)
}

/// Approximate current stack-pointer address via a local variable's address.
/// Stack grows downward on the platforms this profile targets (x86_64 /
/// aarch64 Linux), so comparing two same-thread samples approximates bytes of
/// stack consumed between them. Used only as a coarse cross-check; the
/// authoritative `St_obs` measurement is the process-survival stack-size
/// probe in the profiling harness (see `examples/mem_profile_harness.rs`),
/// not this function.
#[inline(never)]
pub fn stack_addr() -> usize {
    let local: u8 = 0;
    std::hint::black_box(&local as *const u8 as usize)
}
