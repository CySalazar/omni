//! Bring-up state machine — **enum-only scaffold** for P6.7.8.2.
//!
//! `OIP-Driver-Net-015` § S4.1 prescribes a nine-step bring-up sequence
//! that the driver MUST execute after `DriverLoad` spawns its process and
//! the kernel mints the per-driver capability tokens. The phases below
//! mirror the state-machine view of those nine steps:
//!
//! 1. [`Phase::Reset`] — write [`device_status::RESET`] and poll back to
//!    `0x00` (§ S4.1 step 2).
//! 2. [`Phase::Acknowledge`] — write `ACKNOWLEDGE` then `ACKNOWLEDGE |
//!    DRIVER` (§ S4.1 step 3).
//! 3. [`Phase::FeatureNegotiation`] — read `device_feature`, AND with
//!    [`features::REQUIRED_FEATURES`] plus opted-in extras, write
//!    `driver_feature` and `FEATURES_OK` (§ S4.1 step 4).
//! 4. [`Phase::FeaturesLocked`] — re-read `device_status`; if
//!    `FEATURES_OK` is still set, advance. Otherwise transition to
//!    [`Phase::Failed`] (§ S4.1 step 5).
//! 5. [`Phase::VirtqueueSetup`] — `DmaMap` RX / TX virtqueues, program
//!    Common Cfg `queue_*` 64-bit addresses (§ S4.1 step 6).
//! 6. [`Phase::MacAcquired`] — read MAC from the Device Cfg, store
//!    locally for the eventual `NetEvent::MacChanged` emission
//!    (§ S4.1 step 7, OIP-015 § S2.3 + § S3).
//! 7. [`Phase::DriverOk`] — set `DRIVER_OK`; the device is live. Post RX
//!    buffers, `IrqAttach` virtqueue IRQ vectors, register the NET
//!    service channel (§ S4.1 steps 8-9, OIP-015 § S8 common-bringup
//!    finalizer).
//! 8. [`Phase::Failed`] — terminal error state; the driver process exits.
//!
//! Transition tables and the actual `MmioMap` / `DmaMap` / `IrqAttach`
//! syscall invocations land in **P6.7.8.3**. This file deliberately
//! exposes ONLY the phase enum so the scaffold acceptance criterion
//! ("skeleton-only, no syscall integration") stays auditable.
//!
//! [`device_status::RESET`]: crate::device_status::RESET
//! [`features::REQUIRED_FEATURES`]: crate::features::REQUIRED_FEATURES

/// Ordered phases of the virtio-net bring-up state machine.
///
/// The driver advances strictly in `repr(u8)` order. Skipping a phase is
/// a kernel-detectable violation of `OIP-Driver-Net-015` § S4.1; the
/// monotonicity is verified by the scaffold unit tests below and will be
/// re-verified at runtime in P6.7.8.3 by the actual state-machine driver
/// loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Phase {
    /// § S4.1 step 2 — write `device_status::RESET`, await device clear.
    Reset = 0,
    /// § S4.1 step 3 — driver acknowledges the device.
    Acknowledge = 1,
    /// § S4.1 step 4 — feature OR/AND/write loop.
    FeatureNegotiation = 2,
    /// § S4.1 step 5 — re-read `device_status`; confirm `FEATURES_OK`.
    FeaturesLocked = 3,
    /// § S4.1 step 6 — allocate + program RX/TX virtqueues.
    VirtqueueSetup = 4,
    /// § S4.1 step 7 — read negotiated MAC from Device Cfg offset 0.
    MacAcquired = 5,
    /// § S4.1 step 8 — set `DRIVER_OK`; device is live, IRQs attached,
    /// NET service channel registered.
    DriverOk = 6,
    /// Terminal failure phase — emitted on `FEATURES_OK` not retained
    /// (§ S4.1 step 5) or on any subsequent unrecoverable error.
    Failed = 7,
}

impl Phase {
    /// Return the next phase in the bring-up sequence, or `None` if the
    /// driver is already at the terminal state ([`Phase::DriverOk`] or
    /// [`Phase::Failed`]).
    ///
    /// Pure function: no side effects, no syscalls. Used by the eventual
    /// state-machine loop (P6.7.8.3) to drive forward progress; exposed
    /// already so the scaffold's monotonicity tests can validate the
    /// ordering once, here, instead of duplicating the table in the
    /// bring-up driver.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Reset => Some(Self::Acknowledge),
            Self::Acknowledge => Some(Self::FeatureNegotiation),
            Self::FeatureNegotiation => Some(Self::FeaturesLocked),
            Self::FeaturesLocked => Some(Self::VirtqueueSetup),
            Self::VirtqueueSetup => Some(Self::MacAcquired),
            Self::MacAcquired => Some(Self::DriverOk),
            Self::DriverOk | Self::Failed => None,
        }
    }

    /// Returns `true` if the phase is terminal (no further transitions).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::DriverOk | Self::Failed)
    }

    /// Returns `true` if the phase represents a successful steady-state
    /// (the device is live and the driver is serving the NET channel).
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::DriverOk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_are_monotonic() {
        // Spec-anchored ordering: each `next()` MUST emit a strictly
        // greater discriminant than its predecessor (until terminal).
        let mut cur = Phase::Reset;
        while let Some(next) = cur.next() {
            assert!(
                (next as u8) > (cur as u8),
                "Phase::next produced a non-monotonic transition: {cur:?} -> {next:?}"
            );
            cur = next;
        }
        assert_eq!(cur, Phase::DriverOk);
    }

    #[test]
    fn driver_ok_is_terminal_and_live() {
        assert!(Phase::DriverOk.is_terminal());
        assert!(Phase::DriverOk.is_live());
        assert!(Phase::DriverOk.next().is_none());
    }

    #[test]
    fn failed_is_terminal_but_not_live() {
        assert!(Phase::Failed.is_terminal());
        assert!(!Phase::Failed.is_live());
        assert!(Phase::Failed.next().is_none());
    }

    #[test]
    fn pre_terminal_phases_are_neither_terminal_nor_live() {
        for phase in [
            Phase::Reset,
            Phase::Acknowledge,
            Phase::FeatureNegotiation,
            Phase::FeaturesLocked,
            Phase::VirtqueueSetup,
            Phase::MacAcquired,
        ] {
            assert!(
                !phase.is_terminal(),
                "phase {phase:?} unexpectedly terminal"
            );
            assert!(!phase.is_live(), "phase {phase:?} unexpectedly live");
            assert!(phase.next().is_some(), "phase {phase:?} should advance");
        }
    }

    #[test]
    fn discriminants_match_oip_step_ordering() {
        // OIP-015 § S4.1 lists the steps in this exact order; the
        // discriminants encode the step index (Reset is step 2 → 0,
        // DriverOk is step 8 → 6). Anchoring it here means a future
        // re-ordering would surface as a failing test instead of
        // silently breaking the bring-up loop.
        assert_eq!(Phase::Reset as u8, 0);
        assert_eq!(Phase::Acknowledge as u8, 1);
        assert_eq!(Phase::FeatureNegotiation as u8, 2);
        assert_eq!(Phase::FeaturesLocked as u8, 3);
        assert_eq!(Phase::VirtqueueSetup as u8, 4);
        assert_eq!(Phase::MacAcquired as u8, 5);
        assert_eq!(Phase::DriverOk as u8, 6);
        assert_eq!(Phase::Failed as u8, 7);
    }
}
