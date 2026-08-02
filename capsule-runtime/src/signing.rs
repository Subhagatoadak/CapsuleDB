//! Signed capsule verification and signed publishers (Phase 5 / Phase 9).
//!
//! A signature attests to a capsule's **content hash**, not its identity
//! or lifecycle state — the same epistemic payload signed once is valid
//! under any logical ID or version it's registered as (see the
//! content-addressing rationale in `capsule-core::hashing`). This is
//! deliberately a thin, self-contained layer: it does not implement key
//! distribution, revocation, or a certificate/trust chain — a publisher's
//! public key has to reach a verifier out of band for now. Full trust
//! federation is Phase 9's "trust federation policies," not this.

use crate::hex;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("invalid key material: {0}")]
    InvalidKey(String),
    #[error("invalid signature encoding: {0}")]
    InvalidSignature(String),
    #[error("signature verification failed")]
    VerificationFailed,
    #[error("failed to generate random key material: {0}")]
    Random(String),
}

/// An Ed25519 keypair for signing capsule content hashes.
pub struct KeyPair {
    signing_key: SigningKey,
}

impl KeyPair {
    /// Generates a new keypair from OS randomness.
    pub fn generate() -> Result<Self, SigningError> {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(|e| SigningError::Random(e.to_string()))?;
        Ok(Self { signing_key: SigningKey::from_bytes(&secret) })
    }

    /// Reconstructs a keypair from a 32-byte secret key previously
    /// obtained from [`KeyPair::secret_key_hex`].
    pub fn from_secret_hex(hex_str: &str) -> Result<Self, SigningError> {
        let bytes = hex::decode(hex_str).map_err(SigningError::InvalidKey)?;
        let array: [u8; 32] = bytes.try_into().map_err(|_| SigningError::InvalidKey("secret key must be 32 bytes".to_string()))?;
        Ok(Self { signing_key: SigningKey::from_bytes(&array) })
    }

    pub fn secret_key_hex(&self) -> String {
        hex::encode(&self.signing_key.to_bytes())
    }

    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().as_bytes())
    }

    /// Signs `content_hash` (e.g. `"sha256:9f3a..."`), returning a hex
    /// signature.
    pub fn sign_content_hash(&self, content_hash: &str) -> String {
        let signature = self.signing_key.sign(content_hash.as_bytes());
        hex::encode(&signature.to_bytes())
    }
}

/// Verifies a hex signature over `content_hash` against a hex-encoded
/// Ed25519 public key.
pub fn verify_content_hash(public_key_hex: &str, content_hash: &str, signature_hex: &str) -> Result<(), SigningError> {
    let key_bytes = hex::decode(public_key_hex).map_err(SigningError::InvalidKey)?;
    let key_array: [u8; 32] = key_bytes.try_into().map_err(|_| SigningError::InvalidKey("public key must be 32 bytes".to_string()))?;
    let verifying_key = VerifyingKey::from_bytes(&key_array).map_err(|e| SigningError::InvalidKey(e.to_string()))?;

    let sig_bytes = hex::decode(signature_hex).map_err(SigningError::InvalidSignature)?;
    let sig_array: [u8; 64] = sig_bytes.try_into().map_err(|_| SigningError::InvalidSignature("signature must be 64 bytes".to_string()))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

    verifying_key.verify(content_hash.as_bytes(), &signature).map_err(|_| SigningError::VerificationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_round_trip() {
        let keypair = KeyPair::generate().unwrap();
        let hash = "sha256:deadbeef";
        let signature = keypair.sign_content_hash(hash);
        assert!(verify_content_hash(&keypair.public_key_hex(), hash, &signature).is_ok());
    }

    #[test]
    fn verification_fails_for_tampered_content() {
        let keypair = KeyPair::generate().unwrap();
        let signature = keypair.sign_content_hash("sha256:original");
        let err = verify_content_hash(&keypair.public_key_hex(), "sha256:tampered", &signature).unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed));
    }

    #[test]
    fn verification_fails_for_wrong_key() {
        let signer = KeyPair::generate().unwrap();
        let other = KeyPair::generate().unwrap();
        let signature = signer.sign_content_hash("sha256:x");
        let err = verify_content_hash(&other.public_key_hex(), "sha256:x", &signature).unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed));
    }

    #[test]
    fn secret_key_round_trips_through_hex() {
        let keypair = KeyPair::generate().unwrap();
        let restored = KeyPair::from_secret_hex(&keypair.secret_key_hex()).unwrap();
        assert_eq!(keypair.public_key_hex(), restored.public_key_hex());
    }
}
