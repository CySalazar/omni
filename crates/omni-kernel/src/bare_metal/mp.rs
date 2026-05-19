//! Multi-processor enumeration — MB14.c.1 foundation for AP startup.
//!
//! ## Scope (MB14.c.1)
//!
//! Discovers the set of logical CPUs the firmware reports as
//! [`MADT`]-present. The output is a fixed-capacity list of
//! [`CpuEntry`] records — one per `Processor Local APIC` /
//! `Processor Local x2APIC` MADT entry — that downstream code (MB14.c.2
//! INIT-SIPI orchestrator, MB14.d TLB shootdown broadcast,
//! MB14.e per-CPU run-queue) consumes to know "which APs do we wake".
//!
//! **No APs are started here.** This module only parses ACPI tables.
//! The actual INIT-SIPI-SIPI handshake + real-mode trampoline land in
//! MB14.c.2.
//!
//! ## ACPI tables walked
//!
//! ```text
//! RSDP ──┬─► XSDT (revision >= 2, 64-bit entries) ─► [tables…] ─► MADT
//!        └─► RSDT (revision == 0, 32-bit entries) ─► [tables…] ─► MADT
//! ```
//!
//! The MADT (`APIC` signature, ACPI § 5.2.12) has a 44-byte header
//! followed by a variable-length list of "Interrupt Controller
//! Structures". Each ICS starts with a 1-byte type + 1-byte length.
//! MB14.c.1 cares about exactly two types:
//!
//! - **`0x00` — Processor Local APIC** (ACPI § 5.2.12.2): 8 bytes,
//!   carries `acpi_processor_id` (1B), `apic_id` (1B), `flags` (4B).
//! - **`0x09` — Processor Local x2APIC** (ACPI § 5.2.12.12): 16 bytes,
//!   carries `apic_id` (4B, u32), `flags` (4B), `acpi_processor_uid` (4B).
//!
//! Other ICS types (IO APIC, NMI sources, etc.) are skipped without
//! erroring, since their `length` byte tells us how far to advance.
//!
//! The `flags` field bit 0 (`Enabled`) MUST be set for the entry to
//! count as a usable CPU. Bit 1 (`Online Capable`) is observed but not
//! required for the BSP (it's always reported `Enabled`).
//!
//! ## Why this lives in a pure function
//!
//! [`parse_madt`] takes a `&[u8]` and returns a fixed-capacity result.
//! Host tests can hand-craft a MADT byte buffer (see the `tests`
//! module below) and validate the parser without any bare-metal
//! plumbing. The bare-metal entry point [`enumerate_cpus`] is a thin
//! `unsafe` wrapper that locates the MADT via `phys_offset` and feeds
//! its bytes to the pure parser.
//!
//! ## Capacity
//!
//! [`MAX_CPUS`] caps the descriptor array MB14.a will allocate. 32 is
//! generous for a Phase 1 kernel (Proxmox VMID 103 dev VM has 2–4
//! vCPUs); the parser surfaces an error if the MADT advertises more.
//!
//! [`MADT`]: https://uefi.org/specs/ACPI/6.4/05_ACPI_Software_Programming_Model/ACPI_Software_Programming_Model.html#multiple-apic-description-table-madt

#![allow(unsafe_code, reason = "ACPI table walk reads raw physical-memory window")]

/// Hard cap on the number of logical CPUs the kernel tracks.
///
/// 32 covers every reasonable Phase 1 deployment (the Proxmox dev VM is
/// 2–4 vCPUs) while keeping the static `PerCpu` array small. MB14.e can
/// raise this once per-CPU run-queues are sized.
pub const MAX_CPUS: usize = 32;

