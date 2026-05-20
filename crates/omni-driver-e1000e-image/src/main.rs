//! OMNI OS Intel e1000e bootable driver image — P6.7.8.7.
//!
//! `no_std + no_main` ELF entry that the kernel `DriverLoad (73)`
//! syscall ingests per `OIP-Driver-Framework-013` § S5.3 step 9. The
//! kernel calls `spawn_from_elf` against this binary, which lands at
//! `_start` in a freshly minted Ring 3 process with the per-driver
//! capability tokens deposited at well-known user-VA slots by the
//! kernel's driver-loader trampoline (§ S5.3 step 10, to be wired in a
//! follow-up P6.7.8.x).
//!
//! This image is the smallest possible *runnable* e1000e driver: it
//! walks the [`omni_driver_e1000e::bringup::BringUp`] FSM from
//! `Phase::PciEnumeration` → `Phase::Ready`, posting `Event::Advance`
//! on each step. The actual syscall invocations are stubbed for
//! P6.7.8.7 (the real `MmioMap` / `DmaMap` / `IrqAttach` arg layouts
//! depend on the `DriverLoad` capability deposit, which is a follow-up);
//! they appear as raw `syscall` instructions with
//! `rax = SyscallNumber` and the kernel returns through the rich
//! two-register path.
//!
//! Pattern mirrors the `omni-driver-nvme-image` sibling introduced in
//! P6.7.8.5, the `omni-driver-net-virtio-image` sibling introduced in
//! P6.7.8.3, and the `omni-kernel` ↔ `kernel-runner` split that already
//! powers the bare-metal boot path.
//!
//! Build:
//!
//! ```sh
//! cargo build --manifest-path crates/omni-driver-e1000e-image/Cargo.toml \
//!             --target x86_64-unknown-none --release
//! ```
//!
//! The resulting `target/x86_64-unknown-none/release/omni-driver-e1000e-image`
//! ELF is fed to `omni-driver-pack` along with the `manifest.toml` template
//! to produce the `omni-pack v1` blob the kernel verifies.

#![no_std]
#![no_main]
#![allow(unsafe_code)]
#![warn(missing_docs)]

use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

use omni_driver_e1000e::bringup::{BringUp, Event, Phase};

// =============================================================================
// Global allocator stub
// =============================================================================
//
// The `omni-driver-e1000e` library has `extern crate alloc` because future
// bring-up code will need `alloc::vec::Vec` for the RX/TX descriptor
// rings and the per-channel queue bookkeeping. The P6.7.8.7 skeleton
// does not allocate at runtime — the bring-up FSM is `Copy`, the syscall
// wrappers use `[u64; 6]` on the stack, and we never instantiate any
// heap container.
//
// We still need a `#[global_allocator]` declaration to satisfy the
// linker. The `PanicOnAlloc` allocator panics on any allocation
// request, which:
//   - keeps the binary auditable: any heap call would be a P6.7.8.x bug
//     (the bring-up should not need `alloc` until the descriptor-ring
//     allocation lands), and
//   - signals the missing functionality loudly when that bug appears
//     (via `TaskExit(2)` from the panic handler below).

struct PanicOnAlloc;

unsafe impl GlobalAlloc for PanicOnAlloc {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        // SAFETY: any reachable allocation is a driver bug — bail loudly
        // via the panic handler which `TaskExit`s the process.
        panic!(
            "omni-driver-e1000e-image: heap alloc requested but no allocator is wired (P6.7.8.x follow-up)"
        );
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Unreachable in practice (no `alloc` ever succeeds), but a no-op
        // dealloc is the only sound option without an allocator.
    }
}

#[global_allocator]
static GLOBAL_ALLOC: PanicOnAlloc = PanicOnAlloc;

// =============================================================================
// Syscall numbers (mirrors `omni_kernel::syscall::SyscallNumber` — pinned
// here so the image does not pull in the kernel crate, which would create
// a circular workspace dep).
// =============================================================================

/// `TaskExit (11)`.
const SYS_TASK_EXIT: u64 = 11;

/// `WriteConsole (60)`.
#[allow(dead_code, reason = "reserved for diagnostic banner emission")]
const SYS_WRITE_CONSOLE: u64 = 60;

