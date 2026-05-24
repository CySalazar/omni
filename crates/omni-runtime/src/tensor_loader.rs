//! GGUF tensor weight extraction into HAL [`TensorBuffer`]s.
//!
//! This module bridges the GGUF parser ([`crate::gguf`]) and the HAL tensor
//! abstraction ([`omni_hal::tensor`]). It extracts raw bytes for each tensor
//! from the GGUF data blob and, where possible, converts them to a canonical
//! `F32` representation suitable for inference.
//!
//! ## Phase 2 scope
//!
//! - **F32**: passed through as-is (zero-copy byte slice → owned `Vec`).
//! - **F16**: each 16-bit half-precision value is expanded to `f32`.
//! - **BF16**: each 16-bit bfloat16 value is expanded to `f32`.
//! - **I8**: stored as [`TensorDtype::I8`] without conversion.
//! - **All quantized types** (Q4_0, Q4_1, Q5_0, Q5_1, Q8_0, Q8_1, k-quants,
//!   I16, I32, I64, F64): a zero-filled `F32` buffer of the correct shape is
//!   returned. Full dequantization is deferred to Phase 4.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
use omni_types::{OmniError, Result};

use crate::gguf::{GgufDtype, GgufHeader, GgufTensorInfo};

// =============================================================================
// LoadedTensor
// =============================================================================

/// A tensor extracted from a GGUF file, paired with its name and data buffer.
///
/// # Example
///
/// ```rust
/// use omni_runtime::tensor_loader::LoadedTensor;
/// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
///
/// let desc = TensorDescriptor::named(vec![2, 2], TensorDtype::F32, "weights");
/// let buf  = TensorBuffer::new(desc, vec![0u8; 16]);
/// let lt   = LoadedTensor { name: "weights".into(), buffer: buf };
/// assert_eq!(lt.name, "weights");
/// assert_eq!(lt.buffer.len(), 16);
/// ```
#[derive(Debug)]
pub struct LoadedTensor {
    /// GGUF tensor name (e.g. `"token_embd.weight"`).
    pub name: String,
    /// Tensor data in HAL format (always F32 after dequantization, except I8).
    pub buffer: TensorBuffer,
}

// =============================================================================
// dtype helpers
// =============================================================================

/// Map a [`GgufDtype`] to the closest [`TensorDtype`] supported by the HAL.
///
/// Quantized types (`Q4_0` … `Q8_1`, k-quants, integer widths other than I8,
/// F64) map to [`TensorDtype::F32`] because the dequantization step produces
/// `f32` output.
///
/// # Example
///
/// ```rust
/// use omni_runtime::gguf::GgufDtype;
/// use omni_runtime::tensor_loader::gguf_dtype_to_hal;
/// use omni_hal::tensor::TensorDtype;
///
/// assert_eq!(gguf_dtype_to_hal(GgufDtype::F32),  TensorDtype::F32);
/// assert_eq!(gguf_dtype_to_hal(GgufDtype::F16),  TensorDtype::F16);
/// assert_eq!(gguf_dtype_to_hal(GgufDtype::Bf16), TensorDtype::Bf16);
/// assert_eq!(gguf_dtype_to_hal(GgufDtype::I8),   TensorDtype::I8);
/// // Quantized types become F32 after dequantization.
/// assert_eq!(gguf_dtype_to_hal(GgufDtype::Q4_0), TensorDtype::F32);
/// ```
#[must_use]
pub fn gguf_dtype_to_hal(dtype: GgufDtype) -> TensorDtype {
    match dtype {
        GgufDtype::F16 => TensorDtype::F16,
        GgufDtype::Bf16 => TensorDtype::Bf16,
        GgufDtype::I8 => TensorDtype::I8,
        // F32 and all quantized/wide-int/f64 types produce F32 after
        // dequantization (quantized types are zero-filled stubs in Phase 2).
        _ => TensorDtype::F32,
    }
}

