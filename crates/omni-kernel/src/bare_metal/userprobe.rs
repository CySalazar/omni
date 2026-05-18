//! User-probe spawn helper (MB11.7+ scaffold).
//!
//! Scaffold that boot-wires the embedded user-probe ELF into a Ring 3
//! task via [`ProcessControlBlock::spawn_from_elf`].
//!
//! Until that PR lands, this module holds the boot-wiring glue:
//!
//! - [`USERPROBE_ELF`] — placeholder embedded ELF bytes (currently
//!   reuses the MB5 [`TEST_ELF`] header-only ELF for syntactic
//!   parity; the real probe replaces the bytes).
//! - [`spawn_userprobe`] — orchestrator entry point: parses the ELF,
//!   builds an [`AddressSpace`], allocates a user stack, registers the
//!   `ProcessControlBlock` with the scheduler.
//!
//! Once the real probe is embedded, [`spawn_userprobe`] becomes the
//! single function `kmain` calls to launch the first Ring 3 task.
//! The `iretq` trampoline that actually enters Ring 3 lives in
//! [`super::usermode::enter_user_mode`].

#![allow(
    unsafe_code,
    reason = "wraps ProcessControlBlock::spawn_from_elf which is itself unsafe"
)]

#[cfg(target_arch = "x86_64")]
use crate::{
    KernelResult,
    bare_metal::paging::PageMapper,
    memory::{BitmapFrameAllocator, PhysAddr},
    process::ProcessControlBlock,
    scheduling::{PriorityClass, RoundRobinScheduler, TaskId},
};

/// Placeholder ELF for the user probe.
///
/// To be replaced by the real `omni-userprobe-helloworld` output. The
/// current bytes parse but contain no executable code; the real probe
/// binary embeds a `syscall(WriteConsole, "hello\n")` followed by
/// `syscall(TaskExit, 0)` sequence.
///
/// Until the real probe replaces these bytes, [`spawn_userprobe`] is a
/// dry-run that exercises the full spawn flow without producing any
/// observable Ring 3 output.
pub const USERPROBE_ELF: &[u8] = &[
    // ELF64 header (e_ident + e_type + e_machine + e_version)
    0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x02, 0x00, 0x3E, 0x00, 0x01, 0x00,
    0x00, 0x00, // e_entry = 0x4000_0000
    0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00,
    // e_phoff = 0x40, e_shoff = 0, e_flags = 0, e_ehsize = 0x40, e_phentsize = 0x38,
    // e_phnum = 1, e_shentsize = 0, e_shnum = 0, e_shstrndx = 0
    0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x01, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
    // Program header: PT_LOAD, PF_R | PF_X, p_offset = 0, p_vaddr = 0x4000_0000,
    // p_paddr = 0x4000_0000, p_filesz = 0, p_memsz = 0x1000, p_align = 0x1000
    0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Spawn the embedded user-probe binary as a Ring 3 process.
///
/// Reads CR3 to anchor the per-process page-table clone, parses
/// [`USERPROBE_ELF`], allocates a user stack, registers the
/// `ProcessControlBlock` with the scheduler. The first dispatch tick
/// after this call will hand the CPU to the new task via
/// [`super::usermode::enter_user_mode`].
///
/// # Errors
///
/// Same as [`ProcessControlBlock::spawn_from_elf`].
///
/// # Safety
///
/// The caller must own the kernel singletons (single-CPU, no aliasing).
#[cfg(target_arch = "x86_64")]
pub unsafe fn spawn_userprobe<const N: usize>(
    mapper: &mut PageMapper,
    alloc: &mut BitmapFrameAllocator<N>,
    scheduler: &mut RoundRobinScheduler,
) -> KernelResult<TaskId> {
    // SAFETY: caller invariants documented above.
    unsafe {
        let boot_cr3 = PhysAddr(mapper.root_phys.0);
        ProcessControlBlock::spawn_from_elf(
            USERPROBE_ELF,
            boot_cr3,
            mapper,
            alloc,
            scheduler,
            PriorityClass::Interactive,
        )
    }
}
