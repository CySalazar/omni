//! In-crate bump allocator backing the kernel `#[global_allocator]`.
//!
//! Specified by [`OIP-Kernel-012`] § S2. The allocator is **bump**:
//! every allocation advances a single atomic pointer; nothing is ever
//! freed. This is the smallest possible TCB surface for a kernel-side
//! allocator (≈ 80 lines of `unsafe`-free Rust over `core::sync::
//! atomic`) and matches the pattern used by `seL4`, `NOVA`, and
//! `Redox`'s early-boot path.
//!
//! ## Properties (binding by § S2)
//!
//! 1. **One-shot `init`.** Setting `base`/`end` more than once panics.
//! 2. **No `dealloc`.** `dealloc()` is a no-op. The kernel's heap is
//!    populated with long-lived structures (IPC ring buffers, task
//!    table, capability table); transient allocations either avoid
//!    the kernel heap or live for the kernel's lifetime.
//! 3. **Honour alignment.** The bump pointer is rounded up before
//!    each allocation.
//! 4. **OOM returns null.** When `next + size > end`, `alloc` returns
//!    `null_mut()`. The Rust `alloc` crate routes this through its
//!    default alloc-error hook, which on `no_std` ultimately panics
//!    into our [`super::panic`] handler.
//! 5. **Single-CPU at v1.0** — the atomic operations are present so
//!    a future SMP enablement does not require an allocator rewrite.
//! 6. **No external crate.** `linked_list_allocator`, `talc`, and
//!    `buddy_system_allocator` are all reasonable v1.x candidates but
//!    are deferred behind a separate OIP (each adds an external trust
//!    base).
//!
//! ## API surface
//!
//! - [`BumpHeap`] — the allocator type. `pub const fn new()` so it
//!   can be used to initialise a `static` at compile time.
//! - [`BumpHeap::init`] — one-shot installation of the heap region,
//!   called from `kernel_entry` (the runner) once `BootInfo` is
//!   available. K3 leaves the region-selection policy to the runner;
//!   K4 / `OIP-Kernel-005` adds `pick_region` to bridge `BootInfo`'s
//!   `MemoryRegions` to this `init` call.
//! - `#[global_allocator] static GLOBAL_HEAP` — the singleton
//!   instance, attribute-gated `target_os = "none"` so the test
//!   harness on host keeps its own allocator.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Magic value for `BumpHeap::base` / `end` in the uninitialised
/// state. Picked so a stray dereference traps deterministically
/// (canonical-non-canonical `x86_64` address).
const UNINIT: usize = 0;

/// Single-allocator-per-binary bump heap.
///
/// All three fields are `AtomicUsize` rather than `AtomicPtr<u8>`
/// because the alignment math is cleaner in integer space and the
/// final pointer conversion is a single cast at allocation time.
pub struct BumpHeap {
    base: AtomicUsize,
    next: AtomicUsize,
    end: AtomicUsize,
}

