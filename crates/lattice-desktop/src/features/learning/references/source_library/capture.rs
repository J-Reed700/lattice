//! Capturing new sources and refreshing them into new immutable versions.
//! Every version is saved with its provenance (the trust capture hook) in
//! the same transaction.
use super::*;
use crate::features::learning::references::trust;

impl LearningSourceLibraryRepository {
    /// Captures a source research selected, with the planner's reason for
    /// selecting it (`selection_note`, claim sources design §4.4).
    pub async fn add_selected(
        &self,
        source: NewLearningSource<'_>,
        captured: CapturedLearningSource,
        payload_hash: String,
        note: Option<&str>,
    ) -> Result<()> {
        let (program_id, source_id) = (source.program_id.to_owned(), source.source_id.to_owned());
        self.add_by(source, captured, payload_hash, SourceAddedBy::Research)
            .await?;
        let Some(note) = note.map(str::trim).filter(|note| !note.is_empty()) else {
            return Ok(());
        };
        sqlx::query(
            "UPDATE learning_source_library SET selection_note=? WHERE program_id=? AND id=?",
        )
        .bind(note)
        .bind(&program_id)
        .bind(&source_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    /// Captures a new source and its first version, recording who added it
    /// (`added_by`, claim review §5.4).
    pub async fn add_by(
        &self,
        source: NewLearningSource<'_>,
        captured: CapturedLearningSource,
        payload_hash: String,
        added_by: SourceAddedBy,
    ) -> Result<()> {
        let NewLearningSource {
            operation_id,
            source_id,
            version_id,
            program_id,
            kind,
            operation_kind,
            origin,
            requested_url,
            freshness,
        } = source;
        for (v, label) in [
            (operation_id, "operation"),
            (source_id, "source"),
            (version_id, "source version"),
            (program_id, "program"),
        ] {
            uuid(v, label)?;
        }
        let full_text = normalized(&captured.text);
        if full_text.trim().is_empty() {
            return Err(invalid("Source text must not be empty"));
        }
        let title = captured.title.trim();
        if title.is_empty() || title.chars().count() > 180 {
            return Err(invalid("Source title must contain 1–180 characters"));
        }
        if origin.trim().is_empty() || origin.chars().count() > 2048 {
            return Err(invalid("Source origin must contain 1–2048 characters"));
        }
        if requested_url.is_some_and(|value| value.chars().count() > 2048)
            || captured
                .resolved_url
                .as_deref()
                .is_some_and(|value| value.chars().count() > 2048)
        {
            return Err(invalid("Source URLs must not exceed 2048 characters"));
        }
        if captured
            .publisher
            .as_deref()
            .is_some_and(|value| value.chars().count() > 180)
        {
            return Err(invalid("Publisher must not exceed 180 characters"));
        }
        if matches!(
            kind,
            LearningSourceKind::Document | LearningSourceKind::Pasted
        ) && freshness != LearningSourcePolicy::Fixed
        {
            return Err(invalid("Document and pasted sources use the fixed policy"));
        }
        let acquired = now();
        let source_kind = match kind {
            LearningSourceKind::Web => "web",
            LearningSourceKind::Document => "document",
            LearningSourceKind::Pasted => "pasted",
        };
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, program_id).await?;
        if source_operation(
            operation_id,
            program_id,
            source_id,
            operation_kind,
            &payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            return Ok(());
        }
        let collision: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_source_library WHERE program_id=? AND id IN (?,?) UNION SELECT 1 FROM learning_source_versions WHERE program_id=? AND id IN (?,?) LIMIT 1",
        )
        .bind(program_id)
        .bind(source_id)
        .bind(version_id)
        .bind(program_id)
        .bind(source_id)
        .bind(version_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if collision.is_some() {
            return Err(invalid(
                "The requested source or version ID is already in use.",
            ));
        }
        let (text, truncated) = (full_text, captured.truncated);
        let text_digest = hash_text(&text);
        sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at,added_by) VALUES(?,?,?,?,?,?,?,NULL,0,?,?,?)")
            .bind(source_id).bind(program_id).bind(source_kind).bind(origin).bind(requested_url).bind(policy(&freshness)).bind(version_id).bind(acquired).bind(acquired).bind(added_by.as_str()).execute(&mut *tx).await.map_err(db)?;
        insert_version(
            &mut tx,
            SourceVersionInsert {
                program_id,
                source_id,
                version_id,
                version_number: 1,
                title,
                publisher: captured.publisher.as_deref(),
                requested_url,
                resolved_url: captured.resolved_url.as_deref(),
                full_text: &text,
                truncated,
                extraction_version: &captured.extraction_version,
                acquired_at: acquired,
                content_sha256: &text_digest,
            },
        )
        .await?;
        let captured_version = trust::capture::CapturedVersion {
            program_id,
            source_id,
            version_id,
            kind: &kind,
            title,
            resolved_url: captured.resolved_url.as_deref().or(requested_url),
            text: &text,
            acquired_at_ms: acquired,
        };
        trust::capture::record(&mut tx, &captured_version, &captured.signals).await?;
        mirror_legacy(
            &mut tx,
            program_id,
            version_id,
            title,
            captured.resolved_url.as_deref().or(requested_url),
            &text,
            acquired,
        )
        .await?;
        source_operation(
            operation_id,
            program_id,
            source_id,
            operation_kind,
            &payload_hash,
        )
        .record(
            &mut *tx,
            &SourceOperationResult {
                version_id: Some(version_id),
                revision: 0,
            },
        )
        .await?;
        tx.commit().await.map_err(db)
    }

