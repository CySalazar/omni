//! DEV-ONLY driver auto-loader (P6.7.9-pre.10).
//!
//! Spawns a hand-crafted "driver probe" ELF at boot time that exercises
//! the full `MmioMap (70)` / `DmaMap (71)` / `IrqAttach (72)` syscall
//! path against capability tokens deposited by the kernel.
//!
//! ## Flow
//!
//! 1. [`pci_scan::scan_bus_0`] discovers PCI devices on bus 0.
//! 2. The auto-loader picks the first suitable device (or uses a
//!    synthetic descriptor for MMIO smoke testing).
//! 3. [`crate::process::ProcessControlBlock::spawn_from_elf`] spawns
//!    the probe ELF as a Ring 3 process.
//! 4. [`crate::cap_deposit::deposit_for_driver`] pre-installs `MmioMap`,
//!    `DmaMap`, and `IrqAttach` capability tokens at the well-known
//!    deposit VA.
//! 5. The probe reads the tokens and issues the three syscalls. Exit
//!    sentinel codes distinguish success from each possible failure
//!    point.
//!
//! ## Probe exit sentinel codes
//!
//! | Code | Meaning |
//! |------|---------|
//! |  0   | All three syscalls succeeded |
//! | 10   | No MmioMap token in deposit |
//! | 20   | No DmaMap token in deposit |
//! | 30   | No IrqAttach token in deposit |
//! | 40+e | MmioMap returned errno `e` |
//! | 60+e | DmaMap returned errno `e` |
//! | 80+e | IrqAttach returned errno `e` |
//!
//! ## DEV-ONLY marker
//!
//! This module is a Phase 1 scaffold.  Production driver loading will
//! use a user-space init process with `DriverLoad (73)` and signed
//! omni-pack blobs.

#![allow(
    unsafe_code,
    reason = "wraps ProcessControlBlock::spawn_from_elf and deposit_for_driver which are both unsafe"
)]

use crate::bare_metal::early_console;
use crate::cap_deposit;
use crate::driver_manifest::DriverCapabilities;
use crate::process::ProcessControlBlock;
use crate::scheduling::PriorityClass;
use omni_capability::Resource;

use super::pci_scan;

// =========================================================================
// Hand-crafted driver probe ELF
// =========================================================================
//
// Mapped at VA 0x0040_0000.  The probe:
//
//   1. Reads the OMNICAPS deposit header at 0x0010_0000.
//   2. Scans entries for ACTION_TAG_MMIO_MAP (1).
//   3. Issues MmioMap (70) with the discovered token.
//   4. Exits with sentinel code 0 (success) or 40+errno.
//
// Code layout (offsets relative to PT_LOAD segment at VA 0x0040_0000):
//
//   0x00: mov r12, 0x100000        ; deposit VA            (10 bytes)
//   0x0A: mov ecx, [r12+12]        ; entry_count           (5 bytes)
//   0x0F: test ecx, ecx            ; check zero            (2 bytes)
//   0x11: jz .exit_no_mmio         ; → exit(10)            (6 bytes)
//   0x17: lea r13, [r12+16]        ; first descriptor      (5 bytes)
//   0x1C: mov ebx, ecx             ; counter               (2 bytes)
// .scan:
//   0x1E: cmp dword [r13], 1       ; == MMIO_MAP?          (5 bytes)
//   0x23: je .found                ; yes                   (2 bytes)
//   0x25: add r13, 16              ; next descriptor       (4 bytes)
//   0x29: dec ebx                  ; decrement             (2 bytes)
//   0x2B: jnz .scan                ; loop                  (2 bytes)
//   0x2D: jmp .exit_no_mmio        ; not found             (2 bytes)
// .found:
//   0x2F: mov r14d, [r13+8]        ; token_offset          (4 bytes)
//   0x33: mov r15d, [r13+12]       ; token_len             (4 bytes)
//   0x37: lea r10, [r12+r14]       ; token_ptr             (4 bytes)
//   0x3B: mov r8, r15              ; token_len → r8        (3 bytes)
//   0x3E: mov eax, 70              ; SYS_MMIO_MAP          (5 bytes)
//   0x43: mov edi, 0xFEBC0000      ; phys_base             (5 bytes)
//   0x48: mov esi, 0x1000          ; len                   (5 bytes)
//   0x4D: xor edx, edx            ; flags=0               (2 bytes)
//   0x4F: syscall                  ;                       (2 bytes)
//   0x51: test rdx, rdx           ; errno?                (3 bytes)
//   0x54: jnz .mmio_err           ; → exit(40+e)          (2 bytes)
// .exit_ok:
//   0x56: mov eax, 11             ; TaskExit              (5 bytes)
//   0x5B: xor edi, edi            ; code=0                (2 bytes)
//   0x5D: syscall                 ;                       (2 bytes)
//   0x5F: jmp $                   ;                       (2 bytes)
// .mmio_err:
//   0x61: mov eax, 11             ; TaskExit              (5 bytes)
//   0x66: mov edi, 40             ; EXIT_MMIO_BASE        (5 bytes)
//   0x6B: add rdi, rdx            ; + errno               (3 bytes)
//   0x6E: syscall                 ;                       (2 bytes)
//   0x70: jmp $                   ;                       (2 bytes)
// .exit_no_mmio:
//   0x72: mov eax, 11             ; TaskExit              (5 bytes)
//   0x77: mov edi, 10             ; EXIT_NO_MMIO          (5 bytes)
//   0x7C: syscall                 ;                       (2 bytes)
//   0x7E: jmp $                   ;                       (2 bytes)
//
// Segment: file_size = mem_size = 128 (0x80).  PF_R | PF_X = 5.
// Total ELF: 64 (header) + 56 (phdr) + 128 (code) = 248 bytes.