/// `MmioMap (70)`.
#[allow(dead_code, reason = "to be wired post-DriverLoad capability deposit")]
const SYS_MMIO_MAP: u64 = 70;

/// `DmaMap (71)`.
#[allow(dead_code, reason = "to be wired post-DriverLoad capability deposit")]
const SYS_DMA_MAP: u64 = 71;

/// `IrqAttach (72)`.
#[allow(dead_code, reason = "to be wired post-DriverLoad capability deposit")]
const SYS_IRQ_ATTACH: u64 = 72;

// =============================================================================
// Raw syscall wrapper (System V AMD64 ABI: rax=number, rdi/rsi/rdx/r10/r8/r9
// = a0..a5; rcx + r11 clobbered by `syscall`).
// =============================================================================

/// Issue a `syscall` with the given number and up to 5 arguments. Returns
/// the `(rax, rdx)` pair — the two-register convention used by the
/// driver-framework syscalls per `OIP-Driver-Framework-013` § S2.
#[allow(
    dead_code,
    reason = "wired in P6.7.8.x once DriverLoad deposits the tokens"
)]
#[inline(always)]
unsafe fn syscall5(number: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> (u64, u64) {
    let mut rax: u64 = number;
    let mut rdx_out: u64;
    // SAFETY: `syscall` is the canonical Ring 3 → Ring 0 transition on
    // `x86_64`; rax/rcx/r11 are clobbered by the CPU per the SDM. The
    // kernel's `omni_syscall_entry` preserves the rest of the GPR
    // file across the call.
    unsafe {
        core::arch::asm!(
            "syscall",
            inout("rax") rax,
            in("rdi") a0,
            in("rsi") a1,
            inout("rdx") a2 => rdx_out,
            in("r10") a3,
            in("r8")  a4,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags),
        );
    }
    (rax, rdx_out)
}

/// Issue `TaskExit(code)` — diverges on the bare-metal kernel. The
/// kernel's `task_exit` handler never returns to user space (it dequeues
/// the task and yields); the trailing `loop {}` is defensive against a
/// hypothetical kernel bug that lets the syscall return.
#[inline(always)]
unsafe fn sys_exit(code: u64) -> ! {
    // SAFETY: TaskExit invariant: `code` is just a numeric exit-status
    // bag; the kernel does not interpret it semantically in Phase 1.
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_TASK_EXIT,
            in("rdi") code,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    loop {
        core::hint::spin_loop();
    }
}

// =============================================================================
// Driver entry — _start
// =============================================================================

/// ELF entry point. The kernel's `spawn_from_elf` jumps here with
/// `rsp = user_stack_top` and the capability deposit at well-known
/// user-VA slots (P6.7.8.x).
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // P6.7.8.7 driver-image skeleton:
    //
    // 1. Drive the 13-step bring-up FSM to Ready in a self-test loop.
    //    The actual MmioMap / DmaMap / IrqAttach syscalls are stubbed
    //    for P6.7.8.7 — they require the capability tokens the kernel
    //    deposits via the DriverLoad trampoline (P6.7.8.x). Today we
    //    exercise the pure FSM logic so the image's link path is
    //    end-to-end auditable.
    let mut bringup = BringUp::new();
    while !bringup.phase().is_terminal() {
        // Pure-function advance — no syscalls yet.
        match bringup.on_event(Event::Advance) {
            Ok(next) => bringup = next,
            Err(_) => break,
        }
    }

    // Exit with 0 when the FSM converged to Ready; non-zero on any
    // other terminal state so the kernel boot log surfaces the failure.
    let code = if matches!(bringup.phase(), Phase::Ready) {
        0
    } else {
        1
    };
    // SAFETY: TaskExit never returns; pinned by the `noreturn` option
    // on the asm block.
    unsafe { sys_exit(code) }
}

// =============================================================================
// Panic handler (required by `no_std`)
// =============================================================================

/// On panic, exit with a sentinel non-zero code so the kernel boot log
/// can correlate against the bring-up retry counter.
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    // SAFETY: TaskExit terminates the process unconditionally.
    unsafe { sys_exit(2) }
}