/// Compute the total byte size of a tensor on disk given its shape and dtype.
///
/// For quantized types the computation accounts for sub-byte packing and
/// block-level overhead. Returns an error if any dimension overflows `usize`.
// `match_same_arms` is suppressed because semantically identical arms
// (e.g. I8 vs k-quant 1-byte upper bound, F16 vs Q3_K 2-byte upper bound)
// belong to distinct logical categories. Merging them would obscure the
// intent and make the Phase-4 expansion to exact sizes harder to follow.
#[allow(clippy::match_same_arms)]
fn gguf_tensor_byte_size(tensor_info: &GgufTensorInfo) -> Result<usize> {
    let n_elements: usize = tensor_info.dimensions.iter().try_fold(1usize, |acc, &d| {
        let d_usize = usize::try_from(d).map_err(|_| {
            OmniError::internal("tensor_loader::byte_size — dimension overflows usize")
        })?;
        acc.checked_mul(d_usize)
            .ok_or_else(|| OmniError::internal("tensor_loader::byte_size — element count overflow"))
    })?;

    // Bit-width per element depends on the dtype. For quantized formats that
    // use fractional bits-per-element, we compute bytes as ceiling division.
    // All values are taken from the GGUF spec and llama.cpp constants:
    // https://github.com/ggml-org/ggml/blob/master/docs/gguf.md
    let byte_size = match tensor_info.dtype {
        // Floating-point and integer scalar types.
        GgufDtype::F32 | GgufDtype::I32 => n_elements.checked_mul(4),
        // 2-byte element types: F16, BF16, I16.
        GgufDtype::F16 | GgufDtype::Bf16 | GgufDtype::I16 => n_elements.checked_mul(2),
        GgufDtype::I8 => Some(n_elements),
        // 8-byte element types: I64, F64.
        GgufDtype::I64 | GgufDtype::F64 => n_elements.checked_mul(8),
        // Q4_0: 4 bits/element + 2-byte scale per 32-element block = 18 bytes/block.
        GgufDtype::Q4_0 => n_elements.div_ceil(32).checked_mul(18),
        // Q4_1: 4 bits/element + 4 bytes (scale+min) per 32-element block = 20 bytes/block.
        GgufDtype::Q4_1 => n_elements.div_ceil(32).checked_mul(20),
        // Q5_0: 5 bits/element + 2-byte scale per 32-element block = 22 bytes/block.
        GgufDtype::Q5_0 => n_elements.div_ceil(32).checked_mul(22),
        // Q5_1: 5 bits/element + 4 bytes per 32-element block = 24 bytes/block.
        GgufDtype::Q5_1 => n_elements.div_ceil(32).checked_mul(24),
        // Q8_0: 8 bits/element + 2-byte scale per 32-element block = 34 bytes/block.
        GgufDtype::Q8_0 => n_elements.div_ceil(32).checked_mul(34),
        // Q8_1: 8 bits/element + 4 bytes per 32-element block = 36 bytes/block.
        GgufDtype::Q8_1 => n_elements.div_ceil(32).checked_mul(36),
        // k-quant types: conservative upper-bound approximation (see inline comment).
        // For Phase 2 stub the byte size governs only how many bytes are sliced
        // from the data region before being discarded (zeros are returned).
        // 1-byte-per-element upper bound: Q2_K (~2.625 bpe), Q4_K (~4.5 bpe), Q6_K (~6.56 bpe).
        GgufDtype::Q2_K | GgufDtype::Q4_K | GgufDtype::Q6_K => Some(n_elements),
        // 2-byte-per-element upper bound: Q3_K (~3.44 bpe), Q5_K (~5.5 bpe).
        GgufDtype::Q3_K | GgufDtype::Q5_K => n_elements.checked_mul(2),
    }
    .ok_or_else(|| OmniError::internal("tensor_loader::byte_size — byte size overflow"))?;

    Ok(byte_size)
}

// =============================================================================
// extract_tensor_bytes
// =============================================================================

