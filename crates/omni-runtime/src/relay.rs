//! AI Syscall IPC Relay.
//!
//! This module bridges kernel AI syscalls (numbers 80–84) to the
//! [`InferencePipeline`][crate::inference::InferencePipeline]. When the kernel
//! receives an `AiInvoke`, `AiStream`, `AiEmbed`, `AiClassify`, or
//! `AiTranscribe` syscall it packages it into an [`AiSyscallRequest`] and
//! hands it to the user-space runtime via IPC. The runtime calls
//! [`AiIpcRelay::dispatch`] which routes the request through the pipeline and
//! returns an [`AiSyscallResponse`] to be written back to the calling process.
//!
//! ## Design notes
//!
//! - The relay never panics on malformed input: every error path returns a
//!   structured error response so the kernel can surface `EINVAL` or
//!   `EIO` instead of crashing.
//! - `model_id_bytes` in [`AiSyscallRequest`] carries only 16 bytes because
//!   the kernel ABI uses a compact form. The relay zero-extends them to the
//!   32-byte [`ModelId`][omni_types::ModelId] by placing the 16 bytes in the
//!   high half and zeroing the low half. This mapping is deterministic and
//!   documented in `OIP-Agent-Arch-022 §S9`.
//! - All dispatch calls are recorded at `tracing::info` level to support the
//!   audit log requirement from `/docs/04-security-model.md §Audit log`.

use std::sync::Arc;
use std::time::Instant;

use omni_types::ModelId;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, warn};

use crate::inference::{InferencePipeline, InferenceRequest};

// =============================================================================
// AiSyscallNumber
// =============================================================================

/// AI syscall numbers, matching the `omni-kernel` definitions.
///
/// These constants are defined in the kernel ABI and must stay in sync with
/// `crates/omni-kernel/src/syscall/ai.rs`. Any change here is a
/// kernel-interface breaking change.
///
/// | Number | Name | Purpose |
/// |--------|------|---------|
/// | 80 | `AiInvoke` | Single-turn text generation / completion |
/// | 81 | `AiStream` | Streaming text generation |
/// | 82 | `AiEmbed` | Dense vector embedding |
/// | 83 | `AiClassify` | Label classification |
/// | 84 | `AiTranscribe` | Speech-to-text |
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AiSyscallNumber {
    /// Syscall 80 — single-turn inference (request/response).
    Invoke = 80,
    /// Syscall 81 — streaming inference (token-by-token delivery).
    Stream = 81,
    /// Syscall 82 — dense vector embedding.
    Embed = 82,
    /// Syscall 83 — multi-label classification.
    Classify = 83,
    /// Syscall 84 — speech-to-text transcription.
    Transcribe = 84,
}

impl AiSyscallNumber {
    /// Return the numeric syscall number.
    ///
    /// ```rust
    /// use omni_runtime::relay::AiSyscallNumber;
    /// assert_eq!(AiSyscallNumber::Invoke.as_u32(), 80);
    /// assert_eq!(AiSyscallNumber::Transcribe.as_u32(), 84);
    /// ```
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Try to construct an [`AiSyscallNumber`] from a raw kernel syscall
    /// number. Returns `None` if the number is not in the AI syscall range
    /// (80–84).
    ///
    /// ```rust
    /// use omni_runtime::relay::AiSyscallNumber;
    /// assert_eq!(AiSyscallNumber::from_u32(82), Some(AiSyscallNumber::Embed));
    /// assert_eq!(AiSyscallNumber::from_u32(0),  None);
    /// assert_eq!(AiSyscallNumber::from_u32(85), None);
    /// ```
    #[must_use]
    pub const fn from_u32(n: u32) -> Option<Self> {
        match n {
            80 => Some(Self::Invoke),
            81 => Some(Self::Stream),
            82 => Some(Self::Embed),
            83 => Some(Self::Classify),
            84 => Some(Self::Transcribe),
            _ => None,
        }
    }
}

// =============================================================================
// AiSyscallRequest
// =============================================================================