/// A single CPU entry decoded from the MADT.
///
/// The BSP is always present in the list (firmware reports it as
/// Local APIC entry with `Enabled = 1`); callers identify it by
/// matching `apic_id` against the value read from LAPIC register `0x20`
/// (see [`super::lapic::read_lapic_id`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuEntry {
    /// LAPIC ID as reported by firmware. Up to 8 bits for xAPIC entries,
    /// up to 32 bits for x2APIC entries. The kernel widens xAPIC IDs
    /// into `u32` so a single field type covers both.
    pub apic_id: u32,
    /// ACPI processor UID (xAPIC) or processor UID (x2APIC). Opaque to
    /// the kernel — recorded for diagnostics only.
    pub acpi_uid: u32,
    /// `true` iff the firmware `Enabled` flag (bit 0) is set. Disabled
    /// entries are reported but not used by the INIT-SIPI orchestrator.
    pub enabled: bool,
    /// `true` iff the entry came from a `Processor Local x2APIC` (type
    /// 0x09) record. `false` for the legacy `Processor Local APIC`
    /// (type 0x00). The orchestrator uses this to choose between xAPIC
    /// and x2APIC ICR encodings.
    pub x2apic: bool,
}

/// Result of a successful MADT walk: at most [`MAX_CPUS`] entries.
#[derive(Debug, Clone, Copy)]
pub struct CpuTopology {
    entries: [CpuEntry; MAX_CPUS],
    count: usize,
}

impl CpuTopology {
    /// Slice of all decoded entries (BSP + APs, enabled and disabled).
    #[must_use]
    pub fn entries(&self) -> &[CpuEntry] {
        // `count <= MAX_CPUS` is the invariant maintained by `push`.
        self.entries.get(..self.count).unwrap_or(&[])
    }

    /// Total number of CPU entries the MADT advertised.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// `true` if the MADT had zero CPU entries (pathological; firmware
    /// always reports at least the BSP).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Number of entries with `enabled = true`. This is the figure the
    /// AP wake-up orchestrator targets: only `Enabled` CPUs receive
    /// INIT-SIPI-SIPI in MB14.c.2.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.entries().iter().filter(|c| c.enabled).count()
    }
}

/// Error from [`parse_madt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MadtError {
    /// Buffer is shorter than the 44-byte MADT fixed header.
    Truncated,
    /// First 4 bytes are not `b"APIC"`.
    BadSignature,
    /// Header `length` field disagrees with the supplied buffer.
    LengthMismatch,
    /// An ICS entry advertised a length of 0 (would loop forever) or
    /// a length that would walk past the table end.
    MalformedEntry,
    /// The MADT lists more CPU entries than [`MAX_CPUS`] can hold.
    TooManyCpus,
}