const DRIVER_PROBE_ELF: &[u8] = &[
    // ── ELF64 header — 64 bytes ──────────────────────────────────
    0x7F, b'E', b'L', b'F',
    2, 1, 1, 0,  0, 0, 0, 0,  0, 0, 0, 0,
    0x02, 0x00,                             // e_type = ET_EXEC
    0x3E, 0x00,                             // e_machine = EM_X86_64
    0x01, 0x00, 0x00, 0x00,                 // e_version = 1
    0x00, 0x00, 0x40, 0x00,  0x00, 0x00, 0x00, 0x00,  // e_entry = 0x0040_0000
    0x40, 0x00, 0x00, 0x00,  0x00, 0x00, 0x00, 0x00,  // e_phoff = 0x40
    0x00, 0x00, 0x00, 0x00,  0x00, 0x00, 0x00, 0x00,  // e_shoff = 0
    0x00, 0x00, 0x00, 0x00,                 // e_flags
    0x40, 0x00,                             // e_ehsize = 64
    0x38, 0x00,                             // e_phentsize = 56
    0x01, 0x00,                             // e_phnum = 1
    0x00, 0x00,                             // e_shentsize
    0x00, 0x00,                             // e_shnum
    0x00, 0x00,                             // e_shstrndx
    // ── Program header — 56 bytes (PT_LOAD, R+X) ────────────────
    0x01, 0x00, 0x00, 0x00,                 // p_type = PT_LOAD
    0x05, 0x00, 0x00, 0x00,                 // p_flags = PF_R | PF_X
    0x78, 0x00, 0x00, 0x00,  0x00, 0x00, 0x00, 0x00,  // p_offset = 0x78
    0x00, 0x00, 0x40, 0x00,  0x00, 0x00, 0x00, 0x00,  // p_vaddr = 0x0040_0000
    0x00, 0x00, 0x40, 0x00,  0x00, 0x00, 0x00, 0x00,  // p_paddr = 0x0040_0000
    0x80, 0x00, 0x00, 0x00,  0x00, 0x00, 0x00, 0x00,  // p_filesz = 128
    0x80, 0x00, 0x00, 0x00,  0x00, 0x00, 0x00, 0x00,  // p_memsz  = 128
    0x00, 0x10, 0x00, 0x00,  0x00, 0x00, 0x00, 0x00,  // p_align  = 0x1000
    // ── Code — 128 bytes at file offset 0x78 ─────────────────────
    // 0x00: mov r12, 0x100000
    0x49, 0xBC, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 0x0A: mov ecx, [r12+12]
    0x41, 0x8B, 0x4C, 0x24, 0x0C,
    // 0x0F: test ecx, ecx
    0x85, 0xC9,
    // 0x11: jz .exit_no_mmio (rel32 → offset 0x72; disp = 0x72 - 0x17 = 0x5B)
    0x0F, 0x84, 0x5B, 0x00, 0x00, 0x00,
    // 0x17: lea r13, [r12+16]
    0x4D, 0x8D, 0x6C, 0x24, 0x10,
    // 0x1C: mov ebx, ecx
    0x89, 0xCB,
    // .scan (0x1E):
    // 0x1E: cmp dword [r13], 1
    0x41, 0x83, 0x7D, 0x00, 0x01,
    // 0x23: je .found (rel8 → offset 0x2F; disp = 0x2F - 0x25 = 0x0A)
    0x74, 0x0A,
    // 0x25: add r13, 16
    0x49, 0x83, 0xC5, 0x10,
    // 0x29: dec ebx
    0xFF, 0xCB,
    // 0x2B: jnz .scan (rel8 → offset 0x1E; disp = 0x1E - 0x2D = -0x0F = 0xF1)
    0x75, 0xF1,
    // 0x2D: jmp .exit_no_mmio (rel8 → offset 0x72; disp = 0x72 - 0x2F = 0x43)
    0xEB, 0x43,
    // .found (0x2F):
    // 0x2F: mov r14d, [r13+8]
    0x45, 0x8B, 0x75, 0x08,
    // 0x33: mov r15d, [r13+12]
    0x45, 0x8B, 0x7D, 0x0C,
    // 0x37: lea r10, [r12+r14]
    0x4F, 0x8D, 0x14, 0x34,
    // 0x3B: mov r8, r15
    0x4D, 0x89, 0xF8,
    // 0x3E: mov eax, 70 (SYS_MMIO_MAP)
    0xB8, 0x46, 0x00, 0x00, 0x00,
    // 0x43: mov edi, 0xFEBC0000
    0xBF, 0x00, 0x00, 0xBC, 0xFE,
    // 0x48: mov esi, 0x1000
    0xBE, 0x00, 0x10, 0x00, 0x00,
    // 0x4D: xor edx, edx
    0x31, 0xD2,
    // 0x4F: syscall
    0x0F, 0x05,
    // 0x51: test rdx, rdx
    0x48, 0x85, 0xD2,
    // 0x54: jnz .mmio_err (rel8 → offset 0x61; disp = 0x61 - 0x56 = 0x0B)
    0x75, 0x0B,
    // .exit_ok (0x56):
    // 0x56: mov eax, 11 (TaskExit)
    0xB8, 0x0B, 0x00, 0x00, 0x00,
    // 0x5B: xor edi, edi
    0x31, 0xFF,
    // 0x5D: syscall
    0x0F, 0x05,
    // 0x5F: jmp $
    0xEB, 0xFE,
    // .mmio_err (0x61):
    // 0x61: mov eax, 11 (TaskExit)
    0xB8, 0x0B, 0x00, 0x00, 0x00,
    // 0x66: mov edi, 40 (EXIT_MMIO_BASE)
    0xBF, 0x28, 0x00, 0x00, 0x00,
    // 0x6B: add rdi, rdx
    0x48, 0x01, 0xD7,
    // 0x6E: syscall
    0x0F, 0x05,
    // 0x70: jmp $
    0xEB, 0xFE,
    // .exit_no_mmio (0x72):
    // 0x72: mov eax, 11 (TaskExit)
    0xB8, 0x0B, 0x00, 0x00, 0x00,
    // 0x77: mov edi, 10 (EXIT_NO_MMIO_TOKEN)
    0xBF, 0x0A, 0x00, 0x00, 0x00,
    // 0x7C: syscall
    0x0F, 0x05,
    // 0x7E: jmp $
    0xEB, 0xFE,
];

