//! PCI Type 1 configuration space bus scanner (P6.7.9-pre.8).
//!
//! Discovers PCI devices on bus 0 via the legacy CF8/CFC I/O port
//! mechanism.  Used by the DEV-ONLY driver auto-loader to find devices
//! before issuing `DriverLoad`.
//!
//! ## Scope
//!
//! Phase 1 scans a single bus (bus 0) with 32 devices × 8 functions.
//! Multi-bus / PCI-to-PCI bridge traversal is deferred to Phase 2.

#![allow(unsafe_code, reason = "PCI config-space reads via I/O ports")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::doc_markdown,
    clippy::indexing_slicing,
    reason = "PCI register fields are well-defined widths; VirtIO/NVMe are spec names; \
              scanner array indexing is bounded by MAX_DISCOVERED"
)]

use super::arch;

/// Maximum number of devices the scanner will discover.
const MAX_DISCOVERED: usize = 32;

/// A discovered PCI device descriptor.
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    /// PCI bus number (always 0 in Phase 1).
    pub bus: u8,
    /// PCI device number (0..31).
    pub device: u8,
    /// PCI function number (0..7).
    pub function: u8,
    /// Vendor ID from config register 0x00[15:0].
    pub vendor_id: u16,
    /// Device ID from config register 0x00[31:16].
    pub device_id: u16,
    /// Class code from config register 0x08[31:24].
    pub class_code: u8,
    /// Sub-class code from config register 0x08[23:16].
    pub subclass: u8,
    /// BAR0 raw value from config register 0x10.
    pub bar0: u32,
    /// BAR1 raw value from config register 0x14.
    pub bar1: u32,
    /// BAR4 raw value from config register 0x20.
    pub bar4: u32,
    /// BAR5 raw value from config register 0x24.
    pub bar5: u32,
    /// Interrupt line from config register 0x3C[7:0].
    pub irq_line: u8,
}

impl PciDevice {
    /// Decode a 32-bit BAR as a memory-mapped base address (mask low 4 bits).
    #[must_use]
    pub const fn bar_mmio_base(bar_raw: u32) -> u64 {
        (bar_raw & 0xFFFF_FFF0) as u64
    }

    /// Check if BAR is 64-bit (bit 2:1 of the BAR value == 0b10).
    #[must_use]
    pub const fn bar_is_64bit(bar_raw: u32) -> bool {
        (bar_raw & 0x06) == 0x04
    }

    /// Reconstruct a 64-bit BAR address from two consecutive 32-bit BARs.
    #[must_use]
    pub const fn bar64(low: u32, high: u32) -> u64 {
        ((high as u64) << 32) | ((low & 0xFFFF_FFF0) as u64)
    }

    /// Return the 64-bit physical base of BAR0, handling 32/64-bit BARs.
    #[must_use]
    pub const fn bar0_phys(&self) -> u64 {
        if Self::bar_is_64bit(self.bar0) {
            Self::bar64(self.bar0, self.bar1)
        } else {
            Self::bar_mmio_base(self.bar0)
        }
    }

    /// Return the 64-bit physical base of BAR4, handling 32/64-bit BARs.
    #[must_use]
    pub const fn bar4_phys(&self) -> u64 {
        if Self::bar_is_64bit(self.bar4) {
            Self::bar64(self.bar4, self.bar5)
        } else {
            Self::bar_mmio_base(self.bar4)
        }
    }
}

/// Result of a PCI bus scan.
pub struct ScanResult {
    devices: [Option<PciDevice>; MAX_DISCOVERED],
    count: usize,
}

impl ScanResult {
    /// Number of devices discovered.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Find the first device matching the given vendor and device ID.
    #[must_use]
    pub fn find(&self, vendor_id: u16, device_id: u16) -> Option<&PciDevice> {
        self.iter()
            .find(|d| d.vendor_id == vendor_id && d.device_id == device_id)
    }

    /// Find the first device matching the given vendor ID (any device ID).
    #[must_use]
    pub fn find_by_vendor(&self, vendor_id: u16) -> Option<&PciDevice> {
        self.iter().find(|d| d.vendor_id == vendor_id)
    }

    /// Iterator over discovered devices.
    pub fn iter(&self) -> impl Iterator<Item = &PciDevice> {
        self.devices.get(..self.count)
            .unwrap_or(&[])
            .iter()
            .flatten()
    }
}

