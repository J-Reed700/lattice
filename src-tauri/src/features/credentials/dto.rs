use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialOperationResultDto {
    pub success: bool,
    pub message: Option<String>,
}
