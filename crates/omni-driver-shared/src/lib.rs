//! # `omni-driver-shared`
//!
//! OMNI OS driver SDK helpers: capability deposit window parser and
//! well-known constants.
//!
//! ## Purpose
//!
//! When the kernel loads a driver via `DriverLoad` (OIP-013 § S5.3 step 8)
//! it mints attenuated [`CapabilityToken`][cap-token] blobs for every
//! capability declared in the driver's manifest and deposits them in a
//! read-only 32 KiB window at the well-known virtual address
//! `0x0010_0000` before transferring execution to the driver's `_start`.
//!
//! This crate provides:
//!
//! - The well-known VA ([`DRIVER_CAP_DEPOSIT_VA`]) and window length
//!   ([`DRIVER_CAP_DEPOSIT_LEN`]) constants every driver needs to locate
//!   the deposit.
//! - The [`OmniCapsHeader`] layout type that describes the 16-byte header
//!   at the start of the deposit window.
//! - The [`OmniCapsError`] error type returned by header and entry parsers.
//! - The [`caps`] module with the primary driver-facing API:
//!   [`caps::find_token`] (production, reads from the static VA) and
//!   [`caps::find_token_in_buf`] (pure-function variant for tests).
//!
//! ## Intended usage in a driver `_start`
//!
//! ```no_run
//! // Locate the MmioMap token for the first MMIO region.
//! // ACTION_TAG_MMIO_MAP == 1 per OIP-013 § S5.3 wire format.
//! let token_bytes: &[u8] =
//!     omni_driver_shared::caps::find_token(
//!         omni_driver_shared::ACTION_TAG_MMIO_MAP,
//!         |_| true,   // accept the first matching entry
//!     )
//!     .expect("kernel must deposit at least one MmioMap token");
//! // token_bytes is a postcard-encoded CapabilityToken;
//! // present it to the MmioMap (70) syscall.
//! drop(token_bytes);
//! ```
//!
//! ## Design rationale
//!
//! The deposit window uses a flat indexed layout so drivers can scan it
//! without deserializing the full `CapabilityToken` tree: one 16-byte
//! entry descriptor per token (`action_tag` + `resource_tag` + offset + len)
//! precedes the packed postcard blobs.  This crate implements the scan
//! path; the kernel-side encoder lives in
//! `crates/omni-kernel/src/cap_deposit.rs`.
//!
//! **Zero production dependencies.**  Keeping this crate dep-free ensures
//! no transitive supply-chain vulnerability can reach driver binaries
//! through the SDK layer.
//!
//! ## Cross-references
//!
//! - OIP-013 § S5.3 step 8 (deposit ABI specification)
//! - `docs/plans/p6-7-8-9-cap-deposit-trampoline.md` § D3 (design decisions)
//! - `crates/omni-kernel/src/cap_deposit.rs` (kernel-side wire format encoder)
//!
//! [cap-token]: https://docs.omni-os.org/omni_capability/token/struct.CapabilityToken.html

#![doc(html_root_url = "https://docs.omni-os.org/omni-driver-shared")]
// Enable `no_std` for non-test builds.  When `cargo test` compiles this
// crate, `std` is available so that test utilities (proptest, std::vec!)
// work without additional scaffolding.
#![cfg_attr(not(test), no_std)]
#![warn(missing_docs)]
// ----------------------------------------------------------------------------
// Test-only lint relaxations (ADR-0003 § Escape hatches — `cfg_attr(test)`).
// These are intentionally broad for the test module only.
// ----------------------------------------------------------------------------
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::wildcard_imports,
        reason = "test harness relaxations: tests may use expect/unwrap/panic, direct \
                  range indexing, and wildcard imports (proptest::prelude::*)"
    )
)]

// ---------------------------------------------------------------------------
// Public constants — capability deposit window
// ---------------------------------------------------------------------------

/// Well-known user-VA base where the kernel deposits capability tokens.
///
/// The kernel maps a read-only 32 KiB region starting at this address in
/// the driver process's address space before transferring execution to
/// `_start` (OIP-013 § S5.3 step 8).  Drivers **MUST NOT** write to or
/// unmap this region.
///
/// Value: `0x0010_0000` (1 MiB).
///
/// # Example
///
/// ```
/// assert_eq!(omni_driver_shared::DRIVER_CAP_DEPOSIT_VA, 0x0010_0000u64);
/// ```
pub const DRIVER_CAP_DEPOSIT_VA: u64 = 0x0010_0000;

/// Total byte length of the capability deposit window.
///
/// Eight consecutive 4 KiB pages (32 KiB), sized to hold up to
/// [`MAX_ENTRIES`] postcard-encoded `CapabilityToken` blobs plus the
/// fixed header and entry-descriptor table.
///
/// Value: `0x8000` (32 768 bytes).
///
/// # Example
///
/// ```
/// assert_eq!(omni_driver_shared::DRIVER_CAP_DEPOSIT_LEN, 0x8000usize);
/// ```
pub const DRIVER_CAP_DEPOSIT_LEN: usize = 0x8000;

/// Maximum number of capability entries the deposit window may contain.
///
/// 64 covers the worst-case driver manifests planned for Phase 1
/// (ConnectX-series NICs per `OIP-Driver-Net-015 M3`, NVMe per
/// `OIP-Driver-NVMe-014`).
///
/// # Example
///
/// ```
/// assert!(omni_driver_shared::MAX_ENTRIES == 64);
/// ```
pub const MAX_ENTRIES: usize = 64;

// ---------------------------------------------------------------------------
// Public constants — action tags (OIP-013 § S5.3 wire format)
// ---------------------------------------------------------------------------
// These numeric discriminants appear in every entry descriptor's
// `action_tag` field.  Drivers pass the same value to
// [`caps::find_token`] / [`caps::find_token_in_buf`] to locate the
// matching token.

