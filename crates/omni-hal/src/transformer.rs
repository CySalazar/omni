//! Transformer inference building blocks — composing `TensorOp` primitives.
//!
//! This module exposes an async forward pass for a decoder-only transformer
//! model in the style of `LLaMA`.  All computation is routed through
//! `CpuBackend` so the same code path works for any backend that implements
//! `TensorBackend`.
//!
//! # Design
//!
//! - Correctness over performance: Phase 2 targets a working forward pass;
//!   SIMD/batching optimisations are deferred to Phase 4.
//! - No `unsafe` code.
//! - Multi-head attention is implemented by processing each head sequentially
//!   because there is no batched-matmul or slice op yet.
//!
//! # Usage
//!
//! ```no_run
//! # use omni_hal::tensor::{CpuBackend, TensorBuffer, TensorDescriptor, TensorDtype};
//! # use omni_hal::transformer::{TransformerConfig, TransformerLayerWeights, TransformerWeights, transformer_forward};
//! # #[tokio::main]
//! # async fn main() -> omni_types::error::Result<()> {
//! let backend = CpuBackend::new();
//! // ... populate config and weights ...
//! # Ok(())
//! # }
//! ```

// Float arithmetic is pervasive in tensor math.
#![allow(clippy::float_arithmetic)]

use crate::tensor::{
    CpuBackend, TensorBackend, TensorBuffer, TensorDescriptor, TensorDtype, TensorOp,
};
use omni_types::error::{HalErrorKind, OmniError, Result};

// =============================================================================
// Configuration and weight types
// =============================================================================

/// Configuration for a transformer model.
///
/// All dimension fields must be consistent:
/// - `d_model` must be divisible by `n_heads`.
/// - `layers` in [`TransformerWeights`] must contain exactly `n_layers` entries.
///
/// # Example
///
/// ```
/// use omni_hal::transformer::TransformerConfig;
///
/// let cfg = TransformerConfig {
///     n_layers: 2, n_heads: 2, d_model: 8, d_ff: 16,
///     vocab_size: 16, max_seq_len: 32, rms_norm_eps: 1e-5,
/// };
/// assert_eq!(cfg.d_model / cfg.n_heads, 4);
/// ```
pub struct TransformerConfig {
    /// Number of transformer layers (decoder blocks).
    pub n_layers: usize,
    /// Number of attention heads.
    pub n_heads: usize,
    /// Model (hidden) dimension.
    pub d_model: usize,
    /// Feed-forward inner dimension.
    pub d_ff: usize,
    /// Vocabulary size (number of distinct token IDs).
    pub vocab_size: usize,
    /// Maximum sequence length the model was trained on.
    pub max_seq_len: usize,
    /// Epsilon for RMS normalization layers.
    pub rms_norm_eps: f32,
}

/// Weights for a single transformer layer.
///
/// All matrices use F32 dtype.  Shape comments assume `d_model = D` and
/// `d_ff = F`.
pub struct TransformerLayerWeights {
    /// Query projection weight: `[D, D]`.
    pub attn_q: TensorBuffer,
    /// Key projection weight: `[D, D]`.
    pub attn_k: TensorBuffer,
    /// Value projection weight: `[D, D]`.
    pub attn_v: TensorBuffer,
    /// Output projection (after concat of heads): `[D, D]`.
    pub attn_o: TensorBuffer,
    /// FFN `SwiGLU` gate weight: `[D, F]`.
    pub ffn_gate: TensorBuffer,
    /// FFN up projection weight: `[D, F]`.
    pub ffn_up: TensorBuffer,
    /// FFN down projection weight: `[F, D]`.
    pub ffn_down: TensorBuffer,
    /// `RMSNorm` scale for attention sub-layer: `[D]`.
    pub attn_norm: TensorBuffer,
    /// `RMSNorm` scale for FFN sub-layer: `[D]`.
    pub ffn_norm: TensorBuffer,
}

/// Complete weights for a transformer model.
pub struct TransformerWeights {
    /// Token embedding table: `[vocab_size, d_model]`.
    pub token_embedding: TensorBuffer,
    /// Per-layer weights; length must equal `TransformerConfig::n_layers`.
    pub layers: Vec<TransformerLayerWeights>,
    /// Final `RMSNorm` scale: `[d_model]`.
    pub output_norm: TensorBuffer,
    /// Output (lm-head) projection: `[d_model, vocab_size]`.
    pub output_proj: TensorBuffer,
}

// =============================================================================
// Internal helpers
// =============================================================================

/// Read a single `f32` from a flat byte slice at element index `idx`.
fn read_f32_local(bytes: &[u8], idx: usize) -> Result<f32> {
    let start = idx.checked_mul(4).ok_or_else(|| {
        OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer::read_f32::overflow",
        )
    })?;
    let end = start.checked_add(4).ok_or_else(|| {
        OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer::read_f32::overflow",
        )
    })?;
    let chunk: [u8; 4] = bytes
        .get(start..end)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| {
            OmniError::hal(
                HalErrorKind::DeviceFailure,
                "transformer::read_f32::out_of_bounds",
            )
        })?;
    Ok(f32::from_le_bytes(chunk))
}

/// Write a single `f32` to a mutable flat byte slice at element index `idx`.
fn write_f32_local(bytes: &mut [u8], idx: usize, val: f32) -> Result<()> {
    let start = idx.checked_mul(4).ok_or_else(|| {
        OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer::write_f32::overflow",
        )
    })?;
    let end = start.checked_add(4).ok_or_else(|| {
        OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer::write_f32::overflow",
        )
    })?;
    let chunk = bytes.get_mut(start..end).ok_or_else(|| {
        OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer::write_f32::out_of_bounds",
        )
    })?;
    chunk.copy_from_slice(&val.to_le_bytes());
    Ok(())
}