/// Extract the raw on-disk bytes for a single tensor from the GGUF data blob.
///
/// `data` is the full GGUF file byte slice. Tensor data begins at
/// `header.data_offset`; each tensor's bytes start at
/// `header.data_offset + tensor_info.offset` and span `byte_size` bytes
/// (computed from shape × dtype).
///
/// The returned slice is a zero-copy view into `data`; no allocation is
/// performed.
///
/// # Errors
///
/// - [`OmniError::Internal`] if the computed byte range lies outside `data`.
///
/// # Example
///
/// ```rust
/// use omni_runtime::gguf::{GgufDtype, GgufHeader, GgufTensorInfo, GGUF_MAGIC, GGUF_VERSION_3};
/// use omni_runtime::tensor_loader::extract_tensor_bytes;
///
/// // Build a minimal GGUF with one 2-element F32 tensor.
/// let mut buf = Vec::<u8>::new();
/// buf.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
/// buf.extend_from_slice(&GGUF_VERSION_3.to_le_bytes());
/// buf.extend_from_slice(&1u64.to_le_bytes()); // tensor_count
/// buf.extend_from_slice(&0u64.to_le_bytes()); // metadata_kv_count
/// // tensor name "w" (u64 len + bytes)
/// buf.extend_from_slice(&1u64.to_le_bytes());
/// buf.push(b'w');
/// buf.extend_from_slice(&1u32.to_le_bytes()); // n_dimensions
/// buf.extend_from_slice(&2u64.to_le_bytes()); // dim[0] = 2
/// buf.extend_from_slice(&0u32.to_le_bytes()); // dtype F32
/// buf.extend_from_slice(&0u64.to_le_bytes()); // offset 0
/// // Pad to 32-byte alignment, then append 8 bytes of tensor data.
/// while buf.len() % 32 != 0 { buf.push(0); }
/// buf.extend_from_slice(&1.0f32.to_le_bytes());
/// buf.extend_from_slice(&2.0f32.to_le_bytes());
///
/// let header = omni_runtime::gguf::parse_gguf(&buf).unwrap();
/// let t_info = &header.tensors[0];
/// let raw = extract_tensor_bytes(&buf, &header, t_info).unwrap();
/// assert_eq!(raw.len(), 8); // 2 × 4 bytes
/// ```
pub fn extract_tensor_bytes<'a>(
    data: &'a [u8],
    header: &GgufHeader,
    tensor_info: &GgufTensorInfo,
) -> Result<&'a [u8]> {
    let byte_size = gguf_tensor_byte_size(tensor_info)?;

    // offset of this tensor's data within the data region (relative to
    // header.data_offset).
    let tensor_offset_in_region = usize::try_from(tensor_info.offset).map_err(|_| {
        OmniError::internal("tensor_loader::extract — tensor offset overflows usize")
    })?;

    let start = header
        .data_offset
        .checked_add(tensor_offset_in_region)
        .ok_or_else(|| {
            OmniError::internal("tensor_loader::extract — tensor start overflows usize")
        })?;

    let end = start.checked_add(byte_size).ok_or_else(|| {
        OmniError::internal("tensor_loader::extract — tensor end overflows usize")
    })?;

    data.get(start..end).ok_or_else(|| {
        OmniError::internal("tensor_loader::extract — tensor bytes out of bounds in GGUF data")
    })
}

// =============================================================================
// F16 → F32 conversion
// =============================================================================

/// Convert a single IEEE 754 half-precision (F16) bit pattern to `f32`.
///
/// Layout: 1 sign bit, 5 exponent bits (bias 15), 10 mantissa bits.
/// Special values (Inf, `NaN`, subnormals) are handled correctly.
fn f16_bits_to_f32(bits: u16) -> f32 {
    let sign: u32 = u32::from(bits >> 15) << 31;
    let exp_f16: u32 = u32::from((bits >> 10) & 0x1F);
    let mantissa: u32 = u32::from(bits & 0x03FF);

    let f32_bits: u32 = if exp_f16 == 0 {
        // Subnormal F16: convert to F32 subnormal or zero.
        if mantissa == 0 {
            // Positive or negative zero.
            sign
        } else {
            // Normalise the subnormal: find leading 1 bit of mantissa.
            let mut m = mantissa;
            let mut e = 127 - 14; // F32 bias - F16 bias + 1
            while m & 0x0400 == 0 {
                m <<= 1;
                e -= 1;
            }
            m &= 0x03FF;
            sign | (e << 23) | (m << 13)
        }
    } else if exp_f16 == 31 {
        // F16 Inf or NaN → F32 Inf or NaN (preserve mantissa).
        sign | 0x7F80_0000 | (mantissa << 13)
    } else {
        // Normal F16: re-bias exponent from 15 to 127.
        sign | ((exp_f16 + 127 - 15) << 23) | (mantissa << 13)
    };

    f32::from_bits(f32_bits)
}

