//! # On-disk volume implementation
//!
//! Implements [`OnDiskVolume`] — a byte-buffer-backed `OmniFS` volume that
//! serialises the full on-disk format described in OIP-FS-Wire-023:
//!
//! | Region | Blocks | Description |
//! |--------|--------|-------------|
//! | Superblock | 0 | Magic, version, geometry, free-space counters |
//! | Bitmap | 1..=N | Free-space bitmap (1 bit per block) |
//! | Inode table | N+1..=M | Serialised inodes (variable length, 1 block) |
//! | Data | M+1..end-1 | Raw 4 KiB data blocks |
//! | Integrity | last | 24-byte entries: (`block_number` u64 LE + 16-byte tag) |
//!
//! The in-memory representation keeps all data in `BTreeMap` structures for
//! O(log n) access.  [`OnDiskVolume::sync_to_bytes`] serialises the entire
//! volume state to a flat byte buffer and [`OnDiskVolume::mount`] restores it.
//!
//! ## `CoW` write path
//!
//! Every `write_file` call follows the `CoW` contract:
//! 1. Allocate a new block from the bitmap.
//! 2. Write data to the new block.
//! 3. Update the inode pointer to the new block.
//! 4. Free the old block (if any).
//!
//! This ensures the volume is always in a consistent state: either the old
//! block pointers are valid (crash before step 3) or the new ones are valid
//! (crash after step 3).

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::allocator::BlockBitmap;
use crate::integrity::{BlockKey, TAG_LEN, compute_tag, verify_tag};
use crate::{FileMetadata, FileType, FsError, Inode, OMNI_FS_MAGIC, OMNI_FS_VERSION, Superblock};

// =============================================================================
// Constants
// =============================================================================

/// Block size in bytes — must match [`omni_types::blk::BLOCK_SIZE_BYTES`].
const BLOCK_SIZE: usize = 4096;

/// Each on-disk integrity entry is 8 bytes (`block_number`) + 16 bytes (tag).
const INTEGRITY_ENTRY_SIZE: usize = 8 + TAG_LEN;

/// Maximum number of integrity entries that fit in one 4 KiB block.
///
/// Used as a compile-time upper bound check in Phase 3 when the integrity
/// region may need to span multiple blocks.
#[allow(dead_code, clippy::integer_division)]
const INTEGRITY_ENTRIES_PER_BLOCK: usize = BLOCK_SIZE / INTEGRITY_ENTRY_SIZE;

// =============================================================================
// FsckError
// =============================================================================

/// A single inconsistency detected by the filesystem checker.
///
/// Produced by [`OnDiskVolume::fsck`] and collected in [`FsckReport::errors`].
///
/// # Example
///
/// ```rust
/// use omni_fs::ondisk::FsckError;
///
/// let e = FsckError::BadMagic;
/// assert!(matches!(e, FsckError::BadMagic));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FsckError {
    /// Superblock magic number does not equal `b"OMNIFS01"`.
    BadMagic,
    /// Superblock format version is not 1.
    BadVersion,
    /// Superblock `free_blocks` counter does not match the bitmap count.
    FreeBlockMismatch {
        /// Value stored in the superblock.
        superblock: u64,
        /// Value computed by scanning the bitmap.
        bitmap: u64,
    },
    /// An inode references a block that the bitmap shows as unallocated.
    UnallocatedBlockReferenced {
        /// Inode number of the referencing inode.
        inode_number: u64,
        /// Block number that was referenced but not marked allocated.
        block_number: u64,
    },
    /// A data block's stored AEAD tag does not match the computed tag.
    IntegrityViolation {
        /// Block number whose tag did not verify.
        block_number: u64,
    },
}

// =============================================================================
// FsckReport
// =============================================================================

/// Summary report produced by [`OnDiskVolume::fsck`].
///
/// Call [`FsckReport::is_clean`] to determine whether the volume is
/// free of all detected inconsistencies.
///
/// # Example
///
/// ```rust
/// use omni_fs::ondisk::{FsckReport, FsckError};
///
/// let report = FsckReport {
///     superblock_valid: true,
///     bitmap_consistent: true,
///     total_blocks: 1024,
///     free_blocks: 1023,
///     inode_count: 1,
///     errors: vec![],
/// };
/// assert!(report.is_clean());
/// ```
#[derive(Debug, Clone)]
pub struct FsckReport {
    /// `true` if the superblock magic and version are valid.
    pub superblock_valid: bool,
    /// `true` if the bitmap free count matches `superblock.free_blocks`.
    pub bitmap_consistent: bool,
    /// Total block count as read from the superblock.
    pub total_blocks: u64,
    /// Free block count as computed by the bitmap scanner.
    pub free_blocks: u64,
    /// Number of allocated inodes as read from the superblock.
    pub inode_count: u64,
    /// All inconsistencies detected by the checker.
    pub errors: Vec<FsckError>,
}

