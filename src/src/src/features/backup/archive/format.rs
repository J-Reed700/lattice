//! On-disk format of a `.lattice-backup` archive (format version 1).
//!
//! This file is the shared contract between the crypto, payload, and
//! orchestration halves of the archive feature. Change it only with the
//! design doc (`docs/design/2026-09-16-encrypted-backup-archive.md`).
//!
//! ```text
//! +----------------+-----------------+-------------------------+-----------------------+
//! | MAGIC (8 bytes)| header_len u32LE| header JSON (plaintext) | STREAM chunks ...     |
//! +----------------+-----------------+-------------------------+-----------------------+
//! ```
//!
//! Every ciphertext chunk authenticates `MAGIC || header_len || header JSON`
//! as associated data, so the plaintext header cannot be tampered with.
//! Each chunk is `[is_last: u8][ct_len: u32 LE][ciphertext]`; plaintext
//! chunks are `CHUNK_SIZE` bytes except the last, which is flagged. The
//! plaintext is a zstd-compressed tar whose first entry is `manifest.json`.

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use crate::shared::error::AppError;

/// File magic. The trailing byte is the format version.
pub const MAGIC: [u8; 8] = *b"LATTBKP\x01";
/// Format version carried in `MAGIC[7]` and in the header.
pub const FORMAT_VERSION: u8 = 1;
/// Plaintext bytes per STREAM chunk.
pub const CHUNK_SIZE: usize = 1024 * 1024;
/// Upper bound on the plaintext header, to stop a hostile file from making
/// the reader allocate arbitrarily.
pub const MAX_HEADER_LEN: u32 = 64 * 1024;
/// Archive file extension (no leading dot).
pub const ARCHIVE_EXTENSION: &str = "lattice-backup";
/// Subfolder created inside the user's chosen destination.
pub const ARCHIVE_SUBDIR: &str = "Lattice Backups";
/// Archive filename prefix; full name is
/// `lattice-backup-<YYYYMMDDTHHMMSSZ>.lattice-backup`.
pub const ARCHIVE_PREFIX: &str = "lattice-backup-";
/// Suffix used while the file is being written; renamed away on success.
pub const PART_SUFFIX: &str = ".part";
/// Name of the first tar entry.
pub const MANIFEST_ENTRY: &str = "manifest.json";
/// Tar entry path of the SQLite snapshot.
pub const DB_ENTRY: &str = "db/lattice.db";
/// Tar directory prefix of the content-addressed files library.
pub const FILES_ENTRY_PREFIX: &str = "files/";
/// Tar directory prefix of the vault markdown folder.
pub const VAULT_ENTRY_PREFIX: &str = "vault/";
/// Tar entry path of `settings.json`.
pub const SETTINGS_ENTRY: &str = "settings.json";

/// Cipher identifier written into the header.
pub const CIPHER_ID: &str = "chacha20poly1305-stream-be32";
/// Compression identifier written into the header.
pub const COMPRESSION_ID: &str = "zstd";

/// HKDF info strings. Never reuse one for a different purpose.
pub const INFO_FILE_KEY: &[u8] = b"lattice-backup-file-key-v1";
pub const INFO_RECOVERY_KEK: &[u8] = b"lattice-backup-recovery-kek-v1";
pub const INFO_KCV: &[u8] = b"lattice-backup-kcv-v1";
/// AEAD associated data for the two slot kinds.
pub const AAD_SLOT_PASSPHRASE: &[u8] = b"lattice-backup-slot-passphrase-v1";
pub const AAD_SLOT_RECOVERY: &[u8] = b"lattice-backup-slot-recovery-v1";
/// Length of the key check value in bytes.
pub const KCV_LEN: usize = 8;
/// STREAM (BE32) nonce prefix length for a 96-bit AEAD nonce.
pub const STREAM_NONCE_LEN: usize = 7;

/// Errors from the archive layer. Mapped to `AppError` at the boundary.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// Passphrase or recovery code did not unwrap the key.
    #[error("wrong passphrase or recovery code")]
    WrongSecret,
    /// The file is not a valid archive, was truncated, or failed authentication.
    #[error("backup archive is corrupt: {0}")]
    Corrupt(String),
    /// The archive was written by a newer format than this build understands.
    #[error("backup archive format version {0} is not supported by this version of Lattice")]
    UnsupportedVersion(u8),
    /// The file exists but a cloud sync client has not downloaded its contents.
    #[error("backup file has not been downloaded by {0}; open it in that app first, then retry")]
    NotHydrated(String),
    /// I/O failure.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// Database failure while snapshotting or validating.
    #[error("database error: {0}")]
    Database(String),
    /// A stub that has not been implemented yet.
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    /// Anything else.
    #[error("{0}")]
    Other(String),
}

