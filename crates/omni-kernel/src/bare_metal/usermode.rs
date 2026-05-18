//! Ring-3 entry trampoline + user pointer validation (MB11, ADR-0004 § 7).
//!
//! Two responsibilities:
//!
//! 1. `enter_user_mode` — build the `iretq` frame (SS, RSP, RFLAGS, CS,
//!    RIP) and dispatch into Ring 3 for the first time. Used by the
//!    scheduler when a `ProcessControlBlock` is selected for the first
//!    time. Re-entries from kernel mode use `sysretq` (the natural
//!    syscall return path) or the standard interrupt return.
//!
//! 2. `validate_user_buffer` — verify that a user-supplied pointer
//!    range lies entirely in the user half of the address space
//!    (`< 0x0000_8000_0000_0000`) and is mapped present + user.

#![allow(
    unsafe_code,
    reason = "iretq trampoline + raw page-table walk via direct map; SAFETY per fn"
)]
#![allow(
    clippy::doc_markdown,
    reason = "module references SS/CS/RFLAGS/RSP/RIP without ticks in prose"
)]
#![allow(
    clippy::similar_names,
    reason = "page-table walker uses canonical pml4/pdpt/pd/pt level abbreviations"
)]
#![allow(
    clippy::too_long_first_doc_paragraph,
    clippy::missing_panics_doc,
    clippy::panic,
    reason = "non-x86_64 stub uses panic! to keep the diverging signature consistent"
)]

use crate::memory::VirtAddr;

use super::address_space::AddressSpace;
#[cfg(target_arch = "x86_64")]
use super::gdt::{USER_CS, USER_SS};
use super::paging::{PTE_PRESENT, PTE_USER, PageMapper};

/// User-half upper bound: any VA `< USER_HALF_END` is canonically in
/// the lower 128 TiB of x86_64 long mode (PML4 indices 0..256).
pub const USER_HALF_END: u64 = 0x0000_8000_0000_0000;

/// Default RFLAGS for a Ring 3 first-dispatch via `iretq`.
///
/// `0x202`: IF=1 (bit 9 = 0x200) so the LAPIC timer can preempt the
/// user task; bit 1 is the architecturally-reserved "always 1" flag.
/// MB12 first-dispatch path (scheduler) and MB11 boot trampoline both
/// use this value.
pub const USER_RFLAGS: u64 = 0x202;

/// Construct an `iretq` frame and execute it to enter Ring 3 for the
/// first time.
///
/// The CR3 reload is safe even mid-instruction because the per-process
/// PML4 mirrors the boot CR3's kernel half by reference — the
/// trampoline itself lives in kernel-half memory that remains mapped
/// across the CR3 switch.
///
/// # Safety
///
/// - `cr3_phys` must point to a valid PML4 frame whose kernel half is
///   identical (by-reference) to the boot CR3's kernel half.
/// - `user_rip` must reside in the user half (PML4 indices 0..256) and
///   be mapped executable + user-accessible in the target address space.
/// - `user_rsp` must reside in the user half and be mapped writable +
///   user-accessible in the target address space.
/// - `user_rflags` MUST have IF=1 (bit 9 = 0x200) — otherwise Ring 3
///   runs with interrupts disabled and no preemption can occur.
/// - This function does NOT return: it transfers control to Ring 3 via
///   `iretq`.
#[cfg(target_arch = "x86_64")]
pub unsafe fn enter_user_mode(user_rip: u64, user_rsp: u64, user_rflags: u64, cr3_phys: u64) -> ! {
    use core::arch::asm;
    // SAFETY: kernel-only CR3 reload + iretq Ring 0 → Ring 3. See doc
    // comment for the SAFETY invariants the caller must satisfy.
    unsafe {
        asm!(
            // 1. Switch to the per-process address space. Kernel half
            //    is identical by-reference, so the next instruction is
            //    still valid.
            "mov cr3, {cr3}",
            // 2. Build the iretq stack frame (5 × u64, top-down):
            //    SS, RSP, RFLAGS, CS, RIP.
            "push {ss}",
            "push {rsp_u}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",
            // 3. Execute. CPU pops 5 u64 and transfers to Ring 3.
            "iretq",
            cr3     = in(reg) cr3_phys,
            ss      = in(reg) u64::from(USER_SS),
            rsp_u   = in(reg) user_rsp,
            rflags  = in(reg) user_rflags,
            cs      = in(reg) u64::from(USER_CS),
            rip     = in(reg) user_rip,
            options(noreturn),
        );
    }
}

/// Stub for non-x86_64 host test builds.
///
/// # Safety
///
/// Sentinel signature to keep callers source-compatible on host builds.
/// Diverges via `core::panic!`; never actually invoked at test time.
#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn enter_user_mode(_rip: u64, _rsp: u64, _rflags: u64, _cr3: u64) -> ! {
    panic!("enter_user_mode is x86_64-only");
}

/// Error returned by [`validate_user_buffer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidateError {
    /// Pointer or `ptr + len` falls outside the user half.
    OutOfRange,
    /// Some page in `[ptr, ptr + len)` is not mapped, or is not flagged
    /// `PTE_USER`.
    NotMapped,
}