impl FsckReport {
    /// Return `true` if no errors were detected (clean volume).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::FsckReport;
    ///
    /// let clean = FsckReport {
    ///     superblock_valid: true,
    ///     bitmap_consistent: true,
    ///     total_blocks: 512,
    ///     free_blocks: 511,
    ///     inode_count: 1,
    ///     errors: vec![],
    /// };
    /// assert!(clean.is_clean());
    /// ```
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

// =============================================================================
// OnDiskVolume
// =============================================================================

/// A byte-buffer-backed `OmniFS` volume.
///
/// Stores the filesystem in memory using `BTreeMap` structures and can
/// serialise/deserialise to/from a flat byte buffer via
/// [`OnDiskVolume::sync_to_bytes`] / [`OnDiskVolume::mount`].
///
/// # Example
///
/// ```rust
/// use omni_fs::ondisk::OnDiskVolume;
///
/// let mut vol = OnDiskVolume::format(128);
/// assert_eq!(vol.superblock().total_blocks, 128);
/// vol.create_file("/hello.txt").expect("create");
/// vol.write_file("/hello.txt", 0, b"hello").expect("write");
/// let data = vol.read_file("/hello.txt", 0, 5).expect("read");
/// assert_eq!(data, b"hello");
/// ```
#[derive(Debug)]
pub struct OnDiskVolume {
    /// Volume metadata.
    superblock: Superblock,
    /// Free-space bitmap.
    bitmap: BlockBitmap,
    /// Inode table keyed by inode number.
    inodes: BTreeMap<u64, Inode>,
    /// Path → inode number lookup.
    path_map: BTreeMap<String, u64>,
    /// Physical block data keyed by 1-based block number.
    data_blocks: BTreeMap<u64, Vec<u8>>,
    /// Per-block AEAD tags keyed by 1-based block number.
    integrity_tags: BTreeMap<u64, [u8; TAG_LEN]>,
    /// AEAD key for tag computation (Phase-2 stub: all-zero).
    block_key: BlockKey,
    /// Next inode number to mint.
    next_inode: u64,
}

impl OnDiskVolume {
    // -------------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------------

    /// Format a new empty volume with `total_blocks` 4 KiB blocks.
    ///
    /// Block 0 is the superblock.  Block `total_blocks - 1` is reserved
    /// for the integrity tag region.  All remaining blocks are available
    /// for data.  The root directory inode is pre-created at inode number 1.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    /// use omni_fs::OMNI_FS_MAGIC;
    ///
    /// let vol = OnDiskVolume::format(256);
    /// assert_eq!(vol.superblock().magic, OMNI_FS_MAGIC);
    /// assert_eq!(vol.superblock().version, 1);
    /// assert_eq!(vol.superblock().total_blocks, 256);
    /// ```
    #[must_use]
    pub fn format(total_blocks: u64) -> Self {
        // Create the bitmap for all total_blocks blocks (1-based externally).
        // Pre-allocate three structural blocks so that bitmap.free_count()
        // exactly equals superblock.free_blocks:
        //   block 1 = bitmap storage block (on-disk)
        //   block 2 = inode table block (on-disk)
        //   block total_blocks = integrity tag region (last physical block)
        // Block 0 is the superblock; it is below the 1-based range so the
        // bitmap does not track it (it is always "in use" implicitly).
        let mut bitmap = BlockBitmap::new(total_blocks);

        // Pre-allocate blocks 1 (bitmap) and 2 (inodes) with first-fit.
        let _ = bitmap.allocate(); // block 1
        let _ = bitmap.allocate(); // block 2
        // Mark the last block (integrity region) as allocated directly.
        // `total_blocks` is the last 1-based block number.
        bitmap.mark_allocated(total_blocks);

        let free_blocks = bitmap.free_count();

        let superblock = Superblock {
            magic: OMNI_FS_MAGIC,
            version: OMNI_FS_VERSION,
            block_size: 4096,
            total_blocks,
            free_blocks,
            inode_count: 1, // root directory
            root_inode: crate::ROOT_INODE_NUMBER,
            created_at: 0,
            aead_key_id: 0,
        };

        // Pre-create the root directory inode.
        let root_inode = Inode {
            inode_number: crate::ROOT_INODE_NUMBER,
            file_type: FileType::Directory,
            size: 0,
            block_count: 0,
            created: 0,
            modified: 0,
            blocks: Vec::new(),
            name: String::from("/"),
        };

        let mut inodes = BTreeMap::new();
        inodes.insert(crate::ROOT_INODE_NUMBER, root_inode);

        let mut path_map = BTreeMap::new();
        path_map.insert(String::from("/"), crate::ROOT_INODE_NUMBER);

        Self {
            superblock,
            bitmap,
            inodes,
            path_map,
            data_blocks: BTreeMap::new(),
            integrity_tags: BTreeMap::new(),
            block_key: BlockKey::zeroed(),
            next_inode: crate::FIRST_USER_INODE,
        }
    }