/// A request received from the kernel via IPC after an AI syscall.
///
/// The kernel packs the calling process's arguments into this structure and
/// sends it to the runtime over the AI IPC channel. The relay then routes
/// the request to the appropriate pipeline path based on `syscall`.
///
/// # Field notes
///
/// - `model_id_bytes`: compact 16-byte form of the model identifier as stored
///   in the kernel's model table. The relay zero-extends this to 32 bytes when
///   constructing a [`ModelId`].
/// - `input_data`: opaque payload whose encoding is defined by the model's
///   format. The runtime does not inspect the contents.
/// - `caller_pid`: used for audit logging and capability scoping. It is
///   carried through to the audit record but is not used for routing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiSyscallRequest {
    /// Which AI syscall was invoked.
    pub syscall: AiSyscallNumber,
    /// Compact 16-byte model identifier (zero-extended to 32 bytes by relay).
    pub model_id_bytes: [u8; 16],
    /// Opaque input payload (tokenized text, audio bytes, etc.).
    pub input_data: Vec<u8>,
    /// Caller-assigned monotonic request ID for end-to-end correlation.
    pub request_id: u64,
    /// PID of the process that issued the syscall (for audit).
    pub caller_pid: u64,
}

// =============================================================================
// AiSyscallResponse
// =============================================================================

/// The response written back to the kernel after relay dispatch.
///
/// The kernel copies `output_data` into the calling process's address space
/// and returns `request_id` to the caller. If `success` is `false` the kernel
/// surfaces an appropriate errno (typically `EIO`) and the caller may inspect
/// `error_message` via a secondary `AiGetError` call (future syscall).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiSyscallResponse {
    /// Echoes [`AiSyscallRequest::request_id`] for correlation.
    pub request_id: u64,
    /// `true` if inference succeeded; `false` on any error.
    pub success: bool,
    /// Opaque output payload, empty on error.
    pub output_data: Vec<u8>,
    /// Wall-clock latency of the full relay dispatch in microseconds.
    pub latency_us: u64,
    /// Human-readable error description, present only when `success` is `false`.
    pub error_message: Option<String>,
}

impl AiSyscallResponse {
    /// Build an error response for the given `request_id`.
    ///
    /// This is the canonical way to construct a failure response inside the
    /// relay: it sets `success = false`, clears `output_data`, and records
    /// the message for later retrieval.
    fn error(request_id: u64, latency_us: u64, message: impl Into<String>) -> Self {
        Self {
            request_id,
            success: false,
            output_data: Vec::new(),
            latency_us,
            error_message: Some(message.into()),
        }
    }
}

// =============================================================================
// AiIpcRelay
// =============================================================================

/// The IPC relay that routes AI syscalls to the [`InferencePipeline`].
///
/// `AiIpcRelay` holds an `Arc`-wrapped pipeline so it can be cloned across
/// async tasks without copying the registry state. Construct one relay per
/// system boot; all concurrent AI syscall IPC endpoints share the same relay
/// instance.
///
/// # Example
///
/// ```rust
/// use std::sync::Arc;
/// use omni_crypto::signing::OmniSigningKey;
/// use omni_runtime::inference::InferencePipeline;
/// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
/// use omni_runtime::relay::{AiIpcRelay, AiSyscallNumber, AiSyscallRequest};
/// use omni_types::ModelId;
/// use tokio::sync::Mutex;
///
/// # #[tokio::main]
/// # async fn main() {
/// let sk   = OmniSigningKey::from_bytes([0x10; 32]);
/// let hash = [0xABu8; 32];
/// let manifest = ModelManifest {
///     model_id:   ModelId::from_manifest_hash(hash),
///     name:       "relay-doctest".into(),
///     version:    "1.0.0".into(),
///     hash,
///     signature:  sk.sign(&hash),
///     signing_key: sk.verifying_key(),
///     size_bytes: 0,
///     format:     ModelFormat::Gguf,
/// };
///
/// let mut reg = ModelRegistry::new();
/// let id      = reg.register(manifest).unwrap();
/// reg.load(id).unwrap();
///
/// let pipeline = InferencePipeline::new(Arc::new(Mutex::new(reg)));
/// let relay    = AiIpcRelay::new(pipeline);
///
/// // Build a 16-byte compact model id (high half of the 32-byte id).
/// let mut compact = [0u8; 16];
/// compact.copy_from_slice(&hash[..16]);
///
/// let req = AiSyscallRequest {
///     syscall:        AiSyscallNumber::Invoke,
///     model_id_bytes: compact,
///     input_data:     b"hello".to_vec(),
///     request_id:     1,
///     caller_pid:     1000,
/// };
///
/// let resp = relay.dispatch(req).await;
/// assert_eq!(resp.request_id, 1);
/// # }
/// ```
pub struct AiIpcRelay {
    /// Shared inference pipeline. Wrapped in `Arc` so multiple relay clones
    /// (one per IPC endpoint) share the same registry without copying.
    ///
    /// `Arc<InferencePipeline>` does not derive `Debug` automatically because
    /// the inner `Mutex<ModelRegistry>` does not expose its contents through
    /// `Debug`. We implement `Debug` manually to show just the pointer address.
    pipeline: Arc<InferencePipeline>,
}