impl From<ArchiveError> for AppError {
    fn from(err: ArchiveError) -> Self {
        match err {
            ArchiveError::WrongSecret => AppError::PermissionDenied(err.to_string()),
            ArchiveError::Corrupt(_) | ArchiveError::UnsupportedVersion(_) => {
                AppError::BackupCorrupted(err.to_string())
            }
            ArchiveError::NotHydrated(_) => AppError::BackupRestoreFailed(err.to_string()),
            ArchiveError::Io(e) => AppError::FileSystem(e.to_string()),
            ArchiveError::Database(m) => AppError::Database(m),
            ArchiveError::NotImplemented(_) | ArchiveError::Other(_) => {
                AppError::BackupCreationFailed(err.to_string())
            }
        }
    }
}

impl From<serde_json::Error> for ArchiveError {
    fn from(err: serde_json::Error) -> Self {
        ArchiveError::Corrupt(format!("invalid header or manifest json: {err}"))
    }
}

/// One wrapped copy of the master key. All byte fields are standard base64.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeySlot {
    /// Argon2id(passphrase) -> KEK -> ChaCha20-Poly1305 wrap of the master key.
    Passphrase {
        salt: String,
        m_cost_kib: u32,
        t_cost: u32,
        p_cost: u32,
        nonce: String,
        wrapped_key: String,
    },
    /// HKDF-SHA256(recovery entropy) -> KEK -> ChaCha20-Poly1305 wrap of the master key.
    Recovery {
        salt: String,
        nonce: String,
        wrapped_key: String,
    },
}

/// The static envelope: wrapped slots plus a key check value. Built once at
/// setup, persisted locally, and copied verbatim into every archive header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct KeyEnvelope {
    pub slots: Vec<KeySlot>,
    /// base64 of the first `KCV_LEN` bytes of HKDF(master, INFO_KCV).
    pub kcv: String,
}

impl KeyEnvelope {
    pub fn has_passphrase_slot(&self) -> bool {
        self.slots
            .iter()
            .any(|s| matches!(s, KeySlot::Passphrase { .. }))
    }

    pub fn has_recovery_slot(&self) -> bool {
        self.slots
            .iter()
            .any(|s| matches!(s, KeySlot::Recovery { .. }))
    }
}

/// Plaintext, authenticated header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveHeader {
    pub version: u8,
    /// RFC 3339 UTC.
    pub created_at: String,
    pub app_version: String,
    pub cipher: String,
    pub compression: String,
    pub chunk_size: u32,
    /// base64, 16 random bytes; the per-archive file key is
    /// HKDF(master, salt = file_salt, info = INFO_FILE_KEY).
    pub file_salt: String,
    /// base64, `STREAM_NONCE_LEN` random bytes.
    pub stream_nonce: String,
    #[serde(flatten)]
    pub envelope: KeyEnvelope,
}

/// Encrypted manifest, first tar entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Manifest {
    pub version: u8,
    pub created_at: String,
    pub app_version: String,
    /// Always `DB_ENTRY` in v1.
    pub db_entry: String,
    /// Tables whose rows were cleared from the snapshot before packing.
    pub excluded_tables: Vec<String>,
    /// Highest applied `_sqlx_migrations.version` in the snapshot.
    pub migration_version: Option<i64>,
    /// Original absolute path of the files library root on the source machine.
    pub files_root: Option<String>,
    pub files_count: u64,
    /// Original absolute path of the vault folder on the source machine.
    pub vault_root: Option<String>,
    pub vault_file_count: u64,
    pub settings_included: bool,
    /// Sum of entry sizes in bytes (uncompressed), for progress on restore.
    pub total_plaintext_bytes: u64,
}

/// Serialize `MAGIC || header_len || header JSON` to `w`. Returns the exact
/// bytes written, which the caller passes to the encryptor as associated data.
pub fn write_header<W: Write>(w: &mut W, header: &ArchiveHeader) -> Result<Vec<u8>, ArchiveError> {
    let json = serde_json::to_vec(header)?;
    let len = u32::try_from(json.len())
        .map_err(|_| ArchiveError::Other("header too large".to_string()))?;
    if len > MAX_HEADER_LEN {
        return Err(ArchiveError::Other("header too large".to_string()));
    }
    let mut aad = Vec::with_capacity(MAGIC.len() + 4 + json.len());
    aad.extend_from_slice(&MAGIC);
    aad.extend_from_slice(&len.to_le_bytes());
    aad.extend_from_slice(&json);
    w.write_all(&aad)?;
    Ok(aad)
}