/// Wire-format discriminant for the `MmioMap` action (value `1`).
///
/// Use this as the `action_tag` argument to [`caps::find_token`] when
/// looking up a memory-mapped I/O region token.
///
/// # Example
///
/// ```
/// assert_eq!(omni_driver_shared::ACTION_TAG_MMIO_MAP, 1u32);
/// ```
pub const ACTION_TAG_MMIO_MAP: u32 = 1;

/// Wire-format discriminant for the `DmaMap` action (value `2`).
///
/// # Example
///
/// ```
/// assert_eq!(omni_driver_shared::ACTION_TAG_DMA_MAP, 2u32);
/// ```
pub const ACTION_TAG_DMA_MAP: u32 = 2;

/// Wire-format discriminant for the `IrqAttach` action (value `3`).
///
/// # Example
///
/// ```
/// assert_eq!(omni_driver_shared::ACTION_TAG_IRQ_ATTACH, 3u32);
/// ```
pub const ACTION_TAG_IRQ_ATTACH: u32 = 3;

/// Wire-format discriminant for the `PciConfigRead` action (value `4`).
///
/// # Example
///
/// ```
/// assert_eq!(omni_driver_shared::ACTION_TAG_PCI_CFG_READ, 4u32);
/// ```
pub const ACTION_TAG_PCI_CFG_READ: u32 = 4;

/// Wire-format discriminant for the `PciConfigWrite` action (value `5`).
///
/// # Example
///
/// ```
/// assert_eq!(omni_driver_shared::ACTION_TAG_PCI_CFG_WRITE, 5u32);
/// ```
pub const ACTION_TAG_PCI_CFG_WRITE: u32 = 5;

// ---------------------------------------------------------------------------
// Internal wire-format constants (mirrors `omni-kernel::cap_deposit`)
// ---------------------------------------------------------------------------
// These constants are NOT public: they are wire-format details drivers do
// not need to see directly.  Only the `caps` module uses them.

/// 8-byte ASCII magic at offset 0 of the deposit window.
const DEPOSIT_MAGIC: [u8; 8] = *b"OMNICAPS";

/// Wire-format version supported by this crate.
const DEPOSIT_VERSION: u32 = 1;

/// Byte length of the fixed deposit header (magic + version + `entry_count`).
const HEADER_LEN: usize = 16; // 8 + 4 + 4

/// Byte length of each entry descriptor in the indexed table.
const ENTRY_DESCRIPTOR_LEN: usize = 16; // 4 + 4 + 4 + 4

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that may occur while parsing the capability deposit window.
///
/// Produced when header parsing fails, and surfaced as [`None`] inside
/// [`caps::find_token_in_buf`] and [`caps::find_token`].
///
/// # Example
///
/// ```
/// use omni_driver_shared::OmniCapsError;
/// let e = OmniCapsError::InvalidMagic;
/// assert_eq!(e.to_string(), "invalid OMNICAPS magic");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmniCapsError {
    /// The 8-byte magic at the start of the window is not `b"OMNICAPS"`.
    ///
    /// This usually means the deposit VA was read before the kernel
    /// finished initialising the window, or the process was not launched
    /// through `DriverLoad`.
    InvalidMagic,
    /// The `version` field in the header is not `1`.
    ///
    /// Bump `DEPOSIT_VERSION` in both this crate and the kernel's
    /// `cap_deposit.rs` whenever the entry descriptor layout changes.
    UnsupportedVersion,
    /// The `entry_count` field exceeds [`MAX_ENTRIES`] (`64`).
    ///
    /// Should never occur with a correctly minted deposit page; indicates
    /// memory corruption or a version skew.
    EntryCountExceeded,
    /// A token's `token_offset` or `token_len` would read past the end
    /// of the window.
    ///
    /// Indicates a corrupt or attacker-crafted deposit page.
    OutOfBoundsOffset,
}

impl core::fmt::Display for OmniCapsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidMagic => f.write_str("invalid OMNICAPS magic"),
            Self::UnsupportedVersion => f.write_str("unsupported deposit version"),
            Self::EntryCountExceeded => f.write_str("entry count exceeds maximum"),
            Self::OutOfBoundsOffset => f.write_str("token offset or length is out of bounds"),
        }
    }
}

impl core::error::Error for OmniCapsError {}

// ---------------------------------------------------------------------------
// Header layout type
// ---------------------------------------------------------------------------

/// In-memory layout of the capability deposit window header.
///
/// The kernel writes this 16-byte structure at the very start of the
/// 32 KiB deposit window before transferring execution to `_start`.
///
/// ## Wire format
///
/// ```text
/// Offset  Size   Field
/// ──────  ────   ─────────────────────────────────────────────────────────
/// 0x000   8 B    magic       = b"OMNICAPS"
/// 0x008   4 B    version     = 1u32 (little-endian)
/// 0x00C   4 B    entry_count = N ∈ [0, 64] (little-endian)
/// 0x010   N×16   entries[N]  (see entry descriptor layout below)
/// …       …      token blobs, packed, 8-byte-aligned offsets
/// ```
///
/// Entry descriptor layout (16 bytes each):
/// ```text
/// [0..4]   action_tag   — u32 LE (1=MmioMap, 2=DmaMap, 3=IrqAttach,
///                                  4=PciConfigRead, 5=PciConfigWrite)
/// [4..8]   resource_tag — u32 LE (1=MmioRegion, 2=DmaWindow, 3=IrqLine,
///                                  4=PciDevice, 5=Any)
/// [8..12]  token_offset — u32 LE, byte offset from page start
/// [12..16] token_len    — u32 LE, byte length of the postcard blob
/// ```
///
/// ## Notes
///
/// * All `u32` fields are little-endian; on x86-64 (Phase 1 target),
///   native and wire byte order match.
/// * Do not construct or mutate this type directly.  Use the [`caps`]
///   module to read from the kernel-mapped deposit window.
#[repr(C)]
pub struct OmniCapsHeader {
    /// 8-byte ASCII magic: `b"OMNICAPS"`.
    pub magic: [u8; 8],
    /// Wire-format version (little-endian `u32`); must equal `1`.
    pub version: u32,
    /// Number of capability entries in the indexed table that follows.
    pub entry_count: u32,
}