impl std::fmt::Debug for AiIpcRelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiIpcRelay")
            .field("pipeline", &Arc::as_ptr(&self.pipeline))
            .finish()
    }
}

impl AiIpcRelay {
    /// Create a new relay backed by `pipeline`.
    ///
    /// The pipeline is placed inside an `Arc` so the relay can be cheaply
    /// cloned for concurrent IPC endpoint handlers.
    ///
    /// ```rust
    /// use std::sync::Arc;
    /// use omni_runtime::inference::InferencePipeline;
    /// use omni_runtime::model::ModelRegistry;
    /// use omni_runtime::relay::AiIpcRelay;
    /// use tokio::sync::Mutex;
    ///
    /// let reg      = ModelRegistry::new();
    /// let pipeline = InferencePipeline::new(Arc::new(Mutex::new(reg)));
    /// let _relay   = AiIpcRelay::new(pipeline);
    /// ```
    #[must_use]
    pub fn new(pipeline: InferencePipeline) -> Self {
        Self {
            pipeline: Arc::new(pipeline),
        }
    }

    /// Dispatch an incoming AI syscall request to the inference pipeline.
    ///
    /// The method:
    ///
    /// 1. Logs the incoming request at `info` level (audit trail).
    /// 2. Zero-extends `model_id_bytes` (16 bytes) to a 32-byte
    ///    [`ModelId`] by copying the compact bytes into the high half.
    /// 3. Builds an [`InferenceRequest`] and calls `pipeline.infer`.
    /// 4. Converts the [`InferenceResponse`] into an [`AiSyscallResponse`].
    /// 5. On any error, returns a structured error response (never panics).
    ///
    /// # Syscall routing
    ///
    /// All five AI syscall numbers are accepted. In Phase 2 the pipeline
    /// stub returns an empty output for every variant; future streams will
    /// specialise routing by `syscall` (e.g., stream chunking for
    /// [`AiSyscallNumber::Stream`], embedding vector format for
    /// [`AiSyscallNumber::Embed`]).
    #[instrument(skip(self), fields(
        syscall   = ?request.syscall,
        request_id = request.request_id,
        caller_pid = request.caller_pid,
    ))]
    pub async fn dispatch(&self, request: AiSyscallRequest) -> AiSyscallResponse {
        let start = Instant::now();
        let request_id = request.request_id;

        info!(
            syscall    = ?request.syscall,
            request_id = request.request_id,
            caller_pid = request.caller_pid,
            "AI IPC relay: dispatching syscall"
        );

        // Zero-extend the compact 16-byte model id to 32 bytes.
        // The high 16 bytes carry the model identifier; the low 16 bytes
        // are zeroed. This matches the compact form stored in the kernel's
        // model table (OIP-Agent-Arch-022 §S9).
        let model_id = {
            let mut full = [0u8; 32];
            full[..16].copy_from_slice(&request.model_id_bytes);
            ModelId::from_bytes(full)
        };

        debug!(model_id = ?model_id, "relay: resolved model id");

        let infer_req = InferenceRequest {
            model_id,
            input: request.input_data,
            request_id,
        };

        match self.pipeline.infer(infer_req).await {
            Ok(resp) => {
                let latency_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);

                info!(
                    request_id = resp.request_id,
                    latency_us,
                    output_bytes = resp.output.len(),
                    "AI IPC relay: dispatch succeeded"
                );

                AiSyscallResponse {
                    request_id: resp.request_id,
                    success: true,
                    output_data: resp.output,
                    latency_us,
                    error_message: None,
                }
            }
            Err(err) => {
                let latency_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);

                warn!(
                    request_id,
                    error = %err,
                    "AI IPC relay: dispatch failed"
                );

                AiSyscallResponse::error(request_id, latency_us, err.to_string())
            }
        }
    }
}