/// Extract a single attention head's slice from a `[seq_len, d_model]` tensor.
///
/// Returns a new `[seq_len, d_head]` F32 buffer by copying columns
/// `[head * d_head .. (head+1) * d_head]` from each row.
///
/// This replaces a slice/gather op until Phase 4 adds one.
fn extract_head(
    buf: &TensorBuffer,
    seq_len: usize,
    d_model: usize,
    d_head: usize,
    head: usize,
) -> Result<TensorBuffer> {
    let col_start = head
        .checked_mul(d_head)
        .ok_or_else(|| OmniError::hal(HalErrorKind::DeviceFailure, "extract_head::overflow"))?;
    let col_end = col_start
        .checked_add(d_head)
        .ok_or_else(|| OmniError::hal(HalErrorKind::DeviceFailure, "extract_head::overflow"))?;
    if col_end > d_model {
        return Err(OmniError::hal(
            HalErrorKind::DeviceFailure,
            "extract_head::head_out_of_bounds",
        ));
    }

    let out_desc = TensorDescriptor::new(vec![seq_len, d_head], TensorDtype::F32);
    let mut out_bytes = vec![0u8; out_desc.byte_size()];
    let src = buf.as_bytes();

    for row in 0..seq_len {
        for (col_local, col_global) in (col_start..col_end).enumerate() {
            let src_flat = row * d_model + col_global;
            let dst_flat = row * d_head + col_local;
            let val = read_f32_local(src, src_flat)?;
            write_f32_local(&mut out_bytes, dst_flat, val)?;
        }
    }

    Ok(TensorBuffer::new(out_desc, out_bytes))
}

/// Element-wise multiply of two same-shape F32 buffers (used for `SwiGLU`).
fn elementwise_mul(a: &TensorBuffer, b: &TensorBuffer) -> Result<TensorBuffer> {
    if a.descriptor.shape != b.descriptor.shape {
        return Err(OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer::elementwise_mul::shape_mismatch",
        ));
    }
    let n = a.descriptor.num_elements();
    let out_desc = TensorDescriptor::new(a.descriptor.shape.clone(), TensorDtype::F32);
    let mut out_bytes = vec![0u8; out_desc.byte_size()];
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    for i in 0..n {
        let val = read_f32_local(a_bytes, i)? * read_f32_local(b_bytes, i)?;
        write_f32_local(&mut out_bytes, i, val)?;
    }
    Ok(TensorBuffer::new(out_desc, out_bytes))
}

/// Scale each row of a `[seq_len, d_model]` tensor element-wise by the
/// corresponding entry of a `[d_model]` weight vector (`RMSNorm` affine scale).
///
/// `output[s, i] = input[s, i] * weight[i]`
fn apply_norm_weight(
    normed: &TensorBuffer,
    weight: &TensorBuffer,
    seq_len: usize,
    d_model: usize,
) -> Result<TensorBuffer> {
    let out_desc = TensorDescriptor::new(vec![seq_len, d_model], TensorDtype::F32);
    let mut out_bytes = vec![0u8; out_desc.byte_size()];
    let normed_bytes = normed.as_bytes();
    let w_bytes = weight.as_bytes();

    for s in 0..seq_len {
        for i in 0..d_model {
            let x = read_f32_local(normed_bytes, s * d_model + i)?;
            let w = read_f32_local(w_bytes, i)?;
            write_f32_local(&mut out_bytes, s * d_model + i, x * w)?;
        }
    }

    Ok(TensorBuffer::new(out_desc, out_bytes))
}

// =============================================================================
// Attention sub-layer (extracted to respect the too_many_lines limit)
// =============================================================================

/// Run multi-head self-attention for one layer.
///
/// Inputs `q`, `k`, `v` each have shape `[seq_len, d_model]`.
/// Returns the post-projection output: `[seq_len, d_model]`.
#[allow(clippy::too_many_arguments)]
async fn attention_sublayer(
    backend: &CpuBackend,
    q: &TensorBuffer,
    k: &TensorBuffer,
    v: &TensorBuffer,
    attn_o: &TensorBuffer,
    seq_len: usize,
    d_model: usize,
    n_heads: usize,
    d_head: usize,
) -> Result<TensorBuffer> {
    // Scale factor: 1/sqrt(d_head).  Cast is safe: d_head < 2^23 in practice.
    #[allow(clippy::cast_precision_loss)]
    let scale = 1.0_f32 / (d_head as f32).sqrt();

    let mut head_outputs: Vec<TensorBuffer> = Vec::with_capacity(n_heads);

    for head in 0..n_heads {
        let q_h = extract_head(q, seq_len, d_model, d_head, head)?;
        let k_h = extract_head(k, seq_len, d_model, d_head, head)?;
        let v_h = extract_head(v, seq_len, d_model, d_head, head)?;

        // Attention scores: Q_h @ K_h^T → [seq_len, seq_len].
        let scores = backend
            .execute(
                TensorOp::MatMul {
                    transpose_a: false,
                    transpose_b: true,
                },
                &[&q_h, &k_h],
            )
            .await?;

        let scores = backend
            .execute(TensorOp::Scale { scalar: scale }, &[&scores])
            .await?;

        let attn_weights = backend
            .execute(TensorOp::Softmax { axis: 1 }, &[&scores])
            .await?;

        // Weighted sum: attn_weights @ V_h → [seq_len, d_head].
        let head_out = backend
            .execute(
                TensorOp::MatMul {
                    transpose_a: false,
                    transpose_b: false,
                },
                &[&attn_weights, &v_h],
            )
            .await?;

        head_outputs.push(head_out);
    }

    // Concatenate all head outputs along axis 1 → [seq_len, d_model].
    let head_refs: Vec<&TensorBuffer> = head_outputs.iter().collect();
    let concat_out = backend
        .execute(TensorOp::Concat { axis: 1 }, &head_refs)
        .await?;

    // Output projection: [seq_len, d_model] × [d_model, d_model] → [seq_len, d_model].
    backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&concat_out, attn_o],
        )
        .await
}

/// Run the FFN (`SwiGLU`) sub-layer for one layer.
///
/// `x` has shape `[seq_len, d_model]`.  Returns the output: `[seq_len, d_model]`.
async fn ffn_sublayer(
    backend: &CpuBackend,
    x: &TensorBuffer,
    ffn_gate: &TensorBuffer,
    ffn_up: &TensorBuffer,
    ffn_down: &TensorBuffer,
) -> Result<TensorBuffer> {
    // gate = GeLU(x @ gate_w)
    let gate_pre = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[x, ffn_gate],
        )
        .await?;
    let gate = backend.execute(TensorOp::GeLU, &[&gate_pre]).await?;

    // up = x @ up_w
    let up = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[x, ffn_up],
        )
        .await?;

    // act = gate * up  (element-wise SwiGLU)
    let act = elementwise_mul(&gate, &up)?;

    // down = act @ down_w
    backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&act, ffn_down],
        )
        .await
}

// =============================================================================
// Forward pass
// =============================================================================