    pub async fn source_for_refresh(
        &self,
        program_id: &str,
        source_id: &str,
    ) -> Result<(i64, String, LearningSourcePolicy)> {
        Self::validate_program_id(program_id)?;
        owned_id(source_id, "source")?;
        self.backfill_legacy(program_id).await?;
        let r=sqlx::query("SELECT s.revision,s.kind,s.requested_url,s.freshness_policy,s.deleted_at,p.status FROM learning_source_library s JOIN learning_programs p ON p.id=s.program_id WHERE s.program_id=? AND s.id=?")
            .bind(program_id).bind(source_id).fetch_optional(&self.pool).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        if r.get::<String, _>("status") != "active"
            || r.get::<Option<i64>, _>("deleted_at").is_some()
        {
            return Err(AppError::NotFound(
                "Active learning source not found".into(),
            ));
        }
        if r.get::<String, _>("kind") != "web" {
            return Err(invalid("Only web sources can be refreshed"));
        }
        let freshness = parse_policy(r.get("freshness_policy"))?;
        if freshness == LearningSourcePolicy::Fixed {
            return Err(invalid(
                "Change this web source to manual or before-use before refreshing.",
            ));
        }
        let url: Option<String> = r.get("requested_url");
        Ok((
            r.get("revision"),
            url.ok_or_else(|| AppError::Database("Web source has no requested URL".into()))?,
            freshness,
        ))
    }

