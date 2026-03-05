use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::sync::repository::SyncRepository;
use crate::sync::types::{
    AckRequest, AckResponse, DeviceResponse, PullChangesRequest, PullChangesResponse,
    PushChangesRequest, PushChangesResponse, RegisterDeviceRequest, ResolveConflictRequest,
    SyncStatusResponse,
};

#[async_trait]
pub trait SyncService: Send + Sync {
    async fn register_device(
        &self,
        user_id: i64,
        request: RegisterDeviceRequest,
    ) -> AppResult<DeviceResponse>;

    async fn push_changes(
        &self,
        user_id: i64,
        request: PushChangesRequest,
    ) -> AppResult<PushChangesResponse>;

    async fn pull_changes(
        &self,
        user_id: i64,
        request: PullChangesRequest,
    ) -> AppResult<PullChangesResponse>;

    async fn ack_checkpoint(&self, user_id: i64, request: AckRequest) -> AppResult<AckResponse>;

    async fn resolve_conflict(
        &self,
        user_id: i64,
        request: ResolveConflictRequest,
    ) -> AppResult<()>;

    async fn get_status(&self, user_id: i64, device_id: Uuid) -> AppResult<SyncStatusResponse>;
}

pub struct SyncServiceImpl<R: SyncRepository> {
    repository: Arc<R>,
}

impl<R: SyncRepository> SyncServiceImpl<R> {
    pub fn new(repository: Arc<R>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl<R: SyncRepository> SyncService for SyncServiceImpl<R> {
    async fn register_device(
        &self,
        user_id: i64,
        request: RegisterDeviceRequest,
    ) -> AppResult<DeviceResponse> {
        if request.device_name.trim().is_empty() {
            return Err(AppError::Validation(
                "device_name cannot be blank".to_string(),
            ));
        }

        let device = self
            .repository
            .upsert_device(user_id, request.device_id, request.device_name)
            .await?;

        Ok(DeviceResponse {
            device_id: device.device_uuid,
            device_name: device.device_name,
            last_seen_at: device.last_seen_at,
            created_at: device.created_at,
        })
    }

    async fn push_changes(
        &self,
        user_id: i64,
        request: PushChangesRequest,
    ) -> AppResult<PushChangesResponse> {
        if request.changes.is_empty() {
            return Err(AppError::Validation(
                "push request must contain at least one change".to_string(),
            ));
        }

        for change in &request.changes {
            if change.path.trim().is_empty() {
                return Err(AppError::Validation(
                    "all change paths must be non-empty".to_string(),
                ));
            }
        }

        let result = self
            .repository
            .push_changes(user_id, request.device_id, request.changes)
            .await?;

        Ok(PushChangesResponse {
            accepted_paths: result.accepted_paths,
            conflicts: result.conflicts,
            high_watermark_seq: result.high_watermark_seq,
        })
    }

    async fn pull_changes(
        &self,
        user_id: i64,
        request: PullChangesRequest,
    ) -> AppResult<PullChangesResponse> {
        let since_seq = request.since_seq.unwrap_or(0).max(0);
        let requested_limit = request.limit.unwrap_or(200);
        let limit = requested_limit.clamp(1, 1000);

        let batch = self
            .repository
            .pull_changes(user_id, request.device_id, since_seq, limit)
            .await?;

        Ok(PullChangesResponse {
            changes: batch.changes,
            conflicts: batch.conflicts,
            next_seq: batch.next_seq,
        })
    }

    async fn ack_checkpoint(&self, user_id: i64, request: AckRequest) -> AppResult<AckResponse> {
        let ack_seq = request.ack_seq.max(0);
        let ack = self
            .repository
            .ack_checkpoint(user_id, request.device_id, ack_seq)
            .await?;

        Ok(AckResponse {
            device_id: request.device_id,
            last_acked_seq: ack.last_acked_seq,
        })
    }

    async fn resolve_conflict(
        &self,
        user_id: i64,
        request: ResolveConflictRequest,
    ) -> AppResult<()> {
        if request.conflict_id <= 0 {
            return Err(AppError::Validation(
                "conflict_id must be positive".to_string(),
            ));
        }
        if matches!(
            request.resolution,
            crate::sync::types::ConflictResolution::ResolvedMerge
        ) && request
            .merged_content
            .as_ref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(AppError::Validation(
                "merged_content is required for resolved_merge".to_string(),
            ));
        }

        let _ = self
            .repository
            .get_device(user_id, request.device_id)
            .await?;

        self.repository
            .resolve_conflict(
                user_id,
                request.conflict_id,
                request.resolution,
                request.merged_content,
            )
            .await
    }

    async fn get_status(&self, user_id: i64, device_id: Uuid) -> AppResult<SyncStatusResponse> {
        self.repository.get_status(user_id, device_id).await
    }
}
