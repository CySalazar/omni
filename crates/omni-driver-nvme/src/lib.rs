//! # `omni-driver-nvme`
//!
//! OMNI OS first-party NVMe user-space driver — P6.7.8.4 scaffold.
//!
//! ## Scope
//!
//! This crate implements the storage driver of [`OIP-Driver-NVMe-014`]
//! § S1-S6: NVMe 1.4-compliant PCIe SSDs (PCI class `0x01:0x08:0x02`). The
//! driver runs as a Ring 3 user-space process spawned by the kernel
//! through the [`OIP-Driver-Framework-013`] § S5 `DriverLoad` syscall flow,
//! holds capability tokens for `MmioMap` / `DmaMap` / `IrqAttach`
//! attenuated from its issuer, and exposes the BLK service channel
//! `omni.svc.blk.nvme0` per § S4 of OIP-Driver-NVMe-014.
//!
//! ## Delivery layering
//!
//! P6.7.8 is split into atomic sub-tasks. This crate covers **P6.7.8.4 —
//! NVMe driver scaffold** only:
//!
//! - [`pci_ids`] — PCI class code matchers pinned by OIP-014 § S1.
//! - [`controller_regs`] — NVMe Controller Register offsets from
//!   NVMe 1.4 base spec § 3.1.
//! - [`queue_config`] — admin + IO submission/completion queue depth
//!   bounds and queue entry sizes per NVMe 1.4 § 5.
//! - [`transfer_model`] — PRP-only [`TransferModel`](crate::transfer_model::TransferModel)
//!   enum + 4 KiB alignment helpers (PRP is the only model accepted in
//!   v0.3 per OIP-014 § M4).
//! - [`bringup`] — 13-step bring-up state-machine driver
//!   (`PciEnumeration → MmioMap → ReadCap → DisableController → SetupAdminQueues
//!   → EnableController → AttachInterrupts → IdentifyController → IdentifyActiveNsList
//!   → IdentifyNamespace → CreateIoQueues → RegisterBlkChannel → Ready`)
//!   per OIP-014 § S6. No syscall calls — the actual `MmioMap` /
//!   `DmaMap` / `IrqAttach` invocations live in the bootable image
//!   sibling `omni-driver-nvme-image` (P6.7.8.5).
//!
//! The bootable image sibling mirrors the `omni-kernel` ↔ `kernel-runner`
//! and `omni-driver-net-virtio` ↔ `omni-driver-net-virtio-image` split
//! that already powers the bare-metal boot path.
//!
//! ## Cross-references
//!
//! - Driver framework: [`oips/oip-driver-framework-013.md`](../../../oips/oip-driver-framework-013.md)
//! - NVMe driver: [`oips/oip-driver-nvme-014.md`](../../../oips/oip-driver-nvme-014.md)
//! - Developer-authored manifest TOML template:
//!   `crates/omni-driver-nvme/manifest.toml` (consumed offline by the
//!   `omni-driver-pack` build tool — OMNI Forge — to produce the
//!   `omni-pack v1` binary blob that `DriverLoad` ingests per
//!   OIP-013 § S5.5).
//!
//! [`OIP-Driver-NVMe-014`]: ../../../oips/oip-driver-nvme-014.md
//! [`OIP-Driver-Framework-013`]: ../../../oips/oip-driver-framework-013.md

#![doc(html_root_url = "https://docs.omni-os.org/omni-driver-nvme")]
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
// Test-only allow list — mirrors `omni-kernel`'s ADR-0003 carve-out and
// the precedent set by `omni-driver-net-virtio` (P6.7.8.2). The bring-up
// FSM tests use `.unwrap()` / `.expect()` for terseness; production code
// keeps the workspace `deny(unwrap_used, expect_used, panic)` invariants
// at "deny".
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::doc_markdown
    )
)]

extern crate alloc;

pub mod admin;
pub mod bringup;
pub mod controller_regs;
pub mod io;
pub mod pci_ids;
pub mod queue;
pub mod queue_config;
pub mod ring;
pub mod transfer_model;
