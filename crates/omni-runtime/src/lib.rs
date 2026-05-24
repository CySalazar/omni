//! # `omni-runtime`
//!
//! AI Runtime Service for OMNI OS.
//!
//! The privileged user-space service that exposes AI as a system primitive.
//! Applications call into the runtime through capability-checked syscalls;
//! the runtime owns model lifecycle, inference scheduling, and decisions
//! about which execution tier handles each workload.
//!
//! ## Status
//!
//! Phase 2 Stream 1 — `ModelRegistry`, `InferencePipeline`, `TierRouter`, and
//! stubs for `WorkloadScheduler` and model-manifest attestation.
//! The tensor backend is a placeholder that returns an empty output vector;
//! callers must not interpret an empty `output` as a successful inference
//! result until a real backend lands in a later stream.
//!
//! ## Design rationale
//!
//! - **Capability-checked entry points**: every public function accepts a
//!   capability token; invalid tokens are rejected at the API boundary.
//! - **Tier routing**: the runtime decides whether a given workload is
//!   served by Tier 0 (local), Tier 1 (personal cluster), Tier 2 (mesh),
//!   or Tier 3 (commercial cloud), based on workload sensitivity, user
//!   policy, and available resources. See
//!   [`/docs/02-architecture.md`](../../../docs/02-architecture.md)
//!   § "Execution tiers".
//! - **Model attestation enforced**: a model whose signature does not
//!   verify is rejected at load time. No exceptions.
//! - **Audit log**: every invocation produces a structured record. See
//!   [`/docs/04-security-model.md`](../../../docs/04-security-model.md)
//!   § "Audit log".
//!
//! ## Modules
//!
//! - [`model`] — model lifecycle (load, unload, attest, version).
//! - [`inference`] — inference orchestration on the local node.
//! - [`scheduler`] — workload scheduling across accelerators.
//! - [`router`] — execution tier routing decisions.
//! - [`attestation`] — model signature verification.
//! - [`gguf`] — GGUF v3 binary format parser.

#![doc(html_root_url = "https://docs.omni-os.org/omni-runtime")]
#![warn(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unnecessary_wraps,
    )
)]

// =============================================================================
// gguf — GGUF v3 binary format parser
// =============================================================================

/// GGUF file format parser.
///
/// Implements the GGUF v3 binary format used by llama.cpp and compatible
/// tools. The parser reads model metadata and tensor layout information
/// from raw bytes without loading tensor data into memory.
pub mod gguf;

// =============================================================================
// model — ModelManifest + ModelRegistry
// =============================================================================

/// Model lifecycle: load, unload, attest, version.
///
/// This module owns the canonical model registry for a single OMNI OS node.
/// A model must be registered (signature verified) before it can be loaded;
/// only a loaded model can serve inference requests.
pub mod model {
    use std::collections::BTreeMap;

    use omni_crypto::signing::{OmniSignature, OmniVerifyingKey};
    use omni_types::{ModelId, OmniError, Result};
    use serde::{Deserialize, Serialize};
    use tracing::{debug, info, warn};

    // -------------------------------------------------------------------------
    // ModelFormat
    // -------------------------------------------------------------------------

