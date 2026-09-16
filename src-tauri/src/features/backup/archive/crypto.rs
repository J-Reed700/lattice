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

use std::io::{Read, Write};

use aead_stream::{DecryptorBE32, EncryptorBE32};
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
    INFO_KCV, INFO_RECOVERY_KEK, KCV_LEN, STREAM_NONCE_LEN,
};

/// Salt length for both the recovery HKDF salt and the Argon2id salt.
const SLOT_SALT_LEN: usize = 16;
/// ChaCha20-Poly1305 nonce length used when wrapping the master key.
const WRAP_NONCE_LEN: usize = 12;
/// Poly1305 tag length appended to every ciphertext.
const TAG_LEN: usize = 16;
/// Number of words in a recovery code (256 bits of entropy + checksum).
const RECOVERY_WORDS: usize = 24;

/// STREAM-BE32 nonce prefix as the AEAD stack wants it.
type StreamNoncePrefix = chacha20poly1305::aead::array::Array<u8, chacha20poly1305::consts::U7>;

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

// ---------------------------------------------------------------------------
// Streaming AEAD
// ---------------------------------------------------------------------------

fn invalid_data(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

fn truncated() -> std::io::Error {
    invalid_data("backup archive is truncated")
}

fn auth_failed() -> std::io::Error {
    invalid_data("backup archive failed authentication")
}

fn malformed_frame() -> std::io::Error {
    invalid_data("backup archive is corrupt: malformed chunk framing")
}

fn data_after_last() -> std::io::Error {
    invalid_data("backup archive is corrupt: data after the final chunk")
}

fn stream_nonce_prefix(bytes: &[u8; STREAM_NONCE_LEN]) -> StreamNoncePrefix {
    StreamNoncePrefix::from(*bytes)
}

fn write_frame<W: Write>(writer: &mut W, is_last: bool, ciphertext: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(ciphertext.len())
        .map_err(|_| std::io::Error::other("backup archive chunk is too large to frame"))?;
    writer.write_all(&[u8::from(is_last)])?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(ciphertext)
}

/// `Write` adapter that buffers plaintext into `chunk_size` blocks and emits
/// STREAM-BE32 ciphertext chunks framed as `[is_last u8][ct_len u32 LE][ct]`.
/// `finish` must be called to flush the final (flagged) chunk.
///
/// Dropping without calling `finish` does not panic; it simply leaves the
/// stream without its flagged final chunk, which readers reject as truncated.
pub struct EncryptingWriter<W: Write> {
    inner: Option<W>,
    encryptor: Option<EncryptorBE32<ChaCha20Poly1305>>,
    aad: Vec<u8>,
    chunk_size: usize,
    pending: Vec<u8>,
}

impl<W: Write> EncryptingWriter<W> {
    pub fn new(
        inner: W,
        file_key: &[u8; 32],
        stream_nonce: &[u8; STREAM_NONCE_LEN],
        aad: &[u8],
        chunk_size: usize,
    ) -> Result<Self, ArchiveError> {
        if chunk_size == 0 {
            return Err(ArchiveError::Other(
                "chunk size must be positive".to_string(),
            ));
        }
        let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*file_key));
        let encryptor = EncryptorBE32::from_aead(cipher, &stream_nonce_prefix(stream_nonce));
        Ok(Self {
            inner: Some(inner),
            encryptor: Some(encryptor),
            aad: aad.to_vec(),
            chunk_size,
            pending: Vec::with_capacity(chunk_size),
        })
    }

    /// Encrypt one non-final chunk in place and frame it.
    fn seal_next(&mut self, mut buffer: Vec<u8>) -> std::io::Result<()> {
        let encryptor = self
            .encryptor
            .as_mut()
            .ok_or_else(|| std::io::Error::other("backup archive writer already finished"))?;
        encryptor
            .encrypt_next_in_place(self.aad.as_slice(), &mut buffer)
            .map_err(|_| std::io::Error::other("backup archive encryption failed"))?;
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| std::io::Error::other("backup archive writer already finished"))?;
        write_frame(inner, false, buffer.as_slice())
    }

    /// Encrypt and write the last chunk, flush, and hand back the inner writer.
    pub fn finish(mut self) -> Result<W, ArchiveError> {
        let mut inner = self
            .inner
            .take()
            .ok_or_else(|| ArchiveError::Other("writer already finished".to_string()))?;
        let encryptor = self
            .encryptor
            .take()
            .ok_or_else(|| ArchiveError::Other("writer already finished".to_string()))?;
        let mut buffer = std::mem::take(&mut self.pending);
        encryptor
            .encrypt_last_in_place(self.aad.as_slice(), &mut buffer)
            .map_err(|_| ArchiveError::Other("backup archive encryption failed".to_string()))?;
        write_frame(&mut inner, true, buffer.as_slice())?;
        inner.flush()?;
        Ok(inner)
    }
}