/// Parse a MADT byte slice and extract all CPU entries.
///
/// `buf` must point at the MADT header — i.e. the first byte is `'A'`
/// of `b"APIC"`. The function reads exactly `header.length` bytes;
/// trailing bytes (if `buf` is longer) are ignored.
///
/// # Errors
///
/// See [`MadtError`].
pub fn parse_madt(buf: &[u8]) -> Result<CpuTopology, MadtError> {
    // MADT fixed header layout (ACPI § 5.2.12):
    //   off 0..4   : signature ("APIC")
    //   off 4..8   : length (u32 LE; includes header + all ICS entries)
    //   off 8..12  : revision (u8), checksum (u8), oem_id (6 bytes)
    //   off 12..36 : oem_id rest + oem_table_id + oem_revision + creator_*
    //   off 36..40 : local_apic_address (u32 LE) — not used by MB14.c.1
    //   off 40..44 : flags (u32 LE) — bit 0 = PCAT_COMPAT
    //   off 44..   : variable-length ICS entries
    const HEADER_LEN: usize = 44;

    if buf.len() < HEADER_LEN {
        return Err(MadtError::Truncated);
    }
    if buf.get(..4) != Some(b"APIC") {
        return Err(MadtError::BadSignature);
    }
    let length = read_u32_le(buf, 4) as usize;
    if length < HEADER_LEN || length > buf.len() {
        return Err(MadtError::LengthMismatch);
    }

    let mut entries = [CpuEntry {
        apic_id: 0,
        acpi_uid: 0,
        enabled: false,
        x2apic: false,
    }; MAX_CPUS];
    let mut count: usize = 0;

    let mut off = HEADER_LEN;
    while off < length {
        // Every ICS starts with type(u8) + length(u8). A length of 0
        // would loop forever; treat as malformed.
        let ics_type = *buf.get(off).ok_or(MadtError::MalformedEntry)?;
        let ics_len = *buf.get(off + 1).ok_or(MadtError::MalformedEntry)? as usize;
        if ics_len < 2 || off + ics_len > length {
            return Err(MadtError::MalformedEntry);
        }

        match ics_type {
            // Type 0 — Processor Local APIC.
            //   off+0  : type   = 0x00
            //   off+1  : length = 8
            //   off+2  : acpi_processor_id (u8)
            //   off+3  : apic_id           (u8)
            //   off+4  : flags             (u32 LE)
            0x00 if ics_len == 8 => {
                let acpi_uid = u32::from(*buf.get(off + 2).ok_or(MadtError::MalformedEntry)?);
                let apic_id = u32::from(*buf.get(off + 3).ok_or(MadtError::MalformedEntry)?);
                let flags = read_u32_le(buf, off + 4);
                if count >= MAX_CPUS {
                    return Err(MadtError::TooManyCpus);
                }
                // `count < MAX_CPUS == entries.len()` ensured by the
                // guard above; `get_mut` keeps `clippy::indexing_slicing`
                // quiet without a panic site.
                if let Some(slot) = entries.get_mut(count) {
                    *slot = CpuEntry {
                        apic_id,
                        acpi_uid,
                        enabled: (flags & 0x1) != 0,
                        x2apic: false,
                    };
                }
                count += 1;
            }
            // Type 9 — Processor Local x2APIC.
            //   off+0  : type   = 0x09
            //   off+1  : length = 16
            //   off+2..4 : reserved
            //   off+4..8 : x2apic_id (u32 LE)
            //   off+8..12: flags     (u32 LE)
            //   off+12..16: acpi_processor_uid (u32 LE)
            0x09 if ics_len == 16 => {
                let apic_id = read_u32_le(buf, off + 4);
                let flags = read_u32_le(buf, off + 8);
                let acpi_uid = read_u32_le(buf, off + 12);
                if count >= MAX_CPUS {
                    return Err(MadtError::TooManyCpus);
                }
                if let Some(slot) = entries.get_mut(count) {
                    *slot = CpuEntry {
                        apic_id,
                        acpi_uid,
                        enabled: (flags & 0x1) != 0,
                        x2apic: true,
                    };
                }
                count += 1;
            }
            // Any other ICS type (IO APIC, NMI source, etc.) — skip.
            _ => {}
        }

        off += ics_len;
    }

    Ok(CpuTopology { entries, count })
}

/// Read a little-endian `u32` at byte offset `off` in `buf`.
///
/// The caller is responsible for ensuring `off + 4 <= buf.len()`.
/// Returns `0` on out-of-range (defensive — every caller pre-checks).
fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    let Some(s) = buf.get(off..off + 4) else {
        return 0;
    };
    // `s.len() == 4` by construction of the slice above.
    let arr = <[u8; 4]>::try_from(s).unwrap_or([0; 4]);
    u32::from_le_bytes(arr)
}

// =============================================================================
// Bare-metal entry point: locate the MADT via RSDP and parse it.
// =============================================================================