impl BumpHeap {
    /// Construct an uninitialised heap.
    ///
    /// The result is **not safe to allocate against** until
    /// [`Self::init`] runs.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            base: AtomicUsize::new(UNINIT),
            next: AtomicUsize::new(UNINIT),
            end: AtomicUsize::new(UNINIT),
        }
    }

    /// One-shot installation of the heap region.
    ///
    /// `base` is the lowest address of a contiguous region of `len`
    /// bytes the allocator may hand out. The pointer + length pair is
    /// sourced from the bootloader's memory map (`OIP-Kernel-005`
    /// `pick_region` in K4); for K3 the caller is the test harness or
    /// the kernel-runner shim invoking this directly.
    ///
    /// # Safety
    ///
    /// The caller MUST guarantee that:
    /// - `base..base + len` is a single contiguous mapped region the
    ///   kernel exclusively owns for the remainder of the boot.
    /// - The region is at least `MIN_HEAP_BYTES` long (the K4
    ///   `pick_region` enforces this; the K3 path is documented as a
    ///   stop-gap).
    /// - `init` is called exactly once. A second invocation panics.
    ///
    /// # Panics
    ///
    /// Panics if the heap has already been initialised (the `base`
    /// CAS from `UNINIT` to `base as usize` fails).
    pub unsafe fn init(&self, base: *mut u8, len: usize) {
        let base_addr = base as usize;
        let Some(end_addr) = base_addr.checked_add(len) else {
            // The K4 `pick_region` enforces a 4 MiB minimum and reads
            // its inputs from the bootloader memory map, both of which
            // exclude this overflow path in practice. A kernel that
            // *did* reach it is in an invariant-violated state and
            // panicking is the correct loud signal — captured here
            // explicitly so the `Option::expect` lint stays clean.
            #[allow(
                clippy::panic,
                reason = "kernel invariant violation: heap region overflows usize"
            )]
            {
                panic!("BumpHeap::init: base + len overflows usize");
            }
        };
        // Atomically install `base` exactly once. A second init() call
        // observes a non-UNINIT `base` and is reported as a kernel
        // invariant violation.
        if self
            .base
            .compare_exchange(UNINIT, base_addr, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.end.store(end_addr, Ordering::Release);
            self.next.store(base_addr, Ordering::Release);
        } else {
            #[allow(
                clippy::panic,
                reason = "kernel invariant violation: BumpHeap::init called twice"
            )]
            {
                panic!("BumpHeap::init called twice — kernel invariant violation");
            }
        }
    }

    /// Report whether the heap has been initialised.
    ///
    /// Visible to host tests that want to assert pre/post-init
    /// behaviour separately. The bare-metal binary never calls this
    /// — `init` is invoked exactly once from `kernel_entry`.
    #[must_use]
    pub fn is_initialised(&self) -> bool {
        self.base.load(Ordering::Acquire) != UNINIT
    }

    /// Returns the number of bytes already handed out by the
    /// allocator (useful for forensics / telemetry; not part of the
    /// `GlobalAlloc` contract).
    #[must_use]
    pub fn used_bytes(&self) -> usize {
        let base = self.base.load(Ordering::Acquire);
        let next = self.next.load(Ordering::Acquire);
        next.saturating_sub(base)
    }

    /// Returns the total heap region size in bytes.
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        let base = self.base.load(Ordering::Acquire);
        let end = self.end.load(Ordering::Acquire);
        end.saturating_sub(base)
    }
}

impl Default for BumpHeap {
    fn default() -> Self {
        Self::new()
    }
}

/// Round `addr` up to a multiple of `align`.
///
/// `align` MUST be a power of two — `core::alloc::Layout` invariants
/// guarantee this for any `Layout` we receive.
#[inline]
const fn align_up(addr: usize, align: usize) -> usize {
    // `(addr + align - 1) & !(align - 1)` rounds up to a multiple of
    // `align`. The pre-add is checked for overflow at the call site
    // via the subsequent `> end` comparison; we use `wrapping_add`
    // here because an overflow downstream is caught structurally.
    addr.wrapping_add(align - 1) & !(align - 1)
}

// SAFETY: `BumpHeap` upholds the `GlobalAlloc` contract:
//  - `alloc` returns either a properly-aligned, non-null pointer to
//    `layout.size()` writable bytes inside the heap region, or null.
//  - `dealloc` is a no-op, which is a valid `GlobalAlloc` impl for an
//    arena allocator (memory simply leaks until kernel shutdown).
//  - All pointer arithmetic is bounded by `base ≤ next ≤ end`, the
//    sole invariant. The `compare_exchange_weak` loop maintains it
//    under any interleaving across CPUs.
unsafe impl GlobalAlloc for BumpHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let end = self.end.load(Ordering::Acquire);
        if end == UNINIT {
            // Allocator not initialised. Returning null here lets the
            // `alloc` crate trigger its OOM handler — which will
            // route into our panic handler.
            return ptr::null_mut();
        }
        let mut current = self.next.load(Ordering::Acquire);
        loop {
            let aligned = align_up(current, layout.align());
            let Some(new_next) = aligned.checked_add(layout.size()) else {
                return ptr::null_mut();
            };
            if new_next > end {
                return ptr::null_mut();
            }
            match self.next.compare_exchange_weak(
                current,
                new_next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return aligned as *mut u8,
                Err(actual) => current = actual,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // No-op. See § S2 constraint 2.
    }
}

/// The singleton `#[global_allocator]` instance for the bare-metal
/// kernel.
///
/// Gated `target_os = "none"` (the bare-metal cross-target) so that a
/// host build with `--features bare-metal` does not try to install a
/// second `#[global_allocator]` alongside `std`'s default. Host
/// integration tests at `tests/heap.rs` construct a fresh `BumpHeap`
/// over a stack `[u8; N]` buffer and exercise the type directly
/// without registering it globally.
#[cfg(all(target_os = "none", not(test)))]
#[global_allocator]
pub static GLOBAL_HEAP: BumpHeap = BumpHeap::new();
