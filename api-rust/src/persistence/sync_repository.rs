use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::sync::repository::{
    AckResult, DeviceRecord, PullBatch, PushBatchResult, SyncRepository,
};
use crate::sync::types::{
    ChangeSummary, ConflictInfo, ConflictResolution, ConflictStatus, PushChange, SyncAction,
    SyncStatusResponse,
};

#[derive(Debug, Clone)]
pub struct PgSyncRepository {
    pool: PgPool,
}

impl PgSyncRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn get_device_tx(
        tx: &mut Transaction<'_, Postgres>,
        user_id: i64,
        device_id: Uuid,
    ) -> AppResult<DeviceRecord> {
        sqlx::query_as::<_, DeviceRecord>(
            r#"
            SELECT
              id,
              user_id,
              device_uuid,
              device_name,
              last_seen_at,
              created_at
            FROM devices
            WHERE user_id = $1 AND device_uuid = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(tx.as_mut())
        .await?
        .ok_or_else(|| AppError::NotFound(format!("device not registered: {device_id}")))
    }

    async fn high_watermark_tx(tx: &mut Transaction<'_, Postgres>, user_id: i64) -> AppResult<i64> {
        let max_seq = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT MAX(seq)
            FROM document_ops
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(tx.as_mut())
        .await?
        .unwrap_or(0);
        Ok(max_seq)
    }
}