/// Locate the MADT through the firmware-supplied RSDP and decode all
/// CPU entries.
///
/// `rsdp_phys` is the physical address from `BootInfo.rsdp_addr`.
/// `phys_offset` is the virtual offset of the firmware-supplied
/// physical-memory mapping (`BootInfo.physical_memory_offset`).
///
/// Returns `None` if any step in the table walk fails (bad RSDP
/// signature, missing XSDT/RSDT, no MADT, parse error). The caller logs
/// `[mb14.c.1] MADT walk FAILED` in that case and falls back to BSP-only
/// behaviour (which is what MB14.b already does).
///
/// # Safety
///
/// `phys_offset.wrapping_add(rsdp_phys)` must point at a valid RSDP,
/// and every ACPI table physical address reachable from there must lie
/// within the mapped physical-memory window. The same invariants the
/// FADT walker in [`super::arch::find_pm1a_cnt_from_fadt`] depends on.
#[cfg(target_arch = "x86_64")]
pub unsafe fn enumerate_cpus(rsdp_phys: u64, phys_offset: u64) -> Option<CpuTopology> {
    let madt_phys = unsafe { find_table_phys(rsdp_phys, phys_offset, b"APIC")? };
    // The table's `length` field is at offset 4 (u32 LE).
    let madt_ptr = phys_offset.wrapping_add(madt_phys) as *const u8;
    let length = unsafe { madt_ptr.add(4).cast::<u32>().read_unaligned() } as usize;
    if length < 44 {
        return None;
    }
    // SAFETY: the firmware reports a contiguous mapping covering all ACPI
    // tables; the same assumption the FADT walker makes. `length` bytes
    // starting at `madt_ptr` are within that window.
    let buf = unsafe { core::slice::from_raw_parts(madt_ptr, length) };
    parse_madt(buf).ok()
}

/// Walk the RSDP → XSDT/RSDT chain looking for a table whose 4-byte
/// signature equals `target`. Returns the table's *physical* address.
///
/// Modelled on [`super::arch::find_pm1a_cnt_from_fadt`] but generalised
/// to any signature (here used for `b"APIC"` = MADT).
///
/// # Safety
///
/// Same as [`enumerate_cpus`].
#[cfg(target_arch = "x86_64")]
#[allow(
    clippy::integer_division,
    reason = "ACPI SDT entry count is `(length - 36) / entry_size`; both operands are bounded by the firmware-supplied table size and the division is the canonical decoding rule from the spec"
)]
unsafe fn find_table_phys(rsdp_phys: u64, phys_offset: u64, target: &[u8; 4]) -> Option<u64> {
    let p2v = |phys: u64| -> *const u8 { phys_offset.wrapping_add(phys) as *const u8 };
    let read32 = |ptr: *const u8, off: usize| -> u32 {
        unsafe { ptr.add(off).cast::<u32>().read_unaligned() }
    };
    let read64 = |ptr: *const u8, off: usize| -> u64 {
        unsafe { ptr.add(off).cast::<u64>().read_unaligned() }
    };

    // Verify RSDP signature.
    let rsdp = p2v(rsdp_phys);
    let sig = unsafe { core::slice::from_raw_parts(rsdp, 8) };
    if sig != b"RSD PTR " {
        return None;
    }
    let revision = unsafe { *rsdp.add(15) };

    // Search a *SDT for `target`. `wide` toggles 32-bit (RSDT) vs
    // 64-bit (XSDT) pointer width.
    let try_sdt = |sdt_phys: u64, wide: bool| -> Option<u64> {
        let sdt = p2v(sdt_phys);
        let sig4 = unsafe { core::slice::from_raw_parts(sdt, 4) };
        let expected: &[u8] = if wide { b"XSDT" } else { b"RSDT" };
        if sig4 != expected {
            return None;
        }
        let len = read32(sdt, 4) as usize;
        let entry_size: usize = if wide { 8 } else { 4 };
        let entry_count = len.saturating_sub(36) / entry_size;
        for i in 0..entry_count {
            let entry_phys: u64 = if wide {
                read64(sdt, 36 + i * 8)
            } else {
                u64::from(read32(sdt, 36 + i * 4))
            };
            let tbl = p2v(entry_phys);
            let tsig = unsafe { core::slice::from_raw_parts(tbl, 4) };
            if tsig == target {
                return Some(entry_phys);
            }
        }
        None
    };

    // Prefer XSDT (ACPI 2.0+); fall back to RSDT.
    if revision >= 2 {
        let xsdt_phys = read64(rsdp, 24);
        if let Some(p) = try_sdt(xsdt_phys, true) {
            return Some(p);
        }
    }
    let legacy_rsdt = u64::from(read32(rsdp, 16));
    try_sdt(legacy_rsdt, false)
}

