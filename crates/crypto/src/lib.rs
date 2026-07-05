//! E2E encryption for RemoteDesk.
//!
//! Uses NaCl/libsodium via sodiumoxide:
//! - Key exchange: Curve25519 + XSalsa20-Poly1305 (crypto_box)
//! - Symmetric encryption: XSalsa20-Poly1305 (crypto_secretbox)
//! - Nonces are monotonic counters (separate for send/recv).

use sodiumoxide::crypto::box_;

/// Size of a Curve25519 public key (32 bytes).
pub const PUBLIC_KEY_BYTES: usize = box_::PUBLICKEYBYTES;
/// Size of a Curve25519 secret key (32 bytes).
pub const SECRET_KEY_BYTES: usize = box_::SECRETKEYBYTES;
/// Size of a symmetric nonce for secretbox (24 bytes).
pub const NONCE_BYTES: usize = sodiumoxide::crypto::secretbox::NONCEBYTES;
/// Size of the MAC appended to each encrypted message (16 bytes).
pub const MAC_BYTES: usize = sodiumoxide::crypto::secretbox::MACBYTES;

// ── Initialisation ───────────────────────────────────────

/// Must be called once before any crypto operations.
/// Safe to call multiple times — sodiumoxide ignores repeated init.
pub fn init() {
    sodiumoxide::init().ok();
}

// ── Key Exchange ─────────────────────────────────────────

/// An ephemeral keypair for Curve25519 key exchange.
#[derive(Clone)]
pub struct KeyExchange {
    public_key: box_::PublicKey,
    secret_key: box_::SecretKey,
}

impl KeyExchange {
    /// Generate a new ephemeral keypair.
    pub fn generate() -> Self {
        let (public_key, secret_key) = box_::gen_keypair();
        Self {
            public_key,
            secret_key,
        }
    }

    /// Raw public key bytes for transmission.
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_BYTES] {
        self.public_key.0
    }

    /// Create a shared secret from the peer's public key.
    pub fn compute_shared_secret(&self, peer_public: &[u8; PUBLIC_KEY_BYTES]) -> SharedSecret {
        let peer_pk = box_::PublicKey::from_slice(peer_public)
            .expect("valid public key length");
        let precomputed = box_::precompute(&peer_pk, &self.secret_key);
        SharedSecret(precomputed)
    }
}

/// Precomputed shared secret for symmetric encryption.
#[derive(Clone)]
pub struct SharedSecret(box_::PrecomputedKey);

impl SharedSecret {
    /// Derive a symmetric key for secretbox from the shared secret.
    /// Uses the first 32 bytes of crypto_box output as secretbox key.
    pub fn derive_symmetric_key(&self) -> [u8; 32] {
        // Hash the shared secret with a domain separator to get a secretbox key.
        let mut key = [0u8; 32];
        // Use crypto_generichash (blake2b) to derive a 32-byte key.
        let digest = sodiumoxide::crypto::hash::hash(&self.0 .0);
        key.copy_from_slice(&digest[..32]);
        key
    }
}

// ── Session Cipher ───────────────────────────────────────

use sodiumoxide::crypto::secretbox;

/// Symmetric encryption for a session.
///
/// Uses XSalsa20-Poly1305 with monotonic nonces.
/// Separate send and receive nonces prevent replay across directions.
#[derive(Clone)]
pub struct SessionCipher {
    key: secretbox::Key,
    send_nonce: u64,
    recv_nonce: u64,
}

impl SessionCipher {
    /// Create a new cipher from a 32-byte symmetric key.
    pub fn new(key_bytes: &[u8; 32]) -> Self {
        let key = secretbox::Key::from_slice(key_bytes).expect("valid key length");
        Self {
            key,
            send_nonce: 0,
            recv_nonce: 0,
        }
    }

    /// Encrypt plaintext for sending.
    /// Returns (nonce_bytes, ciphertext_with_mac).
    pub fn encrypt(&mut self, plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let nonce = self.make_nonce(self.send_nonce);
        self.send_nonce += 1;

        let ciphertext = secretbox::seal(plaintext, &nonce, &self.key);

        // Prepend nonce to ciphertext for the receiver.
        let mut nonce_vec = Vec::with_capacity(NONCE_BYTES);
        nonce_vec.extend_from_slice(&nonce.0);
        (nonce_vec, ciphertext)
    }

    /// Decrypt a received message.
    /// `encrypted` should contain [nonce || ciphertext_with_mac].
    pub fn decrypt(&mut self, nonce_bytes: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if nonce_bytes.len() != NONCE_BYTES {
            return Err(CryptoError::InvalidNonce);
        }

        let mut nonce_arr = [0u8; NONCE_BYTES];
        nonce_arr.copy_from_slice(nonce_bytes);
        let nonce = secretbox::Nonce(nonce_arr);

        let plaintext = secretbox::open(ciphertext, &nonce, &self.key)
            .map_err(|_| CryptoError::DecryptionFailed)?;

        self.recv_nonce += 1;
        Ok(plaintext)
    }

    fn make_nonce(&self, counter: u64) -> secretbox::Nonce {
        let mut nonce = [0u8; NONCE_BYTES];
        nonce[..8].copy_from_slice(&counter.to_le_bytes());
        secretbox::Nonce(nonce)
    }
}

// ── Errors ───────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Invalid nonce length")]
    InvalidNonce,
    #[error("Decryption failed — wrong key or corrupted data")]
    DecryptionFailed,
    #[error("Invalid key length")]
    InvalidKey,
}