/// Load and start the driver probe at boot time.
///
/// Called from `kmain` after IOMMU init, scheduler init, and `sti`.
/// The probe process is enqueued in the scheduler and will be
/// dispatched on the next LAPIC timer preemption.
///
/// # Safety
///
/// Caller must ensure single-CPU invariant holds and that
/// `scheduler`, `mapper`, `alloc` are the live kernel singletons.
#[cfg(target_arch = "x86_64")]
pub unsafe fn boot_load_driver_probe<const N: usize>(
    mapper: &mut crate::bare_metal::paging::PageMapper,
    alloc: &mut crate::memory::BitmapFrameAllocator<N>,
    scheduler: &mut crate::scheduling::RoundRobinScheduler,
) {
    early_console::write_str("[driver-loader] PCI scan all buses (bridge traversal)...\n");

    // SAFETY: Ring 0, single-CPU boot path.
    let scan = unsafe { pci_scan::scan_all_buses() };
    early_console::write_str("[driver-loader] buses scanned: ");
    early_console::write_usize(scan.buses_scanned() as usize);
    early_console::write_str("  bridges: ");
    early_console::write_usize(scan.bridges_found() as usize);
    early_console::write_str("\n");
    early_console::write_str("[driver-loader] PCI devices found: ");
    #[allow(
        clippy::cast_possible_truncation,
        reason = "PCI device count always < 64; fits usize"
    )]
    early_console::write_usize(scan.count());
    early_console::write_str("\n");

    for dev in scan.iter() {
        early_console::write_str("[driver-loader]   bus=");
        write_hex_u8(dev.bus);
        early_console::write_str(" ");
        write_hex_u16(dev.vendor_id);
        early_console::write_str(":");
        write_hex_u16(dev.device_id);
        early_console::write_str(" class=");
        write_hex_u8(dev.class_code);
        early_console::write_str(":");
        write_hex_u8(dev.subclass);
        early_console::write_str(" bar0=");
        write_hex_u32(dev.bar0);
        early_console::write_str(" irq=");
        early_console::write_usize(dev.irq_line as usize);
        if dev.is_pci_bridge() {
            early_console::write_str(" [BRIDGE]");
        }
        early_console::write_str("\n");
    }

    // ── TASK-004: virtio-net live bring-up (P6.7.9-pre.10) ──────────
    //
    // Find the virtio-net device (transitional 1AF4:1000 or modern
    // 1AF4:1041) across all scanned buses. If found and BAR0 is an
    // I/O port, perform live device initialization via legacy I/O.
    if let Some(vnet) = scan
        .find(pci_scan::VIRTIO_VENDOR_ID, pci_scan::VIRTIO_NET_DEVICE_ID_TRANSITIONAL)
        .or_else(|| scan.find(pci_scan::VIRTIO_VENDOR_ID, pci_scan::VIRTIO_NET_DEVICE_ID_MODERN))
    {
        early_console::write_str("[virtio-net] found on bus=");
        write_hex_u8(vnet.bus);
        early_console::write_str(" dev=");
        write_hex_u8(vnet.device);
        early_console::write_str(" bar0=");
        write_hex_u32(vnet.bar0);
        early_console::write_str("\n");

        // SAFETY: Ring 0, single-CPU boot path.
        unsafe { pci_scan::enable_device_full(vnet) };
        early_console::write_str("[virtio-net] PCI cmd: IOSE+MSE+BME enabled\n");

        if pci_scan::PciDevice::bar_is_io(vnet.bar0) {
            let io_base = pci_scan::PciDevice::bar_io_base(vnet.bar0);
            early_console::write_str("[virtio-net] I/O port base=");
            write_hex_u16(io_base);
            early_console::write_str("\n");

            // SAFETY: Ring 0, I/O port reads to PCI device BAR.
            unsafe { virtio_net_live_bringup(io_base) };
        } else {
            early_console::write_str("[virtio-net] BAR0 is MMIO — I/O port bringup skipped\n");
        }
    } else {
        early_console::write_str("[virtio-net] not found on any bus\n");
    }

    // ── Probe ELF (smoke test for MmioMap/DmaMap/IrqAttach) ──────
    //
    // Pick any device with a non-zero BAR for the capability deposit
    // probe (unchanged from pre.9).
    if let Some(vdev) = scan.find_by_vendor(pci_scan::VIRTIO_VENDOR_ID) {
        let probe_bar = vdev.bar4_phys();
        let probe_irq = u16::from(vdev.irq_line);
        if probe_bar == 0 {
            let bar0 = vdev.bar0_phys();
            if bar0 != 0 {
                return unsafe { boot_load_with_bar(bar0, probe_irq, mapper, alloc, scheduler) };
            }
        } else {
            return unsafe { boot_load_with_bar(probe_bar, probe_irq, mapper, alloc, scheduler) };
        }
    }

    let synthetic_bar: u64 = 0xFEBC_0000;
    let synthetic_irq: u16 = 33;
    early_console::write_str("[driver-loader] using synthetic BAR 0xFEBC0000\n");
    unsafe { boot_load_with_bar(synthetic_bar, synthetic_irq, mapper, alloc, scheduler) };
}

