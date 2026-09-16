use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::AppResult;
use crate::sync::types::{
    ChangeSummary, ConflictInfo, ConflictResolution, PushChange, SyncStatusResponse,
};

#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub id: i64,
    pub user_id: i64,
    pub device_uuid: Uuid,
    pub device_name: String,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PullBatch {
    pub changes: Vec<ChangeSummary>,
    pub conflicts: Vec<ConflictInfo>,
    pub next_seq: i64,
}

#[derive(Debug, Clone)]
pub struct PushBatchResult {
    pub accepted_paths: Vec<String>,
    pub conflicts: Vec<ConflictInfo>,
    pub high_watermark_seq: i64,
}

#[derive(Debug, Clone)]
pub struct AckResult {
    pub last_acked_seq: i64,
}

#[async_trait]
pub trait SyncRepository: Send + Sync {
    async fn upsert_device(
        &self,
        user_id: i64,
        device_id: Uuid,
        device_name: String,
    ) -> AppResult<DeviceRecord>;

    async fn get_device(&self, user_id: i64, device_id: Uuid) -> AppResult<DeviceRecord>;

    async fn pull_changes(
        &self,
        user_id: i64,
        device_id: Uuid,
        since_seq: i64,
        limit: i64,
    ) -> AppResult<PullBatch>;

    async fn push_changes(
        &self,
        user_id: i64,
        device_id: Uuid,
        changes: Vec<PushChange>,
    ) -> AppResult<PushBatchResult>;

    async fn ack_checkpoint(
        &self,
        user_id: i64,
        device_id: Uuid,
        ack_seq: i64,
    ) -> AppResult<AckResult>;

    async fn resolve_conflict(
        &self,
        user_id: i64,
        conflict_id: i64,
        resolution: ConflictResolution,
        merged_content: Option<String>,
    ) -> AppResult<()>;

    async fn get_status(&self, user_id: i64, device_id: Uuid) -> AppResult<SyncStatusResponse>;
}
