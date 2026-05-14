//! Host-mode integration tests for the K3 bump allocator
//! ([`omni_kernel::bare_metal::heap::BumpHeap`]).
//!
//! Specified by `OIP-Kernel-012` § S4. The tests run on the developer
//! host (NOT under `x86_64-unknown-none`) because the test harness
//! itself needs `std` for the runner; this is acceptable because the
//! `BumpHeap` type is `#[cfg(feature = "bare-metal")]` only — the
//! `#[global_allocator]` attribute is the `target_os = "none"` part —
//! so the *type* is host-buildable and exercisable against a
//! synthetic `[u8; N]` heap region.

#![cfg(feature = "bare-metal")]
// Test-only relaxations: integration tests exercise unsafe APIs by
// design (BumpHeap::init, GlobalAlloc::alloc), fail-loudly via
// `expect`/`unwrap`, and do casts/indexing against statically-sized
// fixtures. The corresponding workspace lints are intentionally not
// propagated into the test target.
#![allow(unsafe_code)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::multiple_unsafe_ops_per_block,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::integer_division,
    clippy::cast_possible_truncation,
    clippy::missing_docs_in_private_items,
    clippy::uninlined_format_args
)]

use core::alloc::{GlobalAlloc, Layout};

use omni_kernel::bare_metal::heap::BumpHeap;

/// Fresh, uninitialised `BumpHeap` instance for use as a test fixture.
fn fresh_heap() -> BumpHeap {
    BumpHeap::new()
}

/// A synthetic heap region. We use `'static` so the `unsafe { init }`
/// pointer remains valid for the entire test duration without
/// borrowing-checker contortions.
fn static_region(len: usize) -> (*mut u8, usize) {
    let buf = vec![0u8; len].leak();
    (buf.as_mut_ptr(), buf.len())
}

#[test]
fn uninitialised_heap_allocates_null() {
    // Per § S2 constraint: an uninitialised allocator returns null
    // (the alloc crate's OOM path takes over downstream).
    let heap = fresh_heap();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = unsafe { heap.alloc(layout) };
    assert!(ptr.is_null(), "uninitialised heap must return null");
    assert!(!heap.is_initialised());
}

#[test]
fn init_marks_heap_initialised_and_reports_total() {
    let heap = fresh_heap();
    let (base, len) = static_region(8 * 1024);
    unsafe { heap.init(base, len) };
    assert!(heap.is_initialised());
    assert_eq!(heap.total_bytes(), 8 * 1024);
    assert_eq!(heap.used_bytes(), 0);
}

#[test]
fn allocations_are_monotonic_and_aligned() {
    let heap = fresh_heap();
    let (base, len) = static_region(64 * 1024);
    unsafe { heap.init(base, len) };

    let layout8 = Layout::from_size_align(13, 8).unwrap();
    let p1 = unsafe { heap.alloc(layout8) };
    assert!(!p1.is_null());
    assert_eq!((p1 as usize) % 8, 0, "p1 must be 8-byte aligned");

    let layout16 = Layout::from_size_align(40, 16).unwrap();
    let p2 = unsafe { heap.alloc(layout16) };
    assert!(!p2.is_null());
    assert_eq!((p2 as usize) % 16, 0, "p2 must be 16-byte aligned");

    assert!(
        (p2 as usize) >= (p1 as usize) + 13,
        "p2 must be past p1+size"
    );
    assert!(heap.used_bytes() >= 13 + 40);
}

#[test]
fn oom_returns_null_without_panic() {
    let heap = fresh_heap();
    let (base, len) = static_region(256);
    unsafe { heap.init(base, len) };

    // First allocation: 100 bytes, 1-byte aligned. Should succeed.
    let l1 = Layout::from_size_align(100, 1).unwrap();
    let p1 = unsafe { heap.alloc(l1) };
    assert!(!p1.is_null());

    // Second allocation: 200 bytes, 1-byte aligned. Total 100 + 200 =
    // 300 > 256 byte heap. Must return null, NOT panic.
    let l2 = Layout::from_size_align(200, 1).unwrap();
    let p2 = unsafe { heap.alloc(l2) };
    assert!(p2.is_null(), "OOM must return null, not panic");
}

#[test]
fn zero_size_alloc_still_advances_alignment() {
    // A zero-size allocation must still return an aligned pointer; the
    // `next` pointer is advanced only by alignment padding.
    let heap = fresh_heap();
    let (base, len) = static_region(1024);
    unsafe { heap.init(base, len) };

    let layout = Layout::from_size_align(0, 64).unwrap();
    let p = unsafe { heap.alloc(layout) };
    assert!(!p.is_null());
    assert_eq!((p as usize) % 64, 0);
}

#[test]
fn dealloc_is_noop() {
    // Per § S2 constraint 2: dealloc never frees. used_bytes is
    // monotonic; calling dealloc does not reset it.
    let heap = fresh_heap();
    let (base, len) = static_region(1024);
    unsafe { heap.init(base, len) };

    let layout = Layout::from_size_align(64, 8).unwrap();
    let p = unsafe { heap.alloc(layout) };
    let used_before = heap.used_bytes();
    unsafe { heap.dealloc(p, layout) };
    assert_eq!(
        heap.used_bytes(),
        used_before,
        "dealloc must not reset used_bytes"
    );
}

#[test]
#[should_panic(expected = "BumpHeap::init called twice")]
fn double_init_panics() {
    let heap = fresh_heap();
    let (base, len) = static_region(1024);
    unsafe { heap.init(base, len) };
    // Second call MUST panic per § S2 constraint 1.
    unsafe { heap.init(base, len) };
}

#[test]
fn large_alignment_is_honored() {
    // Pathological case: alignment larger than typical cache line
    // (4096 — page-sized). The bump pointer must skip enough to land
    // on the next 4 KiB boundary.
    let heap = fresh_heap();
    let (base, len) = static_region(32 * 1024);
    unsafe { heap.init(base, len) };

    // Burn some bytes so the bump pointer is *not* at a page boundary.
    let prelude = Layout::from_size_align(7, 1).unwrap();
    let _ = unsafe { heap.alloc(prelude) };

    let page = Layout::from_size_align(64, 4096).unwrap();
    let p = unsafe { heap.alloc(page) };
    assert!(!p.is_null());
    assert_eq!((p as usize) % 4096, 0, "must be 4 KiB aligned");
}

#[test]
fn allocations_fall_inside_heap_region() {
    // Every returned pointer must lie in [base, end).
    let heap = fresh_heap();
    let (base, len) = static_region(4096);
    unsafe { heap.init(base, len) };
    let base_addr = base as usize;

    for _ in 0..16 {
        let layout = Layout::from_size_align(64, 8).unwrap();
        let p = unsafe { heap.alloc(layout) };
        assert!(!p.is_null());
        let addr = p as usize;
        assert!(addr >= base_addr, "p must be at or above base");
        assert!(addr + 64 <= base_addr + len, "p+size must be within region");
    }
}
