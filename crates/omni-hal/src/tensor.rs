//! Tensor HAL — uniform compute dispatch across CPU, GPU, and NPU.
//!
//! This module defines the vendor-neutral types and traits that the rest of
//! OMNI OS uses to run tensor workloads. Callers never see whether inference
//! runs on CPU AVX-512, NVIDIA CUDA, or an Apple Neural Engine — they just
//! call [`TensorBackend::execute`].
//!
//! # Design overview
//!
//! ```text
//!   ┌─────────────────────────────────────┐
//!   │         caller (service crate)      │
//!   │  let buf = backend.allocate(&desc)  │
//!   │  let out = backend.execute(op, &[]) │
//!   └───────────────┬─────────────────────┘
//!                   │ dyn TensorBackend
//!         ┌─────────┴──────────┐
//!         │   CpuBackend       │  ← this module
//!         │  (SIMD dispatch)   │
//!         └────────────────────┘
//!         (future: GpuBackend, NpuBackend)
//! ```
//!
//! # `no_std` note
//!
//! `CpuBackend` uses `Vec` (heap) and `is_x86_feature_detected!` (std),
//! so this module requires the standard library.  The *trait surface*
//! (`TensorBackend`, `TensorDescriptor`, `TensorDtype`, etc.) uses only
//! `alloc` types and could be split into a `no_std`-compatible crate in a
//! future refactor.  That split is deferred to Phase 6 per the OMNI OS
//! `no_std` roadmap.
//!
//! # Async overhead
//!
//! [`TensorBackend`] is decorated with `#[async_trait]`, which boxes the
//! returned future (`Box<dyn Future>`).  This allocation is intentional:
//! Phase 2 targets correctness, not peak throughput.  A zero-copy,
//! poll-based version is planned for Phase 4 after the SIMD dispatch layer
//! is benchmarked.

use async_trait::async_trait;
use omni_types::error::{HalErrorKind, OmniError, Result};

// =============================================================================
// SIMD capability discriminant
// =============================================================================

/// SIMD capability levels detected at runtime on the host CPU.
///
/// Variants are ordered from weakest to strongest within each ISA family.
/// Detection is performed once at [`CpuBackend`] construction time and cached.
///
/// # Example
///
/// ```
/// use omni_hal::tensor::{CpuBackend, SimdCapability, TensorBackend};
///
/// let backend = CpuBackend::new();
/// // At least None is always present; actual level depends on the host CPU.
/// assert!(!backend.capabilities().is_empty());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SimdCapability {
    /// No SIMD support detected; scalar fallback only.
    None,
    /// Intel/AMD SSE 4.2 (128-bit vectors).
    Sse42,
    /// Intel/AMD AVX2 (256-bit vectors, FMA3).
    Avx2,
    /// Intel/AMD AVX-512 (512-bit vectors; implies AVX2).
    Avx512,
    /// ARM Neon / `AdvSIMD` (128-bit vectors).
    Neon,
    /// ARM SVE for `AArch64` (scalable vector extension).
    SveAarch64,
}

// =============================================================================
// Tensor data type
// =============================================================================

/// Numeric data type stored in a [`TensorBuffer`].
///
/// # Example
///
/// ```
/// use omni_hal::tensor::TensorDtype;
///
/// let dtype = TensorDtype::F32;
/// assert_eq!(dtype.bytes_per_element(), 4);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TensorDtype {
    /// 32-bit single-precision floating point (IEEE 754).
    F32,
    /// 16-bit half-precision floating point (IEEE 754-2008).
    F16,
    /// 16-bit brain float (Google bfloat16; mantissa truncated from F32).
    Bf16,
    /// 8-bit signed integer (quantized inference).
    I8,
    /// 8-bit unsigned integer (quantized inference).
    U8,
}

impl TensorDtype {
    /// Number of bytes needed to store one element of this dtype.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::TensorDtype;
    ///
    /// assert_eq!(TensorDtype::F32.bytes_per_element(), 4);
    /// assert_eq!(TensorDtype::F16.bytes_per_element(), 2);
    /// assert_eq!(TensorDtype::Bf16.bytes_per_element(), 2);
    /// assert_eq!(TensorDtype::I8.bytes_per_element(),  1);
    /// assert_eq!(TensorDtype::U8.bytes_per_element(),  1);
    /// ```
    #[must_use]
    pub const fn bytes_per_element(self) -> usize {
        match self {
            Self::F32 => 4,
            Self::F16 | Self::Bf16 => 2,
            Self::I8 | Self::U8 => 1,
        }
    }
}