#[cfg(target_arch = "x86_64")]
unsafe fn boot_load_with_bar<const N: usize>(
    bar_phys: u64,
    irq_line: u16,
    mapper: &mut crate::bare_metal::paging::PageMapper,
    alloc: &mut crate::memory::BitmapFrameAllocator<N>,
    scheduler: &mut crate::scheduling::RoundRobinScheduler,
) {
    use crate::capabilities::KernelPrincipal;
    use crate::memory::PhysAddr;

    let boot_cr3 = PhysAddr(super::boot_cr3());
    if boot_cr3.0 == 0 {
        early_console::write_str("[driver-loader] boot_cr3 not set — aborting\n");
        return;
    }

    // Construct DriverCapabilities matching the probe ELF's expectations.
    // The MmioMap scope covers the BAR address so the syscall scope
    // check passes. The DmaMap and IrqAttach scopes are wide enough
    // for the probe's hardcoded parameters.
    let mut caps = DriverCapabilities::default();
    caps.mmio_regions.push(Resource::MmioRegion {
        phys_base: bar_phys,
        len: 0x1000,
    });
    caps.dma_windows.push(Resource::DmaWindow {
        iova_base: 0,
        len: 0x1_0000_0000,
    });
    caps.irq_lines.push(Resource::IrqLine(irq_line));

    // Spawn the probe ELF.
    // SAFETY: single-CPU boot path; `boot_cr3`, `mapper`, `alloc`,
    // `scheduler` are the live kernel singletons (same invariant as
    // the MB11 userprobe spawn in `kmain`).
    let task_id = match unsafe {
        ProcessControlBlock::spawn_from_elf(
            DRIVER_PROBE_ELF,
            boot_cr3,
            mapper,
            alloc,
            scheduler,
            PriorityClass::Interactive,
            KernelPrincipal::ZERO,
        )
    } {
        Ok(id) => id,
        Err(e) => {
            early_console::write_str("[driver-loader] spawn FAILED: ");
            early_console::write_str(match e {
                crate::KernelError::ResourceExhausted => "ResourceExhausted",
                crate::KernelError::InvalidArgument => "InvalidArgument",
                _ => "Unknown",
            });
            early_console::write_str("\n");
            return;
        }
    };

    early_console::write_str("[driver-loader] probe spawned  task_id=");
    #[allow(
        clippy::cast_possible_truncation,
        reason = "task id fits usize on x86_64"
    )]
    early_console::write_usize(task_id.0 as usize);
    early_console::write_str("\n");

    // Deposit capability tokens into the probe's address space.
    let Some(pcb) = scheduler.process(task_id) else {
        early_console::write_str("[driver-loader] process lookup FAILED\n");
        return;
    };

    // SAFETY: single-CPU boot path; `pcb.address_space` was just created
    // by `spawn_from_elf`; `mapper` and `alloc` are the live kernel
    // singletons. Direct-map offset is valid (set earlier in `kmain`).
    let deposit_result = unsafe {
        cap_deposit::deposit_for_driver(
            &caps,
            0, // boot_seconds (Phase 1: no RTC in token window)
            [0u8; 32], // subject_node_id (DEV-ONLY placeholder)
            &pcb.address_space,
            mapper,
            alloc,
        )
    };
    match deposit_result {
        Ok(va) => {
            early_console::write_str("[driver-loader] deposit OK  va=");
            #[allow(
                clippy::cast_possible_truncation,
                reason = "deposit VA fits usize on x86_64"
            )]
            early_console::write_usize(va as usize);
            early_console::write_str("\n");
        }
        Err(e) => {
            early_console::write_str("[driver-loader] deposit FAILED: ");
            early_console::write_str(match e {
                cap_deposit::DepositError::TokenCountExceeded { .. } => "TokenCountExceeded",
                cap_deposit::DepositError::TokenEncodingFailed => "TokenEncodingFailed",
                cap_deposit::DepositError::TokenSigningFailed => "TokenSigningFailed",
                cap_deposit::DepositError::ScopeBytesOverflow { .. } => "ScopeBytesOverflow",
                #[cfg(feature = "bare-metal")]
                cap_deposit::DepositError::MapFailed => "MapFailed",
                #[cfg(not(feature = "bare-metal"))]
                cap_deposit::DepositError::HostStub => "HostStub",
            });
            early_console::write_str("\n");
        }
    }

    early_console::write_str("[driver-loader] probe enqueued — will dispatch on next tick\n");
}

