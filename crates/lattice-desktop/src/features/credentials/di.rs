//! Credentials feature dependency injection.

use std::path::PathBuf;
use std::sync::Arc;

use crate::features::credentials::adapter::CredentialsAdapter;
use crate::features::credentials::use_cases::SetApiKeyUseCase;
use crate::interfaces::di::Container;
use lattice_core::application::ports::CredentialsPort;

#[derive(Clone)]
pub struct CredentialsDi {
    pub credentials: Arc<dyn CredentialsPort>,
    pub set_api_key_use_case: Arc<SetApiKeyUseCase>,
}

pub fn build(credentials_path: PathBuf) -> CredentialsDi {
    let credentials =
        Arc::new(CredentialsAdapter::new(credentials_path)) as Arc<dyn CredentialsPort>;

    CredentialsDi {
        set_api_key_use_case: Arc::new(SetApiKeyUseCase::new(credentials.clone())),
        credentials,
    }
}

/// Credentials' registrar surface on `Container`.
impl Container {
    // Credentials (from CoreModule)
    pub fn set_api_key_use_case(&self) -> Arc<SetApiKeyUseCase> {
        Arc::clone(self.core.set_api_key_use_case())
    }

    /// The keyring every credential is read from and written to.
    pub fn credentials(&self) -> Arc<dyn CredentialsPort> {
        Arc::clone(self.core.credentials())
    }
}