// =============================================================================
// TensorDescriptor
// =============================================================================

/// Metadata describing the shape, element type, and optional debug name of a
/// tensor.
///
/// A `TensorDescriptor` is a *logical* description of a tensor — it does not
/// own any data. Pass it to [`TensorBackend::allocate`] to obtain a
/// [`TensorBuffer`] backed by real memory.
///
/// # Example
///
/// ```
/// use omni_hal::tensor::{TensorDescriptor, TensorDtype};
///
/// let desc = TensorDescriptor::new(vec![2, 3], TensorDtype::F32);
/// assert_eq!(desc.byte_size(), 2 * 3 * 4);
///
/// let named = TensorDescriptor::named(vec![1, 768], TensorDtype::F16, "embeddings");
/// assert_eq!(named.name.as_deref(), Some("embeddings"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorDescriptor {
    /// Dimensions from outermost to innermost (row-major / C order).
    ///
    /// An empty `shape` (scalar tensor) is valid; it has exactly one element.
    pub shape: Vec<usize>,

    /// Numeric element type.
    pub dtype: TensorDtype,

    /// Optional human-readable label for diagnostics.  Never used in
    /// correctness-critical paths; purely for tracing/logging.
    pub name: Option<String>,
}

impl TensorDescriptor {
    /// Create an unnamed tensor descriptor.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::new(vec![4, 4], TensorDtype::F32);
    /// assert_eq!(desc.name, None);
    /// ```
    #[must_use]
    pub fn new(shape: Vec<usize>, dtype: TensorDtype) -> Self {
        Self {
            shape,
            dtype,
            name: None,
        }
    }

    /// Create a named tensor descriptor.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::named(vec![1, 512], TensorDtype::Bf16, "hidden");
    /// assert_eq!(desc.name.as_deref(), Some("hidden"));
    /// ```
    #[must_use]
    pub fn named(shape: Vec<usize>, dtype: TensorDtype, name: impl Into<String>) -> Self {
        Self {
            shape,
            dtype,
            name: Some(name.into()),
        }
    }

    /// Total number of elements across all dimensions.
    ///
    /// A scalar tensor (empty `shape`) has exactly 1 element.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::new(vec![3, 4, 5], TensorDtype::F32);
    /// assert_eq!(desc.num_elements(), 60);
    ///
    /// // Scalar: empty shape → 1 element.
    /// let scalar = TensorDescriptor::new(vec![], TensorDtype::F32);
    /// assert_eq!(scalar.num_elements(), 1);
    /// ```
    #[must_use]
    pub fn num_elements(&self) -> usize {
        // product-of-dimensions; identity for empty iterator is 1 (scalar).
        self.shape.iter().product()
    }

    /// Total size in bytes required to hold all elements.
    ///
    /// Computed as `num_elements() * dtype.bytes_per_element()`.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::new(vec![2, 3], TensorDtype::F32);
    /// assert_eq!(desc.byte_size(), 24); // 6 elements × 4 bytes
    /// ```
    #[must_use]
    pub fn byte_size(&self) -> usize {
        self.num_elements() * self.dtype.bytes_per_element()
    }
}

// =============================================================================
// TensorBuffer
// =============================================================================

/// An opaque byte buffer paired with its [`TensorDescriptor`].
///
/// `TensorBuffer` is the physical counterpart to [`TensorDescriptor`]: the
/// descriptor records what shape and type the tensor has, while this struct
/// holds the raw bytes.
///
/// Buffers are created via [`TensorBackend::allocate`] and consumed by
/// [`TensorBackend::execute`].  Callers should not inspect the raw bytes
/// unless implementing a backend.
///
/// # Example
///
/// ```rust
/// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
///
/// let desc = TensorDescriptor::new(vec![2], TensorDtype::F32);
/// let buf = TensorBuffer::new(desc.clone(), vec![0u8; desc.byte_size()]);
/// assert_eq!(buf.len(), 8);
/// assert!(!buf.is_empty());
/// ```
#[derive(Debug)]
pub struct TensorBuffer {
    /// Descriptor that was used to allocate this buffer.
    pub descriptor: TensorDescriptor,
    // Raw element bytes.  Layout is always dense row-major (C order).
    bytes: Vec<u8>,
}

