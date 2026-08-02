//! AES-256-GCM encryption for registry backups (Phase 10: "encryption
//! and key management"). Deliberately narrow: this encrypts a single
//! file (a `registry backup` output) under a caller-supplied symmetric
//! key, it is not a general KMS. Key generation, storage, and rotation
//! are the caller's responsibility — `capsule datakey` just produces a
//! random key in the same "print or write to a file" shape as `capsule
//! keygen` does for signing keys.

use aes_gcm::aead::{Aead, Generate, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

const NONCE_LEN: usize = 12;

/// Generates a random 256-bit key and returns it hex-encoded.
pub fn generate_key_hex() -> String {
    let key = Key::<Aes256Gcm>::generate();
    encode_hex(&key)
}

/// Encrypts `input`'s bytes under `key_hex` and writes `nonce || ciphertext`
/// (ciphertext includes the GCM authentication tag) to `output`.
pub fn encrypt_file(key_hex: &str, input: &Path, output: &Path) -> Result<()> {
    let key_bytes = decode_key(key_hex)?;
    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(key_bytes));
    let nonce = Nonce::generate();

    let plaintext = fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    fs::write(output, out).with_context(|| format!("writing {}", output.display()))
}

/// Inverse of [`encrypt_file`]: reads `nonce || ciphertext` from `input`,
/// decrypts under `key_hex`, and writes the recovered plaintext to
/// `output`. Fails (rather than returning garbage) if the key is wrong or
/// the file was tampered with, since GCM authenticates the ciphertext.
pub fn decrypt_file(key_hex: &str, input: &Path, output: &Path) -> Result<()> {
    let key_bytes = decode_key(key_hex)?;
    let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(key_bytes));

    let data = fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    if data.len() < NONCE_LEN {
        bail!("{} is too short to be an encrypted backup", input.display());
    }
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::try_from(nonce_bytes).expect("nonce_bytes is exactly NONCE_LEN long");

    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed: wrong key, or the file is corrupted/tampered with"))?;
    fs::write(output, plaintext).with_context(|| format!("writing {}", output.display()))
}

fn decode_key(key_hex: &str) -> Result<[u8; 32]> {
    let bytes = decode_hex(key_hex.trim())?;
    let array: [u8; 32] = bytes.try_into().map_err(|v: Vec<u8>| anyhow::anyhow!("key must be 32 bytes (64 hex chars), got {}", v.len()))?;
    Ok(array)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        bail!("hex string has odd length");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| anyhow::anyhow!("invalid hex byte '{}': {e}", &s[i..i + 2])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempfile(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("capsule-cli-crypto-test-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn encrypt_then_decrypt_round_trips() {
        let key = generate_key_hex();
        let input = tempfile("plain.txt");
        let encrypted = tempfile("cipher.bin");
        let decrypted = tempfile("decrypted.txt");
        fs::write(&input, b"some backup bytes, not really sqlite but stands in for it").unwrap();

        encrypt_file(&key, &input, &encrypted).unwrap();
        assert_ne!(fs::read(&encrypted).unwrap(), fs::read(&input).unwrap());

        decrypt_file(&key, &encrypted, &decrypted).unwrap();
        assert_eq!(fs::read(&decrypted).unwrap(), fs::read(&input).unwrap());

        for f in [&input, &encrypted, &decrypted] {
            fs::remove_file(f).ok();
        }
    }

    #[test]
    fn decrypting_with_the_wrong_key_fails() {
        let key_a = generate_key_hex();
        let key_b = generate_key_hex();
        let input = tempfile("plain2.txt");
        let encrypted = tempfile("cipher2.bin");
        let decrypted = tempfile("decrypted2.txt");
        fs::write(&input, b"secret").unwrap();

        encrypt_file(&key_a, &input, &encrypted).unwrap();
        assert!(decrypt_file(&key_b, &encrypted, &decrypted).is_err());

        for f in [&input, &encrypted] {
            fs::remove_file(f).ok();
        }
    }

    #[test]
    fn generated_keys_are_32_bytes_hex_encoded() {
        let key = generate_key_hex();
        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