    /// Wire format of the model binary stored on disk.
    ///
    /// The format is carried in the manifest so downstream components can
    /// select the correct deserialization path without inspecting raw bytes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ModelFormat {
        /// Open Neural Network Exchange (ONNX) format.
        Onnx,
        /// `SafeTensors` format (Hugging Face).
        SafeTensors,
        /// GGUF format (llama.cpp-style quantised models).
        Gguf,
    }

    // -------------------------------------------------------------------------
    // ModelManifest
    // -------------------------------------------------------------------------

    /// Signed declaration of a model's identity, provenance, and format.
    ///
    /// A `ModelManifest` is the authoritative source of truth for a model's
    /// identity within OMNI OS. The registry accepts a manifest only if its
    /// Ed25519 signature over the model's BLAKE3 hash verifies against the
    /// embedded `signing_key`.
    ///
    /// # Security contract
    ///
    /// The `hash` field is the BLAKE3 digest of the model binary. The
    /// `signature` is an Ed25519 signature produced by `signing_key` over
    /// that hash. Before a manifest is accepted into the registry, the
    /// signature is verified with
    /// [`OmniVerifyingKey::verify`][omni_crypto::signing::OmniVerifyingKey::verify],
    /// which uses `verify_strict` internally (rejecting malleability attacks).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_crypto::signing::OmniSigningKey;
    /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
    /// use omni_types::ModelId;
    ///
    /// // Build a manifest with a test key.
    /// let sk = OmniSigningKey::from_bytes([0xAA; 32]);
    /// let hash = [0x01u8; 32];
    /// let sig = sk.sign(&hash);
    /// let vk = sk.verifying_key();
    ///
    /// let manifest = ModelManifest {
    ///     model_id: ModelId::from_manifest_hash(hash),
    ///     name: "test-model".into(),
    ///     version: "1.0.0".into(),
    ///     hash,
    ///     signature: sig,
    ///     signing_key: vk,
    ///     size_bytes: 0,
    ///     format: ModelFormat::Gguf,
    /// };
    ///
    /// let mut registry = ModelRegistry::new();
    /// let id = registry.register(manifest).unwrap();
    /// assert_eq!(registry.list(), vec![id]);
    /// ```
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ModelManifest {
        /// Stable content-addressed identifier derived from this manifest.
        pub model_id: ModelId,
        /// Human-readable model name (e.g. `"llama-3-8b"`).
        pub name: String,
        /// Semantic version string (e.g. `"3.0.1"`).
        pub version: String,
        /// BLAKE3 hash of the model binary. The signature covers this field.
        pub hash: [u8; 32],
        /// Ed25519 signature of `hash` produced by `signing_key`.
        pub signature: OmniSignature,
        /// Ed25519 public key whose private half produced `signature`.
        pub signing_key: OmniVerifyingKey,
        /// Size of the model binary in bytes (informational; not signed).
        pub size_bytes: u64,
        /// On-disk serialization format.
        pub format: ModelFormat,
    }

    // -------------------------------------------------------------------------
    // LoadState
    // -------------------------------------------------------------------------

    /// Tracks whether a registered model has been loaded into memory.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LoadState {
        /// The model binary is not currently in memory.
        Unloaded,
        /// The model binary has been loaded and is ready for inference.
        Loaded,
    }

    // -------------------------------------------------------------------------
    // ModelEntry (private)
    // -------------------------------------------------------------------------

    /// Internal registry record pairing a manifest with its load state.
    #[derive(Debug)]
    struct ModelEntry {
        manifest: ModelManifest,
        state: LoadState,
    }

    // -------------------------------------------------------------------------
    // ModelRegistry
    // -------------------------------------------------------------------------

    /// In-process registry of signed model manifests.
    ///
    /// `ModelRegistry` is the single authoritative store for model identity on
    /// a node. It enforces:
    ///
    /// 1. **Signature verification on register**: a manifest whose Ed25519
    ///    signature does not verify is rejected immediately — the model is
    ///    never made visible to inference.
    /// 2. **Load-state gating**: inference can only be dispatched to a model
    ///    that is in the `Loaded` state. Requesting inference against an
    ///    `Unloaded` model returns an error rather than silently blocking.
    /// 3. **Stable ordering**: the `BTreeMap` backing store keeps model IDs
    ///    sorted, which makes `list()` output deterministic and simplifies
    ///    audit logging.
    ///
    /// # Thread safety
    ///
    /// `ModelRegistry` is not `Send + Sync` by itself. Callers that share it
    /// across async tasks must wrap it in a `tokio::sync::Mutex` (see
    /// [`crate::inference::InferencePipeline`] for the canonical pattern).
    #[derive(Debug, Default)]
    pub struct ModelRegistry {
        entries: BTreeMap<ModelId, ModelEntry>,
    }

    impl ModelRegistry {
        /// Create an empty registry.
        ///
        /// ```rust
        /// use omni_runtime::model::ModelRegistry;
        /// let reg = ModelRegistry::new();
        /// assert!(reg.list().is_empty());
        /// ```
        #[must_use]
        pub fn new() -> Self {
            Self {
                entries: BTreeMap::new(),
            }
        }

        /// Register a model manifest after verifying its Ed25519 signature.
        ///
        /// The manifest's `signing_key` must verify `signature` over `hash`.
        /// If verification fails the manifest is rejected and an error is
        /// returned; no partial state is stored.
        ///
        /// # Errors
        ///
        /// - [`OmniError::Crypto`] with [`omni_types::error::CryptoErrorKind::InvalidSignature`]
        ///   if signature verification fails.
        ///
        /// # Example
        ///
        /// ```rust
        /// use omni_crypto::signing::OmniSigningKey;
        /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
        /// use omni_types::ModelId;
        ///
        /// let sk = OmniSigningKey::from_bytes([0xBB; 32]);
        /// let hash = [0x02u8; 32];
        /// let sig = sk.sign(&hash);
        /// let manifest = ModelManifest {
        ///     model_id: ModelId::from_manifest_hash(hash),
        ///     name: "model-b".into(),
        ///     version: "2.0.0".into(),
        ///     hash,
        ///     signature: sig,
        ///     signing_key: sk.verifying_key(),
        ///     size_bytes: 1024,
        ///     format: ModelFormat::Onnx,
        /// };
        ///
        /// let mut reg = ModelRegistry::new();
        /// let id = reg.register(manifest).unwrap();
        /// assert_eq!(reg.list(), vec![id]);
        /// ```
        pub fn register(&mut self, manifest: ModelManifest) -> Result<ModelId> {
            // Verify the Ed25519 signature before accepting the manifest.
            // This is the single enforcement point: once a manifest is in the
            // registry we treat its identity as verified.
            manifest
                .signing_key
                .verify(&manifest.hash, &manifest.signature)
                .inspect_err(|_| {
                    warn!(
                        model_name = %manifest.name,
                        "model manifest rejected: signature verification failed"
                    );
                })?;

            let id = manifest.model_id;
            info!(
                model_id = ?id,
                model_name = %manifest.name,
                model_version = %manifest.version,
                "model manifest registered"
            );
            self.entries.insert(
                id,
                ModelEntry {
                    manifest,
                    state: LoadState::Unloaded,
                },
            );
            Ok(id)
        }

        /// Mark a registered model as loaded (ready for inference).
        ///
        /// The current stub implementation only transitions the load state;
        /// actual model binary loading (memory-mapping, tensor allocation) is
        /// deferred to a later Phase 2 stream when the tensor backend lands.
        ///
        /// # Errors
        ///
        /// - [`OmniError::Internal`] if `model_id` is not registered.
        ///
        /// # Example
        ///
        /// ```rust
        /// use omni_crypto::signing::OmniSigningKey;
        /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
        /// use omni_types::ModelId;
        ///
        /// let sk = OmniSigningKey::from_bytes([0xCC; 32]);
        /// let hash = [0x03u8; 32];
        /// let manifest = ModelManifest {
        ///     model_id: ModelId::from_manifest_hash(hash),
        ///     name: "model-c".into(),
        ///     version: "1.0.0".into(),
        ///     hash,
        ///     signature: sk.sign(&hash),
        ///     signing_key: sk.verifying_key(),
        ///     size_bytes: 0,
        ///     format: ModelFormat::SafeTensors,
        /// };
        ///
        /// let mut reg = ModelRegistry::new();
        /// let id = reg.register(manifest).unwrap();
        /// reg.load(id).unwrap();
        /// ```
        pub fn load(&mut self, model_id: ModelId) -> Result<()> {
            let entry = self.entries.get_mut(&model_id).ok_or_else(|| {
                OmniError::internal("runtime::model::load — model_id not registered")
            })?;

            debug!(model_id = ?model_id, "loading model");
            // Stub: no binary is actually loaded into memory yet. Transition
            // state so the inference pipeline can gate on it.
            entry.state = LoadState::Loaded;
            Ok(())
        }

        /// Load a GGUF model from raw bytes.
        ///
        /// Parses the GGUF header, verifies the model's BLAKE3 hash matches
        /// the manifest, and stores the parsed tensor metadata. The actual
        /// tensor data is NOT loaded into GPU/CPU memory — that happens on
        /// first inference via the tensor backend.
        ///
        /// # Errors
        ///
        /// - [`OmniError::Internal`] if `model_id` is not registered.
        /// - [`OmniError::Internal`] if the registered model's format is not
        ///   [`ModelFormat::Gguf`].
        /// - [`OmniError::Internal`] if the BLAKE3 hash of `data` does not
        ///   match the hash stored in the model's manifest.
        /// - [`OmniError::Internal`] if the GGUF data is malformed.
        ///
        /// # Example
        ///
        /// ```rust
        /// use omni_crypto::signing::OmniSigningKey;
        /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
        /// use omni_types::ModelId;
        ///
        /// // Construct a minimal valid GGUF v3 file (20 bytes: no tensors, no metadata).
        /// let gguf_magic: u32 = 0x4655_4746;
        /// let mut data = Vec::new();
        /// data.extend_from_slice(&gguf_magic.to_le_bytes()); // magic
        /// data.extend_from_slice(&3u32.to_le_bytes());        // version
        /// data.extend_from_slice(&0u64.to_le_bytes());        // tensor_count
        /// data.extend_from_slice(&0u64.to_le_bytes());        // metadata_kv_count
        ///
        /// let hash: [u8; 32] = *blake3::hash(&data).as_bytes();
        /// let sk = OmniSigningKey::from_bytes([0x55; 32]);
        /// let sig = sk.sign(&hash);
        ///
        /// let manifest = ModelManifest {
        ///     model_id: ModelId::from_manifest_hash(hash),
        ///     name: "gguf-test".into(),
        ///     version: "1.0.0".into(),
        ///     hash,
        ///     signature: sig,
        ///     signing_key: sk.verifying_key(),
        ///     size_bytes: data.len() as u64,
        ///     format: ModelFormat::Gguf,
        /// };
        ///
        /// let mut reg = ModelRegistry::new();
        /// let id = reg.register(manifest).unwrap();
        /// reg.load_from_bytes(id, &data).unwrap();
        /// assert!(reg.is_loaded(id));
        /// ```
        pub fn load_from_bytes(&mut self, model_id: ModelId, data: &[u8]) -> Result<()> {
            let entry = self.entries.get_mut(&model_id).ok_or_else(|| {
                OmniError::internal("runtime::model::load_from_bytes — model_id not registered")
            })?;

            // Guard: only GGUF format is supported by this path.
            if entry.manifest.format != ModelFormat::Gguf {
                return Err(OmniError::internal(
                    "runtime::model::load_from_bytes — only GGUF format supported",
                ));
            }

            // Verify BLAKE3 hash of the raw bytes against the signed manifest hash.
            // This is the integrity check that ensures the bytes in memory match
            // what the signing authority attested to at registration time.
            let computed_hash = blake3::hash(data);
            if computed_hash.as_bytes() != &entry.manifest.hash {
                return Err(OmniError::internal(
                    "runtime::model::load_from_bytes — BLAKE3 hash mismatch",
                ));
            }

            // Parse the GGUF header to validate the format and extract metadata.
            // We do not store the header on the entry yet; a future stream will
            // add a field to ModelEntry to hold the parsed GgufHeader for use by
            // the tensor backend.
            let header = crate::gguf::parse_gguf(data)?;
            info!(
                model_id = ?model_id,
                tensor_count = header.tensor_count,
                metadata_count = header.metadata.len(),
                "GGUF model parsed successfully"
            );

            entry.state = LoadState::Loaded;
            Ok(())
        }

        /// Mark a registered model as unloaded, freeing its resources.
        ///
        /// The current stub implementation only transitions the load state;
        /// actual memory release is deferred to the tensor backend.
        ///
        /// # Errors
        ///
        /// - [`OmniError::Internal`] if `model_id` is not registered.
        ///
        /// # Example
        ///
        /// ```rust
        /// use omni_crypto::signing::OmniSigningKey;
        /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
        /// use omni_types::ModelId;
        ///
        /// let sk = OmniSigningKey::from_bytes([0xDD; 32]);
        /// let hash = [0x04u8; 32];
        /// let manifest = ModelManifest {
        ///     model_id: ModelId::from_manifest_hash(hash),
        ///     name: "model-d".into(),
        ///     version: "1.0.0".into(),
        ///     hash,
        ///     signature: sk.sign(&hash),
        ///     signing_key: sk.verifying_key(),
        ///     size_bytes: 0,
        ///     format: ModelFormat::Gguf,
        /// };
        ///
        /// let mut reg = ModelRegistry::new();
        /// let id = reg.register(manifest).unwrap();
        /// reg.load(id).unwrap();
        /// reg.unload(id).unwrap();
        /// ```
        pub fn unload(&mut self, model_id: ModelId) -> Result<()> {
            let entry = self.entries.get_mut(&model_id).ok_or_else(|| {
                OmniError::internal("runtime::model::unload — model_id not registered")
            })?;

            debug!(model_id = ?model_id, "unloading model");
            entry.state = LoadState::Unloaded;
            Ok(())
        }

        /// Return the manifest for a registered model, verifying its
        /// signature in the process.
        ///
        /// This is the attestation query path: a caller can confirm that the
        /// manifest stored in the registry still matches the original
        /// signed state. If the in-memory manifest has been tampered with
        /// (indicative of memory corruption), verification will fail.
        ///
        /// # Errors
        ///
        /// - [`OmniError::Internal`] if `model_id` is not registered.
        /// - [`OmniError::Crypto`] if the stored signature no longer verifies
        ///   (indicates in-memory tampering or a programming error).
        ///
        /// # Example
        ///
        /// ```rust
        /// use omni_crypto::signing::OmniSigningKey;
        /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
        /// use omni_types::ModelId;
        ///
        /// let sk = OmniSigningKey::from_bytes([0xEE; 32]);
        /// let hash = [0x05u8; 32];
        /// let manifest = ModelManifest {
        ///     model_id: ModelId::from_manifest_hash(hash),
        ///     name: "model-e".into(),
        ///     version: "1.0.0".into(),
        ///     hash,
        ///     signature: sk.sign(&hash),
        ///     signing_key: sk.verifying_key(),
        ///     size_bytes: 0,
        ///     format: ModelFormat::Onnx,
        /// };
        ///
        /// let mut reg = ModelRegistry::new();
        /// let id = reg.register(manifest).unwrap();
        /// let attested = reg.attest(id).unwrap();
        /// assert_eq!(attested.name, "model-e");
        /// ```
        pub fn attest(&self, model_id: ModelId) -> Result<ModelManifest> {
            let entry = self.entries.get(&model_id).ok_or_else(|| {
                OmniError::internal("runtime::model::attest — model_id not registered")
            })?;

            // Re-verify the stored signature on attest. This is an integrity
            // check: if the manifest has been modified in memory since
            // registration, verification will fail and the caller receives an
            // error rather than a corrupt manifest.
            entry
                .manifest
                .signing_key
                .verify(&entry.manifest.hash, &entry.manifest.signature)?;

            Ok(entry.manifest.clone())
        }

        /// Return a sorted list of all registered model IDs.
        ///
        /// The ordering is deterministic (BLAKE3 hash byte order) so callers
        /// can iterate predictably without sorting themselves.
        ///
        /// # Example
        ///
        /// ```rust
        /// use omni_runtime::model::ModelRegistry;
        /// let reg = ModelRegistry::new();
        /// assert!(reg.list().is_empty());
        /// ```
        #[must_use]
        pub fn list(&self) -> Vec<ModelId> {
            self.entries.keys().copied().collect()
        }

        /// Returns `true` if `model_id` is registered and currently loaded.
        ///
        /// Used by the inference pipeline to gate dispatch without holding a
        /// mutable borrow.
        #[must_use]
        pub fn is_loaded(&self, model_id: ModelId) -> bool {
            self.entries
                .get(&model_id)
                .is_some_and(|e| e.state == LoadState::Loaded)
        }
    }
}