/// Host-side stub: the bare-metal MADT walk is not reachable from
/// `cargo test` (no physical memory window). Tests exercise
/// [`parse_madt`] directly with hand-crafted byte buffers.
#[cfg(not(target_arch = "x86_64"))]
pub unsafe fn enumerate_cpus(_rsdp_phys: u64, _phys_offset: u64) -> Option<CpuTopology> {
    None
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "tests panic on bounds violation by design — surfaces parser regressions as test failures, not silent wrong values"
)]
mod tests {
    use super::*;

    /// Build a minimal MADT byte buffer suitable for [`parse_madt`].
    ///
    /// `ics_bytes` is appended verbatim after the 44-byte header. The
    /// header `length` field is filled in to match the resulting size.
    /// All other header fields are zeroed (revision, checksums, OEM IDs
    /// — none of which the parser inspects).
    fn make_madt(ics_bytes: &[u8]) -> alloc::vec::Vec<u8> {
        let total = 44 + ics_bytes.len();
        let mut buf = alloc::vec![0u8; total];
        buf[0..4].copy_from_slice(b"APIC");
        #[allow(clippy::cast_possible_truncation)]
        let len_le = (total as u32).to_le_bytes();
        buf[4..8].copy_from_slice(&len_le);
        buf[44..].copy_from_slice(ics_bytes);
        buf
    }

    extern crate alloc;

    #[test]
    fn truncated_buffer_errs() {
        let buf = [0u8; 16];
        assert!(matches!(parse_madt(&buf), Err(MadtError::Truncated)));
    }

    #[test]
    fn bad_signature_errs() {
        let mut buf = make_madt(&[]);
        buf[0] = b'X';
        assert!(matches!(parse_madt(&buf), Err(MadtError::BadSignature)));
    }

    #[test]
    fn length_mismatch_errs_when_header_lies() {
        let mut buf = make_madt(&[]);
        // Claim length = 999 but the buffer is only 44 bytes.
        buf[4..8].copy_from_slice(&999u32.to_le_bytes());
        assert!(matches!(parse_madt(&buf), Err(MadtError::LengthMismatch)));
    }

    #[test]
    fn empty_madt_yields_zero_cpus() {
        let buf = make_madt(&[]);
        let topo = parse_madt(&buf).expect("parse");
        assert!(topo.is_empty());
        assert_eq!(topo.len(), 0);
        assert_eq!(topo.enabled_count(), 0);
    }

    #[test]
    fn single_bsp_local_apic_decoded() {
        // Processor Local APIC: type=0, len=8, acpi_uid=1, apic_id=0,
        // flags=0x1 (Enabled).
        let ics = [
            0x00, 0x08, // type, length
            0x01, // acpi_processor_id
            0x00, // apic_id
            0x01, 0x00, 0x00, 0x00, // flags = Enabled
        ];
        let buf = make_madt(&ics);
        let topo = parse_madt(&buf).expect("parse");
        assert_eq!(topo.len(), 1);
        assert_eq!(topo.enabled_count(), 1);
        let cpu = topo.entries()[0];
        assert_eq!(cpu.apic_id, 0);
        assert_eq!(cpu.acpi_uid, 1);
        assert!(cpu.enabled);
        assert!(!cpu.x2apic);
    }

    #[test]
    fn disabled_local_apic_kept_but_flagged() {
        let ics = [
            0x00, 0x08, // type, length
            0x02, // acpi_processor_id
            0x01, // apic_id = 1
            0x00, 0x00, 0x00, 0x00, // flags = 0 (disabled)
        ];
        let buf = make_madt(&ics);
        let topo = parse_madt(&buf).expect("parse");
        assert_eq!(topo.len(), 1);
        assert_eq!(topo.enabled_count(), 0);
        assert!(!topo.entries()[0].enabled);
    }

