use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Verifier, Signature};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::Path;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use argon2::Argon2;

use zeroize::Zeroize;

use crate::errors::{SealedError, SealedResult};

/// Magic header for encrypted key files.
pub const ENCRYPTED_KEY_MAGIC: &[u8] = b"SEALED_ENC_V1";

/// Ed25519 signing keypair.
pub struct SealedKeyPair {
    signing_key: SigningKey,
}

impl Drop for SealedKeyPair {
    fn drop(&mut self) {
        // Best-effort zeroization (ed25519_dalek doesn't impl Zeroize)
        let mut zero_bytes = [0u8; 32];
        self.signing_key = SigningKey::from_bytes(&zero_bytes);
        zero_bytes.zeroize();
    }
}

/// Signed envelope: payload + signature + public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    pub payload: String,
    pub signature: String,
    pub public_key: String,
    pub algorithm: String,
}

impl SealedKeyPair {
    /// Generate a new Ed25519 keypair.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Load from a 32-byte raw secret key file.
    pub fn load(path: &Path) -> SealedResult<Self> {
        let bytes = fs::read(path).map_err(|_| {
            SealedError::KeyError(format!("Failed to read key file: {}", path.display()))
        })?;
        if bytes.len() != 32 {
            return Err(SealedError::KeyError(
                "Invalid key file: expected 32 bytes".to_string(),
            ));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        let signing_key = SigningKey::from_bytes(&key_bytes);
        key_bytes.zeroize();
        Ok(Self { signing_key })
    }

    /// Save the secret key to a file (unencrypted, 0600 on Unix).
    pub fn save_secret(&self, path: &Path) -> SealedResult<()> {
        fs::write(path, self.signing_key.to_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Save the secret key encrypted (AES-256-GCM + Argon2).
    pub fn save_secret_encrypted(&self, path: &Path, password: &str) -> SealedResult<()> {
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);

        let mut derived_key = [0u8; 32];
        Argon2::default()
            .hash_password_into(password.as_bytes(), &salt, &mut derived_key)
            .map_err(|e| SealedError::KeyError(format!("Key derivation failed: {}", e)))?;

        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| SealedError::KeyError(format!("Cipher init failed: {}", e)))?;
        derived_key.zeroize();

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, self.signing_key.to_bytes().as_ref())
            .map_err(|e| SealedError::KeyError(format!("Encryption failed: {}", e)))?;

        let mut output = Vec::with_capacity(ENCRYPTED_KEY_MAGIC.len() + 16 + 12 + ciphertext.len());
        output.extend_from_slice(ENCRYPTED_KEY_MAGIC);
        output.extend_from_slice(&salt);
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);

        fs::write(path, &output)?;
        Ok(())
    }

    /// Load from a password-encrypted file.
    pub fn load_encrypted(path: &Path, password: &str) -> SealedResult<Self> {
        let data = fs::read(path).map_err(|_| {
            SealedError::KeyError(format!("Failed to read key file: {}", path.display()))
        })?;

        let magic_len = ENCRYPTED_KEY_MAGIC.len();

        let payload = if data.starts_with(ENCRYPTED_KEY_MAGIC) {
            &data[magic_len..]
        } else {
            &data
        };

        if payload.len() < 76 {
            return Err(SealedError::KeyError(
                "Invalid encrypted key file: too short".to_string(),
            ));
        }

        let salt = &payload[..16];
        let nonce_bytes = &payload[16..28];
        let ciphertext = &payload[28..];

        let mut derived_key = [0u8; 32];
        Argon2::default()
            .hash_password_into(password.as_bytes(), salt, &mut derived_key)
            .map_err(|e| SealedError::KeyError(format!("Key derivation failed: {}", e)))?;

        let cipher = Aes256Gcm::new_from_slice(&derived_key)
            .map_err(|e| SealedError::KeyError(format!("Cipher init failed: {}", e)))?;
        derived_key.zeroize();

        let nonce = Nonce::from_slice(nonce_bytes);
        let mut plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| SealedError::KeyError("Decryption failed: wrong password".to_string()))?;

        if plaintext.len() != 32 {
            plaintext.zeroize();
            return Err(SealedError::KeyError(
                "Decrypted key has wrong length".to_string(),
            ));
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&plaintext);
        plaintext.zeroize();
        let signing_key = SigningKey::from_bytes(&key_bytes);
        key_bytes.zeroize();
        Ok(Self { signing_key })
    }

    /// Save the public key.
    pub fn save_public(&self, path: &Path) -> SealedResult<()> {
        let verifying_key = self.signing_key.verifying_key();
        fs::write(path, verifying_key.to_bytes())?;
        Ok(())
    }

    /// Public key as base64.
    pub fn public_key_base64(&self) -> String {
        let verifying_key = self.signing_key.verifying_key();
        BASE64.encode(verifying_key.to_bytes())
    }

    /// Sign a payload string.
    pub fn sign(&self, payload: &str) -> SignedEnvelope {
        let signature = self.signing_key.sign(payload.as_bytes());
        SignedEnvelope {
            payload: payload.to_string(),
            signature: BASE64.encode(signature.to_bytes()),
            public_key: self.public_key_base64(),
            algorithm: "Ed25519".to_string(),
        }
    }
}

impl SignedEnvelope {
    /// Verify using the embedded public key.
    pub fn verify(&self) -> SealedResult<()> {
        let pub_bytes = BASE64.decode(&self.public_key).map_err(|e| {
            SealedError::KeyError(format!("Invalid public key encoding: {}", e))
        })?;
        if pub_bytes.len() != 32 {
            return Err(SealedError::KeyError(
                "Invalid public key: expected 32 bytes".to_string(),
            ));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&pub_bytes);
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)?;
        self.verify_with_verifying_key(&verifying_key)
    }

    /// Verify against a known public key file.
    pub fn verify_with_key(&self, public_key_path: &Path) -> SealedResult<()> {
        let bytes = fs::read(public_key_path).map_err(|_| {
            SealedError::KeyError(format!("Failed to read public key: {}", public_key_path.display()))
        })?;
        if bytes.len() != 32 {
            return Err(SealedError::KeyError("Invalid public key file".to_string()));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        let verifying_key = VerifyingKey::from_bytes(&key_bytes)?;
        self.verify_with_verifying_key(&verifying_key)
    }

    /// Check signature against a verifying key.
    fn verify_with_verifying_key(&self, verifying_key: &VerifyingKey) -> SealedResult<()> {
        let sig_bytes = BASE64.decode(&self.signature).map_err(|e| {
            SealedError::KeyError(format!("Invalid signature encoding: {}", e))
        })?;
        if sig_bytes.len() != 64 {
            return Err(SealedError::KeyError(
                "Invalid signature: expected 64 bytes".to_string(),
            ));
        }
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_arr);

        verifying_key
            .verify(self.payload.as_bytes(), &signature)
            .map_err(|e| SealedError::VerificationFailed(format!("Signature invalid: {}", e)))
    }
}