/// Run a transformer forward pass: token IDs → logits.
///
/// The forward pass is:
/// 1. Embedding lookup.
/// 2. For each layer: `RMSNorm` → Attention → Residual → `RMSNorm` → FFN → Residual.
/// 3. Final `RMSNorm` → Output projection.
///
/// This is a minimal, correctness-first implementation.  Attention is computed
/// one head at a time to avoid a batched-matmul op.
///
/// # Errors
///
/// Returns an error if any tensor operation fails (shape mismatch, out-of-bounds
/// index, unsupported dtype, etc.).
///
/// # Example
///
/// ```no_run
/// # use omni_hal::tensor::{CpuBackend, TensorBuffer, TensorDescriptor, TensorDtype};
/// # use omni_hal::transformer::{TransformerConfig, TransformerLayerWeights, TransformerWeights, transformer_forward};
/// # #[tokio::main]
/// # async fn main() -> omni_types::error::Result<()> {
/// let backend = CpuBackend::new();
/// // ... populate config and weights ...
/// # Ok(())
/// # }
/// ```
#[allow(clippy::integer_division)]
pub async fn transformer_forward(
    backend: &CpuBackend,
    config: &TransformerConfig,
    weights: &TransformerWeights,
    input_ids: &TensorBuffer,
) -> Result<TensorBuffer> {
    let seq_len =
        input_ids.descriptor.shape.first().copied().ok_or_else(|| {
            OmniError::hal(HalErrorKind::DeviceFailure, "forward::input_ids_shape")
        })?;

    let d_model = config.d_model;
    let n_heads = config.n_heads;

    if d_model == 0 || n_heads == 0 || d_model % n_heads != 0 {
        return Err(OmniError::hal(
            HalErrorKind::DeviceFailure,
            "forward::invalid_config",
        ));
    }
    let d_head = d_model / n_heads;

    // 1. Embedding lookup: [seq_len] → [seq_len, d_model].
    let mut hidden = backend
        .execute(
            TensorOp::EmbeddingLookup,
            &[&weights.token_embedding, input_ids],
        )
        .await?;

    // 2. Per-layer forward pass.
    for layer_weights in &weights.layers {
        hidden = layer_forward(
            backend,
            config,
            layer_weights,
            hidden,
            seq_len,
            d_model,
            n_heads,
            d_head,
        )
        .await?;
    }

    // 3. Final RMSNorm + output_norm weight → output projection.
    let final_normed = backend
        .execute(
            TensorOp::RmsNorm {
                epsilon: config.rms_norm_eps,
            },
            &[&hidden],
        )
        .await?;
    let final_normed = apply_norm_weight(&final_normed, &weights.output_norm, seq_len, d_model)?;

    // Output projection: [seq_len, d_model] × [d_model, vocab_size] → [seq_len, vocab_size].
    backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&final_normed, &weights.output_proj],
        )
        .await
}

/// Run a single transformer layer's forward pass.
///
/// Accepts `hidden: TensorBuffer` by value and returns the updated hidden state.
#[allow(clippy::too_many_arguments)]
async fn layer_forward(
    backend: &CpuBackend,
    config: &TransformerConfig,
    layer_weights: &TransformerLayerWeights,
    hidden: TensorBuffer,
    seq_len: usize,
    d_model: usize,
    n_heads: usize,
    d_head: usize,
) -> Result<TensorBuffer> {
    // Attention sub-layer.
    let normed_attn = backend
        .execute(
            TensorOp::RmsNorm {
                epsilon: config.rms_norm_eps,
            },
            &[&hidden],
        )
        .await?;
    let normed_attn = apply_norm_weight(&normed_attn, &layer_weights.attn_norm, seq_len, d_model)?;

    // Q, K, V projections: [seq_len, d_model] × [d_model, d_model] → [seq_len, d_model].
    let q = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&normed_attn, &layer_weights.attn_q],
        )
        .await?;
    let k = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&normed_attn, &layer_weights.attn_k],
        )
        .await?;
    let v = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&normed_attn, &layer_weights.attn_v],
        )
        .await?;

    let attn_proj = attention_sublayer(
        backend,
        &q,
        &k,
        &v,
        &layer_weights.attn_o,
        seq_len,
        d_model,
        n_heads,
        d_head,
    )
    .await?;

    // Residual add.
    let hidden = backend
        .execute(TensorOp::Add, &[&hidden, &attn_proj])
        .await?;

    // FFN sub-layer.
    let normed_ffn = backend
        .execute(
            TensorOp::RmsNorm {
                epsilon: config.rms_norm_eps,
            },
            &[&hidden],
        )
        .await?;
    let normed_ffn = apply_norm_weight(&normed_ffn, &layer_weights.ffn_norm, seq_len, d_model)?;

    let ffn_out = ffn_sublayer(
        backend,
        &normed_ffn,
        &layer_weights.ffn_gate,
        &layer_weights.ffn_up,
        &layer_weights.ffn_down,
    )
    .await?;

    // Residual add.
    backend.execute(TensorOp::Add, &[&hidden, &ffn_out]).await
}

// =============================================================================
// KV cache
// =============================================================================

/// Per-layer key/value cache for incremental (decode-phase) inference.
///
/// During prefill the cache is populated layer-by-layer as each token is
/// processed.  During decode only the new token's K/V slices are appended;
/// the full cached history is then used for attention computation, giving
/// O(1) token generation cost instead of O(`seq_len`²).
///
/// # Example
///
/// ```
/// use omni_hal::transformer::KvCache;
///
/// let cache = KvCache::new(2, 64, 8, 4);
/// assert_eq!(cache.seq_len(), 0);
/// assert_eq!(cache.num_layers(), 2);
/// ```
pub struct KvCache {
    /// (keys, values) pair per transformer layer, each `[current_seq_len, d_model]` F32.
    layers: Vec<(TensorBuffer, TensorBuffer)>,
    /// Maximum sequence length the cache was allocated for.
    max_seq_len: usize,
    /// Dimension per attention head.
    head_dim: usize,
    /// Number of key/value heads (may differ from query heads in GQA).
    num_kv_heads: usize,
    /// How many token positions have been appended so far.
    current_seq_len: usize,
}