/// Convert a single bfloat16 bit pattern to `f32`.
///
/// BF16 shares the same sign and exponent layout as F32 but has only
/// 7 mantissa bits. Conversion is zero-extending the bit pattern to 32 bits
/// (the lower 16 bits of the F32 mantissa become zero).
fn bf16_bits_to_f32(bits: u16) -> f32 {
    f32::from_bits(u32::from(bits) << 16)
}

// =============================================================================
// dequantize_to_f32
// =============================================================================

/// Convert raw GGUF tensor bytes into a [`TensorBuffer`].
///
/// The output dtype depends on the source dtype:
///
/// | Source dtype | Output dtype | Operation |
/// |---|---|---|
/// | F32 | F32 | byte copy |
/// | F16 | F32 | each 16-bit value expanded to f32 |
/// | BF16 | F32 | each 16-bit value expanded to f32 |
/// | I8 | I8 | byte copy |
/// | All others | F32 | zeroed buffer (Phase 2 stub) |
///
/// # Errors
///
/// - [`OmniError::Internal`] if `raw_bytes.len()` is not a multiple of the
///   element byte width for F32, F16, BF16, or I8.
///
/// # Example
///
/// ```rust
/// use omni_runtime::gguf::{GgufDtype, GgufTensorInfo};
/// use omni_runtime::tensor_loader::dequantize_to_f32;
///
/// let info = GgufTensorInfo {
///     name: "w".into(),
///     n_dimensions: 1,
///     dimensions: vec![2],
///     dtype: GgufDtype::F32,
///     offset: 0,
/// };
/// let raw = [0u8, 0, 128, 63, 0, 0, 0, 64]; // 1.0f32, 2.0f32 LE
/// let buf = dequantize_to_f32(&info, &raw).unwrap();
/// assert_eq!(buf.len(), 8);
/// ```
pub fn dequantize_to_f32(tensor_info: &GgufTensorInfo, raw_bytes: &[u8]) -> Result<TensorBuffer> {
    let shape: Vec<usize> = tensor_info.dimensions.iter().map(|&d| d as usize).collect();

    let n_elements: usize = shape.iter().product::<usize>().max(1);

    let (dtype, bytes) = match tensor_info.dtype {
        GgufDtype::F32 => {
            if raw_bytes.len() != n_elements * 4 {
                return Err(OmniError::internal(
                    "tensor_loader::dequantize — F32 byte count mismatch",
                ));
            }
            (TensorDtype::F32, raw_bytes.to_vec())
        }

        GgufDtype::F16 => {
            if raw_bytes.len() != n_elements * 2 {
                return Err(OmniError::internal(
                    "tensor_loader::dequantize — F16 byte count mismatch",
                ));
            }
            let mut out = vec![0u8; n_elements * 4];
            for i in 0..n_elements {
                let lo = raw_bytes.get(i * 2).copied().ok_or_else(|| {
                    OmniError::internal("tensor_loader::dequantize — F16 read OOB")
                })?;
                let hi = raw_bytes.get(i * 2 + 1).copied().ok_or_else(|| {
                    OmniError::internal("tensor_loader::dequantize — F16 read OOB")
                })?;
                let bits = u16::from_le_bytes([lo, hi]);
                let f = f16_bits_to_f32(bits);
                let f_bytes = f.to_le_bytes();
                let dst = out.get_mut(i * 4..i * 4 + 4).ok_or_else(|| {
                    OmniError::internal("tensor_loader::dequantize — F16 write OOB")
                })?;
                dst.copy_from_slice(&f_bytes);
            }
            (TensorDtype::F32, out)
        }

        GgufDtype::Bf16 => {
            if raw_bytes.len() != n_elements * 2 {
                return Err(OmniError::internal(
                    "tensor_loader::dequantize — BF16 byte count mismatch",
                ));
            }
            let mut out = vec![0u8; n_elements * 4];
            for i in 0..n_elements {
                let lo = raw_bytes.get(i * 2).copied().ok_or_else(|| {
                    OmniError::internal("tensor_loader::dequantize — BF16 read OOB")
                })?;
                let hi = raw_bytes.get(i * 2 + 1).copied().ok_or_else(|| {
                    OmniError::internal("tensor_loader::dequantize — BF16 read OOB")
                })?;
                let bits = u16::from_le_bytes([lo, hi]);
                let f = bf16_bits_to_f32(bits);
                let f_bytes = f.to_le_bytes();
                let dst = out.get_mut(i * 4..i * 4 + 4).ok_or_else(|| {
                    OmniError::internal("tensor_loader::dequantize — BF16 write OOB")
                })?;
                dst.copy_from_slice(&f_bytes);
            }
            (TensorDtype::F32, out)
        }

        GgufDtype::I8 => {
            if raw_bytes.len() != n_elements {
                return Err(OmniError::internal(
                    "tensor_loader::dequantize — I8 byte count mismatch",
                ));
            }
            (TensorDtype::I8, raw_bytes.to_vec())
        }

        // All quantized types (Q4_0 … Q6_K, I16, I32, I64, F64):
        // return a zeroed F32 buffer with the correct shape.
        // Full dequantization is deferred to Phase 4.
        _ => {
            let zero_bytes = vec![0u8; n_elements * 4];
            (TensorDtype::F32, zero_bytes)
        }
    };

    let desc = TensorDescriptor::named(shape, dtype, tensor_info.name.clone());
    Ok(TensorBuffer::new(desc, bytes))
}

