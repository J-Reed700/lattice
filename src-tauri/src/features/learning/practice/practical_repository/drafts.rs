//! Activity drafts: reading them and saving them under the operation log.
use super::{
    decode, invalid, json, practical_operation, uuid, LearningPracticalRepository,
    PracticalDraftOperationResult,
};
use crate::features::learning::lab_runtime::LearningLabFile;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::practical_dto::*;
use lattice_core::shared::error::{AppError, Result};
use sqlx::Row;
use std::collections::HashSet;

impl LearningPracticalRepository {
    pub async fn get_draft(
        &self,
        request: &GetLearningPracticalDraftRequestDto,
    ) -> Result<LearningPracticalDraftDto> {
        uuid(&request.program_id, "program")?;
        uuid(&request.activity_id, "activity")?;
        if request.activity_revision < 0 {
            return Err(invalid("Invalid practical activity revision."));
        }
        let mut tx = self.pool.begin().await.map_err(db)?;
        let activity = sqlx::query(
            "SELECT revision FROM learning_practical_activities WHERE program_id=? AND id=?",
        )
        .bind(&request.program_id)
        .bind(&request.activity_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Practical activity not found".into()))?;
        let current_revision: i64 = activity.get("revision");
        if current_revision != request.activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        if let Some(row) = sqlx::query("SELECT draft_revision,files_json,updated_at FROM learning_practical_drafts WHERE program_id=? AND activity_id=? AND activity_revision=?")
            .bind(&request.program_id).bind(&request.activity_id).bind(request.activity_revision)
            .fetch_optional(&mut *tx).await.map_err(db)?
        {
            let draft = LearningPracticalDraftDto {
                program_id: request.program_id.clone(), activity_id: request.activity_id.clone(),
                activity_revision: request.activity_revision, draft_revision: row.get("draft_revision"),
                files: decode(&row.get::<String, _>("files_json"))?, updated_at: Some(row.get("updated_at")),
            };
            tx.commit().await.map_err(db)?;
            return Ok(draft);
        }
        let rows = sqlx::query("SELECT path,content FROM learning_practical_files WHERE activity_id=? AND role='starter' AND editable=1 ORDER BY ordinal")
            .bind(&request.activity_id).fetch_all(&mut *tx).await.map_err(db)?;
        let draft = LearningPracticalDraftDto {
            program_id: request.program_id.clone(),
            activity_id: request.activity_id.clone(),
            activity_revision: request.activity_revision,
            draft_revision: 0,
            files: rows
                .into_iter()
                .map(|row| LearningLabFile {
                    path: row.get("path"),
                    content: row.get("content"),
                })
                .collect(),
            updated_at: None,
        };
        tx.commit().await.map_err(db)?;
        Ok(draft)
    }

    pub async fn save_draft(
        &self,
        request: &SaveLearningPracticalDraftRequestDto,
        payload_hash: &str,
    ) -> Result<LearningPracticalDraftDto> {
        for (value, label) in [
            (&request.operation_id, "operation"),
            (&request.program_id, "program"),
            (&request.activity_id, "activity"),
        ] {
            uuid(value, label)?;
        }
        if request.activity_revision < 0 || request.expected_draft_revision < 0 {
            return Err(invalid("Invalid practical draft revision."));
        }
        if request.files.len() > 64 {
            return Err(invalid(
                "A practical draft cannot contain more than 64 starter files.",
            ));
        }
        let mut seen = HashSet::new();
        let mut total_bytes = 0usize;
        for file in &request.files {
            if file.path.is_empty()
                || file.path.starts_with('/')
                || file.path.contains('\\')
                || file
                    .path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(invalid("Practical draft contains an invalid file path."));
            }
            if !seen.insert(file.path.as_str()) {
                return Err(invalid("Practical draft contains a duplicate file path."));
            }
            if file.content.len() > 256 * 1024 {
                return Err(invalid(
                    "A practical starter file exceeds the 256 KiB limit.",
                ));
            }
            total_bytes = total_bytes.saturating_add(file.content.len());
        }
        if total_bytes > 2 * 1024 * 1024 {
            return Err(invalid("Practical draft exceeds the 2 MiB total limit."));
        }

        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;
        let operation = practical_operation(
            "practical_draft",
            &request.operation_id,
            Some(&request.program_id),
            &request.activity_id,
            "save",
            payload_hash,
            "Operation ID was reused with different practical draft data.",
        );
        if let Some(result) = operation
            .replay::<PracticalDraftOperationResult, _>(&mut *tx)
            .await?
        {
            let replay = LearningPracticalDraftDto {
                program_id: request.program_id.clone(),
                activity_id: request.activity_id.clone(),
                activity_revision: request.activity_revision,
                draft_revision: result.draft_revision,
                files: request.files.clone(),
                updated_at: Some(result.updated_at),
            };
            tx.commit().await.map_err(db)?;
            return Ok(replay);
        }
        Self::active_program(&mut tx, &request.program_id).await?;
        let activity = sqlx::query(
            "SELECT revision FROM learning_practical_activities WHERE program_id=? AND id=?",
        )
        .bind(&request.program_id)
        .bind(&request.activity_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Practical activity not found".into()))?;
        if activity.get::<i64, _>("revision") != request.activity_revision {
            return Err(invalid("Practical activity changed; reload and retry."));
        }
        let allowed_rows = sqlx::query("SELECT path FROM learning_practical_files WHERE activity_id=? AND role='starter' AND editable=1 ORDER BY ordinal")
            .bind(&request.activity_id).fetch_all(&mut *tx).await.map_err(db)?;
        let allowed = allowed_rows
            .into_iter()
            .map(|row| row.get::<String, _>("path"))
            .collect::<HashSet<_>>();
        if request.files.len() != allowed.len()
            || request
                .files
                .iter()
                .any(|file| !allowed.contains(&file.path))
        {
            return Err(invalid(
                "Practical drafts may only change editable starter files.",
            ));
        }
        let existing = sqlx::query("SELECT draft_revision FROM learning_practical_drafts WHERE program_id=? AND activity_id=? AND activity_revision=?")
            .bind(&request.program_id).bind(&request.activity_id).bind(request.activity_revision)
            .fetch_optional(&mut *tx).await.map_err(db)?;
        let current_revision = existing
            .map(|row| row.get::<i64, _>("draft_revision"))
            .unwrap_or(0);
        if current_revision != request.expected_draft_revision {
            return Err(invalid("Practical draft changed; reload and retry."));
        }
        let result_revision = current_revision + 1;
        let timestamp = now();
        sqlx::query("INSERT INTO learning_practical_drafts(program_id,activity_id,activity_revision,draft_revision,files_json,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(program_id,activity_id,activity_revision) DO UPDATE SET draft_revision=excluded.draft_revision,files_json=excluded.files_json,updated_at=excluded.updated_at")
            .bind(&request.program_id).bind(&request.activity_id).bind(request.activity_revision)
            .bind(result_revision).bind(json(&request.files)?).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        operation
            .record(
                &mut *tx,
                &PracticalDraftOperationResult {
                    draft_revision: result_revision,
                    updated_at: timestamp,
                },
            )
            .await?;
        tx.commit().await.map_err(db)?;
        Ok(LearningPracticalDraftDto {
            program_id: request.program_id.clone(),
            activity_id: request.activity_id.clone(),
            activity_revision: request.activity_revision,
            draft_revision: result_revision,
            files: request.files.clone(),
            updated_at: Some(timestamp),
        })
    }
}