impl KvCache {
    /// Create a new empty `KvCache`.
    ///
    /// Each layer is initialised with a zero-byte buffer (`[0, d_model]`
    /// shape) so that the first [`KvCache::append`] call can simply extend it.
    ///
    /// # Arguments
    ///
    /// * `num_layers`   — number of transformer layers.
    /// * `max_seq_len`  — maximum sequence length that may be cached.
    /// * `head_dim`     — dimension per attention head.
    /// * `num_kv_heads` — number of key/value heads.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::KvCache;
    ///
    /// let cache = KvCache::new(4, 128, 16, 8);
    /// assert_eq!(cache.seq_len(), 0);
    /// assert_eq!(cache.num_layers(), 4);
    /// ```
    #[must_use]
    pub fn new(
        num_layers: usize,
        max_seq_len: usize,
        head_dim: usize,
        num_kv_heads: usize,
    ) -> Self {
        let d_model = head_dim * num_kv_heads;
        // Each layer starts with a 0-row, d_model-column buffer.
        // The empty desc still records the shape so later appends know d_model.
        let empty_layer = || {
            let desc = TensorDescriptor::new(vec![0, d_model], TensorDtype::F32);
            let buf = TensorBuffer::new(desc.clone(), Vec::new());
            (buf, TensorBuffer::new(desc, Vec::new()))
        };
        Self {
            layers: (0..num_layers).map(|_| empty_layer()).collect(),
            max_seq_len,
            head_dim,
            num_kv_heads,
            current_seq_len: 0,
        }
    }

    /// Append new key and value slices for `layer` and return the accumulated
    /// K/V tensors for that layer.
    ///
    /// `new_keys` and `new_values` must have shape `[new_tokens, d_model]`
    /// where `d_model == head_dim * num_kv_heads`.
    ///
    /// `current_seq_len` is only incremented when `layer == 0` to avoid
    /// double-counting across layers that process the same token batch.
    ///
    /// Returns `(&keys, &values)` for the full accumulated sequence.
    ///
    /// # Errors
    ///
    /// Returns an error if the resulting sequence length would exceed
    /// `max_seq_len`.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::{KvCache};
    /// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
    ///
    /// let mut cache = KvCache::new(1, 16, 4, 2);
    /// let d_model = 4 * 2; // head_dim * num_kv_heads
    /// let desc = TensorDescriptor::new(vec![1, d_model], TensorDtype::F32);
    /// let zeros = vec![0u8; desc.byte_size()];
    /// let k = TensorBuffer::new(desc.clone(), zeros.clone());
    /// let v = TensorBuffer::new(desc, zeros);
    /// let _ = cache.append(0, &k, &v).unwrap();
    /// assert_eq!(cache.seq_len(), 1);
    /// ```
    pub fn append(
        &mut self,
        layer: usize,
        new_keys: &TensorBuffer,
        new_values: &TensorBuffer,
    ) -> Result<(&TensorBuffer, &TensorBuffer)> {
        let (cached_k, cached_v) = self.layers.get_mut(layer).ok_or_else(|| {
            OmniError::hal(
                HalErrorKind::DeviceFailure,
                "kv_cache::append::layer_out_of_bounds",
            )
        })?;

        // Determine how many new tokens are being appended.
        let new_tokens = new_keys.descriptor.shape.first().copied().unwrap_or(0);

        let new_seq = self.current_seq_len + new_tokens;
        if new_seq > self.max_seq_len {
            return Err(OmniError::hal(
                HalErrorKind::DeviceFailure,
                "kv_cache::append::max_seq_len_exceeded",
            ));
        }

        // Concatenate bytes: existing cache rows first, then new rows.
        let d_model = self.head_dim * self.num_kv_heads;
        let total_seq = cached_k.descriptor.shape.first().copied().unwrap_or(0) + new_tokens;

        let accumulated_k: Vec<u8> = cached_k
            .as_bytes()
            .iter()
            .chain(new_keys.as_bytes())
            .copied()
            .collect();
        let accumulated_v: Vec<u8> = cached_v
            .as_bytes()
            .iter()
            .chain(new_values.as_bytes())
            .copied()
            .collect();

        let k_desc = TensorDescriptor::new(vec![total_seq, d_model], TensorDtype::F32);
        let v_desc = TensorDescriptor::new(vec![total_seq, d_model], TensorDtype::F32);

        *cached_k = TensorBuffer::new(k_desc, accumulated_k);
        *cached_v = TensorBuffer::new(v_desc, accumulated_v);

        // Only layer 0 advances the sequence counter to avoid double-counting.
        if layer == 0 {
            self.current_seq_len = new_seq;
        }

        let (k_ref, v_ref) = self.layers.get(layer).ok_or_else(|| {
            OmniError::hal(
                HalErrorKind::DeviceFailure,
                "kv_cache::append::layer_out_of_bounds",
            )
        })?;
        Ok((k_ref, v_ref))
    }

    /// Current number of cached token positions.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::KvCache;
    ///
    /// let cache = KvCache::new(2, 32, 8, 4);
    /// assert_eq!(cache.seq_len(), 0);
    /// ```
    #[must_use]
    pub fn seq_len(&self) -> usize {
        self.current_seq_len
    }

    /// Reset all layers to empty, discarding the cached K/V history.
    ///
    /// After this call [`KvCache::seq_len`] returns `0`.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::{KvCache};
    /// use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
    ///
    /// let mut cache = KvCache::new(1, 16, 4, 2);
    /// let d_model = 4 * 2;
    /// let desc = TensorDescriptor::new(vec![1, d_model], TensorDtype::F32);
    /// let zeros = vec![0u8; desc.byte_size()];
    /// let k = TensorBuffer::new(desc.clone(), zeros.clone());
    /// let v = TensorBuffer::new(desc, zeros);
    /// let _ = cache.append(0, &k, &v).unwrap();
    /// assert_eq!(cache.seq_len(), 1);
    /// cache.reset();
    /// assert_eq!(cache.seq_len(), 0);
    /// ```
    pub fn reset(&mut self) {
        let d_model = self.head_dim * self.num_kv_heads;
        for (k, v) in &mut self.layers {
            let desc = TensorDescriptor::new(vec![0, d_model], TensorDtype::F32);
            *k = TensorBuffer::new(desc.clone(), Vec::new());
            *v = TensorBuffer::new(desc, Vec::new());
        }
        self.current_seq_len = 0;
    }

    /// Number of transformer layers this cache covers.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::KvCache;
    ///
    /// let cache = KvCache::new(6, 128, 16, 8);
    /// assert_eq!(cache.num_layers(), 6);
    /// ```
    #[must_use]
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }
}

// =============================================================================
// Cached forward pass (synchronous convenience wrapper)
// =============================================================================