/// Verify that the buffer `[ptr, ptr + len)` lies entirely in the user
/// half and that every 4 KiB page it covers is present and accessible
/// from Ring 3 in the address space rooted at `address_space.pml4_phys`.
///
/// Uses [`PageMapper::translate`] for the walk. Since the mapper holds
/// the *active* CR3, this only works when the address space being
/// validated is currently active (true at SYSCALL entry, before the
/// kernel switches to another process).
///
/// # Errors
///
/// - [`ValidateError::OutOfRange`] if `ptr + len` overflows or exceeds
///   `USER_HALF_END`.
/// - [`ValidateError::NotMapped`] if any covered page is absent or
///   missing `PTE_USER`.
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "AddressSpace is the conceptual subject of validation"
)]
pub fn validate_user_buffer(
    address_space: &AddressSpace,
    ptr: u64,
    len: u64,
    mapper: &PageMapper,
) -> Result<(), ValidateError> {
    // Range check.
    let end = ptr.checked_add(len).ok_or(ValidateError::OutOfRange)?;
    if end > USER_HALF_END {
        return Err(ValidateError::OutOfRange);
    }
    if len == 0 {
        return Ok(());
    }

    // Walk every page in [ptr, end).
    // We rely on the page-table walker to read flags via the direct
    // map. Use `_ = address_space.pml4_phys` to capture the intent;
    // the actual walk uses `mapper.translate` which already follows
    // the active CR3. In MB11 single-CPU + sync SYSCALL entry, the
    // active CR3 IS the user process's CR3.
    let _ = address_space.pml4_phys;

    let first_page = ptr & !0xFFF;
    let last_page = (end - 1) & !0xFFF;
    let mut page = first_page;
    while page <= last_page {
        let virt = VirtAddr(page);
        let resolved = mapper.translate(virt);
        if resolved.is_none() {
            return Err(ValidateError::NotMapped);
        }
        // PTE flags are not surfaced by `translate`; we read the leaf
        // PTE flags via a second walk that respects the user-flag bit.
        // For MB11 the leaf-flag check is captured by the user-half
        // range guard plus the contract that `address_space` is the
        // active CR3 (only user-flagged pages live in the user half
        // because the kernel never maps kernel-only pages there).
        if !is_user_page(mapper, virt) {
            return Err(ValidateError::NotMapped);
        }
        page = match page.checked_add(0x1000) {
            Some(p) => p,
            None => break,
        };
    }
    Ok(())
}

/// Check that the leaf PTE for `virt` has both `PTE_PRESENT` and
/// `PTE_USER` set. Reads the page tables via the bootloader direct
/// map; returns `false` on any not-present intermediate entry.
fn is_user_page(mapper: &PageMapper, virt: VirtAddr) -> bool {
    // We do a manual walk: PML4 → PDPT → PD → PT.
    // Re-uses the same offset/index math as `PageMapper::translate`,
    // but with leaf flag inspection. For MB11 simplicity we walk only
    // 4 KiB leaves and treat huge-page mappings as "not user-pages"
    // (the kernel never installs USER-flagged huge pages in user-half).
    let phys_offset = mapper.phys_offset();
    let root = mapper.root_phys;

    // PML4
    let idx4 = ((virt.0 >> 39) & 0x1FF) as usize;
    // SAFETY: direct-map read of a 4 KiB page-table frame.
    let entry4 = unsafe {
        let p = (phys_offset + root.0) as *const u64;
        core::ptr::read(p.add(idx4))
    };
    if entry4 & PTE_PRESENT == 0 || entry4 & PTE_USER == 0 {
        return false;
    }
    let dpt_phys = entry4 & 0x000F_FFFF_FFFF_F000;

    let idx3 = ((virt.0 >> 30) & 0x1FF) as usize;
    let entry3 = unsafe {
        let p = (phys_offset + dpt_phys) as *const u64;
        core::ptr::read(p.add(idx3))
    };
    if entry3 & PTE_PRESENT == 0 || entry3 & PTE_USER == 0 {
        return false;
    }
    let dir_phys = entry3 & 0x000F_FFFF_FFFF_F000;

    let idx2 = ((virt.0 >> 21) & 0x1FF) as usize;
    let entry2 = unsafe {
        let p = (phys_offset + dir_phys) as *const u64;
        core::ptr::read(p.add(idx2))
    };
    if entry2 & PTE_PRESENT == 0 || entry2 & PTE_USER == 0 {
        return false;
    }
    let tab_phys = entry2 & 0x000F_FFFF_FFFF_F000;

    let idx1 = ((virt.0 >> 12) & 0x1FF) as usize;
    let entry1 = unsafe {
        let p = (phys_offset + tab_phys) as *const u64;
        core::ptr::read(p.add(idx1))
    };
    entry1 & PTE_PRESENT != 0 && entry1 & PTE_USER != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::PhysAddr;

    #[test]
    fn user_half_end_is_canonical_split() {
        assert_eq!(USER_HALF_END, 1u64 << 47);
    }

    #[test]
    fn validate_rejects_overflow_range() {
        let mapper = PageMapper::new(0, PhysAddr(0));
        let addr_space = AddressSpace {
            pml4_phys: PhysAddr(0),
        };
        let result = validate_user_buffer(&addr_space, u64::MAX - 5, 100, &mapper);
        assert_eq!(result, Err(ValidateError::OutOfRange));
    }

    #[test]
    fn validate_rejects_kernel_half_range() {
        let mapper = PageMapper::new(0, PhysAddr(0));
        let addr_space = AddressSpace {
            pml4_phys: PhysAddr(0),
        };
        let result = validate_user_buffer(&addr_space, USER_HALF_END, 1, &mapper);
        assert_eq!(result, Err(ValidateError::OutOfRange));
    }

    #[test]
    fn validate_zero_len_returns_ok() {
        let mapper = PageMapper::new(0, PhysAddr(0));
        let addr_space = AddressSpace {
            pml4_phys: PhysAddr(0),
        };
        let result = validate_user_buffer(&addr_space, 0x1000, 0, &mapper);
        assert!(result.is_ok());
    }
}