    /// Mount a volume from a raw byte buffer produced by [`OnDiskVolume::sync_to_bytes`].
    ///
    /// Parses the superblock from block 0, restores the bitmap, inodes, data
    /// blocks, and integrity tags.
    ///
    /// # Errors
    ///
    /// - [`FsError::IntegrityViolation`] if the superblock magic or version
    ///   is invalid.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let vol = OnDiskVolume::format(64);
    /// let raw = vol.sync_to_bytes();
    /// let mounted = OnDiskVolume::mount(&raw).expect("mount");
    /// assert_eq!(mounted.superblock().total_blocks, 64);
    /// ```
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        reason = "all slice accesses are guarded by raw_bytes.len() checks; total_blocks fits in usize on 64-bit targets"
    )]
    pub fn mount(raw_bytes: &[u8]) -> Result<Self, FsError> {
        if raw_bytes.len() < BLOCK_SIZE {
            return Err(FsError::IntegrityViolation);
        }

        // Parse superblock from block 0 bytes.
        let sb = parse_superblock(&raw_bytes[..BLOCK_SIZE])?;

        let total_blocks = sb.total_blocks as usize;
        if raw_bytes.len() < total_blocks * BLOCK_SIZE {
            return Err(FsError::IntegrityViolation);
        }

        // Restore bitmap from block 1.
        let bitmap = {
            let bitmap_start = BLOCK_SIZE;
            let bitmap_end = (bitmap_start + BLOCK_SIZE).min(raw_bytes.len());
            BlockBitmap::from_bytes(&raw_bytes[bitmap_start..bitmap_end], sb.total_blocks)
                .ok_or(FsError::IntegrityViolation)?
        };

        // Restore inodes from block 2.
        let (inodes, path_map, next_inode) = {
            let inode_start = 2 * BLOCK_SIZE;
            let inode_end = (inode_start + BLOCK_SIZE).min(raw_bytes.len());
            deserialize_inodes(&raw_bytes[inode_start..inode_end])
        };

        // Restore integrity tags from the last block.
        let integrity_tags = if total_blocks >= 1 {
            let tag_block_start = (total_blocks - 1) * BLOCK_SIZE;
            let tag_block_end = tag_block_start + BLOCK_SIZE;
            if tag_block_end <= raw_bytes.len() {
                deserialize_integrity_tags(&raw_bytes[tag_block_start..tag_block_end])
            } else {
                BTreeMap::new()
            }
        } else {
            BTreeMap::new()
        };

        // Restore data blocks from blocks 3..total_blocks-1.
        let data_blocks = if total_blocks > 4 {
            let data_start = 3 * BLOCK_SIZE;
            let data_end = (total_blocks - 1) * BLOCK_SIZE;
            deserialize_data_blocks(raw_bytes, data_start, data_end, &bitmap, sb.total_blocks)
        } else {
            BTreeMap::new()
        };

        Ok(Self {
            superblock: sb,
            bitmap,
            inodes,
            path_map,
            data_blocks,
            integrity_tags,
            block_key: BlockKey::zeroed(),
            next_inode,
        })
    }

    // -------------------------------------------------------------------------
    // Filesystem consistency checker
    // -------------------------------------------------------------------------

    /// Run the filesystem consistency checker and return a [`FsckReport`].
    ///
    /// Checks:
    /// 1. Superblock magic and version.
    /// 2. Bitmap `free_count` matches `superblock.free_blocks`.
    /// 3. Every block referenced by an inode is marked allocated in the bitmap.
    /// 4. Every data block's stored AEAD tag verifies (Phase-2: all-zero tags).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let vol = OnDiskVolume::format(64);
    /// let report = vol.fsck();
    /// assert!(report.is_clean());
    /// ```
    #[must_use]
    pub fn fsck(&self) -> FsckReport {
        let mut errors = Vec::new();

        // Check 1: superblock validity.
        let superblock_valid =
            self.superblock.magic == OMNI_FS_MAGIC && self.superblock.version == OMNI_FS_VERSION;
        if !superblock_valid {
            if self.superblock.magic != OMNI_FS_MAGIC {
                errors.push(FsckError::BadMagic);
            }
            if self.superblock.version != OMNI_FS_VERSION {
                errors.push(FsckError::BadVersion);
            }
        }

        // Check 2: bitmap consistency.
        let bitmap_free = self.bitmap.free_count();
        let bitmap_consistent = bitmap_free == self.superblock.free_blocks;
        if !bitmap_consistent {
            errors.push(FsckError::FreeBlockMismatch {
                superblock: self.superblock.free_blocks,
                bitmap: bitmap_free,
            });
        }

        // Check 3: all inode block references are allocated.
        for (inode_number, inode) in &self.inodes {
            for &block_num in &inode.blocks {
                if !self.bitmap.is_allocated(block_num) {
                    errors.push(FsckError::UnallocatedBlockReferenced {
                        inode_number: *inode_number,
                        block_number: block_num,
                    });
                }
            }
        }

        // Check 4: data block integrity tags.
        for (&block_num, data) in &self.data_blocks {
            if let Some(stored_tag) = self.integrity_tags.get(&block_num) {
                if verify_tag(&self.block_key, block_num, data, stored_tag).is_err() {
                    errors.push(FsckError::IntegrityViolation {
                        block_number: block_num,
                    });
                }
            }
            // If no tag is stored for a block, we do not flag it in Phase 2
            // (Phase-3 will require tags for all blocks).
        }

        FsckReport {
            superblock_valid,
            bitmap_consistent,
            total_blocks: self.superblock.total_blocks,
            free_blocks: bitmap_free,
            inode_count: self.superblock.inode_count,
            errors,
        }
    }

    // -------------------------------------------------------------------------
    // Serialisation
    // -------------------------------------------------------------------------

    /// Serialise the entire volume state to a flat byte buffer.
    ///
    /// The layout is:
    /// - Block 0: superblock (4 KiB, zero-padded).
    /// - Block 1: bitmap (4 KiB, zero-padded).
    /// - Block 2: inode table (4 KiB, zero-padded).
    /// - Blocks 3..N-1: data blocks.
    /// - Block N-1: integrity tags.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let vol = OnDiskVolume::format(32);
    /// let raw = vol.sync_to_bytes();
    /// assert_eq!(raw.len(), 32 * 4096);
    /// ```
    #[must_use]
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        reason = "all slice ranges are within the allocated out buffer (total * BLOCK_SIZE); OMNI OS targets 64-bit so usize cast is safe"
    )]
    pub fn sync_to_bytes(&self) -> Vec<u8> {
        let total = self.superblock.total_blocks as usize;
        let mut out = vec![0u8; total * BLOCK_SIZE];

        // Block 0: superblock.
        write_superblock_to_block(&self.superblock, &mut out[..BLOCK_SIZE]);

        // Block 1: bitmap.
        if total > 1 {
            let bm = self.bitmap.as_bytes();
            let copy_len = bm.len().min(BLOCK_SIZE);
            out[BLOCK_SIZE..BLOCK_SIZE + copy_len].copy_from_slice(&bm[..copy_len]);
        }

        // Block 2: inode table.
        if total > 2 {
            let inode_bytes = serialize_inodes(&self.inodes);
            let copy_len = inode_bytes.len().min(BLOCK_SIZE);
            out[2 * BLOCK_SIZE..2 * BLOCK_SIZE + copy_len]
                .copy_from_slice(&inode_bytes[..copy_len]);
        }

        // Blocks 3..N-1: data.
        if total > 4 {
            for (&block_num, data) in &self.data_blocks {
                // Map block numbers to physical positions starting at block 3.
                // We use a compact sequential layout: the n-th data block
                // (sorted by block number) occupies physical block 3+n.
                let phys = self.block_num_to_physical(block_num);
                if phys > 0 && phys < total - 1 {
                    let start = phys * BLOCK_SIZE;
                    let copy_len = data.len().min(BLOCK_SIZE);
                    out[start..start + copy_len].copy_from_slice(&data[..copy_len]);
                }
            }
        }

        // Block N-1: integrity tags.
        if total >= 1 {
            let tag_start = (total - 1) * BLOCK_SIZE;
            write_integrity_tags(
                &self.integrity_tags,
                &mut out[tag_start..tag_start + BLOCK_SIZE],
            );
        }

        out
    }

    // -------------------------------------------------------------------------
    // CRUD operations
    // -------------------------------------------------------------------------

    /// Create a new empty regular file at `path`.
    ///
    /// # Errors
    ///
    /// - [`FsError::FileAlreadyExists`] if `path` is already in use.
    /// - [`FsError::PathTooLong`] if `path.len() > MAX_PATH_LEN`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    /// use omni_fs::FsError;
    ///
    /// let mut vol = OnDiskVolume::format(64);
    /// vol.create_file("/model.gguf").expect("create");
    /// assert_eq!(vol.create_file("/model.gguf"), Err(FsError::FileAlreadyExists));
    /// ```
    pub fn create_file(&mut self, path: &str) -> Result<u64, FsError> {
        if path.len() > crate::MAX_PATH_LEN {
            return Err(FsError::PathTooLong);
        }
        if self.path_map.contains_key(path) {
            return Err(FsError::FileAlreadyExists);
        }

        let name = basename(path);
        let inode_number = self.next_inode;
        self.next_inode = self.next_inode.wrapping_add(1);

        let inode = Inode {
            inode_number,
            file_type: FileType::RegularFile,
            size: 0,
            block_count: 0,
            created: 0,
            modified: 0,
            blocks: Vec::new(),
            name: String::from(name),
        };

        self.inodes.insert(inode_number, inode);
        self.path_map.insert(String::from(path), inode_number);
        self.superblock.inode_count = self.superblock.inode_count.wrapping_add(1);

        Ok(inode_number)
    }

    /// Write `data` bytes into the file at `path`, starting at byte `offset`.
    ///
    /// Uses `CoW` semantics: each block affected by the write is allocated fresh,
    /// data is written to the new block, the inode pointer is updated, and the
    /// old block is freed.
    ///
    /// Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// - [`FsError::FileNotFound`] if `path` does not exist.
    /// - [`FsError::NotAFile`] if `path` is a directory.
    /// - [`FsError::NoSpace`] if the volume has no free blocks.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let mut vol = OnDiskVolume::format(128);
    /// vol.create_file("/weights.bin").expect("create");
    /// let n = vol.write_file("/weights.bin", 0, &[0xFFu8; 8]).expect("write");
    /// assert_eq!(n, 8);
    /// ```
    #[allow(
        clippy::integer_division,
        clippy::cast_possible_truncation,
        clippy::indexing_slicing,
        reason = "block index arithmetic requires floor division; OMNI OS targets 64-bit only"
    )]
    pub fn write_file(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<usize, FsError> {
        let inode_number = self
            .path_map
            .get(path)
            .copied()
            .ok_or(FsError::FileNotFound)?;

        {
            let inode = self
                .inodes
                .get(&inode_number)
                .ok_or(FsError::FileNotFound)?;
            if inode.file_type != FileType::RegularFile {
                return Err(FsError::NotAFile);
            }
        }

        if data.is_empty() {
            return Ok(0);
        }

        let block_size = BLOCK_SIZE as u64;
        let end_offset = offset.saturating_add(data.len() as u64);
        let first_block_idx = offset / block_size;
        let last_block_idx = end_offset.saturating_sub(1) / block_size;

        // Collect old block numbers before mutation.
        let old_blocks: Vec<u64> = {
            let inode = self
                .inodes
                .get(&inode_number)
                .ok_or(FsError::FileNotFound)?;
            inode.blocks.clone()
        };

        // Allocate new blocks for each block index in [first..last].
        let mut new_blocks: Vec<(u64, u64)> = Vec::new(); // (block_idx, new_block_num)
        for blk_idx in first_block_idx..=last_block_idx {
            let new_blk = self.bitmap.allocate().ok_or(FsError::NoSpace)?;
            new_blocks.push((blk_idx, new_blk));
        }

        // Write data into the new blocks, copying any unmodified prefix/suffix
        // from old blocks into the new ones (read-modify-write for partial blocks).
        let mut bytes_written = 0usize;
        for &(blk_idx, new_blk) in &new_blocks {
            // block_start_in_file is implicit: blk_idx * block_size (not used directly)
            let write_start = if blk_idx == first_block_idx {
                (offset % block_size) as usize
            } else {
                0
            };
            let write_end = if blk_idx == last_block_idx {
                ((end_offset - 1) % block_size + 1) as usize
            } else {
                BLOCK_SIZE
            };

            // Seed the new block from the old block (read-modify-write).
            let mut block_data = if let Some(&old_blk) = old_blocks.get(blk_idx as usize) {
                if old_blk != 0 {
                    self.data_blocks
                        .get(&old_blk)
                        .cloned()
                        .unwrap_or_else(|| vec![0u8; BLOCK_SIZE])
                } else {
                    vec![0u8; BLOCK_SIZE]
                }
            } else {
                vec![0u8; BLOCK_SIZE]
            };
            if block_data.len() < BLOCK_SIZE {
                block_data.resize(BLOCK_SIZE, 0u8);
            }

            // Write the data slice into the appropriate region.
            let data_slice = &data[bytes_written..bytes_written + (write_end - write_start)];
            block_data[write_start..write_end].copy_from_slice(data_slice);
            bytes_written += write_end - write_start;

            // Compute and store the integrity tag.
            let tag = compute_tag(&self.block_key, new_blk, &block_data);
            self.integrity_tags.insert(new_blk, tag);

            // Store the new block data.
            self.data_blocks.insert(new_blk, block_data);
        }

        // Update inode block pointers: replace/extend with new blocks.
        {
            let inode = self
                .inodes
                .get_mut(&inode_number)
                .ok_or(FsError::FileNotFound)?;

            // Extend the blocks Vec if necessary.
            while inode.blocks.len() as u64 <= last_block_idx {
                inode.blocks.push(0);
            }

            for &(blk_idx, new_blk) in &new_blocks {
                inode.blocks[blk_idx as usize] = new_blk;
            }

            inode.block_count = inode.blocks.iter().filter(|&&b| b != 0).count() as u32;
            if end_offset > inode.size {
                inode.size = end_offset;
            }
        }

        // CoW: free the old blocks that were replaced.
        for &(blk_idx, _) in &new_blocks {
            if let Some(&old_blk) = old_blocks.get(blk_idx as usize) {
                if old_blk != 0 {
                    self.data_blocks.remove(&old_blk);
                    self.integrity_tags.remove(&old_blk);
                    self.bitmap.free(old_blk);
                }
            }
        }

        // Update superblock free_blocks to match bitmap.
        self.superblock.free_blocks = self.bitmap.free_count();

        Ok(bytes_written)
    }

    /// Read up to `count` bytes from the file at `path` starting at `offset`.
    ///
    /// If the requested range exceeds EOF, only bytes up to EOF are returned
    /// (short read, no error). Verifies the AEAD tag for each block read.
    ///
    /// # Errors
    ///
    /// - [`FsError::FileNotFound`] if `path` does not exist.
    /// - [`FsError::NotAFile`] if `path` is a directory.
    /// - [`FsError::IntegrityViolation`] if any block's tag does not verify.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let mut vol = OnDiskVolume::format(64);
    /// vol.create_file("/data.bin").expect("create");
    /// vol.write_file("/data.bin", 0, b"abcde").expect("write");
    /// let out = vol.read_file("/data.bin", 1, 3).expect("read");
    /// assert_eq!(out, b"bcd");
    /// ```
    #[allow(
        clippy::integer_division,
        clippy::cast_possible_truncation,
        clippy::indexing_slicing,
        reason = "block index arithmetic requires floor division; OMNI OS targets 64-bit only"
    )]
    pub fn read_file(&self, path: &str, offset: u64, count: u32) -> Result<Vec<u8>, FsError> {
        let inode_number = self
            .path_map
            .get(path)
            .copied()
            .ok_or(FsError::FileNotFound)?;

        let inode = self
            .inodes
            .get(&inode_number)
            .ok_or(FsError::FileNotFound)?;

        if inode.file_type != FileType::RegularFile {
            return Err(FsError::NotAFile);
        }

        if offset >= inode.size || count == 0 {
            return Ok(Vec::new());
        }

        let block_size = BLOCK_SIZE as u64;
        let effective_end = (offset + u64::from(count)).min(inode.size);
        let read_len = (effective_end - offset) as usize;
        let mut result = vec![0u8; read_len];

        let first_block_idx = offset / block_size;
        let last_block_idx = (effective_end - 1) / block_size;
        let mut bytes_read = 0usize;

        for abs_block_idx in first_block_idx..=last_block_idx {
            let block_num = inode
                .blocks
                .get(abs_block_idx as usize)
                .copied()
                .unwrap_or(0);

            let block_start = if abs_block_idx == first_block_idx {
                (offset % block_size) as usize
            } else {
                0
            };
            let block_end = if abs_block_idx == last_block_idx {
                ((effective_end - 1) % block_size + 1) as usize
            } else {
                BLOCK_SIZE
            };
            let copy_len = block_end - block_start;

            if block_num == 0 {
                // Sparse block: zeroes already in result.
            } else if let Some(block) = self.data_blocks.get(&block_num) {
                // Verify integrity tag before returning data.
                if let Some(stored_tag) = self.integrity_tags.get(&block_num) {
                    verify_tag(&self.block_key, block_num, block, stored_tag)?;
                }
                let src_end = block_end.min(block.len());
                if block_start < src_end {
                    let actual = src_end - block_start;
                    result[bytes_read..bytes_read + actual]
                        .copy_from_slice(&block[block_start..src_end]);
                }
            }

            bytes_read += copy_len;
        }

        Ok(result)
    }

    /// Delete the file at `path`, freeing all allocated data blocks.
    ///
    /// # Errors
    ///
    /// - [`FsError::FileNotFound`] if `path` does not exist.
    /// - [`FsError::NotAFile`] if `path` is a directory.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    /// use omni_fs::FsError;
    ///
    /// let mut vol = OnDiskVolume::format(64);
    /// vol.create_file("/tmp.bin").expect("create");
    /// vol.delete_file("/tmp.bin").expect("delete");
    /// assert_eq!(vol.stat_file("/tmp.bin"), Err(FsError::FileNotFound));
    /// ```
    pub fn delete_file(&mut self, path: &str) -> Result<(), FsError> {
        let inode_number = self
            .path_map
            .get(path)
            .copied()
            .ok_or(FsError::FileNotFound)?;

        let inode = self
            .inodes
            .get(&inode_number)
            .ok_or(FsError::FileNotFound)?;
        if inode.file_type != FileType::RegularFile {
            return Err(FsError::NotAFile);
        }

        let block_numbers: Vec<u64> = inode.blocks.clone();

        for &block_num in &block_numbers {
            if block_num != 0 {
                self.data_blocks.remove(&block_num);
                self.integrity_tags.remove(&block_num);
                self.bitmap.free(block_num);
            }
        }

        self.inodes.remove(&inode_number);
        self.path_map.remove(path);
        self.superblock.inode_count = self.superblock.inode_count.saturating_sub(1);
        self.superblock.free_blocks = self.bitmap.free_count();

        Ok(())
    }

    /// Return [`FileMetadata`] for the file or directory at `path`.
    ///
    /// # Errors
    ///
    /// - [`FsError::FileNotFound`] if `path` does not exist.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let mut vol = OnDiskVolume::format(64);
    /// vol.create_file("/k.bin").expect("create");
    /// vol.write_file("/k.bin", 0, &[0u8; 100]).expect("write");
    /// let meta = vol.stat_file("/k.bin").expect("stat");
    /// assert_eq!(meta.size, 100);
    /// ```
    pub fn stat_file(&self, path: &str) -> Result<FileMetadata, FsError> {
        let inode_number = self
            .path_map
            .get(path)
            .copied()
            .ok_or(FsError::FileNotFound)?;
        let inode = self
            .inodes
            .get(&inode_number)
            .ok_or(FsError::FileNotFound)?;
        Ok(FileMetadata {
            size: inode.size,
            block_count: u64::from(inode.block_count),
            created: inode.created,
            modified: inode.modified,
        })
    }

    /// List direct child names within the directory at `path`.
    ///
    /// # Errors
    ///
    /// - [`FsError::FileNotFound`] if `path` does not exist.
    /// - [`FsError::NotADirectory`] if `path` is a regular file.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let mut vol = OnDiskVolume::format(64);
    /// vol.create_file("/a.bin").expect("a");
    /// vol.create_file("/b.bin").expect("b");
    /// let mut names = vol.list_directory("/").expect("list");
    /// names.sort();
    /// assert_eq!(names, ["a.bin", "b.bin"]);
    /// ```
    pub fn list_directory(&self, path: &str) -> Result<Vec<String>, FsError> {
        let inode_number = self
            .path_map
            .get(path)
            .copied()
            .ok_or(FsError::FileNotFound)?;
        let inode = self
            .inodes
            .get(&inode_number)
            .ok_or(FsError::FileNotFound)?;
        if inode.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }

        let prefix = if path == "/" {
            String::from("/")
        } else {
            let mut p = String::from(path);
            p.push('/');
            p
        };

        let mut names = Vec::new();
        for candidate in self.path_map.keys() {
            if candidate == path {
                continue;
            }
            if !candidate.starts_with(prefix.as_str()) {
                continue;
            }
            let suffix = &candidate[prefix.len()..];
            if !suffix.contains('/') {
                names.push(String::from(suffix));
            }
        }
        Ok(names)
    }

    /// Return `true` if a file or directory exists at `path`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let mut vol = OnDiskVolume::format(64);
    /// assert!(!vol.exists("/boot.img"));
    /// vol.create_file("/boot.img").unwrap();
    /// assert!(vol.exists("/boot.img"));
    /// ```
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        self.path_map.contains_key(path)
    }

    /// Return the number of free 4 KiB blocks remaining on the volume.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    ///
    /// let vol = OnDiskVolume::format(64);
    /// // At least 1 block is free (all minus superblock and integrity region).
    /// assert!(vol.free_blocks() > 0);
    /// ```
    #[must_use]
    pub fn free_blocks(&self) -> u64 {
        self.superblock.free_blocks
    }

    /// Return a reference to the volume's [`Superblock`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_fs::ondisk::OnDiskVolume;
    /// use omni_fs::OMNI_FS_MAGIC;
    ///
    /// let vol = OnDiskVolume::format(64);
    /// assert_eq!(vol.superblock().magic, OMNI_FS_MAGIC);
    /// ```
    #[must_use]
    pub fn superblock(&self) -> &Superblock {
        &self.superblock
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    /// Map a logical block number (from the allocator) to its physical byte
    /// offset index in the flat buffer.
    ///
    /// Data blocks start at physical block 3 (after superblock, bitmap, inodes).
    /// The mapping is: sorted position among all data block numbers → physical index 3+n.
    fn block_num_to_physical(&self, block_num: u64) -> usize {
        // Collect all data block numbers in sorted order to determine the
        // compact physical layout.
        let sorted: Vec<u64> = self.data_blocks.keys().copied().collect();
        sorted
            .iter()
            .position(|&b| b == block_num)
            .map_or(0, |pos| 3 + pos)
    }
}