impl<W: Write> Write for EncryptingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.encryptor.is_none() || self.inner.is_none() {
            return Err(std::io::Error::other(
                "backup archive writer already finished",
            ));
        }
        self.pending.extend_from_slice(buf);
        while self.pending.len() >= self.chunk_size {
            let rest = self.pending.split_off(self.chunk_size);
            let chunk = std::mem::replace(&mut self.pending, rest);
            self.seal_next(chunk)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Buffered plaintext is not flushed: a partial chunk can only be
        // emitted as the flagged final chunk, which `finish` does.
        match self.inner.as_mut() {
            Some(inner) => inner.flush(),
            None => Ok(()),
        }
    }
}

/// `Read` adapter that reads framed STREAM chunks, authenticates each, and
/// yields plaintext. Truncation (EOF before the flagged last chunk),
/// reordering, and tampering all surface as `io::ErrorKind::InvalidData`.
pub struct DecryptingReader<R: Read> {
    inner: R,
    decryptor: Option<DecryptorBE32<ChaCha20Poly1305>>,
    aad: Vec<u8>,
    max_ciphertext_len: usize,
    plaintext: Vec<u8>,
    position: usize,
    finished: bool,
    /// Set once a chunk fails; every later `read` repeats the failure so a
    /// caller that ignores one error cannot go on to see a short stream as a
    /// complete one.
    poison: Option<&'static str>,
}

impl<R: Read> DecryptingReader<R> {
    pub fn new(
        inner: R,
        file_key: &[u8; 32],
        stream_nonce: &[u8; STREAM_NONCE_LEN],
        aad: &[u8],
        chunk_size: usize,
    ) -> Result<Self, ArchiveError> {
        if chunk_size == 0 {
            return Err(ArchiveError::Other(
                "chunk size must be positive".to_string(),
            ));
        }
        let cipher = ChaCha20Poly1305::new(&chacha20poly1305::Key::from(*file_key));
        let decryptor = DecryptorBE32::from_aead(cipher, &stream_nonce_prefix(stream_nonce));
        Ok(Self {
            inner,
            decryptor: Some(decryptor),
            aad: aad.to_vec(),
            max_ciphertext_len: chunk_size.saturating_add(TAG_LEN),
            plaintext: Vec::new(),
            position: 0,
            finished: false,
            poison: None,
        })
    }

    /// Read, authenticate, and decrypt the next framed chunk.
    fn fill_next_chunk(&mut self) -> std::io::Result<()> {
        let flag = read_optional_byte(&mut self.inner)?.ok_or_else(truncated)?;
        let is_last = match flag {
            0 => false,
            1 => true,
            _ => return Err(malformed_frame()),
        };
        let mut len_bytes = [0u8; 4];
        read_exact_or_truncated(&mut self.inner, &mut len_bytes)?;
        let ciphertext_len = u32::from_le_bytes(len_bytes) as usize;
        if ciphertext_len < TAG_LEN || ciphertext_len > self.max_ciphertext_len {
            return Err(malformed_frame());
        }
        let mut buffer = vec![0u8; ciphertext_len];
        read_exact_or_truncated(&mut self.inner, buffer.as_mut_slice())?;

        if is_last {
            let decryptor = self.decryptor.take().ok_or_else(data_after_last)?;
            decryptor
                .decrypt_last_in_place(self.aad.as_slice(), &mut buffer)
                .map_err(|_| auth_failed())?;
            self.finished = true;
        } else {
            let decryptor = self.decryptor.as_mut().ok_or_else(data_after_last)?;
            decryptor
                .decrypt_next_in_place(self.aad.as_slice(), &mut buffer)
                .map_err(|_| auth_failed())?;
        }

        if self.finished && read_optional_byte(&mut self.inner)?.is_some() {
            return Err(data_after_last());
        }

        self.plaintext = buffer;
        self.position = 0;
        Ok(())
    }
}

