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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::repository::{AckResult, DeviceRecord, PullBatch, PushBatchResult};
    use crate::sync::types::{ConflictResolution, PushChange, SyncAction};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Repository {
        calls: Mutex<Vec<String>>,
        deny_device: bool,
        fail_push: bool,
    }
    impl Repository {
        fn record(&self, call: String) {
            self.calls.lock().unwrap().push(call);
        }
    }
    #[async_trait]
    impl SyncRepository for Repository {
        async fn upsert_device(
            &self,
            user: i64,
            device: Uuid,
            name: String,
        ) -> AppResult<DeviceRecord> {
            self.record(format!("register:{user}:{device}"));
            Ok(DeviceRecord {
                id: 1,
                user_id: user,
                device_uuid: device,
                device_name: name,
                created_at: chrono::Utc::now(),
                last_seen_at: chrono::Utc::now(),
            })
        }
        async fn get_device(&self, user: i64, device: Uuid) -> AppResult<DeviceRecord> {
            self.record(format!("device:{user}:{device}"));
            if self.deny_device {
                return Err(AppError::NotFound("unregistered".into()));
            }
            Ok(DeviceRecord {
                id: 1,
                user_id: user,
                device_uuid: device,
                device_name: "device".into(),
                created_at: chrono::Utc::now(),
                last_seen_at: chrono::Utc::now(),
            })
        }
        async fn pull_changes(
            &self,
            user: i64,
            device: Uuid,
            since: i64,
            limit: i64,
        ) -> AppResult<PullBatch> {
            self.record(format!("pull:{user}:{device}:{since}:{limit}"));
            Ok(PullBatch {
                changes: vec![],
                conflicts: vec![],
                next_seq: 17,
            })
        }
        async fn push_changes(
            &self,
            user: i64,
            device: Uuid,
            changes: Vec<PushChange>,
        ) -> AppResult<PushBatchResult> {
            self.record(format!("push:{user}:{device}"));
            if self.fail_push {
                return Err(AppError::Internal("write failed".into()));
            }
            Ok(PushBatchResult {
                accepted_paths: changes.into_iter().map(|c| c.path).collect(),
                conflicts: vec![],
                high_watermark_seq: 19,
            })
        }
        async fn ack_checkpoint(&self, user: i64, device: Uuid, seq: i64) -> AppResult<AckResult> {
            self.record(format!("ack:{user}:{device}:{seq}"));
            Ok(AckResult {
                last_acked_seq: seq,
            })
        }
        async fn resolve_conflict(
            &self,
            user: i64,
            id: i64,
            _: ConflictResolution,
            content: Option<String>,
        ) -> AppResult<()> {
            self.record(format!(
                "resolve:{user}:{id}:{}",
                content.unwrap_or_default()
            ));
            Ok(())
        }
        async fn get_status(&self, user: i64, device: Uuid) -> AppResult<SyncStatusResponse> {
            self.record(format!("status:{user}:{device}"));
            Ok(SyncStatusResponse {
                device_id: device,
                last_acked_seq: 3,
                pending_conflicts: 2,
                synced_documents: 4,
                server_high_watermark_seq: 19,
            })
        }
    }
    fn change(path: &str) -> PushChange {
        PushChange {
            client_op_id: Uuid::new_v4(),
            action: SyncAction::Update,
            path: path.into(),
            title: None,
            content: None,
            content_hash: None,
            base_version: None,
        }
    }

    #[tokio::test]
    async fn invalid_requests_never_reach_persistence() {
        let repo = Arc::new(Repository::default());
        let service = SyncServiceImpl::new(repo.clone());
        let device_id = Uuid::new_v4();
        assert!(matches!(
            service
                .register_device(
                    42,
                    RegisterDeviceRequest {
                        device_id,
                        device_name: " ".into()
                    }
                )
                .await,
            Err(AppError::Validation(_))
        ));
        for changes in [vec![], vec![change("valid"), change(" ")]] {
            assert!(matches!(
                service
                    .push_changes(42, PushChangesRequest { device_id, changes })
                    .await,
                Err(AppError::Validation(_))
            ));
        }
        for (conflict_id, content) in [
            (0, Some("content".into())),
            (1, None),
            (1, Some(" ".into())),
        ] {
            assert!(matches!(
                service
                    .resolve_conflict(
                        42,
                        ResolveConflictRequest {
                            device_id,
                            conflict_id,
                            resolution: ConflictResolution::ResolvedMerge,
                            merged_content: content
                        }
                    )
                    .await,
                Err(AppError::Validation(_))
            ));
        }
        assert!(repo.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn pagination_and_ack_are_bounded_and_tenant_scoped() {
        let repo = Arc::new(Repository::default());
        let service = SyncServiceImpl::new(repo.clone());
        let device_id = Uuid::new_v4();
        for (since_seq, limit, expected_since, expected_limit) in [
            (None, None, 0, 200),
            (Some(-1), Some(0), 0, 1),
            (Some(8), Some(i64::MAX), 8, 1000),
        ] {
            let result = service
                .pull_changes(
                    42,
                    PullChangesRequest {
                        device_id,
                        since_seq,
                        limit,
                    },
                )
                .await
                .unwrap();
            assert_eq!(result.next_seq, 17);
            assert_eq!(
                repo.calls.lock().unwrap().last().unwrap(),
                &format!("pull:42:{device_id}:{expected_since}:{expected_limit}")
            );
        }
        let ack = service
            .ack_checkpoint(
                42,
                AckRequest {
                    device_id,
                    ack_seq: -1,
                },
            )
            .await
            .unwrap();
        assert_eq!(ack.last_acked_seq, 0);
        assert_eq!(ack.device_id, device_id);
        assert_eq!(
            repo.calls.lock().unwrap().last().unwrap(),
            &format!("ack:42:{device_id}:0")
        );
    }

    #[tokio::test]
    async fn conflict_resolution_checks_device_before_mutation() {
        let device_id = Uuid::new_v4();
        for deny in [false, true] {
            let repo = Arc::new(Repository {
                deny_device: deny,
                ..Default::default()
            });
            let service = SyncServiceImpl::new(repo.clone());
            let result = service
                .resolve_conflict(
                    42,
                    ResolveConflictRequest {
                        device_id,
                        conflict_id: 1,
                        resolution: ConflictResolution::ResolvedMerge,
                        merged_content: Some("merged".into()),
                    },
                )
                .await;
            let calls = repo.calls.lock().unwrap();
            assert_eq!(calls[0], format!("device:42:{device_id}"));
            if deny {
                assert!(matches!(result, Err(AppError::NotFound(_))));
                assert_eq!(calls.len(), 1);
            } else {
                result.unwrap();
                assert_eq!(calls[1], "resolve:42:1:merged");
            }
        }
    }

    #[tokio::test]
    async fn responses_and_repository_failure_are_preserved() {
        let device_id = Uuid::new_v4();
        let repo = Arc::new(Repository::default());
        let service = SyncServiceImpl::new(repo.clone());
        let device = service
            .register_device(
                42,
                RegisterDeviceRequest {
                    device_id,
                    device_name: "Laptop".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(device.device_id, device_id);
        assert_eq!(device.device_name, "Laptop");
        let pushed = service
            .push_changes(
                42,
                PushChangesRequest {
                    device_id,
                    changes: vec![change("note.md")],
                },
            )
            .await
            .unwrap();
        assert_eq!(pushed.accepted_paths, vec!["note.md"]);
        assert_eq!(pushed.high_watermark_seq, 19);
        let status = service.get_status(42, device_id).await.unwrap();
        assert_eq!(status.pending_conflicts, 2);
        assert!(
            repo.calls
                .lock()
                .unwrap()
                .contains(&format!("status:42:{device_id}"))
        );
        let service = SyncServiceImpl::new(Arc::new(Repository {
            fail_push: true,
            ..Default::default()
        }));
        assert!(
            matches!(service.push_changes(42, PushChangesRequest { device_id, changes: vec![change("note.md")] }).await, Err(AppError::Internal(message)) if message == "write failed")
        );
    }
}