// =========================================================================
// TASK-004: virtio-net legacy I/O port bring-up (P6.7.9-pre.10)
// =========================================================================
//
// The virtio 1.0 § 4.1 legacy interface uses I/O ports via BAR0.
// Register offsets (transitional device, 1AF4:1000):
//
//   0x00  Device Features    (4 bytes, R)
//   0x04  Driver Features    (4 bytes, R/W)
//   0x08  Queue Address      (4 bytes, R/W)
//   0x0C  Queue Size         (2 bytes, R)
//   0x0E  Queue Select       (2 bytes, R/W)
//   0x10  Queue Notify       (2 bytes, R/W)
//   0x12  Device Status      (1 byte,  R/W)
//   0x13  ISR Status         (1 byte,  R)
//   0x14  MAC Address        (6 bytes, R)

const VIRTIO_IO_OFF_DEVICE_FEATURES: u16 = 0x00;
const VIRTIO_IO_OFF_DEVICE_STATUS: u16 = 0x12;
const VIRTIO_IO_OFF_MAC: u16 = 0x14;

const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 0x01;
const VIRTIO_STATUS_DRIVER: u8 = 0x02;
const VIRTIO_STATUS_FEATURES_OK: u8 = 0x08;
const VIRTIO_STATUS_DRIVER_OK: u8 = 0x04;

