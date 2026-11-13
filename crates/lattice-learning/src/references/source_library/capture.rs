//! Capturing new sources and refreshing them into new immutable versions.
//! Every version is saved with its provenance (the trust capture hook) in
//! the same transaction.
use super::repository::{
    self,
    capture::{self as stored, RefreshCheckInsert, SourceInsert},
    SourceVersionInsert,
};
use super::*;
use crate::references::trust;
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::http::url_identity::identity;
use lattice_core::shared::persistence::WriteTx;

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
        self.insert_source(
            source,
            captured,
            payload_hash,
            SourceAddedBy::Research,
            Some(Selection { note, once: false }),
        )
        .await
        .map(|_| ())
    }

    /// Captures a source research selected unless the course already holds
    /// that page, which the same transaction decides: two lessons saving one
    /// page at once leave one source, and the second is told which version
    /// is held.
    pub async fn add_selected_once(
        &self,
        source: NewLearningSource<'_>,
        captured: CapturedLearningSource,
        payload_hash: String,
        note: Option<&str>,
    ) -> Result<SelectedSaved> {
        self.insert_source(
            source,
            captured,
            payload_hash,
            SourceAddedBy::Research,
            Some(Selection { note, once: true }),
        )
        .await
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
        self.insert_source(source, captured, payload_hash, added_by, None)
            .await
            .map(|_| ())
    }

    async fn insert_source(
        &self,
        source: NewLearningSource<'_>,
        captured: CapturedLearningSource,
        payload_hash: String,
        added_by: SourceAddedBy,
        selection: Option<Selection<'_>>,
    ) -> Result<SelectedSaved> {
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
        if title.is_empty() {
            return Err(invalid("Source title must not be empty"));
        }
        let publisher = captured.publisher.as_deref();
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
        let mut read = self.database.begin_read().await?;
        let saved = trust::capture::saved_texts(&mut read, program_id).await?;
        read.commit().await?;
        let comparison = trust::capture::compare_saved(
            saved,
            trust::capture::VersionText {
                source_id,
                version_id,
                text: &full_text,
            },
        )
        .await?;
        let mut tx = self.database.begin_write().await?;
        lock_program(&mut tx, program_id).await?;
        if source_operation(
            operation_id,
            program_id,
            source_id,
            operation_kind,
            &payload_hash,
        )
        .seen(&mut tx)
        .await?
        {
            return Ok(SelectedSaved::Added);
        }
        let text_digest = sha256_hex(&full_text);
        if selection.as_ref().is_some_and(|selection| selection.once) {
            if let Some(version_id) = held_page(
                &mut tx,
                program_id,
                [requested_url, captured.resolved_url.as_deref()],
                &text_digest,
            )
            .await?
            {
                return Ok(SelectedSaved::Held { version_id });
            }
        }
        if repository::ids_in_use(&mut tx, program_id, source_id, version_id).await? {
            return Err(invalid(
                "The requested source or version ID is already in use.",
            ));
        }
        let (text, truncated) = (full_text, captured.truncated);
        stored::insert_source(
            &mut tx,
            SourceInsert {
                id: source_id,
                program_id,
                kind: source_kind,
                origin,
                requested_url,
                freshness_policy: policy(&freshness),
                active_version_id: version_id,
                created_at: acquired,
                added_by: added_by.as_str(),
            },
        )
        .await?;
        if let Some(note) = selection
            .and_then(|selection| selection.note)
            .map(str::trim)
            .filter(|note| !note.is_empty())
        {
            stored::set_selection_note(&mut tx, program_id, source_id, note).await?;
        }
        repository::insert_version(
            &mut tx,
            SourceVersionInsert {
                program_id,
                source_id,
                version_id,
                version_number: 1,
                title,
                publisher,
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
        trust::capture::record(&mut tx, &captured_version, &captured.signals, &comparison).await?;
        repository::mirror_legacy(
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
            &mut tx,
            &SourceOperationResult {
                version_id: Some(version_id),
                revision: 0,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(SelectedSaved::Added)
    }

    pub async fn source_for_refresh(
        &self,
        program_id: &str,
        source_id: &str,
    ) -> Result<(i64, String, LearningSourcePolicy)> {
        Self::validate_program_id(program_id)?;
        owned_id(source_id, "source")?;
        self.backfill_legacy(program_id).await?;
        let r = stored::refresh_offer(&self.database, program_id, source_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        if r.program_status != "active" || r.deleted_at.is_some() {
            return Err(AppError::NotFound(
                "Active learning source not found".into(),
            ));
        }
        if r.kind != "web" {
            return Err(invalid("Only web sources can be refreshed"));
        }
        let freshness = parse_policy(&r.freshness_policy)?;
        if freshness == LearningSourcePolicy::Fixed {
            return Err(invalid(
                "Change this web source to manual or before-use before refreshing.",
            ));
        }
        Ok((
            r.revision,
            r.requested_url
                .ok_or_else(|| AppError::Database("Web source has no requested URL".into()))?,
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
        let version_id = uuid::Uuid::new_v4().to_string();
        let comparison = match captured.as_ref().filter(|_| failure.is_none()) {
            Some(captured) => {
                let mut read = self.database.begin_read().await?;
                let saved = trust::capture::saved_texts(&mut read, &req.program_id).await?;
                read.commit().await?;
                Some(
                    trust::capture::compare_saved(
                        saved,
                        trust::capture::VersionText {
                            source_id: &req.source_id,
                            version_id: &version_id,
                            text: &normalized(&captured.text),
                        },
                    )
                    .await?,
                )
            }
            None => None,
        };
        let mut tx = self.database.begin_write().await?;
        lock_program(&mut tx, &req.program_id).await?;
        if source_operation(
            &req.operation_id,
            &req.program_id,
            &req.source_id,
            "refresh",
            &payload_hash,
        )
        .seen(&mut tx)
        .await?
        {
            return Ok(());
        }
        let src = stored::refresh_state(&mut tx, &req.program_id, &req.source_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Learning source not found".into()))?;
        let revision = src.revision;
        if revision != req.expected_revision {
            return Err(conflict("Source changed; reload and retry."));
        }
        if src.kind != "web" {
            return Err(invalid("Only web sources can be refreshed"));
        }
        if src.freshness_policy == "fixed" {
            return Err(invalid(
                "Change this web source to manual or before-use before refreshing.",
            ));
        }
        let active_digest: Option<String> = if let Some(ref id) = src.active_version_id {
            stored::version_digest(&mut tx, &req.program_id, id).await?
        } else {
            None
        };
        let checked = now();
        let (status, pending_id, message) = if let Some(message) = failure {
            (
                LearningSourceCheckStatus::Failed,
                src.pending_version_id.clone(),
                Some(message.chars().take(500).collect::<String>()),
            )
        } else {
            let (captured, comparison) = captured
                .zip(comparison)
                .ok_or_else(|| invalid("Refresh result is missing"))?;
            let text = normalized(&captured.text);
            if text.trim().is_empty()
                || captured.title.trim().is_empty()
                || captured
                    .resolved_url
                    .as_deref()
                    .is_some_and(|value| value.chars().count() > 2048)
            {
                (
                    LearningSourceCheckStatus::Failed,
                    src.pending_version_id.clone(),
                    Some("The page did not contain valid source metadata and text.".into()),
                )
            } else {
                let title = captured.title.trim();
                let publisher = captured.publisher.as_deref();
                let new_digest = sha256_hex(&text);
                if active_digest.as_deref() == Some(new_digest.as_str()) {
                    (LearningSourceCheckStatus::Unchanged, None, None)
                } else {
                    let existing = repository::version_with_digest(
                        &mut tx,
                        &req.program_id,
                        &req.source_id,
                        &new_digest,
                    )
                    .await?;
                    let version_id = if let Some(id) = existing {
                        id
                    } else {
                        let next = repository::next_version_number(
                            &mut tx,
                            &req.program_id,
                            &req.source_id,
                        )
                        .await?;
                        let requested_url = src.requested_url.clone();
                        repository::insert_version(
                            &mut tx,
                            SourceVersionInsert {
                                program_id: &req.program_id,
                                source_id: &req.source_id,
                                version_id: &version_id,
                                version_number: next,
                                title,
                                publisher,
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
                            title,
                            resolved_url: captured.resolved_url.as_deref(),
                            text: &text,
                            acquired_at_ms: checked,
                        };
                        trust::capture::record(
                            &mut tx,
                            &captured_version,
                            &captured.signals,
                            &comparison,
                        )
                        .await?;
                        repository::mirror_legacy(
                            &mut tx,
                            &req.program_id,
                            &version_id,
                            title,
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
        let old_pending = src.pending_version_id;
        let next_revision =
            if status != LearningSourceCheckStatus::Failed && old_pending != pending_id {
                revision + 1
            } else {
                revision
            };
        if next_revision != revision
            && !stored::set_pending_version(
                &mut tx,
                &req.program_id,
                &req.source_id,
                &pending_id,
                (next_revision, revision),
                checked,
            )
            .await?
        {
            return Err(conflict("Source changed; reload and retry."));
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
            &mut tx,
            &SourceOperationResult {
                version_id: result_version,
                revision: next_revision,
            },
        )
        .await?;
        stored::insert_refresh_check(
            &mut tx,
            RefreshCheckInsert {
                operation_id: &req.operation_id,
                program_id: &req.program_id,
                source_id: &req.source_id,
                status: check_name(&status),
                checked_at: checked,
                active_digest: &active_digest,
                pending_version_id: &pending_id,
                message: &message,
            },
        )
        .await?;
        tx.commit().await.map_err(AppError::from)
    }
}

/// How research saves a page it selected.
struct Selection<'a> {
    note: Option<&'a str>,
    once: bool,
}

/// The version of the course's active, admitted source that is this page: the
/// same address (without its fragment) as either of the capture's, or the
/// same text.
async fn held_page(
    tx: &mut WriteTx,
    program_id: &str,
    addresses: [Option<&str>; 2],
    text_digest: &str,
) -> Result<Option<String>> {
    let wanted: Vec<String> = addresses
        .into_iter()
        .flatten()
        .map(identity)
        .filter(|address| !address.is_empty())
        .collect();
    let pages = stored::active_pages(&mut *tx, program_id).await?;
    Ok(pages.into_iter().find_map(|page| {
        let same_address = [&page.resolved_url, &page.requested_url]
            .into_iter()
            .flatten()
            .any(|address| wanted.contains(&identity(address)));
        let same_text = page.content_sha256.as_deref() == Some(text_digest);
        (same_address || same_text).then_some(page.version_id)
    }))
}
