//! Key envelope and streaming AEAD for `.lattice-backup` archives.
//!
//! Two independent pieces live here.
//!
//! * The **key envelope**: the long-lived 256-bit master key, wrapped once per
//!   secret. The recovery slot wraps it under a key derived from a 24-word
//!   BIP-39 code; the optional passphrase slot wraps it under an Argon2id key.
//!   A key check value (KCV) lets a caller tell "wrong secret" apart from
//!   "corrupt file".
//! * The **streaming AEAD**: ChaCha20-Poly1305 in the STREAM (BE32)
//!   construction, framed as `[is_last u8][ct_len u32 LE][ciphertext]`, with
//!   the plaintext header as associated data on every chunk.
//!
//! See `docs/design/2026-09-16-encrypted-backup-archive.md`.

use argon2::{Algorithm, Argon2, Version};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use bip39::{Language, Mnemonic};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
use hkdf::Hkdf;
use sha2_v011::Sha256;
use zeroize::Zeroizing;

use super::format::{
    ArchiveError, KeyEnvelope, KeySlot, AAD_SLOT_PASSPHRASE, AAD_SLOT_RECOVERY, INFO_FILE_KEY,
    INFO_KCV, INFO_RECOVERY_KEK, KCV_LEN,
};

mod stream;

pub use stream::{DecryptingReader, EncryptingWriter};

/// Salt length for both the recovery HKDF salt and the Argon2id salt.
const SLOT_SALT_LEN: usize = 16;
/// ChaCha20-Poly1305 nonce length used when wrapping the master key.
const WRAP_NONCE_LEN: usize = 12;
/// Poly1305 tag length appended to every ciphertext.
const TAG_LEN: usize = 16;
/// Number of words in a recovery code (256 bits of entropy + checksum).
const RECOVERY_WORDS: usize = 24;

/// 256-bit long-lived backup master key. Zeroized on drop.
pub struct MasterKey(Zeroizing<[u8; 32]>);

impl MasterKey {
    pub fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; 32]);
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, bytes.as_mut());
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}

/// 24-word BIP-39 recovery code carrying 256 bits of entropy.
pub struct RecoveryCode {
    words: Vec<String>,
    entropy: Zeroizing<[u8; 32]>,
}

impl RecoveryCode {
    pub fn generate() -> Result<Self, ArchiveError> {
        let mut entropy = Zeroizing::new([0u8; 32]);
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, entropy.as_mut());
        let mnemonic = Mnemonic::from_entropy(entropy.as_ref())
            .map_err(|e| ArchiveError::Other(format!("could not build a recovery code: {e}")))?;
        Ok(Self {
            words: mnemonic.words().map(str::to_string).collect(),
            entropy,
        })
    }

    /// Parse user input: any whitespace/case, 24 valid English words with a
    /// valid checksum. Anything else is `WrongSecret`.
    pub fn parse(text: &str) -> Result<Self, ArchiveError> {
        let normalized = text
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" ");
        let mnemonic = Mnemonic::parse_in(Language::English, normalized.as_str())
            .map_err(|_| ArchiveError::WrongSecret)?;
        if mnemonic.word_count() != RECOVERY_WORDS {
            return Err(ArchiveError::WrongSecret);
        }
        let (buffer, len) = mnemonic.to_entropy_array();
        let bytes = buffer
            .get(..len)
            .filter(|b| b.len() == 32)
            .ok_or(ArchiveError::WrongSecret)?;
        let mut entropy = Zeroizing::new([0u8; 32]);
        entropy.as_mut().copy_from_slice(bytes);
        Ok(Self {
            words: mnemonic.words().map(str::to_string).collect(),
            entropy,
        })
    }

    /// True when `text` has the shape of a 24-word phrase (used to decide
    /// whether a restore secret is a recovery code or a passphrase).
    pub fn looks_like_phrase(text: &str) -> bool {
        text.split_whitespace().count() == 24
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    pub fn phrase(&self) -> String {
        self.words.join(" ")
    }

    pub fn entropy(&self) -> &[u8; 32] {
        &self.entropy
    }
}

