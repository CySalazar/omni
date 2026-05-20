//! # `omni-driver-e1000e`
//!
//! OMNI OS first-party Intel e1000e user-space driver — P6.7.8.6 scaffold.
//!
//! ## Scope
//!
//! This crate implements the **M2 driver** of [`OIP-Driver-Net-015`] § S5:
//! Intel e1000e family Gigabit Ethernet over PCIe (PCI vendor `0x8086`,
//! representative devices `0x10D3` 82574L, `0x153A` I217-LM, `0x153B`
//! I217-V, `0x15A1` I218-LM, `0x15A3` I219-LM, plus close relatives). The
//! driver runs as a Ring 3 user-space process spawned by the kernel
//! through the [`OIP-Driver-Framework-013`] § S5 `DriverLoad` syscall flow,
//! holds capability tokens for `MmioMap` / `DmaMap` / `IrqAttach`
//! attenuated from its issuer, and exposes a `omni.svc.net.eth<N>` IPC
//! channel per § S2 of OIP-Driver-Net-015.
//!
//! ## Delivery layering
//!
//! P6.7.8 is split into atomic sub-tasks. This crate covers **P6.7.8.6 —
//! e1000e driver scaffold** only:
//!
//! - [`pci_ids`] — Intel vendor + per-device PCIe matchers pinned by
//!   OIP-015 § S5 (`pci_vendor_device` entries in the manifest template).
//! - [`controller_regs`] — CSR register offsets from the Intel 82574L
//!   datasheet § 10 ("Programming Interface", base address BAR0).
//! - [`ring_config`] — RX/TX descriptor ring depth bounds, descriptor
//!   entry sizes, and RX buffer-pool defaults per OIP-015 § S1.
//! - [`interrupts`] — `IMS` / `IMC` / `ICR` bit positions for the three
//!   interrupt sources the v0.3 driver enables (`RXT0`, `TXDW`, `LSC`)
//!   per OIP-015 § S5.1 step 10.
//! - [`bringup`] — 13-step bring-up state-machine driver
//!   (`PciEnumeration → MmioMap → DisableInterrupts → GlobalReset →
//!   ReadMac → PhyInit → SetupRxRing → PostRxBuffers → SetupTxRing →
//!   ConfigureRxTx → EnableInterrupts → AttachIrq → RegisterNetChannel
//!   → Ready`) per OIP-015 § S5.1 + § S8. No syscall calls — the actual
//!   `MmioMap` / `DmaMap` / `IrqAttach` invocations live in the bootable
//!   image sibling `omni-driver-e1000e-image` (P6.7.8.7).
//!
//! The bootable image sibling mirrors the `omni-kernel` ↔ `kernel-runner`,
//! `omni-driver-net-virtio` ↔ `omni-driver-net-virtio-image`, and
//! `omni-driver-nvme` ↔ `omni-driver-nvme-image` splits that already
//! power the bare-metal boot path.
//!
//! ## Cross-references
//!
//! - Driver framework: [`oips/oip-driver-framework-013.md`](../../../oips/oip-driver-framework-013.md)
//! - Net driver family: [`oips/oip-driver-net-015.md`](../../../oips/oip-driver-net-015.md)
//! - Developer-authored manifest TOML template:
//!   `crates/omni-driver-e1000e/manifest.toml` (consumed offline by the
//!   `omni-driver-pack` build tool — OMNI Forge — to produce the
//!   `omni-pack v1` binary blob that `DriverLoad` ingests per
//!   OIP-013 § S5.5).
//!
//! [`OIP-Driver-Net-015`]: ../../../oips/oip-driver-net-015.md
//! [`OIP-Driver-Framework-013`]: ../../../oips/oip-driver-framework-013.md

#![doc(html_root_url = "https://docs.omni-os.org/omni-driver-e1000e")]
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
// Test-only allow list — mirrors `omni-kernel`'s ADR-0003 carve-out and
// the precedent set by `omni-driver-net-virtio` (P6.7.8.2) +
// `omni-driver-nvme` (P6.7.8.4). The bring-up FSM tests use `.unwrap()` /
// `.expect()` for terseness; production code keeps the workspace
// `deny(unwrap_used, expect_used, panic)` invariants at "deny".
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

pub mod bringup;
pub mod controller_regs;
pub mod interrupts;
pub mod pci_ids;
pub mod ring_config;