impl TensorBuffer {
    /// Construct a `TensorBuffer` from a pre-allocated byte vector.
    ///
    /// Callers are responsible for ensuring `bytes.len()` equals
    /// `descriptor.byte_size()`.  Backends enforce this contract
    /// internally; external callers should use
    /// [`TensorBackend::allocate`] instead.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::new(vec![4], TensorDtype::U8);
    /// let buf = TensorBuffer::new(desc.clone(), vec![1, 2, 3, 4]);
    /// assert_eq!(buf.as_bytes(), &[1u8, 2, 3, 4]);
    /// ```
    #[must_use]
    pub fn new(descriptor: TensorDescriptor, bytes: Vec<u8>) -> Self {
        Self { descriptor, bytes }
    }

    /// Immutable view of the raw byte content.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::new(vec![1], TensorDtype::I8);
    /// let buf = TensorBuffer::new(desc, vec![42]);
    /// assert_eq!(buf.as_bytes(), &[42u8]);
    /// ```
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Total byte length of this buffer.
    ///
    /// Should equal `self.descriptor.byte_size()` for any well-formed buffer.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::new(vec![3], TensorDtype::F16);
    /// let buf = TensorBuffer::new(desc, vec![0u8; 6]);
    /// assert_eq!(buf.len(), 6);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns `true` if the buffer contains no bytes.
    ///
    /// A well-formed buffer is never empty (even a scalar has `>=1` byte),
    /// but this method is provided to satisfy `clippy::len_without_is_empty`.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
    ///
    /// let desc = TensorDescriptor::new(vec![1], TensorDtype::U8);
    /// let buf = TensorBuffer::new(desc, vec![0]);
    /// assert!(!buf.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

// =============================================================================
// TensorOp
// =============================================================================

/// Operations that a [`TensorBackend`] can execute.
///
/// Each variant corresponds to a primitive tensor operation.  The Phase 2
/// `CpuBackend` stubs every op with a zeroed output; real dispatch is Phase 4.
///
/// # Example
///
/// ```
/// use omni_hal::tensor::TensorOp;
///
/// let op = TensorOp::MatMul { transpose_a: false, transpose_b: true };
/// let _ = format!("{op:?}"); // Debug is always available.
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum TensorOp {
    /// Dense matrix multiplication: `C = α·A·B`.
    ///
    /// - `transpose_a`: transpose the first input before multiplication.
    /// - `transpose_b`: transpose the second input before multiplication.
    MatMul {
        /// If `true`, transpose the first input matrix before the multiply.
        transpose_a: bool,
        /// If `true`, transpose the second input matrix before the multiply.
        transpose_b: bool,
    },

    /// Element-wise addition of two tensors with identical shapes.
    Add,

    /// Element-wise `ReLU` activation: `max(0, x)`.
    Relu,

    /// Softmax normalisation along a specified axis.
    Softmax {
        /// The dimension along which softmax is computed.
        axis: usize,
    },

    /// Reshape a tensor to a new shape without copying data.
    ///
    /// The total number of elements must be unchanged.
    Reshape {
        /// Target shape.  Product must equal the source tensor's
        /// [`TensorDescriptor::num_elements`].
        new_shape: Vec<usize>,
    },
}

// =============================================================================
// TensorBackend trait
// =============================================================================

/// Vendor-neutral interface for running tensor workloads.
///
/// Implementors hide the specifics of the underlying hardware (CPU with
/// AVX-512, NVIDIA CUDA, Apple ANE, etc.).  OMNI OS selects a backend at
/// runtime based on [`SimdCapability`] detection and available drivers.
///
/// # Async contract
///
/// `allocate` and `execute` are `async` because GPU and NPU backends
/// communicate over device queues which are inherently asynchronous.  The
/// CPU backend returns immediately inside an `async fn`, so the overhead is
/// one `Box` allocation per call (see crate-level async overhead note).
///
/// # Object safety
///
/// The trait is object-safe: `Box<dyn TensorBackend>` is valid and used by
/// `omni-runtime`'s backend registry.
///
/// # Example
///
/// ```no_run
/// # use omni_hal::tensor::{CpuBackend, TensorBackend, TensorDescriptor, TensorDtype, TensorOp};
/// # #[tokio::main]
/// # async fn main() -> omni_types::error::Result<()> {
/// let backend = CpuBackend::new();
/// let desc = TensorDescriptor::new(vec![4, 4], TensorDtype::F32);
/// let buf = backend.allocate(&desc).await?;
/// assert_eq!(buf.len(), 64);
/// # Ok(())
/// # }
/// ```
#[async_trait]
pub trait TensorBackend: Send + Sync {
    /// Human-readable backend identifier for logging and diagnostics.
    ///
    /// The returned string is always a compile-time literal (`&'static str`).
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{CpuBackend, TensorBackend};
    ///
    /// let b = CpuBackend::new();
    /// assert_eq!(b.name(), "cpu");
    /// ```
    fn name(&self) -> &'static str;

