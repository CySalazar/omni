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