/// Run a transformer forward pass with KV caching, returning logits for the
/// last token position as a flat `Vec<f32>`.
///
/// This function wraps the async backend in a synchronous interface by driving
/// the Tokio runtime internally.  It is intended for use in the batched
/// inference loop where the caller does not want to manage async plumbing.
///
/// # Prefill vs decode
///
/// - **Prefill** (`cache.seq_len() == 0`): all `input_ids` tokens are processed
///   in a single pass, building up the KV cache from scratch.
/// - **Decode** (`cache.seq_len() > 0`): only the new token(s) are projected
///   and appended to the existing cache; attention uses the full cached history.
///
/// Both paths return logits only for the last token position.
///
/// # Arguments
///
/// * `config`    — model hyperparameters.
/// * `weights`   — model weight tensors.
/// * `input_ids` — token IDs to process (usually a single token in decode mode).
/// * `kv_cache`  — mutable KV cache that is updated in-place.
///
/// # Errors
///
/// Returns an error if any tensor operation fails (shape mismatch, out-of-bounds,
/// unsupported dtype, etc.) or if the sequence length would exceed
/// `config.max_seq_len`.
///
/// # Example
///
/// ```no_run
/// # use omni_hal::transformer::{TransformerConfig, TransformerWeights, TransformerLayerWeights, KvCache, transformer_forward_cached};
/// # use omni_hal::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};
/// # fn make_zeros(shape: Vec<usize>) -> TensorBuffer {
/// #     let desc = TensorDescriptor::new(shape, TensorDtype::F32);
/// #     let bytes = vec![0u8; desc.byte_size()];
/// #     TensorBuffer::new(desc, bytes)
/// # }
/// let config = TransformerConfig {
///     n_layers: 1, n_heads: 2, d_model: 8, d_ff: 16,
///     vocab_size: 16, max_seq_len: 32, rms_norm_eps: 1e-5,
/// };
/// // ... populate weights ...
/// # let weights = TransformerWeights {
/// #     token_embedding: make_zeros(vec![16, 8]),
/// #     layers: vec![TransformerLayerWeights {
/// #         attn_q: make_zeros(vec![8, 8]), attn_k: make_zeros(vec![8, 8]),
/// #         attn_v: make_zeros(vec![8, 8]), attn_o: make_zeros(vec![8, 8]),
/// #         ffn_gate: make_zeros(vec![8, 16]), ffn_up: make_zeros(vec![8, 16]),
/// #         ffn_down: make_zeros(vec![16, 8]),
/// #         attn_norm: make_zeros(vec![8]), ffn_norm: make_zeros(vec![8]),
/// #     }],
/// #     output_norm: make_zeros(vec![8]),
/// #     output_proj: make_zeros(vec![8, 16]),
/// # };
/// let mut cache = KvCache::new(1, 32, 4, 2);
/// let logits = transformer_forward_cached(&config, &weights, &[42u32], &mut cache).unwrap();
/// assert_eq!(logits.len(), 16);
/// ```
#[allow(clippy::integer_division)]
pub fn transformer_forward_cached(
    config: &TransformerConfig,
    weights: &TransformerWeights,
    input_ids: &[u32],
    kv_cache: &mut KvCache,
) -> Result<Vec<f32>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| {
            OmniError::hal(
                HalErrorKind::DeviceFailure,
                // Leak the string so we can use the &'static str API.
                // This path is only hit on runtime construction failure (extremely rare).
                Box::leak(
                    format!("transformer_forward_cached::runtime_build::{e}").into_boxed_str(),
                ),
            )
        })?;
    rt.block_on(transformer_forward_cached_async(
        config, weights, input_ids, kv_cache,
    ))
}

/// Async implementation of the cached transformer forward pass.
///
/// Called from [`transformer_forward_cached`] via `block_on`.
#[allow(clippy::integer_division)]
async fn transformer_forward_cached_async(
    config: &TransformerConfig,
    weights: &TransformerWeights,
    input_ids: &[u32],
    kv_cache: &mut KvCache,
) -> Result<Vec<f32>> {
    let backend = CpuBackend::new();
    let seq_len = input_ids.len();

    if seq_len == 0 {
        return Err(OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer_forward_cached::empty_input",
        ));
    }

    let d_model = config.d_model;
    let n_heads = config.n_heads;

    if d_model == 0 || n_heads == 0 || d_model % n_heads != 0 {
        return Err(OmniError::hal(
            HalErrorKind::DeviceFailure,
            "transformer_forward_cached::invalid_config",
        ));
    }
    let d_head = d_model / n_heads;

    // Build a 1-D U8 index buffer from input_ids so we can reuse EmbeddingLookup.
    // The existing EmbeddingLookup op reads byte values as row indices.
    // We cast each u32 token id to u8 (tokens are assumed < 256 in this stub).
    let idx_desc = TensorDescriptor::new(vec![seq_len], TensorDtype::U8);
    // Cast to u8: valid for small-vocab test models (token ids < 256).
    #[allow(clippy::cast_possible_truncation)]
    let idx_bytes: Vec<u8> = input_ids.iter().map(|&id| id as u8).collect();
    let idx_buf = TensorBuffer::new(idx_desc, idx_bytes);

    // 1. Embedding lookup: [seq_len] → [seq_len, d_model].
    let mut hidden = backend
        .execute(
            TensorOp::EmbeddingLookup,
            &[&weights.token_embedding, &idx_buf],
        )
        .await?;

    // 2. Per-layer forward pass with KV cache update.
    for (layer_idx, layer_weights) in weights.layers.iter().enumerate() {
        hidden = layer_forward_cached(
            &backend,
            config,
            layer_weights,
            hidden,
            seq_len,
            d_model,
            n_heads,
            d_head,
            kv_cache,
            layer_idx,
        )
        .await?;
    }

    // 3. Final RMSNorm + output projection.
    let final_normed = backend
        .execute(
            TensorOp::RmsNorm {
                epsilon: config.rms_norm_eps,
            },
            &[&hidden],
        )
        .await?;
    let final_normed = apply_norm_weight(&final_normed, &weights.output_norm, seq_len, d_model)?;

    // Output projection: [seq_len, d_model] × [d_model, vocab_size] → [seq_len, vocab_size].
    let logits_buf = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&final_normed, &weights.output_proj],
        )
        .await?;

    // Extract the last token's logits row: [vocab_size].
    let vocab_size = config.vocab_size;
    let last_row_start = (seq_len - 1) * vocab_size;
    let logits_bytes = logits_buf.as_bytes();
    let mut logits = Vec::with_capacity(vocab_size);
    for i in 0..vocab_size {
        logits.push(read_f32_local(logits_bytes, last_row_start + i)?);
    }

    Ok(logits)
}

