//! A keyring held in memory, for tests that must never reach the real one.

use crate::application::ports::credentials_port::CredentialsPort;
use crate::shared::error::AppError;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct InMemoryCredentials {
    keys: parking_lot::Mutex<HashMap<String, String>>,
    refuse_writes: AtomicBool,
}

impl InMemoryCredentials {
    /// A keyring that fails every write, as a locked or missing one does.
    pub fn refusing_writes() -> Self {
        Self {
            refuse_writes: AtomicBool::new(true),
            ..Self::default()
        }
    }

    pub fn stored(&self, service: &str) -> Option<String> {
        self.keys.lock().get(service).cloned()
    }
}

#[async_trait]
impl CredentialsPort for InMemoryCredentials {
    async fn set_api_key(&self, service: &str, key: &str) -> Result<(), AppError> {
        if self.refuse_writes.load(Ordering::SeqCst) {
            return Err(AppError::Security("keyring locked".into()));
        }
        self.keys
            .lock()
            .insert(service.to_string(), key.to_string());
        Ok(())
    }
    async fn get_api_key(&self, service: &str) -> Result<Option<String>, AppError> {
        Ok(self.stored(service))
    }
    async fn delete_api_key(&self, service: &str) -> Result<(), AppError> {
        self.keys.lock().remove(service);
        Ok(())
    }
    async fn has_api_key(&self, service: &str) -> Result<bool, AppError> {
        Ok(self.keys.lock().contains_key(service))
    }
    async fn clear_all_credentials(&self) -> Result<(), AppError> {
        self.keys.lock().clear();
        Ok(())
    }
    async fn set_custom_endpoint(&self, _endpoint: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_custom_endpoint(&self) -> Result<Option<String>, AppError> {
        Ok(None)
    }
}
