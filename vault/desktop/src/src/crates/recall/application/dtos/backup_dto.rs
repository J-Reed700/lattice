use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfoDto {
    pub path: String,
    pub name: String,
    pub created_at: String,
    pub version: String,
    pub file_count: usize,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateBackupResultDto {
    pub backup_path: String,
    pub size: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupResultDto {
    pub success: bool,
    pub restored_count: usize,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListBackupsResultDto {
    pub backups: Vec<BackupInfoDto>,
}
