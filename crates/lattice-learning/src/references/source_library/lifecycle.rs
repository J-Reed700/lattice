//! Adopting versions, changing freshness policy, deleting and reimporting sources.
use super::repository::{self, lifecycle as stored, SourceVersionInsert};
use super::*;
use crate::references::trust;
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::persistence::Transactions;

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
        let mut tx = self.database.begin_write().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "adopt",
            &hash,
        )
        .seen(&mut tx)
        .await?
        {
            return Ok(());
        }
        let row = stored::adoption_state(&mut tx, &req.program_id, &req.source_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision = row.revision;
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if row.pending_version_id.as_deref() != Some(&req.version_id) {
            return Err(invalid("Only the pending source version can be adopted."));
        }
        let next_revision = if row.active_version_id.as_deref() == Some(&req.version_id) {
            revision
        } else {
            revision + 1
        };
        if next_revision != revision
            && !stored::adopt_version(
                &mut tx,
                &req.program_id,
                &req.source_id,
                &req.version_id,
                (next_revision, revision),
                now(),
            )
            .await?
        {
            return Err(invalid("Source changed; reload and retry."));
        }
        source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "adopt",
            &hash,
        )
        .record(
            &mut tx,
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
        let mut tx = self.database.begin_write().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "policy",
            &hash,
        )
        .seen(&mut tx)
        .await?
        {
            return Ok(());
        }
        let row = stored::policy_state(&mut tx, &req.program_id, &req.source_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision = row.revision;
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if row.kind != "web" && req.freshness_policy != LearningSourcePolicy::Fixed {
            return Err(invalid("Document and pasted sources use the fixed policy"));
        }
        let next = if row.freshness_policy == policy(&req.freshness_policy) {
            revision
        } else {
            revision + 1
        };
        if next != revision
            && !stored::set_policy(
                &mut tx,
                &req.program_id,
                &req.source_id,
                policy(&req.freshness_policy),
                (next, revision),
                now(),
            )
            .await?
        {
            return Err(invalid("Source changed; reload and retry."));
        }
        source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "policy",
            &hash,
        )
        .record(
            &mut tx,
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
        let mut tx = self.database.begin_write().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if portability_operation(&req.operation_id, &req.program_id, "delete_source", &hash)
            .seen(&mut tx)
            .await?
        {
            return Ok(());
        }
        require_active_program(&mut tx, &req.program_id).await?;
        let row = stored::deletion_state(&mut tx, &req.program_id, &req.source_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        if row.revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if row.deleted_at.is_some() {
            return Err(invalid("This source is already removed."));
        }
        if !stored::mark_deleted(
            &mut tx,
            &req.program_id,
            &req.source_id,
            reason,
            req.expected_revision,
            now(),
        )
        .await?
        {
            return Err(invalid("Source changed; reload and retry."));
        }
        crate::lessons::claim_reviews::source_deleted::reopen_for_deleted_source(
            &mut tx,
            &req.program_id,
            &req.source_id,
        )
        .await?;
        portability_operation(&req.operation_id, &req.program_id, "delete_source", &hash)
            .record(&mut tx, &Some(&req.source_id))
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
        let mut read = self.database.begin_read().await.map_err(db)?;
        let saved_text = stored::active_text_in(&mut read, &req.program_id, &req.source_id).await?;
        let saved = trust::capture::saved_texts(&mut read, &req.program_id).await?;
        read.commit().await.map_err(db)?;
        let comparison = trust::capture::compare_saved(
            saved,
            trust::capture::VersionText {
                source_id: &req.source_id,
                version_id: &req.version_id,
                text: &normalized(
                    req.replacement_text
                        .as_deref()
                        .or(saved_text.as_deref())
                        .unwrap_or_default(),
                ),
            },
        )
        .await?;
        let mut tx = self.database.begin_write().await.map_err(db)?;
        lock_program(&mut tx, &req.program_id).await?;
        if portability_operation(&req.operation_id, &req.program_id, "reimport_source", &hash)
            .seen(&mut tx)
            .await?
        {
            return Ok(());
        }
        require_active_program(&mut tx, &req.program_id).await?;
        let source = stored::source_row(&mut tx, &req.program_id, &req.source_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision = source.revision;
        if revision != req.expected_revision {
            return Err(invalid("Source changed; reload and retry."));
        }
        if source.deleted_at.is_none() {
            return Err(invalid("Only a removed source can be re-imported."));
        }
        let requested_url = source.requested_url.clone();
        let prior = if let Some(version_id) = source.active_version_id.as_deref() {
            stored::stored_version(&mut tx, &req.program_id, &req.source_id, version_id).await?
        } else {
            None
        };
        let prior = prior.ok_or_else(|| {
            AppError::InvalidState("Removed source has no version to re-import.".into())
        })?;
        let text_input = req.replacement_text.as_deref().unwrap_or(&prior.full_text);
        let text = normalized(text_input);
        if text.trim().is_empty() {
            return Err(invalid("Re-imported source text must not be empty."));
        }
        let digest_value = sha256_hex(&text);
        let existing = repository::version_with_digest(
            &mut tx,
            &req.program_id,
            &req.source_id,
            &digest_value,
        )
        .await?;
        let active_version = if let Some(existing_id) = existing {
            existing_id
        } else {
            if stored::version_id_in_use(&mut tx, &req.program_id, &req.version_id).await? {
                return Err(invalid(
                    "The requested source version ID is already in use.",
                ));
            }
            let next =
                repository::next_version_number(&mut tx, &req.program_id, &req.source_id).await?;
            let title = prior.title.clone();
            let publisher = prior.publisher.clone();
            let resolved_url = prior.resolved_url.clone();
            let extraction = prior.extraction_version.clone();
            let truncated = prior.truncated;
            let timestamp = now();
            repository::insert_version(
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
            // The restored version keeps the provenance of the one it restores.
            let prior_id = prior.id.clone();
            let signals =
                trust::capture::carried_forward(&mut tx, &req.program_id, &prior_id).await?;
            let kind = parse_kind(&source.kind)?;
            let restored = trust::capture::CapturedVersion {
                program_id: &req.program_id,
                source_id: &req.source_id,
                version_id: &req.version_id,
                kind: &kind,
                title: &title,
                resolved_url: resolved_url.as_deref(),
                text: &text,
                acquired_at_ms: timestamp,
            };
            trust::capture::record(&mut tx, &restored, &signals, &comparison).await?;
            repository::mirror_legacy(
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
        if !stored::restore_source(
            &mut tx,
            &req.program_id,
            &req.source_id,
            &active_version,
            req.expected_revision,
            timestamp,
        )
        .await?
        {
            return Err(invalid("Source changed; reload and retry."));
        }
        portability_operation(&req.operation_id, &req.program_id, "reimport_source", &hash)
            .record(&mut tx, &Some(&active_version))
            .await?;
        tx.commit().await.map_err(db)
    }
}