impl std::fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryCode(<redacted>)")
    }
}

/// Argon2id cost parameters for the passphrase slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argon2Params {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for Argon2Params {
    /// Production: 64 MiB, t=3, p=1 (about one second on a laptop).
    fn default() -> Self {
        Self {
            m_cost_kib: 64 * 1024,
            t_cost: 3,
            p_cost: 1,
        }
    }
}

impl Argon2Params {
    /// Tiny parameters for unit tests only.
    pub fn for_tests() -> Self {
        Self {
            m_cost_kib: 64,
            t_cost: 1,
            p_cost: 1,
        }
    }
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// HKDF-SHA256 into `okm`. `salt` is `None` for the "no salt" variant.
fn hkdf_sha256(ikm: &[u8], salt: Option<&[u8]>, info: &[u8], okm: &mut [u8]) -> bool {
    Hkdf::<Sha256>::new(salt, ikm).expand(info, okm).is_ok()
}

/// A 256-bit key derived with HKDF-SHA256.
fn hkdf_key(
    ikm: &[u8],
    salt: Option<&[u8]>,
    info: &[u8],
) -> Result<Zeroizing<[u8; 32]>, ArchiveError> {
    let mut okm = Zeroizing::new([0u8; 32]);
    if !hkdf_sha256(ikm, salt, info, okm.as_mut()) {
        return Err(ArchiveError::Other("key derivation failed".to_string()));
    }
    Ok(okm)
}

/// Argon2id key encryption key for a passphrase slot.
fn argon2_key(
    passphrase: &str,
    salt: &[u8],
    params: Argon2Params,
) -> Result<Zeroizing<[u8; 32]>, ArchiveError> {
    let cost = argon2::Params::new(params.m_cost_kib, params.t_cost, params.p_cost, Some(32))
        .map_err(|e| ArchiveError::Other(format!("invalid argon2 parameters: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, cost);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|e| ArchiveError::Other(format!("argon2 failed: {e}")))?;
    Ok(key)
}

/// Wrap `plaintext` (the master key) under `kek`.
fn seal(
    kek: &[u8; 32],
    nonce: &[u8; WRAP_NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, ArchiveError> {
    let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*kek));
    cipher
        .encrypt(
            &chacha20poly1305::Nonce::from(*nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| ArchiveError::Other("could not wrap the backup key".to_string()))
}

/// Unwrap a 32-byte master key. Any failure is indistinguishable `WrongSecret`.
fn open(
    kek: &[u8; 32],
    nonce: &[u8; WRAP_NONCE_LEN],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<MasterKey, ArchiveError> {
    let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*kek));
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                &chacha20poly1305::Nonce::from(*nonce),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| ArchiveError::WrongSecret)?,
    );
    let mut bytes = [0u8; 32];
    let slice = plaintext
        .as_slice()
        .get(..32)
        .filter(|_| plaintext.len() == 32)
        .ok_or(ArchiveError::WrongSecret)?;
    bytes.copy_from_slice(slice);
    Ok(MasterKey::from_bytes(bytes))
}

/// Decode standard base64 into a fixed-size array.
fn decode_fixed<const N: usize>(text: &str) -> Option<[u8; N]> {
    let decoded = BASE64.decode(text).ok()?;
    if decoded.len() != N {
        return None;
    }
    let mut out = [0u8; N];
    out.copy_from_slice(decoded.as_slice());
    Some(out)
}

/// Constant-time byte comparison (the KCV is public, but habits are cheap).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// First `KCV_LEN` bytes of HKDF-SHA256(master, no salt, INFO_KCV).
fn kcv_bytes(master: &MasterKey) -> [u8; KCV_LEN] {
    let mut out = [0u8; KCV_LEN];
    // `KCV_LEN` is far below HKDF-SHA256's 255*32 byte ceiling, so this
    // cannot fail; a failure would leave an all-zero KCV, which only ever
    // causes a false "wrong secret".
    let _ = hkdf_sha256(master.as_bytes(), None, INFO_KCV, &mut out);
    out
}

// ---------------------------------------------------------------------------
// Key envelope
// ---------------------------------------------------------------------------

/// Build a fresh envelope with a mandatory recovery slot and an optional
/// passphrase slot.
pub fn build_envelope(
    master: &MasterKey,
    passphrase: Option<&str>,
    recovery: &RecoveryCode,
    params: Argon2Params,
) -> Result<KeyEnvelope, ArchiveError> {
    let mut envelope = KeyEnvelope::default();
    rotate_recovery_slot(&mut envelope, master, recovery)?;
    if let Some(passphrase) = passphrase {
        set_passphrase_slot(&mut envelope, master, passphrase, params)?;
    }
    Ok(envelope)
}

/// Replace (or add) the passphrase slot.
pub fn set_passphrase_slot(
    envelope: &mut KeyEnvelope,
    master: &MasterKey,
    passphrase: &str,
    params: Argon2Params,
) -> Result<(), ArchiveError> {
    let salt = random_bytes::<SLOT_SALT_LEN>();
    let nonce = random_bytes::<WRAP_NONCE_LEN>();
    let kek = argon2_key(passphrase, &salt, params)?;
    let wrapped = seal(&kek, &nonce, master.as_bytes(), AAD_SLOT_PASSPHRASE)?;
    remove_passphrase_slot(envelope);
    envelope.slots.push(KeySlot::Passphrase {
        salt: BASE64.encode(salt),
        m_cost_kib: params.m_cost_kib,
        t_cost: params.t_cost,
        p_cost: params.p_cost,
        nonce: BASE64.encode(nonce),
        wrapped_key: BASE64.encode(wrapped),
    });
    envelope.kcv = BASE64.encode(kcv_bytes(master));
    Ok(())
}

/// Remove the passphrase slot if present. The recovery slot always remains.
pub fn remove_passphrase_slot(envelope: &mut KeyEnvelope) {
    envelope
        .slots
        .retain(|s| !matches!(s, super::format::KeySlot::Passphrase { .. }));
}

/// Replace the recovery slot with one wrapped under `recovery`.
pub fn rotate_recovery_slot(
    envelope: &mut KeyEnvelope,
    master: &MasterKey,
    recovery: &RecoveryCode,
) -> Result<(), ArchiveError> {
    let salt = random_bytes::<SLOT_SALT_LEN>();
    let nonce = random_bytes::<WRAP_NONCE_LEN>();
    let kek = hkdf_key(recovery.entropy(), Some(&salt), INFO_RECOVERY_KEK)?;
    let wrapped = seal(&kek, &nonce, master.as_bytes(), AAD_SLOT_RECOVERY)?;
    envelope
        .slots
        .retain(|s| !matches!(s, KeySlot::Recovery { .. }));
    envelope.slots.push(KeySlot::Recovery {
        salt: BASE64.encode(salt),
        nonce: BASE64.encode(nonce),
        wrapped_key: BASE64.encode(wrapped),
    });
    envelope.kcv = BASE64.encode(kcv_bytes(master));
    Ok(())
}

/// Unwrap the master key with a parsed recovery code.
fn unwrap_with_recovery(
    envelope: &KeyEnvelope,
    recovery: &RecoveryCode,
) -> Result<MasterKey, ArchiveError> {
    let slot = envelope
        .slots
        .iter()
        .find_map(|s| match s {
            KeySlot::Recovery {
                salt,
                nonce,
                wrapped_key,
            } => Some((salt, nonce, wrapped_key)),
            KeySlot::Passphrase { .. } => None,
        })
        .ok_or(ArchiveError::WrongSecret)?;
    let (salt, nonce, wrapped) = slot;
    let salt = decode_fixed::<SLOT_SALT_LEN>(salt).ok_or(ArchiveError::WrongSecret)?;
    let nonce = decode_fixed::<WRAP_NONCE_LEN>(nonce).ok_or(ArchiveError::WrongSecret)?;
    let wrapped = BASE64
        .decode(wrapped)
        .map_err(|_| ArchiveError::WrongSecret)?;
    let kek = hkdf_key(recovery.entropy(), Some(&salt), INFO_RECOVERY_KEK)
        .map_err(|_| ArchiveError::WrongSecret)?;
    open(&kek, &nonce, wrapped.as_slice(), AAD_SLOT_RECOVERY)
}

/// Unwrap the master key with a passphrase.
fn unwrap_with_passphrase(
    envelope: &KeyEnvelope,
    passphrase: &str,
) -> Result<MasterKey, ArchiveError> {
    let slot = envelope
        .slots
        .iter()
        .find_map(|s| match s {
            KeySlot::Passphrase {
                salt,
                m_cost_kib,
                t_cost,
                p_cost,
                nonce,
                wrapped_key,
            } => Some((salt, *m_cost_kib, *t_cost, *p_cost, nonce, wrapped_key)),
            KeySlot::Recovery { .. } => None,
        })
        .ok_or(ArchiveError::WrongSecret)?;
    let (salt, m_cost_kib, t_cost, p_cost, nonce, wrapped) = slot;
    let salt = decode_fixed::<SLOT_SALT_LEN>(salt).ok_or(ArchiveError::WrongSecret)?;
    let nonce = decode_fixed::<WRAP_NONCE_LEN>(nonce).ok_or(ArchiveError::WrongSecret)?;
    let wrapped = BASE64
        .decode(wrapped)
        .map_err(|_| ArchiveError::WrongSecret)?;
    let kek = argon2_key(
        passphrase,
        &salt,
        Argon2Params {
            m_cost_kib,
            t_cost,
            p_cost,
        },
    )
    .map_err(|_| ArchiveError::WrongSecret)?;
    open(&kek, &nonce, wrapped.as_slice(), AAD_SLOT_PASSPHRASE)
}

/// Unwrap the master key with either a passphrase or a recovery phrase
/// (auto-detected). Returns `WrongSecret` on any failure to authenticate
/// or on a key check value mismatch.
pub fn unwrap_master_key(envelope: &KeyEnvelope, secret: &str) -> Result<MasterKey, ArchiveError> {
    let parsed = if RecoveryCode::looks_like_phrase(secret) {
        RecoveryCode::parse(secret).ok()
    } else {
        None
    };
    let master = match parsed {
        Some(recovery) => unwrap_with_recovery(envelope, &recovery),
        None => unwrap_with_passphrase(envelope, secret),
    }
    // Never leak which step failed.
    .map_err(|_| ArchiveError::WrongSecret)?;
    if !kcv_matches(envelope, &master) {
        return Err(ArchiveError::WrongSecret);
    }
    Ok(master)
}

/// True when `master` is the key this envelope wraps.
pub fn kcv_matches(envelope: &KeyEnvelope, master: &MasterKey) -> bool {
    match decode_fixed::<KCV_LEN>(&envelope.kcv) {
        Some(expected) => ct_eq(&expected, &kcv_bytes(master)),
        None => false,
    }
}

/// Per-archive file key: HKDF-SHA256(master, salt = file_salt, info = INFO_FILE_KEY).
pub fn derive_file_key(master: &MasterKey, file_salt: &[u8]) -> Zeroizing<[u8; 32]> {
    let mut key = Zeroizing::new([0u8; 32]);
    // 32 bytes is always a valid HKDF-SHA256 output length.
    let _ = hkdf_sha256(
        master.as_bytes(),
        Some(file_salt),
        INFO_FILE_KEY,
        key.as_mut(),
    );
    key
}

/// Random bytes helper for salts and nonces.
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut out);
    out
}

#[cfg(test)]
mod tests;