    /// SIMD capability levels available on this backend, from weakest to
    /// strongest.  The slice is never empty: at minimum it contains
    /// [`SimdCapability::None`].
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{CpuBackend, TensorBackend};
    ///
    /// let b = CpuBackend::new();
    /// assert!(!b.capabilities().is_empty());
    /// ```
    fn capabilities(&self) -> &[SimdCapability];

    /// Allocate a zeroed [`TensorBuffer`] matching `desc`.
    ///
    /// Returns `Err(OmniError::Hal { kind: HardwareUnavailable, .. })` if the
    /// requested size exceeds [`TensorBackend::max_tensor_size_bytes`].
    ///
    /// # Errors
    ///
    /// - [`HalErrorKind::HardwareUnavailable`] — requested allocation exceeds
    ///   the backend's size limit.
    async fn allocate(&self, desc: &TensorDescriptor) -> Result<TensorBuffer>;

    /// Execute `op` on the given input buffers and return a new output buffer.
    ///
    /// For Phase 2, `CpuBackend::execute` returns a zeroed buffer of the
    /// correct output shape.  Real SIMD dispatch lands in Phase 4.
    ///
    /// # Errors
    ///
    /// - [`HalErrorKind::HardwareUnavailable`] — no input provided.
    /// - [`HalErrorKind::DeviceFailure`] — unsupported op/dtype combination
    ///   (never happens in the stub; reserved for Phase 4).
    async fn execute(&self, op: TensorOp, inputs: &[&TensorBuffer]) -> Result<TensorBuffer>;

    /// Returns `true` if this backend can process tensors of the given dtype.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{CpuBackend, TensorBackend, TensorDtype};
    ///
    /// let b = CpuBackend::new();
    /// assert!(b.supports_dtype(TensorDtype::F32));
    /// ```
    fn supports_dtype(&self, dtype: TensorDtype) -> bool;

    /// Maximum allocation size in bytes this backend will honour.
    ///
    /// Calls to [`allocate`](TensorBackend::allocate) whose
    /// `desc.byte_size()` exceeds this limit return an error instead of
    /// attempting the allocation.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{CpuBackend, TensorBackend};
    ///
    /// let b = CpuBackend::new();
    /// assert!(b.max_tensor_size_bytes() > 0);
    /// ```
    fn max_tensor_size_bytes(&self) -> usize;
}

// =============================================================================
// CpuBackend
// =============================================================================

/// Default HAL backend that runs tensor ops on the host CPU.
///
/// `CpuBackend` detects available SIMD extensions once at construction time
/// (see [`CpuBackend::new`]) and caches them.  The Phase 2 `execute` method
/// is a stub that returns correctly-shaped zeroed buffers; real SIMD dispatch
/// is scheduled for Phase 4.
///
/// # Default limit
///
/// The default maximum allocation is 4 GiB.  Use [`CpuBackend::with_max_bytes`]
/// to override.
///
/// # Example
///
/// ```
/// use omni_hal::tensor::{CpuBackend, TensorBackend};
///
/// let backend = CpuBackend::new();
/// // Always has at least one capability entry (SimdCapability::None minimum).
/// assert!(!backend.capabilities().is_empty());
/// ```
pub struct CpuBackend {
    // Detected SIMD capability set for this CPU.  Always has at least one
    // entry (SimdCapability::None) so that `capabilities()` never returns an
    // empty slice.
    capabilities: Vec<SimdCapability>,

