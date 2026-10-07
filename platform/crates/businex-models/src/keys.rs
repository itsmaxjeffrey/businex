//! Per-company provider keys, stored protected.
//!
//! Keys are sealed with AES-256-GCM before they touch the database. The nonce
//! is random per seal. Sealed material is bound to its tenant and provider
//! through authenticated additional data, so ciphertext cannot be moved
//! between companies or providers and still open. Display and serialization
//! never expose key material.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("key encryption failed")]
    Seal,
    #[error("key decryption failed")]
    Open,
    #[error("master key invalid")]
    MasterKey,
}

/// Deployment master key (32 raw bytes, base64 encoded in protected
/// configuration; never in source or logs).
pub struct MasterKey(Aes256Gcm);

fn binding_aad(company_id: &str, provider: &str) -> Vec<u8> {
    format!("businex:model-key:v1:{}:{}", company_id, provider).into_bytes()
}

impl MasterKey {
    pub fn from_base64(encoded: String) -> Result<MasterKey, KeyError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| KeyError::MasterKey)?;
        if bytes.len() != 32 {
            return Err(KeyError::MasterKey);
        }
        Ok(MasterKey(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(
            &bytes,
        ))))
    }

    /// Seal one key for one tenant and provider. Random nonce per call.
    pub fn seal(
        &self,
        company_id: String,
        provider: String,
        plaintext: String,
    ) -> Result<SealedKey, KeyError> {
        let mut nonce_bytes = [0u8; 12];
        let random = uuid::Uuid::new_v4();
        nonce_bytes.copy_from_slice(&random.as_bytes()[..12]);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let aad = binding_aad(&company_id, &provider);
        let ciphertext = self
            .0
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: &aad,
                },
            )
            .map_err(|_| KeyError::Seal)?;
        Ok(SealedKey {
            nonce: nonce_bytes.to_vec(),
            ciphertext,
        })
    }

    /// Open sealed material only for the tenant and provider it was sealed
    /// for. Moved ciphertext fails authentication and stays closed.
    pub fn open(
        &self,
        company_id: String,
        provider: String,
        sealed: &SealedKey,
    ) -> Result<String, KeyError> {
        if sealed.nonce.len() != 12 {
            return Err(KeyError::Open);
        }
        let nonce = Nonce::from_slice(&sealed.nonce);
        let aad = binding_aad(&company_id, &provider);
        let plain = self
            .0
            .decrypt(
                nonce,
                Payload {
                    msg: sealed.ciphertext.as_ref(),
                    aad: &aad,
                },
            )
            .map_err(|_| KeyError::Open)?;
        String::from_utf8(plain).map_err(|_| KeyError::Open)
    }
}

/// Sealed key material as stored in the database. Debug/Serialize expose
/// lengths only, never contents.
#[derive(Clone, Serialize, Deserialize)]
pub struct SealedKey {
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

impl std::fmt::Debug for SealedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedKey")
            .field("nonce_len", &self.nonce.len())
            .field("ciphertext_len", &self.ciphertext.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master() -> MasterKey {
        let encoded = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        MasterKey::from_base64(encoded).expect("master key")
    }

    #[test]
    fn seal_and_open_roundtrip() {
        let m = master();
        let sealed = m
            .seal("co-1".into(), "openai".into(), "test-key-material".into())
            .expect("seal");
        assert!(!sealed.ciphertext.is_empty());
        assert_eq!(
            m.open("co-1".into(), "openai".into(), &sealed).expect("open"),
            "test-key-material"
        );
    }

    #[test]
    fn ciphertext_does_not_contain_plaintext() {
        let m = master();
        let sealed = m
            .seal("co-1".into(), "openai".into(), "test-key-material".into())
            .expect("seal");
        let as_text = String::from_utf8_lossy(&sealed.ciphertext);
        assert!(!as_text.contains("test-key-material"));
    }

    #[test]
    fn debug_output_redacts_key_material() {
        let sealed = master()
            .seal("co-1".into(), "openai".into(), "test-key-material".into())
            .expect("seal");
        let printed = format!("{:?}", sealed);
        assert!(!printed.contains("test-key-material"));
    }

    #[test]
    fn wrong_master_key_cannot_open() {
        let sealed = master()
            .seal("co-1".into(), "openai".into(), "sk-test".into())
            .expect("seal");
        let other_encoded = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let other = MasterKey::from_base64(other_encoded).expect("other master");
        assert!(other.open("co-1".into(), "openai".into(), &sealed).is_err());
    }

    #[test]
    fn tenant_binding_prevents_key_moving_between_companies() {
        let m = master();
        let sealed = m
            .seal("co-1".into(), "openai".into(), "test-key-material".into())
            .expect("seal");
        assert!(
            m.open("co-2".into(), "openai".into(), &sealed).is_err(),
            "another tenant must not open moved ciphertext"
        );
    }

    #[test]
    fn provider_binding_prevents_key_moving_between_providers() {
        let m = master();
        let sealed = m
            .seal("co-1".into(), "openai".into(), "test-key-material".into())
            .expect("seal");
        assert!(
            m.open("co-1".into(), "anthropic".into(), &sealed).is_err(),
            "another provider binding must not open moved ciphertext"
        );
    }

    #[test]
    fn random_nonces_differ_per_seal() {
        let m = master();
        let a = m.seal("co-1".into(), "openai".into(), "sk".into()).expect("a");
        let b = m.seal("co-1".into(), "openai".into(), "sk".into()).expect("b");
        assert_ne!(a.nonce, b.nonce, "nonces must be random per seal");
        assert_ne!(a.ciphertext, b.ciphertext);
    }

    #[test]
    fn invalid_master_key_rejected() {
        assert!(MasterKey::from_base64("not-base64!".into()).is_err());
        let short = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);
        assert!(MasterKey::from_base64(short).is_err());
    }
}