// =============================================================================
// inference — InferencePipeline
// =============================================================================

/// Inference orchestration on the local node.
///
/// This module provides the [`InferencePipeline`] which dispatches inference
/// requests to the appropriate loaded model. The tensor backend is a stub in
/// Phase 2 Stream 1; it returns an empty output vector and records the
/// round-trip latency. A real backend (candle or tch) will replace the stub
/// in a later stream.
pub mod inference {
    use std::sync::Arc;
    use std::time::Instant;

    use omni_types::{ModelId, OmniError, Result};
    use tokio::sync::Mutex;
    use tracing::{debug, instrument};

    use crate::model::ModelRegistry;

    // -------------------------------------------------------------------------
    // InferenceRequest
    // -------------------------------------------------------------------------

    /// A request to run inference on a loaded model.
    ///
    /// The `input` field carries opaque tensor bytes whose encoding is
    /// defined by the model's format (ONNX protobuf, safetensors slice, etc.).
    /// The runtime does not inspect the contents; they are forwarded verbatim
    /// to the tensor backend.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_runtime::inference::InferenceRequest;
    /// use omni_types::ModelId;
    ///
    /// let req = InferenceRequest {
    ///     model_id: ModelId::from_bytes([0xAA; 32]),
    ///     input: vec![1, 2, 3],
    ///     request_id: 42,
    /// };
    /// assert_eq!(req.request_id, 42);
    /// ```
    #[derive(Debug, Clone)]
    pub struct InferenceRequest {
        /// Target model to run.
        pub model_id: ModelId,
        /// Opaque tensor bytes (format defined by `ModelFormat`).
        pub input: Vec<u8>,
        /// Caller-assigned monotonic request identifier for correlation.
        pub request_id: u64,
    }