    // Upper bound on single-allocation size.  Enforced in `allocate`.
    max_bytes: usize,
}

/// 4 GiB in bytes — the default maximum tensor allocation for `CpuBackend`.
///
/// This is a safe upper bound that avoids triggering OOM on 64-bit hosts
/// while still accommodating large language model weight tensors.
const DEFAULT_MAX_BYTES: usize = 4 * 1024 * 1024 * 1024; // 4 GiB

impl CpuBackend {
    /// Create a `CpuBackend` with SIMD auto-detection and a 4 GiB allocation
    /// ceiling.
    ///
    /// SIMD detection uses `is_x86_feature_detected!` on `x86_64` and falls
    /// back to `SimdCapability::None` on all other architectures.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{CpuBackend, TensorBackend};
    ///
    /// let backend = CpuBackend::new();
    /// // Capabilities always contains at least one entry.
    /// assert!(!backend.capabilities().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::with_max_bytes(DEFAULT_MAX_BYTES)
    }

    /// Create a `CpuBackend` with a custom allocation ceiling.
    ///
    /// Useful in constrained test environments or embedded scenarios where
    /// memory budgets are tight.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::tensor::{CpuBackend, TensorBackend};
    ///
    /// let backend = CpuBackend::with_max_bytes(64 * 1024); // 64 KiB limit
    /// assert_eq!(backend.max_tensor_size_bytes(), 64 * 1024);
    /// ```
    #[must_use]
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        let capabilities = detect_simd_capabilities();
        Self {
            capabilities,
            max_bytes,
        }
    }
}

impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect the SIMD capability set available on the current CPU.
///
/// The returned vec always contains at least `SimdCapability::None` as the
/// baseline entry.  On `x86_64` with AVX-512 support, the returned vec is
/// `[None, Sse42, Avx2, Avx512]` (all levels up to and including the
/// strongest detected).
///
/// Detection is performed once per [`CpuBackend`] construction.  Results
/// should be considered stable for the lifetime of a process (CPU features
/// do not change at runtime).
fn detect_simd_capabilities() -> Vec<SimdCapability> {
    let mut caps = vec![SimdCapability::None];

    // x86_64-specific detection using the standard library's
    // `is_x86_feature_detected!` macro.  The macro reads CPUID at runtime;
    // no unsafe code is needed.
    #[cfg(target_arch = "x86_64")]
    {
        // Check in ascending order so the vec is ordered weakest → strongest.
        if is_x86_feature_detected!("sse4.2") {
            caps.push(SimdCapability::Sse42);
        }
        if is_x86_feature_detected!("avx2") {
            caps.push(SimdCapability::Avx2);
        }
        // AVX-512 foundation: avx512f is the base feature.
        if is_x86_feature_detected!("avx512f") {
            caps.push(SimdCapability::Avx512);
        }
    }

    // AArch64: Neon is mandatory on all AArch64 CPUs per the ARM architecture
    // specification.  SVE is optional and queried separately.
    #[cfg(target_arch = "aarch64")]
    {
        // Neon (AdvSIMD) is always present on AArch64.
        caps.push(SimdCapability::Neon);

        #[cfg(target_feature = "sve")]
        caps.push(SimdCapability::SveAarch64);
    }

    caps
}

/// Derive the output descriptor for a given `TensorOp` and input set.
///
/// For Phase 2 this is a simple stub: most ops inherit the first input's
/// shape.  `Reshape` uses the requested `new_shape`.  Returns an error if
/// `inputs` is empty for ops that require at least one input.
fn output_descriptor_for(op: &TensorOp, inputs: &[&TensorBuffer]) -> Result<TensorDescriptor> {
    match op {
        TensorOp::Reshape { new_shape } => {
            // Reshape must have exactly one input; grab the dtype from it.
            let src = inputs.first().ok_or_else(|| {
                OmniError::hal(
                    HalErrorKind::HardwareUnavailable,
                    "execute::reshape::no_input",
                )
            })?;
            Ok(TensorDescriptor::new(
                new_shape.clone(),
                src.descriptor.dtype,
            ))
        }
        // For all other ops: require at least one input and inherit its
        // shape/dtype as the output shape (valid for element-wise ops and
        // as a placeholder for MatMul in Phase 2).
        _ => {
            let src = inputs.first().ok_or_else(|| {
                OmniError::hal(HalErrorKind::HardwareUnavailable, "execute::no_input")
            })?;
            Ok(TensorDescriptor::new(
                src.descriptor.shape.clone(),
                src.descriptor.dtype,
            ))
        }
    }
}

