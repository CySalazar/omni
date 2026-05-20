//! # `omni-driver-net-virtio`
//!
//! OMNI OS first-party virtio-net user-space driver — P6.7.8.2 scaffold.
//!
//! ## Scope
//!
//! This crate implements the M1 driver of [`OIP-Driver-Net-015`] § S4:
//! virtio-net over PCI (vendor `0x1AF4`, device `0x1041` modern / `0x1000`
//! legacy). The driver runs as a Ring 3 user-space process spawned by the
//! kernel through the [`OIP-Driver-Framework-013`] § S5 `DriverLoad` syscall
//! flow, holds capability tokens for `MmioMap` / `DmaMap` / `IrqAttach`
//! attenuated from its issuer, and exposes a `omni.svc.net.<ifN>` IPC
//! channel per § S2 of OIP-Driver-Net-015.
//!
//! ## Delivery layering
//!
//! P6.7.8 is split into atomic sub-tasks. This crate covers **P6.7.8.2 —
//! virtio-net crate scaffold** only:
//!
//! - [`pci_ids`] — PCI vendor/device matchers pinned by OIP-015 § S4.
//! - [`device_status`] — `device_status` byte constants from virtio 1.0 § 2.1.
//! - [`features`] — `device_feature` / `driver_feature` bit positions for
//!   the v0.3 negotiated feature set (`VIRTIO_F_VERSION_1`,
//!   `VIRTIO_NET_F_MAC`, `VIRTIO_NET_F_STATUS`).
//! - [`virtqueue`] — virtqueue descriptor / avail-ring / used-ring layout
//!   constants (no allocators, no syscall calls).
//! - [`bringup`] — state-machine **enum-only** scaffold for the
//!   `Reset → Acknowledge → Driver → FeaturesOk → DriverOk` sequence
//!   described by OIP-015 § S4.1. No state transitions are wired here;
//!   the actual driver loop lands in P6.7.8.3.
//!
//! The bootable image sibling that links this lib into a `no_std` +
//! `no_main` ELF (loaded by the kernel's `spawn_from_elf` per OIP-013
//! § S5.3 step 9) lands as `crates/omni-driver-net-virtio-image/` in
//! P6.7.8.3, mirroring the `omni-kernel` ↔ `kernel-runner` split that
//! already powers the bare-metal boot path.
//!
//! ## Cross-references
//!
//! - Driver framework: [`docs/oips/oip-driver-framework-013.md`](../../../oips/oip-driver-framework-013.md)
//! - Net driver family: [`docs/oips/oip-driver-net-015.md`](../../../oips/oip-driver-net-015.md)
//! - Developer-authored manifest TOML template:
//!   `crates/omni-driver-net-virtio/manifest.toml` (consumed offline by
//!   the `omni-driver-pack` build tool — OMNI Forge — to produce the
//!   `omni-pack v1` binary blob that `DriverLoad` ingests per
//!   OIP-013 § S5.5).
//!
//! [`OIP-Driver-Net-015`]: ../../../oips/oip-driver-net-015.md
//! [`OIP-Driver-Framework-013`]: ../../../oips/oip-driver-framework-013.md

#![doc(html_root_url = "https://docs.omni-os.org/omni-driver-net-virtio")]
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
// Test-only allow list — mirrors `omni-kernel`'s ADR-0003 carve-out. The
// driver bring-up FSM tests use `.unwrap()` / `.expect()` for terseness;
// production code keeps the workspace `deny(unwrap_used, expect_used,
// panic)` invariants at "deny".
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
pub mod device_status;
pub mod features;
pub mod pci_ids;
pub mod virtqueue;