// =============================================================================
// Serialisation helpers
// =============================================================================

/// Write the superblock into a 4 KiB block buffer (zero-padded).
///
/// Uses a simple fixed-field layout matching OIP-FS-Wire-023 §S1:
/// `magic(8)` | `version(4)` | `block_size(4)` | `total_blocks(8)` | `free_blocks(8)` |
/// `inode_count(8)` | `root_inode(8)` | `created_at(8)` | `aead_key_id(8)`.
#[allow(
    clippy::indexing_slicing,
    reason = "all slice ranges are statically known and within the 64-byte superblock layout; the assert guards the length"
)]
fn write_superblock_to_block(sb: &Superblock, block: &mut [u8]) {
    assert!(
        block.len() >= 64,
        "superblock block must be at least 64 bytes"
    );
    block[..8].copy_from_slice(&sb.magic);
    block[8..12].copy_from_slice(&sb.version.to_le_bytes());
    block[12..16].copy_from_slice(&sb.block_size.to_le_bytes());
    block[16..24].copy_from_slice(&sb.total_blocks.to_le_bytes());
    block[24..32].copy_from_slice(&sb.free_blocks.to_le_bytes());
    block[32..40].copy_from_slice(&sb.inode_count.to_le_bytes());
    block[40..48].copy_from_slice(&sb.root_inode.to_le_bytes());
    block[48..56].copy_from_slice(&sb.created_at.to_le_bytes());
    block[56..64].copy_from_slice(&sb.aead_key_id.to_le_bytes());
    // Bytes 64..4095 remain zeroed (reserved).
}