// ---------------------------------------------------------------------------
// Capability lookup module
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// BLK channel wire types
// ---------------------------------------------------------------------------

/// Fixed-size wire types for the `omni.svc.blk.<diskN>` IPC channel.
///
/// Provides [`blk::BlkRequest`], [`blk::BlkResponse`], [`blk::BlkStatus`],
/// [`blk::BlkCapacity`], [`blk::BlkDecodeError`], and the associated
/// encode/decode routines per OIP-Driver-NVMe-014 § S4 wire format.
pub mod blk;

/// Helpers for locating capability tokens in the kernel-deposited window.
///
/// This module is the primary API surface that driver `_start` functions
/// use.  See [`caps::find_token`] for the production entry point and
/// [`caps::find_token_in_buf`] for the testable pure-function variant.
pub mod caps {
    // Re-import only what we need from the parent; no wildcard imports.
    use super::{
        DEPOSIT_MAGIC, DEPOSIT_VERSION, DRIVER_CAP_DEPOSIT_LEN, DRIVER_CAP_DEPOSIT_VA,
        ENTRY_DESCRIPTOR_LEN, HEADER_LEN, MAX_ENTRIES, OmniCapsError, OmniCapsHeader,
    };

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Read 4 bytes starting at `offset` in `buf` and return them as a
    /// little-endian `u32`.
    ///
    /// Returns `None` if `offset + 4` would exceed `buf.len()`, preventing
    /// any out-of-bounds read.
    fn read_u32_le(buf: &[u8], offset: usize) -> Option<u32> {
        // Checked addition guards against hypothetical usize overflow on
        // pathological inputs even though offset ≤ DRIVER_CAP_DEPOSIT_LEN.
        let end = offset.checked_add(4)?;
        let bytes: [u8; 4] = buf.get(offset..end)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    /// Scan `entry_count` entry descriptors in `buf` for the first token
    /// that matches `action_tag` and satisfies `resource_predicate`.
    ///
    /// This is the shared inner loop used by both [`find_token`] (which
    /// derives `entry_count` from [`header()`]) and [`find_token_in_buf`]
    /// (which derives it from [`parse_header`]).  Splitting the loop here
    /// avoids redundant header re-parsing when `find_token` already called
    /// `header()` for validation.
    fn scan_entries(
        buf: &[u8],
        entry_count: usize,
        action_tag: u32,
        resource_predicate: impl Fn(&[u8]) -> bool,
    ) -> Option<&[u8]> {
        for i in 0..entry_count {
            let desc_base = HEADER_LEN.checked_add(i.checked_mul(ENTRY_DESCRIPTOR_LEN)?)?;
            let desc_end = desc_base.checked_add(ENTRY_DESCRIPTOR_LEN)?;

            if desc_end > buf.len() {
                return None;
            }

            let this_action_tag = read_u32_le(buf, desc_base)?;
            if this_action_tag != action_tag {
                continue;
            }

            let token_offset_field = desc_base.checked_add(8)?;
            let token_len_field = desc_base.checked_add(12)?;
            let token_offset = read_u32_le(buf, token_offset_field)? as usize;
            let token_len = read_u32_le(buf, token_len_field)? as usize;

            let token_end = token_offset.checked_add(token_len)?;
            if token_end > buf.len() {
                // Out-of-bounds token descriptor: skip silently.
                continue;
            }

            let token_slice = buf.get(token_offset..token_end)?;
            if resource_predicate(token_slice) {
                return Some(token_slice);
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Header parsing
    // -----------------------------------------------------------------------

    /// Parse and validate the 16-byte OMNICAPS header from the first bytes
    /// of `buf`.
    ///
    /// On success returns the `entry_count` field (guaranteed ≤ [`MAX_ENTRIES`]).
    ///
    /// # Errors
    ///
    /// - [`OmniCapsError::InvalidMagic`] — the first 8 bytes are not
    ///   `b"OMNICAPS"`, or `buf` is shorter than 16 bytes.
    /// - [`OmniCapsError::UnsupportedVersion`] — the `version` field is not
    ///   `1`.
    /// - [`OmniCapsError::EntryCountExceeded`] — `entry_count` exceeds
    ///   [`MAX_ENTRIES`] (64).
    pub(crate) fn parse_header(buf: &[u8]) -> Result<u32, OmniCapsError> {
        // Minimum-length guard: header is 16 bytes.
        if buf.len() < HEADER_LEN {
            return Err(OmniCapsError::InvalidMagic);
        }

        // Magic check — `buf.get(..8)` is bounds-safe.
        let magic: &[u8] = buf.get(..8).ok_or(OmniCapsError::InvalidMagic)?;
        if magic != DEPOSIT_MAGIC {
            return Err(OmniCapsError::InvalidMagic);
        }

        // Version check.
        let version = read_u32_le(buf, 8).ok_or(OmniCapsError::UnsupportedVersion)?;
        // `u32::from_le` is a no-op on little-endian (x86-64) but documents
        // the intent clearly for any future big-endian port.
        if u32::from_le(version) != DEPOSIT_VERSION {
            return Err(OmniCapsError::UnsupportedVersion);
        }

        // Entry count check.
        let entry_count = read_u32_le(buf, 12).ok_or(OmniCapsError::InvalidMagic)?;
        if entry_count as usize > MAX_ENTRIES {
            return Err(OmniCapsError::EntryCountExceeded);
        }

        Ok(entry_count)
    }

    // -----------------------------------------------------------------------
    // Unsafe header accessor (static VA)
    // -----------------------------------------------------------------------

    /// Read and validate the OMNICAPS header from the kernel-mapped deposit
    /// window at [`DRIVER_CAP_DEPOSIT_VA`].
    ///
    /// Returns a `'static` reference to the [`OmniCapsHeader`] on success.
    ///
    /// # Safety
    ///
    /// The caller MUST ensure that the virtual address
    /// [`DRIVER_CAP_DEPOSIT_VA`] (`0x0010_0000`) is mapped and readable in
    /// the current process's address space.  In a correctly loaded OMNI
    /// driver process (launched via `DriverLoad`, OIP-013 § S5.3 step 8),
    /// the kernel guarantees this mapping before transferring execution to
    /// `_start`.  Calling this function from any other context is undefined
    /// behaviour.
    ///
    /// # Errors
    ///
    /// - [`OmniCapsError::InvalidMagic`] — magic mismatch.
    /// - [`OmniCapsError::UnsupportedVersion`] — version not `1`.
    #[allow(
        unsafe_code,
        reason = "The deposit VA is mapped RO by the kernel before _start runs \
                  (OIP-013 § S5.3 step 8).  The repr(C) cast is sound: \
                  OmniCapsHeader is 16 bytes, alignment 4, and the page-aligned \
                  deposit VA satisfies both constraints."
    )]
    unsafe fn header() -> Result<&'static OmniCapsHeader, OmniCapsError> {
        // SAFETY: The caller's contract guarantees DRIVER_CAP_DEPOSIT_VA is
        // mapped.  OmniCapsHeader is repr(C) with size 16 and alignment 4.
        // DRIVER_CAP_DEPOSIT_VA (0x0010_0000 = 1 048 576) is page-aligned
        // (divisible by 4096), which satisfies the alignment-4 requirement.
        // The deposit window is at least DRIVER_CAP_DEPOSIT_LEN (0x8000)
        // bytes, so the 16-byte header is fully within the mapped region.
        let ptr = DRIVER_CAP_DEPOSIT_VA as *const OmniCapsHeader;
        // SAFETY (continued): the pointer is non-null, aligned, and within
        // a mapped region per the caller's contract; the reference lifetime
        // is 'static because the kernel never unmaps the deposit page during
        // the driver process's lifetime.
        let h: &'static OmniCapsHeader = unsafe { &*ptr };

        if h.magic != DEPOSIT_MAGIC {
            return Err(OmniCapsError::InvalidMagic);
        }
        // Read as little-endian; on x86-64 this is a no-op but stays
        // correct if a future port changes native byte order.
        if u32::from_le(h.version) != DEPOSIT_VERSION {
            return Err(OmniCapsError::UnsupportedVersion);
        }
        Ok(h)
    }

    // -----------------------------------------------------------------------
    // Public API — find_token (production, static VA)
    // -----------------------------------------------------------------------

    /// Locate the first deposited capability token matching `action_tag`
    /// for which `resource_predicate(token_bytes)` returns `true`.
    ///
    /// The function reads from the kernel-mapped deposit window at the
    /// well-known address [`DRIVER_CAP_DEPOSIT_VA`] (`0x0010_0000`).
    ///
    /// On success, returns a `'static` byte slice containing the
    /// postcard-encoded `CapabilityToken` blob.  Pass these bytes directly
    /// to the corresponding syscall (e.g. `MmioMap (70)`).
    ///
    /// Returns `None` if:
    /// - no entry with the requested `action_tag` exists in the deposit,
    /// - the deposit header is invalid (magic / version mismatch),
    /// - a matching entry's `resource_predicate` returns `false` for all
    ///   candidates, or
    /// - any entry descriptor has an out-of-bounds offset/length (indicates
    ///   memory corruption; silently skipped).
    ///
    /// ## Safety contract (for the caller)
    ///
    /// This is a **safe** function, but it performs an internal `unsafe`
    /// read from a fixed virtual address.  The function is only correct
    /// inside a process that was loaded by the kernel via `DriverLoad`
    /// (OIP-013 § S5.3 step 8).  Calling it from outside a correctly
    /// initialised driver process is unsound.  Use [`find_token_in_buf`]
    /// in host-side tests.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // In a driver _start: find the first MmioMap capability.
    /// let token_bytes: &[u8] =
    ///     omni_driver_shared::caps::find_token(
    ///         omni_driver_shared::ACTION_TAG_MMIO_MAP,
    ///         |_| true,
    ///     )
    ///     .expect("kernel deposited at least one MmioMap token");
    /// drop(token_bytes); // hand to syscall
    /// ```
    #[allow(
        unsafe_code,
        reason = "Two unsafe blocks: (1) header() validates magic/version via a \
                  repr(C) cast of the kernel-mapped page; (2) from_raw_parts constructs \
                  the full deposit slice.  Both are gated on the kernel guarantee that \
                  DRIVER_CAP_DEPOSIT_VA is mapped before _start runs (OIP-013 § S5.3 \
                  step 8).  No mutable aliasing: the kernel maps the region read-only."
    )]
    pub fn find_token(
        action_tag: u32,
        resource_predicate: impl Fn(&[u8]) -> bool,
    ) -> Option<&'static [u8]> {
        // Step 1 — validate the deposit header.
        //
        // SAFETY: In a correctly loaded OMNI driver process the kernel maps a
        // read-only 32 KiB window at DRIVER_CAP_DEPOSIT_VA before handing
        // control to _start (OIP-013 § S5.3 step 8).
        let hdr = unsafe { header() }.ok()?;

        // Convert entry_count from the validated header.  `u32::from_le` is a
        // no-op on little-endian (x86-64) but documents the intent.
        let entry_count = u32::from_le(hdr.entry_count) as usize;

        // Step 2 — construct the full deposit window slice.
        //
        // SAFETY: header() above confirmed the page is mapped and has a valid
        // magic/version header.  The same mapping covers DRIVER_CAP_DEPOSIT_LEN
        // bytes.  Pointer is page-aligned (0x0010_0000 % 4096 == 0); u8
        // alignment is trivially satisfied.  The 'static lifetime is sound
        // because the kernel never unmaps the deposit page.
        let buf: &'static [u8] = unsafe {
            core::slice::from_raw_parts(DRIVER_CAP_DEPOSIT_VA as *const u8, DRIVER_CAP_DEPOSIT_LEN)
        };

        // Step 3 — scan entries with the already-validated entry_count,
        // avoiding a redundant parse_header call inside scan_entries.
        scan_entries(buf, entry_count, action_tag, resource_predicate)
    }

    // -----------------------------------------------------------------------
    // Public API — find_token_in_buf (pure, testable)
    // -----------------------------------------------------------------------

    /// Pure-function variant of [`find_token`] that operates on a
    /// caller-supplied byte slice instead of the well-known deposit VA.
    ///
    /// Suitable for host-side tests and simulated driver environments.
    /// The returned slice is a subslice of `buf` with the same lifetime.
    ///
    /// Returns `None` if:
    /// - the header is invalid (magic / version mismatch),
    /// - no entry with `action_tag` exists,
    /// - all matching entries fail `resource_predicate`, or
    /// - any matching entry has an out-of-bounds `token_offset`/`token_len`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut page = vec![0u8; omni_driver_shared::DRIVER_CAP_DEPOSIT_LEN];
    /// // Write a minimal OMNICAPS header: magic + version=1 + entry_count=0
    /// let hdr: [u8; 16] = [
    ///     b'O', b'M', b'N', b'I', b'C', b'A', b'P', b'S', // magic
    ///     1, 0, 0, 0,  // version = 1 (LE)
    ///     0, 0, 0, 0,  // entry_count = 0 (LE)
    /// ];
    /// page[..16].copy_from_slice(&hdr);
    /// // With 0 entries, find_token_in_buf always returns None.
    /// assert!(
    ///     omni_driver_shared::caps::find_token_in_buf(
    ///         &page,
    ///         omni_driver_shared::ACTION_TAG_MMIO_MAP,
    ///         |_| true,
    ///     )
    ///     .is_none()
    /// );
    /// ```
    pub fn find_token_in_buf(
        buf: &[u8],
        action_tag: u32,
        resource_predicate: impl Fn(&[u8]) -> bool,
    ) -> Option<&[u8]> {
        // Parse and validate the header; convert any parse error to None.
        let entry_count = parse_header(buf).ok()? as usize;
        // Delegate to the shared scan loop.
        scan_entries(buf, entry_count, action_tag, resource_predicate)
    }
}