// =============================================================================
// load_all_tensors
// =============================================================================

/// Load all tensors from a GGUF file into [`TensorBuffer`]s.
///
/// Iterates [`GgufHeader::tensors`], extracts raw bytes for each via
/// [`extract_tensor_bytes`], and converts them via [`dequantize_to_f32`].
///
/// # Errors
///
/// - [`OmniError::Internal`] if any tensor's bytes are out of bounds or the
///   conversion fails.
///
/// # Example
///
/// ```rust
/// use omni_runtime::gguf::{GgufDtype, GgufHeader, parse_gguf, GGUF_MAGIC, GGUF_VERSION_3};
/// use omni_runtime::tensor_loader::load_all_tensors;
///
/// let mut buf = Vec::<u8>::new();
/// buf.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
/// buf.extend_from_slice(&GGUF_VERSION_3.to_le_bytes());
/// buf.extend_from_slice(&0u64.to_le_bytes()); // tensor_count = 0
/// buf.extend_from_slice(&0u64.to_le_bytes()); // metadata_kv_count = 0
///
/// let header = parse_gguf(&buf).unwrap();
/// let tensors = load_all_tensors(&buf, &header).unwrap();
/// assert!(tensors.is_empty());
/// ```
pub fn load_all_tensors(data: &[u8], header: &GgufHeader) -> Result<Vec<LoadedTensor>> {
    header
        .tensors
        .iter()
        .map(|info| {
            let raw = extract_tensor_bytes(data, header, info)?;
            let buffer = dequantize_to_f32(info, raw)?;
            Ok(LoadedTensor {
                name: info.name.clone(),
                buffer,
            })
        })
        .collect()
}

// =============================================================================
// load_tensor_by_name
// =============================================================================