/// Parse the header from the start of `r`. Returns the header and the exact
/// bytes consumed, which the caller passes to the decryptor as associated data.
pub fn read_header<R: Read>(r: &mut R) -> Result<(ArchiveHeader, Vec<u8>), ArchiveError> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)
        .map_err(|_| ArchiveError::Corrupt("file is too short to be a backup".to_string()))?;
    if magic.get(..7) != MAGIC.get(..7) {
        return Err(ArchiveError::Corrupt(
            "not a Lattice backup archive".to_string(),
        ));
    }
    let version = magic.get(7).copied().unwrap_or(0);
    if version != FORMAT_VERSION {
        return Err(ArchiveError::UnsupportedVersion(version));
    }
    let mut len_bytes = [0u8; 4];
    r.read_exact(&mut len_bytes)
        .map_err(|_| ArchiveError::Corrupt("truncated header length".to_string()))?;
    let len = u32::from_le_bytes(len_bytes);
    if len == 0 || len > MAX_HEADER_LEN {
        return Err(ArchiveError::Corrupt(format!(
            "implausible header length {len}"
        )));
    }
    let mut json = vec![0u8; len as usize];
    r.read_exact(&mut json)
        .map_err(|_| ArchiveError::Corrupt("truncated header".to_string()))?;
    let header: ArchiveHeader = serde_json::from_slice(&json)?;
    if header.version != FORMAT_VERSION {
        return Err(ArchiveError::UnsupportedVersion(header.version));
    }
    if header.chunk_size == 0 || header.chunk_size as usize > 16 * CHUNK_SIZE {
        return Err(ArchiveError::Corrupt(format!(
            "implausible chunk size {}",
            header.chunk_size
        )));
    }
    let mut aad = Vec::with_capacity(12 + json.len());
    aad.extend_from_slice(&magic);
    aad.extend_from_slice(&len_bytes);
    aad.extend_from_slice(&json);
    Ok((header, aad))
}

/// Timestamped archive filename for `now`.
pub fn archive_file_name(now: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "{}{}.{}",
        ARCHIVE_PREFIX,
        now.format("%Y%m%dT%H%M%SZ"),
        ARCHIVE_EXTENSION
    )
}

/// True when `name` looks like a file this app wrote (used by retention so we
/// only ever delete our own files in the destination folder).
pub fn is_archive_file_name(name: &str) -> bool {
    name.starts_with(ARCHIVE_PREFIX)
        && name.ends_with(&format!(".{ARCHIVE_EXTENSION}"))
        && !name.ends_with(PART_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> ArchiveHeader {
        ArchiveHeader {
            version: FORMAT_VERSION,
            created_at: "2026-09-16T00:00:00Z".to_string(),
            app_version: "1.0.0".to_string(),
            cipher: CIPHER_ID.to_string(),
            compression: COMPRESSION_ID.to_string(),
            chunk_size: CHUNK_SIZE as u32,
            file_salt: "AAAA".to_string(),
            stream_nonce: "BBBB".to_string(),
            envelope: KeyEnvelope {
                slots: vec![KeySlot::Recovery {
                    salt: "c".into(),
                    nonce: "n".into(),
                    wrapped_key: "w".into(),
                }],
                kcv: "k".to_string(),
            },
        }
    }

    #[test]
    fn header_round_trips_and_aad_matches_bytes() {
        let mut buf = Vec::new();
        let aad = write_header(&mut buf, &header()).unwrap_or_default();
        assert_eq!(aad, buf);
        let mut cursor = std::io::Cursor::new(buf.clone());
        let (parsed, aad2) = read_header(&mut cursor).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed, header());
        assert_eq!(aad2, buf);
        assert_eq!(cursor.position() as usize, buf.len());
    }

    #[test]
    fn rejects_wrong_magic_and_future_version() {
        let mut bad = std::io::Cursor::new(b"NOTABKP\x01\x00\x00\x00\x00".to_vec());
        assert!(matches!(
            read_header(&mut bad),
            Err(ArchiveError::Corrupt(_))
        ));
        let mut future = std::io::Cursor::new(b"LATTBKP\x02\x00\x00\x00\x00".to_vec());
        assert!(matches!(
            read_header(&mut future),
            Err(ArchiveError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn file_names() {
        let ts = chrono::DateTime::parse_from_rfc3339("2026-09-16T12:34:56Z")
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_default();
        let name = archive_file_name(ts);
        assert_eq!(name, "lattice-backup-20260916T123456Z.lattice-backup");
        assert!(is_archive_file_name(&name));
        assert!(!is_archive_file_name(&format!("{name}{PART_SUFFIX}")));
        assert!(!is_archive_file_name("notes.md"));
    }
}