/// Run a single transformer layer with KV cache integration.
///
/// Computes Q/K/V projections, appends K/V to the cache, then runs
/// attention over the full cached K/V history before the FFN sub-layer.
#[allow(clippy::too_many_arguments)]
async fn layer_forward_cached(
    backend: &CpuBackend,
    config: &TransformerConfig,
    layer_weights: &TransformerLayerWeights,
    hidden: TensorBuffer,
    seq_len: usize,
    d_model: usize,
    n_heads: usize,
    d_head: usize,
    kv_cache: &mut KvCache,
    layer_idx: usize,
) -> Result<TensorBuffer> {
    // Attention sub-layer with RMSNorm pre-conditioning.
    let normed_attn = backend
        .execute(
            TensorOp::RmsNorm {
                epsilon: config.rms_norm_eps,
            },
            &[&hidden],
        )
        .await?;
    let normed_attn = apply_norm_weight(&normed_attn, &layer_weights.attn_norm, seq_len, d_model)?;

    // Q/K/V projections for the current token(s): [seq_len, d_model].
    let q = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&normed_attn, &layer_weights.attn_q],
        )
        .await?;
    let k_new = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&normed_attn, &layer_weights.attn_k],
        )
        .await?;
    let v_new = backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&normed_attn, &layer_weights.attn_v],
        )
        .await?;

    // Append K/V to cache and get accumulated history.
    let (k_full, v_full) = kv_cache.append(layer_idx, &k_new, &v_new)?;
    let cached_seq_len = k_full.descriptor.shape.first().copied().unwrap_or(seq_len);

    // Attention over the full cached K/V: Q is [seq_len, d_model],
    // K/V are [cached_seq_len, d_model].
    let attn_proj = attention_sublayer_cached(
        backend,
        &q,
        k_full,
        v_full,
        &layer_weights.attn_o,
        seq_len,
        cached_seq_len,
        d_model,
        n_heads,
        d_head,
    )
    .await?;

    // Residual add.
    let hidden = backend
        .execute(TensorOp::Add, &[&hidden, &attn_proj])
        .await?;

    // FFN sub-layer.
    let normed_ffn = backend
        .execute(
            TensorOp::RmsNorm {
                epsilon: config.rms_norm_eps,
            },
            &[&hidden],
        )
        .await?;
    let normed_ffn = apply_norm_weight(&normed_ffn, &layer_weights.ffn_norm, seq_len, d_model)?;

    let ffn_out = ffn_sublayer(
        backend,
        &normed_ffn,
        &layer_weights.ffn_gate,
        &layer_weights.ffn_up,
        &layer_weights.ffn_down,
    )
    .await?;

    backend.execute(TensorOp::Add, &[&hidden, &ffn_out]).await
}

/// Multi-head attention with potentially different Q `seq_len` and K/V `seq_len`.
///
/// `q` has shape `[q_seq, d_model]` (current tokens).
/// `k` and `v` have shape `[kv_seq, d_model]` (full cached history).
/// Scores matrix is `[q_seq, kv_seq]`.
#[allow(clippy::too_many_arguments)]
async fn attention_sublayer_cached(
    backend: &CpuBackend,
    q: &TensorBuffer,
    k: &TensorBuffer,
    v: &TensorBuffer,
    attn_o: &TensorBuffer,
    q_seq: usize,
    kv_seq: usize,
    d_model: usize,
    n_heads: usize,
    d_head: usize,
) -> Result<TensorBuffer> {
    #[allow(clippy::cast_precision_loss)]
    let scale = 1.0_f32 / (d_head as f32).sqrt();

    let mut head_outputs: Vec<TensorBuffer> = Vec::with_capacity(n_heads);

    for head in 0..n_heads {
        // Q: [q_seq, d_head], K: [kv_seq, d_head], V: [kv_seq, d_head].
        let q_h = extract_head(q, q_seq, d_model, d_head, head)?;
        let k_h = extract_head(k, kv_seq, d_model, d_head, head)?;
        let v_h = extract_head(v, kv_seq, d_model, d_head, head)?;

        // Scores: Q_h @ K_h^T → [q_seq, kv_seq].
        let scores = backend
            .execute(
                TensorOp::MatMul {
                    transpose_a: false,
                    transpose_b: true,
                },
                &[&q_h, &k_h],
            )
            .await?;
        let scores = backend
            .execute(TensorOp::Scale { scalar: scale }, &[&scores])
            .await?;
        let attn_weights = backend
            .execute(TensorOp::Softmax { axis: 1 }, &[&scores])
            .await?;

        // Weighted sum: [q_seq, kv_seq] @ [kv_seq, d_head] → [q_seq, d_head].
        let head_out = backend
            .execute(
                TensorOp::MatMul {
                    transpose_a: false,
                    transpose_b: false,
                },
                &[&attn_weights, &v_h],
            )
            .await?;
        head_outputs.push(head_out);
    }

    // Concatenate heads: [q_seq, d_model].
    let head_refs: Vec<&TensorBuffer> = head_outputs.iter().collect();
    let concat_out = backend
        .execute(TensorOp::Concat { axis: 1 }, &head_refs)
        .await?;

    // Output projection: [q_seq, d_model].
    backend
        .execute(
            TensorOp::MatMul {
                transpose_a: false,
                transpose_b: false,
            },
            &[&concat_out, attn_o],
        )
        .await
}

// =============================================================================
// Batched inference
// =============================================================================

/// A slot holding one active generation sequence.
struct SequenceSlot {
    /// All token IDs seen so far (prompt + generated).
    tokens: Vec<u32>,
    /// KV cache for this sequence.
    cache: KvCache,
    /// Whether the sequence has finished (EOS or `max_len` reached).
    done: bool,
    /// The token that will be processed on the next `step` call.
    next_token: u32,
}

/// Continuous-batching engine for concurrent token generation.
///
/// Each slot independently holds a generation sequence and its KV cache.
/// Calling [`BatchedInference::step`] advances every active slot by one
/// decode step, returning the predicted next token per slot.
///
/// # Example
///
/// ```
/// use omni_hal::transformer::BatchedInference;
///
/// let bi = BatchedInference::new(4, 128);
/// assert_eq!(bi.active_count(), 0);
/// ```
pub struct BatchedInference {
    /// Fixed-size pool of sequence slots; `None` means the slot is free.
    slots: Vec<Option<SequenceSlot>>,
    /// Maximum sequence length for KV caches in this batch.
    max_seq_len: usize,
}