#[async_trait]
impl TensorBackend for CpuBackend {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn capabilities(&self) -> &[SimdCapability] {
        &self.capabilities
    }

    async fn allocate(&self, desc: &TensorDescriptor) -> Result<TensorBuffer> {
        let size = desc.byte_size();
        if size > self.max_bytes {
            tracing::warn!(
                requested_bytes = size,
                limit_bytes = self.max_bytes,
                "CpuBackend::allocate rejected oversized allocation"
            );
            return Err(OmniError::hal(
                HalErrorKind::HardwareUnavailable,
                "allocate::exceeds_max_tensor_size",
            ));
        }

        tracing::trace!(
            shape = ?desc.shape,
            dtype = ?desc.dtype,
            size_bytes = size,
            "CpuBackend::allocate"
        );

        // Allocate a zero-initialised byte buffer.
        let bytes = vec![0u8; size];
        Ok(TensorBuffer::new(desc.clone(), bytes))
    }

    async fn execute(&self, op: TensorOp, inputs: &[&TensorBuffer]) -> Result<TensorBuffer> {
        tracing::trace!(
            op = ?op,
            input_count = inputs.len(),
            "CpuBackend::execute (Phase 2 stub)"
        );

        // Determine the output shape/dtype.
        let out_desc = output_descriptor_for(&op, inputs)?;

        // Phase 2 stub: return a zeroed buffer of the correct size.
        // Real SIMD dispatch (AVX2 matmul, ReLU vectorisation, etc.) is
        // scheduled for Phase 4 once the benchmark harness is in place.
        let size = out_desc.byte_size();
        if size > self.max_bytes {
            return Err(OmniError::hal(
                HalErrorKind::HardwareUnavailable,
                "execute::output_exceeds_max_tensor_size",
            ));
        }

        let bytes = vec![0u8; size];
        Ok(TensorBuffer::new(out_desc, bytes))
    }

    fn supports_dtype(&self, dtype: TensorDtype) -> bool {
        match dtype {
            // F32 is always supported — scalar fallback covers it.
            TensorDtype::F32 | TensorDtype::I8 | TensorDtype::U8 => true,
            // F16 and Bf16 require at minimum AVX-512 on x86_64 for hardware
            // acceleration.  Without it, accuracy/performance would be
            // incorrect, so we conservatively report unsupported.
            TensorDtype::F16 | TensorDtype::Bf16 => {
                self.capabilities.contains(&SimdCapability::Avx512)
                    || self.capabilities.contains(&SimdCapability::Neon)
                    || self.capabilities.contains(&SimdCapability::SveAarch64)
            }
        }
    }