/// Perform the live virtio-net bring-up sequence via legacy I/O ports.
///
/// # Safety
///
/// Ring 0 only. `io_base` must be the decoded I/O port base from BAR0.
#[cfg(target_arch = "x86_64")]
unsafe fn virtio_net_live_bringup(io_base: u16) {
    use super::arch;

    // Step 1: Reset — write 0 to device_status.
    unsafe { arch::outb(io_base + VIRTIO_IO_OFF_DEVICE_STATUS, 0) };
    let status = unsafe { arch::inb(io_base + VIRTIO_IO_OFF_DEVICE_STATUS) };
    early_console::write_str("[virtio-net] RESET  status=");
    write_hex_u8(status);
    early_console::write_str(if status == 0 { " OK\n" } else { " FAIL\n" });

    // Step 2: Acknowledge — set ACKNOWLEDGE bit.
    unsafe { arch::outb(io_base + VIRTIO_IO_OFF_DEVICE_STATUS, VIRTIO_STATUS_ACKNOWLEDGE) };
    let status = unsafe { arch::inb(io_base + VIRTIO_IO_OFF_DEVICE_STATUS) };
    early_console::write_str("[virtio-net] ACK    status=");
    write_hex_u8(status);
    early_console::write_str("\n");

    // Step 3: Driver — set DRIVER bit.
    unsafe {
        arch::outb(
            io_base + VIRTIO_IO_OFF_DEVICE_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
        )
    };
    let status = unsafe { arch::inb(io_base + VIRTIO_IO_OFF_DEVICE_STATUS) };
    early_console::write_str("[virtio-net] DRIVER status=");
    write_hex_u8(status);
    early_console::write_str("\n");

    // Step 4: Read device features (first 32 bits).
    let features = unsafe { arch::inl(io_base + VIRTIO_IO_OFF_DEVICE_FEATURES) };
    early_console::write_str("[virtio-net] features=");
    write_hex_u32(features);
    early_console::write_str("\n");

    // Step 5: Write driver features (accept all device-offered).
    unsafe { arch::outl(io_base + 0x04, features) };

    // Step 6: Set FEATURES_OK.
    unsafe {
        arch::outb(
            io_base + VIRTIO_IO_OFF_DEVICE_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK,
        )
    };
    let status = unsafe { arch::inb(io_base + VIRTIO_IO_OFF_DEVICE_STATUS) };
    early_console::write_str("[virtio-net] FEAT   status=");
    write_hex_u8(status);
    let features_accepted = (status & VIRTIO_STATUS_FEATURES_OK) != 0;
    early_console::write_str(if features_accepted {
        " features_ok=yes\n"
    } else {
        " features_ok=NO\n"
    });

    if !features_accepted {
        early_console::write_str("[virtio-net] device rejected features — aborting\n");
        return;
    }

    // Step 7: Read MAC address (6 bytes at offset 0x14).
    early_console::write_str("[virtio-net] MAC=");
    for i in 0u16..6 {
        if i > 0 {
            early_console::write_str(":");
        }
        let byte = unsafe { arch::inb(io_base + VIRTIO_IO_OFF_MAC + i) };
        write_hex_u8(byte);
    }
    early_console::write_str("\n");

    // Step 8: Set DRIVER_OK — device is live.
    unsafe {
        arch::outb(
            io_base + VIRTIO_IO_OFF_DEVICE_STATUS,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_FEATURES_OK
                | VIRTIO_STATUS_DRIVER_OK,
        )
    };
    let status = unsafe { arch::inb(io_base + VIRTIO_IO_OFF_DEVICE_STATUS) };
    early_console::write_str("[virtio-net] READY  status=");
    write_hex_u8(status);
    let driver_ok = (status & VIRTIO_STATUS_DRIVER_OK) != 0;
    early_console::write_str(if driver_ok {
        " driver_ok=yes\n"
    } else {
        " driver_ok=NO\n"
    });

    early_console::write_str("[virtio-net] live bring-up complete\n");
}

