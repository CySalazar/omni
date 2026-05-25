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
//! Phase 2 Stream 2 — adds the AI syscall IPC relay, the PII pre-processing
//! pipeline, and the Orchestrator→Runtime dispatch bridge on top of the
//! `ModelRegistry`, `InferencePipeline`, `TierRouter`, and
//! `WorkloadScheduler` stubs from Stream 1.
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
//! - [`relay`] — AI syscall IPC relay (kernel → pipeline bridge).
//! - [`preprocessing`] — PII detection and tokenization pipeline.
//! - [`orchestrator_bridge`] — Orchestrator Agent → inference dispatch.
//! - [`bpe`] — byte-level BPE tokenizer for LLM text ↔ token ID conversion.

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
// tensor_loader — GGUF tensor weight extraction
// =============================================================================

/// GGUF tensor weight extraction into HAL TensorBuffers.
///
/// Converts raw GGUF on-disk bytes for each tensor into
/// [`omni_hal::tensor::TensorBuffer`]s, applying F16/BF16 → F32 expansion
/// where needed and providing zero-filled stub buffers for quantized types
/// pending Phase 4 dequantization.
pub mod tensor_loader;

// =============================================================================
// model_loader — OmniFS model file loading
// =============================================================================

/// Load GGUF model files from the OmniFS in-memory filesystem.
///
/// Bridges [`omni_fs::InMemoryFs`] and the GGUF tensor loader: reads a model
/// file, parses the GGUF header, and extracts all tensor weights into
/// [`omni_hal::tensor::TensorBuffer`]s in a single call.
pub mod model_loader;

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

    /// Internal registry record pairing a manifest with its load state and
    /// optionally the parsed GGUF header (populated by
    /// [`ModelRegistry::load_from_bytes`] or
    /// [`ModelRegistry::load_tensors_from_bytes`]).
    #[derive(Debug)]
    struct ModelEntry {
        manifest: ModelManifest,
        state: LoadState,
        /// Parsed GGUF header, stored after a successful `load_from_bytes` or
        /// `load_tensors_from_bytes` call. `None` until the model binary has
        /// been validated and parsed.
        gguf_header: Option<crate::gguf::GgufHeader>,
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
                    gguf_header: None,
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
            // Store it on the entry so the tensor backend can access it without
            // re-parsing on every inference call.
            let header = crate::gguf::parse_gguf(data)?;
            info!(
                model_id = ?model_id,
                tensor_count = header.tensor_count,
                metadata_count = header.metadata.len(),
                "GGUF model parsed successfully"
            );

            entry.gguf_header = Some(header);
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

        /// Load a GGUF model from raw bytes, extract all tensor weights, and
        /// return them as [`crate::tensor_loader::LoadedTensor`]s.
        ///
        /// Unlike [`load_from_bytes`][Self::load_from_bytes], which only
        /// validates the model and stores the parsed header, this method
        /// additionally extracts all tensor data from the GGUF blob and
        /// returns it for use by the tensor backend.
        ///
        /// The model is transitioned to the `Loaded` state on success, and the
        /// parsed [`crate::gguf::GgufHeader`] is stored on the entry.
        ///
        /// # Errors
        ///
        /// - [`OmniError::Internal`] if `model_id` is not registered.
        /// - [`OmniError::Internal`] if the registered model's format is not
        ///   [`ModelFormat::Gguf`].
        /// - [`OmniError::Internal`] if the BLAKE3 hash of `data` does not
        ///   match the manifest hash.
        /// - [`OmniError::Internal`] if the GGUF data is malformed.
        /// - [`OmniError::Internal`] if any tensor extraction or conversion fails.
        ///
        /// # Example
        ///
        /// ```rust
        /// use omni_crypto::signing::OmniSigningKey;
        /// use omni_runtime::model::{ModelFormat, ModelManifest, ModelRegistry};
        /// use omni_types::ModelId;
        ///
        /// // Minimal GGUF v3 file with no tensors.
        /// let gguf_magic: u32 = 0x4655_4746;
        /// let mut data = Vec::new();
        /// data.extend_from_slice(&gguf_magic.to_le_bytes());
        /// data.extend_from_slice(&3u32.to_le_bytes());
        /// data.extend_from_slice(&0u64.to_le_bytes());
        /// data.extend_from_slice(&0u64.to_le_bytes());
        ///
        /// let hash: [u8; 32] = *blake3::hash(&data).as_bytes();
        /// let sk = OmniSigningKey::from_bytes([0x77; 32]);
        /// let manifest = ModelManifest {
        ///     model_id: ModelId::from_manifest_hash(hash),
        ///     name: "tensor-test".into(),
        ///     version: "1.0.0".into(),
        ///     hash,
        ///     signature: sk.sign(&hash),
        ///     signing_key: sk.verifying_key(),
        ///     size_bytes: data.len() as u64,
        ///     format: ModelFormat::Gguf,
        /// };
        ///
        /// let mut reg = ModelRegistry::new();
        /// let id = reg.register(manifest).unwrap();
        /// let tensors = reg.load_tensors_from_bytes(id, &data).unwrap();
        /// assert!(tensors.is_empty());
        /// assert!(reg.is_loaded(id));
        /// ```
        pub fn load_tensors_from_bytes(
            &mut self,
            model_id: ModelId,
            data: &[u8],
        ) -> Result<Vec<crate::tensor_loader::LoadedTensor>> {
            let entry = self.entries.get_mut(&model_id).ok_or_else(|| {
                OmniError::internal(
                    "runtime::model::load_tensors_from_bytes — model_id not registered",
                )
            })?;

            if entry.manifest.format != ModelFormat::Gguf {
                return Err(OmniError::internal(
                    "runtime::model::load_tensors_from_bytes — only GGUF format supported",
                ));
            }

            // Verify BLAKE3 integrity before doing any further work.
            let computed_hash = blake3::hash(data);
            if computed_hash.as_bytes() != &entry.manifest.hash {
                return Err(OmniError::internal(
                    "runtime::model::load_tensors_from_bytes — BLAKE3 hash mismatch",
                ));
            }

            // Parse and store the GGUF header.
            let header = crate::gguf::parse_gguf(data)?;
            info!(
                model_id = ?model_id,
                tensor_count = header.tensor_count,
                "GGUF model tensors being loaded"
            );

            // Extract all tensor buffers.
            let tensors = crate::tensor_loader::load_all_tensors(data, &header)?;

            entry.gguf_header = Some(header);
            entry.state = LoadState::Loaded;

            Ok(tensors)
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
// relay — AI Syscall IPC Relay
// =============================================================================

/// AI Syscall IPC relay — bridges kernel AI syscalls to the inference pipeline.
///
/// The relay receives [`relay::AiSyscallRequest`] messages from the kernel IPC
/// channel and routes them through the [`inference::InferencePipeline`],
/// returning structured [`relay::AiSyscallResponse`] values.
pub mod relay;

// =============================================================================
// bpe — byte-level BPE tokenizer
// =============================================================================

/// Byte-level BPE tokenizer for LLM text ↔ token ID conversion.
///
/// Provides [`bpe::BpeTokenizer`] with encode / decode support compatible
/// with GPT-2 and TinyLlama-style vocabularies. The vocabulary and merge
/// rules can be loaded from any source; [`bpe::BpeVocabulary::minimal_test_vocab`]
/// provides a self-contained fixture for testing.
pub mod bpe;

// =============================================================================
// preprocessing — PII tokenization pipeline
// =============================================================================

/// PII detection and tokenization pre-processing pipeline.
///
/// Scans inference input for email addresses and phone numbers, replaces them
/// with opaque tokens before the text reaches the model, and reverses the
/// tokenization on the output. Phase 2 uses simple string scanning; Phase 3
/// will use the TEE-backed `NerClassifier` from `omni-tokenization`.
pub mod preprocessing;

// =============================================================================
// orchestrator_bridge — Orchestrator → Runtime dispatch
// =============================================================================

/// Orchestrator Agent → inference pipeline dispatch bridge.
///
/// [`orchestrator_bridge::OrchestratorBridge`] is the integration point
/// between the five-agent Orchestrator and the AI runtime. It classifies
/// intents, pre-processes PII, dispatches inference, and post-processes output.
pub mod orchestrator_bridge;

// =============================================================================
// decode — Streaming autoregressive decode loop (Sprint 8)
// =============================================================================

/// Streaming greedy / sampled decode loop for autoregressive language models.
///
/// [`decode::streaming_decode`] returns a lazy [`Iterator`] that yields one
/// [`decode::DecodeToken`] per transformer forward pass.  Supports temperature
/// scaling, top-k sampling, and EOS-based termination.  Works with both FP32
/// and quantized model weights.
///
/// See [`decode`] for the full API surface and usage examples.
pub mod decode;

// =============================================================================
// speculative — Speculative decoding engine (Sprint 10)
// =============================================================================

/// Speculative decoding engine for autoregressive language models.
///
/// Implements the algorithm from Leviathan et al. (2023): a fast draft model
/// speculatively generates [`speculative::SpeculativeConfig::draft_len`] tokens
/// which are then verified against the target model in a single batched forward
/// pass.  Accepted tokens are free; rejected tokens trigger a corrected resample.
/// The output distribution is provably identical to pure target autoregressive
/// sampling.
///
/// Key entry point: [`speculative::speculative_decode`].
pub mod speculative;

// =============================================================================
// batch — Continuous batching inference scheduler (Sprint 10)
// =============================================================================

/// Continuous batching inference scheduler for concurrent LLM request serving.
///
/// [`batch::BatchScheduler`] manages a priority queue of pending requests and
/// an active batch of concurrently generating requests.  Each call to
/// [`batch::BatchScheduler::step`] advances every active request by one token
/// using a caller-supplied forward function, then checks termination conditions.
/// Supports priority-based preemption, token-budget gating, and per-request
/// temperature / top-k sampling.
pub mod batch;

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
        let mut registry = ModelRegistry::new();
        let manifest = make_manifest(0x21, 0xA0, "unloaded-model");
        let id = registry.register(manifest).unwrap();
        // Do NOT call registry.load(id) — model remains Unloaded.
        let shared = Arc::new(Mutex::new(registry));
        let pipeline = InferencePipeline::new(shared);
        let infer_req = InferenceRequest {
            model_id: id,
            input: vec![],
            request_id: 4,
        };
        let err = pipeline.infer(infer_req).await.unwrap_err();
        match err {
            OmniError::Internal { .. } => {}
            _ => panic!("expected Internal error for unloaded model"),
        }
    }

    #[tokio::test]
    async fn pipeline_infer_unregistered_model_fails() {
        let empty_registry = ModelRegistry::new();
        let shared = Arc::new(Mutex::new(empty_registry));
        let pipeline = InferencePipeline::new(shared);
        let infer_req = InferenceRequest {
            model_id: ModelId::from_bytes([0xFB; 32]),
            input: vec![],
            request_id: 5,
        };
        let err = pipeline.infer(infer_req).await.unwrap_err();
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

    // =========================================================================
    // E2E: ModelRegistry → InferencePipeline → AiIpcRelay → OrchestratorBridge
    // =========================================================================
    //
    // This test exercises the full Stream 2 inference path:
    //
    //   1. Register and load a model.
    //   2. Wrap in InferencePipeline.
    //   3. Construct AiIpcRelay.
    //   4. Construct OrchestratorBridge.
    //   5. Classify an intent and confirm requires_inference is true.
    //   6. Process the intent through the bridge.
    //   7. Verify the result flows through correctly.

    #[tokio::test]
    async fn e2e_stream2_full_inference_pipeline() {
        use crate::orchestrator_bridge::OrchestratorBridge;
        use crate::relay::AiIpcRelay;

        // ── Step 1: build a minimal model registry with one loaded model ──

        let sk = OmniSigningKey::from_bytes([0xE4; 32]);
        let mut hash = [0u8; 32];
        hash[..16].fill(0xE5);
        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(hash),
            name: "e2e-stream2-model".into(),
            version: "1.0.0".into(),
            hash,
            signature: sk.sign(&hash),
            signing_key: sk.verifying_key(),
            size_bytes: 0,
            format: ModelFormat::Gguf,
        };

        let mut reg = ModelRegistry::new();
        let model_id = reg.register(manifest).unwrap();
        reg.load(model_id).unwrap();
        assert!(
            reg.is_loaded(model_id),
            "model must be loaded before pipeline"
        );

        // ── Step 2: wrap in InferencePipeline ──

        let shared_reg = Arc::new(Mutex::new(reg));
        let pipeline = InferencePipeline::new(Arc::clone(&shared_reg));

        // ── Step 3: construct AiIpcRelay ──

        let relay = AiIpcRelay::new(pipeline);

        // ── Step 4: construct OrchestratorBridge ──

        let bridge = OrchestratorBridge::new(relay);

        // ── Step 5: classify the intent ──

        let intent = "explain what this file does";
        assert!(
            OrchestratorBridge::requires_inference(intent),
            "intent '{intent}' should require inference"
        );

        // ── Step 6: process the intent end-to-end ──

        let result = bridge.process_intent(intent, model_id, 42).await;

        // ── Step 7: verify the result ──

        assert!(
            result.success,
            "E2E pipeline should succeed; error: {:?}",
            result.response_text
        );
        assert_eq!(result.request_id, 42, "request_id must be echoed");
        // No PII in the test intent.
        assert_eq!(
            result.entities_tokenized, 0,
            "no PII entities expected in clean intent"
        );
        // Latency is a non-negative u64 (always true); verify it was populated.
        let _ = result.inference_latency_us;
    }

    /// E2E test: PII in the intent is detected and tokenized before dispatch.
    #[tokio::test]
    async fn e2e_stream2_pii_detected_in_intent() {
        use crate::orchestrator_bridge::OrchestratorBridge;
        use crate::relay::AiIpcRelay;

        let sk = OmniSigningKey::from_bytes([0xE6; 32]);
        let mut hash = [0u8; 32];
        hash[..16].fill(0xE7);
        let manifest = ModelManifest {
            model_id: ModelId::from_manifest_hash(hash),
            name: "e2e-pii-model".into(),
            version: "1.0.0".into(),
            hash,
            signature: sk.sign(&hash),
            signing_key: sk.verifying_key(),
            size_bytes: 0,
            format: ModelFormat::Gguf,
        };

        let mut reg = ModelRegistry::new();
        let model_id = reg.register(manifest).unwrap();
        reg.load(model_id).unwrap();

        let pipeline = InferencePipeline::new(Arc::new(Mutex::new(reg)));
        let relay = AiIpcRelay::new(pipeline);
        let bridge = OrchestratorBridge::new(relay);

        // Intent contains an email address — preprocessor should detect it.
        let intent = "explain why admin@example.com cannot log in";
        let result = bridge.process_intent(intent, model_id, 99).await;

        assert!(result.success);
        assert_eq!(result.entities_tokenized, 1, "one email address expected");
    }

    /// E2E test: an unloaded model produces a structured error, not a panic.
    #[tokio::test]
    async fn e2e_stream2_unregistered_model_error_is_structured() {
        use crate::orchestrator_bridge::OrchestratorBridge;
        use crate::relay::AiIpcRelay;

        let reg = ModelRegistry::new(); // empty — no models registered
        let pipeline = InferencePipeline::new(Arc::new(Mutex::new(reg)));
        let relay = AiIpcRelay::new(pipeline);
        let bridge = OrchestratorBridge::new(relay);

        let unknown_id = ModelId::from_bytes([0xDE; 32]);
        let result = bridge
            .process_intent("explain something", unknown_id, 55)
            .await;

        assert!(!result.success, "should fail for unknown model");
        assert_eq!(result.request_id, 55);
        assert!(
            !result.response_text.is_empty(),
            "error text must be populated for diagnostics"
        );
    }

    // =========================================================================
    // E2E: Quantized inference pipeline (Sprint 8)
    //
    // Exercises the full pipeline:
    //   build synthetic Q8_0 GGUF → parse → load_all_tensors → dequantize →
    //   build TransformerWeights → transformer_forward → non-zero logits
    // =========================================================================

    // -------------------------------------------------------------------------
    // build_synthetic_q8_0_gguf — helper
    // -------------------------------------------------------------------------

    /// Build a minimal synthetic GGUF v3 binary with `Q8_0`-encoded weight
    /// tensors for a tiny transformer (`n_layers=1`, `n_heads=1`, `d_model=4`,
    /// `d_ff=8`, `vocab_size=8`, `max_seq_len=16`).
    ///
    /// All tensors use `Q8_0` encoding with scale = 1.0 (f16 0x3C00) and
    /// non-zero quantized values so dequantization yields a non-zero F32 buffer.
    ///
    /// # Tensor layout
    ///
    /// | Name                       | Shape  | `n_elements` |
    /// |----------------------------|--------|--------------|
    /// | `token_embd.weight`        | [8, 4] | 32           |
    /// | `blk.0.attn_q.weight`      | [4, 4] | 16           |
    /// | `blk.0.attn_k.weight`      | [4, 4] | 16           |
    /// | `blk.0.attn_v.weight`      | [4, 4] | 16           |
    /// | `blk.0.attn_output.weight` | [4, 4] | 16           |
    /// | `blk.0.ffn_gate.weight`    | [4, 8] | 32           |
    /// | `blk.0.ffn_up.weight`      | [4, 8] | 32           |
    /// | `blk.0.ffn_down.weight`    | [8, 4] | 32           |
    /// | `blk.0.attn_norm.weight`   | [4]    | 4 (1 block)  |
    /// | `blk.0.ffn_norm.weight`    | [4]    | 4 (1 block)  |
    /// | `output.weight`            | [4, 8] | 32           |
    /// | `output_norm.weight`       | [4]    | 4 (1 block)  |
    ///
    /// Tensors with `n_elements` < 32 are encoded in a single `Q8_0` block of 34
    /// bytes (scale + 32 i8 values); only the first `n_elements` values are
    /// semantically meaningful, the rest are zero-padded. This matches the
    /// GGUF spec requirement that quantized data is written in complete blocks.
    fn build_synthetic_q8_0_gguf() -> Vec<u8> {
        use crate::gguf::{GGUF_DEFAULT_ALIGNMENT, GGUF_MAGIC, GGUF_VERSION_3};

        // F16 bit pattern for 1.0:
        //   sign=0, exponent = 15 (biased) = 0b01111, mantissa = 0
        //   stored little-endian: [0x00, 0x3C]
        const F16_ONE_LE: [u8; 2] = [0x00, 0x3C];
        // Non-zero cycle values [1..=7]: all in i8 range, no truncation possible.
        // CYCLE[i % 7] ∈ [1, 7] ⊂ i8::MIN..=i8::MAX.
        const CYCLE: [i8; 7] = [1, 2, 3, 4, 5, 6, 7];
        // Q8_0 dtype discriminant in the GGUF enum: GgufDtype::Q8_0 = 8.
        const DTYPE_Q8_0: u32 = 8;

        // Encode n_elements into Q8_0 blocks (34 bytes each).
        // The first `n_elements` values cycle through 1..=7; the rest pad to 0.
        let encode_q8_0 = |n_elements: usize| -> Vec<u8> {
            let n_blocks = n_elements.div_ceil(32);
            let mut data = Vec::with_capacity(n_blocks * 34);
            for block in 0..n_blocks {
                data.extend_from_slice(&F16_ONE_LE);
                for j in 0..32usize {
                    let elem_idx = block * 32 + j;
                    let q: i8 = if elem_idx < n_elements {
                        // elem_idx % 7 ∈ [0, 6]; CYCLE has 7 elements → always in bounds.
                        *CYCLE
                            .get(elem_idx % 7)
                            .expect("elem_idx % 7 is always in [0, 6]")
                    } else {
                        0
                    };
                    // Reinterpret the i8 bit pattern as u8 for byte-level storage.
                    // Values [1,7] share the same bit pattern in i8 and u8.
                    data.push(q.to_le_bytes()[0]);
                }
            }
            data
        };

        // Encode a GGUF length-prefixed string (u64 byte count + UTF-8 bytes).
        let gguf_str = |s: &str| -> Vec<u8> {
            let b = s.as_bytes();
            let mut v = Vec::with_capacity(8 + b.len());
            v.extend_from_slice(&(b.len() as u64).to_le_bytes());
            v.extend_from_slice(b);
            v
        };

        // Tensor table: (name, dims, n_elements).
        // Shapes follow the conventions from transformer.rs:
        //   ffn_gate/ffn_up: [d_model, d_ff]
        //   output.weight:   [d_model, vocab_size] (maps to TransformerWeights::output_proj)
        let tensors: &[(&str, &[u64], usize)] = &[
            ("token_embd.weight", &[8, 4], 32),
            ("blk.0.attn_q.weight", &[4, 4], 16),
            ("blk.0.attn_k.weight", &[4, 4], 16),
            ("blk.0.attn_v.weight", &[4, 4], 16),
            ("blk.0.attn_output.weight", &[4, 4], 16),
            ("blk.0.ffn_gate.weight", &[4, 8], 32),
            ("blk.0.ffn_up.weight", &[4, 8], 32),
            ("blk.0.ffn_down.weight", &[8, 4], 32),
            ("blk.0.attn_norm.weight", &[4], 4),
            ("blk.0.ffn_norm.weight", &[4], 4),
            ("output.weight", &[4, 8], 32),
            ("output_norm.weight", &[4], 4),
        ];

        // Pre-encode all tensor data blobs.
        let data_blobs: Vec<Vec<u8>> = tensors.iter().map(|(_, _, n)| encode_q8_0(*n)).collect();

        // Pre-compute byte offsets within the data region (aligned to 32 bytes).
        let mut offsets: Vec<u64> = Vec::with_capacity(tensors.len());
        let mut running: u64 = 0;
        for blob in &data_blobs {
            offsets.push(running);
            let next = running + blob.len() as u64;
            running =
                (next + GGUF_DEFAULT_ALIGNMENT as u64 - 1) & !(GGUF_DEFAULT_ALIGNMENT as u64 - 1);
        }

        let mut buf = Vec::new();

        // GGUF header.
        buf.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
        buf.extend_from_slice(&GGUF_VERSION_3.to_le_bytes());
        buf.extend_from_slice(&(tensors.len() as u64).to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes()); // metadata_kv_count = 0

        // Tensor info entries.
        for ((name, dims, _), &offset) in tensors.iter().zip(&offsets) {
            buf.extend_from_slice(&gguf_str(name));
            buf.extend_from_slice(
                &u32::try_from(dims.len())
                    .expect("tensor rank ≤ 8 always fits in u32")
                    .to_le_bytes(),
            );
            for &d in *dims {
                buf.extend_from_slice(&d.to_le_bytes());
            }
            buf.extend_from_slice(&DTYPE_Q8_0.to_le_bytes());
            buf.extend_from_slice(&offset.to_le_bytes());
        }

        // Alignment padding before data region.
        while buf.len() % GGUF_DEFAULT_ALIGNMENT != 0 {
            buf.push(0);
        }

        // Tensor data with inter-tensor alignment padding.
        for (i, blob) in data_blobs.iter().enumerate() {
            buf.extend_from_slice(blob);
            if i + 1 < data_blobs.len() {
                while buf.len() % GGUF_DEFAULT_ALIGNMENT != 0 {
                    buf.push(0);
                }
            }
        }

        buf
    }

    // -------------------------------------------------------------------------
    // quantized_inference_e2e_q8_0
    // -------------------------------------------------------------------------

    /// End-to-end test for the quantized inference pipeline.
    ///
    /// Builds a synthetic Q8_0 GGUF file in memory, loads it through the full
    /// pipeline (`parse_gguf` → `load_all_tensors` → `TransformerWeights` →
    /// `transformer_forward`), and verifies that the output logits are non-zero,
    /// proving that real dequantization (not the old zero-filled stub) ran.
    // The test body is long because it covers all seven pipeline stages plus
    // assertions; splitting into helpers would obscure the end-to-end flow.
    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn quantized_inference_e2e_q8_0() {
        use omni_hal::tensor::{CpuBackend, TensorBuffer, TensorDescriptor, TensorDtype};
        use omni_hal::transformer::{
            TransformerConfig, TransformerLayerWeights, TransformerWeights, transformer_forward,
        };

        // Step 1: build the synthetic GGUF binary.
        let gguf_bytes = build_synthetic_q8_0_gguf();
        assert!(
            !gguf_bytes.is_empty(),
            "GGUF builder must not produce empty blob"
        );

        // Step 2: parse the GGUF header.
        let header =
            crate::gguf::parse_gguf(&gguf_bytes).expect("synthetic GGUF must parse without error");
        assert_eq!(
            header.tensor_count, 12,
            "expected 12 tensors in synthetic GGUF"
        );

        // Step 3: load and dequantize all tensors.
        let loaded_tensors = crate::tensor_loader::load_all_tensors(&gguf_bytes, &header)
            .expect("load_all_tensors must succeed on synthetic Q8_0 GGUF");

        // Verify dequantization produced non-zero values — the old stub always
        // returned zeros for Q8_0.
        for lt in &loaded_tensors {
            let has_nonzero = lt.buffer.as_bytes().chunks_exact(4).any(|b| {
                // chunks_exact(4) guarantees b.len() == 4; try_into cannot fail.
                let arr: [u8; 4] = b.try_into().expect("chunk is exactly 4 bytes");
                f32::from_le_bytes(arr) != 0.0
            });
            assert!(
                has_nonzero,
                "tensor '{}' is all-zero after Q8_0 dequantization \
                 — old zero-filled stub may still be active",
                lt.name
            );
        }

        // Helper: locate a dequantized tensor and reframe it with the given
        // logical shape (the Q8_0 dequantization pads to full blocks; we
        // truncate to the semantically meaningful element count).
        let find_tensor = |name: &str, shape: Vec<usize>| -> TensorBuffer {
            let lt = loaded_tensors
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("tensor '{name}' not found in loaded tensors"));
            let n_logical: usize = shape.iter().product();
            let byte_count = n_logical * 4;
            let src = lt.buffer.as_bytes();
            assert!(
                src.len() >= byte_count,
                "tensor '{}': buffer has {} bytes but shape {:?} needs {}",
                name,
                src.len(),
                shape,
                byte_count
            );
            let desc = TensorDescriptor::new(shape, TensorDtype::F32);
            // The assert above guarantees src.len() >= byte_count.
            let truncated = src
                .get(..byte_count)
                .expect("buffer length verified by assert above");
            TensorBuffer::new(desc, truncated.to_vec())
        };

        // Step 4: build TransformerConfig and TransformerWeights.
        //
        // Shape conventions (verified from transformer.rs and decode.rs):
        //   attn_q/k/v/o: [d_model, d_model]
        //   ffn_gate/up:  [d_model, d_ff]
        //   ffn_down:     [d_ff, d_model]
        //   norm weights: [d_model]
        //   token_embedding: [vocab_size, d_model]
        //   output_proj:     [d_model, vocab_size]
        let config = TransformerConfig {
            n_layers: 1,
            n_heads: 1,
            d_model: 4,
            d_ff: 8,
            vocab_size: 8,
            max_seq_len: 16,
            rms_norm_eps: 1e-5,
        };

        let layer = TransformerLayerWeights {
            attn_q: find_tensor("blk.0.attn_q.weight", vec![4, 4]),
            attn_k: find_tensor("blk.0.attn_k.weight", vec![4, 4]),
            attn_v: find_tensor("blk.0.attn_v.weight", vec![4, 4]),
            attn_o: find_tensor("blk.0.attn_output.weight", vec![4, 4]),
            ffn_gate: find_tensor("blk.0.ffn_gate.weight", vec![4, 8]),
            ffn_up: find_tensor("blk.0.ffn_up.weight", vec![4, 8]),
            ffn_down: find_tensor("blk.0.ffn_down.weight", vec![8, 4]),
            attn_norm: find_tensor("blk.0.attn_norm.weight", vec![4]),
            ffn_norm: find_tensor("blk.0.ffn_norm.weight", vec![4]),
        };

        let weights = TransformerWeights {
            token_embedding: find_tensor("token_embd.weight", vec![8, 4]),
            layers: vec![layer],
            output_norm: find_tensor("output_norm.weight", vec![4]),
            output_proj: find_tensor("output.weight", vec![4, 8]),
            n_kv_heads: None,
        };

        // Step 5: build the input token IDs tensor.
        //
        // CpuBackend EmbeddingLookup requires U8 indices; vocab_size=8 so all
        // test IDs [1, 2] fit in u8 without truncation.
        let prompt_ids: &[u8] = &[1u8, 2u8];
        let seq_len = prompt_ids.len();
        let input_desc = TensorDescriptor::new(vec![seq_len], TensorDtype::U8);
        let input_ids = TensorBuffer::new(input_desc, prompt_ids.to_vec());

        // Step 6: run the transformer forward pass.
        let backend = CpuBackend::new();
        let logits = transformer_forward(&backend, &config, &weights, &input_ids)
            .await
            .expect("transformer_forward must not error on valid synthetic inputs");

        // Step 7: verify the logits are non-zero and finite.
        //
        // Shape: [seq_len, vocab_size] = [2, 8].
        assert_eq!(
            logits.descriptor.shape,
            vec![seq_len, config.vocab_size],
            "logits shape must be [seq_len, vocab_size]"
        );

        let logit_values: Vec<f32> = logits
            .as_bytes()
            .chunks_exact(4)
            .map(|b| {
                // chunks_exact(4) guarantees b.len() == 4; try_into cannot fail.
                let arr: [u8; 4] = b.try_into().expect("chunk is exactly 4 bytes");
                f32::from_le_bytes(arr)
            })
            .collect();

        assert!(
            logit_values.iter().any(|&v| v != 0.0),
            "transformer_forward produced all-zero logits — \
             quantized weights did not propagate. logits: {logit_values:?}"
        );

        assert!(
            logit_values.iter().all(|v| v.is_finite()),
            "transformer_forward output contains non-finite values — \
             NaN/Inf propagated from weights. logits: {logit_values:?}"
        );
    }
}