/// Parse a superblock from the first 64 bytes of a block buffer.
///
/// # Errors
///
/// Returns [`FsError::IntegrityViolation`] if the magic or version is invalid.
#[allow(
    clippy::indexing_slicing,
    reason = "all slice ranges are within [0..64); the block.len() < 64 check above guards all accesses"
)]
fn parse_superblock(block: &[u8]) -> Result<Superblock, FsError> {
    if block.len() < 64 {
        return Err(FsError::IntegrityViolation);
    }

    let mut magic = [0u8; 8];
    magic.copy_from_slice(&block[..8]);
    if magic != OMNI_FS_MAGIC {
        return Err(FsError::IntegrityViolation);
    }

    let version = u32::from_le_bytes(
        block[8..12]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );
    if version != OMNI_FS_VERSION {
        return Err(FsError::IntegrityViolation);
    }

    let block_size = u32::from_le_bytes(
        block[12..16]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );
    let total_blocks = u64::from_le_bytes(
        block[16..24]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );
    let free_blocks = u64::from_le_bytes(
        block[24..32]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );
    let inode_count = u64::from_le_bytes(
        block[32..40]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );
    let root_inode = u64::from_le_bytes(
        block[40..48]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );
    let created_at = u64::from_le_bytes(
        block[48..56]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );
    let aead_key_id = u64::from_le_bytes(
        block[56..64]
            .try_into()
            .map_err(|_| FsError::IntegrityViolation)?,
    );

    Ok(Superblock {
        magic,
        version,
        block_size,
        total_blocks,
        free_blocks,
        inode_count,
        root_inode,
        created_at,
        aead_key_id,
    })
}

