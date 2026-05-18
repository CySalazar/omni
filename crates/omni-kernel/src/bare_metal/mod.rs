//! Bare-metal kernel runtime — panic handler, global allocator, early
//! console, and arch-specific intrinsics.
//!
//! This module is the **K3 deliverable of `OIP-Kernel-003` § 3**,
//! specified by [`OIP-Kernel-012`](../../../../oips/oip-kernel-012.md).
//! Its files exist only when the `bare-metal` feature is enabled;
//! removing the feature folds the module out of the crate's source.
//!
//! ## Layout
//!
//! - [`mod@panic`] — `#[panic_handler]` plus the structured `PanicRecord`
//!   type emitted to the early-boot console. Non-allocating, interrupt-
//!   disabled, halt-on-completion (see `OIP-Kernel-012` § S1).
//! - [`heap`] — `BumpHeap` global allocator, one-shot `init`, atomic
//!   `fetch_update` bump pointer, no `dealloc` (see § S2).
//! - [`early_console`] — pre-init writer to the 16550 UART on COM1
//!   (0x3f8). The panic record is encoded into a static buffer and
//!   flushed via this module byte-by-byte.
//! - [`arch`] — architecture-specific intrinsics: interrupt disable,
//!   halt-forever, port I/O. The `x86_64` impl uses `core::arch::asm`;
//!   a no-op stub exists for non-x86 hosts so that host tests on
//!   developer ARM machines still compile.
//!
//! ## Visibility under `cfg(test)`
//!
//! The **types** in this module (`PanicRecord`, `PanicLocation`,
//! `BumpHeap`) are visible under both `cfg(test)` and the bare-metal
//! build. The **attribute-bearing items** — `#[panic_handler]`,
//! `#[global_allocator]` — are gated `#[cfg(not(test))]` because the
//! standard test harness installs its own panic handler and allocator
//! and would conflict otherwise.
//!
//! This split is what makes `cargo test --workspace --all-features`
//! (with `bare-metal` on) still pass: the type surface is tested
//! against a synthetic heap region in host mode, while the attribute-
//! bearing globals are only present in the bare-metal binary.

#![allow(unsafe_code)]

pub mod address_space;
pub mod arch;
#[cfg(target_arch = "x86_64")]
pub mod context_switch;
pub mod cursor;
pub mod demo;
pub mod early_console;
pub mod elf_loader;
pub mod font;
pub mod gdt;
pub mod graphics;
pub mod heap;
pub mod idt;
pub mod input;
#[cfg(target_arch = "x86_64")]
pub mod lapic;
#[cfg(all(
    target_arch = "x86_64",
    target_os = "none",
    feature = "mb8-smoke",
    not(test)
))]
pub mod mb8_smoke;
pub mod paging;
pub mod panic;
pub mod syscall_entry;
pub mod tss;
pub mod user_stack;
pub mod usermode;
pub mod userprobe;
pub mod userprobe_mb12;
pub mod vga;
pub mod virtio_tablet;
pub mod widget;
pub mod wm;