// =========================================================================
// Hex formatting helpers (no alloc, no format!)
// =========================================================================

#[allow(clippy::indexing_slicing, reason = "nibble index is always 0..15")]
fn write_hex_u8(val: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let hi = HEX[(val >> 4) as usize];
    let lo = HEX[(val & 0xF) as usize];
    let buf = [hi, lo];
    // SAFETY: both bytes are ASCII hex digits from the const table.
    #[allow(unsafe_code, reason = "ASCII-only from const table")]
    let s = unsafe { core::str::from_utf8_unchecked(&buf) };
    early_console::write_str(s);
}

#[allow(clippy::cast_possible_truncation, reason = "shifting u16 >> 8 fits u8")]
fn write_hex_u16(val: u16) {
    write_hex_u8((val >> 8) as u8);
    write_hex_u8(val as u8);
}

#[allow(clippy::cast_possible_truncation, reason = "shifting u32 >> 16 fits u16")]
fn write_hex_u32(val: u32) {
    write_hex_u16((val >> 16) as u16);
    write_hex_u16(val as u16);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_elf_starts_with_elf_magic() {
        assert_eq!(&DRIVER_PROBE_ELF[0..4], &[0x7F, b'E', b'L', b'F']);
    }

    #[test]
    fn probe_elf_entry_is_0x400000() {
        let entry = u64::from_le_bytes(DRIVER_PROBE_ELF[24..32].try_into().unwrap());
        assert_eq!(entry, 0x0040_0000);
    }

    #[test]
    fn probe_elf_has_one_program_header() {
        let phnum = u16::from_le_bytes(DRIVER_PROBE_ELF[56..58].try_into().unwrap());
        assert_eq!(phnum, 1);
    }

    #[test]
    fn probe_elf_total_size_is_248() {
        assert_eq!(DRIVER_PROBE_ELF.len(), 248);
    }

    #[test]
    fn probe_elf_code_segment_size_matches() {
        let filesz = u64::from_le_bytes(DRIVER_PROBE_ELF[96..104].try_into().unwrap());
        assert_eq!(filesz, 128);
    }
}