#[derive(sqlx::FromRow)]
struct ChangeRow {
    seq: i64,
    client_op_id: Uuid,
    action: String,
    path: String,
    title: Option<String>,
    content: Option<String>,
    content_hash: Option<String>,
    base_version: Option<i64>,
    applied_version: Option<i64>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ConflictRow {
    id: i64,
    path: String,
    server_version: Option<i64>,
    incoming_base_version: Option<i64>,
    status: String,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct HeadVersionRow {
    version: i64,
}

#[async_trait]
impl SyncRepository for PgSyncRepository {
    async fn upsert_device(
        &self,
        user_id: i64,
        device_id: Uuid,
        device_name: String,
    ) -> AppResult<DeviceRecord> {
        let record = sqlx::query_as::<_, DeviceRecord>(
            r#"
            INSERT INTO devices (user_id, device_uuid, device_name, last_seen_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (user_id, device_uuid)
            DO UPDATE SET
              device_name = EXCLUDED.device_name,
              last_seen_at = now()
            RETURNING
              id,
              user_id,
              device_uuid,
              device_name,
              last_seen_at,
              created_at
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(device_name)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    async fn get_device(&self, user_id: i64, device_id: Uuid) -> AppResult<DeviceRecord> {
        sqlx::query_as::<_, DeviceRecord>(
            r#"
            SELECT
              id,
              user_id,
              device_uuid,
              device_name,
              last_seen_at,
              created_at
            FROM devices
            WHERE user_id = $1 AND device_uuid = $2
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("device not registered: {device_id}")))
    }

    async fn pull_changes(
        &self,
        user_id: i64,
        device_id: Uuid,
        since_seq: i64,
        limit: i64,
    ) -> AppResult<PullBatch> {
        let device = self.get_device(user_id, device_id).await?;

        let rows = sqlx::query_as::<_, ChangeRow>(
            r#"
            SELECT
              seq,
              client_op_id,
              action::text AS action,
              path,
              title,
              content,
              content_hash,
              base_version,
              applied_version,
              created_at
            FROM document_ops
            WHERE user_id = $1
              AND seq > $2
              AND device_id <> $3
            ORDER BY seq ASC
            LIMIT $4
            "#,
        )
        .bind(user_id)
        .bind(since_seq)
        .bind(device.id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let changes = rows
            .into_iter()
            .map(|row| {
                let action = SyncAction::from_db(&row.action).ok_or_else(|| {
                    AppError::Internal(format!("unknown sync action {}", row.action))
                })?;
                Ok(ChangeSummary {
                    seq: row.seq,
                    client_op_id: row.client_op_id,
                    action,
                    path: row.path,
                    title: row.title,
                    content: row.content,
                    content_hash: row.content_hash,
                    base_version: row.base_version,
                    applied_version: row.applied_version,
                    created_at: row.created_at,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;

        let conflicts = sqlx::query_as::<_, ConflictRow>(
            r#"
            SELECT
              id,
              path,
              server_version,
              incoming_base_version,
              status::text AS status,
              created_at,
              resolved_at
            FROM conflicts
            WHERE user_id = $1
              AND status = 'pending'
            ORDER BY created_at ASC
            LIMIT 200
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            let status = ConflictStatus::from_db(&row.status).ok_or_else(|| {
                AppError::Internal(format!("unknown conflict status {}", row.status))
            })?;
            Ok(ConflictInfo {
                id: row.id,
                path: row.path,
                server_version: row.server_version,
                incoming_base_version: row.incoming_base_version,
                status,
                created_at: row.created_at,
                resolved_at: row.resolved_at,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;

        let next_seq = changes.last().map(|item| item.seq).unwrap_or(since_seq);

        let _ = sqlx::query(
            r#"
            UPDATE devices
            SET last_seen_at = now()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(device.id)
        .execute(&self.pool)
        .await?;

        Ok(PullBatch {
            changes,
            conflicts,
            next_seq,
        })
    }

    async fn push_changes(
        &self,
        user_id: i64,
        device_id: Uuid,
        changes: Vec<PushChange>,
    ) -> AppResult<PushBatchResult> {
        let mut tx = self.pool.begin().await?;
        let device = Self::get_device_tx(&mut tx, user_id, device_id).await?;

        let mut accepted_paths = Vec::new();
        let mut conflicts = Vec::new();
        let mut max_seq = Self::high_watermark_tx(&mut tx, user_id).await?;

        for change in changes {
            let inserted_seq = sqlx::query_scalar::<_, i64>(
                r#"
                INSERT INTO document_ops (
                  user_id,
                  device_id,
                  client_op_id,
                  action,
                  path,
                  title,
                  content,
                  content_hash,
                  base_version,
                  applied_version
                )
                VALUES ($1, $2, $3, $4::sync_action, $5, $6, $7, $8, $9, NULL)
                ON CONFLICT (user_id, device_id, client_op_id)
                DO NOTHING
                RETURNING seq
                "#,
            )
            .bind(user_id)
            .bind(device.id)
            .bind(change.client_op_id)
            .bind(change.action.as_db_str())
            .bind(&change.path)
            .bind(&change.title)
            .bind(&change.content)
            .bind(&change.content_hash)
            .bind(change.base_version)
            .fetch_optional(tx.as_mut())
            .await?;

            let Some(op_seq) = inserted_seq else {
                accepted_paths.push(change.path);
                continue;
            };
            max_seq = max_seq.max(op_seq);

            let head = sqlx::query_as::<_, HeadVersionRow>(
                r#"
                SELECT version
                FROM document_heads
                WHERE user_id = $1 AND path = $2
                FOR UPDATE
                "#,
            )
            .bind(user_id)
            .bind(&change.path)
            .fetch_optional(tx.as_mut())
            .await?;

            let server_version = head.map(|item| item.version).unwrap_or(0);

            if let Some(base_version) = change.base_version
                && base_version != server_version
            {
                let conflict_row = sqlx::query_as::<_, ConflictRow>(
                    r#"
                        INSERT INTO conflicts (
                          user_id,
                          path,
                          server_op_seq,
                          incoming_device_id,
                          incoming_client_op_id,
                          server_version,
                          incoming_base_version,
                          status
                        )
                        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')
                        RETURNING
                          id,
                          path,
                          server_version,
                          incoming_base_version,
                          status::text AS status,
                          created_at,
                          resolved_at
                        "#,
                )
                .bind(user_id)
                .bind(&change.path)
                .bind(op_seq)
                .bind(device.id)
                .bind(change.client_op_id)
                .bind(server_version)
                .bind(base_version)
                .fetch_one(tx.as_mut())
                .await?;

                conflicts.push(ConflictInfo {
                    id: conflict_row.id,
                    path: conflict_row.path,
                    server_version: conflict_row.server_version,
                    incoming_base_version: conflict_row.incoming_base_version,
                    status: ConflictStatus::from_db(&conflict_row.status).ok_or_else(|| {
                        AppError::Internal("invalid conflict status in database".to_string())
                    })?,
                    created_at: conflict_row.created_at,
                    resolved_at: conflict_row.resolved_at,
                });

                continue;
            }

            let applied_version = match change.action {
                SyncAction::Delete => {
                    sqlx::query_scalar::<_, i64>(
                        r#"
                        INSERT INTO document_heads (
                          user_id,
                          path,
                          title,
                          content,
                          content_hash,
                          version,
                          last_modified_device_id,
                          updated_at,
                          deleted_at,
                          deleted_by_device_id
                        )
                        VALUES ($1, $2, NULL, NULL, NULL, 1, $3, now(), now(), $3)
                        ON CONFLICT (user_id, path)
                        DO UPDATE SET
                          version = document_heads.version + 1,
                          last_modified_device_id = EXCLUDED.last_modified_device_id,
                          updated_at = now(),
                          deleted_at = now(),
                          deleted_by_device_id = EXCLUDED.deleted_by_device_id
                        RETURNING version
                        "#,
                    )
                    .bind(user_id)
                    .bind(&change.path)
                    .bind(device.id)
                    .fetch_one(tx.as_mut())
                    .await?
                }
                SyncAction::Create | SyncAction::Update => {
                    sqlx::query_scalar::<_, i64>(
                        r#"
                        INSERT INTO document_heads (
                          user_id,
                          path,
                          title,
                          content,
                          content_hash,
                          version,
                          last_modified_device_id,
                          updated_at,
                          deleted_at,
                          deleted_by_device_id
                        )
                        VALUES ($1, $2, $3, $4, $5, 1, $6, now(), NULL, NULL)
                        ON CONFLICT (user_id, path)
                        DO UPDATE SET
                          title = EXCLUDED.title,
                          content = EXCLUDED.content,
                          content_hash = EXCLUDED.content_hash,
                          version = document_heads.version + 1,
                          last_modified_device_id = EXCLUDED.last_modified_device_id,
                          updated_at = now(),
                          deleted_at = NULL,
                          deleted_by_device_id = NULL
                        RETURNING version
                        "#,
                    )
                    .bind(user_id)
                    .bind(&change.path)
                    .bind(&change.title)
                    .bind(&change.content)
                    .bind(&change.content_hash)
                    .bind(device.id)
                    .fetch_one(tx.as_mut())
                    .await?
                }
            };

            let _ = sqlx::query(
                r#"
                UPDATE document_ops
                SET applied_version = $1
                WHERE seq = $2
                "#,
            )
            .bind(applied_version)
            .bind(op_seq)
            .execute(tx.as_mut())
            .await?;

            let _ = sqlx::query(
                r#"
                INSERT INTO outbox (topic, key, payload)
                VALUES ($1, $2, $3)
                "#,
            )
            .bind("sync.document.changed")
            .bind(format!("{user_id}:{}", change.path))
            .bind(serde_json::json!({
                "user_id": user_id,
                "device_id": device.device_uuid,
                "path": change.path,
                "seq": op_seq,
                "action": change.action.as_db_str(),
                "applied_version": applied_version
            }))
            .execute(tx.as_mut())
            .await?;

            accepted_paths.push(change.path);
        }

        let _ = sqlx::query(
            r#"
            UPDATE devices
            SET last_seen_at = now()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(device.id)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;

        let high_watermark_seq = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT MAX(seq)
            FROM document_ops
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(max_seq);

        Ok(PushBatchResult {
            accepted_paths,
            conflicts,
            high_watermark_seq,
        })
    }

    async fn ack_checkpoint(
        &self,
        user_id: i64,
        device_id: Uuid,
        ack_seq: i64,
    ) -> AppResult<AckResult> {
        let device = self.get_device(user_id, device_id).await?;

        let last_acked_seq = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO device_checkpoints (user_id, device_id, last_acked_seq, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (user_id, device_id)
            DO UPDATE SET
              last_acked_seq = GREATEST(device_checkpoints.last_acked_seq, EXCLUDED.last_acked_seq),
              updated_at = now()
            RETURNING last_acked_seq
            "#,
        )
        .bind(user_id)
        .bind(device.id)
        .bind(ack_seq)
        .fetch_one(&self.pool)
        .await?;

        Ok(AckResult { last_acked_seq })
    }

    async fn resolve_conflict(
        &self,
        user_id: i64,
        conflict_id: i64,
        resolution: ConflictResolution,
        merged_content: Option<String>,
    ) -> AppResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE conflicts
            SET
              status = $1::conflict_status,
              resolution_content = $2,
              resolved_at = now()
            WHERE user_id = $3 AND id = $4
            "#,
        )
        .bind(resolution.as_db_str())
        .bind(merged_content)
        .bind(user_id)
        .bind(conflict_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "conflict {conflict_id} not found for user {user_id}"
            )));
        }

        Ok(())
    }

    async fn get_status(&self, user_id: i64, device_id: Uuid) -> AppResult<SyncStatusResponse> {
        let device = self.get_device(user_id, device_id).await?;

        let last_acked_seq = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT last_acked_seq
            FROM device_checkpoints
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(user_id)
        .bind(device.id)
        .fetch_optional(&self.pool)
        .await?
        .flatten()
        .unwrap_or(0);

        let pending_conflicts = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::bigint
            FROM conflicts
            WHERE user_id = $1 AND status = 'pending'
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let synced_documents = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::bigint
            FROM document_heads
            WHERE user_id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let server_high_watermark_seq = sqlx::query_scalar::<_, Option<i64>>(
            r#"
            SELECT MAX(seq)
            FROM document_ops
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0);

        Ok(SyncStatusResponse {
            device_id: device.device_uuid,
            last_acked_seq,
            pending_conflicts,
            synced_documents,
            server_high_watermark_seq,
        })
    }
}
