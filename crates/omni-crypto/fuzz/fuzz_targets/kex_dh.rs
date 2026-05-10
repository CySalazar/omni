//! Fuzz target: `OmniStaticSecret::diffie_hellman` must not panic on
//! arbitrary peer-public-key inputs.
//!
//! Splits the input into a 32-byte local secret and a 32-byte
//! attacker-supplied peer pubkey. The DH operation MUST always yield
//! a (possibly trivial) shared secret — never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use omni_crypto::kex::{OmniPublicKey, OmniStaticSecret, KEY_LEN};

fuzz_target!(|data: &[u8]| {
    if data.len() < KEY_LEN * 2 {
        return;
    }
    let mut sk_bytes = [0u8; KEY_LEN];
    sk_bytes.copy_from_slice(&data[..KEY_LEN]);
    let mut pk_bytes = [0u8; KEY_LEN];
    pk_bytes.copy_from_slice(&data[KEY_LEN..KEY_LEN * 2]);

    let sk = OmniStaticSecret::from_bytes(sk_bytes);
    let pk = OmniPublicKey::from_bytes(pk_bytes);
    let _ss = sk.diffie_hellman(&pk);
});
