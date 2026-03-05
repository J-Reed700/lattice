use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterDeviceRequest {
    pub device_id: Uuid,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceResponse {
    pub device_id: Uuid,
    pub device_name: String,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    Create,
    Update,
    Delete,
}

impl SyncAction {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "create" => Some(Self::Create),
            "update" => Some(Self::Update),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushChange {
    pub client_op_id: Uuid,
    pub action: SyncAction,
    pub path: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub content_hash: Option<String>,
    pub base_version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushChangesRequest {
    pub device_id: Uuid,
    pub changes: Vec<PushChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullChangesRequest {
    pub device_id: Uuid,
    pub since_seq: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSummary {
    pub seq: i64,
    pub client_op_id: Uuid,
    pub action: SyncAction,
    pub path: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub content_hash: Option<String>,
    pub base_version: Option<i64>,
    pub applied_version: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStatus {
    Pending,
    ResolvedLocal,
    ResolvedRemote,
    ResolvedMerge,
}

impl ConflictStatus {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "resolved_local" => Some(Self::ResolvedLocal),
            "resolved_remote" => Some(Self::ResolvedRemote),
            "resolved_merge" => Some(Self::ResolvedMerge),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub id: i64,
    pub path: String,
    pub server_version: Option<i64>,
    pub incoming_base_version: Option<i64>,
    pub status: ConflictStatus,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushChangesResponse {
    pub accepted_paths: Vec<String>,
    pub conflicts: Vec<ConflictInfo>,
    pub high_watermark_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullChangesResponse {
    pub changes: Vec<ChangeSummary>,
    pub conflicts: Vec<ConflictInfo>,
    pub next_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckRequest {
    pub device_id: Uuid,
    pub ack_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckResponse {
    pub device_id: Uuid,
    pub last_acked_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    ResolvedLocal,
    ResolvedRemote,
    ResolvedMerge,
}

impl ConflictResolution {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::ResolvedLocal => "resolved_local",
            Self::ResolvedRemote => "resolved_remote",
            Self::ResolvedMerge => "resolved_merge",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveConflictRequest {
    pub device_id: Uuid,
    pub conflict_id: i64,
    pub resolution: ConflictResolution,
    pub merged_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusResponse {
    pub device_id: Uuid,
    pub last_acked_seq: i64,
    pub pending_conflicts: i64,
    pub synced_documents: i64,
    pub server_high_watermark_seq: i64,
}