// ===========================================================================
// Unit tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::caps::{find_token_in_buf, parse_header};
    use super::*;
    use proptest::prelude::*;

    // -----------------------------------------------------------------------
    // Test helper: build a synthetic OMNICAPS buffer
    // -----------------------------------------------------------------------

    /// Build a `DRIVER_CAP_DEPOSIT_LEN`-byte buffer containing an OMNICAPS
    /// deposit with the given entries.
    ///
    /// Each entry is `(action_tag, resource_tag, token_bytes)`.
    /// Token blobs are placed immediately after the header + descriptor
    /// table (no padding alignment, which is intentional for tests — the
    /// parser does not require aligned offsets).
    fn build_omnicaps_buf(entries: &[(u32, u32, &[u8])]) -> Vec<u8> {
        let mut buf = vec![0u8; DRIVER_CAP_DEPOSIT_LEN];

        // Header
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        buf[8..12].copy_from_slice(&DEPOSIT_VERSION.to_le_bytes());
        let count = u32::try_from(entries.len()).unwrap();
        buf[12..16].copy_from_slice(&count.to_le_bytes());

        // Token blobs start right after header + descriptor table.
        let mut cursor = HEADER_LEN + entries.len() * ENTRY_DESCRIPTOR_LEN;

        for (i, (action_tag, resource_tag, token_bytes)) in entries.iter().enumerate() {
            let desc_base = HEADER_LEN + i * ENTRY_DESCRIPTOR_LEN;

            buf[desc_base..desc_base + 4].copy_from_slice(&action_tag.to_le_bytes());
            buf[desc_base + 4..desc_base + 8].copy_from_slice(&resource_tag.to_le_bytes());
            buf[desc_base + 8..desc_base + 12]
                .copy_from_slice(&u32::try_from(cursor).unwrap().to_le_bytes());
            buf[desc_base + 12..desc_base + 16]
                .copy_from_slice(&u32::try_from(token_bytes.len()).unwrap().to_le_bytes());

            buf[cursor..cursor + token_bytes.len()].copy_from_slice(token_bytes);
            cursor += token_bytes.len();
        }

        buf
    }

    // -----------------------------------------------------------------------
    // Header parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn header_parser_rejects_bad_magic() {
        // All-zero buffer → magic bytes are all 0x00, not b"OMNICAPS".
        let buf = [0u8; 32];
        assert_eq!(parse_header(&buf), Err(OmniCapsError::InvalidMagic));
    }

    #[test]
    fn header_parser_rejects_bad_magic_partial() {
        // Correct magic for first 7 bytes, wrong last byte.
        let mut buf = [0u8; 32];
        buf[0..7].copy_from_slice(b"OMNICAP");
        buf[7] = b'X'; // 'X' instead of 'S'
        assert_eq!(parse_header(&buf), Err(OmniCapsError::InvalidMagic));
    }

    #[test]
    fn header_parser_rejects_unsupported_version() {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        // version = 2 (unsupported)
        buf[8..12].copy_from_slice(&2u32.to_le_bytes());
        buf[12..16].copy_from_slice(&0u32.to_le_bytes()); // entry_count = 0
        assert_eq!(parse_header(&buf), Err(OmniCapsError::UnsupportedVersion));
    }

    #[test]
    fn header_parser_rejects_version_zero() {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        // version = 0 (unsupported)
        buf[8..12].copy_from_slice(&0u32.to_le_bytes());
        buf[12..16].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(parse_header(&buf), Err(OmniCapsError::UnsupportedVersion));
    }

    #[test]
    fn header_parser_accepts_valid_zero_entry_header() {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        buf[8..12].copy_from_slice(&DEPOSIT_VERSION.to_le_bytes());
        buf[12..16].copy_from_slice(&0u32.to_le_bytes()); // 0 entries
        assert_eq!(parse_header(&buf), Ok(0));
    }

    #[test]
    fn header_parser_rejects_entry_count_exceeded() {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        buf[8..12].copy_from_slice(&DEPOSIT_VERSION.to_le_bytes());
        // MAX_ENTRIES + 1 = 65
        let too_many = u32::try_from(MAX_ENTRIES + 1).unwrap();
        buf[12..16].copy_from_slice(&too_many.to_le_bytes());
        assert_eq!(parse_header(&buf), Err(OmniCapsError::EntryCountExceeded));
    }

    #[test]
    fn header_parser_accepts_max_entries() {
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        buf[8..12].copy_from_slice(&DEPOSIT_VERSION.to_le_bytes());
        let max = u32::try_from(MAX_ENTRIES).unwrap();
        buf[12..16].copy_from_slice(&max.to_le_bytes());
        // MAX_ENTRIES (64) is exactly the limit; should be accepted.
        assert_eq!(parse_header(&buf), Ok(u32::try_from(MAX_ENTRIES).unwrap()));
    }

    #[test]
    fn header_parser_rejects_too_short_buffer() {
        // Buffer shorter than HEADER_LEN (16) bytes.
        let buf = [0u8; 8];
        assert_eq!(parse_header(&buf), Err(OmniCapsError::InvalidMagic));
    }

    // -----------------------------------------------------------------------
    // find_token_in_buf tests
    // -----------------------------------------------------------------------

    #[test]
    fn find_token_locates_action_mmio_map() {
        // Build a page with one MmioMap entry carrying known token bytes.
        let token_data: &[u8] = b"fake-token-payload-mmio";
        let buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, token_data)]);

        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert_eq!(result, Some(token_data));
    }

    #[test]
    fn find_token_returns_none_for_unknown_action() {
        // Page has an MmioMap entry; search for an action_tag that isn't present.
        let token_data: &[u8] = b"fake-token-payload";
        let buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, token_data)]);

        // action_tag 99 does not exist in the deposit.
        let result = find_token_in_buf(&buf, 99, |_| true);
        assert!(result.is_none());
    }

    #[test]
    fn find_token_returns_none_on_empty_deposit() {
        let buf = build_omnicaps_buf(&[]);
        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(result.is_none());
    }

    #[test]
    fn find_token_skips_entries_where_predicate_returns_false() {
        // Both entries have the same action_tag; predicate rejects the first.
        let first: &[u8] = b"token-first";
        let second: &[u8] = b"token-second";
        let buf = build_omnicaps_buf(&[
            (ACTION_TAG_MMIO_MAP, 1, first),
            (ACTION_TAG_MMIO_MAP, 1, second),
        ]);

        // Predicate accepts only if the token contains b"second".
        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |t| {
            t.windows(6).any(|w| w == b"second")
        });
        assert_eq!(result, Some(second));
    }

    #[test]
    fn find_token_rejects_oob_offset() {
        // Build a page and then corrupt the token_offset to point past the end.
        let token_data: &[u8] = b"data";
        let mut buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, token_data)]);

        // token_offset is at descriptor[8..12].
        let desc_base = HEADER_LEN; // first descriptor starts at byte 16
        let offset_field = desc_base + 8;
        // Write an offset that is clearly past the end of the buffer.
        let bad_offset = u32::try_from(DRIVER_CAP_DEPOSIT_LEN + 1).unwrap();
        buf[offset_field..offset_field + 4].copy_from_slice(&bad_offset.to_le_bytes());

        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(result.is_none(), "out-of-bounds offset must yield None");
    }

    #[test]
    fn find_token_rejects_oob_len() {
        // Build a page and then corrupt token_len so offset+len overflows the buffer.
        let token_data: &[u8] = b"data";
        let mut buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, token_data)]);

        // token_len is at descriptor[12..16].
        let desc_base = HEADER_LEN;
        let len_field = desc_base + 12;
        // Write a length that would extend past the buffer even from offset 0.
        let bad_len = u32::try_from(DRIVER_CAP_DEPOSIT_LEN + 1).unwrap();
        buf[len_field..len_field + 4].copy_from_slice(&bad_len.to_le_bytes());

        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(result.is_none(), "out-of-bounds length must yield None");
    }

    #[test]
    fn find_token_multiple_action_types_selects_correct_one() {
        // Page has MmioMap, DmaMap, IrqAttach entries.
        let mmio_tok: &[u8] = b"mmio";
        let dma_tok: &[u8] = b"dma-window";
        let irq_tok: &[u8] = b"irq-line";
        let buf = build_omnicaps_buf(&[
            (ACTION_TAG_MMIO_MAP, 1, mmio_tok),
            (ACTION_TAG_DMA_MAP, 2, dma_tok),
            (ACTION_TAG_IRQ_ATTACH, 3, irq_tok),
        ]);

        assert_eq!(
            find_token_in_buf(&buf, ACTION_TAG_DMA_MAP, |_| true),
            Some(dma_tok)
        );
        assert_eq!(
            find_token_in_buf(&buf, ACTION_TAG_IRQ_ATTACH, |_| true),
            Some(irq_tok)
        );
    }

    // -----------------------------------------------------------------------
    // Property-based test — idempotency
    // -----------------------------------------------------------------------

    proptest! {
        /// `find_token_in_buf` is a pure function: calling it twice with the
        /// same arguments always returns the same result (no hidden mutable
        /// global state, no side effects).
        #[test]
        fn find_token_in_buf_is_idempotent(
            buf in proptest::collection::vec(any::<u8>(), 0..512_usize),
            action_tag: u32,
        ) {
            let r1 = find_token_in_buf(&buf, action_tag, |_| true);
            let r2 = find_token_in_buf(&buf, action_tag, |_| true);
            prop_assert_eq!(r1, r2,
                "find_token_in_buf must be deterministic: same input → same output");
        }
    }

    // -----------------------------------------------------------------------
    // OmniCapsError Display
    // -----------------------------------------------------------------------

    #[test]
    fn error_display_messages() {
        assert_eq!(
            OmniCapsError::InvalidMagic.to_string(),
            "invalid OMNICAPS magic"
        );
        assert_eq!(
            OmniCapsError::UnsupportedVersion.to_string(),
            "unsupported deposit version"
        );
        assert_eq!(
            OmniCapsError::EntryCountExceeded.to_string(),
            "entry count exceeds maximum"
        );
        assert_eq!(
            OmniCapsError::OutOfBoundsOffset.to_string(),
            "token offset or length is out of bounds"
        );
    }

    // =======================================================================
    // Additional tests added by the test engineer (TASK-003 coverage gaps)
    // =======================================================================

    // -----------------------------------------------------------------------
    // Group A — structural and boundary unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn find_token_rejects_descriptor_table_out_of_bounds() {
        // Build a 32-byte buffer that declares entry_count=5 even though only
        // one entry descriptor (bytes 16..32) fits within the buffer boundary.
        //
        // Iteration:
        //   i=0  desc_base=16  desc_end=32 → NOT > 32 → process entry 0
        //        action_tag=DmaMap (skipped, not MmioMap)
        //   i=1  desc_base=32  desc_end=48 → 48 > 32  → return None ← exercised
        let mut buf = [0u8; 32];
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        buf[8..12].copy_from_slice(&DEPOSIT_VERSION.to_le_bytes());
        buf[12..16].copy_from_slice(&5u32.to_le_bytes()); // 5 entries claimed
        // Entry 0 action_tag = DmaMap (not what we search for → continue to i=1)
        buf[16..20].copy_from_slice(&ACTION_TAG_DMA_MAP.to_le_bytes());
        // Entry 1 descriptor starts at byte 32 == buf.len(); desc_end=48 > 32 → return None.

        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(
            result.is_none(),
            "scan must return None when a descriptor table entry extends past the buffer boundary"
        );
    }

    #[test]
    fn find_token_locates_pci_cfg_read_action() {
        // ACTION_TAG_PCI_CFG_READ (4) was not exercised by the implementer's tests.
        let token_data: &[u8] = b"pci-cfg-read-token";
        let buf = build_omnicaps_buf(&[(ACTION_TAG_PCI_CFG_READ, 4, token_data)]);
        let result = find_token_in_buf(&buf, ACTION_TAG_PCI_CFG_READ, |_| true);
        assert_eq!(
            result,
            Some(token_data),
            "find_token_in_buf must locate a PciConfigRead entry"
        );
    }

    #[test]
    fn find_token_locates_pci_cfg_write_action() {
        // ACTION_TAG_PCI_CFG_WRITE (5) was not exercised by the implementer's tests.
        let token_data: &[u8] = b"pci-cfg-write-token";
        let buf = build_omnicaps_buf(&[(ACTION_TAG_PCI_CFG_WRITE, 4, token_data)]);
        let result = find_token_in_buf(&buf, ACTION_TAG_PCI_CFG_WRITE, |_| true);
        assert_eq!(
            result,
            Some(token_data),
            "find_token_in_buf must locate a PciConfigWrite entry"
        );
    }

    #[test]
    fn find_token_returns_empty_slice_for_zero_length_token() {
        // An entry with token_len=0 is valid: `buf.get(offset..offset)` returns
        // `Some(&[])`.  The predicate is called with an empty slice, and when it
        // accepts, the function must return `Some(&[])`.
        let buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, &[])]);
        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert_eq!(
            result,
            Some(&[][..]),
            "zero-length token entry must return Some(&[]) when predicate accepts"
        );
    }

    #[test]
    fn find_token_returns_first_matching_when_multiple_present() {
        // With three entries sharing the same action_tag, the predicate accepting
        // all of them, the first entry (not the second or third) must be returned.
        let first: &[u8] = b"alpha";
        let second: &[u8] = b"beta";
        let third: &[u8] = b"gamma";
        let buf = build_omnicaps_buf(&[
            (ACTION_TAG_MMIO_MAP, 1, first),
            (ACTION_TAG_MMIO_MAP, 1, second),
            (ACTION_TAG_MMIO_MAP, 1, third),
        ]);
        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert_eq!(
            result,
            Some(first),
            "find_token must return the first entry that matches action_tag and predicate"
        );
    }

    #[test]
    fn find_token_token_fits_exactly_at_buffer_end() {
        // Place a token whose last byte is the very last byte of the deposit window.
        // `token_offset + token_len == DRIVER_CAP_DEPOSIT_LEN` exactly.
        // `buf.get(offset..end)` where `end == buf.len()` is a valid (non-empty) range;
        // the function must return `Some(token_data)`.
        let token_data: &[u8] = b"fin!";
        let token_offset = DRIVER_CAP_DEPOSIT_LEN - token_data.len();
        let mut buf = vec![0u8; DRIVER_CAP_DEPOSIT_LEN];
        buf[0..8].copy_from_slice(&DEPOSIT_MAGIC);
        buf[8..12].copy_from_slice(&DEPOSIT_VERSION.to_le_bytes());
        buf[12..16].copy_from_slice(&1u32.to_le_bytes());
        let desc_base = HEADER_LEN;
        buf[desc_base..desc_base + 4].copy_from_slice(&ACTION_TAG_MMIO_MAP.to_le_bytes());
        buf[desc_base + 4..desc_base + 8].copy_from_slice(&1u32.to_le_bytes()); // resource_tag
        buf[desc_base + 8..desc_base + 12]
            .copy_from_slice(&u32::try_from(token_offset).unwrap().to_le_bytes());
        buf[desc_base + 12..desc_base + 16]
            .copy_from_slice(&u32::try_from(token_data.len()).unwrap().to_le_bytes());
        buf[token_offset..].copy_from_slice(token_data);

        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert_eq!(
            result,
            Some(token_data),
            "token whose last byte is exactly the last buffer byte must be returned (not OOB)"
        );
    }

    #[test]
    fn error_debug_format_is_non_empty_for_all_variants() {
        // The `#[derive(Debug)]` on `OmniCapsError` must produce a non-empty string
        // for all four variants.  This exercises the derived impl and guards against
        // accidental removal or renaming that would break downstream `{:?}` users.
        let variants = [
            OmniCapsError::InvalidMagic,
            OmniCapsError::UnsupportedVersion,
            OmniCapsError::EntryCountExceeded,
            OmniCapsError::OutOfBoundsOffset,
        ];
        for variant in &variants {
            let debug_str = format!("{variant:?}");
            assert!(
                !debug_str.is_empty(),
                "Debug format for OmniCapsError::{variant:?} must not be empty"
            );
        }
    }

    #[test]
    fn find_token_in_buf_u32_max_action_tag_returns_none() {
        // u32::MAX is not a defined action tag; a deposit containing only
        // ACTION_TAG_MMIO_MAP must return None for a u32::MAX query.
        let token_data: &[u8] = b"some-token";
        let buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, token_data)]);
        let result = find_token_in_buf(&buf, u32::MAX, |_| true);
        assert!(
            result.is_none(),
            "u32::MAX action_tag must return None when that tag is not present in the deposit"
        );
    }

    #[test]
    fn find_token_with_full_64_entry_scan_all_non_matching_returns_none() {
        // Build a deposit with exactly MAX_ENTRIES (64) entries, all with
        // ACTION_TAG_DMA_MAP.  Querying for ACTION_TAG_MMIO_MAP must scan the
        // entire table and return None.  This exercises the loop body at full depth.
        let empty: &[u8] = &[];
        let entries: Vec<(u32, u32, &[u8])> = vec![(ACTION_TAG_DMA_MAP, 2, empty); MAX_ENTRIES];
        let buf = build_omnicaps_buf(&entries);
        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(
            result.is_none(),
            "must return None after scanning all {MAX_ENTRIES} entries with no action_tag match"
        );
    }

    // -----------------------------------------------------------------------
    // Group B — adversarial / security tests
    // -----------------------------------------------------------------------

    #[test]
    fn adversarial_all_ff_buffer_returns_none_without_panic() {
        // A full 32 KiB buffer of 0xFF is a maximally adversarial input.
        // The first 8 bytes are 0xFF, which does not equal b"OMNICAPS",
        // so parse_header returns InvalidMagic → None.  No panic may occur.
        let buf = vec![0xFFu8; DRIVER_CAP_DEPOSIT_LEN];
        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(
            result.is_none(),
            "all-0xFF buffer must return None (magic check must reject it); must not panic"
        );
    }

    #[test]
    fn adversarial_token_offset_u32_max_returns_none_without_panic() {
        // Corrupt token_offset to u32::MAX (4 294 967 295).
        // As usize on x86-64: token_end = 4 294 967 295 + token_len.
        // checked_add succeeds (no usize overflow on 64-bit); the result is
        // >> DRIVER_CAP_DEPOSIT_LEN → OOB → continue (silently skip) → None.
        let token_data: &[u8] = b"data";
        let mut buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, token_data)]);
        let offset_field = HEADER_LEN + 8; // token_offset at bytes [8..12] of the first descriptor
        buf[offset_field..offset_field + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(
            result.is_none(),
            "token_offset=u32::MAX must yield None without panic (OOB skip path)"
        );
    }

    #[test]
    fn adversarial_token_len_u32_max_returns_none_without_panic() {
        // Corrupt token_len to u32::MAX (4 294 967 295).
        // token_offset is the small value set by the builder (< 1 KiB).
        // checked_add produces a value far beyond DRIVER_CAP_DEPOSIT_LEN
        // → OOB → continue → None.  No panic may occur.
        let token_data: &[u8] = b"data";
        let mut buf = build_omnicaps_buf(&[(ACTION_TAG_MMIO_MAP, 1, token_data)]);
        let len_field = HEADER_LEN + 12; // token_len at bytes [12..16] of the first descriptor
        buf[len_field..len_field + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(
            result.is_none(),
            "token_len=u32::MAX must yield None without panic (OOB skip path)"
        );
    }

    #[test]
    fn adversarial_zero_byte_buffer_returns_none_without_panic() {
        // An empty slice is shorter than HEADER_LEN (16 bytes).
        // parse_header returns InvalidMagic immediately → ok()? → None.
        let result = find_token_in_buf(&[], ACTION_TAG_MMIO_MAP, |_| true);
        assert!(
            result.is_none(),
            "zero-byte buffer must return None without panic"
        );
    }

    #[test]
    fn adversarial_exactly_header_len_minus_one_returns_none_without_panic() {
        // A 15-byte buffer (HEADER_LEN - 1) is one byte short of the minimum
        // needed to read the header.  The `buf.len() < HEADER_LEN` guard in
        // parse_header fires first → InvalidMagic → None.
        let buf = vec![0u8; HEADER_LEN - 1];
        let result = find_token_in_buf(&buf, ACTION_TAG_MMIO_MAP, |_| true);
        assert!(
            result.is_none(),
            "15-byte buffer (HEADER_LEN-1) must return None (minimum-length guard in parse_header)"
        );
    }

    // -----------------------------------------------------------------------
    // Group C — extended property-based test
    // -----------------------------------------------------------------------

    proptest! {
        /// `find_token_in_buf` must **never panic** on any input, regardless of
        /// buffer size (up to the full 32 KiB deposit window) or `action_tag` value.
        ///
        /// This is distinct from the idempotency proptest above: the guarantee here
        /// is "no panic under any input", not merely "deterministic output on a fixed
        /// input".  The test exercises `parse_header`, `scan_entries`, and
        /// `read_u32_le` at realistic buffer sizes (not just up to 512 bytes).
        #[test]
        fn proptest_no_panic_on_arbitrary_buf_up_to_deposit_len(
            buf in proptest::collection::vec(any::<u8>(), 0..=DRIVER_CAP_DEPOSIT_LEN),
            action_tag: u32,
        ) {
            // If this call panics, proptest catches it and reports a failure.
            // The absence of panic IS the assertion — no prop_assert needed.
            let _ = find_token_in_buf(&buf, action_tag, |_| true);
        }
    }
}