impl BatchedInference {
    /// Create a new `BatchedInference` engine with `batch_size` slots.
    ///
    /// All slots start empty.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::BatchedInference;
    ///
    /// let bi = BatchedInference::new(8, 256);
    /// assert_eq!(bi.active_count(), 0);
    /// ```
    #[must_use]
    pub fn new(batch_size: usize, max_seq_len: usize) -> Self {
        Self {
            slots: (0..batch_size).map(|_| None).collect(),
            max_seq_len,
        }
    }

    /// Add a new generation sequence to the first available slot.
    ///
    /// The last token in `tokens` is used as the starting `next_token` for the
    /// first decode step.  The KV cache dimensions are derived from the model
    /// config that will be passed to [`BatchedInference::step`]; here they are
    /// left at placeholder values and will be populated on the first step.
    ///
    /// Returns the slot ID on success.
    ///
    /// # Errors
    ///
    /// Returns an error if all slots are occupied.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::BatchedInference;
    ///
    /// let mut bi = BatchedInference::new(2, 64);
    /// let id = bi.add_sequence(vec![1u32, 2, 3]).unwrap();
    /// assert_eq!(id, 0);
    /// assert_eq!(bi.active_count(), 1);
    /// ```
    pub fn add_sequence(&mut self, tokens: Vec<u32>) -> Result<usize> {
        let last_token = tokens.last().copied().unwrap_or(0);
        for (idx, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                // KV cache dimensions are placeholders; they are resized on the
                // first step when the TransformerConfig is available.
                *slot = Some(SequenceSlot {
                    tokens,
                    cache: KvCache::new(0, self.max_seq_len, 1, 1),
                    done: false,
                    next_token: last_token,
                });
                return Ok(idx);
            }
        }
        Err(OmniError::hal(
            HalErrorKind::DeviceFailure,
            "batched_inference::add_sequence::all_slots_full",
        ))
    }

    /// Advance every active slot by one decode step.
    ///
    /// For each occupied, non-done slot:
    /// 1. The KV cache is (re-)initialised if its layer count does not match
    ///    `config.n_layers` (handles the first decode step after `add_sequence`).
    /// 2. [`transformer_forward_cached`] is called with the slot's `next_token`.
    /// 3. The argmax of the returned logits becomes the predicted next token.
    /// 4. The slot is marked done if the predicted token is EOS (id `2`) or the
    ///    total token count has reached `max_seq_len`.
    ///
    /// Returns `(slot_id, predicted_token)` pairs for every active slot.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying forward pass fails.
    pub fn step(
        &mut self,
        config: &TransformerConfig,
        weights: &TransformerWeights,
    ) -> Result<Vec<(usize, u32)>> {
        #[allow(clippy::integer_division)]
        let d_head = if config.n_heads > 0 {
            config.d_model / config.n_heads
        } else {
            1
        };
        let mut results = Vec::new();

        for (slot_id, slot_opt) in self.slots.iter_mut().enumerate() {
            let slot = match slot_opt {
                Some(s) if !s.done => s,
                _ => continue,
            };

            // Re-initialise cache if this is the first step (placeholder cache
            // has 0 layers) or if the config changed.
            if slot.cache.num_layers() == config.n_layers {
                // Decode: one new token at a time.
                let logits = transformer_forward_cached(
                    config,
                    weights,
                    &[slot.next_token],
                    &mut slot.cache,
                )?;
                let predicted = argmax_f32(&logits);
                slot.tokens.push(predicted);
                slot.next_token = predicted;
                if predicted == 2 || slot.tokens.len() >= self.max_seq_len {
                    slot.done = true;
                }
                results.push((slot_id, predicted));
            } else {
                slot.cache =
                    KvCache::new(config.n_layers, self.max_seq_len, d_head, config.n_heads);
                // Prefill: run the full prompt through the cache.
                let prompt_tokens: Vec<u32> = slot.tokens.clone();
                let logits =
                    transformer_forward_cached(config, weights, &prompt_tokens, &mut slot.cache)?;
                let predicted = argmax_f32(&logits);
                slot.tokens.push(predicted);
                slot.next_token = predicted;
                if predicted == 2 || slot.tokens.len() >= self.max_seq_len {
                    slot.done = true;
                }
                results.push((slot_id, predicted));
            }
        }

        Ok(results)
    }

    /// Return `true` if the sequence in `slot_id` has finished generation.
    ///
    /// Returns `false` for an out-of-range slot ID or a slot that was never
    /// populated.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::BatchedInference;
    ///
    /// let mut bi = BatchedInference::new(2, 64);
    /// let id = bi.add_sequence(vec![1u32]).unwrap();
    /// assert!(!bi.is_done(id));
    /// ```
    #[must_use]
    pub fn is_done(&self, slot_id: usize) -> bool {
        self.slots
            .get(slot_id)
            .and_then(|s| s.as_ref())
            .is_some_and(|s| s.done)
    }

    /// Number of slots that currently hold an active (non-done) sequence.
    ///
    /// # Example
    ///
    /// ```
    /// use omni_hal::transformer::BatchedInference;
    ///
    /// let bi = BatchedInference::new(4, 64);
    /// assert_eq!(bi.active_count(), 0);
    /// ```
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| s.as_ref().is_some_and(|slot| !slot.done))
            .count()
    }
}