impl<R: Read> Read for DecryptingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(message) = self.poison {
            return Err(invalid_data(message));
        }
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            let available = self.plaintext.len().saturating_sub(self.position);
            if available > 0 {
                let take = available.min(buf.len());
                let end = self.position.saturating_add(take);
                let source = self
                    .plaintext
                    .get(self.position..end)
                    .ok_or_else(|| std::io::Error::other("backup archive reader lost its place"))?;
                let target = buf
                    .get_mut(..take)
                    .ok_or_else(|| std::io::Error::other("backup archive reader lost its place"))?;
                target.copy_from_slice(source);
                self.position = end;
                return Ok(take);
            }
            if self.finished {
                return Ok(0);
            }
            if let Err(e) = self.fill_next_chunk() {
                self.poison = Some("backup archive could not be authenticated");
                return Err(e);
            }
        }
    }
}

/// Read a single byte, distinguishing a clean EOF from an error.
fn read_optional_byte<R: Read>(reader: &mut R) -> std::io::Result<Option<u8>> {
    let mut byte = [0u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => return Ok(None),
            Ok(_) => return Ok(byte.first().copied()),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// `read_exact`, but a short read means the archive is truncated.
fn read_exact_or_truncated<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<()> {
    reader.read_exact(buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            truncated()
        } else {
            e
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: usize = 1024;

    fn params() -> Argon2Params {
        Argon2Params::for_tests()
    }

    fn code() -> RecoveryCode {
        RecoveryCode::generate().unwrap_or_else(|e| panic!("generate: {e}"))
    }

    fn envelope_with(passphrase: Option<&str>) -> (MasterKey, RecoveryCode, KeyEnvelope) {
        let master = MasterKey::generate();
        let recovery = code();
        let envelope = build_envelope(&master, passphrase, &recovery, params())
            .unwrap_or_else(|e| panic!("build_envelope: {e}"));
        (master, recovery, envelope)
    }

    /// A syntactically valid 24-word phrase whose checksum word is wrong.
    fn phrase_with_bad_checksum() -> String {
        for _ in 0..64 {
            let good = code();
            let other = code();
            let mut words = good.words().to_vec();
            let last_index = words.len().saturating_sub(1);
            if let (Some(slot), Some(replacement)) =
                (words.get_mut(last_index), other.words().last())
            {
                if slot == replacement {
                    continue;
                }
                *slot = replacement.clone();
            }
            let candidate = words.join(" ");
            if matches!(
                RecoveryCode::parse(&candidate),
                Err(ArchiveError::WrongSecret)
            ) {
                return candidate;
            }
        }
        panic!("could not construct a bad-checksum phrase");
    }

    // -- recovery code -----------------------------------------------------

    #[test]
    fn generate_produces_24_words_and_round_trips() {
        let generated = code();
        assert_eq!(generated.words().len(), 24);
        assert!(RecoveryCode::looks_like_phrase(&generated.phrase()));
        let parsed =
            RecoveryCode::parse(&generated.phrase()).unwrap_or_else(|e| panic!("parse: {e}"));
        assert_eq!(parsed.entropy(), generated.entropy());
        assert_eq!(parsed.words(), generated.words());
    }

    #[test]
    fn parse_normalizes_case_and_whitespace() {
        let generated = code();
        let messy = format!(
            "  {}  ",
            generated
                .words()
                .iter()
                .map(|w| w.to_uppercase())
                .collect::<Vec<_>>()
                .join("\n \t ")
        );
        let parsed = RecoveryCode::parse(&messy).unwrap_or_else(|e| panic!("parse messy: {e}"));
        assert_eq!(parsed.entropy(), generated.entropy());
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(matches!(
            RecoveryCode::parse("not a recovery code"),
            Err(ArchiveError::WrongSecret)
        ));
        assert!(matches!(
            RecoveryCode::parse(&phrase_with_bad_checksum()),
            Err(ArchiveError::WrongSecret)
        ));
        let short = code()
            .words()
            .get(..12)
            .map(|w| w.join(" "))
            .unwrap_or_default();
        assert!(matches!(
            RecoveryCode::parse(&short),
            Err(ArchiveError::WrongSecret)
        ));
    }

    // -- envelope ----------------------------------------------------------

    #[test]
    fn envelope_round_trips_with_passphrase_and_with_recovery_phrase() {
        let (master, recovery, envelope) = envelope_with(Some("correct horse battery"));
        assert!(envelope.has_recovery_slot());
        assert!(envelope.has_passphrase_slot());

        let by_passphrase = unwrap_master_key(&envelope, "correct horse battery")
            .unwrap_or_else(|e| panic!("passphrase unwrap: {e}"));
        assert_eq!(by_passphrase.as_bytes(), master.as_bytes());

        let by_phrase = unwrap_master_key(&envelope, &recovery.phrase())
            .unwrap_or_else(|e| panic!("phrase unwrap: {e}"));
        assert_eq!(by_phrase.as_bytes(), master.as_bytes());
    }

    #[test]
    fn recovery_only_envelope_round_trips() {
        let (master, recovery, envelope) = envelope_with(None);
        assert!(!envelope.has_passphrase_slot());
        let unwrapped = unwrap_master_key(&envelope, &recovery.phrase())
            .unwrap_or_else(|e| panic!("phrase unwrap: {e}"));
        assert_eq!(unwrapped.as_bytes(), master.as_bytes());
        // No passphrase slot: a passphrase-shaped secret is WrongSecret.
        assert!(matches!(
            unwrap_master_key(&envelope, "anything"),
            Err(ArchiveError::WrongSecret)
        ));
    }

    #[test]
    fn wrong_secrets_all_report_wrong_secret() {
        let (_master, _recovery, envelope) = envelope_with(Some("hunter2hunter2"));

        assert!(matches!(
            unwrap_master_key(&envelope, "hunter2hunter3"),
            Err(ArchiveError::WrongSecret)
        ));
        assert!(matches!(
            unwrap_master_key(&envelope, &code().phrase()),
            Err(ArchiveError::WrongSecret)
        ));
        assert!(matches!(
            unwrap_master_key(&envelope, &phrase_with_bad_checksum()),
            Err(ArchiveError::WrongSecret)
        ));
        assert!(matches!(
            unwrap_master_key(&envelope, ""),
            Err(ArchiveError::WrongSecret)
        ));
    }

    #[test]
    fn kcv_matches_only_the_wrapped_key() {
        let (master, _recovery, envelope) = envelope_with(None);
        assert!(kcv_matches(&envelope, &master));
        assert!(!kcv_matches(&envelope, &MasterKey::generate()));

        let mut broken = envelope.clone();
        broken.kcv = "!!!not base64!!!".to_string();
        assert!(!kcv_matches(&broken, &master));

        let mut empty = envelope;
        empty.kcv = String::new();
        assert!(!kcv_matches(&empty, &master));
    }

    #[test]
    fn kcv_mismatch_is_wrong_secret_even_when_the_slot_opens() {
        let (_master, recovery, mut envelope) = envelope_with(None);
        envelope.kcv = BASE64.encode([0u8; KCV_LEN]);
        assert!(matches!(
            unwrap_master_key(&envelope, &recovery.phrase()),
            Err(ArchiveError::WrongSecret)
        ));
    }

    #[test]
    fn rotate_recovery_slot_invalidates_the_old_phrase() {
        let (master, old, mut envelope) = envelope_with(Some("keep me"));
        let new = code();
        rotate_recovery_slot(&mut envelope, &master, &new)
            .unwrap_or_else(|e| panic!("rotate: {e}"));

        assert_eq!(
            envelope
                .slots
                .iter()
                .filter(|s| matches!(s, KeySlot::Recovery { .. }))
                .count(),
            1
        );
        assert!(matches!(
            unwrap_master_key(&envelope, &old.phrase()),
            Err(ArchiveError::WrongSecret)
        ));
        let unwrapped = unwrap_master_key(&envelope, &new.phrase())
            .unwrap_or_else(|e| panic!("new phrase: {e}"));
        assert_eq!(unwrapped.as_bytes(), master.as_bytes());
        // The passphrase slot survives rotation.
        let by_passphrase =
            unwrap_master_key(&envelope, "keep me").unwrap_or_else(|e| panic!("passphrase: {e}"));
        assert_eq!(by_passphrase.as_bytes(), master.as_bytes());
    }

    #[test]
    fn set_and_remove_passphrase_slot() {
        let (master, recovery, mut envelope) = envelope_with(None);
        assert!(!envelope.has_passphrase_slot());

        set_passphrase_slot(&mut envelope, &master, "first pass", params())
            .unwrap_or_else(|e| panic!("set: {e}"));
        assert!(envelope.has_passphrase_slot());
        assert_eq!(
            unwrap_master_key(&envelope, "first pass")
                .unwrap_or_else(|e| panic!("first: {e}"))
                .as_bytes(),
            master.as_bytes()
        );

        // Replacing, not appending.
        set_passphrase_slot(&mut envelope, &master, "second pass", params())
            .unwrap_or_else(|e| panic!("replace: {e}"));
        assert_eq!(
            envelope
                .slots
                .iter()
                .filter(|s| matches!(s, KeySlot::Passphrase { .. }))
                .count(),
            1
        );
        assert!(matches!(
            unwrap_master_key(&envelope, "first pass"),
            Err(ArchiveError::WrongSecret)
        ));
        assert_eq!(
            unwrap_master_key(&envelope, "second pass")
                .unwrap_or_else(|e| panic!("second: {e}"))
                .as_bytes(),
            master.as_bytes()
        );

        remove_passphrase_slot(&mut envelope);
        assert!(!envelope.has_passphrase_slot());
        assert!(envelope.has_recovery_slot());
        assert!(matches!(
            unwrap_master_key(&envelope, "second pass"),
            Err(ArchiveError::WrongSecret)
        ));
        // The recovery slot still works after the passphrase slot is gone.
        assert_eq!(
            unwrap_master_key(&envelope, &recovery.phrase())
                .unwrap_or_else(|e| panic!("recovery: {e}"))
                .as_bytes(),
            master.as_bytes()
        );
    }

    #[test]
    fn passphrase_slot_records_its_cost_parameters() {
        let (master, _recovery, envelope) = envelope_with(Some("pw"));
        let slot = envelope
            .slots
            .iter()
            .find(|s| matches!(s, KeySlot::Passphrase { .. }))
            .cloned();
        match slot {
            Some(KeySlot::Passphrase {
                salt,
                m_cost_kib,
                t_cost,
                p_cost,
                nonce,
                wrapped_key,
            }) => {
                assert_eq!(m_cost_kib, params().m_cost_kib);
                assert_eq!(t_cost, params().t_cost);
                assert_eq!(p_cost, params().p_cost);
                assert_eq!(
                    decode_fixed::<SLOT_SALT_LEN>(&salt).map(|s| s.len()),
                    Some(16)
                );
                assert_eq!(
                    decode_fixed::<WRAP_NONCE_LEN>(&nonce).map(|n| n.len()),
                    Some(12)
                );
                assert_eq!(
                    BASE64.decode(&wrapped_key).map(|w| w.len()).unwrap_or(0),
                    32 + TAG_LEN
                );
            }
            _ => panic!("expected a passphrase slot"),
        }
        assert!(kcv_matches(&envelope, &master));
    }

    #[test]
    fn derive_file_key_differs_per_salt_and_per_master() {
        let master = MasterKey::generate();
        let salt_a = random_bytes::<16>();
        let salt_b = random_bytes::<16>();
        let a = derive_file_key(&master, &salt_a);
        let b = derive_file_key(&master, &salt_b);
        let a_again = derive_file_key(&master, &salt_a);
        assert_ne!(a.as_ref(), b.as_ref());
        assert_eq!(a.as_ref(), a_again.as_ref());
        assert_ne!(a.as_ref(), &[0u8; 32]);

        let other = derive_file_key(&MasterKey::generate(), &salt_a);
        assert_ne!(a.as_ref(), other.as_ref());
    }

    // -- streaming ---------------------------------------------------------

    fn encrypt_all(plain: &[u8], key: &[u8; 32], nonce: &[u8; 7], aad: &[u8]) -> Vec<u8> {
        let mut writer = EncryptingWriter::new(Vec::new(), key, nonce, aad, CHUNK)
            .unwrap_or_else(|e| panic!("writer: {e}"));
        writer
            .write_all(plain)
            .unwrap_or_else(|e| panic!("write: {e}"));
        writer.finish().unwrap_or_else(|e| panic!("finish: {e}"))
    }

    fn decrypt_all(
        ciphertext: &[u8],
        key: &[u8; 32],
        nonce: &[u8; 7],
        aad: &[u8],
    ) -> std::io::Result<Vec<u8>> {
        let mut reader = DecryptingReader::new(
            std::io::Cursor::new(ciphertext.to_vec()),
            key,
            nonce,
            aad,
            CHUNK,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut out = Vec::new();
        reader.read_to_end(&mut out)?;
        Ok(out)
    }

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn stream_round_trips_at_every_chunk_boundary() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"MAGIC|header".as_slice();
        for len in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK, CHUNK * 7 / 2] {
            let plain = sample(len);
            let ciphertext = encrypt_all(&plain, &key, &nonce, aad);
            let decrypted = decrypt_all(&ciphertext, &key, &nonce, aad)
                .unwrap_or_else(|e| panic!("len {len}: {e}"));
            assert_eq!(decrypted, plain, "round trip failed for {len} bytes");
        }
    }

    #[test]
    fn exact_multiple_of_chunk_size_still_emits_a_flagged_last_chunk() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"aad".as_slice();
        let ciphertext = encrypt_all(&sample(2 * CHUNK), &key, &nonce, aad);
        // two full frames plus an empty flagged frame (flag + len + tag).
        let full_frame = 1 + 4 + CHUNK + TAG_LEN;
        let empty_last = 1 + 4 + TAG_LEN;
        assert_eq!(ciphertext.len(), 2 * full_frame + empty_last);
        assert_eq!(ciphertext.get(2 * full_frame).copied(), Some(1u8));
    }

    #[test]
    fn writes_in_small_pieces_match_one_big_write() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"aad".as_slice();
        let plain = sample(CHUNK * 7 / 2);

        let one_shot = encrypt_all(&plain, &key, &nonce, aad);

        let mut writer = EncryptingWriter::new(Vec::new(), &key, &nonce, aad, CHUNK)
            .unwrap_or_else(|e| panic!("writer: {e}"));
        for piece in plain.chunks(37) {
            writer
                .write_all(piece)
                .unwrap_or_else(|e| panic!("write: {e}"));
        }
        let piecemeal = writer.finish().unwrap_or_else(|e| panic!("finish: {e}"));
        assert_eq!(one_shot, piecemeal);
    }

    #[test]
    fn read_to_end_and_tiny_reads_agree() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"aad".as_slice();
        let plain = sample(CHUNK * 7 / 2 + 13);
        let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

        let via_read_to_end = decrypt_all(&ciphertext, &key, &nonce, aad)
            .unwrap_or_else(|e| panic!("read_to_end: {e}"));

        let mut reader =
            DecryptingReader::new(std::io::Cursor::new(ciphertext), &key, &nonce, aad, CHUNK)
                .unwrap_or_else(|e| panic!("reader: {e}"));
        let mut via_small = Vec::new();
        let mut scratch = [0u8; 7];
        loop {
            let n = reader
                .read(&mut scratch)
                .unwrap_or_else(|e| panic!("small read: {e}"));
            if n == 0 {
                break;
            }
            via_small.extend_from_slice(scratch.get(..n).unwrap_or_default());
        }

        assert_eq!(via_read_to_end, plain);
        assert_eq!(via_small, plain);
    }

    #[test]
    fn empty_read_buffer_is_not_end_of_stream() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"aad".as_slice();
        let ciphertext = encrypt_all(&sample(64), &key, &nonce, aad);
        let mut reader =
            DecryptingReader::new(std::io::Cursor::new(ciphertext), &key, &nonce, aad, CHUNK)
                .unwrap_or_else(|e| panic!("reader: {e}"));
        assert_eq!(reader.read(&mut []).unwrap_or(9), 0);
        let mut out = Vec::new();
        reader
            .read_to_end(&mut out)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(out.len(), 64);
    }

    fn assert_invalid_data(result: std::io::Result<Vec<u8>>, what: &str) {
        match result {
            Ok(_) => panic!("{what} should not have decrypted"),
            Err(e) => assert_eq!(
                e.kind(),
                std::io::ErrorKind::InvalidData,
                "{what}: unexpected error {e}"
            ),
        }
    }

    #[test]
    fn truncation_is_rejected() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"aad".as_slice();
        let plain = sample(2 * CHUNK);
        let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

        // Drop the whole final (empty, flagged) chunk.
        let empty_last = 1 + 4 + TAG_LEN;
        let without_last = ciphertext
            .get(..ciphertext.len() - empty_last)
            .unwrap_or_default()
            .to_vec();
        assert_invalid_data(
            decrypt_all(&without_last, &key, &nonce, aad),
            "dropping the last chunk",
        );

        // Cut in the middle of a chunk.
        let mid = ciphertext
            .get(..ciphertext.len() / 2)
            .unwrap_or_default()
            .to_vec();
        assert_invalid_data(decrypt_all(&mid, &key, &nonce, aad), "cutting mid-chunk");

        // Lose a single trailing byte.
        let one_short = ciphertext
            .get(..ciphertext.len() - 1)
            .unwrap_or_default()
            .to_vec();
        assert_invalid_data(decrypt_all(&one_short, &key, &nonce, aad), "one byte short");

        // Nothing at all.
        assert_invalid_data(decrypt_all(&[], &key, &nonce, aad), "empty input");
    }

    #[test]
    fn tampering_is_rejected() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"aad".as_slice();
        let plain = sample(2 * CHUNK);
        let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

        // Flip one ciphertext byte.
        let mut flipped = ciphertext.clone();
        if let Some(byte) = flipped.get_mut(100) {
            *byte ^= 0x01;
        }
        assert_invalid_data(decrypt_all(&flipped, &key, &nonce, aad), "a flipped byte");

        // Swap the two full chunks (same size, so the framing stays valid).
        let full_frame = 1 + 4 + CHUNK + TAG_LEN;
        let mut swapped = Vec::with_capacity(ciphertext.len());
        swapped.extend_from_slice(
            ciphertext
                .get(full_frame..2 * full_frame)
                .unwrap_or_default(),
        );
        swapped.extend_from_slice(ciphertext.get(..full_frame).unwrap_or_default());
        swapped.extend_from_slice(ciphertext.get(2 * full_frame..).unwrap_or_default());
        assert_eq!(swapped.len(), ciphertext.len());
        assert_invalid_data(decrypt_all(&swapped, &key, &nonce, aad), "swapped chunks");

        // Extra data after the flagged last chunk.
        let mut extended = ciphertext.clone();
        extended.extend_from_slice(&[0u8; 8]);
        assert_invalid_data(decrypt_all(&extended, &key, &nonce, aad), "trailing data");

        // A bogus is_last flag byte.
        let mut bad_flag = ciphertext.clone();
        if let Some(byte) = bad_flag.first_mut() {
            *byte = 7;
        }
        assert_invalid_data(decrypt_all(&bad_flag, &key, &nonce, aad), "a bogus flag");

        // An implausible chunk length.
        let mut bad_len = ciphertext.clone();
        for (offset, byte) in u32::MAX.to_le_bytes().iter().enumerate() {
            if let Some(slot) = bad_len.get_mut(1 + offset) {
                *slot = *byte;
            }
        }
        assert_invalid_data(decrypt_all(&bad_len, &key, &nonce, aad), "a bogus length");
    }

    #[test]
    fn a_failed_chunk_poisons_every_later_read() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"aad".as_slice();
        let ciphertext = encrypt_all(&sample(2 * CHUNK), &key, &nonce, aad);
        let empty_last = 1 + 4 + TAG_LEN;
        let truncated_bytes = ciphertext
            .get(..ciphertext.len() - empty_last)
            .unwrap_or_default()
            .to_vec();

        let mut reader = DecryptingReader::new(
            std::io::Cursor::new(truncated_bytes),
            &key,
            &nonce,
            aad,
            CHUNK,
        )
        .unwrap_or_else(|e| panic!("reader: {e}"));

        let mut scratch = vec![0u8; 4096];
        let mut saw_error = false;
        for _ in 0..8 {
            match reader.read(&mut scratch) {
                Ok(0) => panic!("a truncated stream must never read as complete"),
                Ok(_) => {}
                Err(e) => {
                    assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
                    saw_error = true;
                    break;
                }
            }
        }
        assert!(saw_error, "expected the truncated stream to fail");
        // Every later read repeats the failure instead of reporting EOF.
        for _ in 0..3 {
            match reader.read(&mut scratch) {
                Ok(n) => panic!("poisoned reader returned {n} bytes"),
                Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidData),
            }
        }
    }

    #[test]
    fn wrong_key_nonce_or_aad_is_rejected() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let aad = b"MAGIC|header-v1".as_slice();
        let plain = sample(CHUNK + 5);
        let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

        assert_invalid_data(
            decrypt_all(&ciphertext, &random_bytes::<32>(), &nonce, aad),
            "a wrong key",
        );
        assert_invalid_data(
            decrypt_all(&ciphertext, &key, &random_bytes::<7>(), aad),
            "a wrong stream nonce",
        );
        assert_invalid_data(
            decrypt_all(&ciphertext, &key, &nonce, b"MAGIC|header-v2"),
            "a wrong aad",
        );
        assert_invalid_data(decrypt_all(&ciphertext, &key, &nonce, b""), "an empty aad");
    }

    #[test]
    fn dropping_the_writer_without_finishing_does_not_panic() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        let mut writer = EncryptingWriter::new(Vec::new(), &key, &nonce, b"aad", CHUNK)
            .unwrap_or_else(|e| panic!("writer: {e}"));
        writer
            .write_all(&sample(CHUNK + 3))
            .unwrap_or_else(|e| panic!("write: {e}"));
        drop(writer);
    }

    #[test]
    fn zero_chunk_size_is_rejected() {
        let key = random_bytes::<32>();
        let nonce = random_bytes::<7>();
        assert!(EncryptingWriter::new(Vec::new(), &key, &nonce, b"", 0).is_err());
        assert!(
            DecryptingReader::new(std::io::Cursor::new(Vec::new()), &key, &nonce, b"", 0).is_err()
        );
    }

    #[test]
    fn end_to_end_envelope_and_stream() {
        let master = MasterKey::generate();
        let recovery = code();
        let envelope = build_envelope(&master, Some("pass phrase"), &recovery, params())
            .unwrap_or_else(|e| panic!("envelope: {e}"));

        let file_salt = random_bytes::<16>();
        let stream_nonce = random_bytes::<7>();
        let aad = b"LATTBKP\x01....header".as_slice();
        let plain = sample(CHUNK * 3 + 17);

        let file_key = derive_file_key(&master, &file_salt);
        let ciphertext = encrypt_all(&plain, &file_key, &stream_nonce, aad);

        // A fresh process only has the envelope and the user's secret.
        let recovered = unwrap_master_key(&envelope, &recovery.phrase())
            .unwrap_or_else(|e| panic!("unwrap: {e}"));
        let recovered_key = derive_file_key(&recovered, &file_salt);
        assert_eq!(recovered_key.as_ref(), file_key.as_ref());

        let decrypted = decrypt_all(&ciphertext, &recovered_key, &stream_nonce, aad)
            .unwrap_or_else(|e| panic!("decrypt: {e}"));
        assert_eq!(decrypted, plain);
    }
}