/// Serialise the inode table into a byte vector.
///
/// Format: `count(8 LE)` | for each inode: `inode_number(8)` | `file_type(1)` |
/// `_pad(7)` | `size(8)` | `block_count(4)` | `_pad(4)` | `created(8)` |
/// `modified(8)` | `blocks_len(8)` | each block pointer `(8)` |
/// `name_byte_len(2)` | name bytes.
///
/// This is a compact variable-length encoding, not the fixed 256-byte
/// on-disk layout (Phase 3 will use the fixed layout; Phase 2 uses this
/// compact form for in-memory → byte-buffer round-trips).
#[allow(
    clippy::cast_possible_truncation,
    reason = "inode count fits in u64; blocks.len() fits in u64; name length is bounded by MAX_PATH_LEN (4096) which fits in u16"
)]
fn serialize_inodes(inodes: &BTreeMap<u64, Inode>) -> Vec<u8> {
    let mut out = Vec::new();
    // Record count.
    out.extend_from_slice(&(inodes.len() as u64).to_le_bytes());
    for inode in inodes.values() {
        out.extend_from_slice(&inode.inode_number.to_le_bytes());
        out.push(match inode.file_type {
            FileType::RegularFile => 0,
            FileType::Directory => 1,
        });
        out.extend_from_slice(&[0u8; 7]); // padding
        out.extend_from_slice(&inode.size.to_le_bytes());
        out.extend_from_slice(&inode.block_count.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]); // padding
        out.extend_from_slice(&inode.created.to_le_bytes());
        out.extend_from_slice(&inode.modified.to_le_bytes());
        // block pointers count + each pointer
        out.extend_from_slice(&(inode.blocks.len() as u64).to_le_bytes());
        for &b in &inode.blocks {
            out.extend_from_slice(&b.to_le_bytes());
        }
        // name
        let name_bytes = inode.name.as_bytes();
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(name_bytes);
    }
    out
}