    fn max_tensor_size_bytes(&self) -> usize {
        self.max_bytes
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // TensorDtype
    // -------------------------------------------------------------------------

    #[test]
    fn dtype_bytes_per_element_f32() {
        assert_eq!(TensorDtype::F32.bytes_per_element(), 4);
    }

    #[test]
    fn dtype_bytes_per_element_f16_bf16() {
        assert_eq!(TensorDtype::F16.bytes_per_element(), 2);
        assert_eq!(TensorDtype::Bf16.bytes_per_element(), 2);
    }

    #[test]
    fn dtype_bytes_per_element_i8_u8() {
        assert_eq!(TensorDtype::I8.bytes_per_element(), 1);
        assert_eq!(TensorDtype::U8.bytes_per_element(), 1);
    }

    // -------------------------------------------------------------------------
    // TensorDescriptor
    // -------------------------------------------------------------------------

    #[test]
    fn descriptor_num_elements_2d() {
        let d = TensorDescriptor::new(vec![3, 4], TensorDtype::F32);
        assert_eq!(d.num_elements(), 12);
    }

    #[test]
    fn descriptor_num_elements_scalar() {
        // Empty shape → scalar → 1 element.
        let d = TensorDescriptor::new(vec![], TensorDtype::F32);
        assert_eq!(d.num_elements(), 1);
    }

    #[test]
    fn descriptor_byte_size_f32_matrix() {
        let d = TensorDescriptor::new(vec![2, 3], TensorDtype::F32);
        assert_eq!(d.byte_size(), 24); // 6 × 4 bytes
    }

    #[test]
    fn descriptor_byte_size_f16_vector() {
        let d = TensorDescriptor::new(vec![16], TensorDtype::F16);
        assert_eq!(d.byte_size(), 32); // 16 × 2 bytes
    }

    #[test]
    fn descriptor_named_roundtrip() {
        let d = TensorDescriptor::named(vec![1, 128], TensorDtype::Bf16, "query");
        assert_eq!(d.name.as_deref(), Some("query"));
        assert_eq!(d.shape, vec![1, 128]);
        assert_eq!(d.dtype, TensorDtype::Bf16);
    }

    // -------------------------------------------------------------------------
    // TensorBuffer
    // -------------------------------------------------------------------------

    #[test]
    fn buffer_len_and_not_empty() {
        let desc = TensorDescriptor::new(vec![4], TensorDtype::U8);
        let buf = TensorBuffer::new(desc, vec![1, 2, 3, 4]);
        assert_eq!(buf.len(), 4);
        assert!(!buf.is_empty());
    }

    #[test]
    fn buffer_as_bytes_roundtrip() {
        let desc = TensorDescriptor::new(vec![2], TensorDtype::I8);
        let buf = TensorBuffer::new(desc, vec![10, 20]);
        assert_eq!(buf.as_bytes(), &[10u8, 20u8]);
    }

    // -------------------------------------------------------------------------
    // CpuBackend construction + capability detection
    // -------------------------------------------------------------------------

    #[test]
    fn cpu_backend_capabilities_not_empty() {
        let b = CpuBackend::new();
        assert!(!b.capabilities().is_empty());
    }

    #[test]
    fn cpu_backend_capabilities_always_has_none() {
        let b = CpuBackend::new();
        assert!(b.capabilities().contains(&SimdCapability::None));
    }

    #[test]
    fn cpu_backend_name_is_cpu() {
        let b = CpuBackend::new();
        assert_eq!(b.name(), "cpu");
    }

    #[test]
    fn cpu_backend_default_max_bytes() {
        let b = CpuBackend::new();
        assert_eq!(b.max_tensor_size_bytes(), DEFAULT_MAX_BYTES);
    }

    #[test]
    fn cpu_backend_custom_max_bytes() {
        let b = CpuBackend::with_max_bytes(1024);
        assert_eq!(b.max_tensor_size_bytes(), 1024);
    }

    #[test]
    fn cpu_backend_default_impl_matches_new() {
        let b = CpuBackend::default();
        assert_eq!(b.max_tensor_size_bytes(), DEFAULT_MAX_BYTES);
    }

    // -------------------------------------------------------------------------
    // supports_dtype
    // -------------------------------------------------------------------------

    #[test]
    fn supports_dtype_f32_always() {
        let b = CpuBackend::new();
        assert!(b.supports_dtype(TensorDtype::F32));
    }

    #[test]
    fn supports_dtype_i8_u8_always() {
        let b = CpuBackend::new();
        assert!(b.supports_dtype(TensorDtype::I8));
        assert!(b.supports_dtype(TensorDtype::U8));
    }

    #[test]
    fn supports_dtype_f16_depends_on_avx512_or_neon() {
        let b = CpuBackend::new();
        let has_avx512 = b.capabilities().contains(&SimdCapability::Avx512);
        let has_neon = b.capabilities().contains(&SimdCapability::Neon);
        let has_sve = b.capabilities().contains(&SimdCapability::SveAarch64);
        assert_eq!(
            b.supports_dtype(TensorDtype::F16),
            has_avx512 || has_neon || has_sve
        );
    }

    #[test]
    fn supports_dtype_bf16_same_as_f16() {
        let b = CpuBackend::new();
        assert_eq!(
            b.supports_dtype(TensorDtype::Bf16),
            b.supports_dtype(TensorDtype::F16)
        );
    }

    // -------------------------------------------------------------------------
    // allocate (async)
    //
    // All success-path tests return `Result<()>` so we can use `?` instead of
    // `.expect()` / `.unwrap()`, which are denied by workspace lint policy
    // (`clippy::expect_used` + `clippy::unwrap_used` are promoted to errors
    // via `-D warnings` in CI).  Error-path tests use `.is_err()` / `matches!`
    // to avoid the same lint.
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn allocate_correct_size() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![4, 4], TensorDtype::F32);
        let buf = b.allocate(&desc).await?;
        assert_eq!(buf.len(), 64); // 16 elements × 4 bytes
        Ok(())
    }

    #[tokio::test]
    async fn allocate_zeroed() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![8], TensorDtype::U8);
        let buf = b.allocate(&desc).await?;
        assert!(buf.as_bytes().iter().all(|&x| x == 0));
        Ok(())
    }

    #[tokio::test]
    async fn allocate_scalar_one_element() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![], TensorDtype::F32);
        let buf = b.allocate(&desc).await?;
        assert_eq!(buf.len(), 4); // 1 element × 4 bytes
        Ok(())
    }

    #[tokio::test]
    async fn allocate_rejects_oversized() {
        // Set a tiny limit.
        let b = CpuBackend::with_max_bytes(16);
        let desc = TensorDescriptor::new(vec![100], TensorDtype::F32); // 400 bytes
        let result = b.allocate(&desc).await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(OmniError::Hal {
                kind: HalErrorKind::HardwareUnavailable,
                ..
            })
        ));
    }

    // -------------------------------------------------------------------------
    // execute stubs
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn execute_add_returns_same_shape() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![3, 3], TensorDtype::F32);
        let buf = b.allocate(&desc).await?;
        let out = b.execute(TensorOp::Add, &[&buf, &buf]).await?;
        assert_eq!(out.descriptor.shape, vec![3, 3]);
        assert_eq!(out.len(), 9 * 4);
        Ok(())
    }

    #[tokio::test]
    async fn execute_relu_returns_zeroed_stub() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![4], TensorDtype::F32);
        let buf = b.allocate(&desc).await?;
        let out = b.execute(TensorOp::Relu, &[&buf]).await?;
        assert!(out.as_bytes().iter().all(|&x| x == 0));
        Ok(())
    }

    #[tokio::test]
    async fn execute_softmax_returns_correct_shape() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![1, 10], TensorDtype::F32);
        let buf = b.allocate(&desc).await?;
        let out = b.execute(TensorOp::Softmax { axis: 1 }, &[&buf]).await?;
        assert_eq!(out.descriptor.shape, vec![1, 10]);
        Ok(())
    }

    #[tokio::test]
    async fn execute_reshape_changes_shape() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![2, 6], TensorDtype::F32);
        let buf = b.allocate(&desc).await?;
        let out = b
            .execute(
                TensorOp::Reshape {
                    new_shape: vec![3, 4],
                },
                &[&buf],
            )
            .await?;
        assert_eq!(out.descriptor.shape, vec![3, 4]);
        // byte count unchanged: 12 elements × 4 bytes
        assert_eq!(out.len(), 48);
        Ok(())
    }

    #[tokio::test]
    async fn execute_matmul_stub() -> Result<()> {
        let b = CpuBackend::new();
        let desc = TensorDescriptor::new(vec![2, 4], TensorDtype::F32);
        let a = b.allocate(&desc).await?;
        let out = b
            .execute(
                TensorOp::MatMul {
                    transpose_a: false,
                    transpose_b: false,
                },
                &[&a],
            )
            .await?;
        // Phase 2 stub: output shape equals first input shape.
        assert_eq!(out.descriptor.shape, vec![2, 4]);
        Ok(())
    }

    #[tokio::test]
    async fn execute_no_input_returns_error() {
        let b = CpuBackend::new();
        let result = b.execute(TensorOp::Add, &[]).await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(OmniError::Hal {
                kind: HalErrorKind::HardwareUnavailable,
                ..
            })
        ));
    }
}
