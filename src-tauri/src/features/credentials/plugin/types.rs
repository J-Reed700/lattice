//! Credentials plugin DTOs with TypeScript generation

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct StoreCredentialRequest {
    pub service: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ServiceRequest {
    pub service: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct EndpointRequest {
    pub endpoint: String,
}