/// Deserialise inodes from a byte slice.
///
/// Returns `(inodes, path_map, next_inode)`.
#[allow(
    clippy::indexing_slicing,
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    reason = "all slice accesses are guarded by bounds checks; field names must match the on-disk layout; function length is justified by the many fixed-width fields per inode record"
)]
fn deserialize_inodes(data: &[u8]) -> (BTreeMap<u64, Inode>, BTreeMap<String, u64>, u64) {
    let mut inodes = BTreeMap::new();
    let mut path_map = BTreeMap::new();
    let mut next_inode = crate::FIRST_USER_INODE;

    if data.len() < 8 {
        // No inode data; restore root only.
        let root = Inode {
            inode_number: crate::ROOT_INODE_NUMBER,
            file_type: FileType::Directory,
            size: 0,
            block_count: 0,
            created: 0,
            modified: 0,
            blocks: Vec::new(),
            name: String::from("/"),
        };
        inodes.insert(crate::ROOT_INODE_NUMBER, root);
        path_map.insert(String::from("/"), crate::ROOT_INODE_NUMBER);
        return (inodes, path_map, next_inode);
    }

    let count = u64::from_le_bytes(data[..8].try_into().unwrap_or([0u8; 8])) as usize;
    let mut pos = 8usize;

    for _ in 0..count {
        if pos + 40 > data.len() {
            break;
        }
        let inode_number = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap_or([0u8; 8]));
        pos += 8;
        let file_type_byte = data[pos];
        pos += 1 + 7; // type + padding
        let size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap_or([0u8; 8]));
        pos += 8;
        let block_count = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap_or([0u8; 4]));
        pos += 4 + 4; // block_count + padding
        let created = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap_or([0u8; 8]));
        pos += 8;
        let modified = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap_or([0u8; 8]));
        pos += 8;

        // block pointers
        if pos + 8 > data.len() {
            break;
        }
        let blocks_len =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap_or([0u8; 8])) as usize;
        pos += 8;
        let mut blocks = Vec::with_capacity(blocks_len);
        for _ in 0..blocks_len {
            if pos + 8 > data.len() {
                break;
            }
            blocks.push(u64::from_le_bytes(
                data[pos..pos + 8].try_into().unwrap_or([0u8; 8]),
            ));
            pos += 8;
        }

        // name
        if pos + 2 > data.len() {
            break;
        }
        let name_byte_len =
            u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap_or([0u8; 2])) as usize;
        pos += 2;
        if pos + name_byte_len > data.len() {
            break;
        }
        let name = String::from_utf8_lossy(&data[pos..pos + name_byte_len]).into_owned();
        pos += name_byte_len;

        let file_type = if file_type_byte == 0 {
            FileType::RegularFile
        } else {
            FileType::Directory
        };

        // Reconstruct the path for path_map.
        let path = if file_type == FileType::Directory && name == "/" {
            String::from("/")
        } else {
            let mut p = String::from("/");
            p.push_str(&name);
            p
        };

        if inode_number >= next_inode {
            next_inode = inode_number + 1;
        }

        let inode = Inode {
            inode_number,
            file_type,
            size,
            block_count,
            created,
            modified,
            blocks,
            name,
        };
        inodes.insert(inode_number, inode);
        path_map.insert(path, inode_number);
    }

    // Ensure root always exists.
    inodes.entry(crate::ROOT_INODE_NUMBER).or_insert_with(|| {
        path_map.insert(String::from("/"), crate::ROOT_INODE_NUMBER);
        Inode {
            inode_number: crate::ROOT_INODE_NUMBER,
            file_type: FileType::Directory,
            size: 0,
            block_count: 0,
            created: 0,
            modified: 0,
            blocks: Vec::new(),
            name: String::from("/"),
        }
    });

    (inodes, path_map, next_inode)
}