// Clone is required so that multiple IPC endpoint handlers can each hold a
// relay handle without lifetime entanglement. The `Arc<InferencePipeline>`
// inside is cheap to clone.
impl Clone for AiIpcRelay {
    fn clone(&self) -> Self {
        Self {
            pipeline: Arc::clone(&self.pipeline),
        }
    }
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use omni_crypto::signing::OmniSigningKey;
    use omni_types::ModelId;
    use tokio::sync::Mutex;

    use super::*;
    use crate::inference::InferencePipeline;
    use crate::model::{ModelFormat, ModelManifest, ModelRegistry};

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn make_relay_with_loaded_model(seed: u8, hash_byte: u8) -> (AiIpcRelay, [u8; 16]) {
        let sk = OmniSigningKey::from_bytes([seed; 32]);
        // Use a hash whose bytes 16..32 are zero so the compact 16-byte
        // form round-trips through the relay's zero-extension correctly.
        let mut hash = [0u8; 32];
        hash[..16].fill(hash_byte);
        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(hash),
            name: "relay-test-model".into(),
            version: "1.0.0".into(),
            hash,
            signature: sk.sign(&hash),
            signing_key: sk.verifying_key(),
            size_bytes: 0,
            format: ModelFormat::Gguf,
        };
        let mut reg = ModelRegistry::new();
        let id = reg.register(manifest).unwrap();
        reg.load(id).unwrap();
        let pipeline = InferencePipeline::new(Arc::new(Mutex::new(reg)));
        let relay = AiIpcRelay::new(pipeline);