/// Load a single tensor by name from a GGUF file.
///
/// Searches [`GgufHeader::tensors`] for an entry whose name equals `name`,
/// then extracts and dequantizes it.
///
/// # Errors
///
/// - [`OmniError::Internal`] if no tensor with the given name exists.
/// - [`OmniError::Internal`] if extraction or conversion fails.
///
/// # Example
///
/// ```rust
/// use omni_runtime::gguf::{GgufDtype, GGUF_MAGIC, GGUF_VERSION_3, parse_gguf};
/// use omni_runtime::tensor_loader::load_tensor_by_name;
///
/// // Minimal GGUF with no tensors — should return an error for any name.
/// let mut buf = Vec::<u8>::new();
/// buf.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
/// buf.extend_from_slice(&GGUF_VERSION_3.to_le_bytes());
/// buf.extend_from_slice(&0u64.to_le_bytes());
/// buf.extend_from_slice(&0u64.to_le_bytes());
///
/// let header = parse_gguf(&buf).unwrap();
/// assert!(load_tensor_by_name(&buf, &header, "missing").is_err());
/// ```
pub fn load_tensor_by_name(data: &[u8], header: &GgufHeader, name: &str) -> Result<LoadedTensor> {
    let info = header
        .tensors
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| {
            OmniError::internal("tensor_loader::load_by_name — tensor name not found in header")
        })?;
    let raw = extract_tensor_bytes(data, header, info)?;
    let buffer = dequantize_to_f32(info, raw)?;
    Ok(LoadedTensor {
        name: info.name.clone(),
        buffer,
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gguf::{
        GGUF_DEFAULT_ALIGNMENT, GGUF_MAGIC, GGUF_VERSION_3, GgufDtype, GgufTensorInfo, parse_gguf,
    };
    use omni_hal::tensor::TensorDtype;

    // -------------------------------------------------------------------------
    // Test helpers
    // -------------------------------------------------------------------------

    /// Encode a GGUF-format string (u64 length prefix + UTF-8 bytes).
    fn gguf_string(s: &str) -> Vec<u8> {
        let bytes = s.as_bytes();
        let mut buf = Vec::new();
        buf.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        buf.extend_from_slice(bytes);
        buf
    }

    /// Build a minimal GGUF file with the given tensors.
    ///
    /// `tensors`: list of `(name, dims, dtype, data_bytes)`.
    ///
    /// Tensor offsets within the data region are packed sequentially with
    /// 32-byte alignment (matching GGUF spec defaults).
    fn make_test_gguf(tensors: &[(&str, &[u64], GgufDtype, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
        buf.extend_from_slice(&GGUF_VERSION_3.to_le_bytes());
        buf.extend_from_slice(&(tensors.len() as u64).to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes()); // metadata_kv_count

        // Pre-compute offsets for tensor data region.
        // Each tensor's offset is aligned to GGUF_DEFAULT_ALIGNMENT within
        // the data region.
        let mut offsets: Vec<u64> = Vec::new();
        let mut running_offset: u64 = 0;
        for (_, _, _, data) in tensors {
            offsets.push(running_offset);
            let next = running_offset + data.len() as u64;
            // Align up to GGUF_DEFAULT_ALIGNMENT.
            running_offset =
                (next + GGUF_DEFAULT_ALIGNMENT as u64 - 1) & !(GGUF_DEFAULT_ALIGNMENT as u64 - 1);
        }

        // Write tensor info entries.
        for ((name, dims, dtype, _), &offset) in tensors.iter().zip(&offsets) {
            buf.extend_from_slice(&gguf_string(name));
            buf.extend_from_slice(&(dims.len() as u32).to_le_bytes());
            for &d in *dims {
                buf.extend_from_slice(&d.to_le_bytes());
            }
            buf.extend_from_slice(&(*dtype as u32).to_le_bytes());
            buf.extend_from_slice(&offset.to_le_bytes());
        }

        // Pad to 32-byte alignment to start the data region.
        while buf.len() % GGUF_DEFAULT_ALIGNMENT != 0 {
            buf.push(0);
        }

        // Write tensor data, inserting alignment padding between tensors.
        for (i, (_, _, _, data)) in tensors.iter().enumerate() {
            buf.extend_from_slice(data);
            if i + 1 < tensors.len() {
                while buf.len() % GGUF_DEFAULT_ALIGNMENT != 0 {
                    buf.push(0);
                }
            }
        }

        buf
    }

    // -------------------------------------------------------------------------
    // test_gguf_dtype_to_hal
    // -------------------------------------------------------------------------

    #[test]
    fn test_gguf_dtype_to_hal() {
        assert_eq!(gguf_dtype_to_hal(GgufDtype::F32), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::F16), TensorDtype::F16);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Bf16), TensorDtype::Bf16);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::I8), TensorDtype::I8);
        // All quantized types → F32
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q4_0), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q4_1), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q5_0), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q5_1), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q8_0), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q8_1), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q2_K), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q3_K), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q4_K), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q5_K), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::Q6_K), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::I16), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::I32), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::I64), TensorDtype::F32);
        assert_eq!(gguf_dtype_to_hal(GgufDtype::F64), TensorDtype::F32);
    }

    // -------------------------------------------------------------------------
    // test_extract_tensor_bytes_f32
    // -------------------------------------------------------------------------

    #[test]
    fn test_extract_tensor_bytes_f32() {
        let data_bytes: [u8; 8] = [
            0x00, 0x00, 0x80, 0x3F, // 1.0f32 LE
            0x00, 0x00, 0x00, 0x40, // 2.0f32 LE
        ];
        let gguf_data = make_test_gguf(&[("w", &[2], GgufDtype::F32, &data_bytes)]);
        let header = parse_gguf(&gguf_data).unwrap();
        let info = &header.tensors[0];

        let raw = extract_tensor_bytes(&gguf_data, &header, info).unwrap();
        assert_eq!(raw.len(), 8);
        assert_eq!(&raw[0..4], &1.0f32.to_le_bytes());
        assert_eq!(&raw[4..8], &2.0f32.to_le_bytes());
    }

    // -------------------------------------------------------------------------
    // test_dequantize_f32_passthrough
    // -------------------------------------------------------------------------

    #[test]
    fn test_dequantize_f32_passthrough() {
        let info = GgufTensorInfo {
            name: "test".into(),
            n_dimensions: 1,
            dimensions: vec![3],
            dtype: GgufDtype::F32,
            offset: 0,
        };
        let raw: Vec<u8> = [1.0f32, 2.0f32, 3.0f32]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let buf = dequantize_to_f32(&info, &raw).unwrap();
        assert_eq!(buf.descriptor.shape, vec![3]);
        assert_eq!(buf.descriptor.dtype, TensorDtype::F32);
        assert_eq!(buf.len(), 12);
        // Values should pass through unchanged.
        let got: Vec<f32> = buf
            .as_bytes()
            .chunks(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert_eq!(got, vec![1.0f32, 2.0f32, 3.0f32]);
    }

    // -------------------------------------------------------------------------
    // test_dequantize_f16_to_f32
    // -------------------------------------------------------------------------

    #[test]
    fn test_dequantize_f16_to_f32() {
        let info = GgufTensorInfo {
            name: "h".into(),
            n_dimensions: 1,
            dimensions: vec![2],
            dtype: GgufDtype::F16,
            offset: 0,
        };
        // F16 bit patterns for 1.0 and -2.0:
        // 1.0 → sign=0 exp=0b01111 (15) mantissa=0 → 0x3C00
        // -2.0 → sign=1 exp=0b10000 (16) mantissa=0 → 0xC000
        let raw: Vec<u8> = vec![0x00, 0x3C, 0x00, 0xC0];

        let buf = dequantize_to_f32(&info, &raw).unwrap();
        assert_eq!(buf.descriptor.dtype, TensorDtype::F32);
        assert_eq!(buf.len(), 8);

        let v0 = f32::from_le_bytes(buf.as_bytes()[0..4].try_into().unwrap());
        let v1 = f32::from_le_bytes(buf.as_bytes()[4..8].try_into().unwrap());
        assert!((v0 - 1.0f32).abs() < 1e-6, "expected 1.0, got {v0}");
        assert!((v1 - (-2.0f32)).abs() < 1e-6, "expected -2.0, got {v1}");
    }

    // -------------------------------------------------------------------------
    // test_dequantize_bf16_to_f32
    // -------------------------------------------------------------------------

    #[test]
    fn test_dequantize_bf16_to_f32() {
        let info = GgufTensorInfo {
            name: "b".into(),
            n_dimensions: 1,
            dimensions: vec![1],
            dtype: GgufDtype::Bf16,
            offset: 0,
        };
        // BF16 bit pattern for 1.0:
        // F32 1.0 = 0x3F800000; upper 16 bits = 0x3F80
        // Stored in LE: [0x80, 0x3F]
        let raw: Vec<u8> = vec![0x80, 0x3F];

        let buf = dequantize_to_f32(&info, &raw).unwrap();
        assert_eq!(buf.descriptor.dtype, TensorDtype::F32);
        let v = f32::from_le_bytes(buf.as_bytes()[0..4].try_into().unwrap());
        assert!((v - 1.0f32).abs() < 1e-6, "expected 1.0, got {v}");
    }

    // -------------------------------------------------------------------------
    // test_dequantize_i8_passthrough
    // -------------------------------------------------------------------------

    #[test]
    fn test_dequantize_i8_passthrough() {
        let info = GgufTensorInfo {
            name: "qi".into(),
            n_dimensions: 1,
            dimensions: vec![4],
            dtype: GgufDtype::I8,
            offset: 0,
        };
        let raw: Vec<u8> = vec![1, 2, 3, 4];
        let buf = dequantize_to_f32(&info, &raw).unwrap();
        assert_eq!(buf.descriptor.dtype, TensorDtype::I8);
        assert_eq!(buf.as_bytes(), &[1u8, 2, 3, 4]);
    }

    // -------------------------------------------------------------------------
    // test_dequantize_quantized_returns_zeros
    // -------------------------------------------------------------------------

    #[test]
    fn test_dequantize_quantized_returns_zeros() {
        // Q4_0 with 4 elements: byte size = ceil(4/32) * 18 = 18 bytes on disk.
        // But dequantize_to_f32 receives raw_bytes of any length for quantized
        // types; it returns a zero F32 buffer sized to n_elements.
        let info = GgufTensorInfo {
            name: "q".into(),
            n_dimensions: 1,
            dimensions: vec![4],
            dtype: GgufDtype::Q4_0,
            offset: 0,
        };
        // For quantized types dequantize_to_f32 ignores the raw bytes and
        // returns zeros — any non-empty slice is fine.
        let raw: Vec<u8> = vec![0xAB; 18];
        let buf = dequantize_to_f32(&info, &raw).unwrap();
        assert_eq!(buf.descriptor.dtype, TensorDtype::F32);
        assert_eq!(buf.descriptor.shape, vec![4]);
        // All bytes must be zero (zeroed stub).
        assert!(buf.as_bytes().iter().all(|&b| b == 0));
    }

    // -------------------------------------------------------------------------
    // test_load_all_tensors_minimal
    // -------------------------------------------------------------------------

    #[test]
    fn test_load_all_tensors_minimal() {
        let t1_data: Vec<u8> = [1.0f32, 2.0f32]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let t2_data: Vec<u8> = [3.0f32, 4.0f32, 5.0f32]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let gguf_data = make_test_gguf(&[
            ("layer0.weight", &[2], GgufDtype::F32, &t1_data),
            ("layer0.bias", &[3], GgufDtype::F32, &t2_data),
        ]);

        let header = parse_gguf(&gguf_data).unwrap();
        let loaded = load_all_tensors(&gguf_data, &header).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "layer0.weight");
        assert_eq!(loaded[0].buffer.descriptor.shape, vec![2]);
        assert_eq!(loaded[1].name, "layer0.bias");
        assert_eq!(loaded[1].buffer.descriptor.shape, vec![3]);
    }

    // -------------------------------------------------------------------------
    // test_load_tensor_by_name_found
    // -------------------------------------------------------------------------

    #[test]
    fn test_load_tensor_by_name_found() {
        let data: Vec<u8> = [7.0f32].iter().flat_map(|f| f.to_le_bytes()).collect();
        let gguf_data = make_test_gguf(&[("target", &[1], GgufDtype::F32, &data)]);
        let header = parse_gguf(&gguf_data).unwrap();

        let lt = load_tensor_by_name(&gguf_data, &header, "target").unwrap();
        assert_eq!(lt.name, "target");
        let v = f32::from_le_bytes(lt.buffer.as_bytes()[0..4].try_into().unwrap());
        assert!((v - 7.0f32).abs() < 1e-6, "expected 7.0, got {v}");
    }

    // -------------------------------------------------------------------------
    // test_load_tensor_by_name_not_found
    // -------------------------------------------------------------------------

    #[test]
    fn test_load_tensor_by_name_not_found() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
        buf.extend_from_slice(&GGUF_VERSION_3.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());

        let header = parse_gguf(&buf).unwrap();
        assert!(load_tensor_by_name(&buf, &header, "nope").is_err());
    }

    // -------------------------------------------------------------------------
    // test_f16_zero
    // -------------------------------------------------------------------------

    #[test]
    fn test_f16_zero() {
        // F16 bit pattern 0x0000 → 0.0f32
        assert_eq!(f16_bits_to_f32(0x0000), 0.0f32);
    }

    // -------------------------------------------------------------------------
    // test_f16_negative_zero
    // -------------------------------------------------------------------------

    #[test]
    fn test_f16_negative_zero() {
        // F16 0x8000 → -0.0f32
        let v = f16_bits_to_f32(0x8000);
        assert_eq!(v.to_bits(), (-0.0f32).to_bits());
    }

    // -------------------------------------------------------------------------
    // test_f16_infinity
    // -------------------------------------------------------------------------

    #[test]
    fn test_f16_infinity() {
        // F16 0x7C00 → +Inf
        assert!(f16_bits_to_f32(0x7C00).is_infinite());
        assert!(f16_bits_to_f32(0x7C00).is_sign_positive());
        // F16 0xFC00 → -Inf
        assert!(f16_bits_to_f32(0xFC00).is_infinite());
        assert!(f16_bits_to_f32(0xFC00).is_sign_negative());
    }
}