/// Scan PCI bus 0 for all present devices.
///
/// Reads vendor/device ID at each (bus=0, device=0..31, function=0..7)
/// slot via the CF8/CFC mechanism.  Non-existent slots return
/// vendor_id `0xFFFF` and are skipped.
///
/// # Safety
///
/// Must be called from Ring 0.  PCI config reads via I/O ports are
/// side-effect-free.
pub unsafe fn scan_bus_0() -> ScanResult {
    let mut result = ScanResult {
        devices: [None; MAX_DISCOVERED],
        count: 0,
    };

    for dev_slot in 0u8..32 {
        for func in 0u8..8 {
            let id = unsafe { arch::pci_cfg_read32(0, dev_slot, func, 0x00) };
            let vendor_id = (id & 0xFFFF) as u16;
            if vendor_id == 0xFFFF {
                if func == 0 {
                    break;
                }
                continue;
            }
            let device_id = ((id >> 16) & 0xFFFF) as u16;

            let class_reg = unsafe { arch::pci_cfg_read32(0, dev_slot, func, 0x08) };
            let class_code = ((class_reg >> 24) & 0xFF) as u8;
            let subclass = ((class_reg >> 16) & 0xFF) as u8;

            let bar0 = unsafe { arch::pci_cfg_read32(0, dev_slot, func, 0x10) };
            let bar1 = unsafe { arch::pci_cfg_read32(0, dev_slot, func, 0x14) };
            let bar4 = unsafe { arch::pci_cfg_read32(0, dev_slot, func, 0x20) };
            let bar5 = unsafe { arch::pci_cfg_read32(0, dev_slot, func, 0x24) };

            let intr_reg = unsafe { arch::pci_cfg_read32(0, dev_slot, func, 0x3C) };
            let irq_line = (intr_reg & 0xFF) as u8;

            if result.count < MAX_DISCOVERED {
                result.devices[result.count] = Some(PciDevice {
                    bus: 0,
                    device: dev_slot,
                    function: func,
                    vendor_id,
                    device_id,
                    class_code,
                    subclass,
                    bar0,
                    bar1,
                    bar4,
                    bar5,
                    irq_line,
                });
                result.count += 1;
            }

            if func == 0 {
                let header_type = unsafe { arch::pci_cfg_read32(0, dev_slot, 0, 0x0C) };
                let multi_func = (header_type >> 23) & 1;
                if multi_func == 0 {
                    break;
                }
            }
        }
    }

    result
}

/// Enable Bus Master + Memory Space on the given PCI device.
///
/// # Safety
///
/// Ring 0 only.  Writes to PCI command register.
pub unsafe fn enable_bus_master(dev: &PciDevice) {
    let cmd = unsafe { arch::pci_cfg_read32(dev.bus, dev.device, dev.function, 0x04) };
    let new_cmd = cmd | 0x0006; // MSE (bit 1) | BME (bit 2)
    if new_cmd != cmd {
        let addr: u32 = 0x8000_0000
            | (u32::from(dev.bus) << 16)
            | (u32::from(dev.device) << 11)
            | (u32::from(dev.function) << 8)
            | 0x04u32;
        unsafe {
            arch::outl(0xCF8, addr);
            arch::outl(0xCFC, new_cmd);
        }
    }
}

// =========================================================================
// Well-known PCI vendor/device IDs
// =========================================================================

/// Red Hat / VirtIO vendor ID.
pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;

/// VirtIO network device (transitional).
pub const VIRTIO_NET_DEVICE_ID_TRANSITIONAL: u16 = 0x1000;

/// VirtIO network device (modern, non-transitional).
pub const VIRTIO_NET_DEVICE_ID_MODERN: u16 = 0x1041;

/// Intel vendor ID.
pub const INTEL_VENDOR_ID: u16 = 0x8086;

/// NVMe class code (Mass Storage Controller, NVM Express).
pub const NVME_CLASS_CODE: u8 = 0x01;
/// NVMe sub-class code.
pub const NVME_SUBCLASS: u8 = 0x08;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_mmio_base_masks_low_bits() {
        assert_eq!(PciDevice::bar_mmio_base(0xFEBC_0001), 0xFEBC_0000);
        assert_eq!(PciDevice::bar_mmio_base(0xFEBC_000F), 0xFEBC_0000);
    }

    #[test]
    fn bar_is_64bit_detects_type() {
        assert!(!PciDevice::bar_is_64bit(0xFEBC_0000));
        assert!(PciDevice::bar_is_64bit(0xFEBC_0004));
    }

    #[test]
    fn bar64_combines_halves() {
        assert_eq!(PciDevice::bar64(0x0000_0004, 0x0000_0001), 0x0000_0001_0000_0000);
    }

    #[test]
    fn scan_result_find_returns_none_when_empty() {
        let result = ScanResult {
            devices: [None; MAX_DISCOVERED],
            count: 0,
        };
        assert!(result.find(0x1AF4, 0x1000).is_none());
    }

    #[test]
    fn scan_result_iter_yields_nothing_when_empty() {
        let result = ScanResult {
            devices: [None; MAX_DISCOVERED],
            count: 0,
        };
        assert_eq!(result.iter().count(), 0);
    }
}