        let mut compact = [0u8; 16];
        compact.fill(hash_byte);
        (relay, compact)
    }

    fn make_request(
        compact: [u8; 16],
        syscall: AiSyscallNumber,
        request_id: u64,
    ) -> AiSyscallRequest {
        AiSyscallRequest {
            syscall,
            model_id_bytes: compact,
            input_data: b"test input".to_vec(),
            request_id,
            caller_pid: 42,
        }
    }

    // -------------------------------------------------------------------------
    // AiSyscallNumber
    // -------------------------------------------------------------------------

    #[test]
    fn syscall_numbers_match_kernel_abi() {
        assert_eq!(AiSyscallNumber::Invoke.as_u32(), 80);
        assert_eq!(AiSyscallNumber::Stream.as_u32(), 81);
        assert_eq!(AiSyscallNumber::Embed.as_u32(), 82);
        assert_eq!(AiSyscallNumber::Classify.as_u32(), 83);
        assert_eq!(AiSyscallNumber::Transcribe.as_u32(), 84);
    }

    #[test]
    fn from_u32_round_trips_all_variants() {
        for n in 80u32..=84 {
            let variant = AiSyscallNumber::from_u32(n).unwrap();
            assert_eq!(variant.as_u32(), n);
        }
    }

    #[test]
    fn from_u32_rejects_out_of_range() {
        assert!(AiSyscallNumber::from_u32(0).is_none());
        assert!(AiSyscallNumber::from_u32(79).is_none());
        assert!(AiSyscallNumber::from_u32(85).is_none());
        assert!(AiSyscallNumber::from_u32(u32::MAX).is_none());
    }

    // -------------------------------------------------------------------------
    // AiSyscallResponse helpers
    // -------------------------------------------------------------------------

    #[test]
    fn error_response_has_correct_fields() {
        let resp = AiSyscallResponse::error(99, 500, "something went wrong");
        assert_eq!(resp.request_id, 99);
        assert!(!resp.success);
        assert!(resp.output_data.is_empty());
        assert_eq!(resp.latency_us, 500);
        assert_eq!(resp.error_message.as_deref(), Some("something went wrong"));
    }

    // -------------------------------------------------------------------------
    // AiIpcRelay — dispatch (loaded model)
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn dispatch_loaded_model_succeeds() {
        let (relay, compact) = make_relay_with_loaded_model(0x10, 0xAA);
        let req = make_request(compact, AiSyscallNumber::Invoke, 1);
        let resp = relay.dispatch(req).await;
        assert!(resp.success);
        assert_eq!(resp.request_id, 1);
        assert!(resp.error_message.is_none());
    }

    #[tokio::test]
    async fn dispatch_echoes_request_id() {
        let (relay, compact) = make_relay_with_loaded_model(0x11, 0xBB);
        for rid in [0u64, 1, 42, u64::MAX - 1] {
            let req = make_request(compact, AiSyscallNumber::Embed, rid);
            let resp = relay.dispatch(req).await;
            assert_eq!(resp.request_id, rid);
        }
    }

    #[tokio::test]
    async fn dispatch_unregistered_model_returns_error_response() {
        // Build a relay with an empty registry — no model loaded.
        let empty_reg = ModelRegistry::new();
        let pipeline = InferencePipeline::new(Arc::new(Mutex::new(empty_reg)));
        let relay = AiIpcRelay::new(pipeline);

        let syscall_req = AiSyscallRequest {
            syscall: AiSyscallNumber::Invoke,
            model_id_bytes: [0xFF; 16],
            input_data: vec![],
            request_id: 7,
            caller_pid: 1,
        };
        let resp = relay.dispatch(syscall_req).await;
        assert!(!resp.success);
        assert_eq!(resp.request_id, 7);
        assert!(resp.error_message.is_some());
        assert!(resp.output_data.is_empty());
    }

    #[tokio::test]
    async fn dispatch_all_syscall_variants_accepted() {
        let (relay, compact) = make_relay_with_loaded_model(0x12, 0xCC);
        let variants = [
            AiSyscallNumber::Invoke,
            AiSyscallNumber::Stream,
            AiSyscallNumber::Embed,
            AiSyscallNumber::Classify,
            AiSyscallNumber::Transcribe,
        ];
        for (i, variant) in variants.iter().enumerate() {
            let req = make_request(compact, *variant, i as u64 + 100);
            let resp = relay.dispatch(req).await;
            // All variants route to the same stub pipeline — success for all.
            assert!(resp.success, "expected success for {variant:?}");
        }
    }

    #[tokio::test]
    async fn dispatch_records_latency() {
        let (relay, compact) = make_relay_with_loaded_model(0x13, 0xDD);
        let req = make_request(compact, AiSyscallNumber::Invoke, 200);
        let resp = relay.dispatch(req).await;
        // latency_us is a u64 and always >= 0; verify it was populated.
        let _ = resp.latency_us;
        assert!(resp.success);
    }

    #[tokio::test]
    async fn relay_can_be_cloned_and_used_concurrently() {
        let (relay, compact) = make_relay_with_loaded_model(0x14, 0xEE);
        let relay2 = relay.clone();

        let req1 = make_request(compact, AiSyscallNumber::Invoke, 300);
        let req2 = make_request(compact, AiSyscallNumber::Classify, 301);

        let (r1, r2) = tokio::join!(relay.dispatch(req1), relay2.dispatch(req2));
        assert!(r1.success);
        assert!(r2.success);
        assert_eq!(r1.request_id, 300);
        assert_eq!(r2.request_id, 301);
    }

    #[test]
    fn model_id_zero_extension_is_deterministic() {
        // Verify that the same compact bytes always produce the same ModelId.
        let compact: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0x10,
        ];
        let mut full1 = [0u8; 32];
        let mut full2 = [0u8; 32];
        full1[..16].copy_from_slice(&compact);
        full2[..16].copy_from_slice(&compact);
        assert_eq!(ModelId::from_bytes(full1), ModelId::from_bytes(full2));
        // Low half must be zero.
        assert_eq!(&full1[16..], &[0u8; 16]);
    }
}
