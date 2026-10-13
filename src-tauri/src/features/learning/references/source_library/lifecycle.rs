//! Adopting versions, changing freshness policy, deleting and reimporting sources.
use super::*;

impl LearningSourceLibraryRepository {
    pub async fn adopt(&self, req: &AdoptLearningSourceVersionRequestDto) -> Result<()> {
        for (v, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
            (&req.version_id, "source version"),
        ] {
            if label == "source" {
                owned_id(v, label)?;
            } else {
                uuid(v, label)?;
            }
        }
        let hash = hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "adopt",
            &hash,
        )
        .seen(&mut *tx)
        .await?
        {
            return Ok(());
        }
        let row=sqlx::query("SELECT revision,pending_version_id,active_version_id FROM learning_source_library WHERE program_id=? AND id=?")
            .bind(&req.program_id).bind(&req.source_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = row.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        let pending: Option<String> = row.get("pending_version_id");
        if pending.as_deref() != Some(&req.version_id) {
            return Err(invalid("Only the pending source version can be adopted."));
        }
        let current: Option<String> = row.get("active_version_id");
        let next_revision = if current.as_deref() == Some(&req.version_id) {
            revision
        } else {
            revision + 1
        };
        if next_revision != revision {
            let result=sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=NULL,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
            .bind(&req.version_id).bind(next_revision).bind(now()).bind(&req.program_id).bind(&req.source_id).bind(revision).execute(&mut *tx).await.map_err(db)?;
            if result.rows_affected() != 1 {
                return Err(invalid("Source changed; reload and retry."));
            }
        }
        source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "adopt",
            &hash,
        )
        .record(
            &mut *tx,
            &SourceOperationResult {
                version_id: Some(&req.version_id),
                revision: next_revision,
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn update_policy(&self, req: &UpdateLearningSourcePolicyRequestDto) -> Result<()> {
        for (v, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
        ] {
            if label == "source" {
                owned_id(v, label)?;
            } else {
                uuid(v, label)?;
            }
        }
        let hash = hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "policy",
            &hash,
        )
        .seen(&mut *tx)
        .await?
        {
            return Ok(());
        }
        let row=sqlx::query("SELECT revision,kind,freshness_policy FROM learning_source_library WHERE program_id=? AND id=?")
            .bind(&req.program_id).bind(&req.source_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = row.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if row.get::<String, _>("kind") != "web"
            && req.freshness_policy != LearningSourcePolicy::Fixed
        {
            return Err(invalid("Document and pasted sources use the fixed policy"));
        }
        let old: String = row.get("freshness_policy");
        let next = if old == policy(&req.freshness_policy) {
            revision
        } else {
            revision + 1
        };
        if next != revision {
            let result=sqlx::query("UPDATE learning_source_library SET freshness_policy=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
            .bind(policy(&req.freshness_policy)).bind(next).bind(now()).bind(&req.program_id).bind(&req.source_id).bind(revision).execute(&mut *tx).await.map_err(db)?;
            if result.rows_affected() != 1 {
                return Err(invalid("Source changed; reload and retry."));
            }
        }
        source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "policy",
            &hash,
        )
        .record(
            &mut *tx,
            &SourceOperationResult {
                version_id: None,
                revision: next,
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn delete_source(&self, req: &DeleteLearningSourceRequestDto) -> Result<()> {
        for (value, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
        ] {
            if label == "source" {
                owned_id(value, label)?;
            } else {
                uuid(value, label)?;
            }
        }
        let reason = req.reason.trim();
        if reason.is_empty() || reason.chars().count() > 500 {
            return Err(invalid("Deletion reason must contain 1–500 characters."));
        }
        let hash = hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if portability_operation(&req.operation_id, &req.program_id, "delete_source", &hash)
            .seen(&mut *tx)
            .await?
        {
            return Ok(());
        }
        require_active_program(&mut tx, &req.program_id).await?;
        let row = sqlx::query(
            "SELECT revision,deleted_at FROM learning_source_library WHERE program_id=? AND id=?",
        )
        .bind(&req.program_id)
        .bind(&req.source_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?
        .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = row.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if row.get::<Option<i64>, _>("deleted_at").is_some() {
            return Err(invalid("This source is already removed."));
        }
        let timestamp = now();
        let updated = sqlx::query("UPDATE learning_source_library SET deleted_at=?,deletion_reason=?,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND deleted_at IS NULL")
            .bind(timestamp)
            .bind(reason)
            .bind(timestamp)
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(req.expected_revision)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Source changed; reload and retry."));
        }
        portability_operation(&req.operation_id, &req.program_id, "delete_source", &hash)
            .record(&mut *tx, &Some(&req.source_id))
            .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn reimport_source(&self, req: &ReimportLearningSourceRequestDto) -> Result<()> {
        for (value, label) in [
            (&req.operation_id, "operation"),
            (&req.program_id, "program"),
            (&req.source_id, "source"),
            (&req.version_id, "source version"),
        ] {
            if label == "source" {
                owned_id(value, label)?;
            } else {
                uuid(value, label)?;
            }
        }
        if req
            .replacement_text
            .as_ref()
            .is_some_and(|text| text.trim().is_empty())
        {
            return Err(invalid("Replacement source text must not be empty."));
        }
        let hash = hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if portability_operation(&req.operation_id, &req.program_id, "reimport_source", &hash)
            .seen(&mut *tx)
            .await?
        {
            return Ok(());
        }
        require_active_program(&mut tx, &req.program_id).await?;
        let source =
            sqlx::query("SELECT * FROM learning_source_library WHERE program_id=? AND id=?")
                .bind(&req.program_id)
                .bind(&req.source_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = source.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if source.get::<Option<i64>, _>("deleted_at").is_none() {
            return Err(invalid("Only a removed source can be re-imported."));
        }
        let requested_url: Option<String> = source.get("requested_url");
        let current_id: Option<String> = source.get("active_version_id");
        let prior = if let Some(version_id) = current_id.as_deref() {
            sqlx::query("SELECT * FROM learning_source_versions WHERE program_id=? AND source_id=? AND id=?")
                .bind(&req.program_id)
                .bind(&req.source_id)
                .bind(version_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
        } else {
            None
        };
        let prior = prior.ok_or_else(|| {
            AppError::InvalidState("Removed source has no version to re-import.".into())
        })?;
        let prior_text: String = prior.get("full_text");
        let text_input = req.replacement_text.as_deref().unwrap_or(&prior_text);
        let text = normalized(text_input);
        if text.trim().is_empty() {
            return Err(invalid("Re-imported source text must not be empty."));
        }
        let digest_value = hash_text(&text);
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM learning_source_versions WHERE program_id=? AND source_id=? AND content_sha256=?",
        )
        .bind(&req.program_id)
        .bind(&req.source_id)
        .bind(&digest_value)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let active_version = if let Some(existing_id) = existing {
            existing_id
        } else {
            let collision: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&req.program_id)
            .bind(&req.version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            if collision.is_some() {
                return Err(invalid(
                    "The requested source version ID is already in use.",
                ));
            }
            let next: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(version_number),0)+1 FROM learning_source_versions WHERE program_id=? AND source_id=?",
            )
            .bind(&req.program_id)
            .bind(&req.source_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            let title: String = prior.get("title");
            let publisher: Option<String> = prior.get("publisher");
            let resolved_url: Option<String> = prior.get("resolved_url");
            let extraction: String = prior.get("extraction_version");
            let truncated = prior.get::<i64, _>("truncated") != 0;
            let timestamp = now();
            insert_version(
                &mut tx,
                SourceVersionInsert {
                    program_id: &req.program_id,
                    source_id: &req.source_id,
                    version_id: &req.version_id,
                    version_number: next,
                    title: &title,
                    publisher: publisher.as_deref(),
                    requested_url: requested_url.as_deref(),
                    resolved_url: resolved_url.as_deref(),
                    full_text: &text,
                    truncated,
                    extraction_version: &extraction,
                    acquired_at: timestamp,
                    content_sha256: &digest_value,
                },
            )
            .await?;
            mirror_legacy(
                &mut tx,
                &req.program_id,
                &req.version_id,
                &title,
                resolved_url.as_deref().or(requested_url.as_deref()),
                &text,
                timestamp,
            )
            .await?;
            req.version_id.clone()
        };
        let timestamp = now();
        let updated = sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=NULL,deleted_at=NULL,deletion_reason=NULL,revision=revision+1,updated_at=? WHERE program_id=? AND id=? AND revision=? AND deleted_at IS NOT NULL")
            .bind(&active_version)
            .bind(timestamp)
            .bind(&req.program_id)
            .bind(&req.source_id)
            .bind(req.expected_revision)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if updated.rows_affected() != 1 {
            return Err(invalid("Source changed; reload and retry."));
        }
        portability_operation(&req.operation_id, &req.program_id, "reimport_source", &hash)
            .record(&mut *tx, &Some(&active_version))
            .await?;
        tx.commit().await.map_err(db)
    }
}