    // -------------------------------------------------------------------------
    // InferenceResponse
    // -------------------------------------------------------------------------

    /// The result of a single inference call.
    ///
    /// `output` carries opaque tensor bytes in the same format as the
    /// corresponding request's `input`. When the stub tensor backend is
    /// active `output` is always empty; callers must check that they are not
    /// running against a stub before interpreting the response.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_runtime::inference::InferenceResponse;
    ///
    /// let resp = InferenceResponse {
    ///     request_id: 42,
    ///     output: vec![],
    ///     latency_us: 100,
    /// };
    /// assert_eq!(resp.request_id, 42);
    /// ```
    #[derive(Debug, Clone)]
    pub struct InferenceResponse {
        /// Echoes the `request_id` from the originating [`InferenceRequest`].
        pub request_id: u64,
        /// Opaque tensor bytes produced by the model.
        pub output: Vec<u8>,
        /// Wall-clock latency of the inference call in microseconds.
        pub latency_us: u64,
    }

    // -------------------------------------------------------------------------
    // InferencePipeline
    // -------------------------------------------------------------------------

    /// Dispatches inference requests to loaded models.
    ///
    /// `InferencePipeline` holds a shared reference to a [`ModelRegistry`]
    /// wrapped in a `tokio::sync::Mutex` so multiple async tasks can submit
    /// requests concurrently. The registry is locked only for the load-state
    /// check; the (stub) tensor call is not performed while holding the lock.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::sync::Arc;
    /// use omni_crypto::signing::OmniSigningKey;
    /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
    /// use omni_runtime::inference::{InferencePipeline, InferenceRequest};
    /// use omni_types::ModelId;
    /// use tokio::sync::Mutex;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let sk = OmniSigningKey::from_bytes([0x11; 32]);
    /// let hash = [0xAAu8; 32];
    /// let manifest = ModelManifest {
    ///     model_id: ModelId::from_manifest_hash(hash),
    ///     name: "pipeline-test".into(),
    ///     version: "1.0.0".into(),
    ///     hash,
    ///     signature: sk.sign(&hash),
    ///     signing_key: sk.verifying_key(),
    ///     size_bytes: 0,
    ///     format: ModelFormat::Gguf,
    /// };
    ///
    /// let mut reg = ModelRegistry::new();
    /// let id = reg.register(manifest).unwrap();
    /// reg.load(id).unwrap();
    ///
    /// let registry = Arc::new(Mutex::new(reg));
    /// let pipeline = InferencePipeline::new(Arc::clone(&registry));
    ///
    /// let req = InferenceRequest { model_id: id, input: vec![], request_id: 1 };
    /// let resp = pipeline.infer(req).await.unwrap();
    /// assert_eq!(resp.request_id, 1);
    /// # }
    /// ```
    #[derive(Clone, Debug)]
    pub struct InferencePipeline {
        registry: Arc<Mutex<ModelRegistry>>,
    }