    #[test]
    fn x2apic_entry_decoded_with_32bit_id() {
        // Processor Local x2APIC: type=0x09, len=16, reserved, apic_id (u32),
        // flags (u32), acpi_uid (u32).
        let mut ics = alloc::vec::Vec::new();
        ics.extend_from_slice(&[0x09, 0x10, 0x00, 0x00]); // type, length, reserved
        ics.extend_from_slice(&0x1234_5678_u32.to_le_bytes()); // apic_id
        ics.extend_from_slice(&0x1_u32.to_le_bytes()); // flags = Enabled
        ics.extend_from_slice(&0x42_u32.to_le_bytes()); // acpi_uid
        let buf = make_madt(&ics);
        let topo = parse_madt(&buf).expect("parse");
        assert_eq!(topo.len(), 1);
        let cpu = topo.entries()[0];
        assert_eq!(cpu.apic_id, 0x1234_5678);
        assert_eq!(cpu.acpi_uid, 0x42);
        assert!(cpu.enabled);
        assert!(cpu.x2apic);
    }

    #[test]
    fn multiple_cpus_in_order() {
        // Two Local APIC entries (BSP + 1 AP) interleaved with an
        // IO APIC entry (type 1, length 12) the parser must skip.
        let mut ics = alloc::vec::Vec::new();
        // BSP — Local APIC, apic_id=0, enabled.
        ics.extend_from_slice(&[0x00, 0x08, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00]);
        // IO APIC — type 1, length 12, zeroed payload (parser must skip).
        ics.extend_from_slice(&[0x01, 0x0C, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        // AP — Local APIC, apic_id=1, enabled.
        ics.extend_from_slice(&[0x00, 0x08, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00]);
        let buf = make_madt(&ics);
        let topo = parse_madt(&buf).expect("parse");
        assert_eq!(topo.len(), 2);
        assert_eq!(topo.enabled_count(), 2);
        assert_eq!(topo.entries()[0].apic_id, 0);
        assert_eq!(topo.entries()[1].apic_id, 1);
    }

    #[test]
    fn unknown_ics_type_skipped_not_errored() {
        let mut ics = alloc::vec::Vec::new();
        // Some unknown ICS type 0x42, length 6, payload zeroed.
        ics.extend_from_slice(&[0x42, 0x06, 0, 0, 0, 0]);
        // Followed by a real Local APIC.
        ics.extend_from_slice(&[0x00, 0x08, 0x01, 0x05, 0x01, 0x00, 0x00, 0x00]);
        let buf = make_madt(&ics);
        let topo = parse_madt(&buf).expect("parse");
        assert_eq!(topo.len(), 1);
        assert_eq!(topo.entries()[0].apic_id, 5);
    }

    #[test]
    fn zero_length_ics_errs() {
        // An ICS with length=0 would loop forever; parser must reject.
        let ics = [0x00, 0x00];
        let buf = make_madt(&ics);
        assert!(matches!(parse_madt(&buf), Err(MadtError::MalformedEntry)));
    }

    #[test]
    fn ics_running_past_table_end_errs() {
        // ICS claims length=64 but the buffer past the header is only 2 bytes.
        let ics = [0x00, 64];
        let buf = make_madt(&ics);
        assert!(matches!(parse_madt(&buf), Err(MadtError::MalformedEntry)));
    }

    #[test]
    fn too_many_cpus_errs() {
        // Generate MAX_CPUS + 1 Local APIC entries.
        let mut ics = alloc::vec::Vec::new();
        for i in 0..=MAX_CPUS {
            #[allow(clippy::cast_possible_truncation)]
            let apic_id = i as u8;
            ics.extend_from_slice(&[0x00, 0x08, apic_id, apic_id, 0x01, 0, 0, 0]);
        }
        let buf = make_madt(&ics);
        assert!(matches!(parse_madt(&buf), Err(MadtError::TooManyCpus)));
    }
}