/// Restore data blocks from the raw byte buffer.
///
/// Reads physical blocks `3..data_end` and maps them back to logical block
/// numbers using the bitmap's allocated set, skipping structural blocks
/// (blocks 1, 2 = bitmap/inode regions; block `total_blocks` = integrity region).
#[allow(
    clippy::indexing_slicing,
    reason = "byte_pos..byte_pos+BLOCK_SIZE is guarded by the while condition byte_pos+BLOCK_SIZE <= data_end and data_end <= raw.len()"
)]
fn deserialize_data_blocks(
    raw: &[u8],
    data_start: usize,
    data_end: usize,
    bitmap: &BlockBitmap,
    total_blocks: u64,
) -> BTreeMap<u64, Vec<u8>> {
    let mut result = BTreeMap::new();
    // Build the set of USER data block numbers: allocated blocks that are
    // not structural (blocks 1, 2, or total_blocks).
    let data_blocks: Vec<u64> = (3..total_blocks)
        .filter(|&b| bitmap.is_allocated(b))
        .collect();

    let mut phys_idx = 0usize;
    let mut byte_pos = data_start;
    while byte_pos + BLOCK_SIZE <= data_end && phys_idx < data_blocks.len() {
        let block_num = data_blocks[phys_idx];
        let block_data = raw[byte_pos..byte_pos + BLOCK_SIZE].to_vec();
        result.insert(block_num, block_data);
        byte_pos += BLOCK_SIZE;
        phys_idx += 1;
    }
    result
}

/// Write integrity tags into a 4 KiB block buffer.
///
/// Format: for each entry: `block_number(8 LE)` | `tag(16)`.
#[allow(
    clippy::indexing_slicing,
    reason = "pos+INTEGRITY_ENTRY_SIZE <= BLOCK_SIZE is checked before every slice access; block.len() >= BLOCK_SIZE is guaranteed by the caller (sync_to_bytes)"
)]
fn write_integrity_tags(tags: &BTreeMap<u64, [u8; TAG_LEN]>, block: &mut [u8]) {
    let mut pos = 0usize;
    for (&block_num, tag) in tags {
        if pos + INTEGRITY_ENTRY_SIZE > BLOCK_SIZE {
            break; // Integrity block is full; Phase 3 will handle overflow.
        }
        block[pos..pos + 8].copy_from_slice(&block_num.to_le_bytes());
        block[pos + 8..pos + INTEGRITY_ENTRY_SIZE].copy_from_slice(tag);
        pos += INTEGRITY_ENTRY_SIZE;
    }
}

/// Restore integrity tags from a 4 KiB block buffer.
#[allow(
    clippy::indexing_slicing,
    reason = "pos..pos+8 and pos+8..pos+INTEGRITY_ENTRY_SIZE are within [0..block.len()) as guarded by the while condition"
)]
fn deserialize_integrity_tags(block: &[u8]) -> BTreeMap<u64, [u8; TAG_LEN]> {
    let mut result = BTreeMap::new();
    let mut pos = 0usize;
    while pos + INTEGRITY_ENTRY_SIZE <= block.len() {
        let block_num_bytes: [u8; 8] = block[pos..pos + 8].try_into().unwrap_or([0u8; 8]);
        let block_num = u64::from_le_bytes(block_num_bytes);
        if block_num == 0 {
            // Zero block_num is the sentinel for unused entries; stop reading.
            break;
        }
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&block[pos + 8..pos + INTEGRITY_ENTRY_SIZE]);
        result.insert(block_num, tag);
        pos += INTEGRITY_ENTRY_SIZE;
    }
    result
}

// =============================================================================
// Private utility
// =============================================================================

/// Extract the basename component of an absolute path.
fn basename(path: &str) -> &str {
    match path.rfind('/') {
        Some(idx) if idx + 1 == path.len() => "/",
        Some(idx) => &path[idx + 1..],
        None => path,
    }
}