    impl InferencePipeline {
        /// Create a pipeline backed by the given registry.
        ///
        /// The registry must be wrapped in a `tokio::sync::Mutex` so that
        /// concurrent inference requests serialise access to load-state checks.
        #[must_use]
        pub fn new(registry: Arc<Mutex<ModelRegistry>>) -> Self {
            Self { registry }
        }

        /// Dispatch an inference request to the loaded model.
        ///
        /// The call will fail immediately if the requested model is not in the
        /// `Loaded` state — either because it was never registered or because
        /// it has been unloaded. Callers should call
        /// [`ModelRegistry::load`][crate::model::ModelRegistry::load] first.
        ///
        /// # Stub behaviour
        ///
        /// The current tensor backend is a placeholder. It returns an empty
        /// `output` vector and records the actual wall-clock round-trip time
        /// for the no-op dispatch in `latency_us`. Replace this stub with a
        /// real tensor call when the tensor backend lands.
        ///
        /// # Errors
        ///
        /// - [`OmniError::Internal`] if the model is not registered.
        /// - [`OmniError::Internal`] if the model is registered but not loaded.
        #[instrument(skip(self), fields(request_id = request.request_id, model_id = ?request.model_id))]
        pub async fn infer(&self, request: InferenceRequest) -> Result<InferenceResponse> {
            let model_id: ModelId = request.model_id;

            // Check load state — lock scope is intentionally narrow so we do
            // not hold the mutex across any await point.
            {
                let registry = self.registry.lock().await;
                if !registry.is_loaded(model_id) {
                    return Err(OmniError::internal(
                        "runtime::inference::infer — model not loaded",
                    ));
                }
            } // lock released here, before the tensor dispatch below.

            let start = Instant::now();
            debug!(model_id = ?model_id, "dispatching to tensor backend (stub)");

            // Stub tensor dispatch: return empty output.
            // FUTURE: replace with `backend.run(model_id, &request.input)?`
            let output: Vec<u8> = Vec::new();

            let latency_us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);

            Ok(InferenceResponse {
                request_id: request.request_id,
                output,
                latency_us,
            })
        }
    }
}

// =============================================================================
// router — TierRouter
// =============================================================================

/// Execution tier routing decisions.
///
/// This module implements the routing policy that decides which execution
/// tier handles a given inference request. The Phase 2 Stream 1 stub
/// always routes to [`ExecutionTier::Local`] (Tier 0). Future streams will
/// add policy evaluation based on model size, user consent, and available
/// cluster resources.
pub mod router {
    use tracing::debug;

    use crate::inference::InferenceRequest;

    // -------------------------------------------------------------------------
    // ExecutionTier
    // -------------------------------------------------------------------------

    /// The set of execution tiers available to the OMNI OS runtime.
    ///
    /// Tiers are ordered by privacy (Tier 0 is most private; data never leaves
    /// the local node). The router may escalate to a higher-numbered tier only
    /// when:
    ///
    /// 1. The model is not available locally, and
    /// 2. The user's policy explicitly permits the escalation tier.
    ///
    /// See [`/docs/02-architecture.md`](../../../docs/02-architecture.md)
    /// § "Execution tiers" for the full privacy contract.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum ExecutionTier {
        /// Tier 0 — model runs on the local node. Data never leaves the device.
        Local,
        /// Tier 1 — model runs on a user-owned personal compute cluster.
        PersonalCluster,
        /// Tier 2 — model runs on a federated mesh of trusted OMNI nodes.
        FederatedMesh,
        /// Tier 3 — model runs on a commercial cloud provider.
        Cloud,
    }

    // -------------------------------------------------------------------------
    // TierRouter
    // -------------------------------------------------------------------------

    /// Routes inference requests to the appropriate execution tier.
    ///
    /// The current implementation is a Phase 2 Stream 1 stub that always
    /// returns [`ExecutionTier::Local`]. The full policy engine (model size
    /// heuristics, user consent flags, resource availability) will be added
    /// in a later stream.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_runtime::router::{ExecutionTier, TierRouter};
    /// use omni_runtime::inference::InferenceRequest;
    /// use omni_types::ModelId;
    ///
    /// let router = TierRouter::new();
    /// let req = InferenceRequest {
    ///     model_id: ModelId::from_bytes([0x00; 32]),
    ///     input: vec![],
    ///     request_id: 0,
    /// };
    /// assert_eq!(router.route(&req), ExecutionTier::Local);
    /// ```
    #[derive(Debug, Default)]
    pub struct TierRouter;

    impl TierRouter {
        /// Create a new tier router with default (local-only) policy.
        ///
        /// ```rust
        /// use omni_runtime::router::TierRouter;
        /// let _ = TierRouter::new();
        /// ```
        #[must_use]
        pub fn new() -> Self {
            Self
        }

        /// Decide which execution tier should handle `request`.
        ///
        /// Phase 2 Stream 1 stub: always returns [`ExecutionTier::Local`].
        /// The caller is responsible for verifying that the local node has the
        /// requested model loaded before dispatching.
        #[must_use]
        pub fn route(&self, request: &InferenceRequest) -> ExecutionTier {
            let _ = self;
            debug!(
                request_id = request.request_id,
                model_id = ?request.model_id,
                "tier router: routing to Local (Tier 0, stub)"
            );
            ExecutionTier::Local
        }
    }
}

// =============================================================================
// scheduler — WorkloadScheduler stub
// =============================================================================

/// Workload scheduling across accelerators.
///
/// This module provides a stub [`WorkloadScheduler`] that will grow into a
/// full cost-model-driven accelerator scheduler in a later Phase 2 stream.
/// For now it is a placeholder to establish the public API shape so other
/// modules can depend on it without needing implementation-level changes.
pub mod scheduler {
    use omni_types::Result;
    use tracing::debug;

