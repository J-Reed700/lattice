//! Credentials feature dependency injection.

use std::path::PathBuf;
use std::sync::Arc;

use crate::application::ports::CredentialsPort;
use crate::features::credentials::adapter::CredentialsAdapter;
use crate::features::credentials::use_cases::{
    DeleteApiKeyUseCase, GetApiKeyUseCase, SetApiKeyUseCase, SetCustomEndpointUseCase,
};
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct CredentialsDi {
    pub credentials: Arc<dyn CredentialsPort>,
    pub set_api_key_use_case: Arc<SetApiKeyUseCase>,
    pub get_api_key_use_case: Arc<GetApiKeyUseCase>,
    pub delete_api_key_use_case: Arc<DeleteApiKeyUseCase>,
    pub set_custom_endpoint_use_case: Arc<SetCustomEndpointUseCase>,
}

pub fn build(credentials_path: PathBuf) -> CredentialsDi {
    let credentials =
        Arc::new(CredentialsAdapter::new(credentials_path)) as Arc<dyn CredentialsPort>;

    CredentialsDi {
        set_api_key_use_case: Arc::new(SetApiKeyUseCase::new(credentials.clone())),
        get_api_key_use_case: Arc::new(GetApiKeyUseCase::new(credentials.clone())),
        delete_api_key_use_case: Arc::new(DeleteApiKeyUseCase::new(credentials.clone())),
        set_custom_endpoint_use_case: Arc::new(SetCustomEndpointUseCase::new(credentials.clone())),
        credentials,
    }
}

/// Credentials' registrar surface on `Container`.
impl Container {
    // Credentials (from CoreModule)
    pub fn set_api_key_use_case(&self) -> Arc<SetApiKeyUseCase> {
        Arc::clone(self.core.set_api_key_use_case())
    }
}
