//! Where the long-lived backup master key lives on this device.
//!
//! The OS keyring is the primary home (macOS Keychain, Windows Credential
//! Manager, Linux Secret Service). Headless Linux boxes, locked keyrings and
//! CI containers do not have one, and a backup feature that refuses to run
//! there is a backup feature that silently protects nobody — so there is a
//! `0600` file fallback at `<app_data_dir>/backup/master.key`.
//!
//! Losing this key is survivable: every archive header carries the wrapped
//! key, so the passphrase or the 24-word recovery code can always get it back.

use std::path::{Path, PathBuf};
use std::sync::Once;

use base64::prelude::{Engine as _, BASE64_STANDARD};
use zeroize::Zeroize;

use super::crypto::MasterKey;
use super::format::ArchiveError;

/// Keyring service name. Distinct from the credentials adapter's service so
/// "forget my API keys" can never take the backup key with it.
pub const KEYRING_SERVICE: &str = "tech.lattice.app";
/// Keyring account/user name.
pub const KEYRING_USER: &str = "backup-master-key";
/// File fallback name under `<app_data_dir>/backup/`.
pub const KEY_FILE_NAME: &str = "master.key";

static FALLBACK_WARNED: Once = Once::new();

fn warn_fallback_once(reason: &str) {
    FALLBACK_WARNED.call_once(|| {
        tracing::warn!(
            reason,
            "OS keyring unavailable; the backup master key will be stored in a 0600 file \
             under the app data directory instead"
        );
    });
}

/// Loads, stores and clears the backup master key. Deliberately not a trait:
/// there is exactly one implementation and the fallback is part of it.
pub struct MasterKeyStore {
    service: String,
    app_data_dir: PathBuf,
    /// Tests force the file fallback by clearing this.
    use_keyring: bool,
}

impl MasterKeyStore {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            service: KEYRING_SERVICE.to_string(),
            app_data_dir,
            use_keyring: true,
        }
    }

    /// Escape hatch for tests: a bogus service name, or the keyring disabled
    /// entirely, exercises the file fallback without touching the user's
    /// real keychain.
    pub fn with_service(service: String, app_data_dir: PathBuf, use_keyring: bool) -> Self {
        Self {
            service,
            app_data_dir,
            use_keyring,
        }
    }

    /// `<app_data_dir>/backup/master.key`.
    pub fn fallback_path(&self) -> PathBuf {
        self.app_data_dir
            .join(super::config::BACKUP_STATE_DIR)
            .join(KEY_FILE_NAME)
    }

    /// Keyring first, then the file. `Ok(None)` means this device has no key.
    pub fn load(&self) -> Result<Option<MasterKey>, ArchiveError> {
        if self.use_keyring {
            match self.load_from_keyring() {
                Ok(Some(key)) => return Ok(Some(key)),
                Ok(None) => {}
                Err(e) => warn_fallback_once(&e),
            }
        }
        self.load_from_file()
    }

    /// Write to the keyring, falling back to the file on any keyring error.
    pub fn store(&self, key: &MasterKey) -> Result<(), ArchiveError> {
        let mut encoded = BASE64_STANDARD.encode(key.as_bytes());
        let result = if self.use_keyring {
            match self.store_in_keyring(&encoded) {
                Ok(()) => Ok(()),
                Err(e) => {
                    warn_fallback_once(&e);
                    self.store_in_file(&encoded)
                }
            }
        } else {
            self.store_in_file(&encoded)
        };
        encoded.zeroize();
        result
    }

    /// Remove the key from both homes. Missing entries are not an error.
    pub fn clear(&self) -> Result<(), ArchiveError> {
        if self.use_keyring {
            if let Ok(entry) = keyring::Entry::new(&self.service, KEYRING_USER) {
                match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(e) => warn_fallback_once(&e.to_string()),
                }
            }
        }
        match std::fs::remove_file(self.fallback_path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ArchiveError::Io(e)),
        }
    }

    fn load_from_keyring(&self) -> Result<Option<MasterKey>, String> {
        let entry = keyring::Entry::new(&self.service, KEYRING_USER).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(encoded) => decode_key(&encoded).map(Some).map_err(|e| e.to_string()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn store_in_keyring(&self, encoded: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(&self.service, KEYRING_USER).map_err(|e| e.to_string())?;
        entry.set_password(encoded).map_err(|e| e.to_string())
    }

    fn load_from_file(&self) -> Result<Option<MasterKey>, ArchiveError> {
        let path = self.fallback_path();
        // repository-barrier-allow: the key file itself is the resource.
        let encoded = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(ArchiveError::Io(e)),
        };
        decode_key(encoded.trim()).map(Some)
    }

    fn store_in_file(&self, encoded: &str) -> Result<(), ArchiveError> {
        let path = self.fallback_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, encoded.as_bytes())?;
        restrict_permissions(&path)?;
        Ok(())
    }
}

/// Owner read/write only. A backup key readable by every process on a shared
/// box is not a backup key.
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), ArchiveError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), ArchiveError> {
    // Windows inherits the app data directory's ACL, which is already
    // per-user. There is no portable mode bit to set.
    Ok(())
}

fn decode_key(encoded: &str) -> Result<MasterKey, ArchiveError> {
    let mut bytes = BASE64_STANDARD
        .decode(encoded.trim())
        .map_err(|e| ArchiveError::Other(format!("stored backup key is not valid base64: {e}")))?;
    let len = bytes.len();
    let array: [u8; 32] = match bytes.as_slice().try_into() {
        Ok(array) => array,
        Err(_) => {
            bytes.zeroize();
            return Err(ArchiveError::Other(format!(
                "stored backup key is {len} bytes, expected 32"
            )));
        }
    };
    bytes.zeroize();
    Ok(MasterKey::from_bytes(array))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_only_store(dir: &Path) -> MasterKeyStore {
        MasterKeyStore::with_service(
            "tech.lattice.test.invalid".to_string(),
            dir.to_path_buf(),
            false,
        )
    }

    #[test]
    fn file_fallback_round_trips_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_only_store(dir.path());

        assert!(store.load().unwrap().is_none());

        let key = MasterKey::from_bytes([7u8; 32]);
        store.store(&key).unwrap();

        assert!(store.fallback_path().exists());
        let loaded = store.load().unwrap().expect("key should be present");
        assert_eq!(loaded.as_bytes(), key.as_bytes());
    }

    #[test]
    fn file_fallback_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_only_store(dir.path());
        store.store(&MasterKey::from_bytes([1u8; 32])).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(store.fallback_path())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "master.key must be 0600");
        }
    }

    #[test]
    fn clear_removes_the_file_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_only_store(dir.path());
        store.store(&MasterKey::from_bytes([2u8; 32])).unwrap();

        store.clear().unwrap();
        assert!(!store.fallback_path().exists());
        assert!(store.load().unwrap().is_none());
        store.clear().unwrap();
    }

    #[test]
    fn a_truncated_key_file_is_reported_not_silently_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_only_store(dir.path());
        std::fs::create_dir_all(store.fallback_path().parent().unwrap()).unwrap();
        std::fs::write(store.fallback_path(), BASE64_STANDARD.encode([9u8; 16])).unwrap();

        let err = store.load().unwrap_err().to_string();
        assert!(err.contains("expected 32"), "unexpected error: {err}");
    }

    #[test]
    fn a_non_base64_key_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let store = file_only_store(dir.path());
        std::fs::create_dir_all(store.fallback_path().parent().unwrap()).unwrap();
        std::fs::write(store.fallback_path(), "not base64 at all!!!").unwrap();

        assert!(store.load().is_err());
    }
}