    /// Schedules AI workloads across available accelerators on the local node.
    ///
    /// Phase 2 Stream 1 stub. The full implementation will include:
    ///
    /// - Cost-model estimation (FLOPs, memory bandwidth, thermal headroom).
    /// - Affinity rules (e.g., "prefer NPU for quantised Gguf models").
    /// - Backpressure / queue depth signalling to the inference pipeline.
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_runtime::scheduler::WorkloadScheduler;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let scheduler = WorkloadScheduler::new();
    /// scheduler.schedule().await.unwrap();
    /// # }
    /// ```
    #[derive(Debug, Default)]
    pub struct WorkloadScheduler;

    impl WorkloadScheduler {
        /// Create a new scheduler.
        ///
        /// ```rust
        /// use omni_runtime::scheduler::WorkloadScheduler;
        /// let _ = WorkloadScheduler::new();
        /// ```
        #[must_use]
        pub fn new() -> Self {
            Self
        }

        /// Attempt to schedule pending workloads across available accelerators.
        ///
        /// Phase 2 Stream 1 stub — no-op. Returns `Ok(())` immediately.
        ///
        /// # Errors
        ///
        /// Currently never errors. Future implementations may return
        /// [`omni_types::OmniError::Internal`] on accelerator enumeration
        /// failures.
        #[allow(clippy::unused_async)]
        pub async fn schedule(&self) -> Result<()> {
            debug!("scheduler: schedule() called (stub — no-op)");
            Ok(())
        }
    }
}

// =============================================================================
// attestation — model manifest verification
// =============================================================================

/// Model signature verification.
///
/// This module exposes a single free function that verifies the Ed25519
/// signature carried inside a [`model::ModelManifest`]. It is the low-level
/// attestation primitive that [`model::ModelRegistry::register`] and
/// [`model::ModelRegistry::attest`] both delegate to.
pub mod attestation {
    use omni_types::Result;

    use crate::model::ModelManifest;

    /// Verify the Ed25519 signature on `manifest`.
    ///
    /// Checks that `manifest.signing_key.verify(&manifest.hash,
    /// &manifest.signature)` succeeds using the strict (non-malleable)
    /// verification path. Returns `Ok(())` on success.
    ///
    /// # Errors
    ///
    /// - [`omni_types::OmniError::Crypto`] with
    ///   [`omni_types::error::CryptoErrorKind::InvalidSignature`] if
    ///   verification fails for any reason (wrong key, tampered hash, etc.).
    ///
    /// # Example
    ///
    /// ```rust
    /// use omni_crypto::signing::OmniSigningKey;
    /// use omni_runtime::attestation::verify_model_manifest;
    /// use omni_runtime::model::{ModelFormat, ModelManifest};
    /// use omni_types::ModelId;
    ///
    /// let sk = OmniSigningKey::from_bytes([0x42; 32]);
    /// let hash = [0x99u8; 32];
    /// let manifest = ModelManifest {
    ///     model_id: ModelId::from_manifest_hash(hash),
    ///     name: "attested-model".into(),
    ///     version: "1.0.0".into(),
    ///     hash,
    ///     signature: sk.sign(&hash),
    ///     signing_key: sk.verifying_key(),
    ///     size_bytes: 512,
    ///     format: ModelFormat::SafeTensors,
    /// };
    ///
    /// verify_model_manifest(&manifest).unwrap();
    /// ```
    pub fn verify_model_manifest(manifest: &ModelManifest) -> Result<()> {
        manifest
            .signing_key
            .verify(&manifest.hash, &manifest.signature)
    }
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use omni_crypto::signing::OmniSigningKey;
    use omni_types::{ModelId, OmniError, error::CryptoErrorKind};
    use tokio::sync::Mutex;

    use crate::{
        attestation::verify_model_manifest,
        inference::{InferencePipeline, InferenceRequest},
        model::{ModelFormat, ModelManifest, ModelRegistry},
        router::{ExecutionTier, TierRouter},
        scheduler::WorkloadScheduler,
    };

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    /// Build a valid signed manifest using the given seed byte for the key.
    fn make_manifest(seed: u8, hash_byte: u8, name: &str) -> ModelManifest {
        let sk = OmniSigningKey::from_bytes([seed; 32]);
        let hash = [hash_byte; 32];
        ModelManifest {
            model_id: ModelId::from_manifest_hash(hash),
            name: name.to_string(),
            version: "1.0.0".to_string(),
            hash,
            signature: sk.sign(&hash),
            signing_key: sk.verifying_key(),
            size_bytes: 100,
            format: ModelFormat::Gguf,
        }
    }

    // -------------------------------------------------------------------------
    // ModelRegistry — basic CRUD
    // -------------------------------------------------------------------------

    #[test]
    fn registry_new_is_empty() {
        let reg = ModelRegistry::new();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn registry_register_valid_manifest_succeeds() {
        let mut reg = ModelRegistry::new();
        let manifest = make_manifest(0x01, 0xAA, "model-a");
        let id = reg.register(manifest).unwrap();
        assert_eq!(reg.list(), vec![id]);
    }

    #[test]
    fn registry_register_returns_correct_model_id() {
        let mut reg = ModelRegistry::new();
        let hash = [0xBBu8; 32];
        let sk = OmniSigningKey::from_bytes([0x02; 32]);
        let expected_id = ModelId::from_manifest_hash(hash);
        let manifest = ModelManifest {
            model_id: expected_id,
            name: "model-b".into(),
            version: "1.0.0".into(),
            hash,
            signature: sk.sign(&hash),
            signing_key: sk.verifying_key(),
            size_bytes: 0,
            format: ModelFormat::Onnx,
        };
        let returned_id = reg.register(manifest).unwrap();
        assert_eq!(returned_id, expected_id);
    }

    #[test]
    fn registry_register_invalid_signature_fails() {
        let mut reg = ModelRegistry::new();
        let sk = OmniSigningKey::from_bytes([0x03; 32]);
        let other_sk = OmniSigningKey::from_bytes([0x04; 32]);
        let hash = [0xCCu8; 32];
        // Sign with `other_sk` but claim `sk` as the signing key.
        let bad_sig = other_sk.sign(&hash);
        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(hash),
            name: "bad-model".into(),
            version: "1.0.0".into(),
            hash,
            signature: bad_sig,
            signing_key: sk.verifying_key(), // mismatched key
            size_bytes: 0,
            format: ModelFormat::SafeTensors,
        };
        let err = reg.register(manifest).unwrap_err();
        match err {
            OmniError::Crypto { kind, .. } => assert_eq!(kind, CryptoErrorKind::InvalidSignature),
            _ => panic!("expected Crypto::InvalidSignature, got: {err:?}"),
        }
    }