    pub async fn refresh(
        &self,
        req: &RefreshLearningSourceRequestDto,
        captured: Option<CapturedLearningSource>,
        failure: Option<String>,
    ) -> Result<()> {
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
        let payload_hash = hash(req)?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "refresh",
            &payload_hash,
        )
        .seen(&mut *tx)
        .await?
        {
            return Ok(());
        }
        let src=sqlx::query("SELECT revision,active_version_id,pending_version_id,kind,requested_url,freshness_policy FROM learning_source_library WHERE program_id=? AND id=?")
            .bind(&req.program_id).bind(&req.source_id).fetch_optional(&mut *tx).await.map_err(db)?.ok_or_else(||AppError::NotFound("Learning source not found".into()))?;
        let revision: i64 = src.get("revision");
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if src.get::<String, _>("kind") != "web" {
            return Err(invalid("Only web sources can be refreshed"));
        }
        if src.get::<String, _>("freshness_policy") == "fixed" {
            return Err(invalid(
                "Change this web source to manual or before-use before refreshing.",
            ));
        }
        let active_id: Option<String> = src.get("active_version_id");
        let active_digest: Option<String> = if let Some(ref id) = active_id {
            sqlx::query_scalar(
                "SELECT content_sha256 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(&req.program_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
        } else {
            None
        };
        let checked = now();
        let (status, pending_id, message) = if let Some(message) = failure {
            (
                LearningSourceCheckStatus::Failed,
                src.get("pending_version_id"),
                Some(message.chars().take(500).collect::<String>()),
            )
        } else {
            let captured = captured.ok_or_else(|| invalid("Refresh result is missing"))?;
            let text = normalized(&captured.text);
            if text.trim().is_empty()
                || captured.title.trim().is_empty()
                || captured.title.chars().count() > 180
                || captured
                    .publisher
                    .as_deref()
                    .is_some_and(|value| value.chars().count() > 180)
                || captured
                    .resolved_url
                    .as_deref()
                    .is_some_and(|value| value.chars().count() > 2048)
            {
                (
                    LearningSourceCheckStatus::Failed,
                    src.get("pending_version_id"),
                    Some("The page did not contain valid source metadata and text.".into()),
                )
            } else {
                let new_digest = hash_text(&text);
                if active_digest.as_deref() == Some(new_digest.as_str()) {
                    (LearningSourceCheckStatus::Unchanged, None, None)
                } else {
                    let existing:Option<String>=sqlx::query_scalar("SELECT id FROM learning_source_versions WHERE program_id=? AND source_id=? AND content_sha256=?")
                        .bind(&req.program_id).bind(&req.source_id).bind(&new_digest).fetch_optional(&mut *tx).await.map_err(db)?;
                    let version_id = if let Some(id) = existing {
                        id
                    } else {
                        let next:i64=sqlx::query_scalar("SELECT COALESCE(MAX(version_number),0)+1 FROM learning_source_versions WHERE program_id=? AND source_id=?")
                            .bind(&req.program_id).bind(&req.source_id).fetch_one(&mut *tx).await.map_err(db)?;
                        let version_id = uuid::Uuid::new_v4().to_string();
                        let requested_url: Option<String> = src.get("requested_url");
                        insert_version(
                            &mut tx,
                            SourceVersionInsert {
                                program_id: &req.program_id,
                                source_id: &req.source_id,
                                version_id: &version_id,
                                version_number: next,
                                title: captured.title.trim(),
                                publisher: captured.publisher.as_deref(),
                                requested_url: requested_url.as_deref(),
                                resolved_url: captured.resolved_url.as_deref(),
                                full_text: &text,
                                truncated: captured.truncated,
                                extraction_version: &captured.extraction_version,
                                acquired_at: checked,
                                content_sha256: &new_digest,
                            },
                        )
                        .await?;
                        let captured_version = trust::capture::CapturedVersion {
                            program_id: &req.program_id,
                            source_id: &req.source_id,
                            version_id: &version_id,
                            kind: &LearningSourceKind::Web,
                            title: captured.title.trim(),
                            resolved_url: captured.resolved_url.as_deref(),
                            text: &text,
                            acquired_at_ms: checked,
                        };
                        trust::capture::record(&mut tx, &captured_version, &captured.signals)
                            .await?;
                        mirror_legacy(
                            &mut tx,
                            &req.program_id,
                            &version_id,
                            captured.title.trim(),
                            captured.resolved_url.as_deref(),
                            &text,
                            checked,
                        )
                        .await?;
                        version_id
                    };
                    (
                        LearningSourceCheckStatus::UpdateAvailable,
                        Some(version_id),
                        None,
                    )
                }
            }
        };
        let old_pending: Option<String> = src.get("pending_version_id");
        let next_revision =
            if status != LearningSourceCheckStatus::Failed && old_pending != pending_id {
                revision + 1
            } else {
                revision
            };
        if next_revision != revision {
            let result=sqlx::query("UPDATE learning_source_library SET pending_version_id=?,revision=?,updated_at=? WHERE program_id=? AND id=? AND revision=?")
                .bind(&pending_id).bind(next_revision).bind(checked).bind(&req.program_id).bind(&req.source_id).bind(revision).execute(&mut *tx).await.map_err(db)?;
            if result.rows_affected() != 1 {
                return Err(invalid("Source changed; reload and retry."));
            }
        }
        let result_version = pending_id.as_deref();
        source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "refresh",
            &payload_hash,
        )
        .record(
            &mut *tx,
            &SourceOperationResult {
                version_id: result_version,
                revision: next_revision,
            },
        )
        .await?;
        sqlx::query("INSERT INTO learning_source_refresh_checks(operation_id,program_id,source_id,status,checked_at,active_digest,pending_version_id,message) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&req.operation_id).bind(&req.program_id).bind(&req.source_id).bind(check_name(&status)).bind(checked).bind(active_digest).bind(&pending_id).bind(&message).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)
    }
}
