//! Integration tests for [`omni_tee::MockTeeBackend`].
//!
//! These tests exercise the full attest → verify → seal → unseal →
//! `derive_key_for` flow end-to-end, simulating the typical
//! mesh-handshake shape from the consumer's perspective (`omni-mesh`
//! will replicate this pattern with a real backend in P4).

#![cfg(feature = "mock")]
// Integration-test code is allowed to panic on assertion failure: a
// failed `expect`/`unwrap`/index in a test surfaces as the test
// failure itself, which is the desired behaviour. `similar_names` is
// silenced because the two-actor handshake naming convention
// (alice/bob/blob) is more readable than longer, distinct names.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::similar_names
)]

use omni_tee::{
    Measurement, MockTeeBackend, Nonce, SealPolicy, TeeBackend, TeeErrorKind, TeeFamily,
};

#[test]
fn end_to_end_mock_handshake_simulation() {
    // Simulate two parties (`alice` and `bob`), each with their own mock
    // TEE running at a distinct measurement.
    let alice_measurement = Measurement([0x11u8; 48]);
    let bob_measurement = Measurement([0x22u8; 48]);
    let alice = MockTeeBackend::with_measurement(alice_measurement);
    let bob = MockTeeBackend::with_measurement(bob_measurement);

    // Alice sends a fresh nonce to Bob.
    let nonce_a_to_b = Nonce([0xA1u8; 32]);
    // Bob attests using Alice's nonce.
    let bob_quote = bob
        .attest(&nonce_a_to_b, Some(b"transcript-hash-1"))
        .unwrap();

    // Alice verifies Bob's quote, expecting Bob's known measurement.
    alice
        .verify_quote(&bob_quote, &nonce_a_to_b, &bob_measurement)
        .unwrap();

    // Symmetric direction: Bob sends a fresh nonce to Alice.
    let nonce_b_to_a = Nonce([0xB1u8; 32]);
    let alice_quote = alice.attest(&nonce_b_to_a, None).unwrap();
    bob.verify_quote(&alice_quote, &nonce_b_to_a, &alice_measurement)
        .unwrap();

    // Both sides derive a TEE-bound shared key from the peer's quote.
    // Per the XOR-fold mock impl, both sides must compute the same key
    // (XOR is commutative). The real impl will be HKDF over an attested
    // local secret + peer measurement, also symmetric.
    let key_a = alice.derive_key_for(&bob_quote).unwrap();
    let key_b = bob.derive_key_for(&alice_quote).unwrap();
    assert_eq!(key_a.as_bytes(), key_b.as_bytes());

    // The derived key is non-trivial — it depends on both measurements.
    assert!(!key_a.as_bytes().iter().all(|b| *b == 0));

    // Sealing under Alice's measurement, unsealing on Alice succeeds,
    // unsealing on Bob fails.
    let policy = SealPolicy::new(TeeFamily::Mock, alice_measurement);
    let blob = alice.seal(b"user secret payload", &policy).unwrap();
    let recovered = alice.unseal(&blob).unwrap();
    assert_eq!(recovered, b"user secret payload");
    assert_eq!(
        bob.unseal(&blob).unwrap_err().kind,
        TeeErrorKind::UnsealFailed
    );
}

#[test]
fn tampered_quote_is_rejected() {
    let backend = MockTeeBackend::new();
    let nonce = Nonce([0xCDu8; 32]);
    let mut quote = backend.attest(&nonce, None).unwrap();

    // Flip a bit in the body. The mock detects this via the marker-prefix
    // check (any tampering that doesn't preserve the marker is caught).
    quote.body[0] ^= 0x01;

    assert_eq!(
        backend
            .verify_quote(&quote, &nonce, backend.measurement())
            .unwrap_err()
            .kind,
        TeeErrorKind::QuoteSignatureInvalid
    );
}

#[test]
fn replayed_quote_rejected_via_nonce() {
    let backend = MockTeeBackend::new();
    let session_one_nonce = Nonce([0x01u8; 32]);
    let quote = backend.attest(&session_one_nonce, None).unwrap();

    // A later session uses a different nonce. Replaying the old quote
    // is detected at verification time.
    let session_two_nonce = Nonce([0x02u8; 32]);
    assert_eq!(
        backend
            .verify_quote(&quote, &session_two_nonce, backend.measurement())
            .unwrap_err()
            .kind,
        TeeErrorKind::QuoteNonceMismatch
    );
}

#[test]
fn cross_family_quote_rejected() {
    let backend = MockTeeBackend::new();
    let nonce = Nonce::zero();
    let mut quote = backend.attest(&nonce, None).unwrap();

    // Falsely claim TDX family on a quote produced by the Mock.
    quote.family = TeeFamily::IntelTdx;

    assert_eq!(
        backend
            .verify_quote(&quote, &nonce, backend.measurement())
            .unwrap_err()
            .kind,
        TeeErrorKind::QuoteSignatureInvalid
    );
}