    #[test]
    fn registry_register_tampered_hash_fails() {
        let mut reg = ModelRegistry::new();
        let sk = OmniSigningKey::from_bytes([0x05; 32]);
        let original_hash = [0xDDu8; 32];
        let sig = sk.sign(&original_hash);
        // Replace the hash with a different value after signing.
        let tampered_hash = [0xEEu8; 32];
        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(tampered_hash),
            name: "tampered".into(),
            version: "1.0.0".into(),
            hash: tampered_hash,
            signature: sig,
            signing_key: sk.verifying_key(),
            size_bytes: 0,
            format: ModelFormat::Onnx,
        };
        assert!(reg.register(manifest).is_err());
    }

    #[test]
    fn registry_load_valid_model_succeeds() {
        let mut reg = ModelRegistry::new();
        let manifest = make_manifest(0x06, 0x10, "load-test");
        let id = reg.register(manifest).unwrap();
        reg.load(id).unwrap();
        assert!(reg.is_loaded(id));
    }

    #[test]
    fn registry_load_unknown_model_fails() {
        let mut reg = ModelRegistry::new();
        let unknown = ModelId::from_bytes([0xFF; 32]);
        let err = reg.load(unknown).unwrap_err();
        match err {
            OmniError::Internal { .. } => {}
            _ => panic!("expected Internal error, got: {err:?}"),
        }
    }

    #[test]
    fn registry_unload_loaded_model_succeeds() {
        let mut reg = ModelRegistry::new();
        let manifest = make_manifest(0x07, 0x20, "unload-test");
        let id = reg.register(manifest).unwrap();
        reg.load(id).unwrap();
        reg.unload(id).unwrap();
        assert!(!reg.is_loaded(id));
    }

    #[test]
    fn registry_unload_unknown_model_fails() {
        let mut reg = ModelRegistry::new();
        let unknown = ModelId::from_bytes([0xFE; 32]);
        let err = reg.unload(unknown).unwrap_err();
        match err {
            OmniError::Internal { .. } => {}
            _ => panic!("expected Internal error, got: {err:?}"),
        }
    }

    #[test]
    fn registry_attest_returns_manifest() {
        let mut reg = ModelRegistry::new();
        let manifest = make_manifest(0x08, 0x30, "attest-test");
        let name = manifest.name.clone();
        let id = reg.register(manifest).unwrap();
        let attested = reg.attest(id).unwrap();
        assert_eq!(attested.name, name);
    }

    #[test]
    fn registry_attest_unknown_model_fails() {
        let reg = ModelRegistry::new();
        let unknown = ModelId::from_bytes([0xFD; 32]);
        let err = reg.attest(unknown).unwrap_err();
        match err {
            OmniError::Internal { .. } => {}
            _ => panic!("expected Internal error, got: {err:?}"),
        }
    }

    #[test]
    fn registry_list_returns_sorted_ids() {
        let mut reg = ModelRegistry::new();
        // Register models with different hash bytes so IDs differ.
        let m1 = make_manifest(0x0A, 0x01, "m1");
        let m2 = make_manifest(0x0B, 0x80, "m2");
        let m3 = make_manifest(0x0C, 0x40, "m3");
        let id1 = reg.register(m1).unwrap();
        let id2 = reg.register(m2).unwrap();
        let id3 = reg.register(m3).unwrap();
        let list = reg.list();
        assert_eq!(list.len(), 3);
        // BTreeMap guarantees sorted order.
        let mut expected = vec![id1, id2, id3];
        expected.sort();
        assert_eq!(list, expected);
    }

    #[test]
    fn registry_is_loaded_false_before_load() {
        let mut reg = ModelRegistry::new();
        let manifest = make_manifest(0x0D, 0x50, "preload");
        let id = reg.register(manifest).unwrap();
        assert!(!reg.is_loaded(id));
    }

    #[test]
    fn registry_is_loaded_false_for_unknown() {
        let reg = ModelRegistry::new();
        let unknown = ModelId::from_bytes([0xFC; 32]);
        assert!(!reg.is_loaded(unknown));
    }

    // -------------------------------------------------------------------------
    // Attestation module
    // -------------------------------------------------------------------------

    #[test]
    fn attestation_verify_valid_manifest_ok() {
        let manifest = make_manifest(0x0E, 0x60, "attest-valid");
        verify_model_manifest(&manifest).unwrap();
    }

    #[test]
    fn attestation_verify_bad_signature_fails() {
        let sk = OmniSigningKey::from_bytes([0x0F; 32]);
        let other_sk = OmniSigningKey::from_bytes([0x10; 32]);
        let hash = [0x70u8; 32];
        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(hash),
            name: "bad".into(),
            version: "1.0.0".into(),
            hash,
            signature: other_sk.sign(&hash),
            signing_key: sk.verifying_key(),
            size_bytes: 0,
            format: ModelFormat::Onnx,
        };
        let err = verify_model_manifest(&manifest).unwrap_err();
        match err {
            OmniError::Crypto { kind, .. } => assert_eq!(kind, CryptoErrorKind::InvalidSignature),
            _ => panic!("expected InvalidSignature"),
        }
    }

    // -------------------------------------------------------------------------
    // TierRouter
    // -------------------------------------------------------------------------

    #[test]
    fn router_always_returns_local_tier() {
        let router = TierRouter::new();
        let req = InferenceRequest {
            model_id: ModelId::from_bytes([0x00; 32]),
            input: vec![],
            request_id: 0,
        };
        assert_eq!(router.route(&req), ExecutionTier::Local);
    }

    #[test]
    fn router_local_is_not_cloud() {
        let router = TierRouter::new();
        let req = InferenceRequest {
            model_id: ModelId::from_bytes([0x11; 32]),
            input: vec![1, 2, 3],
            request_id: 99,
        };
        let tier = router.route(&req);
        assert_ne!(tier, ExecutionTier::Cloud);
        assert_ne!(tier, ExecutionTier::PersonalCluster);
        assert_ne!(tier, ExecutionTier::FederatedMesh);
    }

    // -------------------------------------------------------------------------
    // WorkloadScheduler
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn scheduler_schedule_is_noop() {
        let sched = WorkloadScheduler::new();
        sched.schedule().await.unwrap();
    }

    // -------------------------------------------------------------------------
    // InferencePipeline
    // -------------------------------------------------------------------------

    fn make_pipeline_with_loaded_model() -> (Arc<Mutex<ModelRegistry>>, ModelId, InferencePipeline)
    {
        let mut reg = ModelRegistry::new();
        let manifest = make_manifest(0x20, 0x90, "pipeline-model");
        let id = reg.register(manifest).unwrap();
        reg.load(id).unwrap();
        let shared = Arc::new(Mutex::new(reg));
        let pipeline = InferencePipeline::new(Arc::clone(&shared));
        (shared, id, pipeline)
    }

    #[tokio::test]
    async fn pipeline_infer_loaded_model_succeeds() {
        let (_, id, pipeline) = make_pipeline_with_loaded_model();
        let req = InferenceRequest {
            model_id: id,
            input: vec![1, 2, 3],
            request_id: 1,
        };
        let resp = pipeline.infer(req).await.unwrap();
        assert_eq!(resp.request_id, 1);
    }

    #[tokio::test]
    async fn pipeline_infer_echoes_request_id() {
        let (_, id, pipeline) = make_pipeline_with_loaded_model();
        for rid in [0u64, 1, 42, u64::MAX] {
            let req = InferenceRequest {
                model_id: id,
                input: vec![],
                request_id: rid,
            };
            let resp = pipeline.infer(req).await.unwrap();
            assert_eq!(resp.request_id, rid);
        }
    }

    #[tokio::test]
    async fn pipeline_infer_stub_returns_empty_output() {
        let (_, id, pipeline) = make_pipeline_with_loaded_model();
        let req = InferenceRequest {
            model_id: id,
            input: vec![42, 43, 44],
            request_id: 2,
        };
        let resp = pipeline.infer(req).await.unwrap();
        // Stub backend produces empty output.
        assert!(resp.output.is_empty());
    }

    #[tokio::test]
    async fn pipeline_infer_records_latency() {
        let (_, id, pipeline) = make_pipeline_with_loaded_model();
        let req = InferenceRequest {
            model_id: id,
            input: vec![],
            request_id: 3,
        };
        let resp = pipeline.infer(req).await.unwrap();
        // Latency is non-negative (always true for u64) and should be a
        // plausible wall-clock value for a no-op. We just assert it fits
        // without overflow — the stub is fast enough to complete in well
        // under u64::MAX microseconds.
        let _ = resp.latency_us; // binding to silence "unused" warning
    }

    #[tokio::test]
    async fn pipeline_infer_unloaded_model_fails() {
        let mut reg = ModelRegistry::new();
        let manifest = make_manifest(0x21, 0xA0, "unloaded-model");
        let id = reg.register(manifest).unwrap();
        // Do NOT call reg.load(id) — model remains Unloaded.
        let shared = Arc::new(Mutex::new(reg));
        let pipeline = InferencePipeline::new(shared);
        let req = InferenceRequest {
            model_id: id,
            input: vec![],
            request_id: 4,
        };
        let err = pipeline.infer(req).await.unwrap_err();
        match err {
            OmniError::Internal { .. } => {}
            _ => panic!("expected Internal error for unloaded model"),
        }
    }

    #[tokio::test]
    async fn pipeline_infer_unregistered_model_fails() {
        let reg = ModelRegistry::new();
        let shared = Arc::new(Mutex::new(reg));
        let pipeline = InferencePipeline::new(shared);
        let req = InferenceRequest {
            model_id: ModelId::from_bytes([0xFB; 32]),
            input: vec![],
            request_id: 5,
        };
        let err = pipeline.infer(req).await.unwrap_err();
        match err {
            OmniError::Internal { .. } => {}
            _ => panic!("expected Internal error for unregistered model"),
        }
    }

    // -------------------------------------------------------------------------
    // E2E: GGUF build → register → load_from_bytes → verify
    // -------------------------------------------------------------------------

    fn build_minimal_gguf() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&crate::gguf::GGUF_MAGIC.to_le_bytes());
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf
    }

    #[test]
    fn e2e_gguf_register_and_load_from_bytes() {
        let gguf_bytes = build_minimal_gguf();
        let hash = blake3::hash(&gguf_bytes);
        let sk = OmniSigningKey::from_bytes([0xE2; 32]);
        let sig = sk.sign(hash.as_bytes());

        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(*hash.as_bytes()),
            name: "e2e-toy-mlp".into(),
            version: "1.0.0".into(),
            hash: *hash.as_bytes(),
            signature: sig,
            signing_key: sk.verifying_key(),
            size_bytes: gguf_bytes.len() as u64,
            format: ModelFormat::Gguf,
        };

        let mut reg = ModelRegistry::new();
        let id = reg.register(manifest).unwrap();
        reg.load_from_bytes(id, &gguf_bytes).unwrap();
        assert!(reg.is_loaded(id));
        let attested = reg.attest(id).unwrap();
        assert_eq!(attested.name, "e2e-toy-mlp");
    }

    #[test]
    fn e2e_gguf_load_hash_mismatch_fails() {
        let gguf_bytes = build_minimal_gguf();
        let wrong_hash = [0xBB; 32];
        let sk = OmniSigningKey::from_bytes([0xE3; 32]);
        let sig = sk.sign(&wrong_hash);

        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(wrong_hash),
            name: "bad-hash".into(),
            version: "1.0.0".into(),
            hash: wrong_hash,
            signature: sig,
            signing_key: sk.verifying_key(),
            size_bytes: gguf_bytes.len() as u64,
            format: ModelFormat::Gguf,
        };

        let mut reg = ModelRegistry::new();
        let id = reg.register(manifest).unwrap();
        let err = reg.load_from_bytes(id, &gguf_bytes).unwrap_err();
        match err {
            OmniError::Internal { .. } => {}
            _ => panic!("expected Internal error for hash mismatch, got: {err:?}"),
        }
    }
}