/// Return the index of the maximum element in a `f32` slice.
///
/// Returns `0` for an empty slice (safe fallback).
fn argmax_f32(values: &[f32]) -> u32 {
    let mut best_idx = 0usize;
    let mut best_val = f32::NEG_INFINITY;
    for (i, &v) in values.iter().enumerate() {
        if v > best_val {
            best_val = v;
            best_idx = i;
        }
    }
    // Cast is safe: vocab_size is always < u32::MAX in practice.
    #[allow(clippy::cast_possible_truncation)]
    let result = best_idx as u32;
    result
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::{TensorBuffer, TensorDescriptor, TensorDtype};

    /// Build a zero-filled F32 TensorBuffer of the given shape.
    fn zeros(shape: Vec<usize>) -> TensorBuffer {
        let desc = TensorDescriptor::new(shape, TensorDtype::F32);
        let bytes = vec![0u8; desc.byte_size()];
        TensorBuffer::new(desc, bytes)
    }

    /// Build a minimal TransformerConfig for use in tests.
    fn tiny_config() -> TransformerConfig {
        TransformerConfig {
            n_layers: 2,
            n_heads: 2,
            d_model: 8,
            d_ff: 16,
            vocab_size: 16,
            max_seq_len: 32,
            rms_norm_eps: 1e-5,
        }
    }

    /// Build TransformerWeights matching `tiny_config`.
    fn tiny_weights() -> TransformerWeights {
        let layer = || TransformerLayerWeights {
            attn_q: zeros(vec![8, 8]),
            attn_k: zeros(vec![8, 8]),
            attn_v: zeros(vec![8, 8]),
            attn_o: zeros(vec![8, 8]),
            ffn_gate: zeros(vec![8, 16]),
            ffn_up: zeros(vec![8, 16]),
            ffn_down: zeros(vec![16, 8]),
            attn_norm: zeros(vec![8]),
            ffn_norm: zeros(vec![8]),
        };
        TransformerWeights {
            token_embedding: zeros(vec![16, 8]),
            layers: vec![layer(), layer()],
            output_norm: zeros(vec![8]),
            output_proj: zeros(vec![8, 16]),
        }
    }

    #[test]
    fn test_kv_cache_new() {
        let cache = KvCache::new(4, 128, 16, 8);
        assert_eq!(cache.num_layers(), 4);
        assert_eq!(cache.seq_len(), 0);
        assert_eq!(cache.head_dim, 16);
        assert_eq!(cache.num_kv_heads, 8);
        assert_eq!(cache.max_seq_len, 128);
    }

    #[test]
    fn test_kv_cache_append() {
        let mut cache = KvCache::new(2, 16, 4, 2);
        let d_model = 4 * 2; // head_dim * num_kv_heads
        let desc = TensorDescriptor::new(vec![3, d_model], TensorDtype::F32);
        let bytes = vec![0u8; desc.byte_size()];
        let k = TensorBuffer::new(desc.clone(), bytes.clone());
        let v = TensorBuffer::new(desc, bytes);
        let (k_ref, v_ref) = cache.append(0, &k, &v).unwrap();
        assert_eq!(k_ref.descriptor.shape, vec![3, d_model]);
        assert_eq!(v_ref.descriptor.shape, vec![3, d_model]);
        assert_eq!(cache.seq_len(), 3);
    }

    #[test]
    fn test_kv_cache_reset() {
        let mut cache = KvCache::new(1, 16, 4, 2);
        let d_model = 4 * 2;
        let desc = TensorDescriptor::new(vec![2, d_model], TensorDtype::F32);
        let bytes = vec![0u8; desc.byte_size()];
        let k = TensorBuffer::new(desc.clone(), bytes.clone());
        let v = TensorBuffer::new(desc, bytes);
        cache.append(0, &k, &v).unwrap();
        assert_eq!(cache.seq_len(), 2);
        cache.reset();
        assert_eq!(cache.seq_len(), 0);
        // Layer buffers should be empty after reset.
        assert!(cache.layers[0].0.is_empty());
        assert!(cache.layers[0].1.is_empty());
    }

    #[test]
    fn test_kv_cache_multi_layer() {
        let mut cache = KvCache::new(3, 16, 4, 2);
        let d_model = 8;

        // Append 2 tokens to layer 0 (advances seq_len).
        let desc0 = TensorDescriptor::new(vec![2, d_model], TensorDtype::F32);
        let bytes0 = vec![0u8; desc0.byte_size()];
        let k0 = TensorBuffer::new(desc0.clone(), bytes0.clone());
        let v0 = TensorBuffer::new(desc0, bytes0);
        cache.append(0, &k0, &v0).unwrap();
        assert_eq!(cache.seq_len(), 2);

        // Append 2 tokens to layer 1 (does NOT change seq_len).
        let desc1 = TensorDescriptor::new(vec![2, d_model], TensorDtype::F32);
        let bytes1 = vec![0u8; desc1.byte_size()];
        let k1 = TensorBuffer::new(desc1.clone(), bytes1.clone());
        let v1 = TensorBuffer::new(desc1, bytes1);
        cache.append(1, &k1, &v1).unwrap();
        assert_eq!(cache.seq_len(), 2);

        // Layer 2 should still be empty.
        assert_eq!(cache.layers[2].0.descriptor.shape[0], 0);
    }

    #[test]
    fn test_batched_inference_new() {
        let bi = BatchedInference::new(4, 128);
        assert_eq!(bi.active_count(), 0);
        assert_eq!(bi.slots.len(), 4);
        assert_eq!(bi.max_seq_len, 128);
    }

    #[test]
    fn test_batched_inference_add_sequence() {
        let mut bi = BatchedInference::new(4, 64);
        let id = bi.add_sequence(vec![1u32, 2, 3]).unwrap();
        assert_eq!(id, 0);
        assert_eq!(bi.active_count(), 1);

        let id2 = bi.add_sequence(vec![5u32]).unwrap();
        assert_eq!(id2, 1);
        assert_eq!(bi.active_count(), 2);
    }

    #[test]
    fn test_batched_inference_add_full() {
        let mut bi = BatchedInference::new(2, 64);
        bi.add_sequence(vec![1u32]).unwrap();
        bi.add_sequence(vec![2u32]).unwrap();
        // Third add must fail.
        let result = bi.add_sequence(vec![3u32]);
        assert!(result.is_err());
    }

    #[test]
    fn test_batched_inference_is_done() {
        let mut bi = BatchedInference::new(2, 64);
        let id = bi.add_sequence(vec![1u32]).unwrap();
        assert!(!bi.is_done(id));
        // Out-of-range slot also returns false.
        assert!(!bi.is_done(99));
    }

    #[test]
    fn test_transformer_forward_cached_prefill() {
        let config = tiny_config();
        let weights = tiny_weights();
        let mut cache = KvCache::new(
            config.n_layers,
            config.max_seq_len,
            config.d_model / config.n_heads,
            config.n_heads,
        );
        let logits =
            transformer_forward_cached(&config, &weights, &[1u32, 2, 3], &mut cache).unwrap();
        assert_eq!(logits.len(), config.vocab_size);
        assert_eq!(cache.seq_len(), 3);
    }

    #[test]
    fn test_transformer_forward_cached_decode() {
        let config = tiny_config();
        let weights = tiny_weights();
        let mut cache = KvCache::new(
            config.n_layers,
            config.max_seq_len,
            config.d_model / config.n_heads,
            config.n_heads,
        );
        // Prefill with 3 tokens.
        transformer_forward_cached(&config, &weights, &[1u32, 2, 3], &mut cache).unwrap();
        assert_eq!(cache.seq_len(), 3);
        // Decode one more token.
        let logits = transformer_forward_cached(&config, &weights, &[4u32], &mut cache).unwrap();
        assert_eq!(logits.len(), config.vocab_size);
        assert_eq!(cache.seq_len(), 4);
    }
}
